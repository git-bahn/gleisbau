/*! Layout represent a subset of the full graph, with information about
which order tracks should be placed.

It is intended as last step before printing. Decoration of commits,
e.g. with tags and branch labels, should be done during printing.
*/

use std::collections::HashMap;
use std::ops::Range;

use regex::Regex;

use crate::print::colors::to_terminal_color;
use crate::settings::Settings;
use crate::track::BranchInfo;
use crate::track::TrackMap;

const ORIGIN: &str = "origin/";


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
    range: Range<usize>,
    settings: &Settings,
) -> Result<TrackLayout, String> {
    let mut branch_visuals = Vec::new();
    let mut track_visual_map = HashMap::new();

    // --- Pass 1: Create initial BranchVis (Colors and Order Groups) ---
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
            let vis_idx = branch_visuals.len();
            
            branch_visuals.push(create_branch_visual(b_idx, branch_info, settings)?);
            track_visual_map.insert(b_idx, vis_idx);
        }
    }

    // --- Pass 2: Connect Visual Groups (Target/Source Order Groups) ---
    // We iterate through the visuals we just created
    for (b_idx, &vis_idx) in track_visual_map.iter() {
        let branch = &track_map.all_branches[*b_idx];
        
        // Resolve Target Order Group
        if let Some(target_idx) = branch.target_branch {
            // Check if the target branch has a visual in our current layout
            if let Some(&target_vis_idx) = track_visual_map.get(&target_idx) {
                let target_order = branch_visuals[target_vis_idx].order_group;
                branch_visuals[vis_idx].target_order_group = Some(target_order);
            }
        }

        // Resolve Source Order Group
        if let Some(source_idx) = branch.source_branch {
            // Check if the source branch has a visual in our current layout
            if let Some(&source_vis_idx) = track_visual_map.get(&source_idx) {
                let source_order = branch_visuals[source_vis_idx].order_group;
                branch_visuals[vis_idx].source_order_group = Some(source_order);
            }
        }
    }

    Ok(TrackLayout {
        source: range,
        track_visual: track_visual_map,
        branch_visual: branch_visuals,
    })
}

fn create_branch_visual(
    idx: usize,
    branch: &BranchInfo,
    settings: &Settings,
) -> Result<BranchVis, String> {
    // 1. Calculate Order Group (Position)
    let order_group = branch_order(&branch.name, &settings.branches.order);

    // 2. Calculate Terminal Color
    let term_color_name = branch_color(
        &branch.name,
        &settings.branches.terminal_colors[..],
        &settings.branches.terminal_colors_unknown,
        idx,
    );
    let term_color = to_terminal_color(&term_color_name)?;

    // 3. Calculate SVG Color
    let svg_color = branch_color(
        &branch.name,
        &settings.branches.svg_colors,
        &settings.branches.svg_colors_unknown,
        idx,
    );

    Ok(BranchVis {
        order_group,
        term_color,
        svg_color,
        // These are handled by assign_sources_targets later
        target_order_group: None,
        source_order_group: None,
        column: None,
    })
}

/// Finds the index for a branch name from a slice of prefixes
fn branch_order(name: &str, order: &[Regex]) -> usize {
    order
        .iter()
        .position(|b| (name.starts_with(ORIGIN) && b.is_match(&name[7..])) || b.is_match(name))
        .unwrap_or(order.len())
}

/// Finds the svg color for a branch name.
fn branch_color<T: Clone>(
    name: &str,
    order: &[(Regex, Vec<T>)],
    unknown: &[T],
    counter: usize,
) -> T {
    let stripped_name = name.strip_prefix(ORIGIN).unwrap_or(name);

    for (regex, colors) in order {
        if regex.is_match(stripped_name) {
            return colors[counter % colors.len()].clone();
        }
    }

    unknown[counter % unknown.len()].clone()
}
