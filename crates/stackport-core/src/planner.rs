use crate::analysis::analyze_portability;
use crate::manifest::{validate_manifest, Manifest, MigrationScope};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationPlan {
    pub target_provider: String,
    pub partial: bool,
    pub steps: Vec<PlanStep>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub resource_id: String,
    pub action: PlanAction,
    pub reason: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Create,
    Manual,
    Unsupported,
}

pub fn create_plan(
    manifest: &Manifest,
    target_provider: &str,
    scope: Option<MigrationScope>,
) -> Result<MigrationPlan, String> {
    validate_manifest(manifest).map_err(|err| err.to_string())?;
    let report = analyze_portability(manifest, target_provider, None)?;
    let selected = selected_resources(manifest, scope.as_ref());
    let compatibility_by_id = report
        .resources
        .iter()
        .map(|resource| (resource.resource_id.as_str(), resource))
        .collect::<std::collections::HashMap<_, _>>();
    let partial = selected.len() != manifest.resources.len();
    let mut warnings = Vec::new();

    let mut steps = Vec::new();
    for resource in &manifest.resources {
        if !selected.contains(&resource.id) {
            continue;
        }

        for dependency in &resource.depends_on {
            if !selected.contains(dependency) {
                warnings.push(format!(
                    "resource `{}` depends on `{}` outside the selected migration scope",
                    resource.id, dependency
                ));
            }
        }

        let compatibility = compatibility_by_id
            .get(resource.id.as_str())
            .ok_or_else(|| format!("missing compatibility result for `{}`", resource.id))?;
        let action = if compatibility.portable {
            PlanAction::Create
        } else if compatibility.missing_capabilities.is_empty() {
            PlanAction::Manual
        } else {
            PlanAction::Unsupported
        };
        let reason = if compatibility.portable {
            "target provider has required capabilities".to_string()
        } else {
            format!(
                "missing capabilities: {:?}",
                compatibility.missing_capabilities
            )
        };
        steps.push(PlanStep {
            resource_id: resource.id.clone(),
            action,
            reason,
            depends_on: resource.depends_on.clone(),
        });
    }

    Ok(MigrationPlan {
        target_provider: target_provider.to_string(),
        partial,
        steps,
        warnings,
    })
}

fn selected_resources(manifest: &Manifest, scope: Option<&MigrationScope>) -> HashSet<String> {
    let mut selected = manifest
        .resources
        .iter()
        .map(|resource| resource.id.clone())
        .collect::<HashSet<_>>();
    if let Some(scope) = scope {
        if !scope.include_resources.is_empty() {
            selected = scope.include_resources.iter().cloned().collect();
        }
        for excluded in &scope.exclude_resources {
            selected.remove(excluded);
        }
    }
    selected
}
