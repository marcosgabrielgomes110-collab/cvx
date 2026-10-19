use cvx_core::{Error, Result};
use cvx_repository::{PushOptions, Repository};

use crate::cli::PushArgs;

/// Executes `cvx push`.
pub fn run(args: PushArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    let outcome = repository.push(&PushOptions {
        remote: args.remote,
        branch: args.branch,
        force: args.force,
    })?;

    let old = outcome
        .old
        .map(|oid| oid.short(7))
        .unwrap_or_else(|| "0000000".to_owned());
    println!(
        "To {}: {} -> {} ({} objects)",
        outcome.remote,
        old,
        outcome.new.short(7),
        outcome.objects
    );
    println!("   {} -> {}", outcome.branch, outcome.branch);
    Ok(())
}
