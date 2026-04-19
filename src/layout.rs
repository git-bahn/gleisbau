/*! Layout represent a subset of the full graph, with information about
which order tracks should be placed.

It is intended as last step before printing. Decoration of commits,
e.g. with tags and branch labels, should be done during printing.
*/

use std::collections::HashMap;
use std::ops::Range;

use crate::track::BranchInfo;
use crate::track::TrackMap;


// These functions in track.rs contains references to BranchVis
// 295: correct_fork_merges
// 337:assign_sources_targets
// 413:extract_actual_branches ?? has colour but not order group
// 704:trace_branch
// 807:asssign_branch_columns

/**
    Given a range of commits in a [TrackMap] you can construct a [TrackLayout]
    which will assign columns and colours to the tracks.
*/
pub struct TrackLayout {
    // Specifies which commits are rendered
    source: Range<usize>,
    // Map a TrackMap.branch index to a TrackLayout.branch_visual index
    track_visual: HashMap<usize, usize>,
    // Visuals for all tracks in the rendered range
    branch_visual: Vec<BranchVis>,
}

/// Branch properties for visualization.
pub struct BranchVis {
    /// The branch's column group (left to right)
    pub order_group: usize,
    /// The branch's merge target column group (left to right)
    pub target_order_group: Option<usize>,
    /// The branch's source branch column group (left to right)
    pub source_order_group: Option<usize>,
    /// The branch's terminal color (index in 256-color palette)
    pub term_color: u8,
    /// SVG color (name or RGB in hex annotation)
    pub svg_color: String,
    /// The column the branch is located in
    pub column: Option<usize>,
}

impl BranchVis {
    pub fn new(order_group: usize, term_color: u8, svg_color: String) -> Self {
        BranchVis {
            order_group,
            target_order_group: None,
            source_order_group: None,
            term_color,
            svg_color,
            column: None,
        }
    }
}
/// Generates a TrackLayout by extracting and calculating visual data for 
/// branches active within a specific commit range.
pub fn layout_track_range(
    track_map: &TrackMap, 
    range: Range<usize>
) -> TrackLayout {
    let mut branch_visuals = Vec::new();
    let mut track_visual_map = HashMap::new();

    // Iterate through the requested commit range
    for i in range.clone() {
        // Find track assigned to commit
        let commit = &track_map.commits[i];
        let Some(b_idx) = commit.branch_trace
        else { 
            todo!("Decide how to handle commit without track");
            /*
                Do I want to show it?
                Perhaps to panic?
                Do I want to autogenerate a branch named "anonymous"?
            */
        };

        // If the track does not yet have a visualization, create it
        if !track_visual_map.contains_key(&b_idx) {
            let branch_info = &track_map.all_branches[b_idx];
            
            // Logic to calculate colors/columns
            let visual_data = create_branch_visual(branch_info);

            // Store the visual data and map it
            let vis_idx = branch_visuals.len();
            branch_visuals.push(visual_data);
            track_visual_map.insert(b_idx, vis_idx);
        }

        // Note: You can now easily store per-commit geometry here if needed,
        // since you have the current commit index 'i' and its branch visual index.
    }

    TrackLayout {
        source: range,
        track_visual: track_visual_map,
        branch_visual: branch_visuals,
    }
}

fn create_branch_visual(_branch: &BranchInfo) -> BranchVis {
    todo!("Implement function");
    BranchVis {
        order_group: 0,
        term_color: 0,
        svg_color: String::new(),
        target_order_group: None,
        source_order_group: None,
        column: None,
    }
}
