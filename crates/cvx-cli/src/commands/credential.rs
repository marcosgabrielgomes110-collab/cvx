use std::io::Read;

use cvx_core::{Error, Result};
use cvx_credential::{Credential, CredentialStore};

use crate::cli::{CredentialArgs, CredentialCommand};

/// Executes `cvx credential`.
pub fn run(args: CredentialArgs) -> Result<()> {
    let store = CredentialStore::open_default()?;

    match args.command {
        CredentialCommand::Store { host, username } => {
            let mut token = String::new();
            std::io::stdin()
                .read_to_string(&mut token)
                .map_err(|error| Error::io("failed to read credential from stdin", error))?;
            let token = token.trim().to_owned();
            if token.is_empty() {
                return Err(Error::Message(
                    "no credential was provided on standard input".to_owned(),
                ));
            }
            store.store(
                &host,
                &Credential {
                    username,
                    password: token,
                },
            )?;
            println!("Stored credential for {host} in {}", store.path().display());
        }
        CredentialCommand::List => {
            for host in store.hosts()? {
                println!("{host}");
            }
        }
        CredentialCommand::Remove { host } => {
            if store.remove(&host)? {
                println!("Removed credential for {host}");
            } else {
                println!("No credential stored for {host}");
            }
        }
    }

    Ok(())
}
