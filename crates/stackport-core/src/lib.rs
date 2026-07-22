pub mod analysis;
pub mod apply;
pub mod diff;
pub mod importers;
pub mod manifest;
pub mod planner;
pub mod rpc;
pub mod state;

pub use analysis::{analyze_portability, ProviderCapabilities};
pub use apply::{apply_plan, ApplyReport, DryRunAdapter, ProviderAdapter};
pub use diff::{diff_state, DiffReport};
pub use importers::{import_supabase, import_vercel};
pub use manifest::{validate_manifest, Manifest, ManifestError, MigrationScope, Resource};
pub use planner::{create_plan, MigrationPlan, PlanStep};
pub use rpc::{handle_rpc_request, JsonRpcRequest, JsonRpcResponse, STACKPORT_RPC_VERSION};
pub use state::{StackState, StateResource};
