use crate::manifest::{resource_fingerprint, validate_manifest, Manifest};
use crate::state::StackState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffReport {
    pub changes: Vec<DiffChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffChange {
    pub resource_id: String,
    pub action: DiffAction,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffAction {
    Create,
    Update,
    Delete,
    Noop,
}

pub fn diff_state(manifest: &Manifest, state: &StackState) -> Result<DiffReport, String> {
    validate_manifest(manifest).map_err(|err| err.to_string())?;
    let state_by_id = state
        .resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<HashMap<_, _>>();
    let desired_ids = manifest
        .resources
        .iter()
        .map(|resource| resource.id.as_str())
        .collect::<HashSet<_>>();

    let mut changes = Vec::new();
    for resource in &manifest.resources {
        let desired = resource_fingerprint(resource);
        match state_by_id.get(resource.id.as_str()) {
            None => changes.push(DiffChange {
                resource_id: resource.id.clone(),
                action: DiffAction::Create,
                reason: "resource is not present in state".to_string(),
            }),
            Some(current) if current.fingerprint != desired => changes.push(DiffChange {
                resource_id: resource.id.clone(),
                action: DiffAction::Update,
                reason: "resource fingerprint differs".to_string(),
            }),
            Some(_) => changes.push(DiffChange {
                resource_id: resource.id.clone(),
                action: DiffAction::Noop,
                reason: "resource matches state".to_string(),
            }),
        }
    }

    for state_resource in &state.resources {
        if !desired_ids.contains(state_resource.id.as_str()) {
            changes.push(DiffChange {
                resource_id: state_resource.id.clone(),
                action: DiffAction::Delete,
                reason: "state resource is no longer desired".to_string(),
            });
        }
    }

    Ok(DiffReport { changes })
}
