#![forbid(unsafe_code)]

use std::path::Path;

use anyhow::Result;
use clap::{Parser, Subcommand};
use fm_parser::{detect_type, parse};
use fm_render_svg::render_svg;

#[derive(Debug, Parser)]
#[command(name = "fm-cli", about = "FrankenMermaid CLI (workspace skeleton)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Detect { input: String },
    Parse { input: String },
    Render { input: String },
}

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Command::Detect { input } => {
            let source = load_input(&input)?;
            println!("{:?}", detect_type(&source));
        }
        Command::Parse { input } => {
            let source = load_input(&input)?;
            let parsed = parse(&source);
            println!(
                "diagram_type={:?} nodes={} edges={}",
                parsed.ir.diagram_type,
                parsed.ir.nodes.len(),
                parsed.ir.edges.len()
            );
            for warning in parsed.warnings {
                eprintln!("warning: {warning}");
            }
        }
        Command::Render { input } => {
            let source = load_input(&input)?;
            let parsed = parse(&source);
            println!("{}", render_svg(&parsed.ir));
        }
    }

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .without_time()
        .try_init();
}

fn load_input(input: &str) -> Result<String> {
    if Path::new(input).exists() {
        Ok(std::fs::read_to_string(input)?)
    } else {
        Ok(input.to_string())
    }
}
