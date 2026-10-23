use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use cvx_core::{Error, Result};
use cvx_credential::resolve_for_host;
use cvx_object::{HashAlgorithm, ObjectId};
use cvx_protocol::{
    AGENT, FetchRequest, LsRefsRequest, PktLine, encode_data, flush, parse_all, read_from,
};

const MAX_BODY: u64 = 2 * 1024 * 1024 * 1024;

/// A reference advertised by a remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRef {
    /// Object identifier.
    pub oid: ObjectId,
    /// Full reference name, such as `refs/heads/main` or `HEAD`.
    pub name: String,
    /// Peeled target for annotated tags.
    pub peeled: Option<ObjectId>,
    /// Symbolic target for symbolic references.
    pub symref: Option<String>,
}

/// Fetch response whose pack is spooled to a temporary file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponseFile {
    /// Temporary `.pack` file. The caller owns and must remove it.
    pub pack_path: PathBuf,
    /// Shallow boundaries reported by the server.
    pub shallow: Vec<ObjectId>,
    /// Commits that are no longer shallow.
    pub unshallow: Vec<ObjectId>,
}

/// Parsed fetch response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchResponse {
    /// Packfile bytes.
    pub pack: Vec<u8>,
    /// Shallow boundaries reported by the server.
    pub shallow: Vec<ObjectId>,
    /// Commits that are no longer shallow.
    pub unshallow: Vec<ObjectId>,
}

/// Parsed `ls-refs` response.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LsRefsResponse {
    /// Advertised references, excluding `HEAD`.
    pub refs: Vec<RemoteRef>,
    /// Target of `HEAD`, when advertised.
    pub head_symref: Option<String>,
}

/// Parsed receive-pack advertisement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReceivePackAdvertisement {
    /// Existing references.
    pub refs: Vec<RemoteRef>,
    /// Capabilities accepted by the server.
    pub capabilities: BTreeMap<String, Option<String>>,
}

/// One ref update sent to `git-receive-pack`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushCommand {
    /// Previous remote value, or `None` for a new reference.
    pub old: Option<ObjectId>,
    /// New value.
    pub new: ObjectId,
    /// Full reference name.
    pub reference: String,
}

/// Result status for one pushed reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRefStatus {
    /// Reference name.
    pub reference: String,
    /// Whether the server accepted the update.
    pub ok: bool,
    /// Server explanation, usually empty on success.
    pub message: String,
}

/// Parsed receive-pack result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushReport {
    /// Whether the server unpacked the packfile.
    pub unpack_ok: bool,
    /// Per-reference status.
    pub refs: Vec<PushRefStatus>,
    /// Sideband progress text emitted by the server.
    pub messages: String,
}

/// HTTPS smart Git transport.
pub struct HttpTransport {
    base: String,
    host: String,
    auth: Option<String>,
    agent: ureq::Agent,
    retries: u32,
}

impl HttpTransport {
    /// Opens an HTTP(S) transport for a repository URL.
    pub fn open(url: &str) -> Result<Self> {
        let parsed = ParsedUrl::parse(url)?;
        let credential = resolve_for_host(&parsed.host)?;
        if parsed.base.starts_with("http://") && credential.is_some() {
            return Err(Error::Message(
                "refusing to send credentials over plaintext HTTP".to_owned(),
            ));
        }
        let auth = credential.map(|credential| {
            format!(
                "Basic {}",
                BASE64.encode(format!("{}:{}", credential.username, credential.password))
            )
        });
        let timeout = std::env::var("CVX_HTTP_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(120);
        let retries = std::env::var("CVX_HTTP_RETRIES")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(2);
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(timeout))
            .redirects(0)
            .user_agent(AGENT)
            .build();
        Ok(Self {
            base: parsed.base,
            host: parsed.host,
            auth,
            agent,
            retries,
        })
    }

    /// Returns the repository base URL, without credentials.
    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// Returns the host used for credential lookup.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Discovers upload-pack capabilities.
    pub fn discover_upload_pack(&self) -> Result<BTreeMap<String, Option<String>>> {
        let url = format!("{}/info/refs?service=git-upload-pack", self.base);
        let response = self.get(&url, true)?;
        parse_advertisement_capabilities(&response)
    }

    /// Discovers the server's advertised object format.
    pub fn object_format(&self) -> Result<HashAlgorithm> {
        let capabilities = self.discover_upload_pack()?;
        match capabilities
            .get("object-format")
            .and_then(|value| value.as_deref())
        {
            Some(format) => {
                HashAlgorithm::parse(format).map_err(|error| Error::unsupported(error.to_string()))
            }
            None => Ok(HashAlgorithm::Sha1),
        }
    }

    /// Lists references through protocol v2 `ls-refs`.
    pub fn ls_refs(&self, prefixes: &[&str]) -> Result<LsRefsResponse> {
        self.ls_refs_with_algorithm(prefixes, HashAlgorithm::Sha1)
    }

    /// Lists references using the negotiated object format.
    pub fn ls_refs_with_algorithm(
        &self,
        prefixes: &[&str],
        algorithm: HashAlgorithm,
    ) -> Result<LsRefsResponse> {
        let request = LsRefsRequest {
            ref_prefixes: prefixes.iter().map(|prefix| (*prefix).to_owned()).collect(),
            symrefs: true,
            peel: true,
            object_format: algorithm,
        };
        let body = request.encode()?;
        let url = format!("{}/git-upload-pack", self.base);
        let response = self.post(&url, "application/x-git-upload-pack-request", &body, true)?;
        parse_ls_refs(&response)
    }

    /// Fetches a self-contained pack for the requested object identifiers.
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
        self.fetch_full_with_algorithm(wants, deepen, shallow, filter, HashAlgorithm::Sha1, false)
    }

    /// Fetches using the negotiated object format and requested thin-pack mode.
    pub fn fetch_full_with_algorithm(
        &self,
        wants: &[ObjectId],
        deepen: Option<u32>,
        shallow: &[ObjectId],
        filter: Option<&str>,
        algorithm: HashAlgorithm,
        thin: bool,
    ) -> Result<FetchResponse> {
        let request = FetchRequest {
            wants: wants.to_vec(),
            haves: Vec::new(),
            done: true,
            thin,
            no_progress: true,
            include_tag: true,
            ofs_delta: true,
            deepen,
            shallow: shallow.to_vec(),
            filter: filter.map(str::to_owned),
            object_format: algorithm,
        };
        let body = request.encode()?;
        let url = format!("{}/git-upload-pack", self.base);
        let response = self.post(&url, "application/x-git-upload-pack-request", &body, true)?;
        parse_fetch_response(&response)
    }

    /// Fetches with the pack spooled to a temporary file.
    pub fn fetch_full_to_file(
        &self,
        wants: &[ObjectId],
        deepen: Option<u32>,
        shallow: &[ObjectId],
        filter: Option<&str>,
        algorithm: HashAlgorithm,
        thin: bool,
    ) -> Result<FetchResponseFile> {
        let request = FetchRequest {
            wants: wants.to_vec(),
            haves: Vec::new(),
            done: true,
            thin,
            no_progress: true,
            include_tag: true,
            ofs_delta: true,
            deepen,
            shallow: shallow.to_vec(),
            filter: filter.map(str::to_owned),
            object_format: algorithm,
        };
        let body = request.encode()?;
        let url = format!("{}/git-upload-pack", self.base);
        let reader =
            self.post_stream(&url, "application/x-git-upload-pack-request", &body, true)?;
        parse_fetch_response_to_file(BufReader::new(reader))
    }

    /// Discovers receive-pack references and capabilities.
    pub fn discover_receive_pack(&self) -> Result<ReceivePackAdvertisement> {
        let url = format!("{}/info/refs?service=git-receive-pack", self.base);
        let response = self.get(&url, false)?;
        parse_receive_advertisement(&response)
    }

    /// Pushes ref updates together with a packfile.
    pub fn push(&self, updates: &[PushCommand], pack: &[u8]) -> Result<PushReport> {
        if updates.is_empty() {
            return Err(Error::Message("no ref updates were requested".to_owned()));
        }
        let advertisement = self.discover_receive_pack()?;
        let algorithm = updates
            .first()
            .map(|update| update.new.algorithm())
            .unwrap_or_default();
        if algorithm != HashAlgorithm::Sha1 {
            let advertised = advertisement
                .capabilities
                .get("object-format")
                .and_then(|value| value.as_deref());
            if advertised != Some(algorithm.name()) {
                return Err(Error::unsupported(format!(
                    "the remote does not support {} pushes",
                    algorithm.name()
                )));
            }
        }
        let mut capabilities = vec!["report-status".to_owned()];
        if algorithm != HashAlgorithm::Sha1 {
            capabilities.push(format!("object-format={}", algorithm.name()));
        }
        if advertisement.capabilities.contains_key("side-band-64k") {
            capabilities.push("side-band-64k".to_owned());
        }
        if advertisement.capabilities.contains_key("ofs-delta") {
            capabilities.push("ofs-delta".to_owned());
        }
        if advertisement.capabilities.contains_key("thin-pack") {
            capabilities.push("thin-pack".to_owned());
        }
        capabilities.push(format!("agent={AGENT}"));

        let mut body = Vec::new();
        for (index, update) in updates.iter().enumerate() {
            let old = update.old.unwrap_or_else(|| ObjectId::zero(algorithm));
            let mut command = format!("{old} {} {}", update.new, update.reference).into_bytes();
            if index == 0 {
                command.push(0);
                command.extend_from_slice(capabilities.join(" ").as_bytes());
            }
            body.extend(encode_data(&command)?);
        }
        body.extend(flush());
        body.extend_from_slice(pack);

        let url = format!("{}/git-receive-pack", self.base);
        let response = self.post(&url, "application/x-git-receive-pack-request", &body, false)?;
        parse_push_response(&response)
    }

    fn post_stream(
        &self,
        url: &str,
        content_type: &str,
        body: &[u8],
        protocol_v2: bool,
    ) -> Result<Box<dyn Read>> {
        let mut request = self.agent.post(url).set("Content-Type", content_type);
        if let Some(auth) = &self.auth {
            request = request.set("Authorization", auth);
        }
        if protocol_v2 {
            request = request.set("Git-Protocol", "version=2");
        }
        let response = request
            .send_bytes(body)
            .map_err(|error| http_error(url, error))?;
        Ok(Box::new(response.into_reader().take(MAX_BODY)))
    }

    /// Pushes ref updates with a packfile streamed from disk.
    pub fn push_file(
        &self,
        updates: &[PushCommand],
        pack_path: &std::path::Path,
    ) -> Result<PushReport> {
        if updates.is_empty() {
            return Err(Error::Message("no ref updates were requested".to_owned()));
        }
        let advertisement = self.discover_receive_pack()?;
        let algorithm = updates
            .first()
            .map(|update| update.new.algorithm())
            .unwrap_or_default();
        let mut capabilities = vec!["report-status".to_owned()];
        if algorithm != HashAlgorithm::Sha1 {
            let advertised = advertisement
                .capabilities
                .get("object-format")
                .and_then(|value| value.as_deref());
            if advertised != Some(algorithm.name()) {
                return Err(Error::unsupported(format!(
                    "the remote does not support {} pushes",
                    algorithm.name()
                )));
            }
            capabilities.push(format!("object-format={}", algorithm.name()));
        }
        if advertisement.capabilities.contains_key("side-band-64k") {
            capabilities.push("side-band-64k".to_owned());
        }
        if advertisement.capabilities.contains_key("ofs-delta") {
            capabilities.push("ofs-delta".to_owned());
        }
        if advertisement.capabilities.contains_key("thin-pack") {
            capabilities.push("thin-pack".to_owned());
        }
        capabilities.push(format!("agent={AGENT}"));

        let mut body = Vec::new();
        for (index, update) in updates.iter().enumerate() {
            let old = update.old.unwrap_or_else(|| ObjectId::zero(algorithm));
            let mut command = format!("{old} {} {}", update.new, update.reference).into_bytes();
            if index == 0 {
                command.push(0);
                command.extend_from_slice(capabilities.join(" ").as_bytes());
            }
            body.extend(encode_data(&command)?);
        }
        body.extend(flush());
        let pack = File::open(pack_path)
            .map_err(|error| Error::io(pack_path.display().to_string(), error))?;
        let content_length = body.len() as u64
            + std::fs::metadata(pack_path)
                .map_err(|error| Error::io(pack_path.display().to_string(), error))?
                .len();
        let reader = Cursor::new(body).chain(pack);
        let url = format!("{}/git-receive-pack", self.base);
        let response = self.post_reader(
            &url,
            "application/x-git-receive-pack-request",
            reader,
            false,
            Some(content_length),
        )?;
        parse_push_response(&response)
    }

    fn post_reader(
        &self,
        url: &str,
        content_type: &str,
        body: impl Read,
        protocol_v2: bool,
        content_length: Option<u64>,
    ) -> Result<Vec<u8>> {
        let mut request = self.agent.post(url).set("Content-Type", content_type);
        if let Some(auth) = &self.auth {
            request = request.set("Authorization", auth);
        }
        if protocol_v2 {
            request = request.set("Git-Protocol", "version=2");
        }
        if let Some(length) = content_length {
            request = request.set("Content-Length", &length.to_string());
        }
        let response = request.send(body).map_err(|error| http_error(url, error))?;
        read_body(response)
    }

    fn get(&self, url: &str, protocol_v2: bool) -> Result<Vec<u8>> {
        let mut last_error = None;
        for attempt in 0..=self.retries {
            let mut request = self.agent.get(url);
            if let Some(auth) = &self.auth {
                request = request.set("Authorization", auth);
            }
            if protocol_v2 {
                request = request.set("Git-Protocol", "version=2");
            }
            match request.call() {
                Ok(response) => return read_body(response),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < self.retries {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }
        Err(http_error(url, last_error.expect("at least one attempt")))
    }

    fn post(
        &self,
        url: &str,
        content_type: &str,
        body: &[u8],
        protocol_v2: bool,
    ) -> Result<Vec<u8>> {
        let mut last_error = None;
        for attempt in 0..=self.retries {
            let mut request = self.agent.post(url).set("Content-Type", content_type);
            if let Some(auth) = &self.auth {
                request = request.set("Authorization", auth);
            }
            if protocol_v2 {
                request = request.set("Git-Protocol", "version=2");
            }
            match request.send_bytes(body) {
                Ok(response) => return read_body(response),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < self.retries {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }
        Err(http_error(url, last_error.expect("at least one attempt")))
    }
}

struct ParsedUrl {
    base: String,
    host: String,
}

impl ParsedUrl {
    fn parse(url: &str) -> Result<Self> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| Error::unsupported(format!("unsupported transport URL: {url}")))?;
        if scheme != "https" && scheme != "http" {
            return Err(Error::unsupported(format!(
                "the '{scheme}' transport is not implemented yet"
            )));
        }
        let (authority, path) = match rest.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (rest, String::new()),
        };
        if authority.is_empty() {
            return Err(Error::Message(format!("invalid repository URL: {url}")));
        }

        let host_authority = if authority.contains('@') {
            return Err(Error::Message(
                "credentials in repository URLs are not supported; use the credential store"
                    .to_owned(),
            ));
        } else {
            authority
        };
        let host = host_authority
            .split(':')
            .next()
            .unwrap_or(host_authority)
            .to_ascii_lowercase();
        let path = path.trim_end_matches('/');
        let base = format!("{scheme}://{host_authority}{path}");
        Ok(Self { base, host })
    }
}

fn http_error(url: &str, error: ureq::Error) -> Error {
    let url = url.split('?').next().unwrap_or(url);
    match error {
        ureq::Error::Status(code, response) => {
            let mut body = String::new();
            let _ = response.into_reader().take(4096).read_to_string(&mut body);
            let body = body.trim();
            if code == 401 || code == 403 {
                Error::Message(format!(
                    "authentication failed for {url} (HTTP {code}); configure a token with `cvx credential store`"
                ))
            } else if body.is_empty() {
                Error::Message(format!("HTTP {code} from {url}"))
            } else {
                Error::Message(format!("HTTP {code} from {url}: {body}"))
            }
        }
        ureq::Error::Transport(transport) => {
            Error::Message(format!("transport error for {url}: {transport}"))
        }
    }
}

fn read_body(response: ureq::Response) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_BODY)
        .read_to_end(&mut body)
        .map_err(|error| Error::io("failed to read HTTP response", error))?;
    Ok(body)
}

fn parse_advertisement_capabilities(bytes: &[u8]) -> Result<BTreeMap<String, Option<String>>> {
    let mut lines = Vec::new();
    for packet in parse_all(bytes)? {
        let PktLine::Data(data) = packet else {
            continue;
        };
        let text = String::from_utf8_lossy(&data);
        if text.starts_with("# service=") {
            continue;
        }
        lines.push(data);
    }
    cvx_protocol::parse_capabilities(&lines)
}

pub(crate) fn parse_ls_refs(bytes: &[u8]) -> Result<LsRefsResponse> {
    let mut response = LsRefsResponse::default();
    for packet in parse_all(bytes)? {
        let PktLine::Data(data) = packet else {
            continue;
        };
        let line = data.strip_suffix(b"\n").unwrap_or(&data);
        if line.starts_with(b"ERR ") {
            return Err(Error::Message(format!(
                "remote error: {}",
                String::from_utf8_lossy(&line[4..])
            )));
        }
        if let Some(reference) = parse_ref_line(line)? {
            if reference.name == "HEAD" {
                if reference.symref.is_some() {
                    response.head_symref = reference.symref;
                }
            } else {
                response.refs.push(reference);
            }
        } else if let Some(head) = parse_unborn_head(line) {
            response.head_symref = Some(head);
        }
    }
    Ok(response)
}

fn parse_ref_line(line: &[u8]) -> Result<Option<RemoteRef>> {
    let line = line.trim_ascii_end();
    let (reference_part, _) = match line.iter().position(|byte| *byte == 0) {
        Some(position) => (&line[..position], Some(&line[position + 1..])),
        None => (line, None),
    };
    let mut fields = reference_part
        .split(|byte| *byte == b' ')
        .filter(|field| !field.is_empty());
    let Some(oid) = fields.next() else {
        return Ok(None);
    };
    if oid == b"unborn" {
        return Ok(None);
    }
    let Some(name) = fields.next() else {
        return Ok(None);
    };
    let oid = match ObjectId::from_hex(std::str::from_utf8(oid).unwrap_or("")) {
        Ok(oid) if !oid.is_zero() => oid,
        _ => return Ok(None),
    };
    let name = std::str::from_utf8(name)
        .map_err(|_| Error::corrupt("remote reference is not UTF-8"))?
        .to_owned();

    let mut peeled = None;
    let mut symref = None;
    for field in fields {
        let text = String::from_utf8_lossy(field);
        if let Some(value) = text.strip_prefix("peeled:") {
            peeled = ObjectId::from_hex(value).ok();
        } else if let Some(value) = text.strip_prefix("symref-target:") {
            symref = Some(value.to_owned());
        }
    }

    Ok(Some(RemoteRef {
        oid,
        name,
        peeled,
        symref,
    }))
}

fn parse_unborn_head(line: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(line).ok()?;
    if !text.starts_with("unborn HEAD") {
        return None;
    }
    text.split_whitespace()
        .find_map(|field| field.strip_prefix("symref-target:"))
        .map(str::to_owned)
}

pub(crate) fn parse_fetch_response_to_file<R: Read>(mut reader: R) -> Result<FetchResponseFile> {
    let mut temp = tempfile::Builder::new()
        .prefix("cvx-fetch-")
        .tempfile()
        .map_err(|error| Error::io("failed to create pack spool file", error))?;
    let mut shallow = Vec::new();
    let mut unshallow = Vec::new();
    let mut in_packfile = false;
    let mut pack_bytes = 0usize;

    loop {
        let packet = read_from(&mut reader)?;
        let Some(packet) = packet else {
            break;
        };
        let PktLine::Data(data) = packet else {
            continue;
        };
        if !in_packfile {
            let line = data.strip_suffix(b"\n").unwrap_or(&data);
            if line == b"packfile" {
                in_packfile = true;
                continue;
            }
            if line.starts_with(b"ERR ") {
                return Err(Error::Message(format!(
                    "remote fetch error: {}",
                    String::from_utf8_lossy(&line[4..])
                )));
            }
            if let Some(value) = line.strip_prefix(b"shallow ") {
                if let Ok(oid) = ObjectId::from_hex(&String::from_utf8_lossy(value)) {
                    shallow.push(oid);
                }
                continue;
            }
            if let Some(value) = line.strip_prefix(b"unshallow ") {
                if let Ok(oid) = ObjectId::from_hex(&String::from_utf8_lossy(value)) {
                    unshallow.push(oid);
                }
                continue;
            }
            if data.first().is_some_and(|byte| (1..=3).contains(byte)) {
                write_sideband(&mut temp, &data, &mut pack_bytes)?;
            }
            continue;
        }
        write_sideband(&mut temp, &data, &mut pack_bytes)?;
    }

    if pack_bytes == 0 {
        return Err(Error::Message("remote did not send a packfile".to_owned()));
    }
    temp.flush()
        .map_err(|error| Error::io("failed to flush pack spool file", error))?;
    let (_, path) = temp
        .keep()
        .map_err(|error| Error::io("failed to persist pack spool file", error.error))?;
    Ok(FetchResponseFile {
        pack_path: path,
        shallow,
        unshallow,
    })
}

fn write_sideband<W: Write>(writer: &mut W, data: &[u8], written: &mut usize) -> Result<()> {
    match data.first().copied() {
        Some(1) => {
            writer
                .write_all(&data[1..])
                .map_err(|error| Error::io("failed to write pack spool file", error))?;
            *written += data.len() - 1;
            Ok(())
        }
        Some(2) => Ok(()),
        Some(3) => Err(Error::Message(format!(
            "remote error: {}",
            String::from_utf8_lossy(&data[1..])
        ))),
        _ => Err(Error::corrupt(
            "remote packfile is missing a sideband channel",
        )),
    }
}

pub(crate) fn parse_fetch_response(bytes: &[u8]) -> Result<FetchResponse> {
    let mut response = FetchResponse::default();
    let mut in_packfile = false;

    for packet in parse_all(bytes)? {
        let PktLine::Data(data) = packet else {
            continue;
        };
        if !in_packfile {
            let line = data.strip_suffix(b"\n").unwrap_or(&data);
            if line == b"packfile" {
                in_packfile = true;
                continue;
            }
            if line.starts_with(b"ERR ") {
                return Err(Error::Message(format!(
                    "remote fetch error: {}",
                    String::from_utf8_lossy(&line[4..])
                )));
            }
            if let Some(value) = line.strip_prefix(b"shallow ") {
                if let Ok(oid) = ObjectId::from_hex(&String::from_utf8_lossy(value)) {
                    response.shallow.push(oid);
                }
                continue;
            }
            if let Some(value) = line.strip_prefix(b"unshallow ") {
                if let Ok(oid) = ObjectId::from_hex(&String::from_utf8_lossy(value)) {
                    response.unshallow.push(oid);
                }
                continue;
            }
            if line == b"acknowledgments"
                || line == b"shallow-info"
                || line == b"wanted-refs"
                || line == b"packfile-uris"
                || line.starts_with(b"NAK")
                || line.starts_with(b"ACK")
            {
                continue;
            }
            if data.first().is_some_and(|byte| (1..=3).contains(byte)) {
                append_sideband(&mut response.pack, &data)?;
            }
            continue;
        }
        append_sideband(&mut response.pack, &data)?;
    }

    if response.pack.is_empty() {
        return Err(Error::Message("remote did not send a packfile".to_owned()));
    }
    Ok(response)
}

fn append_sideband(pack: &mut Vec<u8>, data: &[u8]) -> Result<()> {
    match data.first().copied() {
        Some(1) => {
            pack.extend_from_slice(&data[1..]);
            Ok(())
        }
        Some(2) => Ok(()),
        Some(3) => Err(Error::Message(format!(
            "remote error: {}",
            String::from_utf8_lossy(&data[1..])
        ))),
        _ => Err(Error::corrupt(
            "remote packfile is missing a sideband channel",
        )),
    }
}

pub(crate) fn parse_receive_advertisement(bytes: &[u8]) -> Result<ReceivePackAdvertisement> {
    let mut advertisement = ReceivePackAdvertisement::default();
    for packet in parse_all(bytes)? {
        let PktLine::Data(data) = packet else {
            continue;
        };
        let text = String::from_utf8_lossy(&data);
        if text.starts_with("# service=") {
            continue;
        }
        if let Some(position) = data.iter().position(|byte| *byte == 0) {
            let capabilities = String::from_utf8_lossy(&data[position + 1..]);
            for capability in capabilities.split_whitespace() {
                let (key, value) = match capability.split_once('=') {
                    Some((key, value)) => (key, Some(value.to_owned())),
                    None => (capability, None),
                };
                advertisement.capabilities.insert(key.to_owned(), value);
            }
        }
        if let Some(reference) = parse_ref_line(&data)? {
            if reference.name != "capabilities^{}" {
                advertisement.refs.push(reference);
            }
        }
    }
    Ok(advertisement)
}

pub(crate) fn parse_push_response(bytes: &[u8]) -> Result<PushReport> {
    let mut report_text = Vec::new();
    let mut messages = Vec::new();
    for packet in parse_all(bytes)? {
        let PktLine::Data(data) = packet else {
            continue;
        };
        match data.first().copied() {
            Some(1) => report_text.extend_from_slice(&data[1..]),
            Some(2) => messages.extend_from_slice(&data[1..]),
            Some(3) => {
                return Err(Error::Message(format!(
                    "remote push error: {}",
                    String::from_utf8_lossy(&data[1..])
                )));
            }
            _ => report_text.extend_from_slice(&data),
        }
    }

    let report_text = decode_sideband_report(&report_text);
    let text = String::from_utf8_lossy(&report_text);
    let mut report = PushReport {
        unpack_ok: false,
        refs: Vec::new(),
        messages: String::from_utf8_lossy(&messages).into_owned(),
    };
    for line in text.lines() {
        if let Some(status) = line.strip_prefix("unpack ") {
            report.unpack_ok = status.trim() == "ok";
            if !report.unpack_ok {
                return Err(Error::Message(format!(
                    "remote failed to unpack: {}",
                    status.trim()
                )));
            }
        } else if let Some(status) = line.strip_prefix("ok ") {
            report.refs.push(PushRefStatus {
                reference: status.trim().to_owned(),
                ok: true,
                message: String::new(),
            });
        } else if let Some(status) = line.strip_prefix("ng ") {
            let (reference, message) = status.split_once(' ').unwrap_or((status, ""));
            report.refs.push(PushRefStatus {
                reference: reference.to_owned(),
                ok: false,
                message: message.to_owned(),
            });
        }
    }

    if report.refs.iter().any(|status| !status.ok) {
        let failed: Vec<String> = report
            .refs
            .iter()
            .filter(|status| !status.ok)
            .map(|status| format!("{}: {}", status.reference, status.message))
            .collect();
        return Err(Error::Message(format!(
            "remote rejected the push: {}",
            failed.join("; ")
        )));
    }
    if report.refs.is_empty() {
        return Err(Error::Message(
            "remote did not report a push status".to_owned(),
        ));
    }
    Ok(report)
}

fn decode_sideband_report(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let Ok(packets) = parse_all(bytes) else {
        return bytes.to_vec();
    };
    if !packets
        .iter()
        .any(|packet| matches!(packet, PktLine::Data(_)))
    {
        return bytes.to_vec();
    }

    let mut text = Vec::new();
    for packet in packets {
        if let PktLine::Data(data) = packet {
            text.extend_from_slice(&data);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use cvx_protocol::encode_text;

    #[test]
    fn parses_repository_urls() {
        let parsed = ParsedUrl::parse("https://github.com/owner/repo.git").expect("parse");
        assert_eq!(parsed.base, "https://github.com/owner/repo.git");
        assert_eq!(parsed.host, "github.com");

        assert!(ParsedUrl::parse("https://token@github.com/owner/repo.git/").is_err());
        assert!(ParsedUrl::parse("https://user:pass@github.com/owner/repo.git").is_err());
        assert!(ParsedUrl::parse("ssh://github.com/owner/repo.git").is_err());
        assert!(ParsedUrl::parse("not-a-url").is_err());
    }

    #[test]
    fn parses_ref_lines() {
        let oid = "1111111111111111111111111111111111111111";
        let line = format!("{oid} refs/heads/main\n");
        let reference = parse_ref_line(line.as_bytes())
            .expect("parse")
            .expect("ref");
        assert_eq!(reference.name, "refs/heads/main");
        assert_eq!(reference.oid, ObjectId::from_hex(oid).unwrap());

        let line = format!("{oid} refs/tags/v1 peeled:2222222222222222222222222222222222222222\n");
        let reference = parse_ref_line(line.as_bytes())
            .expect("parse")
            .expect("ref");
        assert_eq!(
            reference.peeled,
            Some(ObjectId::from_hex("2222222222222222222222222222222222222222").unwrap())
        );

        assert!(
            parse_ref_line(b"unborn HEAD symref-target:refs/heads/main\n")
                .expect("parse")
                .is_none()
        );
    }

    #[test]
    fn parses_fetch_pack_sideband() {
        let mut body = encode_text("packfile\n").expect("pkt");
        body.extend(encode_data(b"\x01PACKdata").expect("pkt"));
        body.extend(encode_data(b"\x02progress").expect("pkt"));
        body.extend(flush());
        assert_eq!(parse_fetch_response(&body).expect("pack").pack, b"PACKdata");
    }

    #[test]
    fn spools_fetch_pack_to_a_file() {
        let mut body =
            encode_text("shallow 1111111111111111111111111111111111111111\n").expect("pkt");
        body.extend(encode_text("packfile\n").expect("pkt"));
        body.extend(encode_data(b"\x01PACKspooled").expect("pkt"));
        body.extend(encode_data(b"\x02progress").expect("pkt"));
        body.extend(flush());
        let response = parse_fetch_response_to_file(std::io::Cursor::new(body)).expect("spool");
        let pack = std::fs::read(&response.pack_path).expect("read spool");
        assert_eq!(pack, b"PACKspooled");
        assert_eq!(response.shallow.len(), 1);
        let _ = std::fs::remove_file(response.pack_path);
    }

    #[test]
    fn parses_push_reports() {
        let mut body = encode_data(b"\x01unpack ok\n").expect("pkt");
        body.extend(encode_data(b"\x01ok refs/heads/main\n").expect("pkt"));
        body.extend(flush());
        let report = parse_push_response(&body).expect("report");
        assert!(report.unpack_ok);
        assert_eq!(report.refs.len(), 1);

        let mut body = encode_data(b"\x01unpack ok\n").expect("pkt");
        body.extend(encode_data(b"\x01ng refs/heads/main non-fast-forward\n").expect("pkt"));
        body.extend(flush());
        assert!(parse_push_response(&body).is_err());
    }
}
