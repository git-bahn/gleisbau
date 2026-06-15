/*! 2D structure for drawing lines before generating terminal text.

The [Grid] holds a 2 dimensional array of [GridCell] each holding
a symbol (character), a colour and a z-order (persistence level).

The symbols are defined abstractly here as constants,
whereas the mapping to an acctual character used for printing a symbol
is defined by [Characters].
*/

use crate::settings::Characters;

// Symbols used in [Grid]

pub const SPACE: u8 = 0;
pub const DOT: u8 = 1;
pub const CIRCLE: u8 = 2;
const VER: u8 = 3;
const HOR: u8 = 4;
const CROSS: u8 = 5;
const R_U: u8 = 6;
const R_D: u8 = 7;
const L_D: u8 = 8;
const L_U: u8 = 9;
const VER_L: u8 = 10;
const VER_R: u8 = 11;
const HOR_U: u8 = 12;
const HOR_D: u8 = 13;

const ARR_L: u8 = 14;
const ARR_R: u8 = 15;

/// One cell in a [Grid]
#[derive(Clone, Copy)]
pub struct GridCell {
    /// The symbol shown, encoded as in index into settings::Characters
    pub character: u8,
    /// Standard 8-bit terminal colour code
    pub color: u8,
    /// Persistence level. z-order, lower numbers take preceedence.
    pub pers: u8,
}

impl GridCell {
    pub fn char(&self, characters: &Characters) -> char {
        characters.chars[self.character as usize]
    }
}

/// Two-dimensional grid used to hold the graph layout.
///
/// This can be rendered as unicode text or as SVG.
pub struct Grid {
    pub width: usize,
    pub height: usize,

    /// Grid cells are stored in row-major order.
    pub data: Vec<GridCell>,
}

impl Grid {
    pub fn new(width: usize, height: usize, initial: GridCell) -> Self {
        log::trace!("Grid::new({},{},_)", width, height);
        Grid {
            width,
            height,
            data: vec![initial; width * height],
        }
    }

    pub fn reverse(&mut self) {
        self.data.reverse();
    }
    /// Turn a 2D coordinate into an index of Grid.data
    pub fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }
    pub fn get_tuple(&self, x: usize, y: usize) -> (u8, u8, u8) {
        let v = self.data[self.index(x, y)];
        (v.character, v.color, v.pers)
    }
    pub fn set(&mut self, x: usize, y: usize, character: u8, color: u8, pers: u8) {
        let idx = self.index(x, y);
        self.data[idx] = GridCell {
            character,
            color,
            pers,
        };
    }
    pub fn set_opt(
        &mut self,
        x: usize,
        y: usize,
        character: Option<u8>,
        color: Option<u8>,
        pers: Option<u8>,
    ) {
        let idx = self.index(x, y);
        let cell = &mut self.data[idx];
        if let Some(character) = character {
            cell.character = character;
        }
        if let Some(color) = color {
            cell.color = color;
        }
        if let Some(pers) = pers {
            cell.pers = pers;
        }
    }
}

/// Draw a line that connects two commits on different columns
pub fn zig_zag_line(
    grid: &mut Grid,
    row123: (usize, usize, usize),
    col12: (usize, usize),
    is_merge: bool,
    color: u8,
    pers: u8,
) {
    let (row1, row2, row3) = row123;
    let (col1, col2) = col12;
    vline(grid, (row1, row2), col1, color, pers);
    hline(grid, row2, (col2, col1), is_merge, color, pers);
    vline(grid, (row2, row3), col2, color, pers);
}

/// Draws a vertical line
pub fn vline(grid: &mut Grid, (from, to): (usize, usize), column: usize, color: u8, pers: u8) {
    for i in (from + 1)..to {
        let (curr, _, old_pers) = grid.get_tuple(column * 2, i);
        let (new_col, new_pers) = if pers < old_pers {
            (Some(color), Some(pers))
        } else {
            (None, None)
        };
        match curr {
            DOT | CIRCLE => {}
            HOR => {
                grid.set_opt(column * 2, i, Some(CROSS), Some(color), Some(pers));
            }
            HOR_U | HOR_D => {
                grid.set_opt(column * 2, i, Some(CROSS), Some(color), Some(pers));
            }
            CROSS | VER | VER_L | VER_R => grid.set_opt(column * 2, i, None, new_col, new_pers),
            L_D | L_U => {
                grid.set_opt(column * 2, i, Some(VER_L), new_col, new_pers);
            }
            R_D | R_U => {
                grid.set_opt(column * 2, i, Some(VER_R), new_col, new_pers);
            }
            _ => {
                grid.set_opt(column * 2, i, Some(VER), new_col, new_pers);
            }
        }
    }
}

/// Draw a horizontal line.
/// If from > to, this will cause a backward draw.
fn hline(
    grid: &mut Grid,
    index: usize,
    (from, to): (usize, usize),
    merge: bool,
    color: u8,
    pers: u8,
) {
    if from == to {
        return;
    }

    let from_2 = from * 2;
    let to_2 = to * 2;

    if from < to {
        update_range_forward(grid, index, from_2, to_2, merge, color, pers);
        update_left_cell_forward(grid, index, from_2, color, pers);
        update_right_cell_forward(grid, index, to_2, color, pers);
    } else {
        update_range_backward(grid, index, from_2, to_2, merge, color, pers);
        update_left_cell_backward(grid, index, to_2, color, pers);
        update_right_cell_backward(grid, index, from_2, color, pers);
    }
}

fn update_range_forward(
    grid: &mut Grid,
    index: usize,
    from_2: usize,
    to_2: usize,
    merge: bool,
    color: u8,
    pers: u8,
) {
    for column in (from_2 + 1)..to_2 {
        if merge && column == to_2 - 1 {
            grid.set(column, index, ARR_R, color, pers);
        } else {
            let (curr, _, old_pers) = grid.get_tuple(column, index);
            let (new_col, new_pers) = if pers < old_pers {
                (Some(color), Some(pers))
            } else {
                (None, None)
            };
            match curr {
                DOT | CIRCLE => {}
                VER => grid.set_opt(column, index, Some(CROSS), None, None),
                HOR | CROSS | HOR_U | HOR_D => grid.set_opt(column, index, None, new_col, new_pers),
                L_U | R_U => grid.set_opt(column, index, Some(HOR_U), new_col, new_pers),
                L_D | R_D => grid.set_opt(column, index, Some(HOR_D), new_col, new_pers),
                _ => {
                    grid.set_opt(column, index, Some(HOR), new_col, new_pers);
                }
            }
        }
    }
}

fn update_left_cell_forward(grid: &mut Grid, index: usize, from_2: usize, color: u8, pers: u8) {
    let (left, _, old_pers) = grid.get_tuple(from_2, index);
    let (new_col, new_pers) = if pers < old_pers {
        (Some(color), Some(pers))
    } else {
        (None, None)
    };
    match left {
        DOT | CIRCLE => {}
        VER => grid.set_opt(from_2, index, Some(VER_R), new_col, new_pers),
        VER_L => grid.set_opt(from_2, index, Some(CROSS), None, None),
        VER_R => {}
        HOR | L_U => grid.set_opt(from_2, index, Some(HOR_U), new_col, new_pers),
        _ => {
            grid.set_opt(from_2, index, Some(R_D), new_col, new_pers);
        }
    }
}

fn update_right_cell_forward(grid: &mut Grid, index: usize, to_2: usize, color: u8, pers: u8) {
    let (right, _, old_pers) = grid.get_tuple(to_2, index);
    let (new_col, new_pers) = if pers < old_pers {
        (Some(color), Some(pers))
    } else {
        (None, None)
    };
    match right {
        DOT | CIRCLE => {}
        VER => grid.set_opt(to_2, index, Some(VER_L), None, None),
        VER_L | HOR_U => grid.set_opt(to_2, index, None, new_col, new_pers),
        HOR | R_U => grid.set_opt(to_2, index, Some(HOR_U), new_col, new_pers),
        _ => {
            grid.set_opt(to_2, index, Some(L_U), new_col, new_pers);
        }
    }
}

fn update_range_backward(
    grid: &mut Grid,
    index: usize,
    from_2: usize,
    to_2: usize,
    merge: bool,
    color: u8,
    pers: u8,
) {
    for column in (to_2 + 1)..from_2 {
        if merge && column == to_2 + 1 {
            grid.set(column, index, ARR_L, color, pers);
        } else {
            let (curr, _, old_pers) = grid.get_tuple(column, index);
            let (new_col, new_pers) = if pers < old_pers {
                (Some(color), Some(pers))
            } else {
                (None, None)
            };
            match curr {
                DOT | CIRCLE => {}
                VER => grid.set_opt(column, index, Some(CROSS), None, None),
                HOR | CROSS | HOR_U | HOR_D => grid.set_opt(column, index, None, new_col, new_pers),
                L_U | R_U => grid.set_opt(column, index, Some(HOR_U), new_col, new_pers),
                L_D | R_D => grid.set_opt(column, index, Some(HOR_D), new_col, new_pers),
                _ => {
                    grid.set_opt(column, index, Some(HOR), new_col, new_pers);
                }
            }
        }
    }
}

fn update_left_cell_backward(grid: &mut Grid, index: usize, to_2: usize, color: u8, pers: u8) {
    let (left, _, old_pers) = grid.get_tuple(to_2, index);
    let (new_col, new_pers) = if pers < old_pers {
        (Some(color), Some(pers))
    } else {
        (None, None)
    };
    match left {
        DOT | CIRCLE => {}
        VER => grid.set_opt(to_2, index, Some(VER_R), None, None),
        VER_R => grid.set_opt(to_2, index, None, new_col, new_pers),
        HOR | L_U => grid.set_opt(to_2, index, Some(HOR_U), new_col, new_pers),
        _ => {
            grid.set_opt(to_2, index, Some(R_U), new_col, new_pers);
        }
    }
}

fn update_right_cell_backward(grid: &mut Grid, index: usize, from_2: usize, color: u8, pers: u8) {
    let (right, _, old_pers) = grid.get_tuple(from_2, index);
    let (new_col, new_pers) = if pers < old_pers {
        (Some(color), Some(pers))
    } else {
        (None, None)
    };
    match right {
        DOT | CIRCLE => {}
        VER => grid.set_opt(from_2, index, Some(VER_L), new_col, new_pers),
        VER_R => grid.set_opt(from_2, index, Some(CROSS), None, None),
        VER_L => grid.set_opt(from_2, index, None, new_col, new_pers),
        HOR | R_D => grid.set_opt(from_2, index, Some(HOR_D), new_col, new_pers),
        _ => {
            grid.set_opt(from_2, index, Some(L_D), new_col, new_pers);
        }
    }
}






#[cfg(test)]
mod tests {
    use super::*;
    // A dummy `Characters` struct is needed for `GridCell::char` but is not
    // directly used in `hline` tests, so we can omit it by not calling `char()`.

    // --- Test Cases ---

    /* Testing hline

    Note that hline is given a graph column as input,
    which indexes a grid column at 2*graph_col
        // Graph column: 0   1   2   3   4   5
        // Grid columns: 0 1 2 3 4 5 6 7 8 9
        // Grid row 0:   _ _ _ _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _ _ _ _ _ _ _
        // Grid row 2:   _ _ _ _ _ _ _ _ _ _

    A horizontal line from 1 to 3, would occupy columns 2, 3, 4, 5, 6 inclusive

    */

    const DEF_CH: u8 = SPACE;
    const DEF_COL: u8 = 0;
    const DEF_PERS: u8 = 10; // low persistence, will always be overwritten
    const DEFAULT_CELL: GridCell = GridCell {
        character: DEF_CH,
        color: DEF_COL,
        pers: DEF_PERS,
    };
    const ROW_INDEX: usize = 1;
    const LINE_COLOR: u8 = 14;
    const LINE_PERS: u8 = 5;

    #[test]
    fn hline_skip() {
        let (width, height) = (10, 3);
        let mut grid = Grid::new(width, height, DEFAULT_CELL);
        // Graph column: 0   1   2   3   4   5
        // Grid columns: 0 1 2 3 4 5 6 7 8 9
        // Grid row 0:   _ _ _ _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _ _ _ _ _ _ _
        // Grid row 2:   _ _ _ _ _ _ _ _ _ _

        // Case 1: from == to (should do nothing)
        let initial_char = grid.get_tuple(4 * 2, ROW_INDEX).0;
        super::hline(&mut grid, ROW_INDEX, (4, 4), true, LINE_COLOR, LINE_PERS);
        // Graph column: 0   1   2   3   4   5
        // Grid columns: 0 1 2 3 4 5 6 7 8 9
        // Grid row 0:   _ _ _ _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _ _ _ _ _X_ _
        // Grid row 2:   _ _ _ _ _ _ _ _ _ _

        assert_eq!(
            grid.get_tuple(4 * 2, ROW_INDEX).0,
            initial_char,
            "Same index call should not modify grid"
        );
    }

    /// Case 2: Forward draw (from < to), no merge
    /// Case 2a: out of bounds
    #[test]
    fn hline_forward_no_merge_out_of_bounds() {
        let (width, height) = (10, 3);
        let mut grid = Grid::new(width, height, DEFAULT_CELL);
        super::hline(&mut grid, ROW_INDEX, (2, 5), false, LINE_COLOR, LINE_PERS);
        // Graph column: 0   1   2   3   4   5
        // Grid columns: 0 1 2 3 4 5 6 7 8 9
        // Grid row 0:   _ _ _ _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _F- - - - - - *T  (F=from, T=to)
        // Grid row 2:   _ _ _ _ _ _ _ _ _ _

        // from: 2, to: 5
        // Start: from*2 = 4, End: to*2 = 10.
        // Range: start+1..end = 5..=9. Grid columns updated: 5, 6, 7, 8, 9. (HOR)
        // Ends updated: start=4, end=10. (VER_R)

        // Columns outside the line range (before start) should be default
        assert_eq!(
            grid.get_tuple(0, ROW_INDEX).0,
            SPACE,
            "SPACE at start of row"
        );
        assert_eq!(grid.get_tuple(3, ROW_INDEX).0, SPACE, "SPACE before hline");

        // Start (column 4): Should be R_D - assuming a vline below
        assert_eq!(grid.get_tuple(4, ROW_INDEX).0, R_D, "R_D at start of hline");
        assert_eq!(
            grid.get_tuple(4, ROW_INDEX).1,
            LINE_COLOR,
            "line_color at start of hline"
        );
        assert_eq!(
            grid.get_tuple(4, ROW_INDEX).2,
            LINE_PERS,
            "line_pers at start of hline"
        );

        // End (column 10) is out of bounds for width 10 (index 0-9). The `Grid`
        // implementation should handle this (or it's an expected panic/logic error).
        // *Assuming* the provided `Grid` is simplified for this example and we should
        // test only within bounds. Let's adjust the indices to be safe and meaningful.
    }

    /// Case 2: Forward draw (from < to), no merge
    /// Case 2b: Inside bounds
    #[test]
    fn hline_forward_no_merge_at_bounds() {
        let safe_width = 7; // Max column index 6, max graph column 2 = grid col 5
        let height = 3;
        let mut grid = Grid::new(safe_width, height, DEFAULT_CELL);
        // Graph column: 0   1   2   3
        // Grid columns: 0 1 2 3 4 5 6
        // Grid row 0:   _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _ _ _ _
        // Grid row 2:   _ _ _ _ _ _ _

        let from_idx = 1;
        let to_idx = 3;
        // Index: 0 1 2 3
        // Cell:  - F - T
        // From: 1, To: 3.
        // Start: 2, End: 6.
        // Range: 3..5 (Columns 3, 4, 5) -> HOR
        // Ends: 2, 6 -> R_D, L_U

        assert_eq!(
            grid.get_tuple(2, ROW_INDEX).0,
            SPACE,
            "SPACE at start of line, before written"
        );
        super::hline(
            &mut grid,
            ROW_INDEX,
            (from_idx, to_idx),
            false,
            LINE_COLOR,
            LINE_PERS,
        );
        // Graph column: 0   1   2   3
        // Grid columns: 0 1 2 3 4 5 6
        // Grid row 0:   _ _ _ _ _ _ _
        // Grid row 1:   _ _(╭ ─ ─ ─ ┘)
        // Grid row 2:   _ _ _ _ _ _ _

        // Check column before start
        let grid_cell = grid.get_tuple(1, ROW_INDEX);
        assert_eq!(grid_cell.0, SPACE, "SPACE before hline");
        assert_eq!(grid_cell.1, DEF_COL, "default colour before hline");
        assert_eq!(grid_cell.2, DEF_PERS, "default persistence before hline");

        // Start (column 2): R_D
        let grid_cell = grid.get_tuple(2, ROW_INDEX);
        assert_eq!(grid_cell.0, R_D, "R_D at start of hline");
        assert_eq!(grid_cell.1, LINE_COLOR, "line_color at start of hline");
        assert_eq!(grid_cell.2, LINE_PERS, "line_pers at start of hline");

        // Range (columns 3, 4, 5): HOR
        let grid_cell = grid.get_tuple(3, ROW_INDEX);
        assert_eq!(grid_cell.0, HOR, "HOR in range of hline");
        assert_eq!(grid_cell.1, LINE_COLOR, "line_color in range of hline");
        assert_eq!(grid_cell.2, LINE_PERS, "line_pers in range of hline");

        let grid_cell = grid.get_tuple(4, ROW_INDEX);
        assert_eq!(grid_cell.0, HOR, "HOR in range of hline");
        assert_eq!(grid_cell.1, LINE_COLOR, "line_color in range of hline");
        assert_eq!(grid_cell.2, LINE_PERS, "line_pers in range of hline");

        let grid_cell = grid.get_tuple(5, ROW_INDEX);
        assert_eq!(grid_cell.0, HOR, "HOR in range of hline");
        assert_eq!(grid_cell.1, LINE_COLOR, "line_color in range of hline");
        assert_eq!(grid_cell.2, LINE_PERS, "line_pers in range of hline");

        // End (column 6): L_U
        let grid_cell = grid.get_tuple(6, ROW_INDEX);
        assert_eq!(grid_cell.0, L_U, "L_U at end of hline");
        assert_eq!(grid_cell.1, LINE_COLOR, "line_color at end of hline");
        assert_eq!(grid_cell.2, LINE_PERS, "line_pers at end of hline");

        // Check column after end
        // This is undefined, as max grid col is 6
        // TODO make expected panic
        let grid_cell = grid.get_tuple(7, ROW_INDEX);
        assert_eq!(grid_cell.0, SPACE, "SPACE before hline");
        assert_eq!(grid_cell.1, DEF_COL, "default colour before hline");
        assert_eq!(grid_cell.2, DEF_PERS, "default persistence before hline");
    }

    /// Case 3: Backward draw (from > to), with merge
    #[test]
    fn hline_backward() {
        let (width, height) = (10, 3);
        let mut grid = Grid::new(width, height, DEFAULT_CELL);
        // Set an existing symbol at an end for better coverage:
        grid.set(4, ROW_INDEX, VER, 10, 10); // Start/From pos
        grid.set(8, ROW_INDEX, HOR, 10, 10); // End/To pos

        // Graph column: 0   1   2   3   4
        // Grid columns: 0 1 2 3 4 5 6 7 8 9
        // Grid row 0:   _ _ _ _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _ │ _ _ _ ─ _
        // Grid row 2:   _ _ _ _ _ _ _ _ _ _

        let from_idx = 4;
        let to_idx = 2;
        let merge = true;
        // Index: 0 1 2 3 4
        // Cell:  - - T - F
        // Forward is false.
        // start (orig from*2) = 8, end (orig to*2) = 4. Swapped: start=4, end=8.
        // Range: start+1..end = 5..8. Columns updated: 5, 6, 7 -> HOR
        // Merge: column = start = 4. Symbol = ARR_L.
        // Ends: start=4 (backward), end=8 (forward). (Both should be L_D/R_U if they weren't SPACE)

        super::hline(
            &mut grid,
            ROW_INDEX,
            (from_idx, to_idx),
            merge,
            LINE_COLOR,
            LINE_PERS,
        );
        // Graph column: 0   1   2   3   4
        // Grid columns: 0 1 2 3 4 5 6 7 8 9
        // Grid row 0:   _ _ _ _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _ ├ < ─ ─ ┬ _
        // Grid row 2:   _ _ _ _ _ _ _ _ _ _

        // Check columns before start
        assert_eq!(grid.get_tuple(3, ROW_INDEX).0, SPACE, "SPACE before hline");
        assert_eq!(
            grid.get_tuple(3, ROW_INDEX).1,
            DEF_COL,
            "default colour before hline"
        );
        assert_eq!(
            grid.get_tuple(3, ROW_INDEX).2,
            DEF_PERS,
            "default persistence before hline"
        );

        // Merge: column 4 (start). Should be VER_R.
        assert_eq!(grid.get_tuple(4, ROW_INDEX).0, VER_R, "VER_R at hline 'to'");
        assert_eq!(
            grid.get_tuple(4, ROW_INDEX).1,
            10,
            "unchanged color at hline 'to'"
        );
        assert_eq!(
            grid.get_tuple(4, ROW_INDEX).2,
            10,
            "unchanged pers at hline 'to'"
        );

        // Merge (column 5): ARR_l
        assert_eq!(
            grid.get_tuple(5, ROW_INDEX).0,
            ARR_L,
            "ARR_L before hline 'to'"
        );
        assert_eq!(
            grid.get_tuple(5, ROW_INDEX).1,
            LINE_COLOR,
            "line_color in hline"
        );
        assert_eq!(
            grid.get_tuple(5, ROW_INDEX).2,
            LINE_PERS,
            "line_pers in hline"
        );

        // Range (columns 5, 6): HOR
        assert_eq!(grid.get_tuple(6, ROW_INDEX).0, HOR, "HOR in hline");
        assert_eq!(
            grid.get_tuple(6, ROW_INDEX).1,
            LINE_COLOR,
            "line_color in hline"
        );
        assert_eq!(
            grid.get_tuple(6, ROW_INDEX).2,
            LINE_PERS,
            "line_pers in hline"
        );

        assert_eq!(grid.get_tuple(7, ROW_INDEX).0, HOR, "HOR in hline");
        assert_eq!(
            grid.get_tuple(7, ROW_INDEX).1,
            LINE_COLOR,
            "line_color in hline"
        );
        assert_eq!(
            grid.get_tuple(7, ROW_INDEX).2,
            LINE_PERS,
            "line_pers in hline"
        );

        // Cell 8 (end/from): HOR_D
        assert_eq!(
            grid.get_tuple(8, ROW_INDEX).0,
            HOR_D,
            "HOR_D at hline 'from'"
        );
        assert_eq!(
            grid.get_tuple(8, ROW_INDEX).1,
            LINE_COLOR,
            "line_color at hline 'from'"
        );
        assert_eq!(
            grid.get_tuple(8, ROW_INDEX).2,
            LINE_PERS,
            "line_pers at hline 'from'"
        );
    }

    /// Case 4: Forward draw, with merge, onto a crossing symbol
    #[test]
    fn hline_forward_merge() {
        let merge = true;
        let (width, height) = (7, 3);
        let mut grid = Grid::new(width, height, DEFAULT_CELL);
        grid.set(5, ROW_INDEX, R_U, 10, 10); // Set a symbol that changes range
        grid.set(6, ROW_INDEX, VER, 11, 10); // Set symbol for merge target

        // Graph column: 0   1   2   3
        // Grid columns: 0 1 2 3 4 5 6
        // Grid row 0:   _ _ _ _ _ _ _
        // Grid row 1:   _ _ _ _ _ └ │
        // Grid row 2:   _ _ _ _ _ _ _

        let from_idx = 1;
        let to_idx = 3;
        // Start: 2, End: 6.
        // Index: 0 1 2 3 4 5   6
        // Cell:  - - F - - R_U T
        // Range: 3..6. Columns: 3, 4, 5.
        // Column 5: R_U -> HOR_D (in update_range)
        // Merge: column = end - 1 = 5. Symbol = ARR_R. Overwrites HOR_D.
        // Ends: 2 (forward), 6 (forward).

        super::hline(
            &mut grid,
            ROW_INDEX,
            (from_idx, to_idx),
            merge,
            LINE_COLOR,
            LINE_PERS,
        );
        // Graph column: 0   1   2   3
        // Grid columns: 0 1 2 3 4 5 6
        // Grid row 0:   _ _ _ _ _ _ _
        // Grid row 1:   _ _(╭ ─ ─ > ┤)
        // Grid row 2:   _ _ _ _ _ _ _

        // Start (column 2): R_D
        assert_eq!(grid.get_tuple(2, ROW_INDEX).0, R_D);
        assert_eq!(grid.get_tuple(2, ROW_INDEX).1, LINE_COLOR);
        assert_eq!(grid.get_tuple(2, ROW_INDEX).2, LINE_PERS);

        // Range (column 3, 4): HOR
        assert_eq!(grid.get_tuple(3, ROW_INDEX).0, HOR);
        assert_eq!(grid.get_tuple(3, ROW_INDEX).1, LINE_COLOR);
        assert_eq!(grid.get_tuple(3, ROW_INDEX).2, LINE_PERS);

        assert_eq!(grid.get_tuple(4, ROW_INDEX).0, HOR);
        assert_eq!(grid.get_tuple(4, ROW_INDEX).1, LINE_COLOR);
        assert_eq!(grid.get_tuple(4, ROW_INDEX).2, LINE_PERS);

        // Merge column (end - 1 = 5): ARR_R (Merge overwrites update_range)
        assert_eq!(grid.get_tuple(5, ROW_INDEX).0, ARR_R);
        assert_eq!(grid.get_tuple(5, ROW_INDEX).1, LINE_COLOR);
        assert_eq!(grid.get_tuple(5, ROW_INDEX).2, LINE_PERS);

        // End (column 6): VER_L
        assert_eq!(grid.get_tuple(6, ROW_INDEX).0, VER_L);
        assert_eq!(grid.get_tuple(6, ROW_INDEX).1, 11);
        assert_eq!(grid.get_tuple(6, ROW_INDEX).2, 10);
    }
}
