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

## Current local baseline

The initial local-core benchmark suite is not automated yet. The planned
baseline covers:

| Operation | Data size | Status |
|---|---:|---|
| Loose object write | 1 MiB blob | Planned |
| Loose object read | 1 MiB blob | Planned |
| Packed object read | 1 MiB blob, delta chain | Planned |
| Index parse/write | 10k entries | Planned |
| `status` | 10k files | Planned |
| `status` | 100k files | Planned |
| `diff` | 1k changed lines | Planned |
| `log --oneline` | 100k commits | Planned |
| `switch` | 10k-file tree | Planned |

Illustrative target format, not measured values:

| Operation | Time |
|---|---:|
| Index parse (10k entries) | 8 ms |
| `status` (10k files, warm cache) | 40 ms |
| `log --oneline` (100k commits) | 90 ms |

> These numbers are placeholders used to define the reporting format. Do not
> cite them as CVX performance results.

## Running benchmarks

Once `criterion` benches are added:

```console
$ cargo bench --workspace
```

Until then, use release-mode smoke tests with temporary repositories and record
the commands in the pull request.

## Performance principles

- Never trade correctness for a micro-optimization.
- Measure before changing data structures.
- Keep large repositories in mind: avoid loading whole packs when a targeted
  read is enough.
- Bound decompression and delta recursion.
- Keep status and diff deterministic and testable.
