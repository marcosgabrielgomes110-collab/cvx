use cvx_core::{Error, Result};
use cvx_repository::{Repository, ResetMode, ResetOptions};

use crate::cli::ResetArgs;

/// Executes `cvx reset`.
pub fn run(args: ResetArgs) -> Result<()> {
    let mode = if args.soft {
        ResetMode::Soft
    } else if args.hard {
        ResetMode::Hard
    } else {
        ResetMode::Mixed
    };

    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;
    let outcome = repository.reset(&ResetOptions {
        target: args.target.clone(),
        mode,
    })?;

    let short = outcome.target.short(7);
    if mode == ResetMode::Hard {
        let subject = repository
            .read_commit(outcome.target)
            .map(|commit| commit.subject().to_owned())
            .unwrap_or_default();
        println!("HEAD is now at {short} {subject}");
    } else {
        println!("Reset to {short}");
    }
    Ok(())
}
