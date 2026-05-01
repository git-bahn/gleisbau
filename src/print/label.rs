/*! All tags and branch labels assigned to a commit is interesting for
a UI to show, even though only one of them will determine the looks
used. This module provides add-on decoration not present in
[TrackMap](crate::track::TrackMap)
or [TraclLayout](crate::layout::TrackLayout).
*/

// TODO The current implementation preserves the pre 0.7 term+svg color
// which could be removed or made with Generics in a future version

use std::collections::HashMap;

use git2::BranchType;
use git2::Oid;
use git2::Repository;

use crate::settings::Settings;
use crate::layout;
use crate::print;

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
    /// The branch's terminal color (index in 256-color palette)
    pub term_color: u8,
    /// SVG color (name or RGB in hex annotation)
    pub svg_color: String,
}

#[derive(Clone, Default, PartialEq)]
pub enum LabelType {
    #[default]
    LocalBranch,
    RemoteBranch,
    Tag,
}

impl LabelMap {
    pub fn add_label<T: Into<String>>(
        &mut self,
        oid: Oid,
        name: T,
        kind: LabelType,
        term_color: u8,
        svg_color: String) 
    {
        let name = name.into();
        let label_list = self.labels.entry(oid).or_insert(vec![]);
        label_list.push(Label { name, kind, term_color, svg_color });
    }
    pub fn get_labels(&self, oid: &Oid) -> Option<&Vec<Label>> {
        self.labels.get(oid)
    }
}

/// Extract all branch and tag names from repo and assign colours from settings
pub fn list_labels(settings: &Settings, repository: &Repository)
 -> Result<LabelMap, String> {
    let include_remote = settings.include_remote;

    let mut labels = LabelMap::default();
    extract_branches(&mut labels, settings, repository, include_remote)?;
    extract_tags(&mut labels, settings, repository)?;
    Ok(labels)
}

type TermSvgColor = (
    // The branch's terminal color (index in 256-color palette)
    /*pub term_color:*/ u8,
    // SVG color (name or RGB in hex annotation)
    /*pub svg_color:*/ String,
);

fn extract_branches(
    labels: &mut LabelMap,
    settings: &Settings,
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

    let mut counter: usize = 0;
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
        let (term_color, svg_color) = get_term_svg_color(settings, name, counter);
        counter += 1;

        labels.add_label(t.clone(), name, label_type, term_color, svg_color);
    }
    Ok(())
}

fn extract_tags(
    labels: &mut LabelMap,
    settings: &Settings,
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

    let mut counter: usize = 0;
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
        let (term_color, svg_color) = get_term_svg_color(settings, name, counter);
        counter += 1;

        labels.add_label(oid, name, LabelType::Tag, term_color, svg_color);
    }

    Ok(())
}

/// Look up colour information in settings
fn get_term_svg_color(
    settings: &Settings,
    name_to_color: &str,
    idx: usize) // Counter used to choose between alternative colors
    -> TermSvgColor {
    // Copied from layout.rs
    // TODO Maybe this function should be shared?
    let term_color_str = layout::branch_color(
        name_to_color,
        &settings.branches.terminal_colors,
        &settings.branches.terminal_colors_unknown,
        idx,
    );
    let term_color = print::colors::to_terminal_color(&term_color_str)
        .expect("Valid terminal color string");

    let svg_color = layout::branch_color(
        name_to_color,
        &settings.branches.svg_colors,
        &settings.branches.svg_colors_unknown,
        idx,
    );

    (term_color, svg_color)
}