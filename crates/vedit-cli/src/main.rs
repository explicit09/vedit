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
    ///
    /// If --message is omitted, a message is auto-generated from the diff
    /// against HEAD ("Trimmed clip X by 1.8s" / "5 edits: 2 trims, 1 move,
    /// ..."). Useful for unattended workflows like `vedit watch`.
    Commit {
        /// Path to the OTIO file to snapshot.
        timeline: PathBuf,
        /// Commit message. Generated from the diff if omitted.
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Walk commits from HEAD newest-first. Pass a ref to walk from there.
    Log {
        /// A ref to walk from. Defaults to HEAD.
        #[arg(default_value = "HEAD")]
        refstr: String,
    },
    /// Show one commit: its metadata and the diff against its parent.
    Show {
        /// A ref: HEAD, branch name, or full/short commit hash.
        #[arg(default_value = "HEAD")]
        refstr: String,
    },
    /// Switch HEAD to a branch, or write a timeline to disk.
    ///
    /// With `-o <path>`, writes the timeline at <ref> to that path.
    /// Without `-o`, requires <ref> to be a branch name and switches HEAD.
    Checkout {
        refstr: String,
        /// If given, write the timeline at <ref> to this path instead of
        /// switching branches.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Manage branches.
    ///
    /// `vedit branch <name>` creates a new branch at HEAD.
    /// `vedit branch -d <name>` deletes a branch.
    Branch {
        name: String,
        /// Delete the branch instead of creating it.
        #[arg(short, long)]
        delete: bool,
    },
    /// List branches, marking the current one with `*`.
    Branches,
    /// Watch an OTIO file and auto-commit on change.
    ///
    /// Polls the file's mtime + size, debounces with a settling window,
    /// and runs `vedit commit` when the content actually changed. The
    /// commit message is auto-generated from the diff against HEAD.
    Watch {
        /// Path to the OTIO file to watch.
        timeline: PathBuf,
        /// Polling interval in milliseconds. Default 500.
        #[arg(long, default_value_t = 500)]
        interval: u64,
        /// How long the file must be unchanged (in ms) before we commit.
        /// Guards against half-written files. Default 200.
        #[arg(long, default_value_t = 200)]
        settle: u64,
        /// String to prepend to the auto-generated commit message.
        #[arg(long)]
        message_prefix: Option<String>,
        /// Process exactly one change and exit. Useful for tests and
        /// for triggering vedit from a Resolve hotkey.
        #[arg(long)]
        once: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Diff { before, after, json } => diff::run(&before, &after, json),
        Cmd::Init => cmd::init::run(),
        Cmd::Commit { timeline, message } => cmd::commit::run(&timeline, message.as_deref()),
        Cmd::Log { refstr } => cmd::log::run(&refstr),
        Cmd::Show { refstr } => cmd::show::run(&refstr),
        Cmd::Checkout { refstr, output } => cmd::checkout::run(&refstr, output.as_deref()),
        Cmd::Branch { name, delete } => cmd::branch::run(&name, delete),
        Cmd::Branches => cmd::branches::run(),
        Cmd::Watch {
            timeline,
            interval,
            settle,
            message_prefix,
            once,
        } => cmd::watch::run(
            &timeline,
            cmd::watch::WatchOptions {
                interval: std::time::Duration::from_millis(interval),
                settle: std::time::Duration::from_millis(settle),
                message_prefix,
                once,
            },
        ),
    }
}
