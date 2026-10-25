# renrs

Three small commands for renaming filesystem entries within their current
parent directory. They refuse to overwrite an existing destination.

- `ren PATH NAME` changes the entry's leaf name.
- `unren PATH NAME` reverses that rename: it renames `NAME` beside `PATH` back to
  `PATH`'s leaf name.
- `renx` changes a name using an operation such as append, prepend, or replacement.

## Why renrs?

Renaming should change an entry's name without changing where it lives. `renrs`
provides dedicated commands for renaming files, directories, and symlinks within
their current directory. They never:

- Move an entry to another directory.
- Interpret the new name as a directory to move into.
- Copy file contents, including as a fallback.
- Overwrite an existing destination.

For example, `ren report.txt archive` renames the file to `archive`. If `archive`
already exists—even as a directory—the command fails.

These commands use the [renamest](https://github.com/mevanlc/renamest) library for
native, cross-platform atomic† no-overwrite renames. Unsupported operations fail
rather than falling back to a separate existence check followed by an ordinary
rename.

† On Windows, renamest uses `MoveFileExW` without replacement or copy flags.
Simple renames are [reported to be atomic on NTFS](https://stackoverflow.com/questions/167414/is-an-atomic-file-rename-with-overwrite-possible-on-windows),
but [Microsoft's API documentation](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
does not explicitly guarantee atomicity.

## Install

From a checkout:

```sh
cargo install --path . --locked
```

After the first release, install all three commands with
`cargo install renrs --locked`, or download a
[release archive](https://github.com/mevanlc/renrs/releases).
Archives cover Linux, macOS, Windows, and FreeBSD (x64/ARM64), plus Android ARM64.
FreeBSD requires 15.1 or newer. Put the three executables on your PATH.

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

On Unix, `ren . new-name` renames the current directory within its parent.
Only the literal `.` argument has this behavior; it is not supported by `renx`,
`unren`, or on Windows. Filesystem roots are rejected, and other filesystem
restrictions (such as mount-point rename restrictions) are enforced by the OS.
The command refuses paths containing symlinks, including in the shell's `PWD`,
and refuses an invalid, stale, or unverifiable `PWD`. If `PWD` is unset, only the
physical working-directory path can be checked; prior symlink traversal cannot
be detected. After a successful rename, the shell's `PWD` and prompt may still
show the old name; `cd "$(pwd -P)"` refreshes the shell's path.

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

PRs and pushes to `main` build all nine targets and verify the crate package.
Tags such as `v0.1.0` supply release versions; checkout versions stay at `0.0.0`.
See the [release guide](https://github.com/mevanlc/renrs/blob/main/RELEASING.md) for
first-publication setup, trusted publishing, download verification, and recovery.

## License

[MIT](LICENSE). Renaming uses [renamest](https://crates.io/crates/renamest),
licensed under MIT or Apache-2.0. Release archives include its license notices.
