use cvx_core::{Error, Result};
use cvx_repository::{Repository, StashPushOptions};

use crate::cli::{StashAction, StashArgs};

/// Executes `cvx stash`.
pub fn run(args: StashArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    match args.action {
        StashAction::Push(push) => {
            let outcome = repository.stash_push(&StashPushOptions {
                message: push.message,
            })?;
            println!(
                "Saved working directory and index state {}",
                outcome.message.trim_end()
            );
        }
        StashAction::List => {
            for (index, entry) in repository.stash_list()?.iter().enumerate() {
                println!(
                    "stash@{{{index}}}: {} ({})",
                    entry.message,
                    entry.oid.short(7)
                );
            }
        }
        StashAction::Apply => {
            let outcome = repository.stash_apply()?;
            println!("Applied stash@{{0}} ({})", outcome.oid.short(7));
        }
        StashAction::Pop => {
            let outcome = repository.stash_pop()?;
            println!("Applied and dropped stash@{{0}} ({})", outcome.oid.short(7));
        }
        StashAction::Drop => {
            let outcome = repository.stash_drop()?;
            println!("Dropped stash@{{0}} ({})", outcome.oid.short(7));
        }
    }
    Ok(())
}
