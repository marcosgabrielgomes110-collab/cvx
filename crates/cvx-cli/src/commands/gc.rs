use cvx_core::{Error, Result};
use cvx_repository::Repository;

use crate::cli::GcArgs;

/// Executes `cvx gc`.
pub fn run(args: GcArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;
    let report = repository.gc(args.prune)?;

    if !args.quiet {
        match &report.pack_path {
            Some(path) => println!(
                "Packed {} objects into {}",
                report.packed_objects,
                path.display()
            ),
            None => println!("No loose objects to pack"),
        }
        if args.prune {
            println!("Pruned {} unreachable loose objects", report.pruned_objects);
        }
    }
    Ok(())
}
