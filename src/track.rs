/*! The tracks map assigns a track to each commit.

It is expensive to compute because changes in one end may affect the other end.
Fortunately it can be computed incrementally.
*/

use std::collections::HashMap;

use git2::Commit;
use git2::Oid;

use crate::layout::BranchVis;



/**
    Group commits into tracks. A track is a sequence of commits
    where every commit has a parent inside the track, except the oldest
    commit.
*/
pub struct TrackMap {
    /// List of commits in the map. Stores parent relations.
    pub commits: Vec<CommitInfo>,
    /// Mapping from commit id to index in `commits`
    pub indices: HashMap<Oid, usize>,
    /// All detected branches and tags, including merged and deleted
    pub all_branches: Vec<BranchInfo>,
}

/// Represents a branch (real or derived from merge summary).
pub struct BranchInfo {
    pub target: Oid,
    pub merge_target: Option<Oid>,
    pub source_branch: Option<usize>,
    pub target_branch: Option<usize>,
    pub name: String,
    pub persistence: u8,
    /// Is branch a remote reference
    pub is_remote: bool,
    /// Is branch derived from a merge summary
    pub is_merged: bool,
    /// Is branch a tag reference
    pub is_tag: bool,
    pub visual: BranchVis,
    pub range: (Option<usize>, Option<usize>),
}
impl BranchInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target: Oid,
        merge_target: Option<Oid>,
        name: String,
        persistence: u8,
        is_remote: bool,
        is_merged: bool,
        is_tag: bool,
        visual: BranchVis,
        end_index: Option<usize>,
    ) -> Self {
        BranchInfo {
            target,
            merge_target,
            target_branch: None,
            source_branch: None,
            name,
            persistence,
            is_remote,
            is_merged,
            is_tag,
            visual,
            range: (end_index, None),
        }
    }
}


/// Represents a commit.
pub struct CommitInfo {
    pub oid: Oid,
    pub is_merge: bool,
    pub parents: [Option<Oid>; 2],
    pub children: Vec<Oid>,
    pub branches: Vec<usize>,
    pub tags: Vec<usize>,
    /// Index into TrackMap.all_branches
    pub branch_trace: Option<usize>,
}

impl CommitInfo {
    pub fn new(commit: &Commit) -> Self {
        CommitInfo {
            oid: commit.id(),
            is_merge: commit.parent_count() > 1,
            parents: [commit.parent_id(0).ok(), commit.parent_id(1).ok()],
            children: Vec::new(),
            branches: Vec::new(),
            tags: Vec::new(),
            branch_trace: None,
        }
    }
}

