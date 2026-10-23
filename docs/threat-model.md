# Threat model

## Assets

- object database, packs, indexes and references;
- worktree contents and file metadata;
- repository configuration and hooks;
- HTTPS/SSH credentials and GitHub tokens;
- release artifacts, checksums, SBOM and signing material;
- CI runners and caches.

## Trust boundaries

1. **Local worktree** — untrusted paths, symlinks, encodings and file modes.
2. **Repository data** — packs, indexes, refs and config may be malformed.
3. **Network** — HTTPS, SSH, proxies and remote Git servers are untrusted.
4. **GitHub API** — remote responses, pagination and rate limits.
5. **Hooks** — repository-controlled executable code.
6. **CI and release** — build environment, dependencies and artifacts.

## Threats and mitigations

| Threat | Mitigation | Residual risk |
|---|---|---|
| Malicious pack/index causes panic or OOM | Bounded parsers, property tests, decompression limits | Continuous fuzzing still required |
| Path traversal or symlink escape | Canonical paths, symlinked-ancestor rejection, atomic writes | Platform-specific filesystem behavior |
| Malformed ref or config | Strict validation, explicit errors | Unknown extensions must be rejected |
| Credential leakage in logs/errors | Secret-free errors, stdin/env token input, `0600` file fallback | Third-party debug tooling |
| HTTPS MITM | System TLS roots, no redirects for authenticated requests | Custom CA support not implemented |
| SSH host-key substitution | `StrictHostKeyChecking` configurable, default `accept-new` | Production should set `yes` and provision known_hosts |
| Malicious hook execution | Hooks are opt-in repository code, explicit execution | Hooks can execute arbitrary user-level code |
| Malicious repository resource exhaustion | Parser size limits, delta depth limits, object count checks | Total-process memory budgets still incomplete |
| Release tampering | Checksums, optional GPG signatures, SBOM | Mandatory signing and provenance need release keys |
| Token over-scoping | Scope inspection and documented minimal scopes | Fine-grained token behavior varies |

## Out of scope for the current threat model

- multi-tenant server hosting;
- sandboxing arbitrary hooks;
- remote code execution in third-party CI actions;
- supply-chain compromise of the Rust toolchain itself.

## Reporting

Security issues should be reported through GitHub private vulnerability
reporting as described in `SECURITY.md`.
