use std::path::PathBuf;

use cvx_core::Result;
use cvx_repository::{CloneOptions, Repository};

use crate::cli::CloneArgs;

/// Executes `cvx clone`.
pub fn run(args: CloneArgs) -> Result<()> {
    let directory = args
        .directory
        .unwrap_or_else(|| default_directory(&args.url));
    println!("Cloning into '{}'...", directory.display());

    let outcome = Repository::clone_local(&CloneOptions {
        url: args.url,
        directory,
        branch: args.branch,
        storage: args.storage.into(),
        depth: args.depth,
        filter: args.filter,
        bare: args.bare,
    })?;

    if outcome.repository.is_bare() {
        println!(
            "Cloned bare repository at {} ({} objects)",
            outcome.head.short(7),
            outcome.object_count
        );
    } else {
        println!(
            "Checked out branch '{}' at {} ({} objects)",
            outcome.branch,
            outcome.head.short(7),
            outcome.object_count
        );
    }
    Ok(())
}

fn default_directory(url: &str) -> PathBuf {
    let trimmed = url.trim_end_matches(['/', '\\']);
    let name = trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("repository");
    let name = name
        .strip_suffix(".git")
        .or_else(|| name.strip_suffix(".cvx"))
        .unwrap_or(name);
    PathBuf::from(if name.is_empty() { "repository" } else { name })
}
