# Hardening

## Property-based parser tests

Stable property tests run in normal `cargo test` and assert that malformed input
never panics:

| Crate | Parsers |
|---|---|
| `cvx-pack` | delta, pack index, pack ingestion |
| `cvx-index` | index parser |
| `cvx-object` | commit, tree and tag parsers |
| `cvx-config` | configuration parser |
| `cvx-protocol` | pkt-line decoder |

Run them with:

```console
$ cargo test --workspace proptests
```

## Crash failpoints

`cvx-core::failpoint` injects failures when `CVX_FAILPOINT` contains a
comma-separated failpoint name. Production runs leave it unset.

| Failpoint | Injected point |
|---|---|
| `commit.before-ref-update` | Commit object written, branch not moved |
| `index.before-publish` | Index lock written, not renamed |
| `ref.before-publish` | Ref lock written, not renamed |
| `pack.before-index` | Pack written, index missing |
| `stash.before-reset` | Stash ref written, worktree reset not started |
| `gc.before-prune` | New pack verified, old packs and loose objects not removed |

Recovery tests live in `crates/cvx-cli/tests/recovery.rs` and verify:

- `HEAD` remains unchanged after an interrupted commit;
- lock files are removed after an interrupted index/ref publication;
- staged changes survive and the operation can be retried;
- an interrupted stash keeps its saved entry and can be applied later;
- an interrupted hard reset leaves `HEAD` unchanged and can be retried;
- an interrupted `gc --prune` leaves the repository usable and can be retried;
- concurrent `add` processes either serialize or fail one writer explicitly;
- an orphan pack without an index is ignored by repository discovery.

Example:

```console
$ CVX_FAILPOINT=commit.before-ref-update cvx commit -m "interrupted"
$ cvx status
$ cvx commit -m "retry"
```

## Benchmarks

See [benchmarks](benchmarks.md). Criterion benchmarks cover pack write/ingest,
index serialize/parse, diff rendering and repository operations on 10k files
(100k with `CVX_BENCH_LARGE=1`).

## Attribute normalization

CRLF tests and `.gitattributes` integration live in `cvx-attributes` and
`cvx-repository`. The following are covered:

- `text`, `text=auto`, `-text`, `binary`;
- `eol=lf`, `eol=crlf`;
- nested `.gitattributes` files;
- `core.autocrlf=true`, `input` and `false`;
- normalization on add, status, diff, checkout, restore and merge.

## SHA-256

`cvx init --object-format sha256` creates a SHA-256 repository. Loose objects,
packs, pack indexes, the index file, refs, commits, tags and local clones use
the 32-byte identifier width. Git accepts these repositories directly
(`git --git-dir=.cvx fsck --strict`).

SHA-256 over HTTPS/SSH is supported through protocol-v2 negotiation; mixed-format transfers are rejected explicitly. SSH SHA-256 still needs a dedicated server fixture.

## Delta packs and garbage collection

The pack writer emits `ofs-delta` entries when a candidate base is likely to
produce a smaller pack. Delta write depth is bounded and every generated pack
is verified with `git verify-pack` in tests.

`cvx gc --prune` performs a full repack of reachable objects, verifies the new
pack, then removes superseded packs unless a `.keep` file is present. Reachable
roots include refs, `HEAD`, the index, reflogs, stash refs and control files such
as `ORIG_HEAD`.

## Bare repositories

`cvx init --bare` and `cvx clone --bare` create repositories without a worktree.
Bare discovery, fetch, push, log, tag, fsck and gc work normally. Commands that
need a worktree fail explicitly instead of writing inside the repository.

## Streaming and thin packs

`Pack` is file-backed: it keeps the index and a small decoded-object cache,
while object payloads are read and decompressed from the `.pack` file on
demand. Pack checksums are verified incrementally.

`gc`, clone and push write packs through a temporary file using an incremental
checksum, so the complete compressed pack is not held in memory. Index reads
use bounded buffers, and HTTP/SSH fetch bodies are spooled to a temporary pack
file while pkt-lines are parsed.

Fetch requests thin packs. Ref-delta bases missing from the incoming pack are
resolved from the local object database; an unknown base is still rejected
explicitly. Network push generates thin packs against bases reachable from the
remote tip and falls back to a complete pack when `thin-pack` is not
advertised.

## Hooks and lock contention

Commit hooks run in repository order: `pre-commit`, then `commit-msg` with
`COMMIT_EDITMSG`, then `post-commit` after the ref update. `pre-push` receives
the remote name, URL and ref update on stdin. Missing or non-executable hooks
are skipped.

`CVX_LOCK_TIMEOUT_MS` makes index and ref lock acquisition wait before failing.
The default is zero. Diagnostics identify the lock path and explain how to
remove a stale lock safely. `fsck --repair` only removes lock files when
`CVX_STALE_LOCK_AGE_SECS` is explicitly set; the default is to leave them in
place because lock files carry no reliable owner identity.

## Index repair

`cvx fsck --repair` rebuilds a missing or unreadable index from `HEAD` and
reports the repair. It never deletes objects or rewrites refs. Bare
repositories are not given a synthetic index.

## Soak testing

`scripts/soak.sh` creates a disposable repository, commits repeatedly, runs
concurrent status readers, then executes `fsck` and `gc --prune`:

```console
$ CVX_SOAK_ITERATIONS=200 CVX_SOAK_JOBS=4 scripts/soak.sh
```

It is intentionally opt-in because it is a long-running test rather than a
unit test.

## Network timeouts and SSH host keys

HTTP uses `CVX_HTTP_TIMEOUT_SECS` (default 120) and `CVX_HTTP_RETRIES`
(default 2) for idempotent GET and byte-body POST requests. Streaming push
bodies are not retried automatically.

SSH accepts `CVX_SSH_STRICT_HOST_KEY_CHECKING=yes|no|accept-new` (default
`accept-new`). Production deployments should set `yes` and provision
`known_hosts` explicitly.

## Release verification and signing

Release workflows verify every checksum before publishing. When the
`RELEASE_SIGNING_KEY` secret is configured, archives are detached-signed with
GPG and the `.asc` files are published alongside the checksums.
`scripts/verify-release.sh` verifies checksums and any available signatures.

## Remaining hardening

- index and network response streaming with bounded memory;
- thin-pack generation for push and live SHA-256 server integration tests;
- stale-lock recovery automation and multi-process soak tests;
- fully reproducible artifacts across toolchains;
- working-tree encodings and diff/merge attribute drivers.
