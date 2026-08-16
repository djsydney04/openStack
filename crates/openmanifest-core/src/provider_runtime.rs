use crate::manifest::{looks_sensitive_key, Manifest, Resource, ResourceKind};
use crate::planner::{MigrationPlan, PlanAction, PlanStep};
use crate::providers::{
    provider_definition, HttpMethod, ProviderApiOperationSpec, ProviderDefinition,
    ProviderOperation,
};
use crate::state::{StackState, StateResource, StateSecretRef, STATE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderContext {
    #[serde(default)]
    pub identifiers: HashMap<String, String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderRequestPlan {
    pub requests: Vec<ProviderRequest>,
    pub warnings: Vec<String>,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderRequest {
    pub id: String,
    pub resource_id: String,
    pub provider: String,
    pub provider_resource: String,
    pub action: PlanAction,
    pub method: HttpMethod,
    pub url: String,
    pub operation: ProviderOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql_operation: Option<String>,
    pub auth: ProviderAuthRequirement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_identifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub secret_references: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub identifier_bindings: HashMap<String, ProviderIdentifierBinding>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub identifiers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderIdentifierBinding {
    pub request_id: String,
    pub response_identifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderAuthRequirement {
    pub alternatives: Vec<Vec<ProviderAuthHeader>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderAuthHeader {
    pub name: String,
    pub value_from_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderExecutionReport {
    pub results: Vec<ProviderRequestResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProbeReport {
    pub results: Vec<ProviderProbeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProbeResult {
    pub provider: String,
    pub status: ProviderRequestStatus,
    pub status_code: Option<u16>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderApplyOutcome {
    pub execution: ProviderExecutionReport,
    pub state: StackState,
    pub state_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDriftReport {
    pub resources: Vec<ProviderResourceDrift>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderResourceDrift {
    pub resource_id: String,
    pub status: ProviderDriftStatus,
    pub compared_fields: Vec<String>,
    pub changed_fields: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderDriftStatus {
    InSync,
    Drifted,
    Unknown,
    ReadFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderRequestResult {
    pub request_id: String,
    pub resource_id: String,
    pub status: ProviderRequestStatus,
    pub status_code: Option<u16>,
    pub provider_id: Option<String>,
    pub identifiers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<Value>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRequestStatus {
    Applied,
    Failed,
    Blocked,
}

pub struct ResolvedProviderRequest {
    provider: String,
    provider_resource: String,
    operation: ProviderOperation,
    graphql_operation: Option<String>,
    method: HttpMethod,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Value>,
    identifiers: HashMap<String, String>,
}

impl ResolvedProviderRequest {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn provider_resource(&self) -> &str {
        &self.provider_resource
    }

    pub fn operation(&self) -> &ProviderOperation {
        &self.operation
    }

    pub fn graphql_operation(&self) -> Option<&str> {
        self.graphql_operation.as_deref()
    }

    pub fn method(&self) -> &HttpMethod {
        &self.method
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }

    pub fn identifiers(&self) -> &HashMap<String, String> {
        &self.identifiers
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ProviderTransportResponse {
    pub status_code: u16,
    pub provider_id: Option<String>,
    pub identifiers: HashMap<String, String>,
    pub sensitive_values: HashMap<String, String>,
    pub observed: Option<Value>,
}

pub trait ProviderTransport {
    fn send(&self, request: &ResolvedProviderRequest) -> Result<ProviderTransportResponse, String>;
}

pub struct HttpProviderTransport {
    client: reqwest::blocking::Client,
}

impl HttpProviderTransport {
    pub fn new(timeout: Duration) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("openmanifest/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| format!("failed to create provider HTTP client: {err}"))?;
        Ok(Self { client })
    }
}

impl Default for HttpProviderTransport {
    fn default() -> Self {
        Self::new(Duration::from_secs(30)).expect("default HTTP client should initialize")
    }
}

impl ProviderTransport for HttpProviderTransport {
    fn send(&self, request: &ResolvedProviderRequest) -> Result<ProviderTransportResponse, String> {
        let method = match request.method() {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Delete => reqwest::Method::DELETE,
        };
        let mut builder = self
            .client
            .request(method, request.url())
            .header(reqwest::header::ACCEPT, "application/json");
        for (name, value) in request.headers() {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body() {
            if request.provider() == "supabase"
                && request.provider_resource() == "edge-function"
                && matches!(
                    request.operation(),
                    ProviderOperation::ApplyCreate | ProviderOperation::ApplyUpdate
                )
            {
                builder = builder.multipart(supabase_edge_function_form(body)?);
            } else {
                builder = builder.json(body);
            }
        }

        let response = builder
            .send()
            .map_err(|err| format!("provider request failed before receiving a response: {err}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "provider request returned HTTP {}",
                status.as_u16()
            ));
        }
        let body = response.json::<Value>().unwrap_or(Value::Null);
        if body
            .get("errors")
            .and_then(Value::as_array)
            .map(|errors| !errors.is_empty())
            .unwrap_or(false)
        {
            return Err("provider GraphQL request returned one or more errors".to_string());
        }
        let (provider_id, identifiers, sensitive_values) = extract_response_values(
            request.provider(),
            request.provider_resource(),
            request.graphql_operation(),
            &body,
        );
        let observed = if matches!(
            request.operation(),
            ProviderOperation::Read | ProviderOperation::Import | ProviderOperation::Plan
        ) {
            Some(redact_provider_response(&body, request.provider_resource()))
        } else {
            None
        };
        Ok(ProviderTransportResponse {
            status_code: status.as_u16(),
            provider_id,
            identifiers,
            sensitive_values,
            observed,
        })
    }
}

pub trait RuntimeValueResolver {
    fn environment(&self, name: &str) -> Option<String>;
    fn secret(&self, reference: &str) -> Result<String, String>;
}

pub struct ProcessEnvironment;

impl RuntimeValueResolver for ProcessEnvironment {
    fn environment(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn secret(&self, reference: &str) -> Result<String, String> {
        let env_name = reference.strip_prefix("env:").ok_or_else(|| {
            format!(
                "secret reference `{reference}` needs a provider secret resolver; only env:* is available"
            )
        })?;
        std::env::var(env_name).map_err(|_| format!("environment variable `{env_name}` is not set"))
    }
}

pub fn build_provider_request_plan(
    manifest: &Manifest,
    plan: &MigrationPlan,
    contexts: &HashMap<String, ProviderContext>,
) -> ProviderRequestPlan {
    let resources = manifest
        .resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<HashMap<_, _>>();
    let mut requests = Vec::new();
    let mut warnings = Vec::new();
    let plan_steps = plan
        .steps
        .iter()
        .map(|step| (step.resource_id.as_str(), step))
        .collect::<HashMap<_, _>>();

    for step in &plan.steps {
        match step.action {
            PlanAction::Noop => continue,
            PlanAction::Manual | PlanAction::Unsupported => {
                warnings.push(format!(
                    "resource `{}` cannot be executed: {}",
                    step.resource_id, step.reason
                ));
            }
            PlanAction::Replace => {
                let old_provider = step.prior_provider.as_deref();
                let old_resource = step.prior_provider_resource.as_deref();
                if let (Some(provider), Some(provider_resource)) = (old_provider, old_resource) {
                    compile_request(
                        manifest,
                        step,
                        step.prior_resource.as_ref(),
                        provider,
                        Some(provider_resource),
                        PlanAction::Delete,
                        contexts,
                        &plan_steps,
                        "replace-delete",
                        &mut requests,
                        &mut warnings,
                    );
                } else {
                    warnings.push(format!(
                        "replacement for `{}` is missing prior provider metadata",
                        step.resource_id
                    ));
                }
                let provider = step
                    .provider
                    .as_deref()
                    .unwrap_or(plan.target_provider.as_str());
                compile_request(
                    manifest,
                    step,
                    resources.get(step.resource_id.as_str()).copied(),
                    provider,
                    None,
                    PlanAction::Create,
                    contexts,
                    &plan_steps,
                    "replace-create",
                    &mut requests,
                    &mut warnings,
                );
                compile_service_follow_ups(
                    manifest,
                    step,
                    resources.get(step.resource_id.as_str()).copied(),
                    provider,
                    PlanAction::Create,
                    contexts,
                    &plan_steps,
                    &mut requests,
                    &mut warnings,
                );
            }
            _ => {
                let provider = step
                    .provider
                    .as_deref()
                    .unwrap_or(plan.target_provider.as_str());
                let request_resource = if step.action == PlanAction::Delete {
                    step.prior_resource.as_ref()
                } else {
                    resources.get(step.resource_id.as_str()).copied()
                };
                compile_request(
                    manifest,
                    step,
                    request_resource,
                    provider,
                    step.provider_resource.as_deref(),
                    step.action.clone(),
                    contexts,
                    &plan_steps,
                    action_suffix(&step.action),
                    &mut requests,
                    &mut warnings,
                );
                compile_service_follow_ups(
                    manifest,
                    step,
                    resources.get(step.resource_id.as_str()).copied(),
                    provider,
                    step.action.clone(),
                    contexts,
                    &plan_steps,
                    &mut requests,
                    &mut warnings,
                );
            }
        }
    }

    let executable = warnings.is_empty()
        && requests
            .iter()
            .all(|request| request.unresolved_identifiers.is_empty());
    ProviderRequestPlan {
        requests,
        warnings,
        executable,
    }
}

pub fn build_provider_probe_plan(
    provider: Option<&str>,
    contexts: &HashMap<String, ProviderContext>,
) -> Result<ProviderRequestPlan, String> {
    let definitions = match provider {
        Some(name) => vec![provider_definition(name)
            .ok_or_else(|| format!("provider `{name}` is not registered"))?],
        None => crate::providers::provider_registry(),
    };
    let requests = definitions
        .into_iter()
        .map(|definition| {
            let context = contexts.get(&definition.name).cloned().unwrap_or_default();
            let (method, path, graphql_operation, body) = match definition.name.as_str() {
                "vercel" => (HttpMethod::Get, "/v2/user", None, None),
                "supabase" => (HttpMethod::Get, "/v1/projects", None, None),
                "neon" => (HttpMethod::Get, "/projects?limit=1", None, None),
                "railway" => (
                    HttpMethod::Post,
                    "",
                    Some("me".to_string()),
                    Some(serde_json::json!({
                        "query": "query OpenManifestProviderProbe { me { id } }",
                        "variables": {}
                    })),
                ),
                _ => unreachable!("registered provider has no access probe"),
            };
            let base_url = context
                .base_url
                .as_deref()
                .unwrap_or(definition.api.base_url.as_str())
                .trim_end_matches('/');
            let url = if path.is_empty() {
                base_url.to_string()
            } else {
                format!("{base_url}/{path}", path = path.trim_start_matches('/'))
            };
            let auth = if definition.name == "railway" {
                ProviderAuthRequirement {
                    alternatives: bearer_auth("RAILWAY_TOKEN"),
                }
            } else {
                auth_requirement(&definition.name, "access-probe")
            };
            ProviderRequest {
                id: format!("{}:probe", definition.name),
                resource_id: definition.name.clone(),
                provider: definition.name.clone(),
                provider_resource: "access-probe".to_string(),
                action: PlanAction::Noop,
                method,
                url,
                operation: ProviderOperation::Read,
                graphql_operation,
                auth,
                body,
                unresolved_identifiers: vec![],
                secret_references: HashMap::new(),
                identifier_bindings: HashMap::new(),
                identifiers: HashMap::new(),
            }
        })
        .collect();
    Ok(ProviderRequestPlan {
        requests,
        warnings: vec![],
        executable: true,
    })
}

pub fn execute_provider_probe<T: ProviderTransport, R: RuntimeValueResolver>(
    plan: &ProviderRequestPlan,
    transport: &T,
    resolver: &R,
) -> ProviderProbeReport {
    let provider_by_request = plan
        .requests
        .iter()
        .map(|request| (request.id.as_str(), request.provider.as_str()))
        .collect::<HashMap<_, _>>();
    let report = execute_provider_observations(plan, transport, resolver);
    ProviderProbeReport {
        results: report
            .results
            .into_iter()
            .map(|result| ProviderProbeResult {
                provider: provider_by_request
                    .get(result.request_id.as_str())
                    .copied()
                    .unwrap_or("unknown")
                    .to_string(),
                status: result.status,
                status_code: result.status_code,
                message: result.message,
            })
            .collect(),
    }
}

pub fn build_provider_read_plan(
    manifest: &Manifest,
    state: Option<&StackState>,
    contexts: &HashMap<String, ProviderContext>,
) -> ProviderRequestPlan {
    build_provider_observation_plan(manifest, state, contexts, ProviderOperation::Read)
}

pub fn build_provider_import_plan(
    manifest: &Manifest,
    contexts: &HashMap<String, ProviderContext>,
) -> ProviderRequestPlan {
    build_provider_observation_plan(manifest, None, contexts, ProviderOperation::Import)
}

fn build_provider_observation_plan(
    manifest: &Manifest,
    state: Option<&StackState>,
    contexts: &HashMap<String, ProviderContext>,
    observation: ProviderOperation,
) -> ProviderRequestPlan {
    let state_resources = state
        .map(|state| {
            state
                .resources
                .iter()
                .map(|resource| (resource.id.as_str(), resource))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let empty_plan_steps = HashMap::new();
    let mut requests = Vec::new();
    let mut warnings = Vec::new();

    for resource in &manifest.resources {
        let current = state_resources.get(resource.id.as_str()).copied();
        let provider_name = resource
            .provider
            .as_deref()
            .or_else(|| current.and_then(|resource| resource.provider.as_deref()))
            .or_else(|| state.map(|state| state.provider.as_str()));
        let Some(provider_name) = provider_name else {
            warnings.push(format!(
                "resource `{}` has no provider for {observation:?}",
                resource.id
            ));
            continue;
        };
        let Some(definition) = provider_definition(provider_name) else {
            warnings.push(format!("provider `{provider_name}` is not registered"));
            continue;
        };
        let mapping = definition
            .resources
            .iter()
            .find(|mapping| mapping.stack_kind == resource.kind);
        let provider_resource = current
            .and_then(|current| current.provider_resource.clone())
            .or_else(|| mapping.map(|mapping| mapping.provider_resource.clone()));
        let Some(provider_resource) = provider_resource else {
            warnings.push(format!(
                "provider `{provider_name}` has no resource mapping for `{}`",
                resource.id
            ));
            continue;
        };
        if observation == ProviderOperation::Import
            && mapping.map(|mapping| !mapping.importable).unwrap_or(true)
        {
            warnings.push(format!(
                "provider `{provider_name}` does not support importing `{}`",
                resource.id
            ));
            continue;
        }
        let Some(operation) = definition.api.operations.iter().find(|operation| {
            operation.provider_resource == provider_resource && operation.operation == observation
        }) else {
            warnings.push(format!(
                "provider `{provider_name}` has no `{observation:?}` operation for `{provider_resource}`"
            ));
            continue;
        };
        let step = PlanStep {
            resource_id: resource.id.clone(),
            action: PlanAction::Noop,
            reason: format!("provider {observation:?} observation"),
            depends_on: resource.depends_on.clone(),
            provider: Some(provider_name.to_string()),
            prior_provider: current.and_then(|current| current.provider.clone()),
            provider_id: current.map(|current| current.provider_id.clone()),
            provider_resource: Some(provider_resource.clone()),
            prior_provider_resource: current.and_then(|current| current.provider_resource.clone()),
            identifiers: current
                .map(|current| current.identifiers.clone())
                .unwrap_or_default(),
            prior_fingerprint: current.map(|current| current.fingerprint.clone()),
            desired_fingerprint: None,
            prior_resource: current.and_then(|current| {
                current
                    .last_applied
                    .as_ref()
                    .and_then(|value| serde_json::from_value(value.clone()).ok())
            }),
        };
        let context = contexts.get(provider_name).cloned().unwrap_or_default();
        let (identifiers, identifier_bindings) = collect_identifiers(
            manifest,
            &step,
            Some(resource),
            &context,
            &operation.required_identifiers,
            &empty_plan_steps,
        );
        let unresolved_identifiers = operation
            .required_identifiers
            .iter()
            .filter(|identifier| !has_identifier(&identifiers, identifier))
            .cloned()
            .collect::<Vec<_>>();
        let body = request_body(
            provider_name,
            &provider_resource,
            operation,
            &identifiers,
            manifest,
            Some(resource),
            &PlanAction::Noop,
        );
        let secret_references = secret_references(manifest, body.as_ref());
        requests.push(ProviderRequest {
            id: format!(
                "{}:{}",
                resource.id,
                match observation {
                    ProviderOperation::Read => "read",
                    ProviderOperation::Import => "import",
                    _ => unreachable!("observation plans only support read and import"),
                }
            ),
            resource_id: resource.id.clone(),
            provider: provider_name.to_string(),
            provider_resource: provider_resource.clone(),
            action: PlanAction::Noop,
            method: operation.method.clone(),
            url: request_url(&definition, operation, &context, &identifiers),
            operation: observation.clone(),
            graphql_operation: operation.graphql_operation.clone(),
            auth: auth_requirement(provider_name, &provider_resource),
            body,
            unresolved_identifiers,
            secret_references,
            identifier_bindings,
            identifiers,
        });
    }

    let executable = warnings.is_empty()
        && requests
            .iter()
            .all(|request| request.unresolved_identifiers.is_empty());
    ProviderRequestPlan {
        requests,
        warnings,
        executable,
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_service_follow_ups(
    manifest: &Manifest,
    step: &PlanStep,
    resource: Option<&Resource>,
    provider: &str,
    action: PlanAction,
    contexts: &HashMap<String, ProviderContext>,
    plan_steps: &HashMap<&str, &PlanStep>,
    requests: &mut Vec<ProviderRequest>,
    warnings: &mut Vec<String>,
) {
    if !matches!(action, PlanAction::Create | PlanAction::Update) {
        return;
    }
    let Some(resource) = resource else {
        return;
    };
    let Some(environment) = resource
        .properties
        .get("environment")
        .and_then(Value::as_object)
    else {
        return;
    };
    if environment.is_empty() {
        return;
    }

    match provider {
        "railway" => {
            let mut variable_resource = resource.clone();
            if !variable_resource.depends_on.contains(&resource.id) {
                variable_resource.depends_on.push(resource.id.clone());
            }
            if action == PlanAction::Create {
                compile_request(
                    manifest,
                    step,
                    Some(&variable_resource),
                    provider,
                    Some("service"),
                    PlanAction::Update,
                    contexts,
                    plan_steps,
                    "settings",
                    requests,
                    warnings,
                );
            }
            compile_request(
                manifest,
                step,
                Some(&variable_resource),
                provider,
                Some("variable"),
                PlanAction::Create,
                contexts,
                plan_steps,
                "variables",
                requests,
                warnings,
            );
        }
        "vercel" => {
            let target = resource
                .properties
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("production");
            let values = environment
                .iter()
                .map(|(key, value)| {
                    let value_type = value
                        .as_str()
                        .and_then(secret_name)
                        .map(|_| "encrypted")
                        .unwrap_or("plain");
                    serde_json::json!({
                        "key": key,
                        "value": value,
                        "target": [target],
                        "type": value_type,
                    })
                })
                .collect::<Vec<_>>();
            let mut environment_resource = resource.clone();
            if !environment_resource.depends_on.contains(&resource.id) {
                environment_resource.depends_on.push(resource.id.clone());
            }
            environment_resource.properties = Value::Array(values);
            compile_request(
                manifest,
                step,
                Some(&environment_resource),
                provider,
                Some("environment-variable"),
                PlanAction::Create,
                contexts,
                plan_steps,
                "environment",
                requests,
                warnings,
            );
            if let Some(request) = requests.last_mut() {
                append_query_parameter(&mut request.url, "upsert", "true");
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_request(
    manifest: &Manifest,
    step: &PlanStep,
    resource: Option<&Resource>,
    provider_name: &str,
    provider_resource_hint: Option<&str>,
    action: PlanAction,
    contexts: &HashMap<String, ProviderContext>,
    plan_steps: &HashMap<&str, &PlanStep>,
    suffix: &str,
    requests: &mut Vec<ProviderRequest>,
    warnings: &mut Vec<String>,
) {
    let Some(definition) = provider_definition(provider_name) else {
        warnings.push(format!("provider `{provider_name}` is not registered"));
        return;
    };
    let provider_resource = provider_resource_hint
        .map(str::to_string)
        .or_else(|| resource.and_then(|resource| mapped_resource(&definition, resource)));
    let Some(provider_resource) = provider_resource else {
        warnings.push(format!(
            "provider `{provider_name}` has no resource mapping for `{}`",
            step.resource_id
        ));
        return;
    };
    let operation_kind = action_operation(&action);
    let Some(operation) = definition.api.operations.iter().find(|operation| {
        operation.provider_resource == provider_resource && operation.operation == operation_kind
    }) else {
        warnings.push(format!(
            "provider `{provider_name}` has no `{operation_kind:?}` operation for `{provider_resource}`"
        ));
        return;
    };

    let context = contexts.get(provider_name).cloned().unwrap_or_default();
    let (identifiers, identifier_bindings) = collect_identifiers(
        manifest,
        step,
        resource,
        &context,
        &operation.required_identifiers,
        plan_steps,
    );
    let mut unresolved_identifiers = operation
        .required_identifiers
        .iter()
        .filter(|identifier| !has_identifier(&identifiers, identifier))
        .cloned()
        .collect::<Vec<_>>();
    let mut url = request_url(&definition, operation, &context, &identifiers);
    if provider_name == "supabase"
        && provider_resource == "edge-function"
        && matches!(action, PlanAction::Create | PlanAction::Update)
    {
        if let Some(slug) = resource
            .and_then(|resource| resource.properties.get("slug"))
            .and_then(Value::as_str)
        {
            if !slug.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            }) {
                unresolved_identifiers.push("valid function slug".to_string());
            } else {
                append_query_parameter(&mut url, "slug", slug);
            }
        }
    }
    let body = request_body(
        provider_name,
        &provider_resource,
        operation,
        &identifiers,
        manifest,
        resource,
        &action,
    );
    if provider_name == "supabase"
        && provider_resource == "project-database"
        && action == PlanAction::Create
        && body
            .as_ref()
            .and_then(|body| body.get("db_pass"))
            .and_then(Value::as_str)
            .is_none()
    {
        unresolved_identifiers.push("db_pass".to_string());
    }
    let secret_references = secret_references(manifest, body.as_ref());
    let auth = auth_requirement(provider_name, &provider_resource);

    requests.push(ProviderRequest {
        id: format!("{}:{suffix}", step.resource_id),
        resource_id: step.resource_id.clone(),
        provider: provider_name.to_string(),
        provider_resource,
        action,
        method: operation.method.clone(),
        url,
        operation: operation_kind,
        graphql_operation: operation.graphql_operation.clone(),
        auth,
        body,
        unresolved_identifiers,
        secret_references,
        identifier_bindings,
        identifiers,
    });
}

pub fn execute_provider_requests<T: ProviderTransport, R: RuntimeValueResolver>(
    plan: &ProviderRequestPlan,
    transport: &T,
    resolver: &R,
) -> ProviderExecutionReport {
    execute_provider_requests_with_policy(plan, transport, resolver, true)
}

pub fn execute_provider_observations<T: ProviderTransport, R: RuntimeValueResolver>(
    plan: &ProviderRequestPlan,
    transport: &T,
    resolver: &R,
) -> ProviderExecutionReport {
    execute_provider_requests_with_policy(plan, transport, resolver, false)
}

pub fn compare_provider_observations(
    manifest: &Manifest,
    state: Option<&StackState>,
    report: &ProviderExecutionReport,
) -> ProviderDriftReport {
    let state_resources = state
        .map(|state| {
            state
                .resources
                .iter()
                .map(|resource| (resource.id.as_str(), resource))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let results = report
        .results
        .iter()
        .map(|result| (result.resource_id.as_str(), result))
        .collect::<HashMap<_, _>>();
    let mut resources = Vec::new();

    for resource in &manifest.resources {
        let Some(result) = results.get(resource.id.as_str()).copied() else {
            continue;
        };
        if result.status != ProviderRequestStatus::Applied {
            resources.push(ProviderResourceDrift {
                resource_id: resource.id.clone(),
                status: ProviderDriftStatus::ReadFailed,
                compared_fields: vec![],
                changed_fields: vec![],
                message: result.message.clone(),
            });
            continue;
        }
        let Some(observed) = result.observed.as_ref() else {
            resources.push(ProviderResourceDrift {
                resource_id: resource.id.clone(),
                status: ProviderDriftStatus::Unknown,
                compared_fields: vec![],
                changed_fields: vec![],
                message: "provider returned no comparable JSON configuration".to_string(),
            });
            continue;
        };
        let observed = state_resources
            .get(resource.id.as_str())
            .and_then(|state| find_object_by_id(observed, &state.provider_id))
            .unwrap_or(observed);
        let fields = desired_observation_fields(manifest, resource);
        let mut compared_fields = Vec::new();
        let mut changed_fields = Vec::new();
        for field in fields {
            let Some(actual) = field
                .aliases
                .iter()
                .find_map(|alias| find_value_by_key(observed, alias))
            else {
                continue;
            };
            compared_fields.push(field.name.clone());
            if !values_equivalent(&field.expected, actual) {
                changed_fields.push(field.name);
            }
        }
        let (status, message) = if compared_fields.is_empty() {
            (
                ProviderDriftStatus::Unknown,
                "provider response had no safely comparable fields".to_string(),
            )
        } else if changed_fields.is_empty() {
            (
                ProviderDriftStatus::InSync,
                "observed provider configuration matches desired comparable fields".to_string(),
            )
        } else {
            (
                ProviderDriftStatus::Drifted,
                format!(
                    "observed provider configuration differs in: {}",
                    changed_fields.join(", ")
                ),
            )
        };
        resources.push(ProviderResourceDrift {
            resource_id: resource.id.clone(),
            status,
            compared_fields,
            changed_fields,
            message,
        });
    }
    ProviderDriftReport { resources }
}

pub fn reconcile_plan_with_observations(
    manifest: &Manifest,
    plan: &mut MigrationPlan,
    drift: &ProviderDriftReport,
) {
    for resource_drift in &drift.resources {
        let Some(step) = plan
            .steps
            .iter_mut()
            .find(|step| step.resource_id == resource_drift.resource_id)
        else {
            continue;
        };
        match resource_drift.status {
            ProviderDriftStatus::Drifted if step.action == PlanAction::Noop => {
                let supports_update = manifest
                    .resources
                    .iter()
                    .find(|resource| resource.id == step.resource_id)
                    .and_then(|resource| {
                        let provider = resource.provider.as_deref().or(step.provider.as_deref())?;
                        provider_definition(provider).and_then(|definition| {
                            definition
                                .resources
                                .into_iter()
                                .find(|mapping| mapping.stack_kind == resource.kind)
                                .map(|mapping| mapping.lifecycle.update)
                        })
                    })
                    .unwrap_or(false);
                step.action = if supports_update {
                    PlanAction::Update
                } else {
                    PlanAction::Manual
                };
                step.reason = resource_drift.message.clone();
            }
            ProviderDriftStatus::ReadFailed | ProviderDriftStatus::Unknown => {
                plan.warnings.push(format!(
                    "resource `{}` remote state is {:?}: {}",
                    resource_drift.resource_id, resource_drift.status, resource_drift.message
                ));
            }
            ProviderDriftStatus::InSync | ProviderDriftStatus::Drifted => {}
        }
    }
}

fn execute_provider_requests_with_policy<T: ProviderTransport, R: RuntimeValueResolver>(
    plan: &ProviderRequestPlan,
    transport: &T,
    resolver: &R,
    halt_on_failure: bool,
) -> ProviderExecutionReport {
    let mut results = Vec::new();
    let mut completed = HashMap::<String, ProviderRequestResult>::new();
    let mut provider_secrets = HashMap::<String, HashMap<String, String>>::new();
    let mut halted = false;
    for request in &plan.requests {
        let (result, sensitive_values) = if halted {
            (
                blocked_result(
                    request,
                    "not attempted because an earlier provider request failed".to_string(),
                ),
                HashMap::new(),
            )
        } else {
            if !request.unresolved_identifiers.is_empty() {
                (
                    blocked_result(
                        request,
                        format!(
                            "missing provider identifiers: {}",
                            request.unresolved_identifiers.join(", ")
                        ),
                    ),
                    HashMap::new(),
                )
            } else {
                match resolve_request(request, resolver, &completed, &provider_secrets) {
                    Err(message) => (blocked_result(request, message), HashMap::new()),
                    Ok(resolved) => match transport.send(&resolved) {
                        Ok(response) => {
                            let mut identifiers = resolved.identifiers().clone();
                            identifiers.extend(response.identifiers);
                            (
                                ProviderRequestResult {
                                    request_id: request.id.clone(),
                                    resource_id: request.resource_id.clone(),
                                    status: ProviderRequestStatus::Applied,
                                    status_code: Some(response.status_code),
                                    provider_id: response.provider_id,
                                    identifiers,
                                    observed: response.observed,
                                    message: "provider request applied".to_string(),
                                },
                                response.sensitive_values,
                            )
                        }
                        Err(message) => (
                            ProviderRequestResult {
                                request_id: request.id.clone(),
                                resource_id: request.resource_id.clone(),
                                status: ProviderRequestStatus::Failed,
                                status_code: None,
                                provider_id: None,
                                identifiers: HashMap::new(),
                                observed: None,
                                message,
                            },
                            HashMap::new(),
                        ),
                    },
                }
            }
        };
        halted = halt_on_failure && result.status != ProviderRequestStatus::Applied;
        if result.status == ProviderRequestStatus::Applied {
            if !sensitive_values.is_empty() {
                provider_secrets.insert(result.request_id.clone(), sensitive_values);
            }
            completed.insert(result.request_id.clone(), result.clone());
        }
        results.push(result);
    }
    ProviderExecutionReport { results }
}

pub fn apply_provider_requests<T: ProviderTransport, R: RuntimeValueResolver>(
    manifest: &Manifest,
    migration_plan: &MigrationPlan,
    previous_state: Option<&StackState>,
    request_plan: &ProviderRequestPlan,
    transport: &T,
    resolver: &R,
) -> ProviderApplyOutcome {
    let execution = execute_provider_requests(request_plan, transport, resolver);
    let mut resources = previous_state
        .map(|state| {
            state
                .resources
                .iter()
                .cloned()
                .map(|resource| (resource.id.clone(), resource))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let desired = manifest
        .resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<HashMap<_, _>>();
    let requests = request_plan
        .requests
        .iter()
        .map(|request| (request.id.as_str(), request))
        .collect::<HashMap<_, _>>();
    let mut state_warnings = Vec::new();

    for result in &execution.results {
        if result.status != ProviderRequestStatus::Applied {
            continue;
        }
        let Some(request) = requests.get(result.request_id.as_str()) else {
            continue;
        };
        if request.action == PlanAction::Delete {
            resources.remove(&request.resource_id);
            continue;
        }
        let Some(resource) = desired.get(request.resource_id.as_str()).copied() else {
            continue;
        };
        let previous = resources.get(&request.resource_id);
        let auxiliary_request = request.id.ends_with(":environment")
            || request.id.ends_with(":settings")
            || request.id.ends_with(":variables");
        let mut identifiers = previous
            .map(|resource| resource.identifiers.clone())
            .unwrap_or_default();
        identifiers.extend(result.identifiers.clone());
        let provider_id = if auxiliary_request {
            previous.map(|resource| resource.provider_id.clone())
        } else {
            result
                .provider_id
                .clone()
                .or_else(|| previous.map(|resource| resource.provider_id.clone()))
                .or_else(|| stable_provider_id(request, resource, &identifiers))
        }
        .unwrap_or_else(|| resource.id.clone());
        let provider_resource = provider_definition(&request.provider).and_then(|definition| {
            definition
                .resources
                .into_iter()
                .find(|mapping| mapping.stack_kind == resource.kind)
                .map(|mapping| mapping.provider_resource)
        });
        let secrets = request
            .secret_references
            .iter()
            .map(|(name, provider_ref)| StateSecretRef {
                name: name.clone(),
                provider_ref: provider_ref.clone(),
            })
            .collect();
        match StateResource::from_applied(
            resource,
            provider_id,
            provider_resource,
            identifiers,
            secrets,
        ) {
            Ok(state_resource) => {
                resources.insert(request.resource_id.clone(), state_resource);
            }
            Err(message) => state_warnings.push(message),
        }
    }

    let mut resources = resources.into_values().collect::<Vec<_>>();
    resources.sort_by(|left, right| left.id.cmp(&right.id));
    let state = StackState {
        schema_version: STATE_SCHEMA_VERSION.to_string(),
        provider: migration_plan.target_provider.clone(),
        providers: previous_state
            .map(|state| state.providers.clone())
            .unwrap_or_default(),
        resources,
    };
    ProviderApplyOutcome {
        execution,
        state,
        state_warnings,
    }
}

fn resolve_request<R: RuntimeValueResolver>(
    request: &ProviderRequest,
    resolver: &R,
    completed: &HashMap<String, ProviderRequestResult>,
    provider_secrets: &HashMap<String, HashMap<String, String>>,
) -> Result<ResolvedProviderRequest, String> {
    let headers = resolve_auth(&request.auth, resolver)?;
    let body = request
        .body
        .as_ref()
        .map(|body| {
            let body = resolve_identifiers(body, &request.identifier_bindings, completed)?;
            resolve_secrets(
                &body,
                &request.secret_references,
                resolver,
                provider_secrets,
            )
        })
        .transpose()?;
    let url = resolve_identifier_text(&request.url, &request.identifier_bindings, completed)?;
    let identifiers = request
        .identifiers
        .iter()
        .map(|(name, value)| {
            Ok((
                name.clone(),
                resolve_identifier_text(value, &request.identifier_bindings, completed)?,
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    Ok(ResolvedProviderRequest {
        provider: request.provider.clone(),
        provider_resource: request.provider_resource.clone(),
        operation: request.operation.clone(),
        graphql_operation: request.graphql_operation.clone(),
        method: request.method.clone(),
        url,
        headers,
        body,
        identifiers,
    })
}

fn resolve_identifiers(
    value: &Value,
    bindings: &HashMap<String, ProviderIdentifierBinding>,
    completed: &HashMap<String, ProviderRequestResult>,
) -> Result<Value, String> {
    match value {
        Value::String(value) => {
            resolve_identifier_text(value, bindings, completed).map(Value::String)
        }
        Value::Array(values) => values
            .iter()
            .map(|value| resolve_identifiers(value, bindings, completed))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| {
                Ok((
                    key.clone(),
                    resolve_identifiers(value, bindings, completed)?,
                ))
            })
            .collect::<Result<Map<_, _>, String>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}

fn resolve_identifier_text(
    value: &str,
    bindings: &HashMap<String, ProviderIdentifierBinding>,
    completed: &HashMap<String, ProviderRequestResult>,
) -> Result<String, String> {
    let mut resolved = value.to_string();
    for (identifier, binding) in bindings {
        let placeholder = identifier_placeholder(identifier, binding);
        if !resolved.contains(&placeholder) {
            continue;
        }
        let result = completed.get(&binding.request_id).ok_or_else(|| {
            format!(
                "request `{}` depends on incomplete request `{}`",
                identifier, binding.request_id
            )
        })?;
        let replacement = if binding.response_identifier == "provider_id" {
            result.provider_id.as_ref()
        } else {
            result.identifiers.get(&binding.response_identifier)
        }
        .ok_or_else(|| {
            format!(
                "request `{}` did not return identifier `{}`",
                binding.request_id, binding.response_identifier
            )
        })?;
        resolved = resolved.replace(&placeholder, replacement);
    }
    Ok(resolved)
}

fn extract_response_values(
    provider: &str,
    provider_resource: &str,
    graphql_operation: Option<&str>,
    body: &Value,
) -> (
    Option<String>,
    HashMap<String, String>,
    HashMap<String, String>,
) {
    let mut identifiers = HashMap::new();
    let mut sensitive_values = HashMap::new();
    match provider {
        "vercel" => {
            let target = match provider_resource {
                "environment-variable" => "envId",
                "deployment" => "idOrUrl",
                "domain" => "id",
                _ => "idOrName",
            };
            copy_nested_response_id(body, "id", target, &mut identifiers);
            if provider_resource == "project" {
                copy_nested_response_id(body, "id", "projectId", &mut identifiers);
            }
            if provider_resource == "domain" {
                copy_nested_response_id(body, "name", "domain", &mut identifiers);
            }
        }
        "supabase" => match provider_resource {
            "storage-bucket" => {
                copy_nested_response_id(body, "id", "bucket_id", &mut identifiers);
                copy_nested_response_id(body, "name", "bucket_id", &mut identifiers);
            }
            "edge-function" => {
                copy_nested_response_id(body, "slug", "function_slug", &mut identifiers);
                copy_nested_response_id(body, "id", "function_id", &mut identifiers);
            }
            "project-database" => {
                copy_nested_response_id(body, "id", "ref", &mut identifiers);
                copy_nested_response_id(body, "ref", "ref", &mut identifiers);
            }
            _ => {}
        },
        "neon" => {
            if let Some(project) = body.get("project") {
                copy_response_id(project, "id", "project_id", &mut identifiers);
            }
            if let Some(branch) = body.get("branch") {
                copy_response_id(branch, "id", "branch_id", &mut identifiers);
            }
            if provider_resource == "role" {
                copy_nested_response_id(body, "name", "role_name", &mut identifiers);
            }
            if let Some(connection_uri) = find_string_by_key(body, "connection_uri") {
                sensitive_values.insert("DATABASE_URL".to_string(), connection_uri.to_string());
                sensitive_values.insert("connection_uri".to_string(), connection_uri.to_string());
            }
        }
        "railway" => {
            if let Some(operation) = graphql_operation {
                if let Some(value) = body.get("data").and_then(|data| data.get(operation)) {
                    let identifier = match operation {
                        "projectCreate" | "projectUpdate" => "projectId",
                        "environmentCreate" | "environmentUpdate" => "environmentId",
                        "serviceCreate" | "serviceInstanceUpdate" => "serviceId",
                        "customDomainCreate" | "customDomainUpdate" => "id",
                        _ => "id",
                    };
                    copy_response_id(value, "id", identifier, &mut identifiers);
                }
            }
        }
        _ => copy_response_id(body, "id", "id", &mut identifiers),
    }
    let provider_id = [
        "idOrName",
        "ref",
        "project_id",
        "branch_id",
        "role_name",
        "serviceId",
        "environmentId",
        "envId",
        "bucket_id",
        "function_id",
        "function_slug",
        "idOrUrl",
        "id",
        "domain",
    ]
    .iter()
    .find_map(|key| identifiers.get(*key).cloned());
    (provider_id, identifiers, sensitive_values)
}

fn find_string_by_key<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match value {
        Value::Object(values) => {
            if let Some(value) = values.get(key).and_then(Value::as_str) {
                return Some(value);
            }
            values
                .values()
                .find_map(|value| find_string_by_key(value, key))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_string_by_key(value, key)),
        _ => None,
    }
}

fn redact_provider_response(value: &Value, provider_resource: &str) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| {
                    let sensitive_value = looks_sensitive_key(key)
                        || key.eq_ignore_ascii_case("connection_uri")
                        || (key.eq_ignore_ascii_case("value")
                            && ["environment-variable", "project-secret", "variable"]
                                .contains(&provider_resource));
                    (
                        key.clone(),
                        if sensitive_value {
                            Value::String("${redacted}".to_string())
                        } else {
                            redact_provider_response(value, provider_resource)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| redact_provider_response(value, provider_resource))
                .collect(),
        ),
        _ => value.clone(),
    }
}

struct DesiredObservationField {
    name: String,
    aliases: Vec<String>,
    expected: Value,
}

fn desired_observation_fields(
    manifest: &Manifest,
    resource: &Resource,
) -> Vec<DesiredObservationField> {
    let mut fields = Vec::new();
    let mut push = |name: &str, aliases: Vec<&'static str>, expected: Option<Value>| {
        if let Some(expected) = expected {
            if !is_secret_placeholder(&expected) {
                fields.push(DesiredObservationField {
                    name: name.to_string(),
                    aliases: aliases.into_iter().map(str::to_string).collect(),
                    expected,
                });
            }
        }
    };
    match resource.kind {
        ResourceKind::Project => push(
            "name",
            vec!["name"],
            resource
                .properties
                .get("name")
                .cloned()
                .or_else(|| Some(Value::String(manifest.application.name.clone()))),
        ),
        ResourceKind::Environment => push(
            "name",
            vec!["name"],
            resource.properties.get("name").cloned(),
        ),
        ResourceKind::WebService | ResourceKind::StaticSite => {
            push(
                "name",
                vec!["name"],
                Some(Value::String(service_name(manifest, resource))),
            );
            push(
                "framework",
                vec!["framework"],
                resource.properties.pointer("/build/framework").cloned(),
            );
            push(
                "build.command",
                vec!["buildCommand", "build_command"],
                resource.properties.pointer("/build/command").cloned(),
            );
            push(
                "run.command",
                vec!["startCommand", "start_command"],
                resource.properties.pointer("/run/command").cloned(),
            );
            push(
                "deploy.health_check",
                vec!["healthcheckPath", "health_check"],
                resource.properties.pointer("/deploy/health_check").cloned(),
            );
            push(
                "deploy.replicas",
                vec!["numReplicas", "replicas"],
                resource.properties.pointer("/deploy/replicas").cloned(),
            );
        }
        ResourceKind::Database => {
            push(
                "name",
                vec!["name", "database_name"],
                resource.properties.get("name").cloned(),
            );
            push(
                "region",
                vec!["region", "region_id"],
                resource.properties.get("region").cloned(),
            );
            push(
                "version",
                vec!["version", "pg_version"],
                resource.properties.get("version").cloned(),
            );
        }
        ResourceKind::DatabaseBranch => push(
            "name",
            vec!["name"],
            resource.properties.get("name").cloned(),
        ),
        ResourceKind::DatabaseRole => push(
            "name",
            vec!["name", "role_name"],
            resource.properties.get("name").cloned(),
        ),
        ResourceKind::Domain => push(
            "hostname",
            vec!["domain", "hostname", "name"],
            resource.properties.get("hostname").cloned(),
        ),
        ResourceKind::StorageBucket => {
            push(
                "name",
                vec!["name", "id", "bucket_id"],
                resource.properties.get("name").cloned(),
            );
            push(
                "public",
                vec!["public"],
                resource.properties.get("public").cloned(),
            );
        }
        ResourceKind::Function => push(
            "name",
            vec!["name", "slug"],
            resource.properties.get("name").cloned(),
        ),
        ResourceKind::Auth => {
            if let Some(properties) = resource.properties.as_object() {
                for (name, expected) in properties {
                    if !looks_sensitive_key(name) {
                        push(name, vec![], Some(expected.clone()));
                    }
                }
            }
        }
        _ => {}
    }
    for field in &mut fields {
        if field.aliases.is_empty() {
            field.aliases = vec![field.name.clone()];
        }
    }
    fields
}

fn is_secret_placeholder(value: &Value) -> bool {
    value
        .as_str()
        .map(|value| value.starts_with("${secret:") || value == "${redacted}")
        .unwrap_or(false)
}

fn find_object_by_id<'a>(value: &'a Value, provider_id: &str) -> Option<&'a Value> {
    match value {
        Value::Object(values) => {
            if values
                .get("id")
                .or_else(|| values.get("ref"))
                .and_then(Value::as_str)
                == Some(provider_id)
            {
                return Some(value);
            }
            values
                .values()
                .find_map(|value| find_object_by_id(value, provider_id))
        }
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_object_by_id(value, provider_id)),
        _ => None,
    }
}

fn find_value_by_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(values) => values.get(key).or_else(|| {
            values
                .values()
                .find_map(|value| find_value_by_key(value, key))
        }),
        Value::Array(values) => values
            .iter()
            .find_map(|value| find_value_by_key(value, key)),
        _ => None,
    }
}

fn values_equivalent(expected: &Value, actual: &Value) -> bool {
    if expected == actual {
        return true;
    }
    match (expected, actual) {
        (Value::Number(expected), Value::String(actual)) => expected.to_string() == *actual,
        (Value::String(expected), Value::Number(actual)) => *expected == actual.to_string(),
        _ => false,
    }
}

fn copy_response_id(
    value: &Value,
    source_key: &str,
    target_key: &str,
    identifiers: &mut HashMap<String, String>,
) {
    if let Some(value) = value.get(source_key).and_then(Value::as_str) {
        identifiers.insert(target_key.to_string(), value.to_string());
    }
}

fn copy_nested_response_id(
    value: &Value,
    source_key: &str,
    target_key: &str,
    identifiers: &mut HashMap<String, String>,
) {
    if let Some(value) = find_string_by_key(value, source_key) {
        identifiers.insert(target_key.to_string(), value.to_string());
    }
}

fn stable_provider_id(
    request: &ProviderRequest,
    resource: &Resource,
    identifiers: &HashMap<String, String>,
) -> Option<String> {
    let keys = match request.provider_resource.as_str() {
        "project" => &["projectId", "idOrName", "project_id"][..],
        "project-database" => &["ref", "project_id"][..],
        "environment" => &["environmentId", "environment_id"][..],
        "environment-variable" => &["envId", "name"][..],
        "storage-bucket" => &["bucket_id"][..],
        "edge-function" => &["function_slug", "function_id"][..],
        "project-secret" | "variable" => &["name"][..],
        "branch" => &["branch_id"][..],
        "role" => &["role_name"][..],
        "service" => &["serviceId", "service_id"][..],
        "domain" | "custom-domain" => &["id", "domain"][..],
        "deployment" => &["idOrUrl", "id"][..],
        _ => &[][..],
    };
    keys.iter()
        .find_map(|key| identifiers.get(*key).cloned())
        .or_else(|| {
            resource
                .properties
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn resolve_auth<R: RuntimeValueResolver>(
    auth: &ProviderAuthRequirement,
    resolver: &R,
) -> Result<Vec<(String, String)>, String> {
    for alternative in &auth.alternatives {
        let mut headers = Vec::new();
        let mut complete = true;
        for header in alternative {
            let Some(value) = resolver.environment(&header.value_from_env) else {
                complete = false;
                break;
            };
            let value = match &header.scheme {
                Some(scheme) => format!("{scheme} {value}"),
                None => value,
            };
            headers.push((header.name.clone(), value));
        }
        if complete {
            return Ok(headers);
        }
    }
    let names = auth
        .alternatives
        .iter()
        .map(|alternative| {
            alternative
                .iter()
                .map(|header| header.value_from_env.as_str())
                .collect::<Vec<_>>()
                .join(" + ")
        })
        .collect::<Vec<_>>()
        .join(" or ");
    Err(format!(
        "provider credentials are not set; expected {names}"
    ))
}

fn resolve_secrets<R: RuntimeValueResolver>(
    value: &Value,
    references: &HashMap<String, String>,
    resolver: &R,
    provider_secrets: &HashMap<String, HashMap<String, String>>,
) -> Result<Value, String> {
    match value {
        Value::String(value) => {
            let Some(name) = secret_name(value) else {
                return Ok(Value::String(value.clone()));
            };
            let reference = references
                .get(name)
                .ok_or_else(|| format!("secret `{name}` has no configured reference"))?;
            resolver
                .secret(reference)
                .or_else(|_| resolve_provider_secret(reference, provider_secrets))
                .map(Value::String)
        }
        Value::Array(values) => values
            .iter()
            .map(|value| resolve_secrets(value, references, resolver, provider_secrets))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| {
                Ok((
                    key.clone(),
                    resolve_secrets(value, references, resolver, provider_secrets)?,
                ))
            })
            .collect::<Result<Map<_, _>, String>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}

fn resolve_provider_secret(
    reference: &str,
    provider_secrets: &HashMap<String, HashMap<String, String>>,
) -> Result<String, String> {
    let parts = reference.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        return Err(format!("secret reference `{reference}` is not available"));
    }
    let resource_name = parts[parts.len() - 2];
    let secret_name = parts[parts.len() - 1];
    let prefixes = [
        format!("database:{resource_name}:"),
        format!("service:{resource_name}:"),
        format!("secret:{resource_name}:"),
    ];
    provider_secrets
        .iter()
        .find(|(request_id, values)| {
            prefixes.iter().any(|prefix| request_id.starts_with(prefix))
                && values.contains_key(secret_name)
        })
        .and_then(|(_, values)| values.get(secret_name))
        .cloned()
        .ok_or_else(|| format!("provider secret `{reference}` is not available"))
}

fn request_url(
    definition: &ProviderDefinition,
    operation: &ProviderApiOperationSpec,
    context: &ProviderContext,
    identifiers: &HashMap<String, String>,
) -> String {
    let base_url = context
        .base_url
        .as_deref()
        .unwrap_or(definition.api.base_url.as_str())
        .trim_end_matches('/');
    if operation.graphql_operation.is_some() {
        return base_url.to_string();
    }
    let mut path = operation.path.clone();
    for (name, value) in identifiers {
        path = path.replace(&format!("{{{name}}}"), value);
    }
    let mut url = if path.starts_with("http://") || path.starts_with("https://") {
        path
    } else {
        format!("{base_url}/{path}", path = path.trim_start_matches('/'))
    };
    if definition.name == "vercel" {
        if let Some(team_id) = identifiers
            .get("teamId")
            .or_else(|| identifiers.get("team_id"))
        {
            append_query_parameter(&mut url, "teamId", team_id);
        }
    }
    url
}

fn append_query_parameter(url: &mut String, name: &str, value: &str) {
    url.push(if url.contains('?') { '&' } else { '?' });
    url.push_str(&percent_encode_query(name));
    url.push('=');
    url.push_str(&percent_encode_query(value));
}

fn percent_encode_query(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
        }
    }
    encoded
}

fn collect_identifiers(
    manifest: &Manifest,
    step: &PlanStep,
    resource: Option<&Resource>,
    context: &ProviderContext,
    required_identifiers: &[String],
    plan_steps: &HashMap<&str, &PlanStep>,
) -> (
    HashMap<String, String>,
    HashMap<String, ProviderIdentifierBinding>,
) {
    let mut identifiers = context.identifiers.clone();
    let mut bindings = HashMap::new();
    identifiers.extend(step.identifiers.clone());
    if let Some(config) = resource
        .and_then(|resource| resource.properties.get("provider_config"))
        .and_then(Value::as_object)
    {
        for (key, value) in config {
            if let Some(value) = value.as_str() {
                identifiers.insert(key.clone(), value.to_string());
            }
        }
    }
    if let Some(properties) = resource.and_then(|resource| resource.properties.as_object()) {
        for name in [
            "project_id",
            "projectId",
            "environment_id",
            "environmentId",
            "service_id",
            "serviceId",
            "branch_id",
            "branchId",
            "database_name",
            "role_name",
            "project_url",
            "bucket_id",
            "function_slug",
            "idOrName",
            "idOrUrl",
            "envId",
        ] {
            if let Some(value) = properties.get(name).and_then(Value::as_str) {
                identifiers.insert(name.to_string(), value.to_string());
            }
        }
        copy_property_identifier(properties, "name", "name", &mut identifiers);
        copy_property_identifier(properties, "key", "name", &mut identifiers);
        match resource.map(|resource| &resource.kind) {
            Some(ResourceKind::StorageBucket) => {
                copy_property_identifier(properties, "name", "bucket_id", &mut identifiers)
            }
            Some(ResourceKind::Function) => {
                copy_property_identifier(properties, "slug", "function_slug", &mut identifiers)
            }
            Some(ResourceKind::DatabaseRole) => {
                copy_property_identifier(properties, "name", "role_name", &mut identifiers)
            }
            _ => {}
        }
    }
    if let Some(provider_id) = &step.provider_id {
        identifiers
            .entry("id".to_string())
            .or_insert_with(|| provider_id.clone());
        match resource.map(|resource| &resource.kind) {
            Some(ResourceKind::Project) => {
                identifiers
                    .entry("project_id".to_string())
                    .or_insert_with(|| provider_id.clone());
            }
            Some(ResourceKind::Environment) => {
                identifiers
                    .entry("environment_id".to_string())
                    .or_insert_with(|| provider_id.clone());
            }
            Some(ResourceKind::DatabaseBranch) => {
                identifiers
                    .entry("branch_id".to_string())
                    .or_insert_with(|| provider_id.clone());
            }
            Some(ResourceKind::DatabaseRole) => {
                identifiers
                    .entry("role_name".to_string())
                    .or_insert_with(|| provider_id.clone());
            }
            Some(ResourceKind::WebService | ResourceKind::StaticSite) => {
                identifiers
                    .entry("serviceId".to_string())
                    .or_insert_with(|| provider_id.clone());
                identifiers
                    .entry("idOrName".to_string())
                    .or_insert_with(|| provider_id.clone());
            }
            Some(ResourceKind::Database) => match step.provider.as_deref() {
                Some("supabase") => {
                    identifiers
                        .entry("ref".to_string())
                        .or_insert_with(|| provider_id.clone());
                }
                Some("neon") => {
                    identifiers
                        .entry("project_id".to_string())
                        .or_insert_with(|| provider_id.clone());
                }
                Some("railway") => {
                    identifiers
                        .entry("serviceId".to_string())
                        .or_insert_with(|| provider_id.clone());
                }
                _ => {}
            },
            _ => {}
        }
    }
    if let Some(hostname) = resource
        .and_then(|resource| resource.properties.get("hostname"))
        .and_then(Value::as_str)
    {
        identifiers.insert("domain".to_string(), hostname.to_string());
    }
    add_identifier_aliases(&mut identifiers);
    let dependency_ids = provider_dependency_ids(manifest, resource);
    for identifier in required_identifiers {
        if has_identifier(&identifiers, identifier) {
            continue;
        }
        let Some((dependency, response_identifier)) = dependency_ids
            .iter()
            .filter_map(|dependency_id| {
                let dependency = plan_steps.get(dependency_id.as_str()).copied()?;
                if dependency.provider != step.provider {
                    return None;
                }
                if !dependency_can_supply(manifest, dependency_id, identifier) {
                    return None;
                }
                dependency_response_identifier(identifier, dependency)
                    .map(|response_identifier| (dependency, response_identifier))
            })
            .next()
        else {
            continue;
        };
        if let Some(value) = dependency_identifier_value(dependency, identifier) {
            identifiers.insert(identifier.clone(), value);
        } else if let Some(request_id) = output_request_id(dependency) {
            let binding = ProviderIdentifierBinding {
                request_id,
                response_identifier,
            };
            let placeholder = identifier_placeholder(identifier, &binding);
            let value = if identifier == "project_url" {
                format!("https://{placeholder}.supabase.co")
            } else {
                placeholder
            };
            identifiers.insert(identifier.clone(), value);
            bindings.insert(identifier.clone(), binding);
        }
    }
    if let Some(project_ref) = identifiers.get("ref").cloned() {
        identifiers
            .entry("project_url".to_string())
            .or_insert_with(|| format!("https://{project_ref}.supabase.co"));
    }
    identifiers
        .entry("idOrName".to_string())
        .or_insert_with(|| manifest.application.name.clone());
    (identifiers, bindings)
}

fn copy_property_identifier(
    properties: &Map<String, Value>,
    source: &str,
    target: &str,
    identifiers: &mut HashMap<String, String>,
) {
    if let Some(value) = properties.get(source).and_then(Value::as_str) {
        identifiers
            .entry(target.to_string())
            .or_insert_with(|| value.to_string());
    }
}

fn provider_dependency_ids(manifest: &Manifest, resource: Option<&Resource>) -> Vec<String> {
    let mut dependency_ids = resource
        .map(|resource| resource.depends_on.clone())
        .unwrap_or_default();
    let direct_dependencies = dependency_ids.clone();
    for dependency_id in direct_dependencies {
        if let Some(dependency) = manifest
            .resources
            .iter()
            .find(|resource| resource.id == dependency_id)
        {
            for transitive in &dependency.depends_on {
                if !dependency_ids.contains(transitive) {
                    dependency_ids.push(transitive.clone());
                }
            }
        }
    }
    dependency_ids
}

fn dependency_can_supply(manifest: &Manifest, dependency_id: &str, required: &str) -> bool {
    let Some(resource) = manifest
        .resources
        .iter()
        .find(|resource| resource.id == dependency_id)
    else {
        return false;
    };
    match required {
        "projectId" => resource.kind == ResourceKind::Project,
        "project_id" => matches!(
            resource.kind,
            ResourceKind::Project | ResourceKind::Database
        ),
        "environmentId" | "environment_id" => resource.kind == ResourceKind::Environment,
        "serviceId" | "service_id" => matches!(
            resource.kind,
            ResourceKind::WebService | ResourceKind::StaticSite | ResourceKind::Database
        ),
        "branch_id" | "branchId" => matches!(
            resource.kind,
            ResourceKind::DatabaseBranch | ResourceKind::Database
        ),
        "ref" | "project_url" => {
            matches!(
                resource.kind,
                ResourceKind::Project | ResourceKind::Database
            )
        }
        "idOrName" => matches!(
            resource.kind,
            ResourceKind::Project | ResourceKind::WebService | ResourceKind::StaticSite
        ),
        "id" => true,
        _ => false,
    }
}

fn dependency_response_identifier(required: &str, dependency: &PlanStep) -> Option<String> {
    let response_identifier = match required {
        "projectId" => "projectId",
        "project_id" => "project_id",
        "environmentId" => "environmentId",
        "environment_id" => "environment_id",
        "serviceId" | "service_id" => "serviceId",
        "branch_id" | "branchId" => "branch_id",
        "ref" | "project_url" => "ref",
        "idOrName" => "idOrName",
        "id" => "provider_id",
        _ => return None,
    };
    if dependency.provider_id.is_some() || output_request_id(dependency).is_some() {
        Some(response_identifier.to_string())
    } else {
        None
    }
}

fn dependency_identifier_value(dependency: &PlanStep, required: &str) -> Option<String> {
    if let Some(value) = dependency.identifiers.get(required) {
        return Some(value.clone());
    }
    let aliases = match required {
        "projectId" => &["project_id"][..],
        "project_id" => &["projectId"][..],
        "environmentId" => &["environment_id"][..],
        "environment_id" => &["environmentId"][..],
        "serviceId" => &["service_id"][..],
        "service_id" => &["serviceId"][..],
        "branch_id" => &["branchId"][..],
        "branchId" => &["branch_id"][..],
        "project_url" => &["ref"][..],
        _ => &[][..],
    };
    for alias in aliases {
        if let Some(value) = dependency.identifiers.get(*alias) {
            return Some(if required == "project_url" {
                format!("https://{value}.supabase.co")
            } else {
                value.clone()
            });
        }
    }
    dependency.provider_id.as_ref().map(|provider_id| {
        if required == "project_url" {
            format!("https://{provider_id}.supabase.co")
        } else {
            provider_id.clone()
        }
    })
}
fn output_request_id(step: &PlanStep) -> Option<String> {
    let suffix = match step.action {
        PlanAction::Create => "create",
        PlanAction::Update => "update",
        PlanAction::Replace => "replace-create",
        PlanAction::Noop | PlanAction::Delete | PlanAction::Manual | PlanAction::Unsupported => {
            return None
        }
    };
    Some(format!("{}:{suffix}", step.resource_id))
}

fn identifier_placeholder(identifier: &str, binding: &ProviderIdentifierBinding) -> String {
    format!(
        "${{request:{}:{}:{identifier}}}",
        binding.request_id, binding.response_identifier
    )
}

fn add_identifier_aliases(identifiers: &mut HashMap<String, String>) {
    let aliases = [
        ("project_id", "projectId"),
        ("environment_id", "environmentId"),
        ("service_id", "serviceId"),
        ("branch_id", "branchId"),
        ("team_id", "teamId"),
    ];
    for (snake, camel) in aliases {
        if let Some(value) = identifiers.get(snake).cloned() {
            identifiers.entry(camel.to_string()).or_insert(value);
        } else if let Some(value) = identifiers.get(camel).cloned() {
            identifiers.entry(snake.to_string()).or_insert(value);
        }
    }
}

fn has_identifier(identifiers: &HashMap<String, String>, name: &str) -> bool {
    identifiers
        .get(name)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn mapped_resource(definition: &ProviderDefinition, resource: &Resource) -> Option<String> {
    definition
        .resources
        .iter()
        .find(|mapping| mapping.stack_kind == resource.kind)
        .map(|mapping| mapping.provider_resource.clone())
}

fn action_operation(action: &PlanAction) -> ProviderOperation {
    match action {
        PlanAction::Create => ProviderOperation::ApplyCreate,
        PlanAction::Update => ProviderOperation::ApplyUpdate,
        PlanAction::Delete => ProviderOperation::ApplyDelete,
        PlanAction::Replace => ProviderOperation::ApplyCreate,
        PlanAction::Noop | PlanAction::Manual | PlanAction::Unsupported => ProviderOperation::Plan,
    }
}

fn action_suffix(action: &PlanAction) -> &'static str {
    match action {
        PlanAction::Create => "create",
        PlanAction::Update => "update",
        PlanAction::Delete => "delete",
        PlanAction::Replace => "replace",
        PlanAction::Noop => "noop",
        PlanAction::Manual => "manual",
        PlanAction::Unsupported => "unsupported",
    }
}

fn auth_requirement(provider: &str, provider_resource: &str) -> ProviderAuthRequirement {
    let alternatives = match (provider, provider_resource) {
        ("supabase", "storage-bucket") => vec![vec![
            ProviderAuthHeader {
                name: "Authorization".to_string(),
                value_from_env: "SUPABASE_SERVICE_ROLE_KEY".to_string(),
                scheme: Some("Bearer".to_string()),
            },
            ProviderAuthHeader {
                name: "apikey".to_string(),
                value_from_env: "SUPABASE_SERVICE_ROLE_KEY".to_string(),
                scheme: None,
            },
        ]],
        ("vercel", _) => bearer_auth("VERCEL_TOKEN"),
        ("supabase", _) => bearer_auth("SUPABASE_ACCESS_TOKEN"),
        ("neon", _) => bearer_auth("NEON_API_KEY"),
        ("railway", "project") => bearer_auth("RAILWAY_TOKEN"),
        ("railway", _) => vec![
            vec![ProviderAuthHeader {
                name: "Authorization".to_string(),
                value_from_env: "RAILWAY_TOKEN".to_string(),
                scheme: Some("Bearer".to_string()),
            }],
            vec![ProviderAuthHeader {
                name: "Project-Access-Token".to_string(),
                value_from_env: "RAILWAY_PROJECT_TOKEN".to_string(),
                scheme: None,
            }],
        ],
        _ => vec![],
    };
    ProviderAuthRequirement { alternatives }
}

fn bearer_auth(env_var: &str) -> Vec<Vec<ProviderAuthHeader>> {
    vec![vec![ProviderAuthHeader {
        name: "Authorization".to_string(),
        value_from_env: env_var.to_string(),
        scheme: Some("Bearer".to_string()),
    }]]
}

fn request_body(
    provider: &str,
    provider_resource: &str,
    operation: &ProviderApiOperationSpec,
    identifiers: &HashMap<String, String>,
    manifest: &Manifest,
    resource: Option<&Resource>,
    action: &PlanAction,
) -> Option<Value> {
    if matches!(operation.method, HttpMethod::Get) {
        return if provider == "railway" {
            railway_body(operation, identifiers, manifest, resource, action)
        } else {
            None
        };
    }
    if matches!(operation.method, HttpMethod::Delete) {
        return match (provider, provider_resource) {
            ("railway", _) => railway_body(operation, identifiers, manifest, resource, action),
            ("supabase", "project-secret") => supabase_secret_delete_body(resource),
            _ => None,
        };
    }
    match provider {
        "vercel" => vercel_body(provider_resource, manifest, resource, action),
        "supabase" => supabase_body(provider_resource, identifiers, manifest, resource),
        "neon" => neon_body(provider_resource, manifest, resource),
        "railway" => railway_body(operation, identifiers, manifest, resource, action),
        _ => resource.map(|resource| resource.properties.clone()),
    }
}

fn vercel_body(
    provider_resource: &str,
    manifest: &Manifest,
    resource: Option<&Resource>,
    action: &PlanAction,
) -> Option<Value> {
    let resource = resource?;
    match provider_resource {
        "project" => {
            let mut body = Map::new();
            if matches!(action, PlanAction::Create) {
                body.insert(
                    "name".to_string(),
                    Value::String(service_name(manifest, resource)),
                );
            }
            copy_nested_string(resource, &["build", "framework"], &mut body, "framework");
            copy_nested_string(resource, &["build", "command"], &mut body, "buildCommand");
            copy_nested_string(resource, &["build", "output"], &mut body, "outputDirectory");
            copy_nested_string(resource, &["source", "path"], &mut body, "rootDirectory");
            Some(Value::Object(body))
        }
        "domain" => resource
            .properties
            .get("hostname")
            .cloned()
            .map(|hostname| serde_json::json!({ "name": hostname })),
        "environment-variable" => Some(provider_properties(resource)),
        _ => Some(provider_properties(resource)),
    }
}

fn supabase_body(
    provider_resource: &str,
    identifiers: &HashMap<String, String>,
    manifest: &Manifest,
    resource: Option<&Resource>,
) -> Option<Value> {
    let resource = resource?;
    match provider_resource {
        "project-database" => {
            let mut body = Map::new();
            body.insert(
                "name".to_string(),
                Value::String(manifest.application.name.clone()),
            );
            copy_identifier(
                identifiers,
                "organization_slug",
                &mut body,
                "organization_slug",
            );
            copy_property(resource, "region", &mut body, "region");
            copy_property(resource, "plan", &mut body, "plan");
            copy_property(resource, "db_pass", &mut body, "db_pass");
            Some(Value::Object(body))
        }
        "project-secret" => Some(match provider_properties(resource) {
            Value::Array(values) => Value::Array(values),
            properties => Value::Array(vec![properties]),
        }),
        "edge-function" => {
            let mut metadata = Map::new();
            for key in [
                "name",
                "entrypoint_path",
                "import_map_path",
                "verify_jwt",
                "static_patterns",
            ] {
                copy_property(resource, key, &mut metadata, key);
            }
            if !metadata.contains_key("name") {
                copy_property(resource, "slug", &mut metadata, "name");
            }
            let files = resource
                .properties
                .get("files")
                .cloned()
                .or_else(|| {
                    resource
                        .properties
                        .get("source_path")
                        .cloned()
                        .map(|path| Value::Array(vec![path]))
                })
                .unwrap_or_else(|| Value::Array(vec![]));
            Some(serde_json::json!({
                "metadata": metadata,
                "files": files,
            }))
        }
        _ => Some(provider_properties(resource)),
    }
}

fn supabase_secret_delete_body(resource: Option<&Resource>) -> Option<Value> {
    let resource = resource?;
    let names = match &resource.properties {
        Value::Array(values) => values
            .iter()
            .filter_map(|value| value.get("name").and_then(Value::as_str))
            .map(|name| Value::String(name.to_string()))
            .collect::<Vec<_>>(),
        Value::Object(properties) => properties
            .get("name")
            .and_then(Value::as_str)
            .map(|name| vec![Value::String(name.to_string())])
            .unwrap_or_default(),
        _ => vec![],
    };
    (!names.is_empty()).then_some(Value::Array(names))
}

fn supabase_edge_function_form(body: &Value) -> Result<reqwest::blocking::multipart::Form, String> {
    let metadata = body
        .get("metadata")
        .and_then(Value::as_object)
        .ok_or_else(|| "Supabase Edge Function request requires metadata".to_string())?;
    let mut form = reqwest::blocking::multipart::Form::new()
        .text("metadata", Value::Object(metadata.clone()).to_string());
    for file in body
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (path, file_name) = match file {
            Value::String(path) => (path.as_str(), None),
            Value::Object(file) => {
                let path = file
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Edge Function file object requires `path`".to_string())?;
                (path, file.get("name").and_then(Value::as_str))
            }
            _ => {
                return Err(
                    "Edge Function `files` entries must be paths or {path, name} objects"
                        .to_string(),
                )
            }
        };
        let bytes = std::fs::read(path)
            .map_err(|err| format!("failed to read Edge Function source `{path}`: {err}"))?;
        let file_name = file_name
            .map(str::to_string)
            .or_else(|| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| format!("Edge Function source `{path}` has no file name"))?;
        form = form.part(
            "file",
            reqwest::blocking::multipart::Part::bytes(bytes).file_name(file_name),
        );
    }
    Ok(form)
}

fn neon_body(
    provider_resource: &str,
    manifest: &Manifest,
    resource: Option<&Resource>,
) -> Option<Value> {
    let resource = resource?;
    if provider_resource == "branch" {
        return Some(serde_json::json!({ "branch": provider_properties(resource) }));
    }
    if provider_resource == "role" {
        return Some(serde_json::json!({ "role": provider_properties(resource) }));
    }
    if provider_resource != "project-branch-database" {
        return Some(provider_properties(resource));
    }
    let mut project = Map::new();
    project.insert(
        "name".to_string(),
        Value::String(manifest.application.name.clone()),
    );
    copy_property(resource, "region", &mut project, "region_id");
    if let Some(version) = resource.properties.get("version").and_then(Value::as_str) {
        if let Ok(version) = version.parse::<u64>() {
            project.insert("pg_version".to_string(), Value::Number(version.into()));
        }
    }
    Some(serde_json::json!({ "project": project }))
}

fn railway_body(
    operation: &ProviderApiOperationSpec,
    identifiers: &HashMap<String, String>,
    manifest: &Manifest,
    resource: Option<&Resource>,
    _action: &PlanAction,
) -> Option<Value> {
    let operation_name = operation.graphql_operation.as_deref()?;
    let (query, variables) = match operation_name {
        "projects" => (
            "query projects { projects { edges { node { id name description } } } }",
            serde_json::json!({}),
        ),
        "project" => (
            "query project($id: String!) { project(id: $id) { id name description services { edges { node { id name } } } environments { edges { node { id name } } } } }",
            serde_json::json!({ "id": identifier(identifiers, "projectId") }),
        ),
        "environments" => (
            "query environments($projectId: String!) { environments(projectId: $projectId) { edges { node { id name } } } }",
            serde_json::json!({ "projectId": identifier(identifiers, "projectId") }),
        ),
        "environment" => (
            "query environment($id: String!) { environment(id: $id) { id name serviceInstances { edges { node { serviceId } } } } }",
            serde_json::json!({ "id": identifier(identifiers, "environmentId") }),
        ),
        "variables" => (
            "query variables($projectId: String!, $environmentId: String!, $serviceId: String, $unrendered: Boolean) { variables(projectId: $projectId, environmentId: $environmentId, serviceId: $serviceId, unrendered: $unrendered) }",
            serde_json::json!({
                "projectId": identifier(identifiers, "projectId"),
                "environmentId": identifier(identifiers, "environmentId"),
                "serviceId": identifier(identifiers, "serviceId"),
                "unrendered": true,
            }),
        ),
        "domains" => (
            "query domains($serviceId: String!, $environmentId: String!) { domains(serviceId: $serviceId, environmentId: $environmentId) { serviceDomains { id domain } customDomains { id domain status { certificateStatus } } } }",
            serde_json::json!({
                "serviceId": identifier(identifiers, "serviceId"),
                "environmentId": identifier(identifiers, "environmentId"),
            }),
        ),
        "projectCreate" => {
            let mut input = Map::new();
            input.insert(
                "name".to_string(),
                resource
                    .and_then(|resource| resource.properties.get("name"))
                    .cloned()
                    .unwrap_or_else(|| Value::String(manifest.application.name.clone())),
            );
            (
                "mutation projectCreate($input: ProjectCreateInput!) { projectCreate(input: $input) { id name } }",
                serde_json::json!({ "input": input }),
            )
        }
        "projectUpdate" => (
            "mutation projectUpdate($id: String!, $input: ProjectUpdateInput!) { projectUpdate(id: $id, input: $input) { id name } }",
            serde_json::json!({
                "id": identifier(identifiers, "projectId"),
                "input": resource.map(provider_properties).unwrap_or_default(),
            }),
        ),
        "projectDelete" => (
            "mutation projectDelete($id: String!) { projectDelete(id: $id) }",
            serde_json::json!({ "id": identifier(identifiers, "projectId") }),
        ),
        "environmentCreate" => {
            let mut input = Map::new();
            copy_identifier(identifiers, "projectId", &mut input, "projectId");
            input.insert(
                "name".to_string(),
                resource
                    .and_then(|resource| resource.properties.get("name"))
                    .cloned()
                    .unwrap_or_else(|| Value::String("production".to_string())),
            );
            (
                "mutation environmentCreate($input: EnvironmentCreateInput!) { environmentCreate(input: $input) { id name } }",
                serde_json::json!({ "input": input }),
            )
        }
        "environmentUpdate" => (
            "mutation environmentUpdate($id: String!, $input: EnvironmentUpdateInput!) { environmentUpdate(id: $id, input: $input) { id name } }",
            serde_json::json!({
                "id": identifier(identifiers, "environmentId"),
                "input": resource.map(provider_properties).unwrap_or_default(),
            }),
        ),
        "environmentDelete" => (
            "mutation environmentDelete($id: String!) { environmentDelete(id: $id) }",
            serde_json::json!({ "id": identifier(identifiers, "environmentId") }),
        ),
        "serviceCreate" => {
            let mut input = Map::new();
            copy_identifier(identifiers, "projectId", &mut input, "projectId");
            if let Some(resource) = resource {
                input.insert(
                    "name".to_string(),
                    Value::String(service_name(manifest, resource)),
                );
                if let Some(repository) = resource
                    .properties
                    .pointer("/source/repository")
                    .and_then(Value::as_str)
                {
                    input.insert(
                        "source".to_string(),
                        serde_json::json!({ "repo": repository }),
                    );
                }
            }
            (
                "mutation serviceCreate($input: ServiceCreateInput!) { serviceCreate(input: $input) { id name } }",
                serde_json::json!({ "input": input }),
            )
        }
        "serviceInstanceUpdate" => {
            let mut variables = Map::new();
            copy_identifier(identifiers, "serviceId", &mut variables, "serviceId");
            copy_identifier(
                identifiers,
                "environmentId",
                &mut variables,
                "environmentId",
            );
            let mut input = Map::new();
            if let Some(resource) = resource {
                copy_nested_string(
                    resource,
                    &["run", "command"],
                    &mut input,
                    "startCommand",
                );
                copy_nested_string(
                    resource,
                    &["build", "command"],
                    &mut input,
                    "buildCommand",
                );
                copy_nested_string(
                    resource,
                    &["deploy", "health_check"],
                    &mut input,
                    "healthcheckPath",
                );
                if let Some(replicas) = resource
                    .properties
                    .pointer("/deploy/replicas")
                    .and_then(Value::as_u64)
                {
                    input.insert("numReplicas".to_string(), Value::Number(replicas.into()));
                }
            }
            variables.insert("input".to_string(), Value::Object(input));
            (
                "mutation serviceInstanceUpdate($serviceId: String!, $environmentId: String!, $input: ServiceInstanceUpdateInput!) { serviceInstanceUpdate(serviceId: $serviceId, environmentId: $environmentId, input: $input) }",
                Value::Object(variables),
            )
        }
        "serviceDelete" => {
            let id = identifier(identifiers, "serviceId");
            (
                "mutation serviceDelete($id: String!) { serviceDelete(id: $id) }",
                serde_json::json!({ "id": id }),
            )
        }
        "customDomainCreate" => {
            let mut input = Map::new();
            for key in ["projectId", "environmentId", "serviceId", "domain"] {
                copy_identifier(identifiers, key, &mut input, key);
            }
            (
                "mutation customDomainCreate($input: CustomDomainCreateInput!) { customDomainCreate(input: $input) { id domain status { dnsRecords { hostlabel requiredValue } } } }",
                serde_json::json!({ "input": input }),
            )
        }
        "customDomainUpdate" => (
            "mutation customDomainUpdate($id: String!, $input: CustomDomainUpdateInput!) { customDomainUpdate(id: $id, input: $input) { id domain } }",
            serde_json::json!({ "id": identifier(identifiers, "id"), "input": {} }),
        ),
        "customDomainDelete" => (
            "mutation customDomainDelete($id: String!) { customDomainDelete(id: $id) }",
            serde_json::json!({ "id": identifier(identifiers, "id") }),
        ),
        "variableCollectionUpsert" => {
            let mut variables = Map::new();
            for key in ["projectId", "environmentId", "serviceId"] {
                copy_identifier(identifiers, key, &mut variables, key);
            }
            let values = resource
                .and_then(|resource| resource.properties.get("environment"))
                .cloned()
                .or_else(|| {
                    let properties = resource?.properties.as_object()?;
                    let name = properties.get("name")?.as_str()?;
                    let value = properties.get("value")?.clone();
                    Some(Value::Object(
                        [(name.to_string(), value)].into_iter().collect(),
                    ))
                })
                .unwrap_or_else(|| Value::Object(Map::new()));
            variables.insert("variables".to_string(), values);
            (
                "mutation variableCollectionUpsert($projectId: String!, $environmentId: String!, $serviceId: String, $variables: EnvironmentVariables!) { variableCollectionUpsert(input: { projectId: $projectId, environmentId: $environmentId, serviceId: $serviceId, variables: $variables }) }",
                Value::Object(variables),
            )
        }
        "variableDelete" => {
            let mut input = Map::new();
            for key in ["projectId", "environmentId", "serviceId", "name"] {
                copy_identifier(identifiers, key, &mut input, key);
            }
            (
                "mutation variableDelete($input: VariableDeleteInput!) { variableDelete(input: $input) }",
                serde_json::json!({ "input": input }),
            )
        }
        _ => return Some(serde_json::json!({ "query": operation_name, "variables": identifiers })),
    };
    Some(serde_json::json!({ "query": query, "variables": variables }))
}

fn service_name(manifest: &Manifest, resource: &Resource) -> String {
    let suffix = resource
        .id
        .split(':')
        .next_back()
        .unwrap_or(resource.id.as_str());
    if suffix == "web" || suffix == manifest.application.name {
        manifest.application.name.clone()
    } else {
        format!("{}-{suffix}", manifest.application.name)
    }
}

fn provider_properties(resource: &Resource) -> Value {
    let mut properties = resource.properties.clone();
    if let Some(properties) = properties.as_object_mut() {
        properties.remove("provider_config");
    }
    properties
}

fn copy_nested_string(
    resource: &Resource,
    path: &[&str],
    target: &mut Map<String, Value>,
    target_key: &str,
) {
    let mut value = &resource.properties;
    for key in path {
        let Some(next) = value.get(*key) else {
            return;
        };
        value = next;
    }
    if let Some(value) = value.as_str() {
        target.insert(target_key.to_string(), Value::String(value.to_string()));
    }
}

fn copy_property(
    resource: &Resource,
    source_key: &str,
    target: &mut Map<String, Value>,
    target_key: &str,
) {
    if let Some(value) = resource.properties.get(source_key) {
        if !value.is_null() {
            target.insert(target_key.to_string(), value.clone());
        }
    }
}

fn copy_identifier(
    identifiers: &HashMap<String, String>,
    source_key: &str,
    target: &mut Map<String, Value>,
    target_key: &str,
) {
    if let Some(value) = identifiers.get(source_key) {
        target.insert(target_key.to_string(), Value::String(value.clone()));
    }
}

fn identifier(identifiers: &HashMap<String, String>, key: &str) -> Value {
    identifiers
        .get(key)
        .cloned()
        .map(Value::String)
        .unwrap_or(Value::Null)
}

fn secret_references(manifest: &Manifest, body: Option<&Value>) -> HashMap<String, String> {
    let mut names = Vec::new();
    if let Some(body) = body {
        collect_secret_names(body, &mut names);
    }
    names
        .into_iter()
        .filter_map(|name| {
            manifest
                .variables
                .get(&name)
                .and_then(|variable| variable.secret_ref.clone())
                .map(|reference| (name, reference))
        })
        .collect()
}

fn collect_secret_names(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::String(value) => {
            if let Some(name) = secret_name(value) {
                names.push(name.to_string());
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_secret_names(value, names);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_secret_names(value, names);
            }
        }
        _ => {}
    }
}

fn secret_name(value: &str) -> Option<&str> {
    value
        .strip_prefix("${secret:")
        .and_then(|value| value.strip_suffix('}'))
}

fn blocked_result(request: &ProviderRequest, message: String) -> ProviderRequestResult {
    ProviderRequestResult {
        request_id: request.id.clone(),
        resource_id: request.resource_id.clone(),
        status: ProviderRequestStatus::Blocked,
        status_code: None,
        provider_id: None,
        identifiers: HashMap::new(),
        observed: None,
        message,
    }
}
