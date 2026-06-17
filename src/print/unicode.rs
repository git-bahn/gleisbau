/*! Create graphs in Unicode format with ANSI X3.64 / ISO 6429 colour codes

Terminals usuallly have very tall characters, so to get a square ratio
we need to double the number of rows. Although unicode allows drawing
of lines, it does not support parallel lines inside the same symbol.
To fix this we add extra "inserts", extra lines per commit as needed
to draw lines without unwanted overlap.

The main functions are:
- [print_unicode] - Legacy API. Prints both graph and commit text from a [GitGraph]
- [print_graph_terminal] - Print graph only from [TrackMap] and [TrackLayout]

## Coordinate system

The final output is rendered onto a private 2D struct 'Grid' before printing.
This is in the final coordinate system including extra columns and rows.
[TrackLayout] uses the abstract coordinate system of commit row and
track column, so you need to keep track of which coordinate system a
function uses.
*/

use std::cmp::max;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;
use std::fmt::Write;

use git2::Commit;
use git2::Repository;
use itertools::Itertools;
use textwrap::Options;
use yansi::Paint;

use crate::backend::git2::TrackMap;
use crate::graph::{BranchInfo, CommitInfo, GitGraph, HeadInfo};
use crate::layout::BranchVis;
use crate::layout::TrackLayout;
use crate::print::format::CommitFormat;
use crate::print::grid::vline;
use crate::print::grid::zig_zag_line;
use crate::print::grid::Grid;
use crate::print::grid::GridCell;
use crate::print::grid::CIRCLE;
use crate::print::grid::DOT;
use crate::print::grid::SPACE;
use crate::print::label::list_labels;
use crate::print::label::Label;
use crate::print::label::LabelMap;
use crate::print::label::LabelType;
use crate::settings::{Characters, Settings};

// Color index used by yansi

const WHITE: u8 = 7; // Normal white
const HEAD_COLOR: u8 = 14; // Bright cyan
const HASH_COLOR: u8 = 11; // Bright yellow

/// A set of occupations planned for a row of output.
/// The ordering is not important.
type RowRenderPlan = Vec<Occ>;

/// A list of rows planned for output of a commit and lines associated with it.
type CommitRenderPlan = Vec<RowRenderPlan>;

/**
UnicodeGraphInfo is a type alias for a tuple containing three elements:
graph-lines, text-lines, start-row

1.  graph_lines: `Vec<String>` - This represents the lines of the generated text-based graph
    visualization. Each `String` in this vector corresponds to a single row of
    the graph output, containing characters that form the visual representation
    of the commit history (like lines, dots, and branch intersections).

2.  text_lines: `Vec<String>`: This represents the lines of the commit messages or other
    textual information associated with each commit in the graph. Each `String`
    in this vector corresponds to a line of text that is displayed alongside
    the graph. This can include commit hashes, author information, commit
    messages, branch names, and tags, depending on the formatting settings.
    Some entries in this vector might be empty strings or correspond to
    inserted blank lines for visual spacing.

3.  start_row: `Vec<usize>`: Starting row for commit in the `tracks.commits` vector.
*/
pub type UnicodeGraphInfo = (Vec<String>, Vec<String>, Vec<usize>);

/// Creates a text-based visual representation of a graph.
pub fn print_unicode(graph: &GitGraph, settings: &Settings) -> Result<UnicodeGraphInfo, String> {
    log::trace!("print_unicode - legacy API");
    let repo = &graph.repository;
    let tracks = &graph.tracks;
    let layout = &graph.layout;

    if tracks.all_branches.is_empty() {
        return Ok((vec![], vec![], vec![]));
    }

    // Calculate graph width and vertical inserts
    let num_cols = calculate_graph_dimensions(&graph.layout);
    let inserts = get_inserts(tracks, layout, settings.compact);

    // Use graph with to format commit text, taking inserts into account
    // index_map lists the start row of each commit in layout range
    let (mut text_lines, index_map) = build_commit_lines_and_map(
        settings,
        repo,
        tracks,
        layout,
        num_cols,
        &graph.head,
        &inserts,
    )?;

    // Draw the graph on a grid
    let total_rows = text_lines.len();
    let mut grid = draw_graph_lines(
        settings, tracks, layout, num_cols, &inserts, &index_map, total_rows,
    );

    // Handle reverse order
    if settings.reverse_commit_order {
        text_lines.reverse();
        grid.reverse();
    }

    // Print graph and text as two equal length lists of ansi coloured text rows
    let lines = print_graph_and_text(&settings.characters, &grid, text_lines, settings.colored);

    Ok((lines.0, lines.1, index_map))
}

/// Calculates the necessary column count for the graph grid.
fn calculate_graph_dimensions(layout: &TrackLayout) -> usize {
    let max_column = layout
        .track_visual_vec()
        .iter()
        .map(|b_visual| b_visual.column.unwrap_or(0))
        .max()
        .unwrap_or(0);
    2 * max_column + 1
}

/// Iterates through commits to compute text lines, blank line inserts, and the index map.
fn build_commit_lines_and_map(
    settings: &Settings,
    repository: &Repository,
    tracks: &TrackMap,
    layout: &TrackLayout,
    num_cols: usize,
    the_head: &HeadInfo,
    inserts: &HashMap<usize, CommitRenderPlan>,
) -> Result<
    (
        Vec<Option<String>>,
        Vec<usize>, // index_map: from (commit index relative to layout) to grid row
    ),
    String,
> {
    // Compute textwrap options
    let indent1 = settings
        .wrapping
        .map(|(_, ind1, _)| " ".repeat(ind1.unwrap_or(0)));
    let indent2 = settings
        .wrapping
        .map(|(_, _, ind2)| " ".repeat(ind2.unwrap_or(0)));
    let wrap_options_owned: Option<Options> = settings.wrapping
        .map(|(width, _, _)| {
            let indent1 = indent1.as_ref().unwrap();
            let indent2 = indent2.as_ref().unwrap();
            create_wrapping_options(width, indent1, indent2, num_cols + 4)
        })
        .transpose()? // Return if we got an Err
        .flatten() // reduce OptionOption to Option
    ;
    let wrap_options: &Option<Options> = &wrap_options_owned;

    // Compute decorating labels
    let labels = list_labels(settings, repository)?;
    let head_idx = tracks.indices.get(&the_head.oid);

    // Compute commit text into text_lines and add blank rows
    // if needed to match branch graph inserts.
    let mut index_map = vec![];
    let mut text_lines = vec![];
    let mut offset = 0;

    for idx in layout.iter_commit_index() {
        index_map.push(idx + offset - layout.commit_index_start());

        // Calculate needed graph inserts (for ranges only)
        let cnt_inserts = if let Some(inserts) = inserts.get(&idx) {
            inserts
                .iter()
                .filter(|vec| {
                    vec.iter().all(|occ| match occ {
                        Occ::Commit(_, _) => false,
                        Occ::Range(_, _, _, _) => true,
                    })
                })
                .count()
        } else {
            0
        };

        let head = if head_idx == Some(&idx) {
            Some(the_head)
        } else {
            None
        };

        let lines;
        if let Some(info) = tracks.commits.get(idx) {
            let commit = &repository
                .find_commit(info.oid)
                .map_err(|err| err.message().to_string())?;

            // Format the commit message lines
            lines = format(
                &settings.format,
                layout,
                &labels,
                commit,
                info,
                head,
                settings.colored,
                wrap_options,
            )?;
        } else {
            lines = vec![];
        }
        let num_lines = if lines.is_empty() { 0 } else { lines.len() - 1 };
        let max_inserts = max(cnt_inserts, num_lines);
        let add_lines = max_inserts - num_lines;

        // Extend text_lines with commit lines and blank lines for padding
        text_lines.extend(lines.into_iter().map(Some));
        text_lines.extend((0..add_lines).map(|_| None));

        offset += max_inserts;
    }

    Ok((text_lines, index_map))
}

/// Iterates through commits to compute the index map.
fn build_commit_map(
    layout: &TrackLayout,
    inserts: &HashMap<usize, CommitRenderPlan>, // graphical extra lines
    commit_height: &[usize],                    // text lines required
) -> Result<Vec<usize>, String> {
    // Compute commit index to output row map
    let mut index_map = vec![];
    let mut offset = 0;

    for idx in layout.iter_commit_index() {
        index_map.push(idx + offset - layout.commit_index_start());

        // Calculate needed graph inserts (for ranges only)
        let cnt_inserts = if let Some(inserts) = inserts.get(&idx) {
            inserts
                .iter()
                .filter(|vec| {
                    vec.iter().all(|occ| match occ {
                        Occ::Commit(_, _) => false,
                        Occ::Range(_, _, _, _) => true,
                    })
                })
                .count()
        } else {
            0
        };

        // Calculate needed text inserts
        let commit_height_index = idx - layout.commit_index_start();
        let num_lines = commit_height
            .get(commit_height_index)
            .unwrap_or(&1)
            .saturating_sub(1);

        // Make room for the largest number of inserts
        let max_inserts = max(cnt_inserts, num_lines);
        offset += max_inserts;
    }

    Ok(index_map)
}

/// Initializes the grid and draws all commit/branch connections.
///
/// # Arguments
/// * index_map  map commit index relative to layout start, to a row in the grid
fn draw_graph_lines(
    settings: &Settings,
    tracks: &TrackMap,
    layout: &TrackLayout,
    num_cols: usize,
    inserts: &HashMap<usize, CommitRenderPlan>,
    index_map: &[usize], // map to grid row from relative commit index
    total_rows: usize,
) -> Grid {
    let mut grid = Grid::new(
        num_cols,
        total_rows,
        GridCell {
            character: SPACE,
            color: WHITE,
            pers: settings.branches.persistence.len() as u8 + 2,
        },
    );

    for idx in layout.iter_commit_index() {
        let Some(info) = tracks.commits.get(idx) else {
            continue;
        };
        let Some(trace) = info.branch_trace else {
            continue;
        };
        let branch = &tracks.all_branches[trace];
        let branch_visual = layout
            .track_visual(trace)
            .expect("All commits in range has precomputed visuals");
        let column = branch_visual.column.unwrap();
        let idx_map = index_map[idx - layout.commit_index_start()];

        // Draw commit point (DOT or CIRCLE)
        grid.set(
            column * 2,
            idx_map,
            if info.is_merge() { CIRCLE } else { DOT },
            branch_visual.term_color,
            branch.persistence,
        );

        // Draw parent lines from this commit
        draw_parent_lines(
            tracks,
            layout,
            branch,
            branch_visual,
            &mut grid,
            info,
            inserts,
            index_map,
            idx,
        );
    }
    grid
}

#[allow(clippy::too_many_arguments)]
fn draw_parent_lines(
    tracks: &TrackMap,
    layout: &TrackLayout,
    branch: &BranchInfo,
    branch_visual: &BranchVis,
    grid: &mut Grid,
    info: &CommitInfo,
    inserts: &HashMap<usize, CommitRenderPlan>,
    index_map: &[usize], // map relative commit index to row
    idx: usize,          // absolute commit index
) {
    let column = branch_visual.column.unwrap();
    // index_map is from commit index relative to layout start
    let idx_map = index_map[idx - layout.commit_index_start()];

    let branch_color = branch_visual.term_color;

    for p in 0..info.parents.len() {
        let par_oid = info.parents[p];
        let Some(par_idx) = tracks.indices.get(&par_oid) else {
            // Parent is outside scope of tracks.indices
            // so draw a vertical line to the bottom
            let idx_bottom = grid.height;
            vline(
                grid,
                (idx_map, idx_bottom),
                column,
                branch_color,
                branch.persistence,
            );
            continue;
        };

        // index_map is from relative commit index to row
        let Some(&par_idx_map) = index_map.get(*par_idx - layout.commit_index_start()) else {
            // Parent was outside layout
            // so draw a vertical line to the bottom
            let idx_bottom = grid.height;
            vline(
                grid,
                (idx_map, idx_bottom),
                column,
                branch_color,
                branch.persistence,
            );
            continue;
        };
        let par_info = &tracks.commits[*par_idx];
        let par_track_idx = par_info.branch_trace.unwrap();
        let par_branch = &tracks.all_branches[par_track_idx];
        let par_branch_visual = layout
            .track_visual(par_track_idx)
            .expect("Parent must have visuals");
        let par_column = par_branch_visual.column.unwrap();

        let (color, pers) = if info.is_merge() {
            (par_branch_visual.term_color, par_branch.persistence)
        } else {
            (branch_color, branch.persistence)
        };

        if branch_visual.column == par_branch_visual.column {
            if par_idx_map > idx_map + 1 {
                vline(grid, (idx_map, par_idx_map), column, color, pers);
            }
        } else {
            let split_index = get_deviate_index(tracks, layout, idx, *par_idx);
            // index_map is from relative commit index to row
            let split_idx_map = index_map[split_index - layout.commit_index_start()];
            let insert_ofs = find_insert_ofs(&inserts[&split_index], idx, *par_idx).unwrap();
            let idx_split = split_idx_map + insert_ofs;

            let is_secondary_merge = info.is_merge() && p > 0;

            let row123 = (idx_map, idx_split, par_idx_map);
            let col12 = (column, par_column);
            zig_zag_line(grid, row123, col12, is_secondary_merge, color, pers);
        }
    }
}

/// Create `textwrap::Options` from width and indent.
fn create_wrapping_options<'a>(
    width: Option<usize>,
    indent1: &'a str,
    indent2: &'a str,
    graph_width: usize,
) -> Result<Option<Options<'a>>, String> {
    let wrapping = if let Some(width) = width {
        Some(
            textwrap::Options::new(width)
                .initial_indent(indent1)
                .subsequent_indent(indent2),
        )
    } else if atty::is(atty::Stream::Stdout) {
        let width = crossterm::terminal::size()
            .map_err(|err| err.to_string())?
            .0 as usize;
        let text_width = width.saturating_sub(graph_width);
        if text_width < 40 {
            // If too little space left for text, do not wrap at all
            None
        } else {
            Some(
                textwrap::Options::new(text_width)
                    .initial_indent(indent1)
                    .subsequent_indent(indent2),
            )
        }
    } else {
        None
    };
    Ok(wrapping)
}

/// Find the relative row of the insert that connects the two commits
fn find_insert_ofs(
    commit_inserts: &CommitRenderPlan,
    child_idx: usize,
    parent_idx: usize,
) -> Option<usize> {
    for (insert_idx, sub_entry) in commit_inserts.iter().enumerate() {
        for occ in sub_entry {
            if let Occ::Range(i1, i2, _, _) = occ {
                if *i1 == child_idx && *i2 == parent_idx {
                    return Some(insert_idx);
                }
            }
        }
    }
    None
}

/** Make a plan for drawing commits and lines.

    Calculates required additional rows to visually connect commits that
    are not direct descendants in the main commit list. These "inserts"
    represent the horizontal lines in the graph.

    ## Arguments

    * `tracks`: The track topology used for the layout.
    * `layout`: The layout that should be drawn.
    * `compact`: Enable merging insertions with commits to save place.

    ## Returns

    A `HashMap` where the keys are the indices of commits in the
    `tracks.commits` vector, and the values are a plan for rendering
    that commit. See [CommitRenderPlan]

    # Algorithm

    The internal rendering algorithm follows these steps:

    * Make a plan (called "inserts") for where to draw lines.
    * Follow the plan to draw lines and symbols on a [Grid].
    * Print the grid as ANSI formatted text for a terminal.
*/
fn get_inserts(
    tracks: &TrackMap,
    layout: &TrackLayout,
    compact: bool,
) -> HashMap<usize, CommitRenderPlan> {
    // Initialize an empty HashMap to store the required insertions. The key is the commit
    // index, and the value is a vector of rows, where each row is a vector of Occupations (`Occ`).
    let mut inserts: HashMap<usize, CommitRenderPlan> = HashMap::new();

    // First, for each commit, we initialize an entry in the `inserts`
    // map with a single row containing the commit itself. This ensures
    // that every commit has a position in the grid.
    for idx in layout.iter_commit_index() {
        let Some(info) = tracks.commits.get(idx) else {
            // layout is too far down, so it includes index that are not
            // in TrackMap. Provide an empty render plan for that row.
            inserts.insert(idx, vec![vec![]]);
            continue;
        };
        // Get the visual column assigned to the branch of this commit. Unwrap is safe here
        // because `branch_trace` should always point to a valid branch with an assigned column
        // for commits that are included in the filtered graph.
        let track_inx = info.branch_trace.unwrap();
        let column = layout
            .track_visual(track_inx)
            .expect("Visuals must be present for track")
            .column
            .expect("Track must have a column");

        inserts.insert(idx, vec![vec![Occ::Commit(idx, column)]]);
    }

    // Now, iterate through the commits again to identify connections
    // needed between parents that are not directly adjacent in the
    // `tracks.commits` list.
    for idx in layout.iter_commit_index() {
        let Some(info) = tracks.commits.get(idx) else {
            continue;
        };
        // If the commit has a branch trace (meaning it belongs to a visualized branch).
        if let Some(trace) = info.branch_trace {
            // Get the `BranchInfo` for the current commit's branch.
            let branch_visual = layout
                .track_visual(trace)
                .expect("All tracks in print range must have visuals");
            // Get the visual column of the current commit's branch. Unwrap is safe as explained above.
            let column = branch_visual.column.unwrap();

            // Iterate through the parents of the current commit.
            for p in 0..info.parents.len() {
                let par_oid = info.parents[p];
                // Try to find the index of the parent commit in the `tracks.commits` vector.
                if let Some(par_idx) = tracks.indices.get(&par_oid) {
                    let par_info = &tracks.commits[*par_idx];
                    let par_track_idx = par_info.branch_trace.unwrap();
                    let par_branch_visual_opt = layout.track_visual(par_track_idx);
                    let Some(par_branch_visual) = par_branch_visual_opt else {
                        // Parent does not have visuals.
                        if layout.contains_commit_index(*par_idx) {
                            // Parent should have been visualized
                            // as it is inside the visualisation range
                            panic!("Parent track must have visuals")
                        } else {
                            // Ignore parent outside layout
                            // TODO visualize this relation
                            continue;
                        }
                    };
                    let par_column = par_branch_visual.column.unwrap();
                    // Determine the sorted range of columns between the current commit and its parent.
                    let column_range = sorted(column, par_column);

                    // If the column of the current commit is different from the column of its parent,
                    // it means we need to draw a horizontal line (an "insert") to connect them.
                    if column != par_column {
                        // Find the index in the `tracks.commits` list where the visual connection
                        // should deviate from the parent's line. This helps in drawing the graph
                        // correctly when branches diverge or merge.
                        let split_index = get_deviate_index(tracks, layout, idx, *par_idx);
                        // Access the entry in the `inserts` map for the `split_index`.
                        match inserts.entry(split_index) {
                            // If there's already an entry at this `split_index` (meaning other
                            // insertions might be needed before this commit).
                            Occupied(mut entry) => {
                                // Find the first available row in the existing vector of rows
                                // where the new range doesn't overlap with existing occupations.
                                let mut insert_at = entry.get().len();
                                for (insert_idx, sub_entry) in entry.get().iter().enumerate() {
                                    let occ = has_overlap(
                                        sub_entry,
                                        column_range,
                                        compact,
                                        info.is_merge(),
                                        idx,
                                        p,
                                        par_idx,
                                    );
                                    // If no overlap is found in this row, we can insert here.
                                    if !occ {
                                        insert_at = insert_idx;
                                        break;
                                    }
                                }
                                // Get a mutable reference to the vector of rows for this `split_index`.
                                let vec = entry.get_mut();
                                // If no suitable row was found, add a new row.
                                if insert_at == vec.len() {
                                    vec.push(vec![Occ::Range(
                                        idx,
                                        *par_idx,
                                        column_range.0,
                                        column_range.1,
                                    )]);
                                } else {
                                    // Otherwise, insert the new range into the found row.
                                    vec[insert_at].push(Occ::Range(
                                        idx,
                                        *par_idx,
                                        column_range.0,
                                        column_range.1,
                                    ));
                                }
                            }
                            // If there's no entry at this `split_index` yet.
                            Vacant(entry) => {
                                // Create a new entry with a single row containing the range.
                                entry.insert(vec![vec![Occ::Range(
                                    idx,
                                    *par_idx,
                                    column_range.0,
                                    column_range.1,
                                )]]);
                            }
                        }
                    }
                }
            }
        }
    }

    // Return the map of required insertions.
    inserts
}

/// Checks if a proposed horizontal connection (range) overlaps or conflicts with
/// existing elements in a specific layout row.
///
/// In standard layout modes, any overlap with an existing commit or an existing
/// connection range constitutes a conflict. However, in `compact` mode, an overlap
/// with a commit is permitted if the current commit is a merge commit and the connection
/// belongs to its second (or subsequent) parent. Overlaps between the exact same
/// commit-to-parent connection paths are also ignored to prevent redundant blocking.
///
/// # Arguments
///
/// * `sub_entry` - The current row of visual elements (`Occ`) to check against.
/// * `column_range` - A tuple `(min_col, max_col)` representing the horizontal span of the new connection.
/// * `compact` - A boolean flag; if true, enables tighter graph spacing rule exceptions.
/// * `info_is_merge` - True if the current commit being processed is a merge.
/// * `idx` - The index of the current commit in the track list.
/// * `p` - The parent index currently being evaluated
///   (e.g., `0` for first parent, `1` for second, etc).
/// * `par_idx` - The index of the parent commit in the track list.
///
/// # Returns
///
/// Returns `true` if there is an unallowable visual collision in this row,
/// and `false` if the connection can safely occupy this row.
fn has_overlap(
    sub_entry: &RowRenderPlan,
    column_range: (usize, usize),
    compact: bool,
    info_is_merge: bool,
    idx: usize,
    p: usize,
    par_idx: &usize,
) -> bool {
    // Check for overlaps with existing `Occ` in the current row.
    for other_range in sub_entry {
        // Check if the current column range overlaps with the other range.
        if other_range.overlaps(&column_range) {
            match other_range {
                // If the other occupation is a commit.
                Occ::Commit(target_index, _) => {
                    // In compact mode, we allow overlap with the commit itself
                    // for non-primary parents of merge commits to keep the
                    // graph tighter.
                    if !compact  // in non-compact mode a commit always collides
                        || !info_is_merge // non-merge commit always collide
                        || idx != *target_index // other commits always collide
                        || p == 0
                    // primary parent always collide
                    {
                        return true;
                    }
                }
                // If the other occupation is a connection between commits.
                Occ::Range(o_idx, o_par_idx, _, _) => {
                    // Ignore overlap with connections to the current commit.
                    if idx != *o_idx && par_idx != o_par_idx {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Find the index at which a between-branch connection
/// has to deviate from the current branch's column.
///
/// Returns the last index on the current column.
///
/// Arguments
///   tracks - grouping of commits into tracks
///   layout - 2D arrangement of tracks
///   index - index of commit in TrackMap
///   par_index - index of parent of commit
/// Returns:
///   index of oldest commit in same coloum as start commit ???
fn get_deviate_index(
    tracks: &TrackMap,
    layout: &TrackLayout,
    index: usize,
    par_index: usize,
) -> usize {
    let info = &tracks.commits[index];

    let par_info = &tracks.commits[par_index];
    let par_track_idx = par_info.branch_trace.unwrap();
    let par_branch_visual = layout
        .track_visual(par_track_idx)
        .expect("Parent must have visual");

    let mut min_split_idx = index;
    for sibling_oid in &par_info.children {
        if let Some(&sibling_index) = tracks.indices.get(sibling_oid) {
            if let Some(sibling) = tracks.commits.get(sibling_index) {
                if let Some(sibling_trace) = sibling.branch_trace {
                    let sibling_branch_visual = layout
                        .track_visual(sibling_trace)
                        .expect("Sibling must have visual");
                    if sibling_oid != &info.oid
                        && sibling_branch_visual.column == par_branch_visual.column
                        && sibling_index > min_split_idx
                    {
                        min_split_idx = sibling_index;
                    }
                }
            }
        }
    }

    // TODO: in cases where no crossings occur, the rule for merge commits can also be applied to normal commits
    // See also branch::trace_branch()
    if info.is_merge() {
        max(index, min_split_idx)
    } else {
        (par_index as i32 - 1) as usize
    }
}

//
//  Graph only printing from TrackMap
//

/** Print a graph as lines for a terminal
# Arguments
- settings
- tracks
- layout
- commit_text_height : a text height for each commit in the layout.
  If a commit has height > 1 then extra graph lines will be added
  to match this. The grapy may determine that a commit require two
  lines, even though the commit_text only asked for 1.
# Returns
  [GraphLines], which is a list of graph output and the output row where
  a specific row starts.
*/
pub fn print_graph_terminal(
    settings: &Settings,
    tracks: &TrackMap,
    layout: &TrackLayout,
    commit_text_height: &[usize], // [0] corresponds to track commit layout.commit_index_start()
) -> GraphLines {
    log::trace!("print_graph_terminal(_,_,_,_)");
    if tracks.all_branches.is_empty() {
        return GraphLines::empty();
    }

    // inserts are extra lines needed when the layout cannot be drawn on
    // a single line. They influence the number of rows needed
    let inserts = get_inserts(tracks, layout, settings.compact);

    // The index map gives the row number from a commit index, relative
    // to layout.commit_index_start()
    let index_map =
        build_commit_map(layout, &inserts, commit_text_height).expect("valid commit_text_height");

    // Compute grid size
    let num_cols = calculate_graph_dimensions(layout);
    let min_row_height = 1;
    let rows_without_commit_text = layout
        .commit_count()
        .saturating_sub(commit_text_height.len());
    let total_rows = commit_text_height
        .iter()
        .map(|&x| max(min_row_height, x))
        .take(layout.commit_count())
        .sum::<usize>()
        + rows_without_commit_text * min_row_height;

    // Draw graph as lines on a grid
    let mut grid = draw_graph_lines(
        settings, tracks, layout, num_cols, &inserts, &index_map, total_rows,
    );

    // Handle reverse order
    if settings.reverse_commit_order {
        grid.reverse();
    }

    // 6. Final printing and result
    let lines = grid_print_terminal(&settings.characters, &grid, settings.colored);

    GraphLines {
        graph_lines: lines,
        commit2line: index_map,
    }
}

/// Printed lines of graph along with he commit index
pub struct GraphLines {
    /// The graph printed as lines
    pub graph_lines: Vec<String>,
    /// Map from commit index in [TrackLayout] to line number in 'graph_lines'.
    /// To find the commit index in [TrackMap] add [TrackLayout::commit_index_start].
    pub commit2line: Vec<usize>,
}

impl GraphLines {
    pub fn empty() -> Self {
        Self {
            graph_lines: vec![],
            commit2line: vec![],
        }
    }
}

/// Print a grid as ansi coloured strings. Optionally removing colour.
/// Grid uses symbols, they are rendered according to the provided
/// Characters map.
fn grid_print_terminal(characters: &Characters, grid: &Grid, color: bool) -> Vec<String> {
    let mut g_lines = vec![];

    let cell2string = |cell: &GridCell| -> String {
        if color {
            let chars = cell.char(characters);
            if cell.character == SPACE {
                chars.to_string()
            } else {
                chars.to_string().fixed(cell.color).to_string()
            }
        } else {
            cell.char(characters).to_string()
        }
    };

    for row in grid.data.chunks(grid.width) {
        let mut g_out = String::new();

        let str = row.iter().map(&cell2string).collect::<String>();
        write!(g_out, "{}", str).unwrap();

        g_lines.push(g_out);
    }

    g_lines
}

/// Creates the complete graph visualization, incl. formatter commits.
fn print_graph_and_text(
    characters: &Characters,
    grid: &Grid,
    text_lines: Vec<Option<String>>,
    color: bool,
) -> (Vec<String>, Vec<String>) {
    let mut g_lines = vec![];
    let mut t_lines = vec![];

    for (row, line) in grid.data.chunks(grid.width).zip(text_lines) {
        let mut g_out = String::new();
        let mut t_out = String::new();

        if color {
            for cell in row {
                let chars = cell.char(characters);
                if cell.character == SPACE {
                    write!(g_out, "{}", chars)
                } else {
                    write!(g_out, "{}", chars.to_string().fixed(cell.color))
                }
                .unwrap();
            }
        } else {
            let str = row
                .iter()
                .map(|cell| cell.char(characters))
                .collect::<String>();
            write!(g_out, "{}", str).unwrap();
        }

        if let Some(line) = line {
            write!(t_out, "{}", line).unwrap();
        }

        g_lines.push(g_out);
        t_lines.push(t_out);
    }

    (g_lines, t_lines)
}

/// Format a commit.
#[allow(clippy::too_many_arguments)]
fn format(
    format: &CommitFormat,
    layout: &TrackLayout,
    labels: &LabelMap,
    commit: &Commit,
    info: &CommitInfo,
    head: Option<&HeadInfo>,
    color: bool,
    wrapping: &Option<Options>,
) -> Result<Vec<String>, String> {
    let branch_str = format_branches(layout, info, labels, head, color);

    let hash_color = if color { Some(HASH_COLOR) } else { None };

    crate::print::format::format_commit_metadata(commit, branch_str, wrapping, hash_color, format)
}

/// Build a string listing branches and tag
pub fn format_branches(
    layout: &TrackLayout,
    info: &CommitInfo,
    labels: &LabelMap,
    head: Option<&HeadInfo>,
    color: bool,
) -> String {
    let curr_color = info
        .branch_trace
        .and_then(|branch_idx| layout.track_visual(branch_idx))
        .map(|visual| visual.term_color);

    let mut branch_str = String::new();
    fn append_str_col(target: &mut String, s: &str, color: bool, s_col: u8) {
        if color {
            write!(target, "{}", s.fixed(s_col)).unwrap();
        } else {
            write!(target, "{}", s).unwrap();
        }
    }

    let head_str = "HEAD ->";
    if let Some(head) = head {
        if !head.is_branch {
            branch_str.push(' ');
            append_str_col(&mut branch_str, head_str, color, HEAD_COLOR);
        }
    }

    let commit_branches: Vec<Label> = labels
        .get_labels(&info.oid)
        .into_iter()
        .flatten()
        .filter(|label| {
            label.kind == LabelType::LocalBranch || label.kind == LabelType::RemoteBranch
        })
        .cloned()
        .collect();

    if !commit_branches.is_empty() {
        branch_str.push_str(" (");

        // move head branch up front
        let branches = commit_branches.iter().sorted_by_key(|label| {
            if let Some(head) = head {
                head.name != label.name
            } else {
                false
            }
        });

        for (idx, label) in branches.enumerate() {
            let branch_color = label.term_color;

            if let Some(head) = head {
                if idx == 0 && head.is_branch {
                    append_str_col(&mut branch_str, head_str, color, HEAD_COLOR);
                    branch_str.push(' ');
                }
            }

            append_str_col(&mut branch_str, &label.name, color, branch_color);

            if idx < commit_branches.len() - 1 {
                branch_str.push_str(", ");
            }
        }
        branch_str.push(')');
    }

    let commit_tags: Vec<_> = labels
        .get_labels(&info.oid)
        .into_iter()
        .flatten()
        .filter(|label| label.kind == LabelType::Tag)
        .collect();
    if !commit_tags.is_empty() {
        branch_str.push_str(" [");
        for (idx, tag_label) in commit_tags.iter().enumerate() {
            // Use branch colour if present, otherwise use tag color
            // TODO Should this be the reverse??
            // Is the logic correct, and branches can have color None?
            let tag_color = curr_color.unwrap_or(tag_label.term_color);

            append_str_col(&mut branch_str, &tag_label.name, color, tag_color);

            if idx < commit_tags.len() - 1 {
                branch_str.push_str(", ");
            }
        }
        branch_str.push(']');
    }

    branch_str
}

/** Occupied columns.

The occupation can either be a Commit, which takes just one column,
or a Range, which takes a range of columns. Range is used for horizontal lines.
*/
enum Occ {
    /// Horizontal position of commit markers
    // First  field (usize): The index of a commit within the tracks.commits vector.
    // Second field (usize): The visual column in the grid where this commit is located. This column is determined by the branch the commit belongs to.
    // Purpose: This variant of Occ signifies that a specific row in the grid is occupied by a commit marker (dot or circle) at a particular column.
    Commit(usize, usize), // index in tracks.commits, column

    /// Horizontal line connecting two commits
    // First  field (usize): The index of the starting commit of a visual connection (usually the child commit).
    // Second field (usize): The index of the ending commit of a visual connection (usually the parent commit).
    // Third  field (usize): The starting visual column of the range occupied by the connection line between the two commits. This is the minimum of the columns of the two connected commits.
    // Fourth field (usize): The ending visual column of the range occupied by the connection line between the two commits. This is the maximum of the columns of the two connected commits.
    // Purpose: This variant of Occ signifies that a range of columns in a particular row is occupied by a horizontal line segment connecting a commit to one of its parents. The range spans from the visual column of one commit to the visual column of the other.
    Range(usize, usize, usize, usize), // ?child index, parent index, leftmost column, rightmost column
}

impl Occ {
    fn overlaps(&self, (start, end): &(usize, usize)) -> bool {
        match self {
            Occ::Commit(_, col) => start <= col && end >= col,
            Occ::Range(_, _, s, e) => s <= end && e >= start,
        }
    }
}

/// Sorts two numbers in ascending order
fn sorted(v1: usize, v2: usize) -> (usize, usize) {
    if v2 > v1 {
        (v1, v2)
    } else {
        (v2, v1)
    }
}
