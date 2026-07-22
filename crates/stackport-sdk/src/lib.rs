pub use stackport_core::{
    analyze_portability, apply_plan, create_plan, diff_state, import_supabase, import_vercel,
    validate_manifest, ApplyReport, DiffReport, DryRunAdapter, JsonRpcRequest, JsonRpcResponse,
    Manifest, ManifestError, MigrationPlan, MigrationScope, PlanStep, ProviderAdapter,
    ProviderCapabilities, Resource, StackState, StateResource, STACKPORT_RPC_VERSION,
};

pub struct StackportEngine;

impl StackportEngine {
    pub fn validate(
        &self,
        manifest: &Manifest,
    ) -> Result<stackport_core::manifest::ValidationReport, ManifestError> {
        validate_manifest(manifest)
    }

    pub fn analyze(
        &self,
        manifest: &Manifest,
        target_provider: &str,
    ) -> Result<stackport_core::analysis::CompatibilityReport, String> {
        analyze_portability(manifest, target_provider, None)
    }

    pub fn plan(
        &self,
        manifest: &Manifest,
        target_provider: &str,
        scope: Option<MigrationScope>,
    ) -> Result<MigrationPlan, String> {
        create_plan(manifest, target_provider, scope)
    }

    pub fn diff(&self, manifest: &Manifest, state: &StackState) -> Result<DiffReport, String> {
        diff_state(manifest, state)
    }

    pub fn dry_run(&self, plan: &MigrationPlan) -> ApplyReport {
        apply_plan(plan, &DryRunAdapter, true)
    }
}

impl Default for StackportEngine {
    fn default() -> Self {
        Self
    }
}
