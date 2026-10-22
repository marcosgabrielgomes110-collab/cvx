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

Recovery tests live in `crates/cvx-cli/tests/recovery.rs` and verify:

- `HEAD` remains unchanged after an interrupted commit;
- lock files are removed after an interrupted index/ref publication;
- staged changes survive and the operation can be retried;
- an interrupted stash keeps its saved entry and can be applied later;
- an interrupted hard reset leaves `HEAD` unchanged and can be retried;
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

SHA-256 over HTTPS/SSH and mixed-format transfers are rejected explicitly.

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

## Release verification and signing

Release workflows verify every checksum before publishing. When the
`RELEASE_SIGNING_KEY` secret is configured, archives are detached-signed with
GPG and the `.asc` files are published alongside the checksums.
`scripts/verify-release.sh` verifies checksums and any available signatures.

## Remaining hardening

- pack and index streaming with bounded memory;
- SHA-256 over HTTPS/SSH and thin-pack generation;
- fully reproducible artifacts across toolchains;
- working-tree encodings and diff/merge attribute drivers.
