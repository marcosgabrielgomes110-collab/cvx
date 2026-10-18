use cvx_core::{Error, Result};
use cvx_repository::{CommitOptions, Repository};

use crate::cli::CommitArgs;

/// Executes `cvx commit`.
pub fn run(args: CommitArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    let outcome = repository.commit(CommitOptions {
        messages: args.message,
        allow_empty: args.allow_empty,
    })?;

    let branch = outcome.branch.as_deref().unwrap_or("detached HEAD");
    let root = if outcome.root { " (root-commit)" } else { "" };
    println!(
        "[{branch}{root} {}] {}",
        outcome.oid.short(7),
        outcome.subject
    );

    Ok(())
}
