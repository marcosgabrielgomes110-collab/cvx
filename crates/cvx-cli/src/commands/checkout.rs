use cvx_core::{Error, Result};
use cvx_repository::{Repository, RestoreOptions};

use crate::cli::CheckoutArgs;

/// Executes `cvx checkout -- <paths>`.
pub fn run(args: CheckoutArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    repository.restore(&RestoreOptions {
        staged: false,
        worktree: true,
        source: None,
        paths: args.paths,
        cwd,
        force: false,
    })?;

    Ok(())
}
