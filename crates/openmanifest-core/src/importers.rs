use crate::manifest::{
    Application, Capability, Manifest, Resource, ResourceKind, Variable, MANIFEST_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VercelProject {
    pub name: String,
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub environment: Vec<VercelEnvironmentVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VercelEnvironmentVariable {
    pub key: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupabaseProject {
    pub project_ref: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub database: bool,
    #[serde(default)]
    pub auth: bool,
    #[serde(default)]
    pub storage_buckets: Vec<String>,
    #[serde(default)]
    pub edge_functions: Vec<String>,
}

pub fn import_vercel(project: VercelProject) -> Manifest {
    let mut resources = vec![Resource {
        id: "web".to_string(),
        kind: ResourceKind::WebService,
        provider: Some("vercel".to_string()),
        capabilities: vec![Capability::Build],
        depends_on: vec![],
        properties: serde_json::json!({
            "framework": project.framework,
        }),
    }];
    let mut variables = HashMap::new();

    for env in project.environment {
        variables.insert(
            env.key.clone(),
            Variable {
                description: env
                    .target
                    .map(|target| format!("Imported from Vercel target `{target}`")),
                secret_ref: Some(format!("vercel:{}", env.key)),
                default: None,
            },
        );
    }

    for domain in project.domains {
        resources.push(Resource {
            id: format!("domain:{}", sanitize_id(&domain)),
            kind: ResourceKind::Domain,
            provider: Some("vercel".to_string()),
            capabilities: vec![Capability::CustomDomains],
            depends_on: vec!["web".to_string()],
            properties: serde_json::json!({ "hostname": domain }),
        });
    }

    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: project.name,
            description: Some("Imported from Vercel".to_string()),
            tags: vec!["vercel".to_string()],
        },
        resources,
        variables,
    }
}

pub fn import_supabase(project: SupabaseProject) -> Manifest {
    let name = project
        .name
        .clone()
        .unwrap_or_else(|| format!("supabase-{}", project.project_ref));
    let mut resources = Vec::new();

    if project.database {
        resources.push(Resource {
            id: "database".to_string(),
            kind: ResourceKind::Database,
            provider: Some("supabase".to_string()),
            capabilities: vec![Capability::Postgres],
            depends_on: vec![],
            properties: serde_json::json!({ "project_ref": project.project_ref }),
        });
    }
    if project.auth {
        resources.push(Resource {
            id: "auth".to_string(),
            kind: ResourceKind::Auth,
            provider: Some("supabase".to_string()),
            capabilities: vec![Capability::Auth],
            depends_on: if project.database {
                vec!["database".to_string()]
            } else {
                vec![]
            },
            properties: serde_json::json!({ "project_ref": project.project_ref }),
        });
    }
    for bucket in project.storage_buckets {
        resources.push(Resource {
            id: format!("bucket:{}", sanitize_id(&bucket)),
            kind: ResourceKind::StorageBucket,
            provider: Some("supabase".to_string()),
            capabilities: vec![Capability::ObjectStorage],
            depends_on: vec![],
            properties: serde_json::json!({ "name": bucket }),
        });
    }
    for function_name in project.edge_functions {
        resources.push(Resource {
            id: format!("function:{}", sanitize_id(&function_name)),
            kind: ResourceKind::Function,
            provider: Some("supabase".to_string()),
            capabilities: vec![Capability::EdgeFunctions],
            depends_on: vec![],
            properties: serde_json::json!({ "name": function_name }),
        });
    }

    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name,
            description: Some("Imported from Supabase".to_string()),
            tags: vec!["supabase".to_string()],
        },
        resources,
        variables: HashMap::new(),
    }
}

fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
