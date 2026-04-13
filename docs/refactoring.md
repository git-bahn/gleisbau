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