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

Recovery tests live in `crates/cvx-cli/tests/recovery.rs` and verify:

- `HEAD` remains unchanged after an interrupted commit;
- lock files are removed after an interrupted index/ref publication;
- staged changes survive and the operation can be retried;
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

## Remaining hardening

- SHA-256 object format;
- release signing and fully reproducible artifacts;
- working-tree encodings and diff/merge attribute drivers.
