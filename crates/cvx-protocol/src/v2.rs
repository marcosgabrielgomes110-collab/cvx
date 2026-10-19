use std::collections::BTreeMap;

use cvx_core::{Error, Result};
use cvx_object::ObjectId;

use crate::pkt::{PktLine, delimiter, encode_text, flush, parse_all};

/// Default agent string sent in protocol v2 requests.
pub const AGENT: &str = concat!("cvx/", env!("CARGO_PKG_VERSION"));

/// Request for protocol v2 `ls-refs`.
#[derive(Debug, Clone, Default)]
pub struct LsRefsRequest {
    /// Reference prefixes to include.
    pub ref_prefixes: Vec<String>,
    /// Ask the server to include symbolic reference targets.
    pub symrefs: bool,
    /// Ask the server to peel annotated tags.
    pub peel: bool,
}

impl LsRefsRequest {
    /// Encodes a complete `ls-refs` request.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut arguments = Vec::new();
        if self.symrefs {
            arguments.push("symrefs".to_owned());
        }
        if self.peel {
            arguments.push("peel".to_owned());
        }
        for prefix in &self.ref_prefixes {
            validate_argument(prefix)?;
            arguments.push(format!("ref-prefix {prefix}"));
        }
        encode_request("ls-refs", &arguments)
    }
}

/// Request for protocol v2 `fetch`.
#[derive(Debug, Clone, Default)]
pub struct FetchRequest {
    /// Objects the client wants.
    pub wants: Vec<ObjectId>,
    /// Objects the client already has.
    pub haves: Vec<ObjectId>,
    /// Ask the server to finish the negotiation.
    pub done: bool,
    /// Request a thin pack.
    pub thin: bool,
    /// Suppress progress messages.
    pub no_progress: bool,
    /// Include annotated-tag objects that point at wanted objects.
    pub include_tag: bool,
    /// Allow offset deltas.
    pub ofs_delta: bool,
}

impl FetchRequest {
    /// Encodes a complete `fetch` request.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.wants.is_empty() {
            return Err(Error::Message(
                "a fetch request needs at least one want".to_owned(),
            ));
        }
        let mut arguments = Vec::new();
        for oid in &self.wants {
            arguments.push(format!("want {oid}"));
        }
        for oid in &self.haves {
            arguments.push(format!("have {oid}"));
        }
        if self.done {
            arguments.push("done".to_owned());
        }
        if self.thin {
            arguments.push("thin-pack".to_owned());
        }
        if self.no_progress {
            arguments.push("no-progress".to_owned());
        }
        if self.include_tag {
            arguments.push("include-tag".to_owned());
        }
        if self.ofs_delta {
            arguments.push("ofs-delta".to_owned());
        }
        encode_request("fetch", &arguments)
    }
}

/// Builds a protocol v2 request with command and capability headers.
pub fn encode_request(command: &str, arguments: &[String]) -> Result<Vec<u8>> {
    validate_argument(command)?;
    let mut output = Vec::new();
    output.extend(encode_text(&format!("command={command}\n"))?);
    output.extend(encode_text(&format!("agent={AGENT}\n"))?);
    output.extend(encode_text("object-format=sha1\n")?);
    output.extend(delimiter());
    for argument in arguments {
        validate_argument(argument)?;
        output.extend(encode_text(&format!("{argument}\n"))?);
    }
    output.extend(flush());
    Ok(output)
}

/// Parses capability lines from a protocol v2 server advertisement.
///
/// Each line uses `key` or `key=value` form. The `version 2` line is ignored.
pub fn parse_capabilities(lines: &[Vec<u8>]) -> Result<BTreeMap<String, Option<String>>> {
    let mut capabilities = BTreeMap::new();
    for line in lines {
        let text = std::str::from_utf8(line)
            .map_err(|_| Error::corrupt("protocol capability is not UTF-8"))?;
        let line = text.trim_end_matches('\n');
        if line.is_empty() || line == "version 2" {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some((key, value)) => (key, Some(value.to_owned())),
            None => (line, None),
        };
        if key.is_empty() {
            return Err(Error::corrupt("protocol capability has an empty name"));
        }
        capabilities.insert(key.to_owned(), value);
    }
    Ok(capabilities)
}

/// Parses a pkt-line stream of capability lines.
pub fn parse_capability_packets(bytes: &[u8]) -> Result<BTreeMap<String, Option<String>>> {
    let mut lines = Vec::new();
    for packet in parse_all(bytes)? {
        match packet {
            PktLine::Data(data) => lines.push(data),
            PktLine::Flush | PktLine::Delim => {}
            PktLine::ResponseEnd => break,
        }
    }
    parse_capabilities(&lines)
}

fn validate_argument(argument: &str) -> Result<()> {
    if argument.contains('\n') || argument.contains('\0') {
        return Err(Error::Message(
            "protocol argument contains an invalid character".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkt::parse_all;

    fn oid(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; 20])
    }

    #[test]
    fn encodes_ls_refs_request() {
        let request = LsRefsRequest {
            ref_prefixes: vec!["refs/heads/".to_owned()],
            symrefs: true,
            peel: true,
        };
        let packets = parse_all(&request.encode().expect("encode")).expect("parse");
        assert_eq!(
            packets.first(),
            Some(&PktLine::Data(b"command=ls-refs\n".to_vec()))
        );
        assert!(packets.contains(&PktLine::Delim));
        assert!(packets.contains(&PktLine::Data(b"symrefs\n".to_vec())));
        assert!(packets.contains(&PktLine::Data(b"peel\n".to_vec())));
        assert!(packets.contains(&PktLine::Data(b"ref-prefix refs/heads/\n".to_vec())));
        assert_eq!(packets.last(), Some(&PktLine::Flush));
    }

    #[test]
    fn encodes_fetch_request() {
        let request = FetchRequest {
            wants: vec![oid(1)],
            haves: vec![oid(2)],
            done: true,
            thin: true,
            no_progress: true,
            include_tag: true,
            ofs_delta: true,
        };
        let packets = parse_all(&request.encode().expect("encode")).expect("parse");
        let data: Vec<String> = packets
            .iter()
            .filter_map(PktLine::data)
            .map(|data| String::from_utf8_lossy(data).trim_end().to_owned())
            .collect();
        assert!(data.contains(&format!("want {}", oid(1))));
        assert!(data.contains(&format!("have {}", oid(2))));
        assert!(data.contains(&"done".to_owned()));
        assert!(data.contains(&"thin-pack".to_owned()));
        assert!(data.contains(&"no-progress".to_owned()));
        assert!(data.contains(&"include-tag".to_owned()));
        assert!(data.contains(&"ofs-delta".to_owned()));
    }

    #[test]
    fn rejects_requests_without_wants() {
        assert!(FetchRequest::default().encode().is_err());
        assert!(
            LsRefsRequest {
                ref_prefixes: vec!["refs/heads/\n".to_owned()],
                ..LsRefsRequest::default()
            }
            .encode()
            .is_err()
        );
    }

    #[test]
    fn parses_capabilities() {
        let bytes = [
            b"version 2\n".as_slice(),
            b"agent=git/2.45\n".as_slice(),
            b"fetch=shallow filter\n".as_slice(),
            b"object-format=sha1\n".as_slice(),
        ]
        .concat();
        let mut encoded = Vec::new();
        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            encoded.extend(encode_text(std::str::from_utf8(line).expect("utf8")).expect("pkt"));
        }
        encoded.extend(flush());

        let capabilities = parse_capability_packets(&encoded).expect("capabilities");
        assert_eq!(
            capabilities.get("agent"),
            Some(&Some("git/2.45".to_owned()))
        );
        assert_eq!(
            capabilities.get("fetch"),
            Some(&Some("shallow filter".to_owned()))
        );
        assert_eq!(
            capabilities.get("object-format"),
            Some(&Some("sha1".to_owned()))
        );
    }
}
