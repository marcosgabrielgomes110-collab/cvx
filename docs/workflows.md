# CVX workflows

These diagrams describe the implemented local workflow. Remote and GitHub
flows are marked as planned.

## Initialize a repository

```mermaid
sequenceDiagram
    autonumber
    actor Dev
    participant CLI as cvx-cli
    participant Repo as cvx-repository
    participant FS as filesystem

    Dev->>CLI: cvx init
    CLI->>Repo: init(storage=cvx)
    Repo->>FS: create .cvx layout and HEAD
    Repo->>FS: write config and info/exclude
    Repo-->>CLI: repository
    CLI-->>Dev: Initialized empty CVX repository
```

## Stage and commit

```mermaid
sequenceDiagram
    autonumber
    actor Dev
    participant CLI as cvx-cli
    participant Repo as cvx-repository
    participant Index as cvx-index
    participant ODB as cvx-odb
    participant Refs as cvx-ref

    Dev->>CLI: cvx add .
    CLI->>Repo: add(paths)
    Repo->>ODB: write blobs
    Repo->>Index: update entries under index.lock
    CLI-->>Dev: (silent on success)

    Dev->>CLI: cvx commit -m "message"
    CLI->>Repo: commit(message)
    Repo->>Index: read staged entries
    Repo->>ODB: write trees and commit
    Repo->>Refs: publish branch with compare-and-swap
    Repo->>Refs: append reflog
    CLI-->>Dev: [main] short-id message
```

## Inspect changes

```mermaid
flowchart LR
    Work["worktree"] --> Add["cvx add"]
    Add --> Index["index"]
    Index --> Commit["cvx commit"]
    Commit --> Head["HEAD"]

    Index -- "cvx diff" --> Diff["unified diff"]
    Work -- "cvx diff" --> Diff
    Head -- "cvx diff --cached" --> Cached["staged diff"]
    Index --> Cached

    Head -- "cvx status" --> Status["staged / unstaged / untracked"]
    Index --> Status
    Work --> Status
```

## Restore safely

```mermaid
flowchart TD
    Restore(["cvx restore / cvx checkout -- path"]) --> Source{"source?"}
    Source -->|index| ExtractIndex["read index entries"]
    Source -->|HEAD or revision| ExtractTree["read tree entries"]
    ExtractIndex --> Check["check pathspecs and untracked collisions"]
    ExtractTree --> Check
    Check -->|collision| Refuse["error unless --force"]
    Check -->|safe| Write["atomic file/symlink writes"]
    Write --> Remove["remove tracked paths absent from source"]
    Remove --> Done(["worktree and/or index updated"])
```

## Branch and switch

```mermaid
flowchart TD
    Start(["cvx switch feature"]) --> Exists{"branch exists?"}
    Exists -->|no| Error["invalid reference"]
    Exists -->|yes| Clean{"staged/unstaged changes?"}
    Clean -->|yes| Refuse["error; use --force to discard"]
    Clean -->|no| Collision{"untracked collision?"}
    Collision -->|yes| Refuse
    Collision -->|no| Checkout["write target tree<br/>remove old tracked paths"]
    Checkout --> Index["replace index"]
    Index --> Head["update HEAD + reflog"]
    Head --> Done(["switched"])
```

## Planned clone/push

```mermaid
sequenceDiagram
    autonumber
    actor Dev
    participant CVX
    participant Remote as Git server / GitHub

    Dev->>CVX: cvx clone URL
    CVX->>Remote: discover refs and capabilities
    Remote-->>CVX: refs and pack
    CVX->>CVX: verify and index pack
    CVX->>CVX: write objects, refs and worktree
    CVX-->>Dev: repository ready

    Dev->>CVX: cvx push origin main
    CVX->>Remote: negotiate and send pack
    Remote-->>CVX: update accepted
    CVX-->>Dev: pushed
```
