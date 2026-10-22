use cvx_core::{Error, Result};
use cvx_repository::{PickOptions, Repository};

use crate::cli::PickArgs;

/// Executes `cvx revert`.
pub fn run(args: PickArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;
    let outcome = repository.revert(&PickOptions {
        revision: args.revision,
    })?;
    let branch = outcome.branch.as_deref().unwrap_or("detached HEAD");
    println!("[{branch} {}] {}", outcome.oid.short(7), outcome.subject);
    Ok(())
}
