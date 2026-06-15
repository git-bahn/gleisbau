/*! The tracks map assigns a track to each commit.

It is expensive to compute because changes in one end may affect the other end.
Fortunately it can be computed incrementally.
*/

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use regex::Regex;

use crate::define_u32_index;
pub use crate::settings::MergePatterns;

const ORIGIN: &str = "origin/";
pub const FORK: &str = "fork/";
const MAX_PERSISTENCE: u8 = 255;
const AUTO_PERSISTENCE: u8 = 254;

define_u32_index!(
    /** Index into [TrackMap].all_branches.

    This index is 4 bytes instead of 8 bytes for normal usize.
    We store an index to a commit 3 times in [CommitInfo].
    For a repository with N commits, this will save 4 * 3 * N bytes of memory
    */
    pub struct Binx;
);

/// Index into [TrackMap].commits
pub type Cinx = usize;

/**
    Group commits into tracks. A track is a sequence of commits
    where every commit has a parent inside the track, except the oldest
    commit.
*/
pub struct TrackMap<Oid>
where
    Oid: Clone + Eq,
{
    /// List of commits in the map. Stores parent relations.
    pub commits: Vec<CommitInfo<Oid>>,
    /// Mapping from commit id to index in `commits`
    pub indices: HashMap<Oid, usize>,
    /// All detected branches and tags, including merged and deleted
    pub all_branches: Vec<BranchInfo<Oid>>,
}
impl<Oid: Clone + Eq + Hash> Default for TrackMap<Oid> {
    fn default() -> Self {
        Self::new()
    }
}
impl<Oid: Clone + Eq + Hash> TrackMap<Oid> {
    /// Create an empty TrackMap
    pub fn new() -> Self {
        log::trace!("TrackMap::new()");
        Self {
            commits: vec![],
            indices: HashMap::new(),
            all_branches: vec![],
        }
    }
    /// Append a commit and return the index it got.
    /// Does not deal with branch information.
    pub fn add_commit(&mut self, commit: CommitInfo<Oid>) -> usize {
        let commit_index = self.commits.len();
        self.indices.insert(commit.oid.clone(), commit_index);
        self.commits.push(commit);
        // Make sure every child has a branch
        commit_index
    }
    /// Get at reference to the commit with this Oid
    pub fn commit_at(&self, commit_id: &Oid) -> Option<&CommitInfo<Oid>> {
        self.indices
            .get(commit_id)
            .map(|&commit_index| &self.commits[commit_index])
    }
    /// Get a reference to the branch associated with the commit
    pub fn branch_at(&self, commit_id: &Oid) -> Option<&BranchInfo<Oid>> {
        self.commit_at(commit_id)
            .and_then(|commit_info| commit_info.branch_trace)
            .map(|branch_index| &self.all_branches[branch_index])
    }
    /// Push a branch onto tracks.all_branches and return its index.
    ///
    /// *Note*: You must update the target commit to point
    /// at this branch.
    fn store_branch(&mut self, branch: BranchInfo<Oid>) -> Binx {
        let branch_id = self.all_branches.len();
        self.all_branches.push(branch);
        Binx::new(branch_id)
    }
}

/// Gleisbau branch come in several variants, that indicate their origin
#[derive(PartialEq)]
pub enum BranchInfoType {
    // Reference to remote branch
    Remote,
    // Local branch
    Local,
    // Auto-branch derived from a merge commit, without a git branch label
    Derived,
    // Not a git-branch but a git-tag
    Tag,
}

/// Represents a branch (real or derived from merge summary).
pub struct BranchInfo<Oid> {
    /// The Object ID that the branch/tag points at. Used as the grand-child to start tracing the branch towards grand-parent.
    pub target: Oid,
    /// The merge commit that defined this branch. It has target as a parent.
    pub merge_target: Option<Oid>,
    /// Layout: A branch that this branch was forked from
    pub source_branch: Option<Binx>,
    /// Layout: A branch that merged/continues this branch
    pub target_branch: Option<Binx>,
    /// Name of branch. Either the branch/tag name, or derived from a merge-commit message.
    pub name: String,
    /// When two branches want the same commit, the one that is most persistent wins. In this case lower numbers wins.
    pub persistence: u8,
    /// Is branch a remote reference
    pub is_remote: bool,
    /// Is branch derived from a merge summary
    pub is_merged: bool,
    /// Is branch a tag reference
    pub is_tag: bool,
    /** Layout: Row range occucpied by branch. It is defined as low and
    high index into TrackMap.commits, but in Layout this is used as an
    abstract grid. */
    pub range: (Option<usize>, Option<usize>),
}
impl<Oid> BranchInfo<Oid> {
    pub fn new(
        target: Oid,
        merge_target: Option<Oid>,
        name: String,
        persistence: u8,
        branch_type: BranchInfoType,
        end_index: Option<usize>,
    ) -> Self {
        let is_remote = branch_type == BranchInfoType::Remote;
        let is_merged = branch_type == BranchInfoType::Derived;
        let is_tag = branch_type == BranchInfoType::Tag;
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
            range: (end_index, None),
        }
    }
}

/// Represents a commit.
pub struct CommitInfo<Oid> {
    /// Commit object identifier from git2
    pub oid: Oid,
    /// Parents of commit. Filled in first pass.
    ///
    /// *Note*: May point to commits not yet in TrackMap
    pub parents: Vec<Oid>,
    /// Children of commit. Filled in second pass
    pub children: Vec<Oid>,
    /// Index into TrackMap.all_branches
    pub branch_trace: Option<Binx>,
}
impl<Oid> CommitInfo<Oid> {
    /// True if commit has multiple parents
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

/** Data about a missing parent.

    Mising parents occur when we walk towards the boundary of what is known.

    - [add_child](MissingParent::add_child)
      Store relations to parents not yet seen.
    - [set_branch](MissingParent::set_branch)
      Store candidate new branch for a not yet seen commit.

*/
struct MissingParent<Oid> {
    known_children: Vec<Cinx>,
    candidate_branch: Option<BranchInfo<Oid>>,
}
impl<Oid: Clone + Eq + Hash> Default for MissingParent<Oid> {
    fn default() -> Self {
        Self::new()
    }
}
impl<Oid: Clone + Eq + Hash> MissingParent<Oid> {
    pub fn new() -> Self {
        Self {
            known_children: vec![],
            candidate_branch: None,
        }
    }
    /// Store relation to missing parent from a known child. The child should
    /// have the missing parent as primary parent.
    ///
    /// Later, when calling [Self::set_commit_branch], this child is one of the
    /// candidates for extending its branch to include the parent.
    pub fn add_child(&mut self, child_inx: Cinx) {
        self.known_children.push(child_inx);
    }
    /// Set the candidate branch to create here. Does nothing if one already
    /// is set and it has at least as good persistence as the new one.
    ///
    /// This should be called if a commit has merged the missing parent (the
    /// parent is not the primary parent), or if the user has decided a branch
    /// should start here. Remember to set the branch merge_target to indicate if
    /// the child has merged the parent.
    pub fn set_branch(&mut self, branch: BranchInfo<Oid>) {
        let old_branch = &self
            .candidate_branch
            .as_ref()
            // Keep old candidate if it has lower pers order = more persistene
            .filter(|br| br.persistence <= branch.persistence);
        if old_branch.is_none() {
            self.candidate_branch = Some(branch);
        }
    }
    /// Set branch on no-longer-missing parent commit.
    /// It considers the current candidate branch and the branches of
    /// all known children. If candidate branch is used, a new branch starts.
    /// If a child branch is used, it is extended.
    pub fn set_commit_branch(self, tracks: &mut TrackMap<Oid>, commit_inx: Cinx) {
        //
        // Find the most persistent branch that wants this commit
        //

        // Persistence of the best branch so far
        let mut best_pers = self
            .candidate_branch
            .as_ref()
            .map(|br| br.persistence)
            .unwrap_or(MAX_PERSISTENCE); // the least possible persistent
                                         // Child to extend, or None if the candidate is best
        let mut best_extend = None;

        // Check if any child branch is more persistent
        for child_inx in self.known_children {
            let child = &mut tracks.commits[child_inx];
            if child.branch_trace.is_none() {
                panic!("Known child does not yet have a branch");
            }
            let better_child_branch = child
                .branch_trace
                .map(|branch_index| &tracks.all_branches[branch_index])
                .filter(|br| br.persistence < best_pers);
            if let Some(br) = better_child_branch {
                best_pers = br.persistence;
                best_extend = Some(child_inx);
            }
        }

        //
        // Set commit branch
        //

        let branch_trace;
        if let Some(best_child_inx) = best_extend {
            // Extend the best child
            let child = &mut tracks.commits[best_child_inx];
            let branch_index = child
                .branch_trace
                .expect("All commits must have a branch - including child of unknown parent");
            let child_branch = &mut tracks.all_branches[branch_index];
            child_branch.range.1 = Some(commit_inx);
            branch_trace = Some(branch_index)
        } else if let Some(branch_info) = self.candidate_branch {
            // Create candidate branch
            let branch_index = tracks.store_branch(branch_info);
            branch_trace = Some(branch_index);
        } else {
            // No candidates, so we must be at a head.
            // Create a new track/branch starting at this commit

            let target = tracks.commits[commit_inx].oid.clone();
            let next_branch_index = tracks.all_branches.len();
            let name = format!("anonhead-{}", next_branch_index);
            let persistence = AUTO_PERSISTENCE;
            let end_index = Some(commit_inx);

            let new_branch = BranchInfo::new(
                target, // Target is the commit to add.
                None,   // no merge target
                name,
                persistence,
                BranchInfoType::Local, // Default value
                end_index,
            );

            // Add new branch and use it for the commit to add
            let branch_index = tracks.store_branch(new_branch);
            branch_trace = Some(branch_index);
        }

        // Update commit
        tracks.commits[commit_inx].branch_trace = branch_trace;
    }
}

pub type PersistencePatterns = Vec<Regex>;

/**
    Add repository commit information to a [TrackMap].

    This struct is ment to be temporary. Once the repository has been walked
    it can be discarded. It holds information about commit ids that have not
    yet been processed. Once they are available it will update the previously
    seen commits in TrackMap with the missing relations.

    The curent implementation assumes that commits are only added
    during a full repository walk from newest to oldest commit.
    This means that children are always added before their parents.
    This assumption reduces the amount of memory needed for building.
*/
pub struct Builder<Oid: Clone + Eq + Hash> {
    /// Track structure to update
    tracks: Rc<RefCell<TrackMap<Oid>>>,

    /// Merge patterns to use when deriving a branch name from a merge
    merge_patterns: Rc<MergePatterns>,

    /// Branch persistence from branch name pattern
    persistence: PersistencePatterns,

    /// For each missing parent, store information
    /// that should be added when it is found.
    missing_parents: HashMap<Oid, MissingParent<Oid>>,
}
impl<Oid: Clone + Eq + Hash> Builder<Oid> {
    /// Create a builder for the specified TrackMap
    pub fn new(target: Rc<RefCell<TrackMap<Oid>>>) -> Self {
        Self {
            tracks: target.clone(),
            merge_patterns: MergePatterns::default().into(),
            persistence: vec![],
            missing_parents: HashMap::new(),
        }
    }

    /// Set the regex patterns that are used to derive
    /// branch names from merge commit message.
    pub fn with_merge_patterns(mut self, merge_patterns: MergePatterns) -> Self {
        self.merge_patterns = merge_patterns.into();
        self
    }
    /// Set a sequence of branch name regex that determine the
    /// persistence order of branches.
    pub fn with_persistence_patterns(mut self, persistence: PersistencePatterns) -> Self {
        self.persistence = persistence;
        self
    }

    #[allow(clippy::doc_overindented_list_items)]
    /// Add a commit to the TrackMap.
    /// When a missing parent is added, create the missing relations.
    ///
    /// # Arguments
    /// - id : Child Oid
    /// - message: Child commit message. If child is a merge this may give
    ///            a name to a merged branch.
    /// - parents: List of parent Oid.
    pub fn add_commit(&mut self, id: Oid, message: String, parents: Vec<Oid>) {
        let merge_patterns = self.merge_patterns.clone();

        // The add_commit function has the following steps:
        //
        // 1. Compute and register commit
        // 1.1. Add CommitInfo to tracks
        // 1.2. Compute branch for commit
        //
        // 2. Record parents of commit
        //  for each unknown parent,
        // 2.1. if first parent - store as continuation
        // 2.2. if merge-parent - store as derived branch

        //
        //   1. The commit
        //

        // 1.1. Add CommitInfo to tracks

        let ci = CommitInfo {
            oid: id.clone(),
            parents: parents.clone(),
            // TODO why the child pointer?? Can it be completely removed?
            children: vec![],
            branch_trace: None, // We will set this in a moment
        };
        self.tracks.borrow_mut().add_commit(ci);

        // 1.2. Compute branch_trace for commit

        let commit_index = *self
            .tracks
            .borrow()
            .indices
            .get(&id)
            .expect("Commit must be known in TrackMap");
        self.missing_parents
            .remove(&id)
            .unwrap_or_default()
            .set_commit_branch(&mut *self.tracks.borrow_mut(), commit_index);

        //
        //   2. Parents
        //

        // 2.1 Primary parent

        if !parents.is_empty() {
            // Primary parent continues the commit at this commit
            // Store that information in MissingParent so it can be
            // used when the parent commit is processed.
            let child_inx = *self
                .tracks
                .borrow()
                .indices
                .get(&id)
                .expect("Commit must be known in TrackMap");
            let parent_oid = parents[0].clone();
            self.missing_parents
                .entry(parent_oid)
                .or_default()
                .add_child(child_inx);
        }

        // 2.2. Non-Primary parents

        // Run through all merged parents and add a derived branch
        // if it is not already part of a branch.
        // Normally every merged parent would start a branch.
        // This does not happen if the merged parent has already
        // been claimed by another branch.
        let add_branch_if_missing = |child_oid: Oid, parent_oid: Oid, par_branch_name: String| {
            // Determine persistence and order for the derived branch.
            let par_persistence = branch_order(&par_branch_name, &self.persistence) as u8;

            // Abort if a branch is already here, with at least same persistence.
            // Note: This never happens on first walk of TrackMap
            if let Some(existing_branch) = self.tracks.borrow().branch_at(&parent_oid) {
                if existing_branch.persistence <= par_persistence {
                    return;
                }
            }

            let Some(&end_index) = self.tracks.borrow().indices.get(&parent_oid) else {
                // Parent is outside self.tracks,
                // and we build TrackMap in one batch
                // so it is safe to ignore.
                // BUG Incrementally building TrackMap will
                // fail if this is ignored.
                // TODO Add to known child unknown parent
                // and store  branch info there as well
                return;
            };

            // Create and remember the BranchInfo for the derived merge branch.
            let new_branch = BranchInfo::new(
                parent_oid.clone(),      // Target is the parent of the merge.
                Some(child_oid.clone()), // The merge commit itself.
                par_branch_name,
                par_persistence,
                BranchInfoType::Derived,
                Some(end_index),
            );

            // TODO This code assumes that we only do one walk to fill
            // TrackMap. If run a second time, the commit is already there
            // and MissingParent is probably not the right way to do things.
            self.missing_parents
                .entry(parent_oid)
                .or_default()
                .set_branch(new_branch);
        };

        // Derive merge branches from merged-in parents
        // and store them for later usage.
        // TODO find a better name for this function.
        // Perhaps derive_merge_branches
        // or process_merged_parents
        handle_merge_branches(
            &merge_patterns,
            &id, // Child Oid
            &message,
            &parents,
            add_branch_if_missing,
        );
    }
}

//
//  Generic functions not tied to a particular struct
//

pub fn create_merge_branches<Oid: Clone>(
    merge_patterns: &MergePatterns,
    persistence_patterns: &[Regex],
    child_oid: &Oid,
    message: &str,
    parents: &[Oid],
    end_index: usize,
) -> Vec<BranchInfo<Oid>> {
    let mut merge_branches = vec![];

    // Parse the branch names from the merge summary using configured patterns.
    // use get(parent_index - 1) because the primary parent is NOT in this list
    let par_branch_names = parse_merge_summary(message, merge_patterns);

    // Iterate over branches merged into this branch (Skip primary parent)
    #[allow(clippy::needless_range_loop)]
    for parent_index in 1..parents.len() {
        let parent_oid = parents[parent_index].clone();
        let par_branch_name = par_branch_names
            .get(parent_index - 1)
            .unwrap_or(&"unknown".to_string())
            .clone();

        // Determine persistence and order for the derived branch.
        let persistence = branch_order(&par_branch_name, persistence_patterns) as u8;

        // Create and add the BranchInfo for the derived merge branch.
        let branch_info = BranchInfo::new(
            parent_oid,              // Branch target is the parent of the merge.
            Some(child_oid.clone()), // The merge commit is the merge_target
            par_branch_name,
            persistence,
            BranchInfoType::Derived,
            Some(end_index),
        );
        merge_branches.push(branch_info);
    }

    merge_branches
}

/// Visit all merged parents of a commit.
/// The visitor is provided the assumed branch name
/// derived from the merge message.
///
/// [TrackMap] uses this to derive new branches that targets the merged commit.
pub fn handle_merge_branches<Oid: Clone>(
    merge_patterns: &MergePatterns,
    child_oid: &Oid,
    message: &str,
    parents: &[Oid],
    mut branch_visitor: impl FnMut(Oid, Oid, String),
) {
    // Extract the branch names from the merge summary using configured patterns.
    let par_branch_names = parse_merge_summary(message, merge_patterns);

    // Visit all branches merged into this branch.
    // parent 0 is not visited because it is the target branch.
    #[allow(clippy::needless_range_loop)]
    for parent_index in 1..parents.len() {
        let child_oid = child_oid.clone();
        let parent_oid = parents[parent_index].clone();
        let par_branch_name = par_branch_names
            .get(parent_index - 1)
            .unwrap_or(&"unknown".to_string())
            .clone();

        branch_visitor(child_oid, parent_oid, par_branch_name);
    }
}

/// Finds the index for a branch name from a slice of prefixes
pub fn branch_order(name: &str, order: &[Regex]) -> usize {
    order
        .iter()
        .position(|b| (name.starts_with(ORIGIN) && b.is_match(&name[7..])) || b.is_match(name))
        .unwrap_or(order.len())
}

/// Tries to extract the name of a merged-in branch from the merge commit summary.
/// The number of names returned corresponds to the number of merged-in
/// parents. If no match was found, the empty list is returned.
/// Normal merge return a single item,
/// octo-merges may have several.
///
/// *Note*: git gives full flexibility to use your own messages, so there is
/// no guarantee that all merged-in parents are named.
pub fn parse_merge_summary(summary: &str, patterns: &MergePatterns) -> Vec<String> {
    // Try all merge patterns
    for regex in &patterns.patterns {
        let Some(captures) = regex.captures(summary) else {
            continue;
        };
        if captures.len() != 2 {
            continue;
        }
        let Some(match1) = captures.get(1) else {
            continue;
        };

        // For octo-merge patterns, captured_str holds a comma separated
        // list. For normal merge only a single name will be returned.
        return match1
            .as_str()
            .replace(" and ", ", ") // Standardize separators
            .split(',')
            .map(|s| s.trim().trim_matches('\'').to_string()) // Clean up spacing and quotes
            .filter(|s| !s.is_empty())
            .collect();
    }
    // No pattern match, return an empty list
    vec![]
}

#[cfg(test)]
mod tests {
    use crate::settings::MergePatterns;

    #[test]
    fn parse_merge_summary() {
        let patterns = MergePatterns::default();

        let gitlab_pull = "Merge branch 'feature/my-feature' into 'master'";
        let git_default = "Merge branch 'feature/my-feature' into dev";
        let git_master = "Merge branch 'feature/my-feature'";
        let github_pull = "Merge pull request #1 from user-x/feature/my-feature";
        let github_pull_2 = "Merge branch 'feature/my-feature' of github.com:user-x/repo";
        let bitbucket_pull = "Merged in feature/my-feature (pull request #1)";

        assert_eq!(
            super::parse_merge_summary(gitlab_pull, &patterns),
            vec!["feature/my-feature".to_string()],
        );
        assert_eq!(
            super::parse_merge_summary(git_default, &patterns),
            vec!["feature/my-feature".to_string()],
        );
        assert_eq!(
            super::parse_merge_summary(git_master, &patterns),
            vec!["feature/my-feature".to_string()],
        );
        assert_eq!(
            super::parse_merge_summary(github_pull, &patterns),
            vec!["feature/my-feature".to_string()],
        );
        assert_eq!(
            super::parse_merge_summary(github_pull_2, &patterns),
            vec!["feature/my-feature".to_string()],
        );
        assert_eq!(
            super::parse_merge_summary(bitbucket_pull, &patterns),
            vec!["feature/my-feature".to_string()],
        );

        //
        //  Octo merge
        //

        let octo_git = "Merge branches 'feature/foo', 'feature/bar' and 'bugfix/baz'";
        let octo_git_into =
            "Merge branches 'feature/foo', 'feature/bar' and 'bugfix/baz' into target_branch";
        let octo_bitbucet =
            "Merged in feature/foo, feature/bar, bugfix/baz (pull requests #10, #11, #12)";

        assert_eq!(
            super::parse_merge_summary(octo_git, &patterns),
            vec![
                "feature/foo".to_string(),
                "feature/bar".to_string(),
                "bugfix/baz".to_string(),
            ],
        );
        assert_eq!(
            super::parse_merge_summary(octo_git_into, &patterns),
            vec![
                "feature/foo".to_string(),
                "feature/bar".to_string(),
                "bugfix/baz".to_string(),
            ],
        );
        assert_eq!(
            super::parse_merge_summary(octo_bitbucet, &patterns),
            vec![
                "feature/foo".to_string(),
                "feature/bar".to_string(),
                "bugfix/baz".to_string(),
            ],
        );
    }
}
