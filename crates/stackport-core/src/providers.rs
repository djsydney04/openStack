use crate::manifest::{Capability, Manifest, ResourceKind};
use crate::planner::{MigrationPlan, PlanAction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDefinition {
    pub name: String,
    pub display_name: String,
    pub auth_methods: Vec<AuthMethod>,
    pub capabilities: Vec<Capability>,
    pub resources: Vec<ProviderResourceMapping>,
    pub operations: Vec<ProviderOperation>,
    pub api: ProviderApiSpec,
    pub state: ProviderStatePolicy,
    pub secrets: SecretPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    ApiToken { env_var: String },
    OAuth,
    CliSession { command: String },
    ServiceAccount { env_var: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderResourceMapping {
    pub stack_kind: ResourceKind,
    pub provider_resource: String,
    pub lifecycle: ResourceLifecycle,
    pub importable: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceLifecycle {
    pub read: bool,
    pub plan: bool,
    pub create: bool,
    pub update: bool,
    pub delete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOperation {
    Read,
    Import,
    Plan,
    ApplyCreate,
    ApplyUpdate,
    ApplyDelete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderStatePolicy {
    pub id_format: String,
    pub stores_last_applied_fingerprint: bool,
    pub stores_secret_values: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretPolicy {
    pub supports_secret_references: bool,
    pub provider_secret_resource: String,
    pub stores_plaintext_in_state: bool,
    pub allowed_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderApiSpec {
    pub protocol: ApiProtocol,
    pub base_url: String,
    pub docs_url: String,
    pub auth_headers: Vec<String>,
    pub operations: Vec<ProviderApiOperationSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiProtocol {
    Rest,
    Graphql,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderApiOperationSpec {
    pub provider_resource: String,
    pub operation: ProviderOperation,
    pub method: HttpMethod,
    pub path: String,
    pub graphql_operation: Option<String>,
    pub required_identifiers: Vec<String>,
    pub body_policy: String,
    pub secret_handling: String,
    pub docs_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderExecutionPlan {
    pub providers: Vec<ProviderDefinition>,
    pub steps: Vec<ProviderExecutionStep>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderExecutionStep {
    pub resource_id: String,
    pub provider: String,
    pub provider_resource: Option<String>,
    pub action: PlanAction,
    pub auth_methods: Vec<AuthMethod>,
    pub lifecycle: Option<ResourceLifecycle>,
    pub api_operation: Option<ProviderApiOperationSpec>,
    pub state_id_format: Option<String>,
    pub secret_policy: Option<SecretPolicy>,
    pub notes: Vec<String>,
}

pub fn provider_registry() -> Vec<ProviderDefinition> {
    vec![vercel(), supabase(), neon(), railway()]
}

pub fn provider_definition(name: &str) -> Option<ProviderDefinition> {
    provider_registry()
        .into_iter()
        .find(|provider| provider.name == name)
}

pub fn provider_execution_plan(manifest: &Manifest, plan: &MigrationPlan) -> ProviderExecutionPlan {
    let registry = provider_registry()
        .into_iter()
        .map(|provider| (provider.name.clone(), provider))
        .collect::<HashMap<_, _>>();
    let resources = manifest
        .resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<HashMap<_, _>>();
    let mut used_providers = HashMap::<String, ProviderDefinition>::new();
    let mut warnings = Vec::new();
    let mut steps = Vec::new();

    for step in &plan.steps {
        let resource = resources.get(step.resource_id.as_str()).copied();
        let provider_name = step
            .provider
            .clone()
            .or_else(|| resource.and_then(|resource| resource.provider.clone()))
            .unwrap_or_else(|| plan.target_provider.clone());
        let definition = registry.get(&provider_name);
        if let Some(definition) = definition {
            used_providers.insert(provider_name.clone(), definition.clone());
        } else {
            warnings.push(format!("provider `{provider_name}` is not registered"));
        }

        let mapping = definition.and_then(|definition| match resource {
            Some(resource) => definition
                .resources
                .iter()
                .find(|mapping| mapping.stack_kind == resource.kind),
            None => step
                .provider_resource
                .as_ref()
                .and_then(|provider_resource| {
                    definition
                        .resources
                        .iter()
                        .find(|mapping| &mapping.provider_resource == provider_resource)
                }),
        });
        let provider_resource = mapping
            .map(|mapping| mapping.provider_resource.clone())
            .or_else(|| step.provider_resource.clone());
        let api_operation = definition.and_then(|definition| {
            provider_resource.as_ref().and_then(|provider_resource| {
                definition.api.operations.iter().find(|operation| {
                    operation.provider_resource == *provider_resource
                        && operation.operation == plan_action_operation(&step.action)
                })
            })
        });

        if definition.is_some() && provider_resource.is_none() {
            let detail = resource
                .map(|resource| format!(" of kind `{:?}`", resource.kind))
                .unwrap_or_default();
            warnings.push(format!(
                "provider `{provider_name}` has no mapping for resource `{}`{detail}",
                step.resource_id
            ));
        }

        steps.push(ProviderExecutionStep {
            resource_id: step.resource_id.clone(),
            provider: provider_name,
            provider_resource,
            action: step.action.clone(),
            auth_methods: definition
                .map(|definition| definition.auth_methods.clone())
                .unwrap_or_default(),
            lifecycle: mapping.map(|mapping| mapping.lifecycle.clone()),
            api_operation: api_operation.cloned(),
            state_id_format: definition.map(|definition| definition.state.id_format.clone()),
            secret_policy: definition.map(|definition| definition.secrets.clone()),
            notes: mapping
                .map(|mapping| mapping.notes.clone())
                .unwrap_or_default(),
        });
    }

    ProviderExecutionPlan {
        providers: used_providers.into_values().collect(),
        steps,
        warnings,
    }
}

fn vercel() -> ProviderDefinition {
    ProviderDefinition {
        name: "vercel".to_string(),
        display_name: "Vercel".to_string(),
        auth_methods: vec![
            AuthMethod::ApiToken {
                env_var: "VERCEL_TOKEN".to_string(),
            },
            AuthMethod::CliSession {
                command: "vercel whoami".to_string(),
            },
        ],
        capabilities: vec![
            Capability::Build,
            Capability::ServerlessFunctions,
            Capability::EdgeFunctions,
            Capability::Secrets,
            Capability::CustomDomains,
            Capability::Cron,
        ],
        resources: vec![
            mapping(
                ResourceKind::Project,
                "project",
                lifecycle(true, true, true, true, true),
                true,
                &["maps a neutral project to a Vercel project"],
            ),
            mapping(
                ResourceKind::WebService,
                "project",
                lifecycle(true, true, true, true, true),
                true,
                &["maps service source/build/env to a Vercel project"],
            ),
            mapping(
                ResourceKind::Function,
                "project",
                lifecycle(true, true, true, true, true),
                true,
                &["functions are deployed through the project build output"],
            ),
            mapping(
                ResourceKind::Build,
                "deployment",
                lifecycle(true, true, true, false, true),
                true,
                &["maps an explicit build to an immutable Vercel deployment"],
            ),
            mapping(
                ResourceKind::DeployHook,
                "deploy-hook",
                lifecycle(false, true, false, false, false),
                false,
                &["Vercel exposes hook triggering but not public hook lifecycle management"],
            ),
            mapping(
                ResourceKind::Secret,
                "environment-variable",
                lifecycle(true, true, true, true, true),
                true,
                &["stores provider secret names and targets, never secret values"],
            ),
            mapping(
                ResourceKind::Domain,
                "domain",
                lifecycle(true, true, true, true, true),
                true,
                &["maps domain resources to Vercel project domains"],
            ),
        ],
        operations: standard_operations(),
        api: ProviderApiSpec {
            protocol: ApiProtocol::Rest,
            base_url: "https://api.vercel.com".to_string(),
            docs_url: "https://vercel.com/docs/rest-api".to_string(),
            auth_headers: vec!["Authorization: Bearer ${VERCEL_TOKEN}".to_string()],
            operations: vec![
                rest_op(
                    "project",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v9/projects/{idOrName}",
                    &["idOrName"],
                    "read project settings, deployments, and linked repository metadata",
                    "never request decrypted environment variable values during read",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "project",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v9/projects",
                    &[],
                    "list projects and select by configured app name or project id",
                    "import environment variable names only, not decrypted values",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "project",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/v9/projects",
                    &[],
                    "create project from service source/build configuration",
                    "write env vars by secret reference names only",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "project",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Patch,
                    "/v9/projects/{idOrName}",
                    &["idOrName"],
                    "patch project settings derived from service build/deploy config",
                    "do not include secret values in state or plan output",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "project",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/v9/projects/{idOrName}",
                    &["idOrName"],
                    "delete project when destroy is explicitly requested",
                    "delete provider-side env vars with project deletion",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "environment-variable",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v9/projects/{idOrName}/env",
                    &["idOrName"],
                    "list project environment variables",
                    "do not call decrypted-value endpoint from read/import",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "environment-variable",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v9/projects/{idOrName}/env",
                    &["idOrName"],
                    "import environment variable names and targets",
                    "never request decrypted environment variable values",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "environment-variable",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/v10/projects/{idOrName}/env",
                    &["idOrName"],
                    "create project env vars for selected targets",
                    "secret values must be supplied from runtime secret resolution only",
                    "https://vercel.com/docs/environment-variables/sensitive-environment-variables",
                ),
                rest_op(
                    "environment-variable",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Patch,
                    "/v9/projects/{idOrName}/env/{envId}",
                    &["idOrName", "envId"],
                    "edit existing project env var metadata/value",
                    "secret value is write-only and not persisted in state",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "environment-variable",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/v9/projects/{idOrName}/env/{envId}",
                    &["idOrName", "envId"],
                    "remove project env var by id",
                    "state keeps provider env id and name only",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "domain",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v9/projects/{idOrName}/domains/{domain}",
                    &["idOrName", "domain"],
                    "read project domain verification status",
                    "no secret data involved",
                    "https://vercel.com/docs/rest-api/reference/endpoints/projects/verify-project-domain",
                ),
                rest_op(
                    "domain",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v9/projects/{idOrName}/domains",
                    &["idOrName"],
                    "list project domains for import",
                    "no secret data involved",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "domain",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/v9/projects/{idOrName}/domains",
                    &["idOrName"],
                    "attach custom domain to project",
                    "no secret data involved",
                    "https://vercel.com/docs/integrations/create-integration/vercel-api-integrations",
                ),
                rest_op(
                    "domain",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Post,
                    "/v9/projects/{idOrName}/domains/{domain}/verify",
                    &["idOrName", "domain"],
                    "verify project domain after DNS is configured",
                    "no secret data involved",
                    "https://vercel.com/docs/rest-api/reference/endpoints/projects/verify-project-domain",
                ),
                rest_op(
                    "domain",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/v9/projects/{idOrName}/domains/{domain}",
                    &["idOrName", "domain"],
                    "remove domain from project",
                    "no secret data involved",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "deployment",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v13/deployments/{idOrUrl}",
                    &["idOrUrl"],
                    "read immutable deployment/build status and metadata",
                    "do not include build-time secret values in output",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "deployment",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v6/deployments",
                    &["projectId"],
                    "list deployments for a project",
                    "deployment environment values are not imported",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "deployment",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/v13/deployments",
                    &[],
                    "create a deployment from Git source, files, or build output",
                    "build environment secret values resolve only at request time",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "deployment",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/v13/deployments/{id}",
                    &["id"],
                    "delete an immutable deployment by id",
                    "no secret data involved",
                    "https://vercel.com/docs/rest-api",
                ),
                rest_op(
                    "project",
                    ProviderOperation::Plan,
                    HttpMethod::Get,
                    "/v9/projects/{idOrName}",
                    &["idOrName"],
                    "compare desired service settings with current project settings",
                    "secret comparison uses names/fingerprints only",
                    "https://vercel.com/docs/rest-api",
                ),
            ],
        },
        state: state_policy("vercel:{project_id}:{resource_id}"),
        secrets: secret_policy("environment-variable", &["env:VERCEL_TOKEN", "vercel:*"]),
    }
}

fn supabase() -> ProviderDefinition {
    ProviderDefinition {
        name: "supabase".to_string(),
        display_name: "Supabase".to_string(),
        auth_methods: vec![
            AuthMethod::ApiToken {
                env_var: "SUPABASE_ACCESS_TOKEN".to_string(),
            },
            AuthMethod::CliSession {
                command: "supabase projects list".to_string(),
            },
        ],
        capabilities: vec![
            Capability::Postgres,
            Capability::Auth,
            Capability::ObjectStorage,
            Capability::EdgeFunctions,
            Capability::Secrets,
        ],
        resources: vec![
            mapping(
                ResourceKind::Project,
                "project-database",
                lifecycle(true, true, true, true, true),
                true,
                &["a Supabase project owns its Postgres, Auth, Storage, and Functions"],
            ),
            mapping(
                ResourceKind::Database,
                "project-database",
                lifecycle(true, true, true, true, true),
                true,
                &["database exists inside a Supabase project"],
            ),
            mapping(
                ResourceKind::Auth,
                "auth-config",
                lifecycle(true, true, true, true, false),
                true,
                &["auth is configured on an existing Supabase project"],
            ),
            mapping(
                ResourceKind::StorageBucket,
                "storage-bucket",
                lifecycle(true, true, true, true, true),
                true,
                &["maps bucket names to Supabase Storage buckets"],
            ),
            mapping(
                ResourceKind::Function,
                "edge-function",
                lifecycle(true, true, true, true, true),
                true,
                &["maps stack functions to Supabase Edge Functions"],
            ),
            mapping(
                ResourceKind::Secret,
                "project-secret",
                lifecycle(true, true, true, true, true),
                true,
                &["secret values are written to Supabase only at apply time"],
            ),
        ],
        operations: standard_operations(),
        api: ProviderApiSpec {
            protocol: ApiProtocol::Rest,
            base_url: "https://api.supabase.com".to_string(),
            docs_url: "https://supabase.com/docs/reference/api/introduction".to_string(),
            auth_headers: vec!["Authorization: Bearer ${SUPABASE_ACCESS_TOKEN}".to_string()],
            operations: vec![
                rest_op(
                    "project-database",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v1/projects/{ref}",
                    &["ref"],
                    "read project metadata and Postgres version/config",
                    "do not request database password or revealed API keys",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "project-database",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v1/projects",
                    &[],
                    "list projects and map refs to database resources",
                    "import project refs and names only",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "project-database",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/v1/projects",
                    &["organization_slug"],
                    "create a project/database in an organization",
                    "database password must be supplied at apply time and never stored",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "project-database",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Patch,
                    "/v1/projects/{ref}",
                    &["ref"],
                    "update mutable project settings",
                    "no secret values in state",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "project-database",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/v1/projects/{ref}",
                    &["ref"],
                    "delete project when destroy is explicitly requested",
                    "provider deletes project-scoped secrets",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "auth-config",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v1/projects/{ref}/config/auth",
                    &["ref"],
                    "read project Auth configuration",
                    "redact SMTP passwords, OAuth client secrets, and signing secrets",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "auth-config",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v1/projects/{ref}/config/auth",
                    &["ref"],
                    "import non-secret Auth configuration",
                    "redact SMTP passwords, OAuth client secrets, and signing secrets",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "auth-config",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Patch,
                    "/v1/projects/{ref}/config/auth",
                    &["ref"],
                    "configure Auth on an existing project",
                    "resolve provider credentials and SMTP secrets only at request time",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "auth-config",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Patch,
                    "/v1/projects/{ref}/config/auth",
                    &["ref"],
                    "update project Auth configuration",
                    "never persist returned or submitted secret values",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "storage-bucket",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "{project_url}/storage/v1/bucket",
                    &["project_url"],
                    "list buckets through the project Storage API",
                    "requires service role at apply/read time; never stored",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
                rest_op(
                    "storage-bucket",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "{project_url}/storage/v1/bucket",
                    &["project_url"],
                    "list project buckets for import",
                    "requires service role at import time; never stored",
                    "https://supabase.com/docs/reference/javascript/storage-listbuckets",
                ),
                rest_op(
                    "storage-bucket",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "{project_url}/storage/v1/bucket",
                    &["project_url"],
                    "create a project Storage bucket",
                    "service-role credentials are read from the environment and never stored",
                    "https://supabase.com/docs/reference/javascript/storage-createbucket",
                ),
                rest_op(
                    "storage-bucket",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Put,
                    "{project_url}/storage/v1/bucket/{bucket_id}",
                    &["project_url", "bucket_id"],
                    "update bucket visibility and file restrictions",
                    "service-role credentials are read from the environment and never stored",
                    "https://supabase.com/docs/reference/javascript/storage-updatebucket",
                ),
                rest_op(
                    "storage-bucket",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "{project_url}/storage/v1/bucket/{bucket_id}",
                    &["project_url", "bucket_id"],
                    "delete an empty Storage bucket",
                    "service-role credentials are read from the environment and never stored",
                    "https://supabase.com/docs/reference/javascript/storage-deletebucket",
                ),
                rest_op(
                    "project-secret",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v1/projects/{ref}/secrets",
                    &["ref"],
                    "list Edge Function secret names",
                    "do not persist returned values; store names only",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "project-secret",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v1/projects/{ref}/secrets",
                    &["ref"],
                    "import Edge Function secret names",
                    "discard any returned values and preserve names only",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "project-secret",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Post,
                    "/v1/projects/{ref}/secrets",
                    &["ref"],
                    "upsert project secrets by name",
                    "secret values are write-only runtime inputs",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "project-secret",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/v1/projects/{ref}/secrets",
                    &["ref"],
                    "bulk create project secrets",
                    "secret values are write-only runtime inputs",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "project-secret",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/v1/projects/{ref}/secrets",
                    &["ref"],
                    "bulk delete project secrets by name",
                    "state stores secret names only",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "edge-function",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/v1/projects/{ref}/functions",
                    &["ref"],
                    "list deployed Edge Functions",
                    "function secrets are separate project-secret resources",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "edge-function",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/v1/projects/{ref}/functions",
                    &["ref"],
                    "list deployed Edge Functions for import",
                    "function secrets remain separate references",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "edge-function",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/v1/projects/{ref}/functions/deploy",
                    &["ref"],
                    "deploy or update an Edge Function bundle",
                    "function env uses project secrets by reference",
                    "https://supabase.com/changelog/33720-deploy-and-update-edge-functions-using-the-management-api",
                ),
                rest_op(
                    "edge-function",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Post,
                    "/v1/projects/{ref}/functions/deploy",
                    &["ref"],
                    "deploy a new Edge Function version",
                    "function environment uses project secret references",
                    "https://supabase.com/changelog/33720-deploy-and-update-edge-functions-using-the-management-api",
                ),
                rest_op(
                    "edge-function",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/v1/projects/{ref}/functions/{function_slug}",
                    &["ref", "function_slug"],
                    "delete Edge Function by slug",
                    "function secrets are deleted separately by name",
                    "https://supabase.com/docs/reference/api/introduction",
                ),
                rest_op(
                    "project-database",
                    ProviderOperation::Plan,
                    HttpMethod::Get,
                    "/v1/projects/{ref}",
                    &["ref"],
                    "compare desired project/database settings with current project",
                    "secret comparison uses references only",
                    "https://supabase.com/docs/reference/api/getting-started",
                ),
            ],
        },
        state: state_policy("supabase:{project_ref}:{resource_id}"),
        secrets: secret_policy(
            "project-secret",
            &["env:SUPABASE_ACCESS_TOKEN", "supabase:*"],
        ),
    }
}

fn neon() -> ProviderDefinition {
    ProviderDefinition {
        name: "neon".to_string(),
        display_name: "Neon".to_string(),
        auth_methods: vec![AuthMethod::ApiToken {
            env_var: "NEON_API_KEY".to_string(),
        }],
        capabilities: vec![Capability::Postgres, Capability::Secrets],
        resources: vec![
            mapping(
                ResourceKind::Project,
                "project-branch-database",
                lifecycle(true, true, true, true, true),
                true,
                &["maps a neutral project to a Neon project with its default branch"],
            ),
            mapping(
                ResourceKind::Database,
                "project-branch-database",
                lifecycle(true, true, true, true, true),
                true,
                &["maps database resources to Neon project/branch/database objects"],
            ),
            mapping(
                ResourceKind::DatabaseBranch,
                "branch",
                lifecycle(true, true, true, true, true),
                true,
                &["maps a database branch to a Neon copy-on-write branch"],
            ),
            mapping(
                ResourceKind::DatabaseRole,
                "role",
                lifecycle(true, true, true, false, true),
                true,
                &["maps a database role to a role inside a Neon branch"],
            ),
            mapping(
                ResourceKind::ConnectionString,
                "connection-string-reference",
                lifecycle(true, true, false, false, false),
                true,
                &["connection strings are exposed as references, not stored values"],
            ),
            mapping(
                ResourceKind::Secret,
                "connection-string-reference",
                lifecycle(true, true, false, false, false),
                true,
                &["connection strings are exposed as references, not stored values"],
            ),
        ],
        operations: standard_operations(),
        api: ProviderApiSpec {
            protocol: ApiProtocol::Rest,
            base_url: "https://console.neon.tech/api/v2".to_string(),
            docs_url: "https://api-docs.neon.tech/reference/getting-started-with-neon-api"
                .to_string(),
            auth_headers: vec!["Authorization: Bearer ${NEON_API_KEY}".to_string()],
            operations: vec![
                rest_op(
                    "project-branch-database",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/projects/{project_id}/branches/{branch_id}",
                    &["project_id", "branch_id"],
                    "read branch with databases, roles, and compute endpoint ids",
                    "do not request connection URI during generic read",
                    "https://api-docs.neon.tech/reference/getting-started-with-neon-api",
                ),
                rest_op(
                    "project-branch-database",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/projects",
                    &[],
                    "list projects, then list branches/databases/roles as needed",
                    "connection URI is imported as a secret reference only",
                    "https://api-docs.neon.tech/reference/path-parameters",
                ),
                rest_op(
                    "project-branch-database",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/projects",
                    &[],
                    "create Neon project; follow with branch/database/role creation when needed",
                    "response connection URI must be converted to a secret ref and discarded",
                    "https://api-docs.neon.tech/reference/createproject",
                ),
                rest_op(
                    "project-branch-database",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Patch,
                    "/projects/{project_id}/branches/{branch_id}",
                    &["project_id", "branch_id"],
                    "update branch/database metadata where supported",
                    "no connection strings in state",
                    "https://api-docs.neon.tech/reference/getting-started-with-neon-api",
                ),
                rest_op(
                    "project-branch-database",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/projects/{project_id}",
                    &["project_id"],
                    "delete Neon project when destroy is explicitly requested",
                    "provider deletes associated credentials",
                    "https://api-docs.neon.tech/reference/use-cases",
                ),
                rest_op(
                    "branch",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/projects/{project_id}/branches/{branch_id}",
                    &["project_id", "branch_id"],
                    "read a Neon branch and its endpoint metadata",
                    "no connection URI is returned",
                    "https://api-docs.neon.tech/reference/createprojectbranch",
                ),
                rest_op(
                    "branch",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/projects/{project_id}/branches",
                    &["project_id"],
                    "list project branches for import",
                    "import identifiers and branch metadata only",
                    "https://api-docs.neon.tech/reference/path-parameters",
                ),
                rest_op(
                    "branch",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/projects/{project_id}/branches",
                    &["project_id"],
                    "create a copy-on-write database branch",
                    "role passwords and connection URIs are not persisted",
                    "https://api-docs.neon.tech/reference/createprojectbranch",
                ),
                rest_op(
                    "branch",
                    ProviderOperation::ApplyUpdate,
                    HttpMethod::Patch,
                    "/projects/{project_id}/branches/{branch_id}",
                    &["project_id", "branch_id"],
                    "update branch name or protection metadata",
                    "no secret data involved",
                    "https://api-docs.neon.tech/reference/getting-started-with-neon-api",
                ),
                rest_op(
                    "branch",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/projects/{project_id}/branches/{branch_id}",
                    &["project_id", "branch_id"],
                    "delete a database branch",
                    "provider invalidates branch-scoped credentials",
                    "https://api-docs.neon.tech/reference/getting-started-with-neon-api",
                ),
                rest_op(
                    "role",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/projects/{project_id}/branches/{branch_id}/roles",
                    &["project_id", "branch_id"],
                    "list branch roles",
                    "role passwords are never returned",
                    "https://api-docs.neon.tech/reference/listprojectbranchroles",
                ),
                rest_op(
                    "role",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/projects/{project_id}/branches/{branch_id}/roles",
                    &["project_id", "branch_id"],
                    "import role names",
                    "role passwords are never imported",
                    "https://api-docs.neon.tech/reference/listprojectbranchroles",
                ),
                rest_op(
                    "role",
                    ProviderOperation::ApplyCreate,
                    HttpMethod::Post,
                    "/projects/{project_id}/branches/{branch_id}/roles",
                    &["project_id", "branch_id"],
                    "create a role in a branch",
                    "returned password is runtime-only and never enters state",
                    "https://api-docs.neon.tech/reference/createprojectbranchrole",
                ),
                rest_op(
                    "role",
                    ProviderOperation::ApplyDelete,
                    HttpMethod::Delete,
                    "/projects/{project_id}/branches/{branch_id}/roles/{role_name}",
                    &["project_id", "branch_id", "role_name"],
                    "delete a branch role",
                    "provider invalidates the role password",
                    "https://api-docs.neon.tech/reference/getting-started-with-neon-api",
                ),
                rest_op(
                    "connection-string-reference",
                    ProviderOperation::Read,
                    HttpMethod::Get,
                    "/projects/{project_id}/connection_uri",
                    &["project_id", "database_name", "role_name"],
                    "retrieve URI only when resolving a secret for dependent resources",
                    "return connection URI to secret sink, never state or plan output",
                    "https://api-docs.neon.tech/reference/getconnectionuri",
                ),
                rest_op(
                    "connection-string-reference",
                    ProviderOperation::Import,
                    HttpMethod::Get,
                    "/projects/{project_id}/branches/{branch_id}",
                    &["project_id", "branch_id"],
                    "verify the database and role used by a connection reference",
                    "never retrieve or persist the connection URI during import",
                    "https://api-docs.neon.tech/reference/path-parameters",
                ),
                rest_op(
                    "connection-string-reference",
                    ProviderOperation::Plan,
                    HttpMethod::Get,
                    "/projects/{project_id}/branches/{branch_id}",
                    &["project_id", "branch_id"],
                    "verify referenced database and role exist",
                    "no connection strings in plan output",
                    "https://api-docs.neon.tech/reference/path-parameters",
                ),
            ],
        },
        state: state_policy("neon:{project_id}:{branch_id}:{database_name}"),
        secrets: secret_policy(
            "connection-string-reference",
            &["env:NEON_API_KEY", "neon:*"],
        ),
    }
}

fn railway() -> ProviderDefinition {
    ProviderDefinition {
        name: "railway".to_string(),
        display_name: "Railway".to_string(),
        auth_methods: vec![
            AuthMethod::ApiToken {
                env_var: "RAILWAY_TOKEN".to_string(),
            },
            AuthMethod::CliSession {
                command: "railway whoami".to_string(),
            },
        ],
        capabilities: vec![
            Capability::Build,
            Capability::Postgres,
            Capability::Secrets,
            Capability::CustomDomains,
            Capability::Cron,
        ],
        resources: vec![
            mapping(
                ResourceKind::Project,
                "project",
                lifecycle(true, true, true, true, true),
                true,
                &["maps a neutral project to a Railway project"],
            ),
            mapping(
                ResourceKind::Environment,
                "environment",
                lifecycle(true, true, true, true, true),
                true,
                &["maps a neutral environment to a Railway environment"],
            ),
            mapping(
                ResourceKind::WebService,
                "service",
                lifecycle(true, true, true, true, true),
                true,
                &["maps web services to Railway services within a project/environment"],
            ),
            mapping(
                ResourceKind::Database,
                "postgres-plugin",
                lifecycle(false, true, false, false, false),
                false,
                &[
                    "Railway Postgres template deployment is not yet an executable lifecycle; use Neon or Supabase for managed database IaC",
                ],
            ),
            mapping(
                ResourceKind::Secret,
                "variable",
                lifecycle(true, true, true, true, true),
                true,
                &["variables are addressed by name and scoped to an environment"],
            ),
            mapping(
                ResourceKind::Domain,
                "custom-domain",
                lifecycle(true, true, true, true, true),
                true,
                &["maps domain resources to Railway custom domains"],
            ),
        ],
        operations: standard_operations(),
        api: ProviderApiSpec {
            protocol: ApiProtocol::Graphql,
            base_url: "https://backboard.railway.com/graphql/v2".to_string(),
            docs_url: "https://docs.railway.com/integrations/api".to_string(),
            auth_headers: vec![
                "Authorization: Bearer ${RAILWAY_TOKEN}".to_string(),
                "Project-Access-Token: ${RAILWAY_PROJECT_TOKEN}".to_string(),
            ],
            operations: vec![
                graphql_op(
                    "project",
                    ProviderOperation::Read,
                    "project",
                    &["projectId"],
                    "read project services and environments",
                    "variables are read separately as unrendered references",
                    "https://docs.railway.com/integrations/api/manage-projects",
                ),
                graphql_op(
                    "project",
                    ProviderOperation::Import,
                    "projects",
                    &[],
                    "list projects available to the authenticated account or workspace",
                    "do not import rendered variable values",
                    "https://docs.railway.com/integrations/api/manage-projects",
                ),
                graphql_op(
                    "project",
                    ProviderOperation::ApplyCreate,
                    "projectCreate",
                    &[],
                    "create an empty Railway project",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-projects",
                ),
                graphql_op(
                    "project",
                    ProviderOperation::ApplyUpdate,
                    "projectUpdate",
                    &["projectId"],
                    "update project name or description",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-projects",
                ),
                graphql_op(
                    "project",
                    ProviderOperation::ApplyDelete,
                    "projectDelete",
                    &["projectId"],
                    "delete a project only after explicit apply approval",
                    "provider deletes project-scoped variables",
                    "https://docs.railway.com/integrations/api/manage-projects",
                ),
                graphql_op(
                    "environment",
                    ProviderOperation::Read,
                    "environment",
                    &["environmentId"],
                    "read an environment with its service instances",
                    "staged variables are read as references",
                    "https://docs.railway.com/integrations/api/manage-environments",
                ),
                graphql_op(
                    "environment",
                    ProviderOperation::Import,
                    "environments",
                    &["projectId"],
                    "list environments in a project",
                    "do not import rendered variable values",
                    "https://docs.railway.com/integrations/api/manage-environments",
                ),
                graphql_op(
                    "environment",
                    ProviderOperation::ApplyCreate,
                    "environmentCreate",
                    &["projectId"],
                    "create a Railway environment",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-environments",
                ),
                graphql_op(
                    "environment",
                    ProviderOperation::ApplyUpdate,
                    "environmentUpdate",
                    &["environmentId"],
                    "rename or configure an environment",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-environments",
                ),
                graphql_op(
                    "environment",
                    ProviderOperation::ApplyDelete,
                    "environmentDelete",
                    &["environmentId"],
                    "delete an environment and its deployments after explicit approval",
                    "provider deletes environment-scoped variables",
                    "https://docs.railway.com/integrations/api/manage-environments",
                ),
                graphql_op(
                    "service",
                    ProviderOperation::Read,
                    "project",
                    &["projectId"],
                    "query project with services and environments",
                    "variables are fetched through separate unrendered variable query",
                    "https://docs.railway.com/integrations/api/api-cookbook",
                ),
                graphql_op(
                    "service",
                    ProviderOperation::Import,
                    "projects",
                    &[],
                    "list projects and services for import",
                    "import variable names/references only",
                    "https://docs.railway.com/integrations/api/api-cookbook",
                ),
                graphql_op(
                    "service",
                    ProviderOperation::ApplyCreate,
                    "serviceCreate",
                    &["projectId"],
                    "create service from GitHub repo or image source",
                    "service variables are applied through variableCollectionUpsert",
                    "https://docs.railway.com/integrations/api/api-cookbook",
                ),
                graphql_op(
                    "service",
                    ProviderOperation::ApplyUpdate,
                    "serviceInstanceUpdate",
                    &["serviceId", "environmentId"],
                    "update service instance settings such as start command",
                    "secrets remain variable references",
                    "https://docs.railway.com/integrations/api/api-cookbook",
                ),
                graphql_op(
                    "service",
                    ProviderOperation::ApplyDelete,
                    "serviceDelete",
                    &["serviceId"],
                    "delete service when destroy is explicitly requested",
                    "provider deletes service-scoped variables",
                    "https://docs.railway.com/integrations/api",
                ),
                graphql_op(
                    "variable",
                    ProviderOperation::Read,
                    "variables",
                    &["projectId", "environmentId", "serviceId"],
                    "fetch variables with unrendered=true when available",
                    "prefer unrendered references like ${{Postgres.DATABASE_URL}}",
                    "https://docs.railway.com/integrations/api/manage-variables",
                ),
                graphql_op(
                    "variable",
                    ProviderOperation::Import,
                    "project",
                    &["projectId"],
                    "discover service variable names and unrendered references from a project",
                    "do not import rendered variable values",
                    "https://docs.railway.com/integrations/api/manage-variables",
                ),
                graphql_op(
                    "variable",
                    ProviderOperation::ApplyCreate,
                    "variableCollectionUpsert",
                    &["projectId", "environmentId", "serviceId"],
                    "upsert service or environment variables",
                    "secret values come from runtime secret resolution only",
                    "https://docs.railway.com/integrations/api/manage-variables",
                ),
                graphql_op(
                    "variable",
                    ProviderOperation::ApplyUpdate,
                    "variableCollectionUpsert",
                    &["projectId", "environmentId", "serviceId"],
                    "upsert changed service or environment variables",
                    "secret values come from runtime secret resolution only",
                    "https://docs.railway.com/integrations/api/manage-variables",
                ),
                graphql_op(
                    "variable",
                    ProviderOperation::ApplyDelete,
                    "variableDelete",
                    &["projectId", "environmentId", "serviceId", "name"],
                    "delete variable by name",
                    "state stores variable name only",
                    "https://docs.railway.com/integrations/api/manage-variables",
                ),
                graphql_op(
                    "custom-domain",
                    ProviderOperation::Read,
                    "domains",
                    &["serviceId", "environmentId"],
                    "list domains for service/environment",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-domains",
                ),
                graphql_op(
                    "custom-domain",
                    ProviderOperation::Import,
                    "project",
                    &["projectId"],
                    "discover service custom domains from the project graph",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-domains",
                ),
                graphql_op(
                    "custom-domain",
                    ProviderOperation::ApplyCreate,
                    "customDomainCreate",
                    &["projectId", "environmentId", "serviceId", "domain"],
                    "create custom domain and return DNS verification records",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/api-cookbook",
                ),
                graphql_op(
                    "custom-domain",
                    ProviderOperation::ApplyUpdate,
                    "customDomainUpdate",
                    &["id"],
                    "update custom domain settings when supported",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-domains",
                ),
                graphql_op(
                    "custom-domain",
                    ProviderOperation::ApplyDelete,
                    "customDomainDelete",
                    &["id"],
                    "delete custom domain",
                    "no secret data involved",
                    "https://docs.railway.com/integrations/api/manage-domains",
                ),
                graphql_op(
                    "postgres-plugin",
                    ProviderOperation::ApplyCreate,
                    "serviceCreate",
                    &["projectId"],
                    "create Railway database service from managed Postgres template",
                    "connection variables are referenced by dependent services",
                    "https://docs.railway.com/integrations/api/api-cookbook",
                ),
                graphql_op(
                    "service",
                    ProviderOperation::Plan,
                    "project",
                    &["projectId"],
                    "compare service, environment, variable, deployment, and domain state",
                    "variable comparison uses unrendered references where possible",
                    "https://docs.railway.com/integrations/api/api-cookbook",
                ),
            ],
        },
        state: state_policy("railway:{project_id}:{environment_id}:{service_id}:{resource_id}"),
        secrets: secret_policy("variable", &["env:RAILWAY_TOKEN", "railway:*"]),
    }
}

fn standard_operations() -> Vec<ProviderOperation> {
    vec![
        ProviderOperation::Read,
        ProviderOperation::Import,
        ProviderOperation::Plan,
        ProviderOperation::ApplyCreate,
        ProviderOperation::ApplyUpdate,
        ProviderOperation::ApplyDelete,
    ]
}

fn plan_action_operation(action: &PlanAction) -> ProviderOperation {
    match action {
        PlanAction::Create => ProviderOperation::ApplyCreate,
        PlanAction::Update => ProviderOperation::ApplyUpdate,
        PlanAction::Delete => ProviderOperation::ApplyDelete,
        PlanAction::Replace => ProviderOperation::ApplyCreate,
        PlanAction::Noop | PlanAction::Manual | PlanAction::Unsupported => ProviderOperation::Plan,
    }
}

#[allow(clippy::too_many_arguments)]
fn rest_op(
    provider_resource: &str,
    operation: ProviderOperation,
    method: HttpMethod,
    path: &str,
    required_identifiers: &[&str],
    body_policy: &str,
    secret_handling: &str,
    docs_url: &str,
) -> ProviderApiOperationSpec {
    ProviderApiOperationSpec {
        provider_resource: provider_resource.to_string(),
        operation,
        method,
        path: path.to_string(),
        graphql_operation: None,
        required_identifiers: required_identifiers
            .iter()
            .map(|identifier| identifier.to_string())
            .collect(),
        body_policy: body_policy.to_string(),
        secret_handling: secret_handling.to_string(),
        docs_url: docs_url.to_string(),
    }
}

fn graphql_op(
    provider_resource: &str,
    operation: ProviderOperation,
    graphql_operation: &str,
    required_identifiers: &[&str],
    body_policy: &str,
    secret_handling: &str,
    docs_url: &str,
) -> ProviderApiOperationSpec {
    ProviderApiOperationSpec {
        provider_resource: provider_resource.to_string(),
        operation,
        method: HttpMethod::Post,
        path: "/graphql/v2".to_string(),
        graphql_operation: Some(graphql_operation.to_string()),
        required_identifiers: required_identifiers
            .iter()
            .map(|identifier| identifier.to_string())
            .collect(),
        body_policy: body_policy.to_string(),
        secret_handling: secret_handling.to_string(),
        docs_url: docs_url.to_string(),
    }
}

fn mapping(
    stack_kind: ResourceKind,
    provider_resource: &str,
    lifecycle: ResourceLifecycle,
    importable: bool,
    notes: &[&str],
) -> ProviderResourceMapping {
    ProviderResourceMapping {
        stack_kind,
        provider_resource: provider_resource.to_string(),
        lifecycle,
        importable,
        notes: notes.iter().map(|note| note.to_string()).collect(),
    }
}

fn lifecycle(
    read: bool,
    plan: bool,
    create: bool,
    update: bool,
    delete: bool,
) -> ResourceLifecycle {
    ResourceLifecycle {
        read,
        plan,
        create,
        update,
        delete,
    }
}

fn state_policy(id_format: &str) -> ProviderStatePolicy {
    ProviderStatePolicy {
        id_format: id_format.to_string(),
        stores_last_applied_fingerprint: true,
        stores_secret_values: false,
    }
}

fn secret_policy(provider_secret_resource: &str, allowed_sources: &[&str]) -> SecretPolicy {
    SecretPolicy {
        supports_secret_references: true,
        provider_secret_resource: provider_secret_resource.to_string(),
        stores_plaintext_in_state: false,
        allowed_sources: allowed_sources
            .iter()
            .map(|source| source.to_string())
            .collect(),
    }
}
