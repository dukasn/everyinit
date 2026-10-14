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

use esh::{Engine, ExecutionContext, run_lsp_server};
use std::env;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let engine = Engine::new();
    let token = CancellationToken::new();

    // Spawn Ctrl+C cancellation listener
    let cancel_token = token.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel_token.cancel();
        }
    });

    let mut ctx = ExecutionContext::new().with_token(token);

    if args.len() >= 2 {
        let first_arg = args[1].as_str();

        if first_arg == "-h" || first_arg == "--help" {
            print_help();
            std::process::exit(0);
        }

        if first_arg == "-v" || first_arg == "--version" {
            println!("esh {} (Evin Shell)", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }

        // Subcommand: `esh lsp` or `esh --lsp`
        if first_arg == "lsp" || first_arg == "--lsp" {
            if let Err(e) = run_lsp_server() {
                eprintln!("esh: lsp server error: {}", e);
                std::process::exit(1);
            }
            std::process::exit(0);
        }

        // Subcommand: `esh help [command]`
        if first_arg == "help" {
            if args.len() >= 3 {
                let tool = &args[2];
                if let Some(doc) = esh::catalog::get_ansi_doc(tool) {
                    println!("{}", doc);
                    std::process::exit(0);
                } else {
                    eprintln!("esh: no help documentation found for '{}'", tool);
                    std::process::exit(1);
                }
            } else {
                print_help();
                std::process::exit(0);
            }
        }

        // Mode A: One-liner `esh -c "<script>"`
        if first_arg == "-c" {
            if args.len() < 3 {
                eprintln!("esh: -c requires a script argument");
                std::process::exit(2);
            }
            match engine.eval_str(&args[2], &mut ctx).await {
                Ok(code) => std::process::exit(code),
                Err(e) => {
                    eprintln!("esh: {}", e);
                    std::process::exit(1);
                }
            }
        }

        // Mode C: Pre-flight dry run `esh --dry-run <file.esh>`
        if first_arg == "--dry-run" {
            if args.len() < 3 {
                eprintln!("esh: --dry-run requires a script file argument");
                std::process::exit(2);
            }
            let script_path = PathBuf::from(&args[2]);
            match std::fs::read_to_string(&script_path) {
                Ok(content) => match engine.compile(&content) {
                    Ok(plan) => {
                        println!("{:#?}", plan);
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("esh: parse error: {}", e);
                        std::process::exit(2);
                    }
                },
                Err(e) => {
                    eprintln!("esh: cannot read file '{}': {}", script_path.display(), e);
                    std::process::exit(1);
                }
            }
        }

        // Mode B: File execution `esh ./script.esh [args...]`
        if !first_arg.starts_with('-') {
            let script_path = PathBuf::from(&args[1]);
            ctx.set_positional_args(args[1..].to_vec());

            match engine.eval_file(&script_path, &mut ctx).await {
                Ok(code) => std::process::exit(code),
                Err(e) => {
                    eprintln!("esh: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    print_help();
    std::process::exit(0);
}

fn print_help() {
    println!("esh — shell for project automation");
    println!("\nUsage:");
    println!("  esh -c '<script>'                 Execute inline script string");
    println!("  esh <file.esh> [args...]          Execute script file with positional arguments");
    println!(
        "  esh --dry-run <file.esh>          Parse and display execution plan without side effects"
    );
    println!(
        "  esh lsp                           Run embedded Language Server Protocol (LSP) daemon"
    );
    println!(
        "  esh help <command>                Show documentation for built-in command or keyword"
    );
    println!("  esh -h, --help                    Show help information");
    println!("  esh -v, --version                 Show version information");
}
