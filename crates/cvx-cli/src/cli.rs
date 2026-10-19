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

    /// Clone a repository into a new directory.
    Clone(CloneArgs),

    /// Manage remotes.
    Remote(RemoteArgs),

    /// Download objects and refs from a remote.
    Fetch(FetchArgs),

    /// Update a remote branch.
    Push(PushArgs),

    /// Manage securely stored credentials.
    Credential(CredentialArgs),

    /// Add file contents to the index.
    Add(AddArgs),

    /// Record staged changes in a new commit.
    Commit(CommitArgs),

    /// Show the working tree status.
    Status(StatusArgs),

    /// Show commit history.
    Log(LogArgs),

    /// Show changes between commits, the index and the worktree.
    Diff(DiffArgs),

    /// Restore index entries or worktree files.
    Restore(RestoreArgs),

    /// Restore worktree files from the index.
    Checkout(CheckoutArgs),

    /// List, create or delete branches.
    Branch(BranchArgs),

    /// Switch branches or detach HEAD.
    Switch(SwitchArgs),

    /// Join another branch's history into the current branch.
    Merge(MergeArgs),

    /// Create or list tags.
    Tag(TagArgs),

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

/// Arguments for `cvx clone`.
#[derive(Debug, clap::Args)]
pub struct CloneArgs {
    /// Repository URL or local path.
    #[arg(value_name = "URL")]
    pub url: String,

    /// Destination directory.
    #[arg(value_name = "DIRECTORY")]
    pub directory: Option<PathBuf>,

    /// Branch to check out.
    #[arg(short, long, value_name = "BRANCH")]
    pub branch: Option<String>,

    /// Control-directory convention to create.
    #[arg(long, value_enum, default_value = "cvx", value_name = "STORAGE")]
    pub storage: StorageArg,

    /// Create a shallow clone with this depth.
    #[arg(long, value_name = "N")]
    pub depth: Option<u32>,

    /// Partial-clone filter, for example `blob:none`.
    #[arg(long, value_name = "FILTER")]
    pub filter: Option<String>,
}

/// Arguments for `cvx remote`.
#[derive(Debug, clap::Args)]
pub struct RemoteArgs {
    /// Remote operation.
    #[command(subcommand)]
    pub command: Option<RemoteCommand>,
}

/// Remote subcommands.
#[derive(Debug, Subcommand)]
pub enum RemoteCommand {
    /// List configured remotes.
    List,
    /// Add a remote.
    Add {
        /// Remote name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Remote URL or local path.
        #[arg(value_name = "URL")]
        url: String,
    },
}

/// Arguments for `cvx fetch`.
#[derive(Debug, clap::Args)]
pub struct FetchArgs {
    /// Remote name. Defaults to origin.
    #[arg(value_name = "REMOTE")]
    pub remote: Option<String>,
}

/// Arguments for `cvx push`.
#[derive(Debug, clap::Args)]
pub struct PushArgs {
    /// Remote name. Defaults to origin.
    #[arg(value_name = "REMOTE")]
    pub remote: Option<String>,

    /// Branch to push. Defaults to the current branch.
    #[arg(value_name = "BRANCH")]
    pub branch: Option<String>,

    /// Allow non-fast-forward updates.
    #[arg(short, long)]
    pub force: bool,
}

/// Arguments for `cvx credential`.
#[derive(Debug, clap::Args)]
pub struct CredentialArgs {
    /// Credential operation.
    #[command(subcommand)]
    pub command: CredentialCommand,
}

/// Credential subcommands.
#[derive(Debug, Subcommand)]
pub enum CredentialCommand {
    /// Store a token read from standard input.
    Store {
        /// Host name, such as github.com.
        #[arg(value_name = "HOST")]
        host: String,
        /// Username or token account name.
        #[arg(long, default_value = "x-access-token")]
        username: String,
    },
    /// List hosts with stored credentials.
    List,
    /// Remove a stored credential.
    Remove {
        /// Host name, such as github.com.
        #[arg(value_name = "HOST")]
        host: String,
    },
    /// Git credential-helper `fill` operation.
    Fill,
    /// Git credential-helper `approve` operation.
    Approve,
    /// Git credential-helper `reject` operation.
    Reject,
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

    /// Show branch and tag decorations.
    #[arg(long)]
    pub decorate: bool,
}

/// Arguments for `cvx diff`.
#[derive(Debug, clap::Args)]
pub struct DiffArgs {
    /// Compare HEAD with the index.
    #[arg(long)]
    pub cached: bool,

    /// Show a diffstat instead of hunks.
    #[arg(long)]
    pub stat: bool,

    /// Number of context lines.
    #[arg(short = 'U', long = "unified", default_value_t = 3, value_name = "N")]
    pub context: usize,

    /// Zero, one or two revisions.
    #[arg(value_name = "REV")]
    pub revisions: Vec<String>,

    /// Optional pathspecs after `--`.
    #[arg(last = true, value_name = "PATH")]
    pub paths: Vec<PathBuf>,
}

/// Arguments for `cvx restore`.
#[derive(Debug, clap::Args)]
pub struct RestoreArgs {
    /// Restore index entries.
    #[arg(short = 'S', long)]
    pub staged: bool,

    /// Restore worktree files (the default).
    #[arg(short = 'W', long)]
    pub worktree: bool,

    /// Source revision.
    #[arg(short = 's', long, value_name = "REV")]
    pub source: Option<String>,

    /// Overwrite untracked files.
    #[arg(short = 'f', long)]
    pub force: bool,

    /// Files or directories to restore.
    #[arg(value_name = "PATH", required = true)]
    pub paths: Vec<PathBuf>,
}

/// Arguments for `cvx checkout -- <paths>`.
#[derive(Debug, clap::Args)]
pub struct CheckoutArgs {
    /// Files or directories to restore from the index.
    #[arg(last = true, value_name = "PATH", required = true)]
    pub paths: Vec<PathBuf>,
}

/// Arguments for `cvx branch`.
#[derive(Debug, clap::Args)]
pub struct BranchArgs {
    /// Delete a fully merged branch.
    #[arg(short = 'd', long, conflicts_with = "force_delete")]
    pub delete: bool,

    /// Delete a branch even if it is not merged.
    #[arg(short = 'D', conflicts_with = "delete")]
    pub force_delete: bool,

    /// List branches.
    #[arg(short = 'l', long)]
    pub list: bool,

    /// Branch name.
    #[arg(value_name = "BRANCH")]
    pub name: Option<String>,

    /// Starting point for a new branch.
    #[arg(value_name = "START")]
    pub start_point: Option<String>,
}

/// Arguments for `cvx switch`.
#[derive(Debug, clap::Args)]
pub struct SwitchArgs {
    /// Create and switch to a new branch.
    #[arg(short = 'c', long, conflicts_with = "detach")]
    pub create: bool,

    /// Detach HEAD at the target revision.
    #[arg(long, conflicts_with = "create")]
    pub detach: bool,

    /// Discard local changes instead of refusing to switch.
    #[arg(short = 'f', long)]
    pub force: bool,

    /// Branch name or revision.
    #[arg(value_name = "BRANCH")]
    pub target: Option<String>,
}

/// Arguments for `cvx merge`.
#[derive(Debug, clap::Args)]
pub struct MergeArgs {
    /// Abort an in-progress merge.
    #[arg(long, conflicts_with = "continue_merge")]
    pub abort: bool,

    /// Conclude an in-progress merge after resolving conflicts.
    #[arg(long = "continue", conflicts_with = "abort")]
    pub continue_merge: bool,

    /// Merge-commit message.
    #[arg(short, long, value_name = "MESSAGE")]
    pub message: Option<String>,

    /// Branch or revision to merge.
    #[arg(value_name = "REVISION")]
    pub revision: Option<String>,
}

/// Arguments for `cvx tag`.
#[derive(Debug, clap::Args)]
pub struct TagArgs {
    /// List tags.
    #[arg(short, long)]
    pub list: bool,

    /// Create an annotated tag.
    #[arg(short, long)]
    pub annotate: bool,

    /// Replace an existing tag.
    #[arg(short, long)]
    pub force: bool,

    /// Annotated-tag message.
    #[arg(short, long, value_name = "MESSAGE")]
    pub message: Option<String>,

    /// Tag name.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Commit to tag. Defaults to HEAD.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,
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
