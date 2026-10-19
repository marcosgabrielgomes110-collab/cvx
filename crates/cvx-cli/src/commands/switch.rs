use cvx_core::{Error, Result};
use cvx_repository::{Repository, SwitchOptions};

use crate::cli::SwitchArgs;

/// Executes `cvx switch`.
pub fn run(args: SwitchArgs) -> Result<()> {
    let target = args
        .target
        .ok_or_else(|| Error::Message("a branch or revision is required".to_owned()))?;
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    let outcome = repository.switch(&SwitchOptions {
        target: target.clone(),
        create: args.create,
        force: args.force,
        detach: args.detach,
    })?;

    if outcome.already {
        println!("Already on '{target}'");
    } else if outcome.detached {
        let short = outcome
            .oid
            .map(|oid| oid.short(7))
            .unwrap_or_else(|| "HEAD".to_owned());
        println!("HEAD is now at {short}");
    } else if outcome.created {
        println!(
            "Switched to a new branch '{}'",
            outcome.branch.as_deref().unwrap_or(&target)
        );
    } else {
        println!(
            "Switched to branch '{}'",
            outcome.branch.as_deref().unwrap_or(&target)
        );
    }

    Ok(())
}
