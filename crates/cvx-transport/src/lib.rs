//! HTTPS Git transport.
//!
//! The local filesystem transport lives in `cvx-repository`; this crate speaks
//! the Git smart HTTP protocol for HTTPS repositories. SSH is not implemented
//! yet.

#![forbid(unsafe_code)]

mod http;

pub use http::{
    HttpTransport, LsRefsResponse, PushCommand, PushRefStatus, PushReport,
    ReceivePackAdvertisement, RemoteRef,
};
