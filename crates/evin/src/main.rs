// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

#![allow(clippy::result_large_err)]

#[global_allocator]
static ALLOCATOR: peak_alloc::PeakAlloc = peak_alloc::PeakAlloc;

use clap::{Parser, Subcommand};

pub mod commands;
pub mod engine;
pub mod events;
mod lsp;
pub mod phases;
pub mod resolver;
pub mod sync;
pub mod watch;

#[derive(Parser)]
#[command(
    name = "evin",
    version,
    about = evin_locales::t!("app-description"),
    long_about = None
)]
struct Cli {
    #[arg(short = 'v', long, global = true, help = evin_locales::t!("verbose-help"))]
    verbose: bool,

    #[arg(long, global = true, help = evin_locales::t!("dry-run-help"))]
    dry_run: bool,

    #[arg(long, global = true, help = evin_locales::t!("debug-help"))]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Run the Evin Language Server Protocol server over stdio")]
    Lsp,
    #[command(about = evin_locales::t!("init-command-help"))]
    Init {
        #[arg(short = 'w', long, help = evin_locales::t!("init-workspace-help"))]
        workspace: bool,

        #[arg(short = 'g', long, help = evin_locales::t!("init-gitrules-help"))]
        gitrules: bool,

        #[arg(short = 'e', long, help = evin_locales::t!("init-env-help"))]
        env: bool,

        #[arg(short = 'm', long, help = evin_locales::t!("init-module-help"))]
        module: Option<String>,

        #[arg(short = 'a', long, help = evin_locales::t!("init-all-help"))]
        all: bool,

        #[arg(help = evin_locales::t!("init-path-help"))]
        path: Option<std::path::PathBuf>,
    },

    #[command(about = evin_locales::t!("sync-command-help"))]
    Sync,
    #[command(about = evin_locales::t!("list-command-help"))]
    List,
    #[command(about = evin_locales::t!("explain-command-help"))]
    Explain {
        #[arg(help = evin_locales::t!("explain-target-help"))]
        target: String,
    },
    #[command(about = evin_locales::t!("check-command-help"))]
    Check {
        #[arg(
            short = 'w',
            long,
            conflicts_with_all = ["module", "target"],
            help = evin_locales::t!("check-workspace-help")
        )]
        workspace: bool,

        #[arg(
            short = 'm',
            long,
            conflicts_with = "target",
            help = evin_locales::t!("check-module-help")
        )]
        module: Option<String>,

        #[arg(help = evin_locales::t!("check-target-help"))]
        target: Option<String>,
    },
    #[command(about = evin_locales::t!("run-command-help"))]
    Run {
        #[arg(help = evin_locales::t!("run-target-help"))]
        target: String,

        #[arg(long, conflicts_with = "sequential", help = evin_locales::t!("run-parallel-help"))]
        parallel: bool,

        #[arg(long, conflicts_with = "parallel", help = evin_locales::t!("run-sequential-help"))]
        sequential: bool,

        #[arg(
            last = true,
            allow_hyphen_values = true,
            help = evin_locales::t!("run-args-help")
        )]
        args: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    if std::env::var_os("EVIN_TAPE").is_some() {
        // Tape recordings capture through pipes rather than a host TTY. Keep
        // EVIN's normal ANSI diagnostics so the documentation renderer can
        // map them to Matter's terminal palette.
        console::set_colors_enabled(true);
        console::set_colors_enabled_stderr(true);
    }
    let cli = Cli::parse();
    if matches!(cli.command, Commands::Lsp) {
        if let Err(error) = crate::lsp::run_lsp_server() {
            eprintln!("evin: lsp server error: {error}");
            std::process::exit(1);
        }
        return;
    }
    let dry_run = cli.dry_run;
    let verbose = cli.verbose;
    let debug_enabled = cli.debug || std::env::var("EVIN_LOG").as_deref() == Ok("debug");
    let presentation_mode = evin_ui::mode::Mode::resolve(dry_run, debug_enabled);
    let pipeline_mode = if dry_run {
        evin_action_sdk::PipelineMode::DryRun
    } else {
        evin_action_sdk::PipelineMode::Normal
    };

    if matches!(
        &cli.command,
        Commands::List | Commands::Explain { .. } | Commands::Check { .. }
    ) {
        if !run_config_command_pipeline(
            &cli,
            pipeline_mode,
            presentation_mode,
            verbose,
            debug_enabled,
        )
        .await
        {
            std::process::exit(1);
        }
        return;
    }

    // Watch activation is configuration-driven. Failure to pre-resolve is
    // intentionally left to the normal run pipeline, which can render the
    // diagnostic with full context.
    let initial_watch_spec = if !dry_run {
        match &cli.command {
            Commands::Run { target, .. } => crate::watch::resolve_watch_spec(target)
                .await
                .ok()
                .flatten(),
            Commands::List | Commands::Explain { .. } | Commands::Check { .. } | Commands::Lsp => {
                None
            }
            _ => None,
        }
    } else {
        None
    };
    let watch_mode = initial_watch_spec.is_some();

    // The UI is the sole consumer and lifecycle events must not be dropped
    // when a verbose command publishes a large burst of captured output.
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<crate::events::PipelineEvent>();

    let ui_handle = tokio::spawn(crate::events::run_ui_reporter(
        rx,
        presentation_mode,
        verbose,
        watch_mode,
    ));
    tokio::task::yield_now().await;

    if debug_enabled {
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".to_string());

        let _ = tx.send(crate::events::PipelineEvent::Header {
            version: env!("CARGO_PKG_VERSION").to_string(),
            mode: "debug".to_string(),
            os,
            arch,
            shell,
        });
    }

    let mut ctx = crate::engine::CliContext {
        mode: pipeline_mode,
        verbose,
        debug: debug_enabled,
        ..Default::default()
    };
    let mut run_session = None;

    let phases = match cli.command {
        Commands::Init {
            workspace,
            gitrules,
            env,
            all,
            ref module,
            ref path,
        } => {
            let target_str = path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ".".to_string());
            ctx.targets = vec![target_str];

            let intro_text =
                evin_locales::t!("intro-title", category = "Workspace", target = "init");
            let _ = tx.send(crate::events::PipelineEvent::Intro {
                text: intro_text,
                mode: presentation_mode,
            });
            commands::init_phases(workspace, gitrules, env, all, module.clone(), path.clone())
        }

        Commands::Sync => {
            ctx.targets = vec!["sync".to_string()];
            let intro_text =
                evin_locales::t!("intro-title", category = "Workspace", target = "sync");
            let _ = tx.send(crate::events::PipelineEvent::Intro {
                text: intro_text,
                mode: presentation_mode,
            });
            commands::sync_phases()
        }
        Commands::Run {
            ref target,
            parallel,
            sequential,
            ref args,
        } => {
            ctx.targets = vec![target.clone()];
            ctx.run_payload = args.clone();
            if parallel {
                ctx.forced_execution = Some(evin_config::workspace::ExecutionMode::Parallel);
            } else if sequential {
                ctx.forced_execution = Some(evin_config::workspace::ExecutionMode::Linear);
            }

            run_session = Some(crate::watch::RunSession {
                target: target.clone(),
                args: args.clone(),
                mode: pipeline_mode,
                verbose,
                debug: debug_enabled,
                forced_execution: ctx.forced_execution,
            });

            let is_hook = std::env::var("EVIN_HOOK_INVOCATION").is_ok();
            let intro_text =
                evin_ui::pipeline::presenter::format_intro_title(target, args, is_hook);

            let _ = tx.send(crate::events::PipelineEvent::Intro {
                text: intro_text,
                mode: presentation_mode,
            });
            commands::run_phases()
        }
        Commands::List | Commands::Explain { .. } | Commands::Check { .. } | Commands::Lsp => {
            unreachable!("handled before pipeline setup")
        }
    };

    if let Some(request) = run_session {
        if let Some(spec) = initial_watch_spec {
            crate::watch::supervise(tx.clone(), request, spec).await;
        } else {
            crate::watch::run_once_with_signals(tx.clone(), request).await;
        }
    } else {
        let engine = crate::engine::PipelineEngine::new(tx.clone());
        engine.execute_pipeline(phases, ctx).await;
    }

    if debug_enabled {
        let args: Vec<String> = std::env::args().collect();
        let exec_cmd = args.join(" ");
        let flags = vec![
            "debug=true".to_string(),
            format!("dry_run={}", cli.dry_run),
            format!("verbose={}", cli.verbose),
        ];
        let _ = tx.send(crate::events::PipelineEvent::Footer {
            command: exec_cmd,
            flags,
        });
    }

    let success = ui_handle.await.unwrap_or(false);

    if !success {
        std::process::exit(1);
    }
}

async fn run_config_command_pipeline(
    cli: &Cli,
    pipeline_mode: evin_action_sdk::PipelineMode,
    presentation_mode: evin_ui::mode::Mode,
    verbose: bool,
    debug_enabled: bool,
) -> bool {
    use crate::engine::{CliContext, Phase, PipelineCompletion, PipelineEngine};
    use std::sync::{Arc, Mutex};

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let discovery_output = Arc::new(Mutex::new(None));
    let check_output = Arc::new(Mutex::new(None));
    let check_analysis = Arc::new(Mutex::new(None));
    let context = CliContext {
        mode: pipeline_mode,
        verbose,
        debug: debug_enabled,
        skip_default_summary: true,
        ..Default::default()
    };
    let (intro, phases): (String, Vec<Box<dyn Phase>>) = match &cli.command {
        Commands::List => (
            "Workspace › Catalog".into(),
            vec![
                Box::new(crate::phases::run::ResolveConfigPhase),
                Box::new(crate::commands::discovery::RenderDiscoveryPhase {
                    command: crate::commands::discovery::DiscoveryCommand::List,
                    result: discovery_output.clone(),
                }),
            ],
        ),
        Commands::Explain { target } => (
            "Workspace › Explain".into(),
            vec![
                Box::new(crate::phases::run::ResolveConfigPhase),
                Box::new(crate::commands::discovery::RenderDiscoveryPhase {
                    command: crate::commands::discovery::DiscoveryCommand::Explain(target.clone()),
                    result: discovery_output.clone(),
                }),
            ],
        ),
        Commands::Check {
            workspace,
            module,
            target,
        } => (
            "Workspace › Check".into(),
            vec![
                Box::new(crate::commands::check::CheckResolutionPhase {
                    analysis: check_analysis.clone(),
                }),
                Box::new(crate::commands::check::CheckPhase {
                    workspace_only: *workspace,
                    module_filter: module.clone(),
                    target_filter: target.clone(),
                    analysis: check_analysis.clone(),
                    result: check_output.clone(),
                }),
            ],
        ),
        _ => unreachable!("only discovery commands use this runner"),
    };

    let completion = PipelineEngine::new(sender.clone())
        .execute_pipeline(phases, context)
        .await;
    let events = std::iter::from_fn(|| receiver.try_recv().ok()).collect::<Vec<_>>();
    let resolution_duration = events.iter().find_map(|event| match event {
        crate::events::PipelineEvent::PhaseCompleted(key, duration)
            if key == "ui.phase.resolving_configuration" =>
        {
            Some(*duration)
        }
        _ => None,
    });

    if completion == PipelineCompletion::Success {
        match &cli.command {
            Commands::List | Commands::Explain { .. } => {
                let Some(output) = discovery_output.lock().unwrap().take() else {
                    return false;
                };
                let rendered = match resolution_duration {
                    Some(duration) => {
                        evin_ui::discovery::with_resolution_phase(output.rendered, duration)
                    }
                    None => output.rendered,
                };
                crate::commands::discovery::emit(rendered);
                return output.success;
            }
            Commands::Check { .. } => {
                let Some(result) = check_output.lock().unwrap().take() else {
                    return false;
                };
                match result {
                    crate::commands::check::CheckCommandResult::Report { rendered, success } => {
                        let rendered = match resolution_duration {
                            Some(duration) => {
                                evin_ui::discovery::with_resolution_phase(rendered, duration)
                            }
                            None => rendered,
                        };
                        crate::commands::check::emit_report(rendered, success);
                        return success;
                    }
                    crate::commands::check::CheckCommandResult::ResolutionFailed(_) => {
                        return false;
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    let (ui_sender, ui_receiver) = tokio::sync::mpsc::unbounded_channel();
    let ui_handle = tokio::spawn(crate::events::run_ui_reporter(
        ui_receiver,
        presentation_mode,
        verbose,
        false,
    ));
    if debug_enabled {
        let _ = ui_sender.send(crate::events::PipelineEvent::Header {
            version: env!("CARGO_PKG_VERSION").into(),
            mode: "debug".into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            shell: std::env::var("SHELL").unwrap_or_else(|_| "unknown".into()),
        });
    }
    let _ = ui_sender.send(crate::events::PipelineEvent::WorkspaceHeader {
        version: env!("CARGO_PKG_VERSION").into(),
        workspace: evin_locales::t!("check-workspace-unresolved"),
    });
    let _ = ui_sender.send(crate::events::PipelineEvent::Intro {
        text: intro,
        mode: presentation_mode,
    });
    for event in events {
        let _ = ui_sender.send(event);
    }
    if debug_enabled {
        let _ = ui_sender.send(crate::events::PipelineEvent::Footer {
            command: std::env::args().collect::<Vec<_>>().join(" "),
            flags: vec![
                "debug=true".into(),
                format!("dry_run={}", cli.dry_run),
                format!("verbose={}", cli.verbose),
            ],
        });
    }
    drop(ui_sender);
    let _ = ui_handle.await;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_selectors_are_mutually_exclusive() {
        assert!(Cli::try_parse_from(["evin", "check", "--workspace", "verify"]).is_err());
        assert!(Cli::try_parse_from(["evin", "check", "--module", "docs", "docs:verify"]).is_err());
    }
}
