# GitHub integration

CVX uses two independent GitHub paths:

- Git transport over HTTPS or SSH for clone, fetch and push;
- REST and GraphQL APIs for collaboration.

Tokens are resolved from `CVX_USERNAME` / `CVX_TOKEN` (or `GITHUB_TOKEN`), the
operating-system keychain, or a user-only `0600` credential file. Tokens are
never written to repository configuration or printed by the credential store.

## Token login

```console
$ printf '%s\n' "$GITHUB_TOKEN" | cvx github login
$ cvx github status
$ cvx github logout
```

For HTTPS push, store the same token for the Git host:

```console
$ printf '%s\n' "$GITHUB_TOKEN" | cvx credential store github.com
```

## OAuth device flow

The device flow requires a GitHub OAuth app client id. Create an OAuth app in
GitHub settings, enable device flow, and export the client id:

```console
$ export CVX_GITHUB_CLIENT_ID=Iv1.example
$ cvx github login --oauth --scope repo --scope workflow --scope read:org
```

If no `--scope` is supplied, CVX requests `repo`, `workflow` and `read:org`.
CVX prints the verification URL and user code, polls GitHub, stores the token
securely and prints the authenticated account.

Inspect the scopes granted to the current token:

```console
$ cvx github scopes
$ cvx github scopes --require repo --require workflow
```

Fine-grained tokens may not expose OAuth scope headers; use the token settings
page to verify their permissions.

## Repositories and collaborators

```console
$ cvx github repo create my-project --private --description "Internal service"
$ cvx github repo show
$ cvx github collaborator list
$ cvx github collaborator add alice --permission push
$ cvx github collaborator permission alice
$ cvx github collaborator remove alice
```

Permissions are `pull`, `triage`, `push`, `maintain` and `admin`.

## Issues, assignees and milestones

```console
$ cvx github milestone create "v1.0" --description "First stable release" --due-on 2027-01-31
$ cvx github milestone list
$ cvx github issue create "Crash when merging" --body "Steps to reproduce..." \
    --assignee alice --milestone 1
$ cvx github issue assign 12 alice bob
$ cvx github issue unassign 12 bob
$ cvx github milestone close 1
```

## Pull requests and reviews

```console
$ cvx github pr create "Add transport" --head feature/transport
$ cvx github pr list
$ cvx github pr review 42 --approve --body "Looks good"
$ cvx github pr reviews 42
$ cvx github pr merge 42 --method squash
```

Review threads are read and resolved through GraphQL:

```console
$ cvx github pr threads 42
$ cvx github pr resolve <thread-node-id>
$ cvx github pr unresolve <thread-node-id>
```

## Checks and required status checks

```console
$ cvx github checks
$ cvx github checks --reference HEAD
$ cvx github checks required --branch main
$ cvx github checks require "Test (ubuntu-latest)" "Test (windows-latest)" --branch main --strict
```

`checks required` needs branch protection to be enabled. `checks require`
replaces the required check contexts for that branch and may require repository
administration permission.

## Releases and assets

```console
$ cvx github release create v1.0.0 --name "CVX 1.0.0" --body "Release notes"
$ cvx github release create v1.0.1 --asset target/release/cvx --asset dist/checksums.txt
$ cvx github release list
$ cvx github release assets v1.0.0
$ cvx github release upload v1.0.0 target/release/cvx --label linux-x64 --clobber
$ cvx github release delete-asset <asset-id>
```

Uploads read the file into memory. Use `--clobber` to replace an asset with the
same name.

## GraphQL

```console
$ cvx github graphql --query 'query { viewer { login } }'
$ cvx github graphql --query 'query($owner:String!,$repo:String!){ repository(owner:$owner,name:$repo){ name } }' \
    --variables '{"owner":"marcosgabrielgomes110-collab","repo":"cvx"}'
```

`--variables-file` accepts a JSON object. GraphQL errors are reported as
command failures rather than raw API responses.

## Limitations

- list endpoints return the first page (up to 100 items for most resources);
- review-thread queries return the first 100 threads and comments;
- branch protection must already exist before required checks can be replaced;
- release uploads are buffered in memory;
- OAuth browser callbacks and GitHub App installation tokens are not
  implemented;
- fine-grained tokens may not report OAuth scopes in response headers.
