use crate::analysis::analyze_resource_for_provider;
use crate::manifest::{resource_fingerprint, validate_manifest, Manifest, MigrationScope};
use crate::providers::provider_definition;
use crate::state::{StackState, StateResource};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_provider_resource: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub identifiers: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Create,
    Update,
    Delete,
    Replace,
    Noop,
    Manual,
    Unsupported,
}

pub fn create_plan(
    manifest: &Manifest,
    target_provider: &str,
    scope: Option<MigrationScope>,
) -> Result<MigrationPlan, String> {
    create_plan_with_state(manifest, target_provider, scope, None)
}

pub fn create_plan_with_state(
    manifest: &Manifest,
    target_provider: &str,
    scope: Option<MigrationScope>,
    state: Option<&StackState>,
) -> Result<MigrationPlan, String> {
    validate_manifest(manifest).map_err(|err| err.to_string())?;
    let selected = selected_resources(manifest, scope.as_ref());
    let partial = selected.len() != manifest.resources.len();
    let state_by_id = state
        .map(|state| {
            state
                .resources
                .iter()
                .map(|resource| (resource.id.as_str(), resource))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
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

        let current = state_by_id.get(resource.id.as_str()).copied();
        let desired_fingerprint = resource_fingerprint(resource);
        let desired_provider = resource
            .provider
            .clone()
            .unwrap_or_else(|| target_provider.to_string());
        let compatibility = analyze_resource_for_provider(resource, &desired_provider);
        let proposed_action = planned_action(current, &desired_provider, &desired_fingerprint);
        let action = if compatibility.portable
            && provider_supports_action(&desired_provider, &resource.kind, &proposed_action)
        {
            proposed_action
        } else if compatibility.portable || compatibility.missing_capabilities.is_empty() {
            PlanAction::Manual
        } else {
            PlanAction::Unsupported
        };
        let reason = if compatibility.portable && action == PlanAction::Manual {
            format!(
                "provider `{desired_provider}` does not support the required lifecycle operation for `{:?}`",
                resource.kind
            )
        } else if compatibility.portable {
            action_reason(&action, current)
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
            provider: Some(desired_provider),
            prior_provider: current.and_then(|current| current.provider.clone()),
            provider_id: current.map(|current| current.provider_id.clone()),
            provider_resource: current.and_then(|current| current.provider_resource.clone()),
            prior_provider_resource: current.and_then(|current| current.provider_resource.clone()),
            identifiers: current
                .map(|current| current.identifiers.clone())
                .unwrap_or_default(),
            prior_fingerprint: current.map(|current| current.fingerprint.clone()),
            desired_fingerprint: Some(desired_fingerprint),
        });
    }

    if let Some(state) = state {
        for current in &state.resources {
            if manifest
                .resources
                .iter()
                .any(|resource| resource.id == current.id)
            {
                continue;
            }
            if !scope_includes_state_resource(scope.as_ref(), &current.id) {
                continue;
            }
            steps.push(delete_step(current, target_provider));
        }
    }

    Ok(MigrationPlan {
        target_provider: target_provider.to_string(),
        partial,
        steps,
        warnings,
    })
}

fn planned_action(
    current: Option<&StateResource>,
    desired_provider: &str,
    desired_fingerprint: &str,
) -> PlanAction {
    match current {
        None => PlanAction::Create,
        Some(current)
            if current.provider.as_deref().unwrap_or(desired_provider) != desired_provider =>
        {
            PlanAction::Replace
        }
        Some(current) if current.fingerprint != desired_fingerprint => PlanAction::Update,
        Some(_) => PlanAction::Noop,
    }
}

fn provider_supports_action(
    provider: &str,
    kind: &crate::manifest::ResourceKind,
    action: &PlanAction,
) -> bool {
    if action == &PlanAction::Noop {
        return true;
    }
    let Some(lifecycle) = provider_definition(provider).and_then(|definition| {
        definition
            .resources
            .into_iter()
            .find(|mapping| &mapping.stack_kind == kind)
            .map(|mapping| mapping.lifecycle)
    }) else {
        return false;
    };
    match action {
        PlanAction::Create => lifecycle.create,
        PlanAction::Update => lifecycle.update,
        PlanAction::Delete => lifecycle.delete,
        PlanAction::Replace => lifecycle.create && lifecycle.delete,
        PlanAction::Noop => true,
        PlanAction::Manual | PlanAction::Unsupported => false,
    }
}

fn action_reason(action: &PlanAction, current: Option<&StateResource>) -> String {
    match action {
        PlanAction::Create => "resource is not present in state".to_string(),
        PlanAction::Update => "desired configuration differs from last applied state".to_string(),
        PlanAction::Delete => "state resource is no longer desired".to_string(),
        PlanAction::Replace => format!(
            "provider changed from `{}` and requires replacement",
            current
                .and_then(|resource| resource.provider.as_deref())
                .unwrap_or("unknown")
        ),
        PlanAction::Noop => "resource matches last applied state".to_string(),
        PlanAction::Manual | PlanAction::Unsupported => String::new(),
    }
}

fn delete_step(current: &StateResource, target_provider: &str) -> PlanStep {
    PlanStep {
        resource_id: current.id.clone(),
        action: PlanAction::Delete,
        reason: action_reason(&PlanAction::Delete, Some(current)),
        depends_on: vec![],
        provider: Some(
            current
                .provider
                .clone()
                .unwrap_or_else(|| target_provider.to_string()),
        ),
        prior_provider: current.provider.clone(),
        provider_id: Some(current.provider_id.clone()),
        provider_resource: current.provider_resource.clone(),
        prior_provider_resource: current.provider_resource.clone(),
        identifiers: current.identifiers.clone(),
        prior_fingerprint: Some(current.fingerprint.clone()),
        desired_fingerprint: None,
    }
}

fn scope_includes_state_resource(scope: Option<&MigrationScope>, id: &str) -> bool {
    match scope {
        None => true,
        Some(scope) => {
            (scope.include_resources.is_empty()
                || scope
                    .include_resources
                    .iter()
                    .any(|resource| resource == id))
                && !scope
                    .exclude_resources
                    .iter()
                    .any(|resource| resource == id)
        }
    }
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
