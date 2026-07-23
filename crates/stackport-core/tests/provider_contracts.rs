use stackport_core::planner::{create_plan, create_plan_with_state, PlanAction};
use stackport_core::provider_runtime::{
    apply_provider_requests, build_provider_probe_plan, build_provider_request_plan,
    execute_provider_probe, HttpProviderTransport, ProviderContext, ProviderRequestStatus,
    ProviderTransport, ProviderTransportResponse, ResolvedProviderRequest, RuntimeValueResolver,
};
use stackport_core::providers::{provider_registry, ProviderOperation};
use stackport_core::stack_spec::{
    provider_contexts_for_target, stack_spec_to_manifest, validate_stack_spec, StackSpec,
};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

const EXAMPLES: [(&str, &str); 4] = [
    ("vercel", "vercel.stack.yaml"),
    ("supabase", "supabase.stack.yaml"),
    ("neon", "neon.stack.yaml"),
    ("railway", "railway.stack.yaml"),
];

#[test]
fn every_advertised_provider_lifecycle_has_an_api_operation() {
    for provider in provider_registry() {
        for mapping in &provider.resources {
            let operations = provider
                .api
                .operations
                .iter()
                .filter(|operation| operation.provider_resource == mapping.provider_resource)
                .map(|operation| operation.operation.clone())
                .collect::<Vec<_>>();
            let expected = [
                (mapping.lifecycle.read, ProviderOperation::Read),
                (mapping.importable, ProviderOperation::Import),
                (mapping.lifecycle.create, ProviderOperation::ApplyCreate),
                (mapping.lifecycle.update, ProviderOperation::ApplyUpdate),
                (mapping.lifecycle.delete, ProviderOperation::ApplyDelete),
            ];
            for (advertised, operation) in expected {
                if advertised {
                    assert!(
                        operations.contains(&operation),
                        "{} {} advertises {:?} without an API operation",
                        provider.name,
                        mapping.provider_resource,
                        operation
                    );
                }
            }
        }
    }
}

#[test]
fn every_provider_example_is_valid_and_compiles_without_unresolved_ids() {
    for (provider, file) in EXAMPLES {
        let spec = read_example(file);
        let validation = validate_stack_spec(&spec, Some("production")).expect("valid stack");
        assert!(validation.valid, "{file}");
        assert!(validation.warnings.is_empty(), "{file}");

        let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
        let plan = create_plan(&manifest, provider, None).expect("migration plan");
        let requests = build_provider_request_plan(
            &manifest,
            &plan,
            &provider_contexts_for_target(&spec, "production").expect("provider context"),
        );

        assert!(requests.executable, "{file}: {:#?}", requests.warnings);
        assert!(requests.warnings.is_empty(), "{file}");
        assert!(requests
            .requests
            .iter()
            .all(|request| request.unresolved_identifiers.is_empty()));
        let serialized = serde_json::to_string(&requests).expect("serialize request plan");
        assert!(!serialized.contains("provider_config"), "{file}");
        assert!(!serialized.contains("example-secret-value"), "{file}");
    }
}

#[test]
fn every_provider_example_executes_end_to_end_against_contract_transport() {
    for (provider, file) in EXAMPLES {
        let spec = read_example(file);
        let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
        let plan = create_plan(&manifest, provider, None).expect("migration plan");
        let requests = build_provider_request_plan(
            &manifest,
            &plan,
            &provider_contexts_for_target(&spec, "production").expect("provider context"),
        );
        let outcome = apply_provider_requests(
            &manifest,
            &plan,
            None,
            &requests,
            &ContractTransport,
            &ContractResolver,
        );

        assert!(
            outcome
                .execution
                .results
                .iter()
                .all(|result| result.status == ProviderRequestStatus::Applied),
            "{file}: {:#?}",
            outcome.execution.results
        );
        assert_eq!(
            outcome.state.resources.len(),
            manifest.resources.len(),
            "{file}"
        );
        assert!(outcome.state_warnings.is_empty(), "{file}");
        let serialized = serde_json::to_string(&outcome).expect("serialize apply outcome");
        assert!(!serialized.contains("example-secret-value"), "{file}");
        assert!(!serialized.contains("provider-token"), "{file}");
    }
}

#[test]
fn destroy_plans_use_prior_snapshots_and_reverse_dependency_order() {
    for (provider, file) in EXAMPLES {
        let spec = read_example(file);
        let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
        let contexts = provider_contexts_for_target(&spec, "production").expect("context");
        let create = create_plan(&manifest, provider, None).expect("create plan");
        let create_requests = build_provider_request_plan(&manifest, &create, &contexts);
        let applied = apply_provider_requests(
            &manifest,
            &create,
            None,
            &create_requests,
            &ContractTransport,
            &ContractResolver,
        );

        let mut empty_manifest = manifest.clone();
        empty_manifest.resources.clear();
        let destroy = create_plan_with_state(&empty_manifest, provider, None, Some(&applied.state))
            .expect("destroy plan");
        assert!(destroy
            .steps
            .iter()
            .all(|step| matches!(step.action, PlanAction::Delete | PlanAction::Manual)));
        let positions = destroy
            .steps
            .iter()
            .enumerate()
            .map(|(position, step)| (step.resource_id.as_str(), position))
            .collect::<HashMap<_, _>>();
        for step in &destroy.steps {
            for dependency in &step.depends_on {
                if let Some(dependency_position) = positions.get(dependency.as_str()) {
                    assert!(
                        positions[step.resource_id.as_str()] < *dependency_position,
                        "{file}"
                    );
                }
            }
        }

        let requests = build_provider_request_plan(&empty_manifest, &destroy, &contexts);
        assert!(
            requests
                .requests
                .iter()
                .all(|request| request.unresolved_identifiers.is_empty()),
            "{file}: {:#?}",
            requests.requests
        );
        assert!(requests
            .requests
            .iter()
            .all(|request| request.action == PlanAction::Delete));

        if provider == "supabase" {
            let secret = requests
                .requests
                .iter()
                .find(|request| request.provider_resource == "project-secret")
                .expect("Supabase secret delete request");
            assert_eq!(
                secret.body.as_ref().expect("delete body")[0],
                "WEBHOOK_SECRET"
            );
            assert!(secret.url.contains("/supabase-project/secrets"));
        }
        if provider == "railway" {
            let variable = requests
                .requests
                .iter()
                .find(|request| request.resource_id == "secret:api_token")
                .expect("Railway variable delete request");
            assert_eq!(
                variable.body.as_ref().expect("delete body")["variables"]["input"]["name"],
                "API_TOKEN"
            );
        }
    }
}

#[test]
fn provider_access_probes_are_read_only_and_return_sanitized_status() {
    let plan = build_provider_probe_plan(None, &HashMap::new()).expect("probe plan");
    assert_eq!(plan.requests.len(), 4);
    assert!(plan.requests.iter().all(|request| {
        request.operation == ProviderOperation::Read
            && matches!(
                request.method,
                stackport_core::providers::HttpMethod::Get
                    | stackport_core::providers::HttpMethod::Post
            )
    }));
    let serialized_plan = serde_json::to_string(&plan).expect("serialize probe plan");
    assert!(!serialized_plan.contains("provider-token"));

    let report = execute_provider_probe(&plan, &ContractTransport, &ContractResolver);
    assert_eq!(report.results.len(), 4);
    assert!(report
        .results
        .iter()
        .all(|result| result.status == ProviderRequestStatus::Applied));
    let serialized_report = serde_json::to_string(&report).expect("serialize probe report");
    assert!(!serialized_report.contains("provider-token"));
    assert!(!serialized_report.contains("observed"));
}

#[test]
fn vercel_team_scope_is_encoded_and_composes_with_upsert() {
    let mut spec = read_example("vercel.stack.yaml");
    spec.targets
        .get_mut("production")
        .expect("production target")
        .config
        .insert("team_id".to_string(), "team id/one".to_string());
    let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
    let plan = create_plan(&manifest, "vercel", None).expect("plan");
    let requests = build_provider_request_plan(
        &manifest,
        &plan,
        &provider_contexts_for_target(&spec, "production").expect("context"),
    );
    assert!(requests
        .requests
        .iter()
        .all(|request| request.url.contains("teamId=team%20id%2Fone")));
    let environment = requests
        .requests
        .iter()
        .find(|request| request.provider_resource == "environment-variable")
        .expect("environment request");
    assert!(environment.url.ends_with("&upsert=true"));
    assert!(environment.identifier_bindings.contains_key("idOrName"));
}

#[test]
fn all_provider_probes_use_the_real_http_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock providers");
    let address = listener.local_addr().expect("mock provider address");
    let server = thread::spawn(move || {
        let expected = [
            ("GET /v2/user ", r#"{"user":{"id":"vercel-user"}}"#),
            ("GET /v1/projects ", "[]"),
            ("GET /projects?limit=1 ", r#"{"projects":[]}"#),
            ("POST / ", r#"{"data":{"me":{"id":"railway-user"}}}"#),
        ];
        for (request_line, response_body) in expected {
            let (mut stream, _) = listener.accept().expect("accept provider probe");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");
            let mut bytes = [0_u8; 8192];
            let count = stream.read(&mut bytes).expect("read provider probe");
            let request = String::from_utf8_lossy(&bytes[..count]);
            assert!(request.starts_with(request_line), "{request}");
            assert!(
                request.contains("authorization: Bearer provider-token"),
                "{request}"
            );
            if request_line.starts_with("POST") {
                assert!(request.contains("StackportProviderProbe"), "{request}");
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write provider response");
        }
    });
    let contexts = EXAMPLES
        .iter()
        .map(|(provider, _)| {
            (
                (*provider).to_string(),
                ProviderContext {
                    identifiers: HashMap::new(),
                    base_url: Some(format!("http://{address}")),
                },
            )
        })
        .collect();
    let plan = build_provider_probe_plan(None, &contexts).expect("probe plan");
    let report = execute_provider_probe(
        &plan,
        &HttpProviderTransport::new(Duration::from_secs(2)).expect("HTTP transport"),
        &ContractResolver,
    );
    server.join().expect("mock provider server");

    assert!(report
        .results
        .iter()
        .all(|result| result.status == ProviderRequestStatus::Applied));
}

struct ContractTransport;

impl ProviderTransport for ContractTransport {
    fn send(&self, request: &ResolvedProviderRequest) -> Result<ProviderTransportResponse, String> {
        assert!(!request.url().contains("${request:"), "{}", request.url());
        if let Some(body) = request.body() {
            let body = body.to_string();
            assert!(!body.contains("${request:"), "{body}");
            assert!(!body.contains("provider_config"), "{body}");
        }
        if request.provider_resource() == "access-probe" {
            return empty_response();
        }

        match (request.provider(), request.provider_resource()) {
            ("vercel", "project") => response("vercel-project", "idOrName"),
            ("vercel", "environment-variable") => response("vercel-env", "envId"),
            ("vercel", "domain") => {
                assert!(request.url().contains("/vercel-project/domains"));
                response("app.example.com", "domain")
            }
            ("supabase", "project-database") => response("supabase-project", "ref"),
            ("supabase", "auth-config") => {
                assert!(request.url().contains("/supabase-project/config/auth"));
                empty_response()
            }
            ("supabase", "storage-bucket") => {
                assert_eq!(
                    request.url(),
                    "https://supabase-project.supabase.co/storage/v1/bucket"
                );
                response("avatars", "bucket_id")
            }
            ("supabase", "edge-function") => {
                assert!(request
                    .url()
                    .contains("/supabase-project/functions/deploy?slug=webhook"));
                response("webhook", "function_slug")
            }
            ("supabase", "project-secret") => {
                let body = request.body().expect("secret body");
                assert!(body.is_array());
                assert_eq!(body[0]["name"], "WEBHOOK_SECRET");
                assert_eq!(body[0]["value"], "example-secret-value");
                empty_response()
            }
            ("neon", "project-branch-database") => Ok(ProviderTransportResponse {
                status_code: 201,
                provider_id: Some("neon-project".to_string()),
                identifiers: [
                    ("project_id".to_string(), "neon-project".to_string()),
                    ("branch_id".to_string(), "default-branch".to_string()),
                ]
                .into_iter()
                .collect(),
                sensitive_values: HashMap::new(),
                observed: None,
            }),
            ("neon", "branch") => {
                assert!(request.url().contains("/projects/neon-project/branches"));
                response("analytics-branch", "branch_id")
            }
            ("neon", "role") => {
                assert!(request
                    .url()
                    .contains("/projects/neon-project/branches/analytics-branch/roles"));
                response("application", "role_name")
            }
            ("railway", "project") => response("railway-project", "projectId"),
            ("railway", "environment") => {
                assert_graphql_identifier(request, "projectId", "railway-project");
                response("railway-environment", "environmentId")
            }
            ("railway", "service") => {
                if request.graphql_operation() == Some("serviceCreate") {
                    assert_graphql_identifier(request, "projectId", "railway-project");
                    response("railway-service", "serviceId")
                } else {
                    assert_graphql_identifier(request, "serviceId", "railway-service");
                    assert_graphql_identifier(request, "environmentId", "railway-environment");
                    empty_response()
                }
            }
            ("railway", "variable") => {
                assert_graphql_identifier(request, "projectId", "railway-project");
                assert_graphql_identifier(request, "environmentId", "railway-environment");
                assert_graphql_identifier(request, "serviceId", "railway-service");
                empty_response()
            }
            ("railway", "custom-domain") => {
                assert_graphql_identifier(request, "projectId", "railway-project");
                assert_graphql_identifier(request, "environmentId", "railway-environment");
                assert_graphql_identifier(request, "serviceId", "railway-service");
                response("railway-domain", "id")
            }
            other => Err(format!("unhandled provider contract request: {other:?}")),
        }
    }
}

fn response(provider_id: &str, identifier: &str) -> Result<ProviderTransportResponse, String> {
    Ok(ProviderTransportResponse {
        status_code: 200,
        provider_id: Some(provider_id.to_string()),
        identifiers: [(identifier.to_string(), provider_id.to_string())]
            .into_iter()
            .collect(),
        sensitive_values: HashMap::new(),
        observed: None,
    })
}

fn empty_response() -> Result<ProviderTransportResponse, String> {
    Ok(ProviderTransportResponse {
        status_code: 200,
        ..ProviderTransportResponse::default()
    })
}

fn assert_graphql_identifier(request: &ResolvedProviderRequest, name: &str, expected: &str) {
    let variables = &request.body().expect("GraphQL body")["variables"];
    let actual = variables
        .get(name)
        .or_else(|| variables.get("input").and_then(|input| input.get(name)));
    assert_eq!(actual.and_then(|value| value.as_str()), Some(expected));
}

struct ContractResolver;

impl RuntimeValueResolver for ContractResolver {
    fn environment(&self, _name: &str) -> Option<String> {
        Some("provider-token".to_string())
    }

    fn secret(&self, _reference: &str) -> Result<String, String> {
        Ok("example-secret-value".to_string())
    }
}

fn read_example(file: &str) -> StackSpec {
    let path = examples_dir().join(file);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_yaml::from_str(&source)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}
