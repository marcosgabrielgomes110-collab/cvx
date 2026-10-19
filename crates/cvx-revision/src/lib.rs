//! Revision parsing and commit graph traversal.
//!
//! The first milestone walks commits in committer-date order. Topological
//! ordering, revision ranges, reflog selectors and replace refs are added with
//! the workflow features that need them.

#![forbid(unsafe_code)]

use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

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

/// Returns whether `ancestor` is reachable from `descendant`.
pub fn is_ancestor<F>(ancestor: ObjectId, descendant: ObjectId, mut read: F) -> Result<bool>
where
    F: FnMut(ObjectId) -> Result<Commit>,
{
    if ancestor == descendant {
        return Ok(true);
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([descendant]);
    visited.insert(descendant);

    while let Some(oid) = queue.pop_front() {
        let commit = read(oid)?;
        for parent in commit.parents {
            if parent == ancestor {
                return Ok(true);
            }
            if visited.insert(parent) {
                queue.push_back(parent);
            }
        }
    }

    Ok(false)
}

/// Finds the best common ancestor of two commits.
///
/// The result is the common ancestor with the smallest combined distance from
/// both tips. Ancestors that are themselves reachable from another common
/// ancestor are discarded, so criss-cross histories return a lowest common
/// ancestor instead of an arbitrary older commit.
pub fn merge_base<F>(left: ObjectId, right: ObjectId, mut read: F) -> Result<Option<ObjectId>>
where
    F: FnMut(ObjectId) -> Result<Commit>,
{
    if left == right {
        return Ok(Some(left));
    }

    let mut cache = HashMap::new();
    let left_distances = ancestor_distances(left, &mut read, &mut cache)?;
    let right_distances = ancestor_distances(right, &mut read, &mut cache)?;

    let common: Vec<ObjectId> = left_distances
        .keys()
        .filter(|oid| right_distances.contains_key(*oid))
        .copied()
        .collect();
    if common.is_empty() {
        return Ok(None);
    }

    let common_set: HashSet<ObjectId> = common.iter().copied().collect();
    let mut ancestor_sets: HashMap<ObjectId, HashSet<ObjectId>> = HashMap::new();
    for oid in &common {
        ancestor_sets.insert(*oid, ancestor_set(*oid, &mut read, &mut cache)?);
    }

    let mut candidates: Vec<ObjectId> = common
        .into_iter()
        .filter(|candidate| {
            !common_set.iter().any(|other| {
                other != candidate
                    && ancestor_sets
                        .get(other)
                        .is_some_and(|ancestors| ancestors.contains(candidate))
            })
        })
        .collect();

    candidates.sort_by_key(|oid| (left_distances[oid] + right_distances[oid], *oid));
    Ok(candidates.first().copied())
}

fn ancestor_distances<F>(
    start: ObjectId,
    read: &mut F,
    cache: &mut HashMap<ObjectId, Commit>,
) -> Result<HashMap<ObjectId, usize>>
where
    F: FnMut(ObjectId) -> Result<Commit>,
{
    let mut distances = HashMap::new();
    let mut queue = VecDeque::from([(start, 0usize)]);
    distances.insert(start, 0);

    while let Some((oid, distance)) = queue.pop_front() {
        if let Entry::Vacant(entry) = cache.entry(oid) {
            entry.insert(read(oid)?);
        }
        let parents = cache[&oid].parents.clone();
        for parent in parents {
            if let Entry::Vacant(entry) = distances.entry(parent) {
                entry.insert(distance + 1);
                queue.push_back((parent, distance + 1));
            }
        }
    }

    Ok(distances)
}

fn ancestor_set<F>(
    start: ObjectId,
    read: &mut F,
    cache: &mut HashMap<ObjectId, Commit>,
) -> Result<HashSet<ObjectId>>
where
    F: FnMut(ObjectId) -> Result<Commit>,
{
    let mut ancestors = HashSet::new();
    let mut queue = VecDeque::from([start]);

    while let Some(oid) = queue.pop_front() {
        if !ancestors.insert(oid) {
            continue;
        }
        if let Entry::Vacant(entry) = cache.entry(oid) {
            entry.insert(read(oid)?);
        }
        for parent in &cache[&oid].parents {
            if !ancestors.contains(parent) {
                queue.push_back(*parent);
            }
        }
    }

    Ok(ancestors)
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
    fn finds_merge_base_for_linear_and_diverged_history() {
        let root = oid(1);
        let left = oid(2);
        let right = oid(3);
        let commits = HashMap::from([
            (root, commit(Vec::new(), 10)),
            (left, commit(vec![root], 20)),
            (right, commit(vec![root], 21)),
        ]);

        let base = merge_base(left, right, |id| Ok(commits[&id].clone())).expect("merge base");
        assert_eq!(base, Some(root));

        let base = merge_base(root, left, |id| Ok(commits[&id].clone())).expect("merge base");
        assert_eq!(base, Some(root));
    }

    #[test]
    fn detects_ancestry() {
        let root = oid(1);
        let child = oid(2);
        let other = oid(3);
        let commits = HashMap::from([
            (root, commit(Vec::new(), 10)),
            (child, commit(vec![root], 20)),
            (other, commit(Vec::new(), 30)),
        ]);

        assert!(is_ancestor(root, child, |id| Ok(commits[&id].clone())).expect("ancestor check"));
        assert!(!is_ancestor(child, root, |id| Ok(commits[&id].clone())).expect("ancestor check"));
        assert!(!is_ancestor(root, other, |id| Ok(commits[&id].clone())).expect("ancestor check"));
    }

    #[test]
    fn merge_base_of_unrelated_history_is_none() {
        let left = oid(1);
        let right = oid(2);
        let commits = HashMap::from([
            (left, commit(Vec::new(), 10)),
            (right, commit(Vec::new(), 20)),
        ]);

        assert_eq!(
            merge_base(left, right, |id| Ok(commits[&id].clone())).expect("merge base"),
            None
        );
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
