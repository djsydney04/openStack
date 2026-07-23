use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

pub const MANIFEST_SCHEMA_VERSION: &str = "stackport/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema_version: String,
    pub application: Application,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub variables: HashMap<String, Variable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Application {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    pub id: String,
    pub kind: ResourceKind,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Project,
    Environment,
    WebService,
    StaticSite,
    Build,
    DeployHook,
    Database,
    DatabaseBranch,
    DatabaseRole,
    ConnectionString,
    Auth,
    StorageBucket,
    Function,
    Secret,
    Domain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Build,
    ServerlessFunctions,
    EdgeFunctions,
    Postgres,
    Auth,
    ObjectStorage,
    Secrets,
    CustomDomains,
    Cron,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Variable {
    pub description: Option<String>,
    pub secret_ref: Option<String>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationScope {
    #[serde(default)]
    pub include_resources: Vec<String>,
    #[serde(default)]
    pub exclude_resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationReport {
    pub valid: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("unsupported schema_version `{0}`, expected `{expected}`", expected = MANIFEST_SCHEMA_VERSION)]
    UnsupportedSchema(String),
    #[error("application.name is required")]
    MissingApplicationName,
    #[error("resource id is required at index {0}")]
    MissingResourceId(usize),
    #[error("duplicate resource id `{0}`")]
    DuplicateResourceId(String),
    #[error("resource `{resource}` depends on unknown resource `{dependency}`")]
    UnknownDependency {
        resource: String,
        dependency: String,
    },
    #[error("resource dependency cycle detected: {0}")]
    DependencyCycle(String),
    #[error("resource `{0}` contains a likely secret value; use variables.*.secret_ref instead")]
    SecretValue(String),
}

pub fn validate_manifest(manifest: &Manifest) -> Result<ValidationReport, ManifestError> {
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(ManifestError::UnsupportedSchema(
            manifest.schema_version.clone(),
        ));
    }
    if manifest.application.name.trim().is_empty() {
        return Err(ManifestError::MissingApplicationName);
    }

    let mut ids = HashSet::new();
    for (index, resource) in manifest.resources.iter().enumerate() {
        if resource.id.trim().is_empty() {
            return Err(ManifestError::MissingResourceId(index));
        }
        if !ids.insert(resource.id.clone()) {
            return Err(ManifestError::DuplicateResourceId(resource.id.clone()));
        }
        if contains_likely_secret_value(&resource.properties) {
            return Err(ManifestError::SecretValue(resource.id.clone()));
        }
    }

    for resource in &manifest.resources {
        for dependency in &resource.depends_on {
            if !ids.contains(dependency) {
                return Err(ManifestError::UnknownDependency {
                    resource: resource.id.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }
    validate_acyclic_dependencies(manifest)?;

    let warnings = manifest
        .variables
        .iter()
        .filter_map(|(key, variable)| {
            if looks_sensitive_key(key) && variable.secret_ref.is_none() {
                Some(format!(
                    "variable `{key}` looks sensitive but has no secret_ref"
                ))
            } else {
                None
            }
        })
        .collect();

    Ok(ValidationReport {
        valid: true,
        warnings,
    })
}

fn validate_acyclic_dependencies(manifest: &Manifest) -> Result<(), ManifestError> {
    let mut remaining = manifest
        .resources
        .iter()
        .map(|resource| {
            (
                resource.id.as_str(),
                resource
                    .depends_on
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>(),
            )
        })
        .collect::<HashMap<_, _>>();

    loop {
        let ready = remaining
            .iter()
            .filter(|(_, dependencies)| dependencies.is_empty())
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        if ready.is_empty() {
            break;
        }
        for id in ready {
            remaining.remove(id);
            for dependencies in remaining.values_mut() {
                dependencies.remove(id);
            }
        }
    }

    if remaining.is_empty() {
        return Ok(());
    }
    let mut cycle = remaining.keys().copied().collect::<Vec<_>>();
    cycle.sort();
    Err(ManifestError::DependencyCycle(cycle.join(", ")))
}

pub fn resource_fingerprint(resource: &Resource) -> String {
    serde_json::to_string(resource).unwrap_or_else(|_| resource.id.clone())
}

fn contains_likely_secret_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            looks_sensitive_key(key)
                && value
                    .as_str()
                    .map(|s| !s.starts_with("${secret:") && s.len() > 8)
                    .unwrap_or(false)
                || contains_likely_secret_value(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_likely_secret_value),
        _ => false,
    }
}

pub fn looks_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "apikey",
        "api_key",
        "private_key",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}
