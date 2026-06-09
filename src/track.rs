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

define_u32_index!(
    /** Index into [TrackMap].all_branches.

    This index is 4 bytes instead of 8 bytes for normal usize.
    We store an index to a commit 3 times in [CommitInfo].
    For a repository with N commits, this will save 4 * 3 * N bytes of memory
    */
    pub struct Binx;
);

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
    pub merge_target: Option<Oid>,
    pub source_branch: Option<Binx>,
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
    /// Parents of commit. Filled in first pass
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
pub struct Builder<Oid>
    where Oid: Clone + Eq + Hash
{
    /// Track structure to update
    tracks: Rc<RefCell<TrackMap<Oid>>>,

    /// Merge patterns to use when deriving a branch name from a merge
    merge_patterns: MergePatterns,

    /// Branch persistence from branch name pattern
    persistence: PersistencePatterns,
}
impl<Oid> Builder<Oid> 
    where Oid: Clone + Eq + Hash 
{
    /// Create a builder for the specified TrackMap
    pub fn new(target: Rc<RefCell<TrackMap<Oid>>>) -> Self {
        Self {
            tracks: target.clone(),
            merge_patterns: MergePatterns::default(),
            persistence: vec![],
        }
    }

    /// Set the regex patterns that are used to derive
    /// branch names from merge commit message.
    pub fn with_merge_patterns(mut self, merge_patterns: MergePatterns) -> Self {
        self.merge_patterns = merge_patterns;
        self
    }
    /// Set a sequence of branch name regex that determine the
    /// persistence order of branches.
    pub fn with_persistence_patterns(mut self, persistence: PersistencePatterns) -> Self {
        self.persistence = persistence;
        self
    }
    
    /// Add a commit to the TrackMap.
    /// When a missing parent is added, create the missing relations.
    pub fn add_commit(&mut self, id: Oid, parents: Vec<Oid>) {

        // TODO Expand existing track to this commit or create a new track
        let track = Binx(0); // BUG - should be computed properly. This is only to make it compile

        // Find parents
        let ci = CommitInfo {
            oid: id,
            parents,
            children: vec![],
            branch_trace: Some(track),
        };
        self.tracks.borrow_mut().add_commit(ci);
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
