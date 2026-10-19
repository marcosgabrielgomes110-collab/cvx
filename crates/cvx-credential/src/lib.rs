//! Credential discovery and secure storage.
//!
//! Credentials are resolved from, in order:
//!
//! 1. `CVX_USERNAME` / `CVX_TOKEN` (or `GITHUB_TOKEN`) environment variables;
//! 2. the operating-system keychain;
//! 3. a user-only credential file outside the repository.
//!
//! The file is written with mode `0600` on Unix and is rejected on read if its
//! permissions allow group or other access. Tokens are never written to the
//! repository configuration or printed by this crate.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use cvx_core::IoResultExt;
use cvx_core::{Error, Result};

const KEYCHAIN_SERVICE: &str = "cvx";
const DEFAULT_USERNAME: &str = "x-access-token";

/// A username and secret pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    /// Username or token account name.
    pub username: String,
    /// Secret token or password.
    pub password: String,
}

/// Where a credential was stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialBackend {
    /// The operating-system keychain.
    Keychain,
    /// The user-only credential file.
    File,
}

/// User-level credential store keyed by host.
#[derive(Debug, Clone)]
pub struct CredentialStore {
    path: PathBuf,
    keychain: bool,
}

impl CredentialStore {
    /// Opens the default user credential store.
    ///
    /// `CVX_CREDENTIAL_FILE` overrides the file location. Set
    /// `CVX_DISABLE_KEYCHAIN=1` to force file-only storage.
    pub fn open_default() -> Result<Self> {
        let path = match env::var_os("CVX_CREDENTIAL_FILE").filter(|value| !value.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => config_dir()?.join("cvx").join("credentials"),
        };
        let keychain = env::var("CVX_DISABLE_KEYCHAIN")
            .map(|value| value != "1")
            .unwrap_or(true);
        Ok(Self { path, keychain })
    }

    /// Opens a file-only credential store at an explicit path.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            keychain: false,
        }
    }

    /// Opens a credential store with an explicit keychain preference.
    pub fn with_keychain(path: impl Into<PathBuf>, keychain: bool) -> Self {
        Self {
            path: path.into(),
            keychain,
        }
    }

    /// Returns the credential file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns whether the keychain backend is enabled.
    pub fn uses_keychain(&self) -> bool {
        self.keychain
    }

    /// Returns the credential for a host.
    pub fn get(&self, host: &str) -> Result<Option<Credential>> {
        validate_host(host)?;
        let entries = self.read_all()?;
        let stored = entries.get(host);

        if self.keychain {
            match keychain_get(host) {
                Ok(Some(password)) => {
                    let username = stored
                        .map(|credential| credential.username.clone())
                        .unwrap_or_else(default_username);
                    return Ok(Some(Credential { username, password }));
                }
                Ok(None) => {}
                Err(_) => {
                    if let Some(credential) =
                        stored.filter(|credential| !credential.password.is_empty())
                    {
                        return Ok(Some(credential.clone()));
                    }
                    return Ok(None);
                }
            }
        }

        Ok(stored
            .filter(|credential| !credential.password.is_empty())
            .cloned())
    }

    /// Stores or replaces a credential for a host.
    pub fn store(&self, host: &str, credential: &Credential) -> Result<CredentialBackend> {
        validate_host(host)?;
        validate_field(&credential.username)?;
        validate_field(&credential.password)?;

        let mut entries = self.read_all()?;
        let mut stored = credential.clone();

        if self.keychain && keychain_set(host, &credential.password).is_ok() {
            stored.password.clear();
            entries.insert(host.to_owned(), stored);
            self.write_all(&entries)?;
            return Ok(CredentialBackend::Keychain);
        }

        entries.insert(host.to_owned(), stored);
        self.write_all(&entries)?;
        Ok(CredentialBackend::File)
    }

    /// Removes a credential for a host, returning whether it existed.
    pub fn remove(&self, host: &str) -> Result<bool> {
        validate_host(host)?;
        let mut entries = self.read_all()?;
        let removed_file = entries.remove(host).is_some();
        let removed_keychain = if self.keychain {
            keychain_delete(host).unwrap_or(false)
        } else {
            false
        };
        if removed_file {
            self.write_all(&entries)?;
        }
        Ok(removed_file || removed_keychain)
    }

    /// Lists hosts with stored credentials.
    pub fn hosts(&self) -> Result<Vec<String>> {
        Ok(self.read_all()?.into_keys().collect())
    }

    fn read_all(&self) -> Result<BTreeMap<String, Credential>> {
        check_permissions(&self.path)?;
        let contents = match std::fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new());
            }
            Err(error) => return Err(Error::io(self.path.display().to_string(), error)),
        };

        let mut entries = BTreeMap::new();
        for (index, line) in contents.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 3 {
                return Err(Error::corrupt(format!(
                    "{} line {} is malformed",
                    self.path.display(),
                    index + 1
                )));
            }
            entries.insert(
                fields[0].to_owned(),
                Credential {
                    username: fields[1].to_owned(),
                    password: fields[2].to_owned(),
                },
            );
        }
        Ok(entries)
    }

    fn write_all(&self, entries: &BTreeMap<String, Credential>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            cvx_core::fs::ensure_dir(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                    .path_context(parent)?;
            }
        }

        let mut contents = String::new();
        for (host, credential) in entries {
            contents.push_str(host);
            contents.push('\t');
            contents.push_str(&credential.username);
            contents.push('\t');
            contents.push_str(&credential.password);
            contents.push('\n');
        }

        #[cfg(unix)]
        let mode = Some(0o600);
        #[cfg(not(unix))]
        let mode = None;
        cvx_core::fs::write_atomic(&self.path, contents.as_bytes(), mode)?;
        check_permissions(&self.path)
    }
}

/// Resolves credentials for a host from the environment or the default store.
pub fn resolve_for_host(host: &str) -> Result<Option<Credential>> {
    let token = env::var("CVX_TOKEN")
        .ok()
        .or_else(|| env::var("GITHUB_TOKEN").ok())
        .filter(|value| !value.is_empty());
    if let Some(token) = token {
        let username = env::var("CVX_USERNAME")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(default_username);
        return Ok(Some(Credential {
            username,
            password: token,
        }));
    }

    CredentialStore::open_default()?.get(host)
}

/// Parses Git credential-helper input into key/value pairs.
pub fn parse_helper_input(input: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in input.lines() {
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_owned(), value.to_owned());
        }
    }
    fields
}

fn default_username() -> String {
    env::var("CVX_USERNAME")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_USERNAME.to_owned())
}

fn keychain_entry(host: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, host)
        .map_err(|error| Error::Message(format!("keychain error: {error}")))
}

fn keychain_get(host: &str) -> Result<Option<String>> {
    let entry = keychain_entry(host)?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(Error::Message(format!("keychain error: {error}"))),
    }
}

fn keychain_set(host: &str, password: &str) -> Result<()> {
    keychain_entry(host)?
        .set_password(password)
        .map_err(|error| Error::Message(format!("keychain error: {error}")))
}

fn keychain_delete(host: &str) -> Result<bool> {
    let entry = keychain_entry(host)?;
    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(Error::Message(format!("keychain error: {error}"))),
    }
}

fn config_dir() -> Result<PathBuf> {
    if let Some(directory) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(directory));
    }
    #[cfg(windows)]
    if let Some(directory) = env::var_os("APPDATA").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(directory));
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::Message("cannot determine the user config directory".to_owned()))?;
    Ok(PathBuf::from(home).join(".config"))
}

fn validate_host(host: &str) -> Result<()> {
    if host.is_empty() || host.contains(['\t', '\n', '\r', '\0']) {
        return Err(Error::Message("invalid credential host".to_owned()));
    }
    Ok(())
}

fn validate_field(value: &str) -> Result<()> {
    if value.contains(['\t', '\n', '\r', '\0']) {
        return Err(Error::Message(
            "credential contains an invalid character".to_owned(),
        ));
    }
    Ok(())
}

fn check_permissions(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(path).path_context(path)?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(Error::Message(format!(
                "credential file {} has insecure permissions {mode:o}; run `chmod 600`",
                path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential() -> Credential {
        Credential {
            username: "x-access-token".to_owned(),
            password: "secret".to_owned(),
        }
    }

    #[test]
    fn stores_and_removes_credentials() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = CredentialStore::at(temp.path().join("config/credentials"));
        assert!(store.get("github.com").expect("get").is_none());

        assert_eq!(
            store.store("github.com", &credential()).expect("store"),
            CredentialBackend::File
        );
        assert_eq!(store.get("github.com").expect("get"), Some(credential()));
        assert_eq!(store.hosts().expect("hosts"), vec!["github.com"]);

        assert!(store.remove("github.com").expect("remove"));
        assert!(store.get("github.com").expect("get").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn writes_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("config/credentials");
        let store = CredentialStore::at(&path);
        store.store("github.com", &credential()).expect("store");

        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_insecure_files() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("credentials");
        std::fs::write(&path, "github.com\tuser\tsecret\n").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("permissions");

        let store = CredentialStore::at(&path);
        assert!(store.get("github.com").is_err());
    }

    #[test]
    fn rejects_malformed_records() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("credentials");
        std::fs::write(&path, "bad record\n").expect("write");
        let store = CredentialStore::at(&path);
        assert!(store.get("github.com").is_err());
    }

    #[test]
    fn parses_helper_input() {
        let fields = parse_helper_input(
            "protocol=https\nhost=github.com\nusername=x-access-token\npassword=secret\n\n",
        );
        assert_eq!(fields.get("host").map(String::as_str), Some("github.com"));
        assert_eq!(fields.get("password").map(String::as_str), Some("secret"));
    }
}
