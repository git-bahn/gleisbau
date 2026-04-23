/*! All tags and branch labels assigned to a commit is interesting for
a UI to show, even though only one of them will determine the looks
used. This module provides add-on decoration not present in
[TrackMap](crate::track::TrackMap)
or [TraclLayout](crate::layout::TrackLayout).
*/

use std::collections::HashMap;

use git2::BranchType;
use git2::Oid;
use git2::Repository;

/// All branch- and tag-labels present
#[derive(Default)]
pub struct LabelMap {
    // TODO rename to oid2label_vec
    labels: HashMap<Oid, Vec<Label>>,
}

#[derive(Clone, Default)]
pub struct Label {
    pub name: String,
    pub kind: LabelType,
}

#[derive(Clone, Default, PartialEq)]
pub enum LabelType {
    #[default]
    LocalBranch,
    RemoteBranch,
    Tag,
}

impl LabelMap {
    pub fn add_label<T: Into<String>>(&mut self, oid: Oid, name: T, kind: LabelType) {
        let name = name.into();
        let label_list = self.labels.entry(oid).or_insert(vec![]);
        label_list.push(Label { name, kind });
    }
    pub fn get_labels(&self, oid: &Oid) -> Option<&Vec<Label>> {
        self.labels.get(oid)
    }
}

pub fn list_labels(repository: &Repository, include_remote: bool)
 -> Result<LabelMap, String> {
    let mut labels = LabelMap::default();
    extract_branches(&mut labels, repository, include_remote)?;
    extract_tags(&mut labels, repository)?;
    Ok(labels)
}

fn extract_branches(
    labels: &mut LabelMap,
    repository: &Repository,
    include_remote: bool)
    -> Result<(), String>
  {

    //
    // Add branches
    // origin: fn extract_actual_branches
    //

    // Determine if remote branches should be included based on settings.
    let filter = if include_remote {
        None
    } else {
        Some(BranchType::Local)
    };
    // Retrieve branches from the repository, handling potential errors.
    let actual_branches = repository
        .branches(filter)
        .map_err(|err| err.message().to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.message().to_string())?;

    for (br, tp) in actual_branches {
        let Some(n) = br.get().name() else { continue; };
        let Some(t) = br.get().target() else { continue; };

        // Determine the starting index for slicing the branch name string.
        let start_index = match tp {
            BranchType::Local => 11,  // "refs/heads/"
            BranchType::Remote => 13, // "refs/remotes/"
        };
        let name = &n[start_index..];
        let label_type = match tp {
            BranchType::Local => LabelType::LocalBranch,
            BranchType::Remote => LabelType::RemoteBranch,
        };

        labels.add_label(t.clone(), name, label_type);
    }
    Ok(())
}

fn extract_tags(
    labels: &mut LabelMap,
    repository: &Repository)
    -> Result<(), String>
  {


    // Iterate over all tags in the repository.
    let mut tags_raw = Vec::new();
    repository
        .tag_foreach(|oid, name| {
            tags_raw.push((oid, name.to_vec()));
            true // Continue iteration.
        })
        .map_err(|err| err.message().to_string())?;

    for (oid, name_bytes) in tags_raw {
        // Convert tag name bytes to a UTF-8 string. Tags typically start with "refs/tags/".
        let name = std::str::from_utf8(&name_bytes[5..])
            .map_err(|err| err.to_string())?;

        // Resolve the target OID of the tag. It could be a tag object or directly a commit.
        let target = repository
            .find_tag(oid)
            .map(|tag| tag.target_id())
            .or_else(|_| // If not a tag object, try as a direct commit.
                repository.find_commit(oid).map(|_| oid))
            .map_err(|err| err.to_string())?;
        
        let oid = target.clone();
        labels.add_label(oid, name, LabelType::Tag);
    }

    Ok(())
}