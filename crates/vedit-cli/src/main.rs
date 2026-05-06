use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod diff;

#[derive(Parser)]
#[command(name = "vedit", version, about = "Version control for video timelines.")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show what changed between two OTIO timelines.
    Diff {
        /// The earlier timeline.
        before: PathBuf,
        /// The later timeline.
        after: PathBuf,
        /// Emit machine-readable JSON instead of human prose.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Diff {
            before,
            after,
            json,
        } => diff::run(&before, &after, json),
    }
}
