use stackport_core::analysis::analyze_portability;
use stackport_core::apply::{apply_plan, DryRunAdapter};
use stackport_core::diff::{diff_state, DiffAction};
use stackport_core::importers::{
    import_supabase, import_vercel, SupabaseProject, VercelEnvironmentVariable, VercelProject,
};
use stackport_core::manifest::{
    resource_fingerprint, validate_manifest, Application, Capability, Manifest, MigrationScope,
    Resource, ResourceKind, MANIFEST_SCHEMA_VERSION,
};
use stackport_core::planner::{create_plan, create_plan_with_state, PlanAction};
use stackport_core::provider_runtime::{
    apply_provider_requests, build_provider_import_plan, build_provider_read_plan,
    build_provider_request_plan, compare_provider_observations, execute_provider_observations,
    execute_provider_requests, reconcile_plan_with_observations, HttpProviderTransport,
    ProviderAuthHeader, ProviderAuthRequirement, ProviderContext, ProviderExecutionReport,
    ProviderRequest, ProviderRequestPlan, ProviderRequestResult, ProviderRequestStatus,
    ProviderTransport, ProviderTransportResponse, ResolvedProviderRequest, RuntimeValueResolver,
};
use stackport_core::providers::{
    provider_execution_plan, provider_registry, HttpMethod, ProviderOperation,
};
use stackport_core::rpc::{handle_rpc_request, JsonRpcRequest};
use stackport_core::stack_spec::{
    provider_contexts_for_target, stack_spec_to_manifest, validate_stack_spec, StackSpec,
};
use stackport_core::state::{StackState, StateResource};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

#[test]
fn validates_manifest_graph_and_secret_references() {
    let manifest = sample_manifest();
    let report = validate_manifest(&manifest).expect("manifest should validate");
    assert!(report.valid);
    assert!(report.warnings.is_empty());
}

#[test]
fn rejects_inline_secret_values() {
    let mut manifest = sample_manifest();
    manifest.resources[0].properties = serde_json::json!({
        "api_token": "plaintext-token-value"
    });
    let error = validate_manifest(&manifest).expect_err("inline secret should fail");
    assert!(error.to_string().contains("likely secret value"));
}

#[test]
fn imports_vercel_without_secret_values() {
    let manifest = import_vercel(VercelProject {
        name: "next-app".to_string(),
        framework: Some("nextjs".to_string()),
        domains: vec!["example.com".to_string()],
        environment: vec![VercelEnvironmentVariable {
            key: "DATABASE_URL".to_string(),
            target: Some("production".to_string()),
        }],
    });

    validate_manifest(&manifest).expect("imported manifest should validate");
    assert_eq!(manifest.application.name, "next-app");
    assert_eq!(manifest.resources.len(), 2);
    assert_eq!(
        manifest.variables["DATABASE_URL"].secret_ref.as_deref(),
        Some("vercel:DATABASE_URL")
    );
}

#[test]
fn imports_supabase_resources() {
    let manifest = import_supabase(SupabaseProject {
        project_ref: "abc123".to_string(),
        name: Some("platform".to_string()),
        database: true,
        auth: true,
        storage_buckets: vec!["avatars".to_string()],
        edge_functions: vec!["webhook".to_string()],
    });

    validate_manifest(&manifest).expect("imported manifest should validate");
    assert_eq!(manifest.resources.len(), 4);
    assert!(manifest
        .resources
        .iter()
        .any(|resource| resource.id == "auth"));
}

#[test]
fn analyzes_missing_target_capabilities() {
    let report = analyze_portability(&sample_manifest(), "vercel", None).expect("analysis");
    assert!(!report.portable);
    assert!(report
        .resources
        .iter()
        .any(|resource| resource.resource_id == "database" && !resource.portable));
}

#[test]
fn creates_partial_plan_with_scope_warnings() {
    let plan = create_plan(
        &sample_manifest(),
        "render",
        Some(MigrationScope {
            include_resources: vec!["web".to_string()],
            exclude_resources: vec![],
        }),
    )
    .expect("plan");

    assert!(plan.partial);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].action, PlanAction::Create);
    assert_eq!(plan.warnings.len(), 1);
}

#[test]
fn diffs_manifest_against_state() {
    let manifest = sample_manifest();
    let state = StackState {
        schema_version: "stackport-state/v1".to_string(),
        provider: "render".to_string(),
        providers: vec![],
        resources: vec![StateResource {
            id: "old".to_string(),
            provider: Some("render".to_string()),
            provider_id: "old-1".to_string(),
            provider_resource: Some("service".to_string()),
            fingerprint: "stale".to_string(),
            identifiers: Default::default(),
            last_applied: None,
            secrets: vec![],
        }],
    };

    let diff = diff_state(&manifest, &state).expect("diff");
    assert!(diff
        .changes
        .iter()
        .any(|change| change.action == DiffAction::Create));
    assert!(diff
        .changes
        .iter()
        .any(|change| change.action == DiffAction::Delete));
}

#[test]
fn dry_run_apply_plans_creates_and_skips_unsupported() {
    let plan = create_plan(&sample_manifest(), "vercel", None).expect("plan");
    let report = apply_plan(&plan, &DryRunAdapter, true);
    assert!(report.dry_run);
    assert_eq!(report.results.len(), plan.steps.len());
}

#[test]
fn plans_create_update_delete_noop_and_replace_from_state() {
    let manifest = sample_manifest();
    let state = StackState {
        schema_version: "stackport-state/v1".to_string(),
        provider: "mixed".to_string(),
        providers: vec![],
        resources: vec![
            StateResource {
                id: "database".to_string(),
                provider: Some("supabase".to_string()),
                provider_id: "project-ref".to_string(),
                provider_resource: Some("project-database".to_string()),
                fingerprint: resource_fingerprint(&manifest.resources[0]),
                identifiers: [("ref".to_string(), "project-ref".to_string())]
                    .into_iter()
                    .collect(),
                last_applied: Some(serde_json::to_value(&manifest.resources[0]).unwrap()),
                secrets: vec![],
            },
            StateResource {
                id: "web".to_string(),
                provider: Some("vercel".to_string()),
                provider_id: "prj_web".to_string(),
                provider_resource: Some("project".to_string()),
                fingerprint: "stale".to_string(),
                identifiers: [("idOrName".to_string(), "prj_web".to_string())]
                    .into_iter()
                    .collect(),
                last_applied: None,
                secrets: vec![],
            },
            StateResource {
                id: "old-service".to_string(),
                provider: Some("railway".to_string()),
                provider_id: "svc_old".to_string(),
                provider_resource: Some("service".to_string()),
                fingerprint: "old".to_string(),
                identifiers: [("serviceId".to_string(), "svc_old".to_string())]
                    .into_iter()
                    .collect(),
                last_applied: None,
                secrets: vec![],
            },
        ],
    };

    let plan = create_plan_with_state(&manifest, "railway", None, Some(&state)).expect("plan");
    assert_eq!(action_for(&plan, "database"), PlanAction::Noop);
    assert_eq!(action_for(&plan, "web"), PlanAction::Update);
    assert_eq!(action_for(&plan, "old-service"), PlanAction::Delete);

    let execution = provider_execution_plan(&manifest, &plan);
    let delete = execution
        .steps
        .iter()
        .find(|step| step.resource_id == "old-service")
        .expect("delete execution step");
    assert_eq!(delete.provider, "railway");
    assert_eq!(delete.provider_resource.as_deref(), Some("service"));
    assert_eq!(
        delete
            .api_operation
            .as_ref()
            .and_then(|operation| operation.graphql_operation.as_deref()),
        Some("serviceDelete")
    );

    let mut replaced_state = state;
    let web = replaced_state
        .resources
        .iter_mut()
        .find(|resource| resource.id == "web")
        .unwrap();
    web.provider = Some("railway".to_string());
    let replacement =
        create_plan_with_state(&manifest, "railway", None, Some(&replaced_state)).expect("plan");
    assert_eq!(action_for(&replacement, "web"), PlanAction::Replace);
}

#[test]
fn handles_json_rpc_version_and_plan() {
    let version_response = handle_rpc_request(JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::json!("1"),
        method: "stackport.version".to_string(),
        params: serde_json::json!({}),
    });
    assert!(version_response.error.is_none());

    let plan_response = handle_rpc_request(JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: serde_json::json!("2"),
        method: "plan.create".to_string(),
        params: serde_json::json!({
            "manifest": sample_manifest(),
            "target_provider": "render"
        }),
    });
    assert!(plan_response.error.is_none());
    assert_eq!(
        plan_response.result.unwrap()["target_provider"],
        serde_json::json!("render")
    );
}

#[test]
fn converts_yaml_stack_spec_into_manifest_for_target_provider() {
    let spec: StackSpec = serde_yaml::from_str(include_str!("../../../fixtures/stack.app.yaml"))
        .expect("stack yaml should parse");

    let report = validate_stack_spec(&spec, Some("production")).expect("stack should validate");
    assert!(report.valid);
    assert_eq!(report.manifest_resources, 3);

    let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
    let web = manifest
        .resources
        .iter()
        .find(|resource| resource.id == "service:web")
        .expect("web service");
    assert_eq!(web.provider.as_deref(), Some("railway"));
    assert!(manifest.variables.contains_key("DATABASE_URL"));
}

#[test]
fn changing_stack_target_changes_deployment_provider() {
    let spec: StackSpec = serde_yaml::from_str(include_str!("../../../fixtures/stack.app.yaml"))
        .expect("stack yaml should parse");

    let preview = stack_spec_to_manifest(&spec, Some("preview")).expect("preview manifest");
    let production =
        stack_spec_to_manifest(&spec, Some("production")).expect("production manifest");

    let preview_web = preview
        .resources
        .iter()
        .find(|resource| resource.id == "service:web")
        .unwrap();
    let production_web = production
        .resources
        .iter()
        .find(|resource| resource.id == "service:web")
        .unwrap();

    assert_eq!(preview_web.provider.as_deref(), Some("vercel"));
    assert_eq!(production_web.provider.as_deref(), Some("railway"));
}

#[test]
fn provider_registry_captures_platform_contracts() {
    let providers = provider_registry();
    let names = providers
        .iter()
        .map(|provider| provider.name.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert!(names.contains("vercel"));
    assert!(names.contains("supabase"));
    assert!(names.contains("neon"));
    assert!(names.contains("railway"));
    assert!(providers
        .iter()
        .all(|provider| !provider.secrets.stores_plaintext_in_state));
    assert!(providers
        .iter()
        .all(|provider| provider.operations.len() >= 6));
    assert!(providers
        .iter()
        .all(|provider| !provider.api.operations.is_empty()));

    let railway = providers
        .iter()
        .find(|provider| provider.name == "railway")
        .expect("railway provider");
    assert_eq!(
        railway.api.base_url,
        "https://backboard.railway.com/graphql/v2"
    );
    assert!(railway
        .api
        .operations
        .iter()
        .any(|operation| operation.graphql_operation.as_deref() == Some("serviceCreate")));

    let vercel = providers
        .iter()
        .find(|provider| provider.name == "vercel")
        .expect("vercel provider");
    assert!(vercel
        .api
        .operations
        .iter()
        .any(|operation| operation.path == "/v9/projects/{idOrName}/domains"));

    for provider in &providers {
        for mapping in &provider.resources {
            let has = |operation| {
                provider.api.operations.iter().any(|candidate| {
                    candidate.provider_resource == mapping.provider_resource
                        && candidate.operation == operation
                })
            };
            assert!(
                !mapping.lifecycle.read || has(ProviderOperation::Read),
                "{}:{} advertises read without an operation",
                provider.name,
                mapping.provider_resource
            );
            assert!(
                !mapping.importable || has(ProviderOperation::Import),
                "{}:{} advertises import without an operation",
                provider.name,
                mapping.provider_resource
            );
            assert!(
                !mapping.lifecycle.create || has(ProviderOperation::ApplyCreate),
                "{}:{} advertises create without an operation",
                provider.name,
                mapping.provider_resource
            );
            assert!(
                !mapping.lifecycle.update || has(ProviderOperation::ApplyUpdate),
                "{}:{} advertises update without an operation",
                provider.name,
                mapping.provider_resource
            );
            assert!(
                !mapping.lifecycle.delete || has(ProviderOperation::ApplyDelete),
                "{}:{} advertises delete without an operation",
                provider.name,
                mapping.provider_resource
            );
        }
    }
}

#[test]
fn provider_execution_plan_maps_stack_resources_to_platform_resources() {
    let spec: StackSpec = serde_yaml::from_str(include_str!("../../../fixtures/stack.app.yaml"))
        .expect("stack yaml should parse");
    let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
    let plan = create_plan(&manifest, "railway", None).expect("plan");
    let execution_plan = provider_execution_plan(&manifest, &plan);

    assert!(execution_plan.warnings.is_empty());
    assert!(execution_plan
        .steps
        .iter()
        .any(|step| step.resource_id == "service:web"
            && step.provider == "railway"
            && step.provider_resource.as_deref() == Some("service")
            && step
                .api_operation
                .as_ref()
                .and_then(|operation| operation.graphql_operation.as_deref())
                == Some("serviceCreate")));
    assert!(execution_plan
        .steps
        .iter()
        .any(|step| step.resource_id == "database:primary"
            && step.provider == "neon"
            && step.provider_resource.as_deref() == Some("project-branch-database")
            && step
                .api_operation
                .as_ref()
                .map(|operation| operation.path.as_str())
                == Some("/projects")));
    assert!(execution_plan.steps.iter().all(|step| step
        .secret_policy
        .as_ref()
        .map(|policy| !policy.stores_plaintext_in_state)
        .unwrap_or(true)));
}

#[test]
fn builds_redacted_provider_requests_for_mixed_provider_stack() {
    let spec: StackSpec = serde_yaml::from_str(include_str!("../../../fixtures/stack.app.yaml"))
        .expect("stack yaml should parse");
    let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
    let plan = create_plan_with_state(&manifest, "railway", None, None).expect("plan");
    let contexts = provider_contexts_for_target(&spec, "production").expect("contexts");
    let request_plan = build_provider_request_plan(&manifest, &plan, &contexts);

    let railway = request_plan
        .requests
        .iter()
        .find(|request| request.resource_id == "service:web")
        .expect("railway service request");
    assert_eq!(railway.provider, "railway");
    assert_eq!(railway.graphql_operation.as_deref(), Some("serviceCreate"));
    assert!(railway.unresolved_identifiers.is_empty());
    assert_eq!(
        railway.body.as_ref().unwrap()["variables"]["input"]["projectId"],
        "project-demo"
    );

    let neon = request_plan
        .requests
        .iter()
        .find(|request| request.resource_id == "database:primary")
        .expect("neon project request");
    assert_eq!(neon.provider, "neon");
    assert_eq!(neon.url, "https://console.neon.tech/api/v2/projects");
    assert_eq!(neon.body.as_ref().unwrap()["project"]["pg_version"], 16);

    let serialized = serde_json::to_string(&request_plan).unwrap();
    assert!(!serialized.contains("Bearer "));
    assert!(!serialized.contains("token-value"));
    assert!(serialized.contains("RAILWAY_TOKEN"));
}

#[test]
fn builds_read_and_import_plans_from_provider_state() {
    let mut manifest = sample_manifest();
    manifest.resources = vec![manifest.resources.remove(1)];
    manifest.resources[0].depends_on.clear();
    let state = StackState {
        schema_version: stackport_core::STATE_SCHEMA_VERSION.to_string(),
        provider: "vercel".to_string(),
        providers: vec![],
        resources: vec![StateResource {
            id: "web".to_string(),
            provider: Some("vercel".to_string()),
            provider_id: "prj_123".to_string(),
            provider_resource: Some("project".to_string()),
            fingerprint: resource_fingerprint(&manifest.resources[0]),
            identifiers: HashMap::new(),
            last_applied: None,
            secrets: vec![],
        }],
    };

    let read = build_provider_read_plan(&manifest, Some(&state), &HashMap::new());
    assert!(read.executable, "{:?}", read.warnings);
    assert_eq!(read.requests.len(), 1);
    assert_eq!(read.requests[0].operation, ProviderOperation::Read);
    assert_eq!(
        read.requests[0].url,
        "https://api.vercel.com/v9/projects/prj_123"
    );

    let import = build_provider_import_plan(&manifest, &HashMap::new());
    assert!(import.executable, "{:?}", import.warnings);
    assert_eq!(import.requests[0].operation, ProviderOperation::Import);
    assert_eq!(import.requests[0].url, "https://api.vercel.com/v9/projects");
}

#[test]
fn rpc_exposes_provider_read_and_import_plans() {
    let mut manifest = sample_manifest();
    manifest.resources = vec![manifest.resources.remove(1)];
    manifest.resources[0].depends_on.clear();
    for method in ["providers.readPlan", "providers.importPlan"] {
        let response = handle_rpc_request(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(method),
            method: method.to_string(),
            params: serde_json::json!({
                "manifest": manifest,
                "target_provider": "vercel",
                "contexts": {}
            }),
        });
        assert!(response.error.is_none(), "{method}: {:?}", response.error);
        assert_eq!(
            response.result.unwrap()["requests"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn builds_vercel_and_supabase_create_requests() {
    let spec: StackSpec = serde_yaml::from_str(include_str!("../../../fixtures/stack.app.yaml"))
        .expect("stack yaml should parse");
    let vercel_manifest = stack_spec_to_manifest(&spec, Some("preview")).expect("manifest");
    let vercel_plan = create_plan(&vercel_manifest, "vercel", None).expect("plan");
    let vercel_requests = build_provider_request_plan(
        &vercel_manifest,
        &vercel_plan,
        &provider_contexts_for_target(&spec, "preview").unwrap(),
    );
    let project = vercel_requests
        .requests
        .iter()
        .find(|request| request.resource_id == "service:web")
        .expect("Vercel project request");
    assert_eq!(project.url, "https://api.vercel.com/v9/projects");
    assert_eq!(project.body.as_ref().unwrap()["name"], "demo-stack");

    let mut supabase_manifest = sample_manifest();
    supabase_manifest.resources.truncate(1);
    supabase_manifest.resources[0].properties = serde_json::json!({
        "region": "us-east-1",
        "plan": "free",
        "db_pass": "${secret:SUPABASE_DB_PASSWORD}",
        "provider_config": { "organization_slug": "org-demo" }
    });
    supabase_manifest.variables.insert(
        "SUPABASE_DB_PASSWORD".to_string(),
        stackport_core::manifest::Variable {
            description: None,
            secret_ref: Some("env:SUPABASE_DB_PASSWORD".to_string()),
            default: None,
        },
    );
    let supabase_plan = create_plan(&supabase_manifest, "supabase", None).expect("plan");
    let supabase_requests =
        build_provider_request_plan(&supabase_manifest, &supabase_plan, &HashMap::new());
    let project = &supabase_requests.requests[0];
    assert_eq!(project.url, "https://api.supabase.com/v1/projects");
    assert_eq!(
        project.body.as_ref().unwrap()["organization_slug"],
        "org-demo"
    );
    assert_eq!(
        project.secret_references["SUPABASE_DB_PASSWORD"],
        "env:SUPABASE_DB_PASSWORD"
    );
}

#[test]
fn resolves_credentials_and_secrets_only_inside_transport_boundary() {
    let request_plan = ProviderRequestPlan {
        requests: vec![ProviderRequest {
            id: "secret:create".to_string(),
            resource_id: "secret".to_string(),
            provider: "vercel".to_string(),
            provider_resource: "environment-variable".to_string(),
            action: PlanAction::Create,
            method: stackport_core::providers::HttpMethod::Post,
            url: "https://provider.invalid/secrets".to_string(),
            operation: stackport_core::providers::ProviderOperation::ApplyCreate,
            graphql_operation: None,
            auth: ProviderAuthRequirement {
                alternatives: vec![vec![ProviderAuthHeader {
                    name: "Authorization".to_string(),
                    value_from_env: "VERCEL_TOKEN".to_string(),
                    scheme: Some("Bearer".to_string()),
                }]],
            },
            body: Some(serde_json::json!({ "value": "${secret:API_KEY}" })),
            unresolved_identifiers: vec![],
            secret_references: [("API_KEY".to_string(), "env:API_KEY".to_string())]
                .into_iter()
                .collect(),
            identifier_bindings: HashMap::new(),
        }],
        warnings: vec![],
        executable: true,
    };
    let transport = InspectingTransport::default();
    let resolver = TestResolver {
        environment: [
            ("VERCEL_TOKEN".to_string(), "token-value".to_string()),
            ("API_KEY".to_string(), "secret-value".to_string()),
        ]
        .into_iter()
        .collect(),
    };

    let report = execute_provider_requests(&request_plan, &transport, &resolver);
    assert_eq!(report.results[0].status, ProviderRequestStatus::Applied);
    assert_eq!(
        report.results[0].provider_id.as_deref(),
        Some("provider-123")
    );
    assert_eq!(
        transport.calls.lock().unwrap().as_slice(),
        &["secret:create"]
    );

    let serialized_plan = serde_json::to_string(&request_plan).unwrap();
    let serialized_report = serde_json::to_string(&report).unwrap();
    assert!(!serialized_plan.contains("token-value"));
    assert!(!serialized_plan.contains("secret-value"));
    assert!(!serialized_report.contains("token-value"));
    assert!(!serialized_report.contains("secret-value"));
}

#[test]
fn http_transport_sends_a_real_redacted_provider_request() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept provider request");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buffer = [0_u8; 8192];
        let bytes = stream.read(&mut buffer).expect("read provider request");
        let request = String::from_utf8_lossy(&buffer[..bytes]);
        assert!(request.starts_with("POST /v9/projects HTTP/1.1"));
        assert!(request.contains("authorization: Bearer token-value"));
        assert!(request.contains("\"name\":\"demo-stack\""));
        stream
            .write_all(
                b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 16\r\nConnection: close\r\n\r\n{\"id\":\"prj_123\"}",
            )
            .expect("write provider response");
    });
    let request_plan = ProviderRequestPlan {
        requests: vec![ProviderRequest {
            id: "service:web:create".to_string(),
            resource_id: "service:web".to_string(),
            provider: "vercel".to_string(),
            provider_resource: "project".to_string(),
            action: PlanAction::Create,
            method: stackport_core::providers::HttpMethod::Post,
            url: format!("http://{address}/v9/projects"),
            operation: stackport_core::providers::ProviderOperation::ApplyCreate,
            graphql_operation: None,
            auth: ProviderAuthRequirement {
                alternatives: vec![vec![ProviderAuthHeader {
                    name: "Authorization".to_string(),
                    value_from_env: "VERCEL_TOKEN".to_string(),
                    scheme: Some("Bearer".to_string()),
                }]],
            },
            body: Some(serde_json::json!({ "name": "demo-stack" })),
            unresolved_identifiers: vec![],
            secret_references: HashMap::new(),
            identifier_bindings: HashMap::new(),
        }],
        warnings: vec![],
        executable: true,
    };
    let resolver = TestResolver {
        environment: [("VERCEL_TOKEN".to_string(), "token-value".to_string())]
            .into_iter()
            .collect(),
    };
    let transport = HttpProviderTransport::new(Duration::from_secs(2)).unwrap();
    let manifest = Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: "demo-stack".to_string(),
            description: None,
            tags: vec![],
        },
        resources: vec![Resource {
            id: "service:web".to_string(),
            kind: ResourceKind::WebService,
            provider: Some("vercel".to_string()),
            capabilities: vec![Capability::Build],
            depends_on: vec![],
            properties: serde_json::json!({ "build": { "framework": "nextjs" } }),
        }],
        variables: Default::default(),
    };
    let migration_plan = create_plan(&manifest, "vercel", None).unwrap();

    let outcome = apply_provider_requests(
        &manifest,
        &migration_plan,
        None,
        &request_plan,
        &transport,
        &resolver,
    );
    server.join().expect("mock provider thread");

    assert_eq!(
        outcome.execution.results[0].status,
        ProviderRequestStatus::Applied
    );
    assert_eq!(outcome.execution.results[0].status_code, Some(201));
    assert_eq!(
        outcome.execution.results[0].provider_id.as_deref(),
        Some("prj_123")
    );
    assert_eq!(outcome.state.resources[0].provider_id, "prj_123");
    assert_eq!(
        outcome.state.resources[0]
            .identifiers
            .get("idOrName")
            .map(String::as_str),
        Some("prj_123")
    );
    assert!(outcome.state.resources[0].last_applied.is_some());
}

#[test]
fn http_read_redacts_provider_secret_fields() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept provider request");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("read provider request");
        let body = r#"{"site_url":"https://example.test","client_secret":"must-not-leak","nested":{"connection_uri":"postgres://must-not-leak"}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write provider response");
    });
    let plan = ProviderRequestPlan {
        requests: vec![ProviderRequest {
            id: "auth:read".to_string(),
            resource_id: "auth".to_string(),
            provider: "supabase".to_string(),
            provider_resource: "auth-config".to_string(),
            action: PlanAction::Noop,
            method: HttpMethod::Get,
            url: format!("http://{address}/auth"),
            operation: ProviderOperation::Read,
            graphql_operation: None,
            auth: ProviderAuthRequirement {
                alternatives: vec![vec![ProviderAuthHeader {
                    name: "Authorization".to_string(),
                    value_from_env: "SUPABASE_ACCESS_TOKEN".to_string(),
                    scheme: Some("Bearer".to_string()),
                }]],
            },
            body: None,
            unresolved_identifiers: vec![],
            secret_references: HashMap::new(),
            identifier_bindings: HashMap::new(),
        }],
        warnings: vec![],
        executable: true,
    };
    let resolver = TestResolver {
        environment: [(
            "SUPABASE_ACCESS_TOKEN".to_string(),
            "token-value".to_string(),
        )]
        .into_iter()
        .collect(),
    };
    let report = execute_provider_observations(
        &plan,
        &HttpProviderTransport::new(Duration::from_secs(2)).unwrap(),
        &resolver,
    );
    server.join().expect("mock provider should finish");

    let observed = report.results[0].observed.as_ref().expect("observed body");
    assert_eq!(observed["site_url"], "https://example.test");
    assert_eq!(observed["client_secret"], "${redacted}");
    assert_eq!(observed["nested"]["connection_uri"], "${redacted}");
    let serialized = serde_json::to_string(&report).unwrap();
    assert!(!serialized.contains("must-not-leak"));
    assert!(!serialized.contains("token-value"));
}

#[test]
fn supabase_edge_function_uses_real_multipart_upload() {
    let source_path =
        std::env::temp_dir().join(format!("stackport-edge-function-{}.ts", std::process::id()));
    std::fs::write(&source_path, "Deno.serve(() => new Response('stackport'));")
        .expect("write Edge Function fixture");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept provider request");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_http_request(&mut stream);
        assert!(request
            .starts_with("POST /v1/projects/project-ref/functions/deploy?slug=hello HTTP/1.1"));
        assert!(request
            .to_ascii_lowercase()
            .contains("content-type: multipart/form-data; boundary="));
        assert!(request.contains("name=\"metadata\""));
        assert!(request.contains("\"entrypoint_path\":\"index.ts\""));
        assert!(request.contains("stackport-edge-function-"));
        assert!(request.contains("Deno.serve(() => new Response('stackport'));"));
        let response = r#"{"id":"fn_123","slug":"hello"}"#;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                )
                .as_bytes(),
            )
            .expect("write provider response");
    });
    let manifest = Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: "demo".to_string(),
            description: None,
            tags: vec![],
        },
        resources: vec![Resource {
            id: "function:hello".to_string(),
            kind: ResourceKind::Function,
            provider: Some("supabase".to_string()),
            capabilities: vec![Capability::EdgeFunctions],
            depends_on: vec![],
            properties: serde_json::json!({
                "slug": "hello",
                "name": "Hello",
                "entrypoint_path": "index.ts",
                "files": [source_path.to_string_lossy()]
            }),
        }],
        variables: HashMap::new(),
    };
    let migration_plan = create_plan(&manifest, "supabase", None).unwrap();
    let contexts = [(
        "supabase".to_string(),
        ProviderContext {
            identifiers: [("ref".to_string(), "project-ref".to_string())]
                .into_iter()
                .collect(),
            base_url: Some(format!("http://{address}")),
        },
    )]
    .into_iter()
    .collect();
    let requests = build_provider_request_plan(&manifest, &migration_plan, &contexts);
    assert!(requests.executable, "{:?}", requests.warnings);
    assert!(requests.requests[0].url.ends_with("?slug=hello"));
    let resolver = TestResolver {
        environment: [(
            "SUPABASE_ACCESS_TOKEN".to_string(),
            "supabase-token".to_string(),
        )]
        .into_iter()
        .collect(),
    };

    let report = execute_provider_requests(
        &requests,
        &HttpProviderTransport::new(Duration::from_secs(2)).unwrap(),
        &resolver,
    );
    server.join().expect("mock provider should finish");
    std::fs::remove_file(source_path).expect("remove Edge Function fixture");
    assert_eq!(report.results[0].status, ProviderRequestStatus::Applied);
    assert_eq!(report.results[0].provider_id.as_deref(), Some("fn_123"));
}

#[test]
fn provider_observations_continue_after_an_independent_read_fails() {
    let web = Resource {
        id: "web:a".to_string(),
        kind: ResourceKind::WebService,
        provider: Some("vercel".to_string()),
        capabilities: vec![Capability::Build],
        depends_on: vec![],
        properties: serde_json::json!({ "framework": "nextjs" }),
    };
    let mut second = web.clone();
    second.id = "web:b".to_string();
    let manifest = Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: "demo".to_string(),
            description: None,
            tags: vec![],
        },
        resources: vec![web.clone(), second.clone()],
        variables: HashMap::new(),
    };
    let state = StackState {
        schema_version: stackport_core::STATE_SCHEMA_VERSION.to_string(),
        provider: "vercel".to_string(),
        providers: vec![],
        resources: vec![
            state_resource_for(&web, "prj_fail"),
            state_resource_for(&second, "prj_ok"),
        ],
    };
    let plan = build_provider_read_plan(&manifest, Some(&state), &HashMap::new());
    let transport = FailFirstReadTransport::default();
    let resolver = TestResolver {
        environment: [("VERCEL_TOKEN".to_string(), "token-value".to_string())]
            .into_iter()
            .collect(),
    };

    let report = execute_provider_observations(&plan, &transport, &resolver);

    assert_eq!(transport.calls.lock().unwrap().len(), 2);
    assert_eq!(report.results[0].status, ProviderRequestStatus::Failed);
    assert_eq!(report.results[1].status, ProviderRequestStatus::Applied);
}

#[test]
fn remote_drift_changes_a_noop_into_an_update() {
    let resource = Resource {
        id: "web".to_string(),
        kind: ResourceKind::WebService,
        provider: Some("vercel".to_string()),
        capabilities: vec![Capability::Build],
        depends_on: vec![],
        properties: serde_json::json!({
            "build": { "framework": "nextjs" }
        }),
    };
    let manifest = Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: "demo".to_string(),
            description: None,
            tags: vec![],
        },
        resources: vec![resource.clone()],
        variables: HashMap::new(),
    };
    let state = StackState {
        schema_version: stackport_core::STATE_SCHEMA_VERSION.to_string(),
        provider: "vercel".to_string(),
        providers: vec![],
        resources: vec![state_resource_for(&resource, "prj_123")],
    };
    let mut plan = create_plan_with_state(&manifest, "vercel", None, Some(&state)).unwrap();
    assert_eq!(plan.steps[0].action, PlanAction::Noop);
    let observations = ProviderExecutionReport {
        results: vec![ProviderRequestResult {
            request_id: "web:read".to_string(),
            resource_id: "web".to_string(),
            status: ProviderRequestStatus::Applied,
            status_code: Some(200),
            provider_id: Some("prj_123".to_string()),
            identifiers: HashMap::new(),
            observed: Some(serde_json::json!({
                "id": "prj_123",
                "name": "demo",
                "framework": "vite"
            })),
            message: "provider request applied".to_string(),
        }],
    };

    let drift = compare_provider_observations(&manifest, Some(&state), &observations);
    assert_eq!(drift.resources[0].changed_fields, vec!["framework"]);
    reconcile_plan_with_observations(&manifest, &mut plan, &drift);
    assert_eq!(plan.steps[0].action, PlanAction::Update);
}

#[test]
fn auxiliary_environment_request_does_not_replace_project_state_id() {
    let manifest = Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: "demo".to_string(),
            description: None,
            tags: vec![],
        },
        resources: vec![Resource {
            id: "web".to_string(),
            kind: ResourceKind::WebService,
            provider: Some("vercel".to_string()),
            capabilities: vec![Capability::Build],
            depends_on: vec![],
            properties: serde_json::json!({
                "build": { "framework": "nextjs" },
                "environment": { "API_KEY": "${secret:API_KEY}" },
                "target": "production"
            }),
        }],
        variables: [(
            "API_KEY".to_string(),
            stackport_core::manifest::Variable {
                description: None,
                secret_ref: Some("env:API_KEY".to_string()),
                default: None,
            },
        )]
        .into_iter()
        .collect(),
    };
    let plan = create_plan(&manifest, "vercel", None).unwrap();
    let requests = build_provider_request_plan(&manifest, &plan, &HashMap::new());
    assert_eq!(requests.requests.len(), 2);
    let resolver = TestResolver {
        environment: [
            ("VERCEL_TOKEN".to_string(), "token".to_string()),
            ("API_KEY".to_string(), "secret".to_string()),
        ]
        .into_iter()
        .collect(),
    };

    let outcome = apply_provider_requests(
        &manifest,
        &plan,
        None,
        &requests,
        &VercelEnvironmentTransport,
        &resolver,
    );

    assert_eq!(outcome.state.resources[0].provider_id, "prj_123");
    assert_eq!(outcome.state.resources[0].secrets[0].name, "API_KEY");
}

#[test]
fn pipes_neon_connection_uri_to_railway_without_persisting_it() {
    let spec: StackSpec = serde_yaml::from_str(include_str!("../../../fixtures/stack.app.yaml"))
        .expect("stack yaml should parse");
    let manifest = stack_spec_to_manifest(&spec, Some("production")).expect("manifest");
    let migration_plan = create_plan(&manifest, "railway", None).expect("plan");
    let request_plan = build_provider_request_plan(
        &manifest,
        &migration_plan,
        &provider_contexts_for_target(&spec, "production").unwrap(),
    );
    assert!(request_plan.executable);
    let transport = MixedProviderTransport::default();
    let resolver = TestResolver {
        environment: [
            ("NEON_API_KEY".to_string(), "neon-token".to_string()),
            ("RAILWAY_TOKEN".to_string(), "railway-token".to_string()),
        ]
        .into_iter()
        .collect(),
    };

    let outcome = apply_provider_requests(
        &manifest,
        &migration_plan,
        None,
        &request_plan,
        &transport,
        &resolver,
    );

    assert!(outcome
        .execution
        .results
        .iter()
        .all(|result| result.status == ProviderRequestStatus::Applied));
    assert_eq!(transport.calls.lock().unwrap().len(), 5);
    let service_state = outcome
        .state
        .resources
        .iter()
        .find(|resource| resource.id == "service:web")
        .expect("service state");
    assert_eq!(service_state.provider_id, "svc_123");
    assert_eq!(service_state.secrets[0].name, "DATABASE_URL");
    assert_eq!(
        service_state.secrets[0].provider_ref,
        "neon:primary:DATABASE_URL"
    );
    let serialized = serde_json::to_string(&outcome).unwrap();
    assert!(!serialized.contains("postgres://runtime-only"));
    assert!(!serialized.contains("neon-token"));
    assert!(!serialized.contains("railway-token"));
}

fn sample_manifest() -> Manifest {
    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: "demo".to_string(),
            description: None,
            tags: vec![],
        },
        resources: vec![
            Resource {
                id: "database".to_string(),
                kind: ResourceKind::Database,
                provider: Some("supabase".to_string()),
                capabilities: vec![Capability::Postgres],
                depends_on: vec![],
                properties: serde_json::json!({ "engine": "postgres" }),
            },
            Resource {
                id: "web".to_string(),
                kind: ResourceKind::WebService,
                provider: Some("vercel".to_string()),
                capabilities: vec![Capability::Build],
                depends_on: vec!["database".to_string()],
                properties: serde_json::json!({ "framework": "nextjs" }),
            },
        ],
        variables: Default::default(),
    }
}

fn action_for(plan: &stackport_core::planner::MigrationPlan, resource_id: &str) -> PlanAction {
    plan.steps
        .iter()
        .find(|step| step.resource_id == resource_id)
        .unwrap_or_else(|| panic!("missing plan step for {resource_id}"))
        .action
        .clone()
}

#[derive(Default)]
struct InspectingTransport {
    calls: Mutex<Vec<String>>,
}

impl ProviderTransport for InspectingTransport {
    fn send(&self, request: &ResolvedProviderRequest) -> Result<ProviderTransportResponse, String> {
        assert_eq!(request.url(), "https://provider.invalid/secrets");
        assert_eq!(
            request.headers(),
            &[(
                "Authorization".to_string(),
                "Bearer token-value".to_string()
            )]
        );
        assert_eq!(request.body().unwrap()["value"], "secret-value");
        self.calls.lock().unwrap().push("secret:create".to_string());
        Ok(ProviderTransportResponse {
            status_code: 201,
            provider_id: Some("provider-123".to_string()),
            identifiers: Default::default(),
            sensitive_values: Default::default(),
            observed: None,
        })
    }
}

struct TestResolver {
    environment: HashMap<String, String>,
}

#[derive(Default)]
struct MixedProviderTransport {
    calls: Mutex<Vec<String>>,
}

#[derive(Default)]
struct FailFirstReadTransport {
    calls: Mutex<Vec<String>>,
}

struct VercelEnvironmentTransport;

impl ProviderTransport for VercelEnvironmentTransport {
    fn send(&self, request: &ResolvedProviderRequest) -> Result<ProviderTransportResponse, String> {
        let (provider_id, identifier) = if request.provider_resource() == "project" {
            ("prj_123", "idOrName")
        } else {
            ("env_456", "envId")
        };
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
}

impl ProviderTransport for FailFirstReadTransport {
    fn send(&self, request: &ResolvedProviderRequest) -> Result<ProviderTransportResponse, String> {
        self.calls.lock().unwrap().push(request.url().to_string());
        if request.url().contains("prj_fail") {
            return Err("simulated unavailable resource".to_string());
        }
        Ok(ProviderTransportResponse {
            status_code: 200,
            provider_id: Some("prj_ok".to_string()),
            identifiers: HashMap::new(),
            sensitive_values: HashMap::new(),
            observed: Some(serde_json::json!({ "id": "prj_ok" })),
        })
    }
}

impl ProviderTransport for MixedProviderTransport {
    fn send(&self, request: &ResolvedProviderRequest) -> Result<ProviderTransportResponse, String> {
        self.calls.lock().unwrap().push(format!(
            "{}:{}",
            request.provider(),
            request.graphql_operation().unwrap_or("rest")
        ));
        match (request.provider(), request.graphql_operation()) {
            ("neon", None) => Ok(ProviderTransportResponse {
                status_code: 201,
                provider_id: Some("neon-project-123".to_string()),
                identifiers: [("project_id".to_string(), "neon-project-123".to_string())]
                    .into_iter()
                    .collect(),
                sensitive_values: [(
                    "DATABASE_URL".to_string(),
                    "postgres://runtime-only".to_string(),
                )]
                .into_iter()
                .collect(),
                observed: None,
            }),
            ("railway", Some("serviceCreate")) => Ok(ProviderTransportResponse {
                status_code: 200,
                provider_id: Some("svc_123".to_string()),
                identifiers: [("serviceId".to_string(), "svc_123".to_string())]
                    .into_iter()
                    .collect(),
                sensitive_values: HashMap::new(),
                observed: None,
            }),
            ("railway", Some("serviceInstanceUpdate")) => {
                assert_eq!(request.body().unwrap()["variables"]["serviceId"], "svc_123");
                Ok(ProviderTransportResponse {
                    status_code: 200,
                    provider_id: None,
                    identifiers: HashMap::new(),
                    sensitive_values: HashMap::new(),
                    observed: None,
                })
            }
            ("railway", Some("variableCollectionUpsert")) => {
                assert_eq!(request.body().unwrap()["variables"]["serviceId"], "svc_123");
                assert_eq!(
                    request.body().unwrap()["variables"]["variables"]["DATABASE_URL"],
                    "postgres://runtime-only"
                );
                Ok(ProviderTransportResponse {
                    status_code: 200,
                    provider_id: None,
                    identifiers: HashMap::new(),
                    sensitive_values: HashMap::new(),
                    observed: None,
                })
            }
            ("railway", Some("customDomainCreate")) => {
                assert_eq!(
                    request.body().unwrap()["variables"]["input"]["serviceId"],
                    "svc_123"
                );
                Ok(ProviderTransportResponse {
                    status_code: 200,
                    provider_id: Some("domain_123".to_string()),
                    identifiers: [("id".to_string(), "domain_123".to_string())]
                        .into_iter()
                        .collect(),
                    sensitive_values: HashMap::new(),
                    observed: None,
                })
            }
            _ => Err("unexpected provider request".to_string()),
        }
    }
}

impl RuntimeValueResolver for TestResolver {
    fn environment(&self, name: &str) -> Option<String> {
        self.environment.get(name).cloned()
    }

    fn secret(&self, reference: &str) -> Result<String, String> {
        let name = reference
            .strip_prefix("env:")
            .ok_or_else(|| "test resolver only supports env references".to_string())?;
        self.environment
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing {name}"))
    }
}

fn state_resource_for(resource: &Resource, provider_id: &str) -> StateResource {
    StateResource {
        id: resource.id.clone(),
        provider: resource.provider.clone(),
        provider_id: provider_id.to_string(),
        provider_resource: Some("project".to_string()),
        fingerprint: resource_fingerprint(resource),
        identifiers: HashMap::new(),
        last_applied: None,
        secrets: vec![],
    }
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut expected_length = None;
    loop {
        let mut buffer = [0_u8; 4096];
        let bytes = stream.read(&mut buffer).expect("read provider request");
        assert!(bytes > 0, "provider request closed before body completed");
        request.extend_from_slice(&buffer[..bytes]);
        if expected_length.is_none() {
            if let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                expected_length = Some(header_end + 4 + content_length);
            }
        }
        if expected_length
            .map(|expected| request.len() >= expected)
            .unwrap_or(false)
        {
            break;
        }
    }
    String::from_utf8(request).expect("provider request should be UTF-8")
}
