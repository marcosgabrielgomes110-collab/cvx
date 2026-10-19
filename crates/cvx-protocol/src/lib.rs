//! Git wire protocol primitives.
//!
//! The crate currently implements pkt-line framing and protocol v2 request
//! builders. Transports compose these primitives over HTTPS, SSH or a local
//! process.

#![forbid(unsafe_code)]

mod pkt;
mod v2;

pub use pkt::{
    MAX_PKT_LINE_LEN, PktLine, delimiter, encode_data, encode_text, flush, parse_all, read_from,
    response_end,
};
pub use v2::{
    AGENT, FetchRequest, LsRefsRequest, encode_request, parse_capabilities,
    parse_capability_packets,
};
