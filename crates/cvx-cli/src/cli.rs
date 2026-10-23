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

    /// GitHub account, repository and collaboration workflows.
    Github(GithubArgs),

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

    /// Verify repository integrity.
    Fsck(FsckArgs),

    /// Consolidate loose objects into a pack.
    Gc(GcArgs),

    /// Move HEAD and optionally reset the index and worktree.
    Reset(ResetArgs),

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

    /// Apply a commit on top of the current branch.
    CherryPick(PickArgs),

    /// Revert a commit on top of the current branch.
    Revert(PickArgs),

    /// Save and restore local changes.
    Stash(StashArgs),

    /// Create or list tags.
    Tag(TagArgs),

    /// Print version and format information.
    Version,
}

/// Arguments for `cvx stash`.
#[derive(Debug, clap::Args)]
pub struct StashArgs {
    /// Stash operation to perform.
    #[command(subcommand)]
    pub action: StashAction,
}

/// Stash subcommands.
#[derive(Debug, Subcommand)]
pub enum StashAction {
    /// Save tracked index and worktree changes.
    Push(StashPushArgs),
    /// List stash entries.
    List,
    /// Apply the newest stash without removing it.
    Apply,
    /// Apply and remove the newest stash.
    Pop,
    /// Remove the newest stash.
    Drop,
}

/// Arguments for `cvx stash push`.
#[derive(Debug, clap::Args)]
pub struct StashPushArgs {
    /// Stash message.
    #[arg(short, long, value_name = "MESSAGE")]
    pub message: Option<String>,
}

/// Arguments for `cvx cherry-pick` and `cvx revert`.
#[derive(Debug, clap::Args)]
pub struct PickArgs {
    /// Commit or revision to apply.
    #[arg(value_name = "REVISION")]
    pub revision: String,
}

/// Arguments for `cvx fsck`.
#[derive(Debug, clap::Args)]
pub struct FsckArgs {
    /// Treat warnings as errors.
    #[arg(long)]
    pub strict: bool,

    /// Rebuild a missing or unreadable index from HEAD.
    #[arg(long)]
    pub repair: bool,
}

/// Arguments for `cvx gc`.
#[derive(Debug, clap::Args)]
pub struct GcArgs {
    /// Delete unreachable loose objects.
    #[arg(long)]
    pub prune: bool,

    /// Suppress the summary.
    #[arg(short, long)]
    pub quiet: bool,
}

/// Arguments for `cvx reset`.
#[derive(Debug, clap::Args)]
pub struct ResetArgs {
    /// Commit or revision to reset to.
    #[arg(default_value = "HEAD")]
    pub target: String,

    /// Move HEAD only; keep the index and worktree.
    #[arg(long, conflicts_with_all = ["mixed", "hard"])]
    pub soft: bool,

    /// Move HEAD and reset the index; keep the worktree (default).
    #[arg(long, conflicts_with_all = ["soft", "hard"])]
    pub mixed: bool,

    /// Move HEAD, reset the index and overwrite the worktree.
    #[arg(long, conflicts_with_all = ["soft", "mixed"])]
    pub hard: bool,

    /// Pathspecs whose index entries should be reset.
    #[arg(last = true, value_name = "PATH")]
    pub paths: Vec<PathBuf>,
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

    /// Object hash format for a new repository.
    #[arg(long, value_enum, default_value = "sha1", value_name = "FORMAT")]
    pub object_format: ObjectFormatArg,

    /// Create a bare repository without a worktree.
    #[arg(long)]
    pub bare: bool,

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

    /// Clone without a worktree.
    #[arg(long)]
    pub bare: bool,
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

/// Arguments for `cvx github`.
#[derive(Debug, clap::Args)]
pub struct GithubArgs {
    /// GitHub operation.
    #[command(subcommand)]
    pub command: GithubCommand,
}

/// GitHub subcommands.
#[derive(Debug, Subcommand)]
pub enum GithubCommand {
    /// Authenticate with a token or GitHub's OAuth device flow.
    Login(LoginArgs),
    /// Show the authenticated GitHub account.
    Status,
    /// Remove the stored GitHub token.
    Logout,
    /// Show the scopes granted to the current token.
    Scopes(ScopesArgs),
    /// Execute a GraphQL query.
    Graphql(GraphqlArgs),
    /// Repository operations.
    Repo {
        /// Repository operation.
        #[command(subcommand)]
        command: GithubRepoCommand,
    },
    /// Issue operations.
    Issue {
        /// Issue operation.
        #[command(subcommand)]
        command: GithubIssueCommand,
    },
    /// Pull-request operations.
    Pr {
        /// Pull-request operation.
        #[command(subcommand)]
        command: GithubPrCommand,
    },
    /// Release operations.
    Release {
        /// Release operation.
        #[command(subcommand)]
        command: GithubReleaseCommand,
    },
    /// Label operations.
    Label {
        /// Label operation.
        #[command(subcommand)]
        command: GithubLabelCommand,
    },
    /// Collaborator and permission operations.
    Collaborator {
        /// Collaborator operation.
        #[command(subcommand)]
        command: GithubCollaboratorCommand,
    },
    /// Milestone operations.
    Milestone {
        /// Milestone operation.
        #[command(subcommand)]
        command: GithubMilestoneCommand,
    },
    /// Check runs and required status checks.
    Checks(ChecksArgs),
    /// List recent GitHub Actions workflow runs.
    Workflows {
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

/// Arguments for `cvx github login`.
#[derive(Debug, clap::Args)]
pub struct LoginArgs {
    /// Use the OAuth device flow instead of reading a token from stdin.
    #[arg(long)]
    pub oauth: bool,

    /// OAuth app client id. Defaults to CVX_GITHUB_CLIENT_ID.
    #[arg(long, value_name = "CLIENT_ID")]
    pub client_id: Option<String>,

    /// OAuth scope. May be repeated. Defaults to repo, workflow and read:org.
    #[arg(long = "scope", value_name = "SCOPE")]
    pub scopes: Vec<String>,
}

/// Arguments for `cvx github scopes`.
#[derive(Debug, clap::Args)]
pub struct ScopesArgs {
    /// Fail if one of these scopes is missing.
    #[arg(long = "require", value_name = "SCOPE")]
    pub require: Vec<String>,
}

/// Arguments for `cvx github graphql`.
#[derive(Debug, clap::Args)]
pub struct GraphqlArgs {
    /// GraphQL query string.
    #[arg(long, value_name = "QUERY")]
    pub query: String,

    /// JSON object with query variables.
    #[arg(long, value_name = "JSON")]
    pub variables: Option<String>,

    /// Read JSON variables from a file.
    #[arg(long, value_name = "FILE", conflicts_with = "variables")]
    pub variables_file: Option<PathBuf>,
}

/// Collaborator operations.
#[derive(Debug, Subcommand)]
pub enum GithubCollaboratorCommand {
    /// List collaborators and their permissions.
    List {
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Add or update a collaborator.
    Add {
        /// GitHub login.
        #[arg(value_name = "USER")]
        user: String,
        /// Permission: pull, triage, push, maintain or admin.
        #[arg(long, default_value = "push", value_name = "PERMISSION")]
        permission: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Remove a collaborator.
    Remove {
        /// GitHub login.
        #[arg(value_name = "USER")]
        user: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Show a collaborator's effective permission.
    Permission {
        /// GitHub login.
        #[arg(value_name = "USER")]
        user: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

/// Milestone operations.
#[derive(Debug, Subcommand)]
pub enum GithubMilestoneCommand {
    /// List milestones.
    List {
        /// State: open, closed or all.
        #[arg(long, default_value = "open", value_name = "STATE")]
        state: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Create a milestone.
    Create {
        /// Milestone title.
        #[arg(value_name = "TITLE")]
        title: String,
        /// Description.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
        /// Due date in ISO 8601 form.
        #[arg(long, value_name = "DATE")]
        due_on: Option<String>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Close a milestone.
    Close {
        /// Milestone number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

/// Arguments for `cvx github checks`.
#[derive(Debug, clap::Args)]
pub struct ChecksArgs {
    /// Check operation. Omit to list check runs.
    #[command(subcommand)]
    pub command: Option<ChecksCommand>,

    /// Commit or ref for check-run listing. Defaults to the local HEAD.
    #[arg(long, value_name = "REF")]
    pub reference: Option<String>,

    /// Repository as owner/name or URL.
    #[arg(long, value_name = "OWNER/REPO")]
    pub repo: Option<String>,
}

/// Required-check operations.
#[derive(Debug, Subcommand)]
pub enum ChecksCommand {
    /// Show required status checks for a branch.
    Required {
        /// Branch name.
        #[arg(long, default_value = "main", value_name = "BRANCH")]
        branch: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Replace required status checks for a branch.
    Require {
        /// Check context. May be repeated.
        #[arg(value_name = "CONTEXT", required = true)]
        contexts: Vec<String>,
        /// Branch name.
        #[arg(long, default_value = "main", value_name = "BRANCH")]
        branch: String,
        /// Require branches to be up to date before merging.
        #[arg(long)]
        strict: bool,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

/// GitHub repository operations.
#[derive(Debug, Subcommand)]
pub enum GithubRepoCommand {
    /// Create a repository.
    Create {
        /// Repository name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Create a private repository.
        #[arg(long)]
        private: bool,
        /// Repository description.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
        /// Push the current branch after creating the repository.
        #[arg(long)]
        push: bool,
        /// Branch to push. Defaults to the current branch.
        #[arg(long, value_name = "BRANCH")]
        branch: Option<String>,
    },
    /// Delete a repository.
    Delete {
        /// Repository as owner/name or URL. Defaults to origin.
        #[arg(value_name = "OWNER/REPO")]
        repo: Option<String>,
        /// Confirm deletion.
        #[arg(long)]
        yes: bool,
    },
    /// Show repository information.
    Show {
        /// Repository as owner/name or URL. Defaults to origin.
        #[arg(value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

/// GitHub issue operations.
#[derive(Debug, Subcommand)]
pub enum GithubIssueCommand {
    /// Create an issue.
    Create {
        /// Issue title.
        #[arg(value_name = "TITLE")]
        title: String,
        /// Issue body.
        #[arg(short, long, value_name = "TEXT")]
        body: Option<String>,
        /// Assignee login. May be repeated.
        #[arg(long = "assignee", value_name = "USER")]
        assignees: Vec<String>,
        /// Milestone number.
        #[arg(long, value_name = "NUMBER")]
        milestone: Option<u64>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// List issues.
    List {
        /// Issue state: open, closed or all.
        #[arg(long, default_value = "open", value_name = "STATE")]
        state: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Close an issue.
    Close {
        /// Issue number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Add assignees to an issue or pull request.
    Assign {
        /// Issue or pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Assignee login. May be repeated.
        #[arg(value_name = "USER", required = true)]
        assignees: Vec<String>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Remove assignees from an issue or pull request.
    Unassign {
        /// Issue or pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Assignee login. May be repeated.
        #[arg(value_name = "USER", required = true)]
        assignees: Vec<String>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

/// GitHub pull-request operations.
#[derive(Debug, Subcommand)]
pub enum GithubPrCommand {
    /// Create a pull request.
    Create {
        /// Pull-request title.
        #[arg(value_name = "TITLE")]
        title: String,
        /// Source branch.
        #[arg(long, value_name = "BRANCH")]
        head: String,
        /// Target branch. Defaults to the repository default branch.
        #[arg(long, value_name = "BRANCH")]
        base: Option<String>,
        /// Pull-request body.
        #[arg(short, long, value_name = "TEXT")]
        body: Option<String>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// List pull requests.
    List {
        /// Pull-request state: open, closed or all.
        #[arg(long, default_value = "open", value_name = "STATE")]
        state: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Merge a pull request.
    Merge {
        /// Pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Merge method: merge, squash or rebase.
        #[arg(long, default_value = "merge", value_name = "METHOD")]
        method: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// List reviews for a pull request.
    Reviews {
        /// Pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Create a review for a pull request.
    Review {
        /// Pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Approve the pull request.
        #[arg(long, conflicts_with_all = ["request_changes", "comment"])]
        approve: bool,
        /// Request changes.
        #[arg(long, conflicts_with_all = ["approve", "comment"])]
        request_changes: bool,
        /// Leave a review comment.
        #[arg(long, conflicts_with_all = ["approve", "request_changes"])]
        comment: bool,
        /// Review body.
        #[arg(short, long, value_name = "TEXT")]
        body: Option<String>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// List review threads for a pull request.
    Threads {
        /// Pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Resolve a review thread.
    Resolve {
        /// Review thread node id from `cvx github pr threads`.
        #[arg(value_name = "THREAD_ID")]
        thread_id: String,
    },
    /// Unresolve a review thread.
    Unresolve {
        /// Review thread node id from `cvx github pr threads`.
        #[arg(value_name = "THREAD_ID")]
        thread_id: String,
    },
}

/// GitHub label operations.
#[derive(Debug, Subcommand)]
pub enum GithubLabelCommand {
    /// List labels.
    List {
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Create a label.
    Create {
        /// Label name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Six-digit hex color.
        #[arg(long, default_value = "ededed", value_name = "HEX")]
        color: String,
        /// Label description.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Delete a label.
    Delete {
        /// Label name.
        #[arg(value_name = "NAME")]
        name: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Add labels to an issue or pull request.
    Add {
        /// Issue or pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Label names.
        #[arg(value_name = "LABEL", required = true)]
        labels: Vec<String>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Remove a label from an issue or pull request.
    Remove {
        /// Issue or pull-request number.
        #[arg(value_name = "NUMBER")]
        number: u64,
        /// Label name.
        #[arg(value_name = "LABEL")]
        label: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
}

/// GitHub release operations.
#[derive(Debug, Subcommand)]
pub enum GithubReleaseCommand {
    /// List releases.
    List {
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Create a release.
    Create {
        /// Tag name.
        #[arg(value_name = "TAG")]
        tag: String,
        /// Release name.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// Release notes.
        #[arg(short, long, value_name = "TEXT")]
        body: Option<String>,
        /// Create a draft release.
        #[arg(long)]
        draft: bool,
        /// Mark the release as a prerelease.
        #[arg(long)]
        prerelease: bool,
        /// File to upload as a release asset. May be repeated.
        #[arg(long = "asset", value_name = "PATH")]
        assets: Vec<PathBuf>,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// List assets attached to a release.
    Assets {
        /// Tag name.
        #[arg(value_name = "TAG")]
        tag: String,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Upload an asset to a release.
    Upload {
        /// Tag name.
        #[arg(value_name = "TAG")]
        tag: String,
        /// File to upload.
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Override the asset file name.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
        /// Asset label.
        #[arg(long, value_name = "LABEL")]
        label: Option<String>,
        /// Delete an existing asset with the same name first.
        #[arg(long)]
        clobber: bool,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
    /// Delete a release asset by id.
    DeleteAsset {
        /// Asset id.
        #[arg(value_name = "ASSET_ID")]
        asset_id: u64,
        /// Repository as owner/name or URL.
        #[arg(long, value_name = "OWNER/REPO")]
        repo: Option<String>,
    },
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

/// Object formats exposed on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ObjectFormatArg {
    /// SHA-1 object identifiers.
    Sha1,
    /// SHA-256 object identifiers.
    Sha256,
}

impl From<ObjectFormatArg> for cvx_repository::HashAlgorithm {
    fn from(value: ObjectFormatArg) -> Self {
        match value {
            ObjectFormatArg::Sha1 => Self::Sha1,
            ObjectFormatArg::Sha256 => Self::Sha256,
        }
    }
}
