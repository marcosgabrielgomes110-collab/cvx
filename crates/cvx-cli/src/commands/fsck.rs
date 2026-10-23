use cvx_core::{Error, Result};
use cvx_repository::{FsckOptions, Repository};

use crate::cli::FsckArgs;

/// Executes `cvx fsck`.
pub fn run(args: FsckArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;
    let report = repository.fsck_with_options(FsckOptions {
        repair: args.repair,
    })?;

    println!("Checked {} reachable objects", report.checked_objects);
    if !report.dangling.is_empty() {
        if args.strict {
            for oid in &report.dangling {
                eprintln!("dangling object {oid}");
            }
        } else {
            println!("Dangling objects: {}", report.dangling.len());
        }
    }
    for repair in &report.repairs {
        println!("Repaired: {repair}");
    }
    for error in &report.errors {
        eprintln!("error: {error}");
    }

    let dangling_failure = args.strict && !report.dangling.is_empty();
    if !report.is_clean() || dangling_failure {
        return Err(Error::integrity(format!(
            "repository has {} error(s)",
            report.errors.len() + usize::from(dangling_failure)
        )));
    }
    println!("Repository is healthy");
    Ok(())
}
