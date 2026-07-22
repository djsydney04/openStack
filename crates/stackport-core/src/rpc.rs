use crate::analysis::{analyze_portability, ProviderCapabilities};
use crate::apply::{apply_plan, DryRunAdapter};
use crate::diff::diff_state;
use crate::importers::{import_supabase, import_vercel, SupabaseProject, VercelProject};
use crate::manifest::{validate_manifest, Manifest, MigrationScope};
use crate::planner::{create_plan, MigrationPlan};
use crate::stack_spec::{stack_spec_to_manifest, validate_stack_spec, StackSpec};
use crate::state::StackState;
use serde::{Deserialize, Serialize};

pub const STACKPORT_RPC_VERSION: &str = "2026-07-22";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalyzeParams {
    pub manifest: Manifest,
    pub target_provider: String,
    #[serde(default)]
    pub capabilities: Option<ProviderCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanParams {
    pub manifest: Manifest,
    pub target_provider: String,
    #[serde(default)]
    pub scope: Option<MigrationScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffParams {
    pub manifest: Manifest,
    pub state: StackState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyParams {
    pub plan: MigrationPlan,
    #[serde(default = "default_true")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StackSpecParams {
    pub spec: StackSpec,
    #[serde(default)]
    pub target: Option<String>,
}

pub fn handle_rpc_request(request: JsonRpcRequest) -> JsonRpcResponse {
    if request.jsonrpc != "2.0" {
        return error(request.id, -32600, "jsonrpc must be `2.0`");
    }

    let result = match request.method.as_str() {
        "stackport.version" => Ok(serde_json::json!({
            "rpc_version": STACKPORT_RPC_VERSION,
            "engine": env!("CARGO_PKG_VERSION"),
        })),
        "manifest.validate" => parse_params::<Manifest>(request.params)
            .and_then(|manifest| validate_manifest(&manifest).map_err(|err| err.to_string()))
            .and_then(to_value),
        "import.vercel" => parse_params::<VercelProject>(request.params)
            .map(import_vercel)
            .and_then(to_value),
        "import.supabase" => parse_params::<SupabaseProject>(request.params)
            .map(import_supabase)
            .and_then(to_value),
        "portability.analyze" => parse_params::<AnalyzeParams>(request.params)
            .and_then(|params| {
                analyze_portability(
                    &params.manifest,
                    &params.target_provider,
                    params.capabilities,
                )
            })
            .and_then(to_value),
        "plan.create" => parse_params::<PlanParams>(request.params)
            .and_then(|params| create_plan(&params.manifest, &params.target_provider, params.scope))
            .and_then(to_value),
        "state.diff" => parse_params::<DiffParams>(request.params)
            .and_then(|params| diff_state(&params.manifest, &params.state))
            .and_then(to_value),
        "apply.dryRun" => parse_params::<ApplyParams>(request.params)
            .map(|params| apply_plan(&params.plan, &DryRunAdapter, params.dry_run))
            .and_then(to_value),
        "stackSpec.validate" => parse_params::<StackSpecParams>(request.params)
            .and_then(|params| validate_stack_spec(&params.spec, params.target.as_deref()))
            .and_then(to_value),
        "stackSpec.toManifest" => parse_params::<StackSpecParams>(request.params)
            .and_then(|params| stack_spec_to_manifest(&params.spec, params.target.as_deref()))
            .and_then(to_value),
        _ => return error(request.id, -32601, "method not found"),
    };

    match result {
        Ok(value) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(value),
            error: None,
        },
        Err(message) => error(request.id, -32000, &message),
    }
}

fn parse_params<T: for<'de> Deserialize<'de>>(params: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(params).map_err(|err| err.to_string())
}

fn to_value<T: Serialize>(value: T) -> Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|err| err.to_string())
}

fn error(id: serde_json::Value, code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
    }
}

fn default_true() -> bool {
    true
}
