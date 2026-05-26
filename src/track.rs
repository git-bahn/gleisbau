/*! The tracks map assigns a track to each commit.

It is expensive to compute because changes in one end may affect the other end.
Fortunately it can be computed incrementally.
*/

use std::collections::HashMap;

use regex::Regex;

use crate::define_u32_index;
use crate::settings::MergePatterns;

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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target: Oid,
        merge_target: Option<Oid>,
        name: String,
        persistence: u8,
        is_remote: bool,
        is_merged: bool,
        is_tag: bool,
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

//
//  Generic functions not tied to a particular struct
//

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
