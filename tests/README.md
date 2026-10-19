# Integration tests

Crate-local unit tests and CLI workflow tests live next to their crates. This
top-level directory is reserved for cross-crate suites that exercise several
layers at once:

- `compatibility/` — differential tests against the official Git binary;
- `protocol/` — wire-protocol and packfile tests;
- `corruption/` — malformed repositories and integrity failures;
- `recovery/` — interruption, stale locks and crash recovery;
- `end_to_end/` — complete CLI workflows.

Current differential coverage lives in
`crates/cvx-cli/tests/workflow.rs` and is being expanded toward these suites.

The CVX binary never invokes Git. These suites may use Git as an oracle.
