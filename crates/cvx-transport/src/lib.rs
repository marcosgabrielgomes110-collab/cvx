//! Git network transports.
//!
//! HTTPS uses the smart HTTP protocol with protocol v2 for fetch and
//! receive-pack for push. SSH delegates authentication and key management to
//! the system `ssh` client.

#![forbid(unsafe_code)]

mod http;
mod ssh;

use cvx_core::{Error, Result};
use cvx_object::ObjectId;

pub use http::{
    FetchResponse, FetchResponseFile, HttpTransport, LsRefsResponse, PushCommand, PushRefStatus,
    PushReport, ReceivePackAdvertisement, RemoteRef,
};
pub use ssh::SshTransport;

/// A network transport selected from a repository URL.
pub enum NetworkTransport {
    /// HTTP or HTTPS.
    Http(HttpTransport),
    /// SSH or SCP-like URL.
    Ssh(SshTransport),
}

impl NetworkTransport {
    /// Opens the transport appropriate for a repository URL.
    pub fn open(url: &str) -> Result<Self> {
        if url.starts_with("http://") || url.starts_with("https://") {
            return Ok(Self::Http(HttpTransport::open(url)?));
        }
        if url.starts_with("ssh://") || looks_like_scp(url) {
            return Ok(Self::Ssh(SshTransport::open(url)?));
        }
        Err(Error::unsupported(format!(
            "the transport for '{url}' is not implemented"
        )))
    }

    /// Returns the repository URL stored in configuration.
    pub fn base_url(&self) -> &str {
        match self {
            Self::Http(transport) => transport.base_url(),
            Self::Ssh(transport) => transport.base_url(),
        }
    }

    /// Discovers the server's object format.
    pub fn object_format(&self) -> Result<cvx_object::HashAlgorithm> {
        match self {
            Self::Http(transport) => transport.object_format(),
            Self::Ssh(transport) => transport.object_format(),
        }
    }

    /// Lists references through protocol v2 `ls-refs`.
    pub fn ls_refs(&self, prefixes: &[&str]) -> Result<LsRefsResponse> {
        match self {
            Self::Http(transport) => transport.ls_refs(prefixes),
            Self::Ssh(transport) => transport.ls_refs(prefixes),
        }
    }

    /// Lists references using a negotiated object format.
    pub fn ls_refs_with_algorithm(
        &self,
        prefixes: &[&str],
        algorithm: cvx_object::HashAlgorithm,
    ) -> Result<LsRefsResponse> {
        match self {
            Self::Http(transport) => transport.ls_refs_with_algorithm(prefixes, algorithm),
            Self::Ssh(transport) => transport.ls_refs_with_algorithm(prefixes, algorithm),
        }
    }

    /// Fetches a self-contained pack.
    pub fn fetch(&self, wants: &[ObjectId]) -> Result<Vec<u8>> {
        match self {
            Self::Http(transport) => transport.fetch(wants),
            Self::Ssh(transport) => transport.fetch(wants),
        }
    }

    /// Fetches with shallow depth and/or a partial-clone filter.
    pub fn fetch_full(
        &self,
        wants: &[ObjectId],
        deepen: Option<u32>,
        shallow: &[ObjectId],
        filter: Option<&str>,
    ) -> Result<FetchResponse> {
        match self {
            Self::Http(transport) => transport.fetch_full(wants, deepen, shallow, filter),
            Self::Ssh(transport) => transport.fetch_full(wants, deepen, shallow, filter),
        }
    }

    /// Fetches with an explicit object format and thin-pack mode.
    pub fn fetch_full_with_algorithm(
        &self,
        wants: &[ObjectId],
        deepen: Option<u32>,
        shallow: &[ObjectId],
        filter: Option<&str>,
        algorithm: cvx_object::HashAlgorithm,
        thin: bool,
    ) -> Result<FetchResponse> {
        match self {
            Self::Http(transport) => {
                transport.fetch_full_with_algorithm(wants, deepen, shallow, filter, algorithm, thin)
            }
            Self::Ssh(transport) => {
                transport.fetch_full_with_algorithm(wants, deepen, shallow, filter, algorithm, thin)
            }
        }
    }

    /// Fetches with the pack spooled to a temporary file.
    pub fn fetch_full_to_file(
        &self,
        wants: &[ObjectId],
        deepen: Option<u32>,
        shallow: &[ObjectId],
        filter: Option<&str>,
        algorithm: cvx_object::HashAlgorithm,
        thin: bool,
    ) -> Result<FetchResponseFile> {
        match self {
            Self::Http(transport) => {
                transport.fetch_full_to_file(wants, deepen, shallow, filter, algorithm, thin)
            }
            Self::Ssh(transport) => {
                transport.fetch_full_to_file(wants, deepen, shallow, filter, algorithm, thin)
            }
        }
    }

    /// Discovers receive-pack references.
    pub fn discover_receive_pack(&self) -> Result<ReceivePackAdvertisement> {
        match self {
            Self::Http(transport) => transport.discover_receive_pack(),
            Self::Ssh(transport) => transport.discover_receive_pack(),
        }
    }

    /// Pushes ref updates and a packfile.
    pub fn push(&self, updates: &[PushCommand], pack: &[u8]) -> Result<PushReport> {
        match self {
            Self::Http(transport) => transport.push(updates, pack),
            Self::Ssh(transport) => transport.push(updates, pack),
        }
    }
}

/// Returns whether a URL uses the SCP-like `user@host:path` form.
pub fn looks_like_scp(url: &str) -> bool {
    if url.contains("://") || url.starts_with('/') || url.starts_with("\\\\") {
        return false;
    }
    if url.len() > 2 && url.as_bytes()[1] == b':' && url.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    #[cfg(windows)]
    if std::path::Path::new(url).is_absolute() {
        return false;
    }
    url.split_once(':').is_some_and(|(authority, path)| {
        !authority.is_empty()
            && !authority.contains('/')
            && !path.is_empty()
            && !path.starts_with('/')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_scp_like_urls_without_mistaking_windows_paths() {
        assert!(looks_like_scp("git@github.com:owner/repo.git"));
        assert!(looks_like_scp("example.com:repo.git"));
        assert!(!looks_like_scp("C:\\repos\\test"));
        assert!(!looks_like_scp("\\\\?\\C:\\repos\\test"));
        assert!(!looks_like_scp("\\\\server\\share\\repo"));
        assert!(!looks_like_scp("/tmp/repo"));
        assert!(!looks_like_scp("https://github.com/owner/repo.git"));
        assert!(!looks_like_scp("relative/path"));
    }
}
