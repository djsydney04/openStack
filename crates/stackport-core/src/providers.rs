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
        let Some(resource) = resources.get(step.resource_id.as_str()) else {
            warnings.push(format!(
                "plan step `{}` does not match a manifest resource",
                step.resource_id
            ));
            continue;
        };
        let provider_name = resource
            .provider
            .clone()
            .unwrap_or_else(|| plan.target_provider.clone());
        let definition = registry.get(&provider_name);
        if let Some(definition) = definition {
            used_providers.insert(provider_name.clone(), definition.clone());
        } else {
            warnings.push(format!("provider `{provider_name}` is not registered"));
        }

        let mapping = definition.and_then(|definition| {
            definition
                .resources
                .iter()
                .find(|mapping| mapping.stack_kind == resource.kind)
        });

        if definition.is_some() && mapping.is_none() {
            warnings.push(format!(
                "provider `{provider_name}` has no mapping for resource `{}` of kind `{:?}`",
                resource.id, resource.kind
            ));
        }

        steps.push(ProviderExecutionStep {
            resource_id: resource.id.clone(),
            provider: provider_name,
            provider_resource: mapping.map(|mapping| mapping.provider_resource.clone()),
            action: step.action.clone(),
            auth_methods: definition
                .map(|definition| definition.auth_methods.clone())
                .unwrap_or_default(),
            lifecycle: mapping.map(|mapping| mapping.lifecycle.clone()),
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
                ResourceKind::WebService,
                "project",
                lifecycle(true, true, true, true, true),
                true,
                &["maps service source/build/env to a Vercel project"],
            ),
            mapping(
                ResourceKind::Function,
                "function",
                lifecycle(true, true, true, true, true),
                true,
                &["functions are deployed through the project build output"],
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
                ResourceKind::Database,
                "project-database",
                lifecycle(true, true, true, false, false),
                true,
                &["database exists inside a Supabase project"],
            ),
            mapping(
                ResourceKind::Auth,
                "auth-config",
                lifecycle(true, true, false, true, false),
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
                ResourceKind::Database,
                "project-branch-database",
                lifecycle(true, true, true, true, true),
                true,
                &["maps database resources to Neon project/branch/database objects"],
            ),
            mapping(
                ResourceKind::Secret,
                "connection-string-reference",
                lifecycle(true, true, true, true, false),
                true,
                &["connection strings are exposed as references, not stored values"],
            ),
        ],
        operations: standard_operations(),
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
                ResourceKind::WebService,
                "service",
                lifecycle(true, true, true, true, true),
                true,
                &["maps web services to Railway services within a project/environment"],
            ),
            mapping(
                ResourceKind::Database,
                "postgres-plugin",
                lifecycle(true, true, true, true, true),
                true,
                &["maps Postgres resources to Railway database services"],
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
