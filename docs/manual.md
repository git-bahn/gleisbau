# Gleisbau manual

**Content**

* [Overview](#overview)
* [Formatting](#formatting)
* [Custom branching models](#custom-branching-models)

## Overview


**Branching models**

TODO write about branching model, and per repository configuration
TODO write about default branching models
TODO write about custom branching model

For **defining your own models**, see section [Custom branching models](#custom-branching-models).

**Styles**

TODO write about rendering styles.
Besides the default `normal` (alias `thin`), supported styles are `round`, `bold`, `double` and `ascii`.:


![styles](https://user-images.githubusercontent.com/44003176/103467621-357ce780-4d51-11eb-8ff9-dd7be8b40f84.png)

Style `ascii` can be used for devices and media that do not support Unicode/UTF-8 characters. 

**Formatting**

Gleisbau supports predefined as well as custom commit formatting.
Available presets follow Git: `oneline` (the default), `short`, `medium` and `full`. For details and custom formatting, see section [Formatting](#formatting).




## Custom branching models

Branching models are configured using the files in `APP_DATA/git-graph/models`. 

* Windows: `C:\Users\<user>\AppData\Roaming\git-graph`
* Linux: `~/.config/git-graph`
* OSX: `~/Library/Application Support/git-graph`

**Branching model files** are in [TOML](https://toml.io/en/) format and have several sections, relying on Regular Expressions to categorize branches. The listing below shows the `git-flow` model (slightly abbreviated) with explanatory comments.

```toml
# RegEx patterns for branch groups by persistence, from most persistent
# to most short-leved branches. This is used to back-trace branches.
# Branches not matching any pattern are assumed least persistent.
persistence = [
    '^(master|main|trunk)$', # Matches exactly `master` or `main`  or `trunk`
    '^(develop|dev)$',
    '^feature.*$',     # Matches everything starting with `feature`
    '^release.*$',
    '^hotfix.*$',
    '^bugfix.*$',
]

# RegEx patterns for visual ordering of branches, from left to right.
# Here, `master`, `main` or `trunk` are shown left-most, followed by branches
# starting with `hotfix` or `release`, followed by `develop` or `dev`.
# Branches not matching any pattern (e.g. starting with `feature`)
# are displayed further to the right.
order = [
    '^(master|main|trunk)$',      # Matches exactly `master` or `main` or `trunk`
    '^(hotfix|release).*$', # Matches everything starting with `hotfix` or `release`
    '^(develop|dev)$',      # Matches exactly `develop` or `dev`
]

# Colors of branches in terminal output. 
# For supported colors, see section Colors (below this listing).
[terminal_colors]
# Each entry is composed of a RegEx pattern and a list of colors that
# will be used alternating (see e.g. `feature...`).
matches = [
    [
        '^(master|main|trunk)$',
        ['bright_blue'],
    ],
    [
        '^(develop|dev)$',
        ['bright_yellow'],
    ],
    [   # Branches obviously merged in from forks are prefixed with 'fork/'. 
        # The 'fork/' prefix is only available in order and colors, but not in persistence!
        '^(feature|fork/).*$',
        ['bright_magenta', 'bright_cyan'], # Multiple colors for alternating use
    ],
        [
        '^release.*$',
        ['bright_green'],
    ],
        [
        '^(bugfix|hotfix).*$',
        ['bright_red'],
    ],
    [
        '^tags/.*$',
        ['bright_green'],
    ],
]
# A list of colors that are used (alternating) for all branches
# not matching any of the above pattern. 
unknown = ['white']

# Colors of branches in SVG output. 
# Same structure as terminal_colors. 
# For supported colors, see section Colors (below this listing).
[svg_colors]
matches = [
    [
        '^(master|main|trunk)$',
        ['blue'],
    ],
    [ 
        '...',
    ]
]
unknown = ['gray']
```

**Tags**

Internally, all tags start with `tag/`. To match Git tags, use RegEx patterns like `^tags/.*$`. However, only tags that are not on any branch are ordered and colored separately.

**Colors**

**Terminal colors** support the 8 system color names `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan` and `white`, as well as each of them prefixed with `bright_` (e.g. `bright_blue`).

Further, indices of the 256-color palette are supported. For a full list, see [here](https://jonasjacek.github.io/colors/). Indices must be quoted as strings (e.g. `'16'`)

**SVG colors** support all named web colors (full list [here](https://htmlcolorcodes.com/color-names/)), as well as RGB colors in hex notation, like `#ffffff`.
