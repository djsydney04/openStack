use crate::manifest::Resource;
use crate::planner::{MigrationPlan, PlanAction, PlanStep};
use crate::state::StateResource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyReport {
    pub dry_run: bool,
    pub results: Vec<ApplyStepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyStepResult {
    pub resource_id: String,
    pub status: ApplyStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    Planned,
    Applied,
    Skipped,
    Failed,
}

pub trait ProviderAdapter {
    fn provider_name(&self) -> &str;
    fn check_auth(&self) -> Result<String, String>;
    fn read(&self, resource: &Resource) -> Result<Option<StateResource>, String>;
    fn import(&self, resource: &Resource) -> Result<StateResource, String>;
    fn plan(&self, step: &PlanStep) -> Result<String, String>;
    fn create(&self, step: &PlanStep) -> Result<String, String>;
    fn update(&self, step: &PlanStep) -> Result<String, String>;
    fn delete(&self, step: &PlanStep) -> Result<String, String>;
    fn replace(&self, step: &PlanStep) -> Result<String, String>;
}

pub struct DryRunAdapter;

impl ProviderAdapter for DryRunAdapter {
    fn provider_name(&self) -> &str {
        "dry-run"
    }

    fn check_auth(&self) -> Result<String, String> {
        Ok("dry-run adapter does not require provider credentials".to_string())
    }

    fn read(&self, _resource: &Resource) -> Result<Option<StateResource>, String> {
        Ok(None)
    }

    fn import(&self, resource: &Resource) -> Result<StateResource, String> {
        StateResource::from_applied(
            resource,
            format!("dry-run:{}", resource.id),
            Some(format!("{:?}", resource.kind)),
            Default::default(),
            vec![],
        )
    }

    fn plan(&self, step: &PlanStep) -> Result<String, String> {
        Ok(format!("dry-run plan for `{}`", step.resource_id))
    }

    fn create(&self, step: &PlanStep) -> Result<String, String> {
        Ok(format!("dry-run create for `{}`", step.resource_id))
    }

    fn update(&self, step: &PlanStep) -> Result<String, String> {
        Ok(format!("dry-run update for `{}`", step.resource_id))
    }

    fn delete(&self, step: &PlanStep) -> Result<String, String> {
        Ok(format!("dry-run delete for `{}`", step.resource_id))
    }

    fn replace(&self, step: &PlanStep) -> Result<String, String> {
        Ok(format!("dry-run replace for `{}`", step.resource_id))
    }
}

pub fn apply_plan<A: ProviderAdapter>(
    plan: &MigrationPlan,
    adapter: &A,
    dry_run: bool,
) -> ApplyReport {
    let results = plan
        .steps
        .iter()
        .map(|step| match step.action {
            PlanAction::Create if dry_run => planned_result(step, adapter.create(step)),
            PlanAction::Update if dry_run => planned_result(step, adapter.update(step)),
            PlanAction::Delete if dry_run => planned_result(step, adapter.delete(step)),
            PlanAction::Replace if dry_run => planned_result(step, adapter.replace(step)),
            PlanAction::Create => applied_result(step, adapter.create(step)),
            PlanAction::Update => applied_result(step, adapter.update(step)),
            PlanAction::Delete => applied_result(step, adapter.delete(step)),
            PlanAction::Replace => applied_result(step, adapter.replace(step)),
            PlanAction::Noop => ApplyStepResult {
                resource_id: step.resource_id.clone(),
                status: ApplyStatus::Skipped,
                message: step.reason.clone(),
            },
            PlanAction::Manual | PlanAction::Unsupported => ApplyStepResult {
                resource_id: step.resource_id.clone(),
                status: ApplyStatus::Skipped,
                message: step.reason.clone(),
            },
        })
        .collect();

    ApplyReport { dry_run, results }
}

fn planned_result(step: &PlanStep, result: Result<String, String>) -> ApplyStepResult {
    match result {
        Ok(message) => ApplyStepResult {
            resource_id: step.resource_id.clone(),
            status: ApplyStatus::Planned,
            message,
        },
        Err(message) => ApplyStepResult {
            resource_id: step.resource_id.clone(),
            status: ApplyStatus::Failed,
            message,
        },
    }
}

fn applied_result(step: &PlanStep, result: Result<String, String>) -> ApplyStepResult {
    match result {
        Ok(message) => ApplyStepResult {
            resource_id: step.resource_id.clone(),
            status: ApplyStatus::Applied,
            message,
        },
        Err(message) => ApplyStepResult {
            resource_id: step.resource_id.clone(),
            status: ApplyStatus::Failed,
            message,
        },
    }
}
