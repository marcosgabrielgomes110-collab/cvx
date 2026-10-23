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
| 9 | Streaming index/network and large-repository memory limits | Complete — v0.9.2 |
| 10 | Security, concurrency and durability | In progress — v0.9.3 |
| 11 | Git completeness, GitHub surface, packaging and operations | Planned — v0.11.0 |
| 12 | Release-candidate stabilization and real-repository soak tests | Planned — v1.0.0-rc.1 |
| 13 | Production release | Planned — v1.0.0 |

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

## Phase 9 — streaming, network and memory (v0.9.0)

This phase is about making the engine safe for large repositories and real
network traffic. It is the next release, not a feature-only milestone.

Implemented through v0.9.2:

- `cvx fsck --repair` rebuilds a missing or unreadable index from `HEAD`;
- clone and `gc` pack writing streams through a temporary file with an
  incremental checksum instead of holding the complete pack in memory;
- pack publication remains ordered before index publication and preserves the
  `pack.before-index` crash point;
- network push emits thin packs against remote-known bases and falls back when
  the server does not advertise `thin-pack`;
- `push --force-with-lease` validates the local remote-tracking ref;
- streaming index reads with incremental checksum;
- spooled HTTP/SSH fetch response bodies and push packs streamed from disk;
- opt-in 1M-file benchmark with an address-space ceiling;
- end-to-end SHA-256 HTTP clone and thin-pack push against `git-http-backend`.

Delivered groundwork in v0.8.1:

- commit lifecycle hooks (`pre-commit`, `commit-msg`, `post-commit`) and
  `pre-push`, with `core.hooksPath`;
- `CVX_LOCK_TIMEOUT_MS` and clearer stale-lock diagnostics;
- `cvx reset <REV> -- <PATH>...` for pathspec index resets;
- integration coverage for hooks, lock timeout and reset pathspecs.

### Known limitations after v0.9.2

- object ingestion still reads the spooled pack once before writing objects;
  callback-streamed ingestion is deferred;
- the 1M-file benchmark is opt-in and not part of the default CI matrix;
- SHA-256 end-to-end coverage exists for HTTP; SSH uses the same protocol path
  but still needs a dedicated server fixture;
- incremental repack, `have`/`ack`, shallow update/unshallow, proxy/TLS and
  network chaos stay in Phase 10/11 hardening.

## Phase 10 — security, concurrency and durability (v0.10.0)

Implemented in v0.9.3:

- stale-lock repair through `fsck --repair` with an age threshold;
- opt-in soak script with concurrent status readers;
- HTTP timeout/retry controls and SSH strict host-key policy;
- threat model, release checklist and SBOM generation.

### Data safety and concurrency

- stale-lock recovery automation beyond the explicit `fsck --repair` path;
- automated multi-process soak tests for index, refs, packs and `gc`;
- transactional multi-reference push (all refs or none);
- crash-consistency tests for pack deletion, loose-object pruning and
  reflog-only history;
- `cvx fsck --repair` coverage for more control-state corruption;
- explicit reflog retention policy in `gc` and an option to opt out of pruning;
- durability review on Windows and network filesystems (fsync, replace and
  directory synchronization semantics).

### Network hardening

- end-to-end SHA-256 SSH fixtures;
- fetch negotiation with `have`/`ack`, bounded `want` sets and incremental
  packs;
- side-band, push options, atomic push and `force-with-lease` coverage;
- proxy support, custom TLS roots and connection reuse beyond the current
  timeout/retry controls;
- shallow fetch updates, `--unshallow` and partial-clone filters beyond
  `blob:none`;
- network chaos tests: truncation, redirects, expired tokens, rate limits and
  interrupted pack streams.

### Security and supply chain

- independent security audit of parsers, transports, credential handling and
  worktree writes (the threat model is now documented);
- continuous fuzzing in CI, not only deterministic property tests;
- `cargo-audit`/`cargo-deny` and dependency policy (SBOM generation is now in
  the release workflow);
- build provenance and mandatory signed release artifacts;
- reproducible builds across Linux, macOS and Windows toolchains;
- strict SSH host-key policy option and documented `known_hosts` workflow;
- secret redaction audit, token scope minimization and malicious-repository
  resource limits.

### Exit criteria for 0.10.0

- no known data-loss path under crash, concurrent access or interrupted
  network transfer;
- security checks and dependency policy run in CI;
- releases are signed, carry an SBOM and reproduce from a clean checkout;
- multi-process soak tests pass on all supported platforms.

## Phase 11 — Git completeness and ecosystem (v0.11.0)

### Git compatibility

- hooks beyond commit lifecycle and pre-push (`prepare-commit-msg`,
  `pre-merge-commit`, `post-checkout`, `post-merge`);
- commit and tag signing, and verification;
- submodules, Git LFS, reftable, multi-pack index and alternates;
- linked worktrees and pack v3/index v4 writing;
- working-tree encodings and diff/merge attribute drivers;
- merge rename detection and directory/file conflict resolution;
- remaining reset edge cases with pathspecs;
- `cherry-pick`/`revert` conflict state with `--continue`/`--abort` and author
  preservation;
- stash support for untracked files and index restoration.

### GitHub and product surface

- automatic pagination for lists, review threads and comments;
- GitHub App/installation tokens and OAuth browser callback;
- webhooks, teams, milestones beyond the current API subset, and rate-limit
  handling;
- streaming release-asset uploads instead of loading assets into memory;
- project/board and branch-protection operations where the API permits.

### Operations and packaging

- installers and packages for Linux, macOS and Windows;
- man pages, shell completions and a complete command/config reference;
- migration guide from Git, backup/restore and disaster-recovery procedures;
- version support policy, release cadence and security-update guarantees;
- documented performance envelope and supported repository sizes;
- CI gates for MSRV, feature combinations, Windows paths and long-running
  soak tests.

### Exit criteria for 0.11.0

- the documented Git subset is implemented or explicitly unsupported without
  silent data loss;
- installers exist for the three platforms;
- migration and disaster-recovery documentation is tested;
- GitHub list operations paginate correctly.

## Phase 12 — release candidate (v1.0.0-rc.1)

- feature freeze except critical fixes;
- run the full differential suite against Git on Linux, macOS and Windows;
- migrate real repositories and run soak tests for at least two weeks;
- publish release-candidate binaries and collect a public bug report window;
- verify signed, reproducible artifacts and the SBOM from a clean environment;
- exercise upgrade, backup and restore procedures;
- close all P0 data-loss and P0 security findings.

### Exit criteria for rc.1

- no open P0 correctness, data-loss or security issue;
- no known regression against the supported Git subset;
- release checklist and rollback procedure exercised end-to-end.

## Phase 13 — production release (v1.0.0)

- publish the stable release from the validated rc.1 tree;
- publish signed binaries, checksums, SBOM and provenance;
- freeze the support policy and security-response process;
- announce the supported repository sizes, platforms and Git compatibility;
- preserve a rollback path and post-release monitoring window.

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
