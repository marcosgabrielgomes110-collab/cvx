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

Repository-level scenarios still to automate:

| Operation | Data size | Status |
|---|---:|---|
| `status` | 10k files | Planned |
| `status` | 100k files | Planned |
| `log --oneline` | 100k commits | Planned |
| `switch` | 10k-file tree | Planned |
| `clone` | 100k-object repository | Planned |

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
