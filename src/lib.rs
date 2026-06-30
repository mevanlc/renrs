use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use clap::{Args as ClapArgs, Parser, ValueEnum};
use faccess::PathExt;

#[derive(Debug, Parser)]
#[command(about = "Rename a filesystem entry without moving it")]
pub struct RenameArgs {
    #[command(flatten)]
    filters: TypeFilterArgs,

    /// Filesystem entry to rename.
    path: PathBuf,

    /// New leaf name for PATH.
    name: OsString,
}

#[derive(Debug, Parser)]
#[command(about = "Rename the stem of a filesystem entry without moving it")]
pub struct StemArgs {
    #[command(flatten)]
    filters: TypeFilterArgs,

    /// Filesystem entry to rename.
    path: PathBuf,

    /// New stem for PATH, preserving the suffix after its last dot.
    stem: OsString,
}

#[derive(Debug, Parser)]
#[command(about = "Rename the extension of a filesystem entry without moving it")]
pub struct ExtArgs {
    #[command(flatten)]
    filters: TypeFilterArgs,

    /// Filesystem entry to rename.
    path: PathBuf,

    /// New extension for PATH, preserving the prefix through its last dot.
    extension: OsString,
}

#[derive(Debug, ClapArgs)]
struct TypeFilterArgs {
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

pub fn run(args: RenameArgs) -> Result<(), String> {
    rename_entry(args.path, args.filters.filetype, |source| {
        rename_target(source, &args.name)
    })
}

pub fn run_stem(args: StemArgs) -> Result<(), String> {
    rename_entry(args.path, args.filters.filetype, |source| {
        stem_rename_target(source, &args.stem)
    })
}

pub fn run_ext(args: ExtArgs) -> Result<(), String> {
    rename_entry(args.path, args.filters.filetype, |source| {
        ext_rename_target(source, &args.extension)
    })
}

fn rename_entry<F>(
    source: PathBuf,
    filetype: Option<Vec<FileType>>,
    make_target: F,
) -> Result<(), String>
where
    F: FnOnce(&Path) -> Result<PathBuf, String>,
{
    let metadata = fs::symlink_metadata(&source)
        .map_err(|err| format!("cannot read '{}': {err}", source.display()))?;

    if let Some(filetypes) = filetype {
        let filetypes = FileTypes::from_specs(&filetypes);
        if !filetypes.matches(&source, &metadata) {
            return Err(format!(
                "refusing to rename '{}': type does not match supplied --type filter",
                source.display()
            ));
        }
    }

    let target = make_target(&source)?;
    rename_without_overwrite(&source, &target)
}

fn rename_target(source: &Path, name: &OsString) -> Result<PathBuf, String> {
    ensure_source_has_file_name(source)?;
    ensure_leaf_name(source, name, "new name")?;

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(name))
}

fn stem_rename_target(source: &Path, stem: &OsString) -> Result<PathBuf, String> {
    let file_name = ensure_source_has_file_name(source)?;
    ensure_leaf_name(source, stem, "new stem")?;

    let suffix = suffix_from_last_dot(file_name).ok_or_else(|| {
        format!(
            "refusing to rename '{}': file name has no '.'",
            source.display()
        )
    })?;
    let mut target_name = stem.clone();
    target_name.push(suffix);

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(target_name))
}

fn ext_rename_target(source: &Path, extension: &OsString) -> Result<PathBuf, String> {
    let file_name = ensure_source_has_file_name(source)?;
    ensure_leaf_name(source, extension, "new extension")?;

    let prefix = prefix_through_last_dot(file_name).ok_or_else(|| {
        format!(
            "refusing to rename '{}': file name has no '.'",
            source.display()
        )
    })?;
    let mut target_name = prefix;
    target_name.push(extension);

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(target_name))
}

fn ensure_source_has_file_name(source: &Path) -> Result<&OsStr, String> {
    source.file_name().ok_or_else(|| {
        format!(
            "refusing to rename '{}': path has no file name",
            source.display()
        )
    })
}

fn ensure_leaf_name(source: &Path, name: &OsString, label: &str) -> Result<(), String> {
    let name = Path::new(name);
    if !is_leaf_name(name) {
        return Err(format!(
            "refusing to rename '{}': {label} must not contain path components",
            source.display()
        ));
    }

    Ok(())
}

fn is_leaf_name(name: &Path) -> bool {
    let mut components = name.components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn rename_without_overwrite(source: &Path, target: &Path) -> Result<(), String> {
    renamore::rename_exclusive(source, target).map_err(|err| {
        if err.kind() == ErrorKind::AlreadyExists {
            format!(
                "refusing to rename: target '{}' already exists",
                target.display()
            )
        } else {
            format!(
                "cannot rename '{}' to '{}': {err}",
                source.display(),
                target.display()
            )
        }
    })
}

#[cfg(any(unix, target_os = "redox"))]
fn suffix_from_last_dot(name: &OsStr) -> Option<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = name.as_bytes();
    let dot = bytes.iter().rposition(|byte| *byte == b'.')?;
    Some(OsString::from_vec(bytes[dot..].to_vec()))
}

#[cfg(any(unix, target_os = "redox"))]
fn prefix_through_last_dot(name: &OsStr) -> Option<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = name.as_bytes();
    let dot = bytes.iter().rposition(|byte| *byte == b'.')?;
    Some(OsString::from_vec(bytes[..=dot].to_vec()))
}

#[cfg(windows)]
fn suffix_from_last_dot(name: &OsStr) -> Option<OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let code_units: Vec<u16> = name.encode_wide().collect();
    let dot = code_units.iter().rposition(|unit| *unit == b'.' as u16)?;
    Some(OsString::from_wide(&code_units[dot..]))
}

#[cfg(windows)]
fn prefix_through_last_dot(name: &OsStr) -> Option<OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let code_units: Vec<u16> = name.encode_wide().collect();
    let dot = code_units.iter().rposition(|unit| *unit == b'.' as u16)?;
    Some(OsString::from_wide(&code_units[..=dot]))
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

        run(rename_args(&old, "new", None)).unwrap();

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

        let err = run(rename_args(&old, "new", None)).unwrap_err();

        assert!(err.contains("already exists"));
        assert_eq!(fs::read_to_string(old).unwrap(), "old");
        assert_eq!(fs::read_to_string(new).unwrap(), "new");
    }

    #[test]
    fn refuses_name_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run(rename_args(&old, "subdir/new", None)).unwrap_err();

        assert!(err.contains("new name must not contain path components"));
        assert!(old.exists());
    }

    #[test]
    fn accepts_matching_type_filter() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "contents").unwrap();

        run(rename_args(&old, "new", Some(vec![FileType::File]))).unwrap();

        assert!(new.exists());
    }

    #[test]
    fn refuses_non_matching_type_filter() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run(rename_args(&old, "new", Some(vec![FileType::Directory]))).unwrap_err();

        assert!(err.contains("type does not match"));
        assert!(old.exists());
    }

    #[test]
    fn empty_filter_matches_empty_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "").unwrap();

        run(rename_args(&old, "new", Some(vec![FileType::Empty]))).unwrap();

        assert!(new.exists());
    }

    #[test]
    fn stem_renames_only_text_before_last_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("archive.tar.gz");
        let new = tempdir.path().join("renamed.gz");
        fs::write(&old, "contents").unwrap();

        run_stem(stem_args(&old, "renamed", None)).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn stem_refuses_file_name_without_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run_stem(stem_args(&old, "new", None)).unwrap_err();

        assert!(err.contains("file name has no '.'"));
        assert!(old.exists());
    }

    #[test]
    fn stem_refuses_stem_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old.txt");
        fs::write(&old, "contents").unwrap();

        let err = run_stem(stem_args(&old, "subdir/new", None)).unwrap_err();

        assert!(err.contains("new stem must not contain path components"));
        assert!(old.exists());
    }

    #[test]
    fn ext_renames_only_text_after_last_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("archive.tar.gz");
        let new = tempdir.path().join("archive.tar.xz");
        fs::write(&old, "contents").unwrap();

        run_ext(ext_args(&old, "xz", None)).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn ext_refuses_file_name_without_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run_ext(ext_args(&old, "txt", None)).unwrap_err();

        assert!(err.contains("file name has no '.'"));
        assert!(old.exists());
    }

    #[test]
    fn ext_refuses_extension_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old.txt");
        fs::write(&old, "contents").unwrap();

        let err = run_ext(ext_args(&old, "subdir/new", None)).unwrap_err();

        assert!(err.contains("new extension must not contain path components"));
        assert!(old.exists());
    }

    #[test]
    fn parses_type_filter_multiple_times() {
        let args =
            RenameArgs::try_parse_from(["ren", "-tf", "--type", "dir", "old", "new"]).unwrap();

        assert_eq!(
            args.filters.filetype,
            Some(vec![FileType::File, FileType::Directory])
        );
    }

    #[test]
    fn rejects_compact_multiple_type_specs() {
        let err = RenameArgs::try_parse_from(["ren", "-tfd", "old", "new"]).unwrap_err();

        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    fn rename_args(path: &Path, name: &str, filetype: Option<Vec<FileType>>) -> RenameArgs {
        RenameArgs {
            filters: TypeFilterArgs { filetype },
            path: path.to_path_buf(),
            name: OsString::from(name),
        }
    }

    fn stem_args(path: &Path, stem: &str, filetype: Option<Vec<FileType>>) -> StemArgs {
        StemArgs {
            filters: TypeFilterArgs { filetype },
            path: path.to_path_buf(),
            stem: OsString::from(stem),
        }
    }

    fn ext_args(path: &Path, extension: &str, filetype: Option<Vec<FileType>>) -> ExtArgs {
        ExtArgs {
            filters: TypeFilterArgs { filetype },
            path: path.to_path_buf(),
            extension: OsString::from(extension),
        }
    }
}
