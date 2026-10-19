use cvx_core::{Error, Result};
use cvx_repository::Repository;

use crate::cli::FetchArgs;

/// Executes `cvx fetch`.
pub fn run(args: FetchArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    let outcome = repository.fetch(args.remote.as_deref())?;
    for (reference, oid) in &outcome.updated {
        println!(" * [updated] {:<40} {}", reference, oid.short(7));
    }
    println!(
        "Fetched {} objects and {} tags from '{}'",
        outcome.objects, outcome.tags, outcome.remote
    );
    Ok(())
}
