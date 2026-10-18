use cvx_core::{Error, Result};
use cvx_repository::{AddOptions, Repository};

use crate::cli::AddArgs;

/// Executes `cvx add`.
pub fn run(args: AddArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    let paths = if args.paths.is_empty() {
        if args.all || args.update {
            vec![repository.work_tree().to_path_buf()]
        } else {
            eprintln!("Nothing specified, nothing added.");
            return Ok(());
        }
    } else {
        args.paths
    };

    repository.add(AddOptions {
        paths,
        cwd,
        force: args.force,
        update: args.update,
        all: args.all,
    })?;

    Ok(())
}
