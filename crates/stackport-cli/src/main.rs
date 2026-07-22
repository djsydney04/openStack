use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use stackport_core::analysis::analyze_portability;
use stackport_core::apply::{apply_plan, DryRunAdapter};
use stackport_core::diff::diff_state;
use stackport_core::importers::{import_supabase, import_vercel, SupabaseProject, VercelProject};
use stackport_core::manifest::{validate_manifest, Manifest, MigrationScope};
use stackport_core::planner::{create_plan, MigrationPlan};
use stackport_core::rpc::{handle_rpc_request, JsonRpcRequest};
use stackport_core::state::StackState;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "stackport")]
#[command(about = "Provider-neutral stack portability toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate {
        manifest: PathBuf,
    },
    Import {
        #[command(subcommand)]
        source: ImportCommand,
    },
    Analyze {
        manifest: PathBuf,
        #[arg(long)]
        target: String,
    },
    Plan {
        manifest: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long = "include")]
        include_resources: Vec<String>,
        #[arg(long = "exclude")]
        exclude_resources: Vec<String>,
    },
    Diff {
        manifest: PathBuf,
        state: PathBuf,
    },
    Apply {
        plan: PathBuf,
        #[arg(long, default_value_t = true)]
        dry_run: bool,
    },
    Rpc {
        #[arg(long)]
        once: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    Vercel { input: PathBuf },
    Supabase { input: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { manifest } => {
            let manifest = read_json::<Manifest>(&manifest)?;
            print_json(&validate_manifest(&manifest)?)?;
        }
        Command::Import { source } => match source {
            ImportCommand::Vercel { input } => {
                let project = read_json::<VercelProject>(&input)?;
                print_json(&import_vercel(project))?;
            }
            ImportCommand::Supabase { input } => {
                let project = read_json::<SupabaseProject>(&input)?;
                print_json(&import_supabase(project))?;
            }
        },
        Command::Analyze { manifest, target } => {
            let manifest = read_json::<Manifest>(&manifest)?;
            print_json(
                &analyze_portability(&manifest, &target, None).map_err(anyhow::Error::msg)?,
            )?;
        }
        Command::Plan {
            manifest,
            target,
            include_resources,
            exclude_resources,
        } => {
            let manifest = read_json::<Manifest>(&manifest)?;
            let scope = if include_resources.is_empty() && exclude_resources.is_empty() {
                None
            } else {
                Some(MigrationScope {
                    include_resources,
                    exclude_resources,
                })
            };
            print_json(&create_plan(&manifest, &target, scope).map_err(anyhow::Error::msg)?)?;
        }
        Command::Diff { manifest, state } => {
            let manifest = read_json::<Manifest>(&manifest)?;
            let state = read_json::<StackState>(&state)?;
            print_json(&diff_state(&manifest, &state).map_err(anyhow::Error::msg)?)?;
        }
        Command::Apply { plan, dry_run } => {
            let plan = read_json::<MigrationPlan>(&plan)?;
            print_json(&apply_plan(&plan, &DryRunAdapter, dry_run))?;
        }
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
            Err(err) => stackport_core::JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: serde_json::Value::Null,
                result: None,
                error: Some(stackport_core::rpc::JsonRpcError {
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

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    writeln!(handle)?;
    Ok(())
}
