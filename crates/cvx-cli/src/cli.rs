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
