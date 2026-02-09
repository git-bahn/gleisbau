# gleisbau

[![Tests](https://github.com/git-bahn/gleisbau/actions/workflows/tests.yml/badge.svg)](https://github.com/git-bahn/gleisbau/actions/workflows/tests.yml)
[![GitHub](https://img.shields.io/badge/github-repo-blue?logo=github)](https://github.com/git-bahn/gleisbau)
[![Crate](https://img.shields.io/crates/v/gleisbau.svg)](https://crates.io/crates/gleisbau)
[![MIT license](https://img.shields.io/github/license/git-bahn/gleisbau)](https://github.com/git-bahn/gleisbau/blob/master/LICENSE)

A library to visualize Git history graphs in a comprehensible way, following different branching models.

## Features

* Structured graphs
* Render any small subgraph in less than 10 ms.
* Renders a 70k commit repo in 40 sec, using less than 1GB memory.
* Control layout via branching models
* Different styles, including ASCII-only (i.e. no "special characters")

## Usage

**For detailed information, see the [manual](docs/manual.md)**.

**For details on how to create your own branching models see the manual, section [Custom branching models](docs/manual.md#custom-branching-models).**

## Limitations

* Summaries of merge commits (i.e. 1st line of message) should not be modified! gleisbau needs them to categorize merged branches.
* Supports only the primary remote repository `origin`.
* Does currently not support "octopus merges" (i.e. no more than 2 parents)
* On Windows PowerShell, piping to file output does not work properly (changes encoding), so you may want to use the default Windows console instead

## Contributing

Please report any issues and feature requests in the [issue tracker](https://github.com/git-bahn/gleisbau/issues).

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.
