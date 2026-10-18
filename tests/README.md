# Integration tests

Unit and crate-local integration tests live next to their crates. This top-level
directory is reserved for cross-crate suites that exercise several layers at
once:

- `compatibility/` — differential tests against the official Git binary;
- `protocol/` — wire-protocol and packfile tests;
- `corruption/` — malformed repositories and integrity failures;
- `recovery/` — interruption, stale locks and crash recovery;
- `end_to_end/` — complete CLI workflows.

The CVX binary never invokes Git. These suites may use Git as an oracle.
