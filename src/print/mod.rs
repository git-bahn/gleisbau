/*! Create visual representations of git graphs.

Printing is the final step that transforms a layout to the representation
where it will be used. This means colouring, exact placement, text wrapping
and similar issues are dealt with here.

The original git-graph had two print functions:

* [unicode::print_unicode] for printing to a terminal using fixed font and ansi colour codes.
  This is candidate for deprecation along with the commit format code.
* print_svg for printing to SVG XML for display in a browser or elsewhere.
  This has been kept in git-graph so not available from gleisbau.

The gleisbau library adds one new function:

* [unicode::print_graph_terminal] for printing only the graph to a terminal
*/

pub mod colors;
pub mod format;
pub mod grid;
pub mod label;
pub mod unicode;
