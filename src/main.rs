use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use faccess::PathExt;

#[derive(Debug, Parser)]
#[command(about = "Rename a filesystem entry without moving it")]
struct Args {
    /// Only rename PATH if its type matches one of the supplied type specs.
    #[arg(
        long = "type",
        short = 't',
        value_name = "tspec",
        hide_possible_values = true,
        value_enum,
        help = "Require type: file (f), directory (d/dir), symlink (l), executable (x), empty (e), socket (s), pipe (p), char-device (c), block-device (b)"
    )]
    filetype: Option<Vec<FileType>>,

    /// Filesystem entry to rename.
    path: PathBuf,

    /// New leaf name for PATH.
    name: OsString,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum FileType {
    #[value(alias = "f")]
    File,
    #[value(alias = "d", alias = "dir")]
    Directory,
    #[value(alias = "l")]
    Symlink,
    #[value(alias = "b")]
    BlockDevice,
    #[value(alias = "c")]
    CharDevice,
    /// A file which is executable by the current effective user
    #[value(alias = "x")]
    Executable,
    #[value(alias = "e")]
    Empty,
    #[value(alias = "s")]
    Socket,
    #[value(alias = "p")]
    Pipe,
}

#[derive(Default)]
struct FileTypes {
    files: bool,
    directories: bool,
    symlinks: bool,
    block_devices: bool,
    char_devices: bool,
    sockets: bool,
    pipes: bool,
    executables_only: bool,
    empty_only: bool,
}

impl FileTypes {
    fn from_specs(values: &[FileType]) -> Self {
        let mut file_types = Self::default();

        for value in values {
            match value {
                FileType::File => file_types.files = true,
                FileType::Directory => file_types.directories = true,
                FileType::Symlink => file_types.symlinks = true,
                FileType::Executable => {
                    file_types.executables_only = true;
                    file_types.files = true;
                }
                FileType::Empty => file_types.empty_only = true,
                FileType::BlockDevice => file_types.block_devices = true,
                FileType::CharDevice => file_types.char_devices = true,
                FileType::Socket => file_types.sockets = true,
                FileType::Pipe => file_types.pipes = true,
            }
        }

        if file_types.empty_only && !(file_types.files || file_types.directories) {
            file_types.files = true;
            file_types.directories = true;
        }

        file_types
    }

    fn matches(&self, path: &Path, metadata: &fs::Metadata) -> bool {
        let file_type = metadata.file_type();

        ((self.files && file_type.is_file())
            || (self.directories && file_type.is_dir())
            || (self.symlinks && file_type.is_symlink())
            || (self.block_devices && is_block_device(file_type))
            || (self.char_devices && is_char_device(file_type))
            || (self.sockets && is_socket(file_type))
            || (self.pipes && is_pipe(file_type)))
            && (!self.executables_only || path.executable())
            && (!self.empty_only || is_empty(path, metadata))
    }
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ren: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), String> {
    let source = args.path;
    let metadata = fs::symlink_metadata(&source)
        .map_err(|err| format!("cannot read '{}': {err}", source.display()))?;

    if let Some(filetypes) = args.filetype {
        let filetypes = FileTypes::from_specs(&filetypes);
        if !filetypes.matches(&source, &metadata) {
            return Err(format!(
                "refusing to rename '{}': type does not match supplied --type filter",
                source.display()
            ));
        }
    }

    let target = rename_target(&source, &args.name)?;
    ensure_missing(&target)?;

    fs::rename(&source, &target).map_err(|err| {
        format!(
            "cannot rename '{}' to '{}': {err}",
            source.display(),
            target.display()
        )
    })
}

fn rename_target(source: &Path, name: &OsString) -> Result<PathBuf, String> {
    if source.file_name().is_none() {
        return Err(format!(
            "refusing to rename '{}': path has no file name",
            source.display()
        ));
    }

    let name = Path::new(name);
    if !is_leaf_name(name) {
        return Err(format!(
            "refusing to rename '{}': new name must not contain path components",
            source.display()
        ));
    }

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(name))
}

fn is_leaf_name(name: &Path) -> bool {
    let mut components = name.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn ensure_missing(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(format!(
            "refusing to rename: target '{}' already exists",
            path.display()
        )),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("cannot check target '{}': {err}", path.display())),
    }
}

fn is_empty(path: &Path, metadata: &fs::Metadata) -> bool {
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
    } else if file_type.is_file() {
        metadata.len() == 0
    } else {
        false
    }
}

#[cfg(any(unix, target_os = "redox"))]
fn is_block_device(ft: fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;

    ft.is_block_device()
}

#[cfg(windows)]
fn is_block_device(_: fs::FileType) -> bool {
    false
}

#[cfg(any(unix, target_os = "redox"))]
fn is_char_device(ft: fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;

    ft.is_char_device()
}

#[cfg(windows)]
fn is_char_device(_: fs::FileType) -> bool {
    false
}

#[cfg(any(unix, target_os = "redox"))]
fn is_socket(ft: fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;

    ft.is_socket()
}

#[cfg(windows)]
fn is_socket(_: fs::FileType) -> bool {
    false
}

#[cfg(any(unix, target_os = "redox"))]
fn is_pipe(ft: fs::FileType) -> bool {
    use std::os::unix::fs::FileTypeExt;

    ft.is_fifo()
}

#[cfg(windows)]
fn is_pipe(_: fs::FileType) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_file_within_same_parent() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "contents").unwrap();

        run(args(&old, "new", None)).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn refuses_to_overwrite_existing_entry() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "old").unwrap();
        fs::write(&new, "new").unwrap();

        let err = run(args(&old, "new", None)).unwrap_err();

        assert!(err.contains("already exists"));
        assert_eq!(fs::read_to_string(old).unwrap(), "old");
        assert_eq!(fs::read_to_string(new).unwrap(), "new");
    }

    #[test]
    fn refuses_name_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run(args(&old, "subdir/new", None)).unwrap_err();

        assert!(err.contains("new name must not contain path components"));
        assert!(old.exists());
    }

    #[test]
    fn accepts_matching_type_filter() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "contents").unwrap();

        run(args(&old, "new", Some(vec![FileType::File]))).unwrap();

        assert!(new.exists());
    }

    #[test]
    fn refuses_non_matching_type_filter() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run(args(&old, "new", Some(vec![FileType::Directory]))).unwrap_err();

        assert!(err.contains("type does not match"));
        assert!(old.exists());
    }

    #[test]
    fn empty_filter_matches_empty_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "").unwrap();

        run(args(&old, "new", Some(vec![FileType::Empty]))).unwrap();

        assert!(new.exists());
    }

    #[test]
    fn parses_type_filter_multiple_times() {
        let args = Args::try_parse_from(["ren", "-tf", "--type", "dir", "old", "new"]).unwrap();

        assert_eq!(
            args.filetype,
            Some(vec![FileType::File, FileType::Directory])
        );
    }

    #[test]
    fn rejects_compact_multiple_type_specs() {
        let err = Args::try_parse_from(["ren", "-tfd", "old", "new"]).unwrap_err();

        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    fn args(path: &Path, name: &str, filetype: Option<Vec<FileType>>) -> Args {
        Args {
            filetype,
            path: path.to_path_buf(),
            name: OsString::from(name),
        }
    }
}
