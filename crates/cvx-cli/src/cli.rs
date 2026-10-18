use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use cvx_repository::InitStorage;

/// CVX command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "cvx",
    version,
    about = "Own your history. Connect your code.",
    long_about = "CVX is a Git-compatible version control system written in Rust.\n\
                  New repositories use a CVX-branded control directory while existing\n\
                  Git repositories are opened directly in compatibility mode."
)]
pub struct Cli {
    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level CVX commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create an empty CVX repository.
    Init(InitArgs),

    /// Add file contents to the index.
    Add(AddArgs),

    /// Record staged changes in a new commit.
    Commit(CommitArgs),

    /// Show the working tree status.
    Status(StatusArgs),

    /// Show commit history.
    Log(LogArgs),

    /// Print version and format information.
    Version,
}

/// Arguments for `cvx init`.
#[derive(Debug, clap::Args)]
pub struct InitArgs {
    /// Directory to initialize.
    #[arg(default_value = ".")]
    pub directory: PathBuf,

    /// Initial branch name.
    ///
    /// Falls back to init.defaultBranch from Git configuration, then "main".
    #[arg(long, value_name = "BRANCH")]
    pub initial_branch: Option<String>,

    /// Control-directory convention to create.
    #[arg(long, value_enum, default_value = "cvx", value_name = "STORAGE")]
    pub storage: StorageArg,

    /// Suppress the summary.
    #[arg(short, long)]
    pub quiet: bool,
}

/// Arguments for `cvx add`.
#[derive(Debug, clap::Args)]
pub struct AddArgs {
    /// Files or directories to stage.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Stage all changes, including deletions.
    #[arg(short = 'A', long, conflicts_with = "update")]
    pub all: bool,

    /// Update tracked files only.
    #[arg(short = 'u', long)]
    pub update: bool,

    /// Include ignored files.
    #[arg(short = 'f', long)]
    pub force: bool,
}

/// Arguments for `cvx commit`.
#[derive(Debug, clap::Args)]
pub struct CommitArgs {
    /// Commit message. May be supplied more than once.
    #[arg(short, long, value_name = "MESSAGE", required = true)]
    pub message: Vec<String>,

    /// Allow a commit with no changes.
    #[arg(long)]
    pub allow_empty: bool,
}

/// Arguments for `cvx status`.
#[derive(Debug, clap::Args)]
pub struct StatusArgs {
    /// Show untracked files.
    #[arg(long, value_enum, default_value = "normal", value_name = "WHEN")]
    pub untracked_files: UntrackedArg,

    /// Machine-readable output.
    #[arg(long)]
    pub porcelain: bool,

    /// Short output format.
    #[arg(long)]
    pub short: bool,
}

/// Untracked-file display modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum UntrackedArg {
    /// Do not show untracked files.
    No,
    /// Show untracked files and collapse directories.
    Normal,
    /// Show every untracked file.
    All,
}

/// Arguments for `cvx log`.
#[derive(Debug, clap::Args)]
pub struct LogArgs {
    /// Revision to start from.
    #[arg(value_name = "REVISION")]
    pub revision: Option<String>,

    /// Maximum number of commits.
    #[arg(short = 'n', long = "max-count", value_name = "N")]
    pub max_count: Option<usize>,

    /// One line per commit.
    #[arg(long)]
    pub oneline: bool,
}

/// Storage conventions exposed on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StorageArg {
    /// Create `.cvx/` with Git-compatible contents.
    Cvx,
    /// Create a standard `.git/` directory.
    Git,
}

impl From<StorageArg> for InitStorage {
    fn from(value: StorageArg) -> Self {
        match value {
            StorageArg::Cvx => Self::Cvx,
            StorageArg::Git => Self::Git,
        }
    }
}
