use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use cvx_core::{Error, Result};
use cvx_object::ObjectId;
use cvx_protocol::{
    FetchRequest, LsRefsRequest, PktLine, encode_data, encode_text, flush, parse_all, read_from,
};

use crate::http::{
    FetchResponse, LsRefsResponse, PushCommand, PushReport, ReceivePackAdvertisement,
    parse_fetch_response, parse_ls_refs, parse_push_response, parse_receive_advertisement,
};

/// SSH transport that delegates authentication to the system `ssh` client.
///
/// Keys and `ssh-agent` are handled by OpenSSH. `CVX_SSH` overrides the client
/// binary and `CVX_SSH_KEY` adds an explicit identity file. Host keys use
/// `StrictHostKeyChecking=accept-new` by default.
pub struct SshTransport {
    original: String,
    user: String,
    host: String,
    port: Option<u16>,
    path: String,
    ssh_command: String,
    key: Option<String>,
}

impl SshTransport {
    /// Parses an SSH repository URL.
    pub fn open(url: &str) -> Result<Self> {
        let ssh_command = std::env::var("CVX_SSH").unwrap_or_else(|_| "ssh".to_owned());
        let key = std::env::var("CVX_SSH_KEY")
            .ok()
            .filter(|value| !value.is_empty());

        if let Some(rest) = url.strip_prefix("ssh://") {
            let (authority, path) = rest
                .split_once('/')
                .ok_or_else(|| Error::Message(format!("invalid SSH URL: {url}")))?;
            let (user, host, port) = parse_authority(authority, "git")?;
            return Ok(Self {
                original: url.to_owned(),
                user,
                host,
                port,
                path: format!("/{path}"),
                ssh_command,
                key,
            });
        }

        if let Some((authority, path)) = split_scp_like(url) {
            let (user, host, port) = parse_authority(authority, "git")?;
            return Ok(Self {
                original: url.to_owned(),
                user,
                host,
                port,
                path: path.to_owned(),
                ssh_command,
                key,
            });
        }

        Err(Error::unsupported(format!(
            "invalid SSH repository URL: {url}"
        )))
    }

    /// Returns the configured repository URL.
    pub fn base_url(&self) -> &str {
        &self.original
    }

    /// Lists references using protocol v2 `ls-refs`.
    pub fn ls_refs(&self, prefixes: &[&str]) -> Result<LsRefsResponse> {
        let mut session = self.spawn("git-upload-pack")?;
        session.handshake()?;
        let request = LsRefsRequest {
            ref_prefixes: prefixes.iter().map(|prefix| (*prefix).to_owned()).collect(),
            symrefs: true,
            peel: true,
        };
        session.write(&request.encode()?)?;
        let response = session.read_until_flush()?;
        session.close_stdin();
        session.finish()?;
        parse_ls_refs(&response)
    }

    /// Fetches a self-contained pack for the requested objects.
    pub fn fetch(&self, wants: &[ObjectId]) -> Result<Vec<u8>> {
        Ok(self.fetch_full(wants, None, &[], None)?.pack)
    }

    /// Fetches with shallow depth and/or a partial-clone filter.
    pub fn fetch_full(
        &self,
        wants: &[ObjectId],
        deepen: Option<u32>,
        shallow: &[ObjectId],
        filter: Option<&str>,
    ) -> Result<FetchResponse> {
        let mut session = self.spawn("git-upload-pack")?;
        session.handshake()?;
        let request = FetchRequest {
            wants: wants.to_vec(),
            haves: Vec::new(),
            done: true,
            thin: false,
            no_progress: true,
            include_tag: true,
            ofs_delta: true,
            deepen,
            shallow: shallow.to_vec(),
            filter: filter.map(str::to_owned),
        };
        session.write(&request.encode()?)?;
        session.close_stdin();
        let response = session.read_to_end()?;
        let _ = session.finish();
        parse_fetch_response(&response)
    }

    /// Discovers receive-pack references.
    pub fn discover_receive_pack(&self) -> Result<ReceivePackAdvertisement> {
        let mut session = self.spawn("git-receive-pack")?;
        let advertisement = session.read_until_flush()?;
        session.close_stdin();
        session.finish()?;
        parse_receive_advertisement(&advertisement)
    }

    /// Pushes ref updates and a packfile.
    pub fn push(&self, updates: &[PushCommand], pack: &[u8]) -> Result<PushReport> {
        if updates.is_empty() {
            return Err(Error::Message("no ref updates were requested".to_owned()));
        }
        let mut session = self.spawn("git-receive-pack")?;
        let advertisement_bytes = session.read_until_flush()?;
        let advertisement = parse_receive_advertisement(&advertisement_bytes)?;

        let mut capabilities = vec!["report-status".to_owned()];
        if advertisement.capabilities.contains_key("side-band-64k") {
            capabilities.push("side-band-64k".to_owned());
        }
        if advertisement.capabilities.contains_key("ofs-delta") {
            capabilities.push("ofs-delta".to_owned());
        }
        capabilities.push(format!("agent={}", cvx_protocol::AGENT));

        let mut body = Vec::new();
        for (index, update) in updates.iter().enumerate() {
            let old = update.old.unwrap_or(ObjectId::ZERO);
            let mut command = format!("{old} {} {}", update.new, update.reference).into_bytes();
            if index == 0 {
                command.push(0);
                command.extend_from_slice(capabilities.join(" ").as_bytes());
            }
            body.extend(encode_data(&command)?);
        }
        body.extend(flush());
        body.extend_from_slice(pack);
        session.write(&body)?;
        session.close_stdin();
        let response = session.read_to_end()?;
        let _ = session.finish();
        parse_push_response(&response)
    }

    fn spawn(&self, service: &str) -> Result<SshSession> {
        let mut command = Command::new(&self.ssh_command);
        command
            .arg("-T")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("SendEnv=GIT_PROTOCOL")
            .env("GIT_PROTOCOL", "version=2");
        if let Some(port) = self.port {
            command.arg("-p").arg(port.to_string());
        }
        if let Some(key) = &self.key {
            command.arg("-i").arg(key);
        }
        command
            .arg(format!("{}@{}", self.user, self.host))
            .arg(format!("{service} '{}'", shell_quote(&self.path)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|error| Error::Message(format!("failed to start ssh: {error}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Message("ssh stdin is unavailable".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Message("ssh stdout is unavailable".to_owned()))?;
        Ok(SshSession {
            child,
            stdin: Some(stdin),
            stdout,
        })
    }
}

struct SshSession {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
}

impl SshSession {
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Message("ssh stdin is closed".to_owned()))?;
        stdin
            .write_all(bytes)
            .map_err(|error| Error::io("failed to write to ssh", error))?;
        stdin
            .flush()
            .map_err(|error| Error::io("failed to flush ssh input", error))
    }

    fn close_stdin(&mut self) {
        self.stdin.take();
    }

    fn handshake(&mut self) -> Result<()> {
        // GIT_PROTOCOL=version=2 is forwarded through OpenSSH's SendEnv, so
        // servers normally advertise version 2 immediately. Fall back to an
        // explicit version packet for servers that expect the stateful form.
        let response = self.read_until_flush()?;
        if has_version_2(&response)? {
            return Ok(());
        }

        self.write(&encode_text("version=2\n")?)?;
        let response = self.read_until_flush()?;
        if has_version_2(&response)? {
            Ok(())
        } else {
            Err(Error::unsupported(
                "the SSH server does not support Git protocol v2",
            ))
        }
    }

    fn read_until_flush(&mut self) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        loop {
            match read_from(&mut self.stdout)? {
                Some(packet) => {
                    output.extend(packet.encode()?);
                    if packet == PktLine::Flush {
                        return Ok(output);
                    }
                }
                None => {
                    return Err(Error::Message(
                        "SSH server closed the connection before a flush packet".to_owned(),
                    ));
                }
            }
        }
    }

    fn read_to_end(&mut self) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        self.stdout
            .read_to_end(&mut output)
            .map_err(|error| Error::io("failed to read SSH response", error))?;
        Ok(output)
    }

    fn finish(mut self) -> Result<()> {
        let mut stderr = String::new();
        if let Some(mut handle) = self.child.stderr.take() {
            let _ = handle.read_to_string(&mut stderr);
        }
        let status = self
            .child
            .wait()
            .map_err(|error| Error::Message(format!("failed to wait for ssh: {error}")))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Message(format!(
                "ssh exited with {status}: {}",
                stderr.trim()
            )))
        }
    }
}

fn has_version_2(bytes: &[u8]) -> Result<bool> {
    for packet in parse_all(bytes)? {
        if let PktLine::Data(data) = packet {
            let line = data.strip_suffix(b"\n").unwrap_or(&data);
            if line == b"version 2" {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn parse_authority(authority: &str, default_user: &str) -> Result<(String, String, Option<u16>)> {
    let (user, host_port) = match authority.rsplit_once('@') {
        Some((user, host)) => (user.to_owned(), host),
        None => (default_user.to_owned(), authority),
    };
    if host_port.is_empty() {
        return Err(Error::Message("SSH URL is missing a host".to_owned()));
    }
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            let port = port
                .parse::<u16>()
                .map_err(|_| Error::Message(format!("invalid SSH port: {port}")))?;
            (host.to_owned(), Some(port))
        }
        _ => (host_port.to_owned(), None),
    };
    Ok((user, host, port))
}

fn split_scp_like(url: &str) -> Option<(&str, &str)> {
    if url.contains("://") || url.starts_with('/') {
        return None;
    }
    if url.len() > 2 && url.as_bytes()[1] == b':' && url.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let (authority, path) = url.split_once(':')?;
    if authority.is_empty() || path.is_empty() || path.starts_with('/') {
        return None;
    }
    if authority.contains('/') {
        return None;
    }
    Some((authority, path))
}

fn shell_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_urls() {
        let transport = SshTransport::open("ssh://git@github.com/owner/repo.git").expect("ssh url");
        assert_eq!(transport.user, "git");
        assert_eq!(transport.host, "github.com");
        assert_eq!(transport.path, "/owner/repo.git");

        let transport = SshTransport::open("git@github.com:owner/repo.git").expect("scp url");
        assert_eq!(transport.user, "git");
        assert_eq!(transport.host, "github.com");
        assert_eq!(transport.path, "owner/repo.git");

        let transport = SshTransport::open("ssh://git@example.com:2222/repo.git").expect("port");
        assert_eq!(transport.port, Some(2222));

        assert!(SshTransport::open("C:\\repos\\test").is_err());
        assert!(SshTransport::open("relative/path").is_err());
    }

    #[test]
    fn quotes_remote_paths() {
        assert_eq!(shell_quote("owner/repo.git"), "owner/repo.git");
        assert_eq!(shell_quote("it's/repo.git"), "it'\\''s/repo.git");
    }
}
