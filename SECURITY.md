# Security policy

## Supported versions

CVX is currently in the `0.1.x` local-core alpha. Security fixes are applied to
the `main` branch and to the latest published release.

## Reporting a vulnerability

Please do **not** open a public issue for security problems.

Use GitHub's private vulnerability reporting:

1. Open the repository's **Security** tab.
2. Select **Report a vulnerability**.
3. Include the affected version, a minimal reproduction and the expected impact.

You can also use the repository's [private security advisory form][advisory].

## Scope

Security-sensitive areas include:

- object, pack and index parsing;
- malformed or malicious repositories;
- path traversal and symlink handling during `restore`, `checkout` and
  `switch`;
- ref-name and configuration parsing;
- credential and token handling once network transports exist;
- decompression and delta-chain resource limits.

## Design commitments

- CVX never executes the `git` binary at runtime.
- Unsupported formats fail explicitly instead of being guessed.
- Object hashes are verified when objects are read.
- Decompression and delta recursion are bounded.
- Commits, refs and index writes use lock files and atomic replacement.
- Credentials must never be written to logs or repository files.

## Disclosure

We aim to acknowledge reports within a few days. Please allow time for a fix and
a coordinated release before publishing details.

[advisory]: https://github.com/marcosgabrielgomes110-collab/cvx/security/advisories/new
