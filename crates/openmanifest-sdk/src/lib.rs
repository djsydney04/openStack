pub use openmanifest_core::{
    analyze_portability, apply_plan, compile_app_manifest, create_plan, diff_state,
    import_supabase, import_vercel, provider_execution_plan, stack_spec_to_manifest,
    validate_app_manifest, validate_manifest, validate_stack_spec, AppManifest, AppManifestReport,
    ApplyReport, DiffReport, DryRunAdapter, JsonRpcRequest, JsonRpcResponse, Manifest,
    ManifestError, MigrationPlan, MigrationScope, PlanStep, ProviderAdapter, ProviderCapabilities,
    ProviderExecutionPlan, Resource, StackSpec, StackSpecReport, StackState, StateResource,
    OPENMANIFEST_RPC_VERSION,
};

pub struct OpenManifestEngine;

impl OpenManifestEngine {
    pub fn validate_app_manifest(
        &self,
        manifest: &AppManifest,
        target: Option<&str>,
    ) -> Result<AppManifestReport, String> {
        validate_app_manifest(manifest, target)
    }

    pub fn compile_app_manifest(
        &self,
        manifest: &AppManifest,
        target: Option<&str>,
    ) -> Result<Manifest, String> {
        compile_app_manifest(manifest, target)
    }

    pub fn validate(
        &self,
        manifest: &Manifest,
    ) -> Result<openmanifest_core::manifest::ValidationReport, ManifestError> {
        validate_manifest(manifest)
    }

    pub fn analyze(
        &self,
        manifest: &Manifest,
        target_provider: &str,
    ) -> Result<openmanifest_core::analysis::CompatibilityReport, String> {
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

    pub fn provider_execution_plan(
        &self,
        manifest: &Manifest,
        plan: &MigrationPlan,
    ) -> ProviderExecutionPlan {
        provider_execution_plan(manifest, plan)
    }

    pub fn validate_stack_spec(
        &self,
        spec: &StackSpec,
        target: Option<&str>,
    ) -> Result<StackSpecReport, String> {
        validate_stack_spec(spec, target)
    }

    pub fn stack_spec_to_manifest(
        &self,
        spec: &StackSpec,
        target: Option<&str>,
    ) -> Result<Manifest, String> {
        stack_spec_to_manifest(spec, target)
    }
}

impl Default for OpenManifestEngine {
    fn default() -> Self {
        Self
    }
}

#[deprecated(note = "use OpenManifestEngine")]
pub type StackportEngine = OpenManifestEngine;
