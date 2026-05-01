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
    
    // Counter for color rotation moved here
    let mut color_counter = 0;

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
            
            // We increment the counter only when a new visual is needed
            color_counter += 1;

            let visual_data = create_branch_visual(
                color_counter,
                branch_info, 
                track_map,
                settings
            )?;

            let vis_idx = branch_visuals.len();
            branch_visuals.push(visual_data);
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
    
    // Pass 3: The Packing Algorithm
    let mut layout = TrackLayout {
        source: range,
        track_visual: track_visual_map,
        branch_visual: branch_visuals,
    };
    assign_branch_columns(&track_map, &mut layout, settings, false, false);
 
    Ok(layout)
}

fn create_branch_visual(
    idx: usize,
    branch: &BranchInfo,
    track_map: &TrackMap, // Now we pass the map to look up other branches
    settings: &Settings,
) -> Result<BranchVis, String> {
    let mut name_to_color = &branch.name;

    // The Logic from trace_branch: 
    // If this is a remote branch, check if we should inherit a local color
    if branch.name.starts_with(ORIGIN) {
        let local_name = &branch.name[7..];
        // Look for a local branch with the same name in TrackMap
        if let Some(local_idx) = track_map.all_branches.iter().position(|b| b.name == local_name) {
            // We can now use the local_name for color calculation 
            name_to_color = &track_map.all_branches[local_idx].name;
        }
    }

    let order_group = branch_order(name_to_color, &settings.branches.order);
    let term_color_str = branch_color(
        name_to_color,
        &settings.branches.terminal_colors,
        &settings.branches.terminal_colors_unknown,
        idx,
    );
    let term_color = to_terminal_color(&term_color_str)?;

    let svg_color = branch_color(
        name_to_color,
        &settings.branches.svg_colors,
        &settings.branches.svg_colors_unknown,
        idx,
    );

    Ok(BranchVis {
        order_group,
        term_color,
        svg_color,
        target_order_group: None,
        source_order_group: None,
        column: None,
    })
}

/// Sorts branches into columns for visualization, that all branches can be
/// visualizes linearly and without overlaps. Uses Shortest-First scheduling.
pub fn assign_branch_columns(
    track_map: &TrackMap,
    layout: &mut TrackLayout,
    settings: &Settings,
    shortest_first: bool,
    forward: bool,
) {
    // 1. Group occupancy tracking
    // occupied[group_idx][column_idx] = Vec<(start_commit_idx, end_commit_idx)>
    let mut occupied: Vec<Vec<Vec<(usize, usize)>>> = vec![vec![]; settings.branches.order.len() + 1];

    let length_sort_factor = if shortest_first { 1 } else { -1 };
    let start_sort_factor = if forward { 1 } else { -1 };

    // 2. Prepare branches for sorting. 
    // We only care about branches that have a visual representation in this layout.
    let mut branches_sort: Vec<_> = layout.track_visual.iter()
        .map(|(&branch_idx, &vis_idx)| {
            let br = &track_map.all_branches[branch_idx];
            let vis = &layout.branch_visual[vis_idx];
            (
                branch_idx,
                vis_idx,
                br.range.0.unwrap_or(0),
                br.range.1.unwrap_or(track_map.commits.len() - 1),
                vis.source_order_group.unwrap_or(settings.branches.order.len() + 1),
                vis.target_order_group.unwrap_or(settings.branches.order.len() + 1),
            )
        })
        .collect();

    // Sort by priority groups, then length, then start position
    branches_sort.sort_by_cached_key(|tup| {
        (
            std::cmp::max(tup.4, tup.5),
            (tup.3 as i32 - tup.2 as i32) * length_sort_factor,
            tup.2 as i32 * start_sort_factor,
        )
    });

    // 3. Assign columns inside each group
    for (b_idx, v_idx, start, end, _, _) in branches_sort {
        let branch_topo = &track_map.all_branches[b_idx];
        let group = layout.branch_visual[v_idx].order_group;
        let group_occ = &mut occupied[group];

        // Determine if we should search columns from the right (for forks/merges)
        let align_right = should_align_right(branch_topo, v_idx, layout);

        let col_count = group_occ.len();
        let mut found_column = col_count;

        for i in 0..col_count {
            let col_idx = if align_right { col_count - i - 1 } else { i };
            
            // Check if this column is physically blocked by another branch in this range
            let is_blocked = group_occ[col_idx].iter().any(|(s, e)| start <= *e && end >= *s);
            
            if !is_blocked {
                // Logic check: don't occupy the same column as our merge target 
                // if they overlap at the point of merge
                let is_merge_collision = check_merge_collision(branch_topo, col_idx, layout);
                
                if !is_merge_collision {
                    found_column = col_idx;
                    break;
                }
            }
        }

        // Update the visual data
        layout.branch_visual[v_idx].column = Some(found_column);
        if found_column == group_occ.len() {
            group_occ.push(vec![]);
        }
        group_occ[found_column].push((start, end));
    }

    // 4. Final Pass: Apply group offsets to calculate absolute columns
    finalize_absolute_columns(&mut layout.branch_visual, occupied);
}

fn finalize_absolute_columns(
    branch_visual_list: &mut Vec<BranchVis>, 
    occupied: Vec<Vec<Vec<(usize, usize)>>>
 ) {
    
    // Compute start column of each group
    let mut group_offset: Vec<usize> = vec![];
    let mut acc = 0;
    for group in occupied {
        group_offset.push(acc);
        acc += group.len();
    }

    // Compute branch column. Up till now we have computed the branch group
    // and the column offset within that group. This was to make it easy to
    // insert columns between groups. Now it is time to convert offset relative
    // to the group the final column.
    for branch_visual in branch_visual_list {
        if let Some(column) = branch_visual.column {
            let offset = group_offset[branch_visual.order_group];
            branch_visual.column = Some(column + offset);
        }
    }
}

/// Helper: Determines if a branch prefers to be on the right side of its group
fn should_align_right(branch: &BranchInfo, v_idx: usize, layout: &TrackLayout) -> bool {
    let this_group = layout.branch_visual[v_idx].order_group;
    
    let source_to_right = branch.source_branch
        .and_then(|s_idx| layout.track_visual.get(&s_idx))
        .map(|&sv_idx| layout.branch_visual[sv_idx].order_group > this_group)
        .unwrap_or(false);

    let target_to_right = branch.target_branch
        .and_then(|t_idx| layout.track_visual.get(&t_idx))
        .map(|&tv_idx| layout.branch_visual[tv_idx].order_group > this_group)
        .unwrap_or(false);

    source_to_right || target_to_right
}

/// Helper: Ensures a branch doesn't overlap with the column its target is merging into
fn check_merge_collision(branch: &BranchInfo, col_idx: usize, layout: &TrackLayout) -> bool {
    if let Some(target_idx) = branch.target_branch {
        if let Some(&tv_idx) = layout.track_visual.get(&target_idx) {
            let target_vis = &layout.branch_visual[tv_idx];
            let this_vis = &layout.branch_visual[layout.track_visual[&branch.target_branch.unwrap()]];

            if target_vis.order_group == this_vis.order_group
            && target_vis.column == Some(col_idx) {
                return true;
            }
        }
    }
    false
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
