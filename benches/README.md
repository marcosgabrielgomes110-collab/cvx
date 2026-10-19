# Benchmarks

The benchmark policy, reporting format and planned scenarios are documented in
[docs/benchmarks.md](../docs/benchmarks.md).

Planned Criterion benchmarks:

- object hashing and loose-object writes;
- packfile lookup and delta resolution;
- index parse/write;
- `status` and `diff` on 10k and 100k files;
- `switch` on 10k-file trees;
- `log` on 100k commits.

Run the suite with:

```console
$ cargo bench --workspace
```
