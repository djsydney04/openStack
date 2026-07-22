use crate::planner::{MigrationPlan, PlanAction, PlanStep};
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
    fn create(&self, step: &PlanStep) -> Result<String, String>;
}

pub struct DryRunAdapter;

impl ProviderAdapter for DryRunAdapter {
    fn create(&self, step: &PlanStep) -> Result<String, String> {
        Ok(format!("dry-run create for `{}`", step.resource_id))
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
            PlanAction::Create if dry_run => ApplyStepResult {
                resource_id: step.resource_id.clone(),
                status: ApplyStatus::Planned,
                message: adapter.create(step).unwrap_or_else(|err| err),
            },
            PlanAction::Create => match adapter.create(step) {
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
