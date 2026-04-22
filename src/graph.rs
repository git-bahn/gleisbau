//! A graph structure representing the history of a Git repository.
//!
//! To generate a [GitGraph], you must use [Builder] to construct it.
//!
//! ### Visualization of branches
//! gleisbau uses the term *branch* a little different from how git uses it.
//! In git-lingo this means "a label on some commit", whereas in gleisbau
//! it means "a path in the ancestor graph of a repository". Nodes are
//! commits, edges are directed from a child to its parents.
//!
//! In the text below, the term
//! - *git-branch* is a label on a commit.
//! - *branch* is the visualization of an ancestor path.
//!
//! gleisbau visualizes branches as a vertical line. Only
//! the primary parent of a commit can be on the same branch as the
//! commit. Horizontal lines represent forks (multiple children) or
//! merges (multiple parents), and show the remaining parent relations.


use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;

pub use git2::{BranchType, Commit, Error, Oid, Reference, Repository};

use crate::layout;
use crate::print::label;
use crate::settings::Settings;
use crate::track;

pub use crate::layout::BranchVis;
pub use crate::layout::TrackLayout;
pub use crate::print::label::LabelMap;
pub use crate::track::BranchInfo;
pub use crate::track::CommitInfo;
pub use crate::track::TrackMap;

/// Represents a git history graph.
pub struct GitGraph {
    pub repository: Repository,
    /// Track structure, may be updated by a separate thread
    pub tracks: Arc<Mutex<TrackMap>>,
    /// Layout of all commits in track structure
    pub layout: TrackLayout,
    /// Labels to show next to commits
    pub labels: LabelMap,
    /// The current HEAD
    pub head: HeadInfo,
}

/** Builder of a GitGraph struct. This handles one-time processing of the
repository. */
#[derive(Default)]
pub struct Builder<'a> {
    repository: Option<Repository>,
    settings: Option<&'a Settings>,
    start_point: Option<String>,
    max_count: Option<usize>,
    refspecs: Vec<String>,
}

impl<'a> Builder<'a> {
    pub fn new() -> Self {
        Builder::default()
    }
    pub fn with_repository(mut self, repository: Repository) -> Self {
        self.repository = Some(repository);
        self
    }
    pub fn with_settings(mut self, settings: &'a Settings) -> Self {
        self.settings = Some(settings);
        self
    }
    pub fn with_start_point(mut self, start_point: String) -> Self {
        self.start_point = Some(start_point);
        self
    }
    pub fn with_max_count(mut self, max_count: usize) -> Self {
        self.max_count = Some(max_count);
        self
    }
    pub fn with_refspecs(mut self, refspecs: Vec<String>) -> Self {
        self.refspecs = refspecs;
        self
    }
    pub fn build(self) -> Result<GitGraph, String> {
        GitGraph::new(
            self.repository.expect("You must specify repository"),
            self.settings.expect("You must specify settings"),
            self.start_point,
            self.max_count,
            self.refspecs,
        )
    }
}

impl GitGraph {
    // TODO Move all GitGraph construction functionality to Builder

    /// Generate a branch graph for a repository.
    /// It has been made private as a migration step towards a new API.
    /// You must use [Builder] to construct a GitGraph instance.
    fn new(
        mut repository: Repository,
        settings: &Settings,
        start_point: Option<String>,
        max_count: Option<usize>,
        refspecs: Vec<String>,
    ) -> Result<Self, String> {
        #![doc = include_str!("../docs/branch_assignment.md")]
        let mut stashes = HashSet::new();
        repository
            .stash_foreach(|_, _, oid| {
                stashes.insert(*oid);
                true
            })
            .map_err(|err| err.message().to_string())?;

        let mut walk = repository
            .revwalk()
            .map_err(|err| err.message().to_string())?;

        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
            .map_err(|err| err.message().to_string())?;

        track::configure_revwalk(&repository, &mut walk, start_point, &refspecs)?;

        if repository.is_shallow() {
            return Err("ERROR: gleisbau does not support shallow clones due to a missing feature in the underlying libgit2 library.".to_string());
        }

        let head = HeadInfo::new(&repository.head().map_err(|err| err.message().to_string())?)?;

        // commits will hold the CommitInfo for all commits covered
        // indices maps git object id to an index into commits.
        let mut commits = Vec::new();
        let mut indices = HashMap::new();
        let mut idx = 0;
        for oid in walk {
            if let Some(max) = max_count {
                if idx >= max {
                    break;
                }
            }
            if let Ok(oid) = oid {
                if !stashes.contains(&oid) {
                    let commit = repository.find_commit(oid).unwrap();

                    commits.push(CommitInfo::new(&commit));
                    indices.insert(oid, idx);
                    idx += 1;
                }
            }
        }

        track::assign_children(&mut commits, &indices);

        let mut all_branches = track::assign_branches(&repository, &mut commits, &indices, settings)?;
        track::correct_fork_merges(&commits, &indices, &mut all_branches)?;
        track::assign_sources_targets(&commits, &indices, &mut all_branches);

        // Remove commits not on a branch. This will give all commits a new index.
        let filtered_commits: Vec<CommitInfo> = commits
            .into_iter()
            .filter(|info| info.branch_trace.is_some())
            .collect();

        // Create indices from git object id into the filtered commits
        let filtered_indices: HashMap<Oid, usize> = filtered_commits
            .iter()
            .enumerate()
            .map(|(idx, info)| (info.oid, idx))
            .collect();

        // Map from old index to new index. None, if old index was removed
        let index_map: HashMap<usize, Option<&usize>> = indices
            .iter()
            .map(|(oid, index)| (*index, filtered_indices.get(oid)))
            .collect();

        // Update branch.range from old to new index. Shrink if endpoints were removed.
        for branch in all_branches.iter_mut() {
            if let Some(mut start_idx) = branch.range.0 {
                let mut idx0 = index_map[&start_idx];
                while idx0.is_none() {
                    start_idx += 1;
                    idx0 = index_map[&start_idx];
                }
                branch.range.0 = Some(*idx0.unwrap());
            }
            if let Some(mut end_idx) = branch.range.1 {
                let mut idx0 = index_map[&end_idx];
                while idx0.is_none() {
                    end_idx -= 1;
                    idx0 = index_map[&end_idx];
                }
                branch.range.1 = Some(*idx0.unwrap());
            }
        }

        let all_commits = 0..filtered_commits.len();
        let tracks = TrackMap {
            commits: filtered_commits,
            indices: filtered_indices,
            all_branches,
        };

        // Layout tracks in 2D
        let layout = layout::layout_track_range(&tracks, all_commits, &settings)?;

        // Extract labels for formatting commits
        let labels = label::list_labels(&repository, settings.include_remote)?;

        Ok(GitGraph {
            repository,
            tracks: Arc::new(Mutex::new(tracks)),
            layout,
            labels,
            head,
        })
    }

    pub fn take_repository(self) -> Repository {
        self.repository
    }

    pub fn commit(&self, id: Oid) -> Result<Commit<'_>, Error> {
        self.repository.find_commit(id)
    }
}

/// Information about the current HEAD
pub struct HeadInfo {
    pub oid: Oid,
    pub name: String,
    pub is_branch: bool,
}
impl HeadInfo {
    fn new(head: &Reference) -> Result<Self, String> {
        let name = head.name().ok_or_else(|| "No name for HEAD".to_string())?;
        let name = if name == "HEAD" {
            name.to_string()
        } else {
            name[11..].to_string()
        };

        let h = HeadInfo {
            oid: head.target().ok_or_else(|| "No id for HEAD".to_string())?,
            name,
            is_branch: head.is_branch(),
        };
        Ok(h)
    }
}
