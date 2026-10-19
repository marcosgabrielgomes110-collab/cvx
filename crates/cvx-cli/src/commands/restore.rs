use cvx_core::{Error, Result};
use cvx_repository::{Repository, RestoreOptions};

use crate::cli::RestoreArgs;

/// Executes `cvx restore`.
pub fn run(args: RestoreArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    repository.restore(&RestoreOptions {
        staged: args.staged,
        worktree: args.worktree,
        source: args.source,
        paths: args.paths,
        cwd,
        force: args.force,
    })?;

    Ok(())
}
