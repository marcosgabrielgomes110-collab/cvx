# Benchmarks

Performance numbers are part of CVX's compatibility contract: a version control
engine that is correct but unusable on real repositories is not finished.

## How to report a benchmark

Use this format in pull requests and release notes:

```markdown
# Read benchmark

**Environment:** Arch Linux x86_64, kernel 6.12, 32 GiB RAM
**Language:** Rust 1.99.0, release profile
**Data:** 1,000,000 records

| Operation | Time |
|---|---:|
| Sequential read | 120 ms |
| Index lookup | 3 ms |

> Values are illustrative. Run the benchmark suite on your machine
> to obtain real results.
```

Always include:

- exact hardware and operating system;
- Rust version and build profile;
- repository size, file count and pack layout;
- warm or cold cache;
- command used to reproduce the result.

## Current automated baseline

Criterion benchmarks are implemented for the engine hot paths:

| Crate | Operation | Data sizes |
|---|---|---|
| `cvx-pack` | pack write and ingest | 100x256, 1000x1024, 5000x256 objects |
| `cvx-index` | index serialize and parse | 1k, 10k, 50k entries |
| `cvx-diff` | unified diff and diffstat | 1k, 10k, 50k lines |

Repository-level benchmarks are included for 10k files; set
`CVX_BENCH_LARGE=1` to also run the 100k-file scenario. `scripts/bench-large.sh`
adds an optional address-space ceiling so large runs fail loudly instead of
silently exhausting memory:

```console
$ CVX_BENCH_LARGE=1 CVX_BENCH_MEMORY_KB=4194304 scripts/bench-large.sh
$ CVX_BENCH_1M=1 CVX_BENCH_MEMORY_KB=4194304 scripts/bench-large.sh
$ CVX_BENCH_FILES=250000 CVX_BENCH_MEMORY_KB=4194304 scripts/bench-large.sh
```

The 1M-file run is deliberately opt-in and is intended for a dedicated runner;
it is not part of the standard CI matrix.

Pack reads are file-backed: `Pack` keeps only the index and a small object
cache in memory, and decompresses objects on demand from the `.pack` file.

| Operation | Data size | Status |
|---|---:|---|
| `status` | 10k files | Implemented |
| `status` | 100k files | Implemented, opt-in |
| `diff` | 10k files | Implemented |
| `log -n 10` | 10k-file repository | Implemented |
| `log --oneline` | 100k commits | Planned |
| `switch` | 10k-file tree | Planned |
| `clone` | 100k-object repository | Planned |

### Example run

Measured on Linux 7.2.11 x86_64, 12 CPUs, Rust 1.99.1, release profile:

| Operation | Data size | Time |
|---|---:|---:|
| `status` | 10k files | 46.6 ms |
| `diff` | 10k files | 40.6 ms |
| `log -n 10` | 10k-file repository | 12.3 µs |

> These are one machine's results, not guarantees. Reproduce with
> `cargo bench -p cvx-cli --bench repository -- --quick`.

Run the current suite with:

```console
$ cargo bench -p cvx-pack -p cvx-index -p cvx-diff
$ cargo bench -p cvx-pack --bench throughput -- --quick
```

Criterion writes HTML reports under `target/criterion/`. Do not compare results
from different machines or build profiles; always include the environment and
command used.

## Performance principles

- Never trade correctness for a micro-optimization.
- Measure before changing data structures.
- Keep large repositories in mind: avoid loading whole packs when a targeted
  read is enough.
- Bound decompression and delta recursion.
- Keep status and diff deterministic and testable.
