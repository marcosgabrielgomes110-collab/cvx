use cvx_core::{Error, Result};
use cvx_repository::{Repository, TagOptions};

use crate::cli::TagArgs;

/// Executes `cvx tag`.
pub fn run(args: TagArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    if args.list || args.name.is_none() {
        if args.target.is_some() || args.message.is_some() || args.annotate || args.force {
            return Err(Error::Message(
                "tag creation options cannot be used with --list".to_owned(),
            ));
        }
        for (name, _) in repository.tags()? {
            println!("{name}");
        }
        return Ok(());
    }

    let name = args.name.expect("checked above");
    let outcome = repository.create_tag(&TagOptions {
        name,
        target: args.target,
        message: args.message,
        annotated: args.annotate,
        force: args.force,
    })?;

    if outcome.annotated {
        println!(
            "Created annotated tag {} ({})",
            outcome.name,
            outcome.oid.short(7)
        );
    }

    Ok(())
}
