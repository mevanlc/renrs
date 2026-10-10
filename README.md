# renrs

Three small commands for renaming filesystem entries within their current
parent directory. They refuse to overwrite an existing destination.

- `ren PATH NAME` changes the entry's leaf name.
- `unren PATH NAME` reverses that rename: it renames `NAME` beside `PATH` back to
  `PATH`'s leaf name.
- `renx` changes a name using an operation such as append, prepend, or replacement.

## Install

From a checkout:

```sh
cargo install --path . --locked
```

After the first release, install all three commands with
`cargo install renrs --locked`, or download a
[release archive](https://github.com/mevanlc/renrs/releases).
Archives cover Linux, macOS, and Windows (x64/ARM64), plus Android ARM64. Put the
three executables on your PATH.

## Usage

```sh
ren reports/old.txt new.txt
unren reports/old.txt new.txt
renx --append .bak reports/old.txt
renx --prepend draft- reports/old.txt
renx --ext md reports/old.txt
renx --stem new reports/old.txt
renx --name new.md reports/old.txt
renx --sr-regex=ig -s 'old' -r 'new' reports/old.txt
renx --sr-fixed=2-3 -s 'old' -r 'new' reports/old-old-old.txt
```

`--append` and `--prepend` operate on the complete basename. `--stem` preserves
its suffix from the last dot and requires a dot; `--ext` replaces the extension,
or appends one when there is no dot. `--name` replaces the complete basename.
Supply exactly one `renx` operation. Names cannot contain path components.

Search and replace requires `-s SEARCH` and `-r REPLACEMENT`. Both modes replace
the first match by default. Optional mode settings must use `=`:

- `--sr-regex[=i|g|ig|gi]`: Rust regular expressions; `i` ignores case, `g` replaces
  all matches, and replacements support captures such as `$1`.
- `--sr-fixed[=OPTIONS]`: literal search; use `g` for all matches, or one-based
  occurrence ranges such as `2,4-6,8-`. Add case-insensitivity with `i:RANGES` or
  `RANGES:i`; `ig` and `gi` replace all matches ignoring case.

All commands accept repeated `-t, --type SPEC` filters; an entry must match at
least one supplied type: file (`f`), directory (`d`/`dir`), symlink (`l`),
executable (`x`), empty (`e`), socket (`s`), pipe (`p`), character device (`c`),
or block device (`b`). Run any command with `--help` for its full options or
`--version` for its version.

## Development and releases

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo nextest run --locked --all-targets --all-features
uv run scripts/test_release.py
actionlint
```

PRs and pushes to `main` build all seven targets and verify the crate package.
Tags such as `v0.1.0` supply release versions; checkout versions stay at `0.0.0`.
See the [release guide](https://github.com/mevanlc/renrs/blob/main/RELEASING.md) for
first-publication setup, trusted publishing, download verification, and recovery.

## License

[MIT](LICENSE). Renaming uses [renamest](https://crates.io/crates/renamest),
licensed under MIT or Apache-2.0. Release archives include its license notices.
