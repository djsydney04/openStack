use crate::manifest::{
    validate_manifest, Application, Capability, Manifest, Resource, ResourceKind, Variable,
    MANIFEST_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const STACK_SPEC_VERSION: &str = "stackport/app/v1alpha1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackSpec {
    pub version: String,
    pub app: StackApp,
    #[serde(default)]
    pub targets: HashMap<String, StackTarget>,
    #[serde(default)]
    pub services: HashMap<String, ServiceSpec>,
    #[serde(default)]
    pub databases: HashMap<String, DatabaseSpec>,
    #[serde(default)]
    pub secrets: HashMap<String, SecretSpec>,
    #[serde(default)]
    pub domains: HashMap<String, DomainSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackApp {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackTarget {
    pub provider: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceSpec {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub source: Option<SourceSpec>,
    #[serde(default)]
    pub build: Option<BuildSpec>,
    #[serde(default)]
    pub run: Option<RunSpec>,
    #[serde(default)]
    pub env: HashMap<String, EnvValue>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub deploy: Option<DeploySpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSpec {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildSpec {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub framework: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunSpec {
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploySpec {
    #[serde(default)]
    pub replicas: Option<u32>,
    #[serde(default)]
    pub health_check: Option<String>,
    #[serde(default)]
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatabaseSpec {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretSpec {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainSpec {
    pub hostname: String,
    #[serde(default)]
    pub service: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum EnvValue {
    Plain(String),
    SecretRef { secret: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackSpecReport {
    pub valid: bool,
    pub manifest_resources: usize,
    pub warnings: Vec<String>,
}

pub fn validate_stack_spec(
    spec: &StackSpec,
    target: Option<&str>,
) -> Result<StackSpecReport, String> {
    if spec.version != STACK_SPEC_VERSION {
        return Err(format!(
            "unsupported stack spec version `{}`, expected `{STACK_SPEC_VERSION}`",
            spec.version
        ));
    }
    if spec.app.name.trim().is_empty() {
        return Err("app.name is required".to_string());
    }

    let manifest = stack_spec_to_manifest(spec, target)?;
    let report = validate_manifest(&manifest).map_err(|err| err.to_string())?;

    Ok(StackSpecReport {
        valid: report.valid,
        manifest_resources: manifest.resources.len(),
        warnings: report.warnings,
    })
}

pub fn stack_spec_to_manifest(spec: &StackSpec, target: Option<&str>) -> Result<Manifest, String> {
    let target_provider = match target {
        Some(name) => Some(resolve_target(spec, name)?.provider.clone()),
        None => None,
    };
    let target_region = match target {
        Some(name) => resolve_target(spec, name)?.region.clone(),
        None => None,
    };

    let mut resources = Vec::new();
    let mut variables = HashMap::new();

    for (name, database) in &spec.databases {
        resources.push(Resource {
            id: format!("database:{name}"),
            kind: ResourceKind::Database,
            provider: database
                .provider
                .clone()
                .or_else(|| target_provider.clone()),
            capabilities: vec![Capability::Postgres],
            depends_on: vec![],
            properties: serde_json::json!({
                "engine": database.engine.clone().unwrap_or_else(|| "postgres".to_string()),
                "version": database.version,
                "plan": database.plan,
                "region": database.region.clone().or_else(|| target_region.clone()),
            }),
        });
    }

    for (name, service) in &spec.services {
        let service_provider = service.provider.clone().or_else(|| target_provider.clone());
        let mut properties = serde_json::json!({
            "source": service.source,
            "build": service.build,
            "run": service.run,
            "deploy": service.deploy,
            "target": target,
        });
        properties["environment"] = service_env_properties(name, service, &mut variables);

        resources.push(Resource {
            id: format!("service:{name}"),
            kind: ResourceKind::WebService,
            provider: service_provider.clone(),
            capabilities: service_capabilities(service_provider.as_deref()),
            depends_on: service
                .depends_on
                .iter()
                .map(|dependency| normalize_dependency_id(dependency))
                .collect(),
            properties,
        });

        if let Some(deploy) = &service.deploy {
            for hostname in &deploy.domains {
                resources.push(domain_resource(
                    &format!("{name}:{hostname}"),
                    hostname,
                    &format!("service:{name}"),
                    target_provider.clone(),
                ));
            }
        }
    }

    for (name, secret) in &spec.secrets {
        variables.insert(
            name.clone(),
            Variable {
                description: secret.description.clone(),
                secret_ref: secret
                    .from
                    .clone()
                    .or_else(|| Some(format!("stack:{name}"))),
                default: None,
            },
        );
    }

    for (name, domain) in &spec.domains {
        let service_id = domain
            .service
            .as_ref()
            .map(|service| format!("service:{service}"))
            .unwrap_or_else(|| "service:web".to_string());
        resources.push(domain_resource(
            name,
            &domain.hostname,
            &service_id,
            target_provider.clone(),
        ));
    }

    Ok(Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
        application: Application {
            name: spec.app.name.clone(),
            description: spec.app.description.clone(),
            tags: spec.app.tags.clone(),
        },
        resources,
        variables,
    })
}

fn resolve_target<'a>(spec: &'a StackSpec, name: &str) -> Result<&'a StackTarget, String> {
    spec.targets
        .get(name)
        .ok_or_else(|| format!("target `{name}` is not defined"))
}

fn service_env_properties(
    service_name: &str,
    service: &ServiceSpec,
    variables: &mut HashMap<String, Variable>,
) -> serde_json::Value {
    let mut env = serde_json::Map::new();
    for (key, value) in &service.env {
        match value {
            EnvValue::Plain(value) => {
                env.insert(key.clone(), serde_json::Value::String(value.clone()));
            }
            EnvValue::SecretRef { secret } => {
                let secret_ref = format!("${{secret:{secret}}}");
                env.insert(key.clone(), serde_json::Value::String(secret_ref));
                variables.entry(secret.clone()).or_insert_with(|| Variable {
                    description: Some(format!("Referenced by service `{service_name}`")),
                    secret_ref: Some(format!("stack:{secret}")),
                    default: None,
                });
            }
        }
    }
    serde_json::Value::Object(env)
}

fn normalize_dependency_id(dependency: &str) -> String {
    if dependency.contains(':') {
        dependency.to_string()
    } else {
        format!("database:{dependency}")
    }
}

fn service_capabilities(provider: Option<&str>) -> Vec<Capability> {
    match provider {
        Some("vercel") | Some("netlify") => {
            vec![Capability::Build, Capability::ServerlessFunctions]
        }
        _ => vec![Capability::Build],
    }
}

fn domain_resource(
    name: &str,
    hostname: &str,
    service_id: &str,
    provider: Option<String>,
) -> Resource {
    Resource {
        id: format!("domain:{}", sanitize_id(name)),
        kind: ResourceKind::Domain,
        provider,
        capabilities: vec![Capability::CustomDomains],
        depends_on: vec![service_id.to_string()],
        properties: serde_json::json!({ "hostname": hostname }),
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
