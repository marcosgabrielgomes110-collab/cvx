# Security policy

## Supported versions

CVX is currently in the `0.6.x` beta series. Security fixes are applied to the
`main` branch and to the latest published release.

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
- HTTPS protocol framing, redirects and token handling;
- SSH process invocation, host-key policy and remote path quoting;
- keychain and credential-file storage permissions and secret leakage;
- GitHub API token handling, request authorization and destructive operations;
- shallow boundaries and partial-clone lazy object fetching;
- decompression, pack ingestion and delta-chain resource limits;
- test-only failpoint injection (`CVX_FAILPOINT`) and crash recovery.

## Design commitments

- CVX never executes the `git` binary at runtime.
- Unsupported formats fail explicitly instead of being guessed.
- Object hashes are verified when objects are read.
- Decompression and delta recursion are bounded.
- Commits, refs and index writes use lock files and atomic replacement.
- Credentials must never be written to logs or repository files.
- Stored credentials use a user-only file (`0600` on Unix) outside the
  repository; insecure permissions are rejected on read.
- HTTP redirects are not followed for authenticated transport requests.
- Tokens are read from standard input or environment variables, not command
  line arguments.
- SSH host keys use `StrictHostKeyChecking=accept-new`; authentication is
  delegated to OpenSSH and `ssh-agent`.
- The keychain is preferred; file fallback is `0600` and rejected if group or
  other permissions are present.

## Disclosure

We aim to acknowledge reports within a few days. Please allow time for a fix and
a coordinated release before publishing details.

[advisory]: https://github.com/marcosgabrielgomes110-collab/cvx/security/advisories/new
