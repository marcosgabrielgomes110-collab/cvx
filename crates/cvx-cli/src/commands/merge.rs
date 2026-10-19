use cvx_core::{Error, Result};
use cvx_repository::{MergeOptions, MergeOutcome, Repository};

use crate::cli::MergeArgs;

/// Executes `cvx merge`.
pub fn run(args: MergeArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    if args.abort {
        if args.revision.is_some() {
            return Err(Error::Message(
                "a revision cannot be used with --abort".to_owned(),
            ));
        }
        repository.merge_abort()?;
        println!("Merge aborted.");
        return Ok(());
    }

    if args.continue_merge {
        if args.revision.is_some() {
            return Err(Error::Message(
                "a revision cannot be used with --continue".to_owned(),
            ));
        }
        let outcome = repository.merge_continue(args.message.as_deref())?;
        print_merged(&outcome);
        return Ok(());
    }

    let revision = args
        .revision
        .ok_or_else(|| Error::Message("a branch or revision is required".to_owned()))?;

    let outcome = repository.merge(&MergeOptions {
        revision,
        message: args.message,
    })?;

    match outcome {
        MergeOutcome::AlreadyUpToDate => println!("Already up to date."),
        MergeOutcome::FastForward { oid } => {
            println!("Fast-forward to {}", oid.short(7));
        }
        MergeOutcome::Merged { oid, subject } => {
            println!("[{}] {subject}", oid.short(7));
        }
        MergeOutcome::Conflicts { paths, .. } => {
            let mut message =
                String::from("Automatic merge failed; fix conflicts and then commit the result.");
            for path in paths {
                message.push_str("\n\tCONFLICT: ");
                message.push_str(&String::from_utf8_lossy(&path));
            }
            return Err(Error::merge_conflicts(message));
        }
    }

    Ok(())
}

fn print_merged(outcome: &MergeOutcome) {
    if let MergeOutcome::Merged { oid, subject } = outcome {
        println!("[{}] {subject}", oid.short(7));
    }
}
