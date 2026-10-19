use std::collections::BTreeMap;
use std::io::Read;

use cvx_core::{Error, Result};
use cvx_credential::{
    Credential, CredentialBackend, CredentialStore, parse_helper_input, resolve_for_host,
};

use crate::cli::{CredentialArgs, CredentialCommand};

/// Executes `cvx credential`.
pub fn run(args: CredentialArgs) -> Result<()> {
    match args.command {
        CredentialCommand::Store { host, username } => {
            let token = read_token()?;
            let store = CredentialStore::open_default()?;
            let backend = store.store(
                &host,
                &Credential {
                    username,
                    password: token,
                },
            )?;
            let location = match backend {
                CredentialBackend::Keychain => "the operating-system keychain".to_owned(),
                CredentialBackend::File => store.path().display().to_string(),
            };
            println!("Stored credential for {host} in {location}");
        }
        CredentialCommand::List => {
            for host in CredentialStore::open_default()?.hosts()? {
                println!("{host}");
            }
        }
        CredentialCommand::Remove { host } => {
            if CredentialStore::open_default()?.remove(&host)? {
                println!("Removed credential for {host}");
            } else {
                println!("No credential stored for {host}");
            }
        }
        CredentialCommand::Fill => {
            let fields = read_fields()?;
            let host = fields.get("host").ok_or_else(|| {
                Error::Message("credential input is missing the host field".to_owned())
            })?;
            if let Some(credential) = resolve_for_host(host)? {
                println!("username={}", credential.username);
                println!("password={}", credential.password);
            }
        }
        CredentialCommand::Approve => {
            let fields = read_fields()?;
            let host = fields.get("host").ok_or_else(|| {
                Error::Message("credential input is missing the host field".to_owned())
            })?;
            let username = fields
                .get("username")
                .cloned()
                .unwrap_or_else(|| "x-access-token".to_owned());
            let password = fields.get("password").ok_or_else(|| {
                Error::Message("credential input is missing the password field".to_owned())
            })?;
            CredentialStore::open_default()?.store(
                host,
                &Credential {
                    username,
                    password: password.clone(),
                },
            )?;
        }
        CredentialCommand::Reject => {
            let fields = read_fields()?;
            if let Some(host) = fields.get("host") {
                CredentialStore::open_default()?.remove(host)?;
            }
        }
    }

    Ok(())
}

fn read_token() -> Result<String> {
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
    Ok(token)
}

fn read_fields() -> Result<BTreeMap<String, String>> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| Error::io("failed to read credential input", error))?;
    Ok(parse_helper_input(&input))
}
