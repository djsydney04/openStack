use crate::manifest::{validate_manifest, Capability, Manifest, Resource, ResourceKind};
use crate::providers::provider_definition;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub provider: String,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub target_provider: String,
    pub portable: bool,
    pub resources: Vec<ResourceCompatibility>,
    pub required_capabilities: Vec<Capability>,
    pub supported_capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceCompatibility {
    pub resource_id: String,
    pub portable: bool,
    pub missing_capabilities: Vec<Capability>,
    pub notes: Vec<String>,
}

pub fn analyze_portability(
    manifest: &Manifest,
    target_provider: &str,
    catalog: Option<ProviderCapabilities>,
) -> Result<CompatibilityReport, String> {
    validate_manifest(manifest).map_err(|err| err.to_string())?;

    let supported = catalog
        .unwrap_or_else(|| default_capabilities(target_provider))
        .capabilities
        .into_iter()
        .collect::<HashSet<_>>();

    let mut required = HashSet::new();
    let mut resources = Vec::new();

    for resource in &manifest.resources {
        let compatibility = analyze_resource(resource, &supported);
        let capabilities = resource_capabilities(resource);

        for capability in &capabilities {
            required.insert(capability.clone());
        }

        resources.push(compatibility);
    }

    let portable = resources.iter().all(|resource| resource.portable);
    let mut required_capabilities = required.into_iter().collect::<Vec<_>>();
    required_capabilities.sort_by_key(|capability| format!("{capability:?}"));
    let mut supported_capabilities = supported.into_iter().collect::<Vec<_>>();
    supported_capabilities.sort_by_key(|capability| format!("{capability:?}"));

    Ok(CompatibilityReport {
        target_provider: target_provider.to_string(),
        portable,
        resources,
        required_capabilities,
        supported_capabilities,
    })
}

pub fn analyze_resource_for_provider(resource: &Resource, provider: &str) -> ResourceCompatibility {
    let supported = default_capabilities(provider)
        .capabilities
        .into_iter()
        .collect::<HashSet<_>>();
    analyze_resource(resource, &supported)
}

fn analyze_resource(resource: &Resource, supported: &HashSet<Capability>) -> ResourceCompatibility {
    let missing_capabilities = resource_capabilities(resource)
        .into_iter()
        .filter(|capability| !supported.contains(capability))
        .collect::<Vec<_>>();
    let notes = if missing_capabilities.is_empty() {
        vec!["direct migration supported".to_string()]
    } else {
        vec!["requires manual mapping or alternate provider".to_string()]
    };
    ResourceCompatibility {
        resource_id: resource.id.clone(),
        portable: missing_capabilities.is_empty(),
        missing_capabilities,
        notes,
    }
}

fn resource_capabilities(resource: &Resource) -> Vec<Capability> {
    let mut capabilities = if resource.capabilities.is_empty() {
        default_resource_capabilities(&resource.kind)
    } else {
        resource.capabilities.clone()
    };
    capabilities.sort_by_key(|capability| format!("{capability:?}"));
    capabilities.dedup();
    capabilities
}

pub fn default_capabilities(provider: &str) -> ProviderCapabilities {
    if let Some(definition) = provider_definition(provider) {
        return ProviderCapabilities {
            provider: definition.name,
            capabilities: definition.capabilities,
        };
    }

    let mut catalog = HashMap::<&str, Vec<Capability>>::new();
    catalog.insert(
        "vercel",
        vec![
            Capability::Build,
            Capability::ServerlessFunctions,
            Capability::EdgeFunctions,
            Capability::Secrets,
            Capability::CustomDomains,
            Capability::Cron,
        ],
    );
    catalog.insert(
        "supabase",
        vec![
            Capability::Postgres,
            Capability::Auth,
            Capability::ObjectStorage,
            Capability::EdgeFunctions,
            Capability::Secrets,
        ],
    );
    catalog.insert(
        "render",
        vec![
            Capability::Build,
            Capability::ServerlessFunctions,
            Capability::Postgres,
            Capability::Secrets,
            Capability::CustomDomains,
            Capability::Cron,
        ],
    );
    catalog.insert(
        "fly",
        vec![
            Capability::Build,
            Capability::Postgres,
            Capability::Secrets,
            Capability::CustomDomains,
            Capability::Cron,
        ],
    );
    catalog.insert(
        "netlify",
        vec![
            Capability::Build,
            Capability::ServerlessFunctions,
            Capability::EdgeFunctions,
            Capability::Secrets,
            Capability::CustomDomains,
        ],
    );
    ProviderCapabilities {
        provider: provider.to_string(),
        capabilities: catalog
            .remove(provider)
            .unwrap_or_else(|| vec![Capability::Build, Capability::Secrets]),
    }
}

fn default_resource_capabilities(kind: &ResourceKind) -> Vec<Capability> {
    match kind {
        ResourceKind::Project | ResourceKind::Environment => vec![],
        ResourceKind::WebService | ResourceKind::StaticSite => vec![Capability::Build],
        ResourceKind::Build | ResourceKind::DeployHook => vec![Capability::Build],
        ResourceKind::Database => vec![Capability::Postgres],
        ResourceKind::DatabaseBranch | ResourceKind::DatabaseRole => vec![Capability::Postgres],
        ResourceKind::ConnectionString => vec![Capability::Postgres, Capability::Secrets],
        ResourceKind::Auth => vec![Capability::Auth],
        ResourceKind::StorageBucket => vec![Capability::ObjectStorage],
        ResourceKind::Function => vec![Capability::ServerlessFunctions],
        ResourceKind::Secret => vec![Capability::Secrets],
        ResourceKind::Domain => vec![Capability::CustomDomains],
    }
}
