# Roadmap

CVX ships in explicit compatibility slices. A feature is only announced when it
is implemented, covered by tests and documented in the compatibility matrix.

| Phase | Scope | Status |
| --- | --- | --- |
| 0 | Specification, architecture, compatibility contract | Complete |
| 1 | Objects, index, refs, commits, status, log, pack reads | Complete |
| 2A | Diff, restore/checkout, branches, switch | Complete |
| 2B | Merge, conflicts, tags creation, rename detection | Complete |
| 3 | Pack writing, clone/fetch/push, SSH/HTTPS, credentials | Complete |
| 4 | GitHub API, OAuth, collaborators, reviews, checks, milestones, GraphQL, release assets | Complete — v0.3.0 |
| 5 | Fuzzing, crash recovery, benchmarks, SHA-256, release hardening | Complete — v0.4.0 |
| 6 | Maintenance and history editing: fsck, gc, reset, cherry-pick, revert, stash | Complete — v0.5.0 |
| 7 | Reliability, delta packs and bare repositories | Complete — v0.6.0 |
| 8 | Streaming pack reads, thin packs, SHA-256 network negotiation | Complete — v0.7.1 |
| 9 | Production readiness: streaming, concurrency, security, Git completeness, packaging | In progress — v0.8.1 |

## Phase 2B — merge and history workflows

- three-way merge with base/ours/theirs;
- conflict markers and index stages 1/2/3;
- precise conflict status codes;
- merge commits with two parents;
- `merge --abort` and `merge --continue`;
- annotated and lightweight tag creation;
- `log --decorate`;
- rename detection in status and diff.

## Phase 3 — transport

Implemented:

- pack writing, pack ingestion and version-2 index generation;
- object enumeration and reachability;
- clone, fetch, push and remotes over local paths, `file://`, `https://`,
  `ssh://` and SCP-like URLs;
- protocol v2 pkt-line framing, capability/response parsing and request builders;
- receive-pack push with report-status parsing;
- HTTPS basic authentication and SSH through the system OpenSSH client;
- OS keychain credentials with a user-only `0600` file fallback;
- Git credential-helper `fill`, `approve` and `reject`;
- shallow clones (`--depth`) and partial clones (`--filter=blob:none`) with
  lazy blob fetch.

Known limitations:

- shallow fetch updates/`--unshallow` are not implemented;
- partial clones fetch missing blobs one at a time;
- `--depth` and `--filter` are rejected for local-path clones;
- SSH requires an OpenSSH client, key or agent outside CVX.

## Phase 4 — GitHub

Complete in v0.3.0:

- token and OAuth device-flow authentication with keychain/file storage;
- OAuth scope inspection and required-scope checks;
- repository creation, deletion and inspection;
- collaborator listing, add/remove and effective permission lookup;
- pull request creation, listing, reviews, review threads and merge;
- review-thread resolution and unresolution through GraphQL;
- check-run listing and required status-check inspection/replacement;
- issue creation, listing, closing, assignees and milestones;
- repository label creation, listing, deletion and issue/PR assignment;
- release creation, listing, asset upload and deletion;
- workflow-run visibility;
- generic GraphQL queries with inline or file-based variables.

Known limitations:

- GraphQL review threads return the first 100 threads and comments;
- required status checks require branch protection to be enabled already;
- release uploads read the asset into memory before sending;
- fine-grained tokens may not expose OAuth scope headers.

## Phase 5 — release quality

Implemented:

- property-based fuzz tests for pack, index, object, config and protocol
  parsers;
- crash failpoints and recovery tests for commit, index, ref and pack
  publication;
- Criterion benchmarks for pack, index, diff and repository operations,
  including an opt-in 100k-file scenario;
- `.gitattributes` text/eol/binary matching with nested files;
- CRLF normalization for `core.autocrlf=true`, `input` and `false`;
- local SHA-256 repositories: loose objects, packs, index, refs and clones;
- checksum verification, deterministic timestamps and optional GPG signing in
  the release workflow.

Remaining:

- fully reproducible artifacts across toolchains;
- working-tree encodings and diff/merge attribute drivers.

## Phase 6 — maintenance and history editing

Implemented in v0.5.0:

- `cvx fsck` checks refs, commits, trees, tags, index entries and dangling
  objects; `--strict` also fails on dangling objects;
- `cvx gc [--prune]` packs reachable objects; `--prune` performs a full repack,
  removes old packs and deletes unreachable loose objects;
- `cvx reset --soft|--mixed|--hard` updates HEAD, index and worktree with
  `ORIG_HEAD` and reflog entries;
- revision ancestry suffixes such as `HEAD~2` and `main^`;
- `cvx cherry-pick` and `cvx revert` for clean single-parent commits;
- `cvx stash push|list|apply|pop|drop` for tracked index and worktree changes.

Known limitations:

- cherry-pick/revert conflict resolution is abort-only: conflicts apply no
  changes and there is no `--continue`/`--abort` state;
- cherry-pick does not preserve the original author identity;
- stash does not include untracked files and applies only to the commit where
  it was created;
- reset does not accept pathspecs;
- the default `gc` leaves old packs in place; `gc --prune` performs the full
  repack and removal.

## Phase 7 — reliability, pack performance and bare repositories

Implemented in v0.6.0:

- offset-delta pack writing with bounded chains and Git `verify-pack` coverage;
- `gc --prune` repacks reachable objects, removes superseded packs unless a
  `.keep` file exists, and respects index entries, reflogs, stash refs and
  control references;
- bare repositories through `cvx init --bare` and `cvx clone --bare`, including
  fetch, push, log, tag, fsck and gc;
- explicit rejection of worktree commands in bare repositories;
- crash-recovery tests for stash publication and hard reset.

Remaining:

- pack/index streaming and thin-pack generation moved to Phase 8;
- hooks, commit signing, submodules and LFS moved to Phase 9.

## Phase 8 — streaming, thin packs and network formats

Implemented in v0.7.0:

- file-backed `Pack` reads with on-demand zlib decompression and a bounded
  object cache;
- thin-pack ingestion with external ref-delta bases resolved from the local
  object database;
- protocol-v2 `object-format` negotiation and SHA-256 receive-pack capability
  handling for HTTPS and SSH;
- fetch requests for thin packs and partial-clone lazy fetches using local bases;
- `gc.before-prune` recovery coverage and concurrent `add` consistency tests;
- memory-ceiling large benchmark script.

Remaining:

- streaming index and network response bodies, and bounded-memory pack writing;
- thin-pack generation for push;
- end-to-end SHA-256 server integration tests against a live Git server;
- lock timeout/stale-lock recovery and multi-process soak tests;
- hooks, commit signing, submodules and LFS.

## Phase 9 — production readiness (in progress)

The 0.7.x series is a beta with strong local and GitHub workflows, but it is
not yet production-grade for teams, CI fleets or large repositories. The
following work is required before a 1.0 production declaration.

Implemented in v0.8.1:

- commit lifecycle hooks (`pre-commit`, `commit-msg`, `post-commit`) and
  `pre-push`, with `core.hooksPath`;
- `CVX_LOCK_TIMEOUT_MS` for index/ref lock contention and clearer stale-lock
  diagnostics;
- `cvx reset <REV> -- <PATH>...` for pathspec index resets;
- integration coverage for hook rejection, message rewriting, pre-push
  rejection and lock timeout diagnostics.

### P0 — data safety and concurrency

- stale-lock automatic recovery (timeout and diagnostics are implemented);
- multi-process soak tests for index, refs, packs and `gc`;
- transactional multi-reference push (all refs or none);
- crash-consistency tests for pack deletion, loose-object pruning and
  reflog-only history;
- `cvx fsck --repair` for safe index/control-state recovery, never silent
  object deletion;
- explicit reflog retention policy in `gc` and a way to opt out of pruning;
- durability review on Windows and network filesystems (fsync, replace and
  directory synchronization semantics).

### P0 — streaming and scale

- streaming index parsing and writing with bounded memory;
- streaming or spooled HTTP/SSH response bodies and pack ingestion;
- bounded-memory pack writing and object enumeration;
- incremental repack, pack reuse and delta/search heuristics comparable to
  Git on large repositories;
- benchmark and regression gates for 1M+ objects, long history and large
  binary files;
- documented memory budgets and failure behavior under pressure.

### P0 — network and wire protocol

- thin-pack generation for push, not only ingestion;
- end-to-end SHA-256 tests against real HTTPS and SSH Git servers;
- fetch negotiation with `have`/`ack`, bounded `want` sets and incremental
  packs;
- protocol coverage for side-band, push options, atomic push and
  `force-with-lease`;
- retries, timeouts, connection reuse, proxy support and custom TLS roots;
- shallow fetch updates, `--unshallow` and partial-clone filters beyond
  `blob:none`;
- network chaos tests: truncation, redirects, expired tokens, rate limits and
  interrupted pack streams.

### P0 — security and supply chain

- formal threat model and independent security audit of parsers, transports,
  credential handling and worktree writes;
- continuous fuzzing in CI, not only deterministic property tests;
- `cargo-audit`/`cargo-deny`, dependency policy and SBOM generation;
- build provenance and mandatory signed release artifacts;
- reproducible builds across Linux, macOS and Windows toolchains;
- strict SSH host-key policy option and documented `known_hosts` workflow;
- secret redaction audit, token scope minimization and malicious-repository
  resource limits.

### P1 — Git compatibility completeness

- hooks beyond commit lifecycle and pre-push (`prepare-commit-msg`, `pre-merge-commit`, `post-checkout`, `post-merge`);
- commit and tag signing, and verification;
- submodules, Git LFS, reftable, multi-pack index and alternates;
- linked worktrees and pack v3/index v4 writing;
- working-tree encodings and diff/merge attribute drivers;
- merge rename detection and directory/file conflict resolution;
- remaining reset edge cases with pathspecs (implemented for index resets);
- `cherry-pick`/`revert` conflict state with `--continue`/`--abort` and author
  preservation;
- stash support for untracked files and index restoration.

### P1 — GitHub and product surface

- automatic pagination for lists, review threads and comments;
- GitHub App/installation tokens and OAuth browser callback;
- webhooks, teams, milestones beyond the current API subset, and rate-limit
  handling;
- streaming release-asset uploads instead of loading assets into memory;
- project/board and branch-protection operations where the API permits.

### P2 — operations and release engineering

- installers and packages for Linux, macOS and Windows;
- man pages, shell completions and a complete command/config reference;
- migration guide from Git, backup/restore and disaster-recovery procedures;
- version support policy, release cadence and security-update guarantees;
- documented performance envelope and supported repository sizes;
- CI gates for MSRV, feature combinations, Windows paths and long-running
  soak tests.

### Exit criteria for 1.0

1. No known data-loss path under crash, concurrent access or interrupted
   network transfer; `fsck` can diagnose and safely repair control state.
2. Pack, index and network operations stay within documented memory budgets on
   a 1M-object repository.
3. Network parity covers thin packs, SHA-256, shallow/partial updates,
   retries and transactional multi-ref push.
4. Security audit, continuous fuzzing, SBOM, signed releases and reproducible
   builds are complete.
5. Hooks, commit signing, submodules, LFS and the documented Git subset are
   implemented or explicitly unsupported without silent data loss.
6. Installers, migration documentation and a support policy exist for all
   three platforms.
7. At least one release-candidate cycle has run against real repositories
   without critical regressions.

## Definition of done

A phase is done when:

1. the user-facing commands are implemented;
2. `git fsck` and differential tests pass;
3. supported and unsupported cases are documented;
4. CI is green on Linux, macOS and Windows;
5. no new runtime dependency on the `git` executable was introduced.
