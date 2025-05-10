# cmdy

A simple CLI tool to manage, select, and execute predefined shell command snippets.

## Features

- Define command snippets in TOML files and organize them in directories
- Interactive fuzzy search using [fzf] (with ANSI colors, reversed layout, rounded border, 50% height by default)
- Filter snippets by tags
- Run snippets directly from the selection menu
- `edit` subcommand: open a snippet's source file in your `$EDITOR`
- `clip` subcommand: copy a snippet's command to the system clipboard

## Install

Requires Rust and [cargo].

```sh
cargo install cmdy
```

Alternatively, download a release binary from the project [releases page].

## Quickstart

1. Create a snippets directory (default location is `$XDG_CONFIG_HOME/cmdy/commands` or `~/.config/cmdy/commands`).
2. Add one or more `.toml` files with snippet definitions (see _Defining Snippets_ below).
3. Run:
   ```sh
   cmdy
   ```
4. Type to filter, select a snippet, and press Enter to execute it.

### Subcommands

- `cmdy edit` — Launch your `$EDITOR` on the selected snippet's TOML file.
- `cmdy clip` — Copy the selected snippet's `command` string to your clipboard, then exit.

### Flags

- `--dir <DIRECTORY>` — Specify a custom snippets directory (overrides default).
- `-t, --tag <TAG>` — Show only snippets tagged with `<TAG>` (can be repeated).
- `-s, --search <SEARCH>` — Pre-populate the initial filter query for the interactive selector.

## Configuration

Global settings are loaded from `cmdy.toml` in your config directory (`$XDG_CONFIG_HOME/cmdy/cmdy.toml` or `~/.config/cmdy/cmdy.toml`).

Example `cmdy.toml`:

```toml
# Command used for interactive filtering
filter_command = "fzf --ansi --layout=reverse --border=rounded --height=50%"

# Additional snippet directories to scan
directories = ["/path/to/other/snippets"]
```

## Defining Snippets

Create one or more `.toml` files in your snippets directory, using the `[[commands]]` table:

```toml
[[commands]]
# human-readable name shown in the menu
description = "List all files with details"
# the shell command to run
command     = "ls -lAh --color=auto"
# optional tags, used for filtering
tags        = ["files", "list"]

[[commands]]
description = "Show current date and time"
command     = "date '+%Y-%m-%d %H:%M:%S'"
```

Files without valid `[[commands]]` tables are skipped, and duplicate `description` names across files will error.

## Development

Clone the repo and build:

```sh
git clone https://github.com/yourusername/cmdy.git
cd cmdy
cargo build
cargo test
```

Ensure you have [fzf] installed in your `PATH` for interactive selection.

## License

Distributed under the MIT License. See [LICENSE] for details.

[fzf]: https://github.com/junegunn/fzf
[cargo]: https://doc.rust-lang.org/cargo/
[releases page]: https://github.com/yourusername/cmdy/releases
[LICENSE]: LICENSE