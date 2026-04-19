/*! The tracks map assigns a track to each commit.

It is expensive to compute because changes in one end may affect the other end.
Fortunately it can be computed incrementally.
*/

use std::collections::HashMap;

use git2::BranchType;
use git2::Commit;
use git2::Error;
use git2::Oid;
use git2::Repository;
use regex::Regex;

use crate::settings::{MergePatterns, Settings};

const ORIGIN: &str = "origin/";
const FORK: &str = "fork/";



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
pub struct CommitInfo {
    pub oid: Oid,
    pub is_merge: bool,
    pub parents: [Option<Oid>; 2],
    pub children: Vec<Oid>,
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
            branch_trace: None,
        }
    }
}

/// For a single refspec, find a base branch to compare against
/// using the branch's upstream tracking ref.
fn find_base_oid(repository: &Repository, refspec: &str, tip_oid: Oid) -> Option<Oid> {
    if let Ok(branch) = repository.find_branch(refspec, BranchType::Local) {
        if let Ok(upstream) = branch.upstream() {
            if let Some(oid) = upstream.get().target() {
                if oid != tip_oid {
                    return Some(oid);
                }
            }
        }
    }

    None
}

fn hide_ancestors_of(repository: &Repository, walk: &mut git2::Revwalk, merge_base: Oid) {
    if let Ok(commit) = repository.find_commit(merge_base) {
        for parent in commit.parents() {
            let _ = walk.hide(parent.id());
        }
    }
}

pub fn configure_revwalk(
    repository: &Repository,
    walk: &mut git2::Revwalk,
    start_point: Option<String>,
    refspecs: &[String],
) -> Result<(), String> {
    if !refspecs.is_empty() {
        let mut resolved_oids = Vec::with_capacity(refspecs.len());
        for refspec in refspecs {
            let object = repository
                .revparse_single(refspec)
                .map_err(|err| format!("Failed to resolve refspec '{}': {}", refspec, err))?;
            let oid = object.id();
            walk.push(oid).map_err(|err| err.message().to_string())?;
            resolved_oids.push(oid);
        }

        if resolved_oids.len() == 1 {
            // Single refspec: auto-detect base branch
            if let Some(base_oid) = find_base_oid(repository, &refspecs[0], resolved_oids[0]) {
                walk.push(base_oid)
                    .map_err(|err| err.message().to_string())?;
                if let Ok(mb) = repository.merge_base(resolved_oids[0], base_oid) {
                    hide_ancestors_of(repository, walk, mb);
                }
            }
        } else {
            // Multiple refspecs: compute merge-base of all
            let mut base = resolved_oids[0];
            let mut base_found = true;
            for oid in &resolved_oids[1..] {
                match repository.merge_base(base, *oid) {
                    Ok(mb) => base = mb,
                    Err(_) => {
                        base_found = false;
                        break;
                    }
                }
            }
            if base_found {
                hide_ancestors_of(repository, walk, base);
            }
        }
    } else if let Some(start) = start_point {
        let object = repository
            .revparse_single(&start)
            .map_err(|err| format!("Failed to resolve start point '{}': {}", start, err))?;
        walk.push(object.id())
            .map_err(|err| err.message().to_string())?;
    } else {
        walk.push_glob("*")
            .map_err(|err| err.message().to_string())?;
    }
    Ok(())
}

/// Walks through the commits and adds each commit's Oid to the children of its parents.
pub fn assign_children(commits: &mut [CommitInfo], indices: &HashMap<Oid, usize>) {
    for idx in 0..commits.len() {
        let (oid, parents) = {
            let info = &commits[idx];
            (info.oid, info.parents)
        };
        for par_oid in &parents {
            if let Some(par_idx) = par_oid.and_then(|oid| indices.get(&oid)) {
                commits[*par_idx].children.push(oid);
            }
        }
    }
}

/// Extracts branches from repository and merge summaries, assigns branches and branch traces to commits.
///
/// Algorithm:
/// * Find all actual branches (incl. target oid) and all extract branches from merge summaries (incl. parent oid)
/// * Sort all branches by persistence
/// * Iterating over all branches in persistence order, trace back over commit parents until a trace is already assigned
pub fn assign_branches(
    repository: &Repository,
    commits: &mut [CommitInfo],
    indices: &HashMap<Oid, usize>,
    settings: &Settings,
) -> Result<Vec<BranchInfo>, String> {
    let mut branch_idx = 0;

    let mut branches = extract_branches(repository, commits, indices, settings)?;

    // We only want to keep branches that has assigned some commit,
    // or that is merged into some other branch.
    // Compute branch index map that deletes the unwanted.
    let mut index_map: Vec<_> = (0..branches.len())
        .map(|old_idx| {
            let (target, is_merged) = {
                let branch = &branches[old_idx];
                (branch.target, branch.is_merged)
            };
            if let Some(&idx) = &indices.get(&target) {
                let info = &mut commits[idx];
                let oid = info.oid;
                let any_assigned =
                    trace_branch(repository, commits, indices, &mut branches, oid, old_idx)
                        .unwrap_or(false);

                if any_assigned || !is_merged {
                    branch_idx += 1;
                    Some(branch_idx - 1)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let mut commit_count = vec![0; branches.len()];
    for info in commits.iter_mut() {
        if let Some(trace) = info.branch_trace {
            commit_count[trace] += 1;
        }
    }

    // Get rid of branches that have no commits and is merged and is not a tag
    let mut count_skipped = 0;
    for (idx, branch) in branches.iter().enumerate() {
        if let Some(mapped) = index_map[idx] {
            if commit_count[idx] == 0 && branch.is_merged && !branch.is_tag {
                index_map[idx] = None;
                count_skipped += 1;
            } else {
                index_map[idx] = Some(mapped - count_skipped);
            }
        }
    }

    for info in commits.iter_mut() {
        if let Some(trace) = info.branch_trace {
            info.branch_trace = index_map[trace];
        }
    }

    let branches: Vec<_> = branches
        .into_iter()
        .enumerate()
        .filter_map(|(arr_index, branch)| {
            if index_map[arr_index].is_some() {
                Some(branch)
            } else {
                None
            }
        })
        .collect();

    Ok(branches)
}

pub fn correct_fork_merges(
    commits: &[CommitInfo],
    indices: &HashMap<Oid, usize>,
    branches: &mut [BranchInfo],
) -> Result<(), String> {
    for idx in 0..branches.len() {
        if let Some(merge_target) = branches[idx]
            .merge_target
            .and_then(|oid| indices.get(&oid))
            .and_then(|idx| commits.get(*idx))
            .and_then(|info| info.branch_trace)
            .and_then(|trace| branches.get(trace))
        {
            if branches[idx].name == merge_target.name {
                branches[idx].name = format!("{}{}", FORK, branches[idx].name);
            }
        }
    }
    Ok(())
}

pub fn assign_sources_targets(
    commits: &[CommitInfo],
    indices: &HashMap<Oid, usize>,
    branches: &mut [BranchInfo],
) {
    // 1. Identify Target Branches (where does this branch merge INTO?)
    for idx in 0..branches.len() {
        branches[idx].target_branch = branches[idx]
            .merge_target
            .and_then(|oid| indices.get(&oid))
            .and_then(|idx| commits.get(*idx))
            .and_then(|info| info.branch_trace);
    }

    // 2. Identify Source Branches (where did this branch fork FROM?)
    for info in commits {
        for par_oid in info.parents.iter().flatten() {
            if let Some(par_info) = indices.get(par_oid).and_then(|&i| commits.get(i)) {
                // If the parent is on a different branch trace, that's our source
                if par_info.branch_trace != info.branch_trace {
                    if let (Some(this_b_idx), Some(src_b_idx)) = (info.branch_trace, par_info.branch_trace) {
                        branches[this_b_idx].source_branch = Some(src_b_idx);
                    }
                }
            }
        }
    }
}

/// Extracts and processes actual Git branches (local and remote) from the repository.
///
/// This function iterates through the branches found in the Git repository,
/// filters them based on the `include_remote` setting, and constructs `BranchInfo`
/// objects for each valid branch. It assigns properties like name, type (local/remote)
/// based on the provided settings.
///
/// Arguments:
/// - `repository`: A reference to the Git `Repository` object.
/// - `indices`: A HashMap mapping commit OIDs to their corresponding indices in the `commits` list.
/// - `settings`: A reference to the application `Settings` containing branch configuration.
///
/// Returns:
/// A `Result` containing a `Vec<BranchInfo>` on success, or a `String` error message on failure.
fn extract_actual_branches(
    repository: &Repository,
    indices: &HashMap<Oid, usize>,
    settings: &Settings,
) -> Result<Vec<BranchInfo>, String> {
    // Determine if remote branches should be included based on settings.
    let filter = if settings.include_remote {
        None
    } else {
        Some(BranchType::Local)
    };

    // Retrieve branches from the repository, handling potential errors.
    let actual_branches = repository
        .branches(filter)
        .map_err(|err| err.message().to_string())?
        .collect::<Result<Vec<_>, Error>>()
        .map_err(|err| err.message().to_string())?;

    // Process each actual branch to create `BranchInfo` objects.
    let valid_branches = actual_branches
        .iter()
        .filter_map(|(br, tp)| {
            let reference = br.get();
            let name_full = reference.name()?;
            let target_oid = reference.target()?;

            // Strip prefix: "refs/heads/" (11) or "refs/remotes/" (13)
            let start_index = match tp {
                BranchType::Local => 11,
                BranchType::Remote => 13,
            };
            let name = name_full.get(start_index..).unwrap_or(name_full);
            let commit_idx = indices.get(&target_oid).cloned();

            let persistence = branch_order(name, &settings.branches.persistence) as u8;

            Some(BranchInfo {
                target: target_oid,
                merge_target: None,
                source_branch: None,
                target_branch: None,
                name: name.to_string(),
                persistence,
                is_remote: &BranchType::Remote == tp,
                is_merged: false,
                is_tag: false,
                range: (None, commit_idx), // Start is unknown yet, end is the branch head
            })
        })
        .collect();

    Ok(valid_branches)
}

/// Iterates through commits, identifies merge commits, and derives branch information
/// from their summaries.
///
/// This function processes each commit in the provided list. If a commit is identified
/// as a merge commit and has a summary, it attempts to parse a branch name from the summary.
/// A `BranchInfo` object is then created for this derived branch, representing the merge
/// point and its properties.
///
/// Arguments:
/// - `repository`: A reference to the Git `Repository` object.
/// - `commits`: A slice of `CommitInfo` objects, representing the commits to process.
/// - `settings`: A reference to the application `Settings` containing branch and merge pattern configuration.
/// - `counter`: A mutable reference to a counter, incremented for each processed merge branch.
///
/// Returns:
/// A `Result` containing a `Vec<BranchInfo>` on success, or a `String` error message on failure.
fn extract_merge_branches(
    repository: &Repository,
    commits: &[CommitInfo],
    settings: &Settings,
    counter: &mut usize,
) -> Result<Vec<BranchInfo>, String> {
    let mut merge_branches = Vec::new();

    for (idx, info) in commits.iter().enumerate() {
        // Only process if the commit is a merge.
        if info.is_merge {
            let commit = repository
                .find_commit(info.oid)
                .map_err(|err| err.message().to_string())?;

            // Attempt to get the commit summary.
            if let Some(summary) = commit.summary() {

                let parent_oid = commit
                    .parent_id(1)
                    .map_err(|err| err.message().to_string())?;

                // Parse the branch name from the merge summary using configured patterns.
                let branch_name = parse_merge_summary(summary, &settings.merge_patterns)
                    .unwrap_or_else(|| "unknown".to_string());

                // Determine persistence and order for the derived branch.
                let persistence = branch_order(&branch_name, &settings.branches.persistence) as u8;

                // Create and add the BranchInfo for the derived merge branch.
                let branch_info = BranchInfo::new(
                    parent_oid,     // Target is the parent of the merge.
                    Some(info.oid), // The merge commit itself.
                    branch_name,
                    persistence,
                    false, // Not a remote branch.
                    true,  // This is a derived merge branch.
                    false, // Not a tag.
                    Some(idx + 1), // End index typically points to the commit after the merge.
                );
                merge_branches.push(branch_info);
            }
        }
    }
    Ok(merge_branches)
}

/// Extracts Git tags and treats them as branches, assigning appropriate properties.
///
/// This function iterates through all tags in the repository, resolves their target
/// commit OID, and if the target commit is found within the `commits` list,
/// a `BranchInfo` object is created for the tag. Tags are assigned a higher
/// persistence value to ensure they are displayed prominently.
///
/// Arguments:
/// - `repository`: A reference to the Git `Repository` object.
/// - `indices`: A HashMap mapping commit OIDs to their corresponding indices in the `commits` list.
/// - `settings`: A reference to the application `Settings` containing branch configuration.
/// - `counter`: A mutable reference to a counter, incremented for each processed tag.
///
/// Returns:
/// A `Result` containing a `Vec<BranchInfo>` on success, or a `String` error message on failure.
fn extract_tags_as_branches(
    repository: &Repository,
    indices: &HashMap<Oid, usize>,
    settings: &Settings,
    counter: &mut usize,
) -> Result<Vec<BranchInfo>, String> {
    let mut tags_info = Vec::new();
    let mut tags_raw = Vec::new();

    // Iterate over all tags in the repository.
    repository
        .tag_foreach(|oid, name| {
            tags_raw.push((oid, name.to_vec()));
            true // Continue iteration.
        })
        .map_err(|err| err.message().to_string())?;

    for (oid, name_bytes) in tags_raw {
        // Convert tag name bytes to a UTF-8 string. Tags typically start with "refs/tags/".
        let name = std::str::from_utf8(&name_bytes[5..]).map_err(|err| err.to_string())?;

        // Resolve the target OID of the tag. It could be a tag object or directly a commit.
        let target = repository
            .find_tag(oid)
            .map(|tag| tag.target_id())
            .or_else(|_| repository.find_commit(oid).map(|_| oid)); // If not a tag object, try as a direct commit.

        if let Ok(target_oid) = target {
            // If the target commit is within our processed commits, create a BranchInfo.
            if let Some(target_index) = indices.get(&target_oid) {

                // Create the BranchInfo object for the tag.
                let tag_info = BranchInfo::new(
                    target_oid,
                    None, // No merge OID for tags.
                    name.to_string(),
                    settings.branches.persistence.len() as u8 + 1, // Tags usually have highest persistence.
                    false,                                         // Not a remote branch.
                    false,                                         // Not a derived merge branch.
                    true,                                          // This is a tag.
                    Some(*target_index),
                );
                tags_info.push(tag_info);
            }
        }
    }
    Ok(tags_info)
}

/// Extracts (real or derived from merge summary) and assigns basic properties to branches and tags.
///
/// This function orchestrates the extraction of branch information from various sources:
/// 1. Actual Git branches (local and remote).
/// 2. Branches derived from merge commit summaries.
/// 3. Git tags, treated as branches for visualization purposes.
///
/// It combines the results from these extraction steps, sorts them based on
/// persistence and merge status, and returns a comprehensive list of `BranchInfo` objects.
///
/// Arguments:
/// - `repository`: A reference to the Git `Repository` object.
/// - `commits`: A slice of `CommitInfo` objects, representing all relevant commits.
/// - `indices`: A HashMap mapping commit OIDs to their corresponding indices in the `commits` list.
/// - `settings`: A reference to the application `Settings` containing all necessary configuration.
///
/// Returns:
/// A `Result` containing a `Vec<BranchInfo>` on success, or a `String` error message on failure.
fn extract_branches(
    repository: &Repository,
    commits: &[CommitInfo],
    indices: &HashMap<Oid, usize>,
    settings: &Settings,
) -> Result<Vec<BranchInfo>, String> {
    let mut counter = 0; // Counter for unique branch/tag identification, especially for coloring.
    let mut all_branches: Vec<BranchInfo> = Vec::new();

    // 1. Extract actual local and remote branches.
    let actual_branches = extract_actual_branches(repository, indices, settings)?;
    all_branches.extend(actual_branches);

    // 2. Extract branches derived from merge commit summaries.
    let merge_branches = extract_merge_branches(repository, commits, settings, &mut counter)?;
    all_branches.extend(merge_branches);

    // 3. Extract tags and treat them as branches for visualization.
    let tags_as_branches = extract_tags_as_branches(repository, indices, settings, &mut counter)?;
    all_branches.extend(tags_as_branches);

    // Sort all collected branches and tags.
    // Sorting criteria: first by persistence, then by whether they are merged (unmerged first).
    all_branches.sort_by_cached_key(|branch| (branch.persistence, !branch.is_merged));

    Ok(all_branches)
}

/// Traces back branches by following 1st commit parent,
/// until a commit is reached that already has a trace.
pub fn trace_branch(
    repository: &Repository,
    commits: &mut [CommitInfo],
    indices: &HashMap<Oid, usize>,
    branches: &mut [BranchInfo],
    oid: Oid,
    branch_index: usize,
) -> Result<bool, Error> {
    let mut curr_oid = oid;
    let mut prev_index: Option<usize> = None;
    let mut start_index: Option<i32> = None;
    let mut any_assigned = false;

    while let Some(index) = indices.get(&curr_oid) {
        let info = &mut commits[*index];
        
        if let Some(old_trace) = info.branch_trace {
            // Compare names and ranges without touching visuals
            let (old_name, old_range_start) = {
                let old_branch = &branches[old_trace];
                (old_branch.name.clone(), old_branch.range.0)
            };
            
            let new_name = &branches[branch_index].name;
            let old_end_val = old_range_start.unwrap_or(0);
            let new_end_val = branches[branch_index].range.0.unwrap_or(0);

            if new_name == &old_name && old_end_val >= new_end_val {
                // Branch continuation logic
                let old_branch = &mut branches[old_trace];
                if let Some(old_limit) = old_branch.range.1 {
                    if index > &old_limit {
                        old_branch.range = (None, None);
                    } else {
                        old_branch.range = (Some(*index), old_branch.range.1);
                    }
                } else {
                    old_branch.range = (Some(*index), old_branch.range.1);
                }
            } else {
                // Determine the start_index for the branch visual range
                match prev_index {
                    None => start_index = Some(*index as i32 - 1),
                    Some(p_idx) => {
                        if commits[p_idx].is_merge {
                            let mut temp_index = p_idx;
                            for sibling_oid in &commits[*index].children {
                                if sibling_oid != &curr_oid {
                                    if let Some(&sib_idx) = indices.get(sibling_oid) {
                                        if sib_idx > temp_index { temp_index = sib_idx; }
                                    }
                                }
                            }
                            start_index = Some(temp_index as i32);
                        } else {
                            start_index = Some(*index as i32 - 1);
                        }
                    }
                }
                break;
            }
        }

        info.branch_trace = Some(branch_index);
        any_assigned = true;

        let commit = repository.find_commit(curr_oid)?;
        if commit.parent_count() == 0 {
            start_index = Some(*index as i32);
            break;
        }
        prev_index = Some(*index);
        curr_oid = commit.parent_id(0)?;
    }

    // Finalize the range for this branch
    let branch = &mut branches[branch_index];
    finalize_branch_range(branch, start_index);
    
    Ok(any_assigned)
}

fn finalize_branch_range(branch: &mut BranchInfo, start_index: Option<i32>) {
    if let Some(end) = branch.range.0 {
        if let Some(si) = start_index {
            if si < end as i32 {
                branch.range = (None, None);
            } else {
                branch.range = (branch.range.0, Some(si as usize));
            }
        } else {
            branch.range = (branch.range.0, None);
        }
    } else {
        branch.range = (branch.range.0, start_index.map(|si| si as usize));
    }
}

/// Finds the index for a branch name from a slice of prefixes
fn branch_order(name: &str, order: &[Regex]) -> usize {
    order
        .iter()
        .position(|b| (name.starts_with(ORIGIN) && b.is_match(&name[7..])) || b.is_match(name))
        .unwrap_or(order.len())
}

/// Tries to extract the name of a merged-in branch from the merge commit summary.
pub fn parse_merge_summary(summary: &str, patterns: &MergePatterns) -> Option<String> {
    for regex in &patterns.patterns {
        if let Some(captures) = regex.captures(summary) {
            if captures.len() == 2 && captures.get(1).is_some() {
                return captures.get(1).map(|m| m.as_str().to_string());
            }
        }
    }
    None
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
            Some("feature/my-feature".to_string()),
        );
        assert_eq!(
            super::parse_merge_summary(git_default, &patterns),
            Some("feature/my-feature".to_string()),
        );
        assert_eq!(
            super::parse_merge_summary(git_master, &patterns),
            Some("feature/my-feature".to_string()),
        );
        assert_eq!(
            super::parse_merge_summary(github_pull, &patterns),
            Some("feature/my-feature".to_string()),
        );
        assert_eq!(
            super::parse_merge_summary(github_pull_2, &patterns),
            Some("feature/my-feature".to_string()),
        );
        assert_eq!(
            super::parse_merge_summary(bitbucket_pull, &patterns),
            Some("feature/my-feature".to_string()),
        );
    }
}

