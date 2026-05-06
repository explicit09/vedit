use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod author;
mod cmd;
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
        before: PathBuf,
        after: PathBuf,
        /// Emit machine-readable JSON instead of human prose.
        #[arg(long)]
        json: bool,
    },
    /// Create a new vedit repository in the current directory.
    Init,
    /// Snapshot an OTIO file as a new commit on the current branch.
    Commit {
        /// Path to the OTIO file to snapshot.
        timeline: PathBuf,
        /// Commit message.
        #[arg(short, long)]
        message: String,
    },
    /// Walk commits from HEAD newest-first.
    Log,
    /// Show one commit: its metadata and the diff against its parent.
    Show {
        /// A ref: HEAD, branch name, or full/short commit hash.
        #[arg(default_value = "HEAD")]
        refstr: String,
    },
    /// Write the timeline at a given ref to a file.
    Checkout {
        refstr: String,
        /// Output path. Defaults to <ref>.otio in the current directory.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Diff { before, after, json } => diff::run(&before, &after, json),
        Cmd::Init => cmd::init::run(),
        Cmd::Commit { timeline, message } => cmd::commit::run(&timeline, &message),
        Cmd::Log => cmd::log::run(),
        Cmd::Show { refstr } => cmd::show::run(&refstr),
        Cmd::Checkout { refstr, output } => cmd::checkout::run(&refstr, output.as_deref()),
    }
}
