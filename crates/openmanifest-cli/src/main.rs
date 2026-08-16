use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use openmanifest_core::manifest::Manifest;
use openmanifest_core::planner::{create_plan_with_state, MigrationPlan};
use openmanifest_core::provider_runtime::{
    apply_provider_requests, build_provider_import_plan, build_provider_probe_plan,
    build_provider_read_plan, build_provider_request_plan, compare_provider_observations,
    execute_provider_observations, execute_provider_probe, reconcile_plan_with_observations,
    HttpProviderTransport, ProcessEnvironment, ProviderContext, ProviderDriftStatus,
    ProviderRequestStatus,
};
use openmanifest_core::providers::{
    provider_definition, provider_execution_plan, provider_registry,
};
use openmanifest_core::rpc::{handle_rpc_request, JsonRpcRequest};
use openmanifest_core::stack_spec::{
    provider_contexts_for_target, stack_spec_to_manifest, validate_stack_spec, StackSpec,
};
use openmanifest_core::state::StackState;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "openmanifest")]
#[command(about = "Simple infrastructure as code for application platforms")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate an application manifest without contacting providers
    Validate {
        /// Path to the OpenManifest YAML file
        manifest: PathBuf,
        /// Optional target to validate
        #[arg(long)]
        target: Option<String>,
    },
    /// Compile YAML into the normalized provider-neutral resource graph
    Compile {
        /// Path to the OpenManifest YAML file
        manifest: PathBuf,
        /// Optional target to compile
        #[arg(long)]
        target: Option<String>,
    },
    /// Compare desired resources with state and print the proposed changes
    Plan {
        /// Path to the OpenManifest YAML file
        manifest: PathBuf,
        /// Target environment defined in the manifest
        #[arg(long)]
        target: String,
        /// State file used for create, update, delete, and no-op decisions
        #[arg(long, default_value = ".openmanifest/state.json")]
        state: PathBuf,
        /// Include provider lifecycle and operation details
        #[arg(long)]
        provider_details: bool,
        /// Compile the exact credential-free provider requests
        #[arg(long, conflicts_with = "provider_details")]
        provider_requests: bool,
        /// Read provider state before calculating the final plan
        #[arg(long)]
        refresh: bool,
    },
    /// Build or execute read-only provider requests for managed resources
    Read {
        /// Path to the OpenManifest YAML file
        manifest: PathBuf,
        /// Target environment defined in the manifest
        #[arg(long)]
        target: String,
        /// State file containing provider identifiers
        #[arg(long, default_value = ".openmanifest/state.json")]
        state: PathBuf,
        /// Contact providers instead of printing the request plan
        #[arg(long)]
        execute: bool,
    },
    /// Discover matching provider resources without adopting them into state
    Import {
        /// Path to the OpenManifest YAML file describing resources to discover
        manifest: PathBuf,
        /// Target environment defined in the manifest
        #[arg(long)]
        target: String,
        /// Contact providers instead of printing the discovery request plan
        #[arg(long)]
        execute: bool,
    },
    /// Print provider requests or execute them after explicit approval
    Apply {
        /// Path to the OpenManifest YAML file
        manifest: PathBuf,
        /// Target environment defined in the manifest
        #[arg(long)]
        target: String,
        /// State file updated after successful provider operations
        #[arg(long, default_value = ".openmanifest/state.json")]
        state: PathBuf,
        /// Execute provider mutations; omission is always a dry preview
        #[arg(long)]
        auto_approve: bool,
    },
    #[command(hide = true)]
    Stack {
        #[command(subcommand)]
        command: StackCommand,
    },
    /// Inspect provider capabilities or test provider credentials
    Providers {
        #[command(subcommand)]
        command: ProvidersCommand,
    },
    /// Run the versioned line-delimited JSON-RPC engine for SDKs
    Rpc {
        #[arg(long)]
        once: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum StackCommand {
    Validate {
        stack: PathBuf,
        #[arg(long)]
        target: Option<String>,
    },
    Manifest {
        stack: PathBuf,
        #[arg(long)]
        target: Option<String>,
    },
    Plan {
        stack: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long, default_value = ".openmanifest/state.json")]
        state: PathBuf,
        #[arg(long)]
        provider_details: bool,
        #[arg(long, conflicts_with = "provider_details")]
        provider_requests: bool,
        #[arg(long)]
        refresh: bool,
    },
    Read {
        stack: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long, default_value = ".openmanifest/state.json")]
        state: PathBuf,
        #[arg(long)]
        execute: bool,
    },
    Import {
        stack: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long)]
        execute: bool,
    },
    Apply {
        stack: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long, default_value = ".openmanifest/state.json")]
        state: PathBuf,
        #[arg(long)]
        auto_approve: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProvidersCommand {
    /// List registered providers
    List,
    /// Print one provider's complete machine-readable contract
    Show { provider: String },
    /// Print or execute harmless provider access probes
    Doctor {
        provider: Option<String>,
        #[arg(long)]
        execute: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { manifest, target } => run_stack_command(StackCommand::Validate {
            stack: manifest,
            target,
        })?,
        Command::Compile { manifest, target } => run_stack_command(StackCommand::Manifest {
            stack: manifest,
            target,
        })?,
        Command::Plan {
            manifest,
            target,
            state,
            provider_details,
            provider_requests,
            refresh,
        } => run_stack_command(StackCommand::Plan {
            stack: manifest,
            target,
            state,
            provider_details,
            provider_requests,
            refresh,
        })?,
        Command::Read {
            manifest,
            target,
            state,
            execute,
        } => run_stack_command(StackCommand::Read {
            stack: manifest,
            target,
            state,
            execute,
        })?,
        Command::Import {
            manifest,
            target,
            execute,
        } => run_stack_command(StackCommand::Import {
            stack: manifest,
            target,
            execute,
        })?,
        Command::Apply {
            manifest,
            target,
            state,
            auto_approve,
        } => run_stack_command(StackCommand::Apply {
            stack: manifest,
            target,
            state,
            auto_approve,
        })?,
        Command::Stack { command } => run_stack_command(command)?,
        Command::Providers { command } => match command {
            ProvidersCommand::List => {
                print_json(&provider_registry())?;
            }
            ProvidersCommand::Show { provider } => {
                let definition = provider_definition(&provider)
                    .ok_or_else(|| anyhow::anyhow!("provider `{provider}` is not registered"))?;
                print_json(&definition)?;
            }
            ProvidersCommand::Doctor { provider, execute } => {
                let plan = build_provider_probe_plan(provider.as_deref(), &HashMap::new())
                    .map_err(anyhow::Error::msg)?;
                if execute {
                    let report = execute_provider_probe(
                        &plan,
                        &HttpProviderTransport::default(),
                        &ProcessEnvironment,
                    );
                    let failed = report
                        .results
                        .iter()
                        .any(|result| result.status != ProviderRequestStatus::Applied);
                    print_json(&report)?;
                    if failed {
                        anyhow::bail!("one or more provider access probes failed");
                    }
                } else {
                    print_json(&plan)?;
                }
            }
        },
        Command::Rpc { once } => {
            if let Some(line) = once {
                let request = serde_json::from_str::<JsonRpcRequest>(&line)
                    .context("failed to parse JSON-RPC request")?;
                print_json(&handle_rpc_request(request))?;
            } else {
                run_rpc_loop()?;
            }
        }
    }
    Ok(())
}

fn run_stack_command(command: StackCommand) -> Result<()> {
    match command {
        StackCommand::Validate { stack, target } => {
            let stack = read_yaml::<StackSpec>(&stack)?;
            print_json(
                &validate_stack_spec(&stack, target.as_deref()).map_err(anyhow::Error::msg)?,
            )?;
        }
        StackCommand::Manifest { stack, target } => {
            let stack = read_yaml::<StackSpec>(&stack)?;
            print_json(
                &stack_spec_to_manifest(&stack, target.as_deref()).map_err(anyhow::Error::msg)?,
            )?;
        }
        StackCommand::Plan {
            stack,
            target,
            state,
            provider_details,
            provider_requests,
            refresh,
        } => {
            let stack = read_yaml::<StackSpec>(&stack)?;
            let target_provider = stack_target_provider(&stack, &target)?;
            let manifest =
                stack_spec_to_manifest(&stack, Some(&target)).map_err(anyhow::Error::msg)?;
            let previous_state = read_json_if_exists::<StackState>(&state)?;
            let mut plan =
                create_plan_with_state(&manifest, &target_provider, None, previous_state.as_ref())
                    .map_err(anyhow::Error::msg)?;
            let contexts =
                provider_contexts_for_target(&stack, &target).map_err(anyhow::Error::msg)?;
            if refresh {
                refresh_plan_from_provider(
                    &manifest,
                    previous_state.as_ref(),
                    &contexts,
                    &mut plan,
                )?;
            }
            if provider_requests {
                print_json(&build_provider_request_plan(&manifest, &plan, &contexts))?;
            } else if provider_details {
                print_json(&provider_execution_plan(&manifest, &plan))?;
            } else {
                print_json(&plan)?;
            }
        }
        StackCommand::Read {
            stack,
            target,
            state,
            execute,
        } => {
            let stack = read_yaml::<StackSpec>(&stack)?;
            let manifest =
                stack_spec_to_manifest(&stack, Some(&target)).map_err(anyhow::Error::msg)?;
            let previous_state = read_json_if_exists::<StackState>(&state)?;
            let contexts =
                provider_contexts_for_target(&stack, &target).map_err(anyhow::Error::msg)?;
            let request_plan =
                build_provider_read_plan(&manifest, previous_state.as_ref(), &contexts);
            if execute {
                if !request_plan.executable {
                    anyhow::bail!(
                            "provider read plan is not executable; resolve warnings and missing identifiers"
                        );
                }
                print_json(&execute_provider_observations(
                    &request_plan,
                    &HttpProviderTransport::default(),
                    &ProcessEnvironment,
                ))?;
            } else {
                print_json(&request_plan)?;
            }
        }
        StackCommand::Import {
            stack,
            target,
            execute,
        } => {
            let stack = read_yaml::<StackSpec>(&stack)?;
            let manifest =
                stack_spec_to_manifest(&stack, Some(&target)).map_err(anyhow::Error::msg)?;
            let contexts =
                provider_contexts_for_target(&stack, &target).map_err(anyhow::Error::msg)?;
            let request_plan = build_provider_import_plan(&manifest, &contexts);
            if execute {
                if !request_plan.executable {
                    anyhow::bail!(
                            "provider import plan is not executable; resolve warnings and missing identifiers"
                        );
                }
                print_json(&execute_provider_observations(
                    &request_plan,
                    &HttpProviderTransport::default(),
                    &ProcessEnvironment,
                ))?;
            } else {
                print_json(&request_plan)?;
            }
        }
        StackCommand::Apply {
            stack,
            target,
            state,
            auto_approve,
        } => {
            let stack = read_yaml::<StackSpec>(&stack)?;
            let target_provider = stack_target_provider(&stack, &target)?;
            let manifest =
                stack_spec_to_manifest(&stack, Some(&target)).map_err(anyhow::Error::msg)?;
            let previous_state = read_json_if_exists::<StackState>(&state)?;
            let mut plan =
                create_plan_with_state(&manifest, &target_provider, None, previous_state.as_ref())
                    .map_err(anyhow::Error::msg)?;
            let contexts =
                provider_contexts_for_target(&stack, &target).map_err(anyhow::Error::msg)?;
            if auto_approve && previous_state.is_some() {
                refresh_plan_from_provider(
                    &manifest,
                    previous_state.as_ref(),
                    &contexts,
                    &mut plan,
                )?;
            }
            let request_plan = build_provider_request_plan(&manifest, &plan, &contexts);
            if !auto_approve {
                print_json(&request_plan)?;
            } else {
                if !request_plan.executable {
                    anyhow::bail!(
                            "provider request plan is not executable; resolve warnings and missing identifiers"
                        );
                }
                let transport = HttpProviderTransport::default();
                let outcome = apply_provider_requests(
                    &manifest,
                    &plan,
                    previous_state.as_ref(),
                    &request_plan,
                    &transport,
                    &ProcessEnvironment,
                );
                write_json_atomic(&state, &outcome.state)?;
                let failed = outcome.execution.results.iter().any(|result| {
                    matches!(
                        result.status,
                        ProviderRequestStatus::Failed | ProviderRequestStatus::Blocked
                    )
                });
                print_json(&outcome)?;
                if failed {
                    anyhow::bail!("one or more provider requests failed; state reflects only successful operations");
                }
            }
        }
    }
    Ok(())
}

fn refresh_plan_from_provider(
    manifest: &Manifest,
    state: Option<&StackState>,
    contexts: &HashMap<String, ProviderContext>,
    plan: &mut MigrationPlan,
) -> Result<()> {
    let Some(state) = state else {
        plan.warnings
            .push("remote refresh skipped because no state file exists".to_string());
        return Ok(());
    };
    let read_plan = build_provider_read_plan(manifest, Some(state), contexts);
    if !read_plan.executable {
        anyhow::bail!(
            "provider refresh is not executable: {}",
            read_plan.warnings.join("; ")
        );
    }
    let read_report = execute_provider_observations(
        &read_plan,
        &HttpProviderTransport::default(),
        &ProcessEnvironment,
    );
    let drift = compare_provider_observations(manifest, Some(state), &read_report);
    if drift
        .resources
        .iter()
        .any(|resource| resource.status == ProviderDriftStatus::ReadFailed)
    {
        anyhow::bail!("provider refresh failed for one or more resources");
    }
    reconcile_plan_with_observations(manifest, plan, &drift);
    Ok(())
}

fn run_rpc_loop() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => handle_rpc_request(request),
            Err(err) => openmanifest_core::JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: serde_json::Value::Null,
                result: None,
                error: Some(openmanifest_core::rpc::JsonRpcError {
                    code: -32700,
                    message: err.to_string(),
                }),
            },
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}

fn read_json<T: for<'de> serde::Deserialize<'de>>(path: &PathBuf) -> Result<T> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse `{}`", path.display()))
}

fn read_yaml<T: for<'de> serde::Deserialize<'de>>(path: &PathBuf) -> Result<T> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    serde_yaml::from_str(&content)
        .with_context(|| format!("failed to parse YAML `{}`", path.display()))
}

fn read_json_if_exists<T: for<'de> serde::Deserialize<'de>>(path: &PathBuf) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    read_json(path).map(Some)
}

fn write_json_atomic<T: serde::Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create `{}`", parent.display()))?;
        }
    }
    let temporary = path.with_extension("json.tmp");
    let content = serde_json::to_vec_pretty(value)?;
    fs::write(&temporary, content)
        .with_context(|| format!("failed to write `{}`", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to replace `{}`", path.display()))?;
    Ok(())
}

fn stack_target_provider(stack: &StackSpec, target: &str) -> Result<String> {
    stack
        .targets
        .get(target)
        .map(|target| target.provider.clone())
        .ok_or_else(|| anyhow::anyhow!("target `{target}` is not defined"))
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    writeln!(handle)?;
    Ok(())
}
