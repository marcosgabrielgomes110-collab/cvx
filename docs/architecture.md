# CVX architecture

> All diagrams below are written in [Mermaid](https://mermaid.js.org/) and render
> directly on GitHub, in Markdown previews with Mermaid support, and at
> [mermaid.live](https://mermaid.live/). They are versioned with the code.

## System overview

CVX is a Git-compatible engine with a Rust CLI, a local object store, an
optional network layer, and GitHub as the first collaboration platform.

```mermaid
flowchart TB
    Dev["Developer / CI / editor"] --> CLI["cvx-cli"]
    CLI --> Repo["cvx-repository<br/>use cases"]

    subgraph Local["Local engine"]
        Repo --> Core["cvx-core<br/>errors, paths, fs, refs rules"]
        Repo --> Config["cvx-config"]
        Repo --> Object["cvx-object"]
        Repo --> ODB["cvx-odb"]
        Repo --> Index["cvx-index"]
        Repo --> Refs["cvx-ref"]
        Repo --> Revision["cvx-revision"]
        Repo --> Diff["cvx-diff"]
        Repo --> Merge["cvx-merge"]
        Repo --> Worktree["cvx-worktree"]
        ODB --> Pack["cvx-pack"]
        Worktree --> Ignore["cvx-ignore"]
    end

    subgraph Remote["Remote transport (HTTPS implemented, SSH planned)"]
        Transport["cvx-transport<br/>HTTPS / SSH"]
        Protocol["cvx-protocol<br/>Git wire protocol"]
        Credential["cvx-credential<br/>secure token store"]
        Transport --> Protocol
        Transport --> Credential
    end

    subgraph GitHub["GitHub (planned)"]
        API["cvx-github-api"]
        Flow["cvx-github-workflow<br/>PRs, issues, releases"]
        Flow --> API
    end

    Repo --> Transport
    Flow -.-> Repo
```

Solid arrows are implemented dependencies. Dotted arrows are planned
integration points.

## Crate dependency graph

Dependencies point from higher-level orchestration toward lower-level formats.
`cvx-core` has no knowledge of Git objects, refs, packs or GitHub.

```mermaid
flowchart LR
    Core["cvx-core"]
    Object["cvx-object"]
    Config["cvx-config"]
    Pack["cvx-pack"]
    ODB["cvx-odb"]
    Index["cvx-index"]
    Refs["cvx-ref"]
    Ignore["cvx-ignore"]
    Revision["cvx-revision"]
    Worktree["cvx-worktree"]
    Diff["cvx-diff"]
    Repo["cvx-repository"]
    Transport["cvx-transport"]
    Protocol["cvx-protocol"]
    Credential["cvx-credential"]
    CLI["cvx-cli"]
    Testkit["cvx-testkit"]

    Object --> Core
    Config --> Core
    Pack --> Object
    ODB --> Pack
    ODB --> Object
    Index --> Object
    Refs --> Object
    Ignore --> Core
    Revision --> ODB
    Revision --> Refs
    Worktree --> Index
    Worktree --> Ignore
    Diff --> Object
    Merge --> Object
    Merge --> Index
    Merge --> Diff
    Repo --> Merge
    Repo --> ODB
    Repo --> Index
    Repo --> Refs
    Repo --> Revision
    Repo --> Worktree
    Repo --> Diff
    Repo --> Config
    Repo --> Transport
    Transport --> Protocol
    Transport --> Credential
    Transport --> Core
    CLI --> Repo
    Testkit --> Repo
```

## Repository discovery

Discovery walks upward until it finds `.cvx/` or `.git/`. CVX prefers `.cvx/`
when both exist and rejects formats that are not implemented.

```mermaid
flowchart TD
    Start(["start directory"]) --> Cvx{".cvx exists?"}
    Cvx -->|yes| OpenCvx["open .cvx<br/>Git-compatible layout"]
    Cvx -->|no| Git{".git exists?"}
    Git -->|no| Parent{parent directory?}
    Parent -->|yes| Start
    Parent -->|no| NotFound["error: not a repository"]

    Git -->|directory| OpenGit["open .git<br/>compatibility mode"]
    Git -->|file| Linked["error: linked worktree unsupported"]
    OpenCvx --> Format{"format supported?"}
    OpenGit --> Format
    Format -->|sha1, files refs| Ready(["repository ready"])
    Format -->|sha256, reftable, bare, partial| Unsupported["explicit unsupported error"]
```

## Object model

```mermaid
classDiagram
    class ObjectId {
        +bytes
        +from_hex()
        +to_hex()
        +short()
    }

    class ObjectKind {
        <<enumeration>>
        Blob
        Tree
        Commit
        Tag
    }

    class Tree {
        +entries()
        +parse()
        +serialize()
    }

    class TreeEntry {
        +mode
        +name
        +oid
    }

    class Commit {
        +tree
        +parents
        +author
        +committer
        +message
    }

    class Signature {
        +name
        +email
        +timestamp
        +offset_minutes
    }

    class Tag {
        +object
        +kind
        +name
        +tagger
        +message
    }

    class Index {
        +entries
        +parse()
        +serialize()
    }

    class IndexEntry {
        +path
        +mode
        +oid
        +stage
        +stat
    }

    Tree "1" o-- "0..*" TreeEntry
    TreeEntry --> ObjectId
    Commit --> ObjectId
    Commit --> Signature
    Tag --> ObjectId
    Tag --> Signature
    Index "1" o-- "0..*" IndexEntry
    IndexEntry --> ObjectId
```

## Commit flow

`cvx commit` writes immutable objects first, then publishes the branch
reference atomically. A failure before the ref update leaves unreachable
objects, never a partially published commit.

```mermaid
sequenceDiagram
    autonumber
    actor Dev
    participant CLI as cvx-cli
    participant Repo as cvx-repository
    participant Index as cvx-index
    participant ODB as cvx-odb
    participant Refs as cvx-ref

    Dev->>CLI: cvx commit -m "message"
    CLI->>Repo: commit(options)
    Repo->>Index: read index
    Index-->>Repo: staged entries
    Repo->>Repo: build tree nodes
    Repo->>ODB: write tree objects
    ODB-->>Repo: tree ids
    Repo->>ODB: write commit object
    ODB-->>Repo: commit id
    Repo->>Refs: update ref (compare-and-swap)
    Refs-->>Repo: ref published
    Repo->>Refs: append reflog
    Repo-->>CLI: commit outcome
    CLI-->>Dev: [branch] short-id subject
```

## Status and diff pipeline

```mermaid
flowchart LR
    Head["HEAD commit tree"] --> Compare1["compare trees"]
    Index["Index stage 0"] --> Compare1
    Index --> Compare2["compare content and mode"]
    Worktree["Worktree files"] --> Compare2
    Compare1 --> Staged["staged changes"]
    Compare2 --> Unstaged["unstaged changes"]
    Walk["Ignore-aware walk"] --> Untracked["untracked paths"]
    Conflict["Index stages 1/2/3"] --> Conflicts["conflicts"]
    Staged --> Status["cvx status"]
    Unstaged --> Status
    Untracked --> Status
    Conflicts --> Status
    Compare1 --> Diff["cvx diff"]
    Compare2 --> Diff
    Walk --> Diff
```

## Checkout and branch switching

```mermaid
sequenceDiagram
    autonumber
    actor Dev
    participant CLI as cvx-cli
    participant Repo as cvx-repository
    participant Index as index.lock
    participant WT as worktree
    participant Refs as cvx-ref

    Dev->>CLI: cvx switch feature
    CLI->>Repo: switch(target)
    Repo->>Index: acquire lock
    Repo->>Repo: check status and collisions
    alt dirty or untracked collision
        Repo-->>CLI: error (use --force to discard)
    else safe
        Repo->>WT: write target files safely
        Repo->>WT: remove files absent from target
        Repo->>Index: write new index and publish
        Repo->>Refs: update HEAD symbolically
        Repo->>Refs: append HEAD reflog
        Repo-->>CLI: switched
    end
```

## Merge flow

`cvx merge` compares the merge base with both tips. Non-overlapping changes are
combined, while conflicting paths receive markers in the worktree and stages
1/2/3 in the index. A clean merge creates a two-parent commit; conflicts leave
`MERGE_HEAD` and `MERGE_MSG` behind for `--continue` or `--abort`.

```mermaid
sequenceDiagram
    autonumber
    actor Dev
    participant CLI as cvx-cli
    participant Repo as cvx-repository
    participant Rev as cvx-revision
    participant Merge as cvx-merge
    participant Index as index.lock
    participant WT as worktree
    participant ODB as cvx-odb

    Dev->>CLI: cvx merge feature
    CLI->>Repo: merge(revision)
    Repo->>Repo: check clean index and worktree
    Repo->>Rev: merge_base(HEAD, feature)
    alt target is ancestor
        Repo-->>CLI: already up to date
    else HEAD is ancestor
        Repo->>WT: fast-forward worktree
        Repo->>Index: publish target index
        Repo-->>CLI: Fast-forward
    else diverged
        Repo->>Merge: merge files (base, ours, theirs)
        alt clean
            Repo->>ODB: write merged blobs and commit
            Repo-->>CLI: merge commit
        else conflicts
            Repo->>WT: write conflict markers
            Repo->>Index: write stages 1/2/3
            Repo->>Repo: write MERGE_HEAD and MERGE_MSG
            Repo-->>CLI: conflict paths
        end
    end
```

## Future remote flow

This diagram documents intended behavior, not current support.

```mermaid
sequenceDiagram
    autonumber
    actor Dev
    participant CVX
    participant GitHub

    Dev->>CVX: cvx push origin main
    CVX->>GitHub: discover refs and capabilities
    GitHub-->>CVX: refs, protocol version
    CVX->>CVX: negotiate objects already present
    CVX->>CVX: build packfile
    CVX->>GitHub: send pack and ref update
    GitHub-->>CVX: accept or reject
    CVX->>CVX: update remote-tracking refs
    CVX-->>Dev: result summary
```

## Where to go next

- [Compatibility matrix](compatibility.md)
- [Detailed design notes](architecture/overview.md)
- [Repository layout](spec/repository-layout.md)
- [Workflows](workflows.md)
- [Benchmarks](benchmarks.md)
