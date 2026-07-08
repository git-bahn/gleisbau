This section describes the algorithm introduced by the 0.7.x series

# Overview

The goal of Gleisbau is to present visually pleasing graphs of git
repositories. It is based on the idea that each commit should be assigned
a single track, which then forms a vertical line. These tracks are then
sorted and coloured according to user configuration.

The term track is used to avoid the confusion found in the term branch.
In git a branch means a label on a single commit. One commit can have
several branch labels. A commit can also have a tag label. All labels
can be moved around. When people discuss the structure of a git
repository they also use the term branch, but in this case it means
sequence of commits, starting at a fork point.

## Terms
Lets define some terms, first for graph topology:

- *git graph*: A directed graph where each node represents a commit and
  each edge points from child to parent.
- *merge commit*:
  A commit with more than one parent.
- *fork commit*:
  A commit with more than one child.
- *primary parent*:
  The first parent of a commit. Git orders parents.
- *head commit*:
  A commit where no other commit has it as primary parent.
- *track*:
  A path in a git graph where every edge in the path is to a primary parent.
- *track start*:
  The commit in a track that has no child in the track.
- *track end*:
  The commit in a track that has no parent in the track.
- *track merge target*:
  A specific child of track start chosen as the one that merged the track.
- *track fork source*:
  The primary parent to the track end.
- *track map*:
  A covering of a git graph with tracks, such that every commit belongs
  to exactly one track.

Terms for geometry:

- *layout*:
  An illustration of a graph. Two different layouts can represent the
  same graph, but two graphs with different topology will always have
  different layouts. Note that some special handling is done if the
  layout-graph is a subset of a larger graph.
- *label*:
  Gleisbau refers to the movable references to commits in git as labels.
  This covers branch labels as well as tag labels.
- *branch visualisation*: (TODO rename -> track visualisation)
  A Track is presented geometrically as vertical line with some colour.
- *line*:
  An edge is represented as a line. Lines between two commits in the same
  track are vertical, the rest are mostly horizontal but it is possible
  for two tracks A and B to link vertical if the edge is between track A end
  and track B start.

Terms for algorithm:

- *Build tracks*:
  The algorithm that assigns commits to tracks.
- *Layout tracks*:
  The algorithm that produces a [TrackLayout](crate::layout::TrackLayout)
  from a [TrackMap](crate::track::TrackMap)

- *track persistence*: (topology)
  Each track is assigned a persistence order, represented by a number.
  It is used by the algorithm that assigns commits to tracks. If two tracks
  has the same parent, the parent is assigned to the track with the lowest
  preceedence number.
- *Abstract Grid*: (layout)
  A grid where each row represents exactly one commit, and each track
  occupies a line in exactly one column.
- *Terminal Grid*: (layout)
  When making a layout for terminal usage, the narrow characters make it
  more visually pleasing if only every second column is used for lines -
  thus double width relative to the abstract grid. The horizontal lines
  should be kept apart if they link unrelated commits, so sometimes we
  need to insert one extra line per commit.

## Algorithm

### When to start a track/branch?
A commit with no children is a head commit. This starts a branch.

A commit merged into another commit is a head commit, but only if it is not
continued (used as primary parent)
It is possible for a head commit to be merged into multiple other commits.
The strongest suggestion is used for the new branch.

A non-head commit can start a branch if the merge identifies it as a higher
persistence branch than provided via primary parent relations.

A branch starts if explicitly started by the user. Via a branch label or
a tag label. Only the most persistent user branch at a commit is used to
start the branch.
A branch may start implicitly at secondary parents of a merge commit.
This does not happen, if a more persistent branch can claim the parent.


### Which branch does a commit A belong to?
The following canddidate branches are considered:
- (explicit) User specified labels
- (continuation) Any commit B that has A as primary parent provide a canddiate branch
- (derived) Any commit B that has A as non-primary parent is a merge commit. 
  This can generate branch candidates from the merge message.
