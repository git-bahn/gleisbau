# Refactoring for larger repositories

The 0.7.x series of gleisbau is used for a major refactoring.
The target goal is to have the library ready for usage in [gitui](https://github.com/gitui-org/gitui),
an interactive application with requirements for start-up time, 
memory usage, and rendering time. It must be possible to delegate
long running activities to a separate thread, and at the same time
render subsets of the graph in short time.

This can be achieved by splitting the library along some majour feature
lines:
- Traversing the graph to assign branch traces to commits
- Render a subset of the branch graph

Some final UI elements will be moved from the library to applications
(like git-graph), as they are less reusable.
- Printing of commit content
- Printing of SVG

All of this must be done while preserving some of [git-graph](https://github.com/git-bahn/git-graph)'s most noticable features:
- Configurable branch priority, ordering, and colouring
- Stable branch layout - the subset size should not affect how branches are rendered.

# Step: Introduce Builder

I need to split GitGraph into two parts: Mapping of tracks and layout of graph.
The GitGraph structure is used by applications as if it was the final render,
therefore it belongs with the layout. However, GitGraph::new does a full
mapping of tracks as well as layout.

This should be split into two steps, none controlled by GitGraph

- Tracking branches
- Layout of branches

The start of ths separation is to introduce the builder pattern. This is closer
to how the final code will work.

# Step: Split GitGraph

I need to split GitGraph data to get a different coupling.
This will be done so legacy code can still use GitGraph with only some
API changes.

Some are purely related to decoration, like a list of tags, branches for
a commit, as well as information about which commit is the current head.
In git-graph and git-igitt this was achieved via access to internal
structures in GitGraph. A similar set of data will be provided via
migration fuctions on GitGraph.

The new process must be able to produce topology data without any geometry
or presentation data. The first refactoring is to 
reverse the link between BranchInfo and BranchViz.

- BranchInfo will be built when traversing the graph, and must not know
  anything about visualization.
- BranchViz will be built repeatedly when rendering, from BranchInfo.

## AS-IS

GitGraph --> BranchInfo --> BranchVis

## TO-BE


TrackMap --> BranchInfo
GitGraph --> TrackMap
GitGraph --> BranchVis
GitGraph --> LabelMap
