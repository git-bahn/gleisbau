# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.7.1] - 2026-02-09

### Added

- A CHANGELOG.md file.

### Changed

- Extract library to its own crate named "gleisbau".

### Removed

- git-graph main.rs and dependencies.
- Instructions related to the command line tool git-graph.


## [0.7.0] - 2025-11-14

Last release where library is part of git-graph.

### Added

- (BREAKING) graph::get_repo, add argument skip_repo_owner_validation
  false gives the previous behaviour.
- (BREAKING) GitGraph::new, add argument start_point to control where
  traversal should start.
  Set to None to get the previous behaviour.
  
- Lots of API docs
- "trunk" as supported main branch name

### Changed

- Update git2 dependency to version 0.20

### Removed

- (BREAKING) GitGraph public fields "tags" and "branches"


## [0.6.0] - 2024-05-24

### Added

- Reverse order option
