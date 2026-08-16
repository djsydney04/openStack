use crate::analysis::{analyze_portability, ProviderCapabilities};
use crate::apply::{apply_plan, DryRunAdapter};
use crate::diff::diff_state;
use crate::importers::{import_supabase, import_vercel, SupabaseProject, VercelProject};
use crate::manifest::{validate_manifest, Manifest, MigrationScope};
use crate::planner::{create_plan_with_state, MigrationPlan};
use crate::provider_runtime::{
    apply_provider_requests, build_provider_import_plan, build_provider_probe_plan,
    build_provider_read_plan, build_provider_request_plan, compare_provider_observations,
    execute_provider_observations, execute_provider_probe, reconcile_plan_with_observations,
    HttpProviderTransport, ProcessEnvironment, ProviderContext, ProviderDriftStatus,
};
use crate::providers::{provider_definition, provider_execution_plan, provider_registry};
use crate::stack_spec::{stack_spec_to_manifest, validate_stack_spec, StackSpec};
use crate::state::StackState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const OPENMANIFEST_RPC_VERSION: &str = "2026-08-14";
// Kept for source compatibility during the product rename.
pub const STACKPORT_RPC_VERSION: &str = "2026-07-23";

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
    #[serde(default)]
    pub state: Option<StackState>,
    #[serde(default)]
    pub contexts: HashMap<String, ProviderContext>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderParams {
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderApplyParams {
    #[serde(flatten)]
    pub plan: PlanParams,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProbeParams {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub contexts: HashMap<String, ProviderContext>,
}

pub fn handle_rpc_request(request: JsonRpcRequest) -> JsonRpcResponse {
    if request.jsonrpc != "2.0" {
        return error(request.id, -32600, "jsonrpc must be `2.0`");
    }

    let result = match request.method.as_str() {
        "openmanifest.version" => Ok(serde_json::json!({
            "rpc_version": OPENMANIFEST_RPC_VERSION,
            "engine": env!("CARGO_PKG_VERSION"),
        })),
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
            .and_then(|params| {
                create_plan_with_state(
                    &params.manifest,
                    &params.target_provider,
                    params.scope,
                    params.state.as_ref(),
                )
            })
            .and_then(to_value),
        "state.diff" => parse_params::<DiffParams>(request.params)
            .and_then(|params| diff_state(&params.manifest, &params.state))
            .and_then(to_value),
        "apply.dryRun" => parse_params::<ApplyParams>(request.params)
            .map(|params| apply_plan(&params.plan, &DryRunAdapter, params.dry_run))
            .and_then(to_value),
        "appManifest.validate" | "stackSpec.validate" =>
            parse_params::<StackSpecParams>(request.params)
            .and_then(|params| validate_stack_spec(&params.spec, params.target.as_deref()))
            .and_then(to_value),
        "appManifest.compile" | "stackSpec.toManifest" =>
            parse_params::<StackSpecParams>(request.params)
            .and_then(|params| stack_spec_to_manifest(&params.spec, params.target.as_deref()))
            .and_then(to_value),
        "providers.list" => to_value(provider_registry()),
        "providers.show" => parse_params::<ProviderParams>(request.params)
            .and_then(|params| {
                provider_definition(&params.provider)
                    .ok_or_else(|| format!("provider `{}` is not registered", params.provider))
            })
            .and_then(to_value),
        "providers.probePlan" => parse_params::<ProviderProbeParams>(request.params)
            .and_then(|params| {
                build_provider_probe_plan(params.provider.as_deref(), &params.contexts)
            })
            .and_then(to_value),
        "providers.probe" => parse_params::<ProviderProbeParams>(request.params)
            .and_then(|params| {
                let plan =
                    build_provider_probe_plan(params.provider.as_deref(), &params.contexts)?;
                Ok(execute_provider_probe(
                    &plan,
                    &HttpProviderTransport::default(),
                    &ProcessEnvironment,
                ))
            })
            .and_then(to_value),
        "providers.executionPlan" => parse_params::<PlanParams>(request.params)
            .and_then(|params| {
                let plan = create_plan_with_state(
                    &params.manifest,
                    &params.target_provider,
                    params.scope,
                    params.state.as_ref(),
                )?;
                Ok(provider_execution_plan(&params.manifest, &plan))
            })
            .and_then(to_value),
        "providers.requestPlan" => parse_params::<PlanParams>(request.params)
            .and_then(|params| {
                let plan = create_plan_with_state(
                    &params.manifest,
                    &params.target_provider,
                    params.scope,
                    params.state.as_ref(),
                )?;
                Ok(build_provider_request_plan(
                    &params.manifest,
                    &plan,
                    &params.contexts,
                ))
            })
            .and_then(to_value),
        "providers.readPlan" => parse_params::<PlanParams>(request.params)
            .map(|params| {
                build_provider_read_plan(
                    &params.manifest,
                    params.state.as_ref(),
                    &params.contexts,
                )
            })
            .and_then(to_value),
        "providers.importPlan" => parse_params::<PlanParams>(request.params)
            .map(|params| build_provider_import_plan(&params.manifest, &params.contexts))
            .and_then(to_value),
        "providers.read" => parse_params::<PlanParams>(request.params)
            .and_then(|params| {
                let plan = build_provider_read_plan(
                    &params.manifest,
                    params.state.as_ref(),
                    &params.contexts,
                );
                if !plan.executable {
                    return Err(
                        "provider read plan is not executable; resolve warnings and missing identifiers"
                            .to_string(),
                    );
                }
                Ok(execute_provider_observations(
                    &plan,
                    &HttpProviderTransport::default(),
                    &ProcessEnvironment,
                ))
            })
            .and_then(to_value),
        "providers.import" => parse_params::<PlanParams>(request.params)
            .and_then(|params| {
                let plan = build_provider_import_plan(&params.manifest, &params.contexts);
                if !plan.executable {
                    return Err(
                        "provider import plan is not executable; resolve warnings and missing identifiers"
                            .to_string(),
                    );
                }
                Ok(execute_provider_observations(
                    &plan,
                    &HttpProviderTransport::default(),
                    &ProcessEnvironment,
                ))
            })
            .and_then(to_value),
        "providers.refreshPlan" => parse_params::<PlanParams>(request.params)
            .and_then(|params| {
                let state = params
                    .state
                    .as_ref()
                    .ok_or_else(|| "providers.refreshPlan requires state".to_string())?;
                let read_plan =
                    build_provider_read_plan(&params.manifest, Some(state), &params.contexts);
                if !read_plan.executable {
                    return Err(format!(
                        "provider refresh is not executable: {}",
                        read_plan.warnings.join("; ")
                    ));
                }
                let read = execute_provider_observations(
                    &read_plan,
                    &HttpProviderTransport::default(),
                    &ProcessEnvironment,
                );
                let drift =
                    compare_provider_observations(&params.manifest, Some(state), &read);
                if drift
                    .resources
                    .iter()
                    .any(|resource| resource.status == ProviderDriftStatus::ReadFailed)
                {
                    return Err("provider refresh failed for one or more resources".to_string());
                }
                let mut plan = create_plan_with_state(
                    &params.manifest,
                    &params.target_provider,
                    params.scope,
                    Some(state),
                )?;
                reconcile_plan_with_observations(&params.manifest, &mut plan, &drift);
                Ok(serde_json::json!({
                    "plan": plan,
                    "read": read,
                    "drift": drift,
                }))
            }),
        "providers.apply" => parse_params::<ProviderApplyParams>(request.params)
            .and_then(|params| {
                if !params.confirm {
                    return Err("providers.apply requires confirm=true".to_string());
                }
                let plan = create_plan_with_state(
                    &params.plan.manifest,
                    &params.plan.target_provider,
                    params.plan.scope,
                    params.plan.state.as_ref(),
                )?;
                let request_plan = build_provider_request_plan(
                    &params.plan.manifest,
                    &plan,
                    &params.plan.contexts,
                );
                if !request_plan.executable {
                    return Err(
                        "provider request plan is not executable; resolve warnings and missing identifiers"
                            .to_string(),
                    );
                }
                Ok(apply_provider_requests(
                    &params.plan.manifest,
                    &plan,
                    params.plan.state.as_ref(),
                    &request_plan,
                    &HttpProviderTransport::default(),
                    &ProcessEnvironment,
                ))
            })
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
