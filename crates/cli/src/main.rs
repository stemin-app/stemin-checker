//! `stemin` — the content author's tool.
//!
//! `stemin check` runs exactly what the learner's browser runs on an import:
//! the walk, the structural rules, and the compile. It writes nothing. A
//! repository that passes it imports, and one that fails it does not. It
//! reports every fault it finds, each with its file and its line, and exits
//! non-zero when there is one.

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use stemin_compile::compile;
use stemin_format::{Fault, tree};

/// The URL a check derives its domain ids from when it is given none. An id
/// names nothing in a message, so any fixed address serves.
const PLACEHOLDER_URL: &str = "https://localhost/stemin-check";

/// The Stemin content tool.
#[derive(Parser)]
#[command(name = "stemin", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a content repository: everything the import of a learner checks.
    /// Exits non-zero if anything is wrong.
    Check {
        /// The repository's root. Defaults to the working directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// The URL learners add the repository by. The domain ids come from
        /// it; a check passes or fails the same way with any URL.
        #[arg(long, default_value = PLACEHOLDER_URL)]
        url: String,
    },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .without_time()
        .with_target(false)
        // Colour for a terminal, plain text for a CI log or a pipe.
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Check { path, url } => run_check(&path, &url),
    }
}

/// Read the repository, check it, compile it, and report.
fn run_check(path: &Path, url: &str) -> ExitCode {
    let faults = faults(path, url);
    if faults.is_empty() {
        tracing::info!("{} is a valid content repository", path.display());
        return ExitCode::SUCCESS;
    }
    for fault in &faults {
        // A fault names a file from the repository's root. The author gave a
        // path to that root, so the line names the file from where they stand.
        let file = path.join(&fault.at.file);
        if fault.at.line == 0 {
            tracing::error!("{}: {}", file.display(), fault.kind);
        } else {
            tracing::error!("{}:{}: {}", file.display(), fault.at.line, fault.kind);
        }
    }
    let n = faults.len();
    let word = if n == 1 { "fault" } else { "faults" };
    tracing::error!("{n} {word}");
    ExitCode::FAILURE
}

/// Every fault, in the order the import meets them.
///
/// The disk, the walk and the structural rules report together, so a file that
/// will not parse does not hide the faults in the files beside it. The compile
/// runs once all three are clean: on a tree with a hole in it, it would report
/// links to the content it could not read.
fn faults(path: &Path, url: &str) -> Vec<Fault> {
    let (files, mut faults) = tree::files(path);
    let (root, walked) = tree::read_map("", &files);
    faults.extend(walked);
    let Some(root) = root else {
        return faults;
    };
    let structural = stemin_format::check::check(&root);
    if faults.is_empty() && structural.is_empty() {
        return compile::check(&root, url, &files);
    }
    faults.extend(structural);
    faults
}
