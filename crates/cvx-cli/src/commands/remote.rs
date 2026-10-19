use cvx_core::{Error, Result};
use cvx_repository::Repository;

use crate::cli::{RemoteArgs, RemoteCommand};

/// Executes `cvx remote`.
pub fn run(args: RemoteArgs) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|error| Error::io("current directory", error))?;
    let repository = Repository::discover(&cwd)?;

    match args.command {
        None | Some(RemoteCommand::List) => {
            for remote in repository.remotes()? {
                println!("{}\t{}", remote.name, remote.url);
            }
        }
        Some(RemoteCommand::Add { name, url }) => {
            let remote = repository.add_remote(&name, &url)?;
            println!("Added remote '{}' ({})", remote.name, remote.url);
        }
    }

    Ok(())
}
