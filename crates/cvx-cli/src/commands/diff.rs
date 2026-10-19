use cvx_core::{Error, Result};
use cvx_repository::{DiffOptions, Repository};

use crate::cli::DiffArgs;

/// Executes `cvx diff`.
pub fn run(args: DiffArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    let output = repository.diff(&DiffOptions {
        cached: args.cached,
        stat: args.stat,
        context: args.context,
        revisions: args.revisions,
        paths: args.paths,
        cwd,
    })?;

    print!("{output}");
    Ok(())
}
