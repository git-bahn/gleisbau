/*! All tags and branch labels assigned to a commit is interesting for
a UI to show, even though only one of them will determine the looks
used. This module provides add-on decoration not present in
[TrackMap](crate::track::TrackMap)
or [TraclLayout](crate::layout::TrackLayout).
*/

use std::collections::HashMap;

use git2::Oid;

/// All branch- and tag-labels present
pub struct LabelMap {
    // TODO rename to oid2label_vec
    labels: HashMap<Oid, Vec<Label>>,
}

pub struct Label {
    pub name: String,
    pub kind: LabelType,
}

#[derive(PartialEq)]
pub enum LabelType {
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