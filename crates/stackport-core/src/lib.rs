pub mod analysis;
pub mod apply;
pub mod diff;
pub mod importers;
pub mod manifest;
pub mod planner;
pub mod provider_runtime;
pub mod providers;
pub mod rpc;
pub mod stack_spec;
pub mod state;

pub use analysis::{analyze_portability, analyze_resource_for_provider, ProviderCapabilities};
pub use apply::{apply_plan, ApplyReport, DryRunAdapter, ProviderAdapter};
pub use diff::{diff_state, DiffReport};
pub use importers::{import_supabase, import_vercel};
pub use manifest::{validate_manifest, Manifest, ManifestError, MigrationScope, Resource};
pub use planner::{create_plan, create_plan_with_state, MigrationPlan, PlanStep};
pub use provider_runtime::{
    apply_provider_requests, build_provider_import_plan, build_provider_probe_plan,
    build_provider_read_plan, build_provider_request_plan, compare_provider_observations,
    execute_provider_observations, execute_provider_probe, execute_provider_requests,
    reconcile_plan_with_observations, HttpProviderTransport, ProcessEnvironment,
    ProviderApplyOutcome, ProviderAuthHeader, ProviderAuthRequirement, ProviderContext,
    ProviderDriftReport, ProviderDriftStatus, ProviderExecutionReport, ProviderIdentifierBinding,
    ProviderProbeReport, ProviderProbeResult, ProviderRequest, ProviderRequestPlan,
    ProviderRequestResult, ProviderRequestStatus, ProviderResourceDrift, ProviderTransport,
    ProviderTransportResponse, ResolvedProviderRequest, RuntimeValueResolver,
};
pub use providers::{
    provider_definition, provider_execution_plan, provider_registry, ApiProtocol, AuthMethod,
    HttpMethod, ProviderApiOperationSpec, ProviderApiSpec, ProviderDefinition,
    ProviderExecutionPlan, ProviderExecutionStep, ProviderOperation, ProviderResourceMapping,
    ResourceLifecycle, SecretPolicy,
};
pub use rpc::{handle_rpc_request, JsonRpcRequest, JsonRpcResponse, STACKPORT_RPC_VERSION};
pub use stack_spec::{
    provider_contexts_for_target, stack_spec_to_manifest, validate_stack_spec, StackSpec,
    StackSpecReport,
};
pub use state::{StackState, StateProvider, StateResource, StateSecretRef, STATE_SCHEMA_VERSION};
