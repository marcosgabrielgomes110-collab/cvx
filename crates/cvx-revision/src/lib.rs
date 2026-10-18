//! Revision parsing and commit graph traversal.
//!
//! The first milestone walks commits in committer-date order. Topological
//! ordering, revision ranges, reflog selectors and replace refs are added with
//! the workflow features that need them.

#![forbid(unsafe_code)]

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use cvx_core::Result;
use cvx_object::{Commit, ObjectId};

/// A commit visited by the log walker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Commit identifier.
    pub oid: ObjectId,
    /// Parsed commit.
    pub commit: Commit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeapItem {
    timestamp: i64,
    oid: ObjectId,
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp
            .cmp(&other.timestamp)
            .then_with(|| other.oid.cmp(&self.oid))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Walks a commit graph starting at `start`.
///
/// `read` is called only for commits not already cached. The result is limited
/// by `limit` when provided. Missing commits and malformed objects surface as
/// errors instead of being skipped.
pub fn walk_commits<F>(start: ObjectId, limit: Option<usize>, mut read: F) -> Result<Vec<LogEntry>>
where
    F: FnMut(ObjectId) -> Result<Commit>,
{
    if limit == Some(0) {
        return Ok(Vec::new());
    }

    let mut cache = HashMap::new();
    let start_commit = read(start)?;
    let mut heap = BinaryHeap::new();
    heap.push(HeapItem {
        timestamp: start_commit.committer.timestamp,
        oid: start,
    });
    cache.insert(start, start_commit);

    let mut visited = HashSet::new();
    let mut entries = Vec::new();

    while let Some(item) = heap.pop() {
        if !visited.insert(item.oid) {
            continue;
        }

        let commit = cache
            .get(&item.oid)
            .expect("commit is cached before being queued")
            .clone();
        entries.push(LogEntry {
            oid: item.oid,
            commit: commit.clone(),
        });

        if limit.is_some_and(|limit| entries.len() >= limit) {
            break;
        }

        for parent in &commit.parents {
            if !cache.contains_key(parent) {
                let parent_commit = read(*parent)?;
                cache.insert(*parent, parent_commit);
            }
            if !visited.contains(parent) {
                heap.push(HeapItem {
                    timestamp: cache[parent].committer.timestamp,
                    oid: *parent,
                });
            }
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cvx_object::{Commit, Signature};

    fn oid(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; 20])
    }

    fn commit(parents: Vec<ObjectId>, timestamp: i64) -> Commit {
        let signature = Signature::new("A", "a@b", timestamp, 0);
        Commit {
            tree: oid(0x99),
            parents,
            author: signature.clone(),
            committer: signature,
            message: "message\n".to_owned(),
            extra_headers: Vec::new(),
        }
    }

    #[test]
    fn walks_in_committer_date_order() {
        let root = oid(1);
        let old = oid(2);
        let new = oid(3);
        let commits = HashMap::from([
            (root, commit(Vec::new(), 10)),
            (old, commit(vec![root], 20)),
            (new, commit(vec![old], 30)),
        ]);

        let entries = walk_commits(new, None, |id| Ok(commits[&id].clone())).expect("walk");
        let ids: Vec<ObjectId> = entries.iter().map(|entry| entry.oid).collect();
        assert_eq!(ids, vec![new, old, root]);
    }

    #[test]
    fn honors_limit() {
        let root = oid(1);
        let child = oid(2);
        let commits = HashMap::from([
            (root, commit(Vec::new(), 10)),
            (child, commit(vec![root], 20)),
        ]);

        let entries = walk_commits(child, Some(1), |id| Ok(commits[&id].clone())).expect("walk");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].oid, child);
    }

    #[test]
    fn visits_merge_parents_once() {
        let root = oid(1);
        let left = oid(2);
        let right = oid(3);
        let merge = oid(4);
        let commits = HashMap::from([
            (root, commit(Vec::new(), 10)),
            (left, commit(vec![root], 20)),
            (right, commit(vec![root], 21)),
            (merge, commit(vec![left, right], 30)),
        ]);

        let entries = walk_commits(merge, None, |id| Ok(commits[&id].clone())).expect("walk");
        let count = entries.iter().filter(|entry| entry.oid == root).count();
        assert_eq!(count, 1);
        assert_eq!(entries.len(), 4);
    }
}
