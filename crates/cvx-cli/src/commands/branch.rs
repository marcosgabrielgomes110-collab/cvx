use cvx_core::{Error, Result};
use cvx_repository::Repository;

use crate::cli::BranchArgs;

/// Executes `cvx branch`.
pub fn run(args: BranchArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    if args.delete || args.force_delete {
        let name = args
            .name
            .ok_or_else(|| Error::Message("branch name is required for deletion".to_owned()))?;
        if args.start_point.is_some() {
            return Err(Error::Message(
                "a start point cannot be used with --delete".to_owned(),
            ));
        }
        let reference = format!("refs/heads/{name}");
        let old = repository.refs().read_ref(&reference)?;
        repository.delete_branch(&name, args.force_delete)?;
        if let Some(oid) = old {
            println!("Deleted branch {name} (was {}).", oid.short(7));
        }
        return Ok(());
    }

    if args.list || args.name.is_none() {
        if args.start_point.is_some() {
            return Err(Error::Message(
                "a start point requires a branch name".to_owned(),
            ));
        }
        for branch in repository.branches()? {
            let marker = if branch.current { "* " } else { "  " };
            println!("{marker}{}", branch.name);
        }
        return Ok(());
    }

    let name = args.name.expect("checked above");
    repository.create_branch(&name, args.start_point.as_deref())?;
    Ok(())
}
