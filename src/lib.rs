use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use clap::{Args as ClapArgs, Parser, ValueEnum};
use faccess::PathExt;
use regex::bytes::RegexBuilder;

// Compile the vendored fork into the package so crates.io installs use the same
// implementation as local builds. Its additional library APIs are unused here.
#[allow(dead_code)]
#[path = "../vendor/renamore/src/lib.rs"]
mod renamore;

#[derive(Debug, Parser)]
#[command(
    name = "ren",
    version,
    about = "Rename a filesystem entry without moving it"
)]
pub struct RenameArgs {
    #[command(flatten)]
    filters: TypeFilterArgs,

    /// Filesystem entry to rename.
    path: PathBuf,

    /// New leaf name for PATH.
    name: OsString,
}

#[derive(Debug, Parser)]
#[command(
    name = "unren",
    version,
    about = "Undo a rename without moving the filesystem entry"
)]
pub struct UnrenArgs {
    #[command(flatten)]
    filters: TypeFilterArgs,

    /// Filesystem entry to restore.
    path: PathBuf,

    /// Leaf name of the entry to rename back to PATH.
    name: OsString,
}

#[derive(Debug, Parser)]
#[command(
    name = "renx",
    version,
    about = "Rename a filesystem entry without moving it"
)]
#[command(group(
    clap::ArgGroup::new("search-operation")
        .args(["sr_regex", "sr_fixed"])
))]
pub struct RenxArgs {
    #[command(flatten)]
    filters: TypeFilterArgs,

    #[command(flatten)]
    operation: RenxOperationArgs,

    /// Search pattern or fixed string for a search-replace operation.
    #[arg(short = 's', value_name = "search", requires = "search-operation")]
    search: Option<OsString>,

    /// Replacement for a search-replace operation.
    #[arg(short = 'r', value_name = "replacement", requires = "search-operation")]
    replacement: Option<OsString>,

    /// Filesystem entry to rename.
    path: PathBuf,
}

#[derive(Debug, ClapArgs)]
#[group(id = "operation", required = true, multiple = false)]
struct RenxOperationArgs {
    /// Append STR to PATH's basename.
    #[arg(long, value_name = "str")]
    append: Option<OsString>,

    /// Prepend STR to PATH's basename.
    #[arg(long, value_name = "str")]
    prepend: Option<OsString>,

    /// Set PATH's extension, appending .STR if the basename has no dot.
    #[arg(long, value_name = "str")]
    ext: Option<OsString>,

    /// Replace PATH's stem, preserving the suffix after its last dot.
    #[arg(long, value_name = "str")]
    stem: Option<OsString>,

    /// Replace PATH's complete basename.
    #[arg(long, value_name = "str")]
    name: Option<OsString>,

    /// Search and replace using a Rust regular expression.
    #[arg(
        long,
        value_name = "sr-regex-opts",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "",
        requires_all = ["search", "replacement"]
    )]
    sr_regex: Option<RegexOptions>,

    /// Search and replace using a fixed string and optional occurrence ranges.
    #[arg(
        long,
        value_name = "sr-fixed-opts",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "",
        requires_all = ["search", "replacement"]
    )]
    sr_fixed: Option<FixedOptions>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RegexOptions {
    case_insensitive: bool,
    global: bool,
}

impl FromStr for RegexOptions {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "" => Ok(Self::default()),
            "i" => Ok(Self {
                case_insensitive: true,
                global: false,
            }),
            "g" => Ok(Self {
                case_insensitive: false,
                global: true,
            }),
            "ig" | "gi" => Ok(Self {
                case_insensitive: true,
                global: true,
            }),
            _ => Err("expected one of: i, g, ig, gi".to_owned()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FixedOptions {
    case_insensitive: bool,
    ranges: OccurrenceRanges,
}

impl Default for FixedOptions {
    fn default() -> Self {
        Self {
            case_insensitive: false,
            ranges: OccurrenceRanges::first(),
        }
    }
}

impl FromStr for FixedOptions {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Ok(Self::default());
        }

        if matches!(value, "ig" | "gi") {
            return Ok(Self {
                case_insensitive: true,
                ranges: OccurrenceRanges::All,
            });
        }

        let (case_insensitive, ranges) = if let Some(ranges) = value.strip_prefix("i:") {
            (true, ranges)
        } else if let Some(ranges) = value.strip_suffix(":i") {
            (true, ranges)
        } else {
            (false, value)
        };

        Ok(Self {
            case_insensitive,
            ranges: ranges.parse()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OccurrenceRanges {
    All,
    Selected(Vec<OccurrenceRange>),
}

impl OccurrenceRanges {
    fn first() -> Self {
        Self::Selected(vec![OccurrenceRange { start: 1, end: 1 }])
    }

    fn includes(&self, occurrence: usize) -> bool {
        match self {
            Self::All => true,
            Self::Selected(ranges) => ranges
                .iter()
                .any(|range| occurrence >= range.start && occurrence <= range.end),
        }
    }
}

impl FromStr for OccurrenceRanges {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "g" {
            return Ok(Self::All);
        }
        if value.is_empty() {
            return Err("occurrence ranges must not be empty".to_owned());
        }

        let mut ranges = value
            .split(',')
            .map(OccurrenceRange::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        ranges.sort_unstable_by_key(|range| range.start);

        let mut merged: Vec<OccurrenceRange> = Vec::with_capacity(ranges.len());
        for range in ranges {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end.saturating_add(1)
            {
                previous.end = previous.end.max(range.end);
            } else {
                merged.push(range);
            }
        }

        Ok(Self::Selected(merged))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OccurrenceRange {
    start: usize,
    end: usize,
}

impl FromStr for OccurrenceRange {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err("occurrence range must not be empty".to_owned());
        }

        if let Some((start, end)) = value.split_once('-') {
            if (start.is_empty() && end.is_empty()) || end.contains('-') {
                return Err(format!("invalid occurrence range '{value}'"));
            }

            let start = if start.is_empty() {
                1
            } else {
                parse_occurrence(start, value)?
            };
            let end = if end.is_empty() {
                usize::MAX
            } else {
                parse_occurrence(end, value)?
            };
            if start > end {
                return Err(format!("occurrence range '{value}' is reversed"));
            }

            Ok(Self { start, end })
        } else {
            let occurrence = parse_occurrence(value, value)?;
            Ok(Self {
                start: occurrence,
                end: occurrence,
            })
        }
    }
}

fn parse_occurrence(value: &str, range: &str) -> Result<usize, String> {
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("invalid occurrence range '{range}'"));
    }
    let occurrence = value
        .parse::<usize>()
        .map_err(|_| format!("invalid occurrence range '{range}'"))?;
    if occurrence == 0 {
        return Err("occurrence ranges are 1-indexed; zero is invalid".to_owned());
    }
    Ok(occurrence)
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
        rename_target(source, &args.name).map(RenameTarget::Changed)
    })
}

pub fn run_unren(args: UnrenArgs) -> Result<(), String> {
    let file_name = ensure_source_has_file_name(&args.path)?;
    ensure_leaf_name(&args.path, &args.name, "name")?;

    let parent = args.path.parent().unwrap_or_else(|| Path::new(""));
    let source = parent.join(&args.name);
    let target = parent.join(file_name);

    rename_entry(source, args.filters.filetype, |_| {
        Ok(RenameTarget::Changed(target))
    })
}

pub fn run_renx(args: RenxArgs) -> Result<(), String> {
    let RenxArgs {
        filters,
        operation,
        search,
        replacement,
        path,
    } = args;
    let operation = RenxOperation::from_args(operation, search, replacement)?;

    rename_entry(path, filters.filetype, |source| {
        operation.make_target(source)
    })
}

#[derive(Debug)]
enum RenxOperation {
    Append(OsString),
    Prepend(OsString),
    Ext(OsString),
    Stem(OsString),
    Name(OsString),
    Regex {
        options: RegexOptions,
        search: OsString,
        replacement: OsString,
    },
    Fixed {
        options: FixedOptions,
        search: OsString,
        replacement: OsString,
    },
}

impl RenxOperation {
    fn from_args(
        args: RenxOperationArgs,
        search: Option<OsString>,
        replacement: Option<OsString>,
    ) -> Result<Self, String> {
        if let Some(value) = args.append {
            Ok(Self::Append(value))
        } else if let Some(value) = args.prepend {
            Ok(Self::Prepend(value))
        } else if let Some(value) = args.ext {
            Ok(Self::Ext(value))
        } else if let Some(value) = args.stem {
            Ok(Self::Stem(value))
        } else if let Some(value) = args.name {
            Ok(Self::Name(value))
        } else if let Some(options) = args.sr_regex {
            Ok(Self::Regex {
                options,
                search: search.ok_or_else(|| "-s is required with --sr-regex".to_owned())?,
                replacement: replacement
                    .ok_or_else(|| "-r is required with --sr-regex".to_owned())?,
            })
        } else if let Some(options) = args.sr_fixed {
            Ok(Self::Fixed {
                options,
                search: search.ok_or_else(|| "-s is required with --sr-fixed".to_owned())?,
                replacement: replacement
                    .ok_or_else(|| "-r is required with --sr-fixed".to_owned())?,
            })
        } else {
            Err("exactly one rename operation is required".to_owned())
        }
    }

    fn make_target(&self, source: &Path) -> Result<RenameTarget, String> {
        match self {
            Self::Append(value) => append_rename_target(source, value).map(RenameTarget::Changed),
            Self::Prepend(value) => prepend_rename_target(source, value).map(RenameTarget::Changed),
            Self::Ext(value) => ext_rename_target(source, value).map(RenameTarget::Changed),
            Self::Stem(value) => stem_rename_target(source, value).map(RenameTarget::Changed),
            Self::Name(value) => rename_target(source, value).map(RenameTarget::Changed),
            Self::Regex {
                options,
                search,
                replacement,
            } => search_replace_target(source, |file_name| {
                regex_replace_name(file_name, search, replacement, *options)
            }),
            Self::Fixed {
                options,
                search,
                replacement,
            } => search_replace_target(source, |file_name| {
                fixed_replace_name(file_name, search, replacement, options)
            }),
        }
    }
}

enum RenameTarget {
    Changed(PathBuf),
    Unchanged,
}

fn rename_entry<F>(
    source: PathBuf,
    filetype: Option<Vec<FileType>>,
    make_target: F,
) -> Result<(), String>
where
    F: FnOnce(&Path) -> Result<RenameTarget, String>,
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

    match make_target(&source)? {
        RenameTarget::Changed(target) => rename_without_overwrite(&source, &target),
        RenameTarget::Unchanged => Ok(()),
    }
}

fn rename_target(source: &Path, name: &OsStr) -> Result<PathBuf, String> {
    ensure_source_has_file_name(source)?;
    ensure_leaf_name(source, name, "new name")?;

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(name))
}

fn append_rename_target(source: &Path, suffix: &OsStr) -> Result<PathBuf, String> {
    let file_name = ensure_source_has_file_name(source)?;
    let mut target_name = file_name.to_os_string();
    target_name.push(suffix);

    rename_target(source, &target_name)
}

fn prepend_rename_target(source: &Path, prefix: &OsStr) -> Result<PathBuf, String> {
    let file_name = ensure_source_has_file_name(source)?;
    let mut target_name = prefix.to_os_string();
    target_name.push(file_name);

    rename_target(source, &target_name)
}

fn stem_rename_target(source: &Path, stem: &OsStr) -> Result<PathBuf, String> {
    let file_name = ensure_source_has_file_name(source)?;
    ensure_leaf_name(source, stem, "new stem")?;

    let suffix = suffix_from_last_dot(file_name).ok_or_else(|| {
        format!(
            "refusing to rename '{}': file name has no '.'",
            source.display()
        )
    })?;
    let mut target_name = stem.to_os_string();
    target_name.push(suffix);

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(target_name))
}

fn ext_rename_target(source: &Path, extension: &OsStr) -> Result<PathBuf, String> {
    let file_name = ensure_source_has_file_name(source)?;
    ensure_leaf_name(source, extension, "new extension")?;

    let mut target_name = prefix_through_last_dot(file_name).unwrap_or_else(|| {
        let mut prefix = file_name.to_os_string();
        prefix.push(".");
        prefix
    });
    target_name.push(extension);

    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent.join(target_name))
}

fn search_replace_target<F>(source: &Path, replace: F) -> Result<RenameTarget, String>
where
    F: FnOnce(&OsStr) -> Result<OsString, String>,
{
    let file_name = ensure_source_has_file_name(source)?;
    let target_name = replace(file_name)?;
    if target_name == file_name {
        return Ok(RenameTarget::Unchanged);
    }

    ensure_leaf_name(source, &target_name, "resulting name")?;
    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(RenameTarget::Changed(parent.join(target_name)))
}

fn ensure_source_has_file_name(source: &Path) -> Result<&OsStr, String> {
    source.file_name().ok_or_else(|| {
        format!(
            "refusing to rename '{}': path has no file name",
            source.display()
        )
    })
}

fn ensure_leaf_name(source: &Path, name: &OsStr, label: &str) -> Result<(), String> {
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

fn regex_replace_name(
    file_name: &OsStr,
    search: &OsStr,
    replacement: &OsStr,
    options: RegexOptions,
) -> Result<OsString, String> {
    let pattern = search
        .to_str()
        .ok_or_else(|| "regular expression must be valid UTF-8".to_owned())?;
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(options.case_insensitive)
        .build()
        .map_err(|err| format!("invalid regular expression: {err}"))?;
    let file_name = os_str_to_search_bytes(file_name, "file name")?;
    let replacement = os_str_to_search_bytes(replacement, "regular expression replacement")?;

    let result = if options.global {
        regex.replace_all(&file_name, replacement.as_slice())
    } else {
        regex.replace(&file_name, replacement.as_slice())
    };
    search_bytes_to_os_string(result.into_owned())
}

fn fixed_replace_name(
    file_name: &OsStr,
    search: &OsStr,
    replacement: &OsStr,
    options: &FixedOptions,
) -> Result<OsString, String> {
    let file_name = os_str_to_search_bytes(file_name, "file name")?;
    let search = os_str_to_search_bytes(search, "fixed search string")?;
    if search.is_empty() {
        return Err("fixed search string must not be empty".to_owned());
    }
    let replacement = os_str_to_search_bytes(replacement, "fixed replacement")?;
    let matches = fixed_matches(&file_name, &search, options.case_insensitive)?;

    let mut result = Vec::with_capacity(file_name.len());
    let mut cursor = 0;
    for (index, (start, end)) in matches.into_iter().enumerate() {
        result.extend_from_slice(&file_name[cursor..start]);
        if options.ranges.includes(index + 1) {
            result.extend_from_slice(&replacement);
        } else {
            result.extend_from_slice(&file_name[start..end]);
        }
        cursor = end;
    }
    result.extend_from_slice(&file_name[cursor..]);

    search_bytes_to_os_string(result)
}

fn fixed_matches(
    file_name: &[u8],
    search: &[u8],
    case_insensitive: bool,
) -> Result<Vec<(usize, usize)>, String> {
    if case_insensitive {
        let search = std::str::from_utf8(search)
            .map_err(|_| "case-insensitive fixed search must be valid UTF-8".to_owned())?;
        let regex = RegexBuilder::new(&regex::escape(search))
            .case_insensitive(true)
            .build()
            .map_err(|err| format!("invalid fixed search string: {err}"))?;
        return Ok(regex
            .find_iter(file_name)
            .map(|matched| (matched.start(), matched.end()))
            .collect());
    }

    Ok(exact_fixed_matches(file_name, search))
}

fn exact_fixed_matches(file_name: &[u8], search: &[u8]) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let mut cursor = 0;
    while cursor + search.len() <= file_name.len() {
        let Some(relative_start) = file_name[cursor..]
            .windows(search.len())
            .position(|candidate| candidate == search)
        else {
            break;
        };
        let start = cursor + relative_start;
        let end = start + search.len();
        matches.push((start, end));
        cursor = end;
    }
    matches
}

#[cfg(any(unix, target_os = "redox"))]
fn os_str_to_search_bytes(value: &OsStr, _label: &str) -> Result<Vec<u8>, String> {
    use std::os::unix::ffi::OsStrExt;

    Ok(value.as_bytes().to_vec())
}

#[cfg(windows)]
fn os_str_to_search_bytes(value: &OsStr, label: &str) -> Result<Vec<u8>, String> {
    value
        .to_str()
        .map(|value| value.as_bytes().to_vec())
        .ok_or_else(|| format!("{label} must be valid Unicode"))
}

#[cfg(any(unix, target_os = "redox"))]
fn search_bytes_to_os_string(value: Vec<u8>) -> Result<OsString, String> {
    use std::os::unix::ffi::OsStringExt;

    Ok(OsString::from_vec(value))
}

#[cfg(windows)]
fn search_bytes_to_os_string(value: Vec<u8>) -> Result<OsString, String> {
    String::from_utf8(value)
        .map(OsString::from)
        .map_err(|_| "resulting name must be valid Unicode".to_owned())
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
    fn append_renames_complete_basename() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("report.txt");
        let new = tempdir.path().join("report.txt.bak");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--append", ".bak"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn prepend_renames_complete_basename() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("report.txt");
        let new = tempdir.path().join("draft-report.txt");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--prepend", "draft-"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn stem_renames_only_text_before_last_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("archive.tar.gz");
        let new = tempdir.path().join("renamed.gz");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--stem", "renamed"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn stem_refuses_file_name_without_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run_renx(renx_args(&old, &["--stem", "new"])).unwrap_err();

        assert!(err.contains("file name has no '.'"));
        assert!(old.exists());
    }

    #[test]
    fn stem_refuses_stem_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old.txt");
        fs::write(&old, "contents").unwrap();

        let err = run_renx(renx_args(&old, &["--stem", "subdir/new"])).unwrap_err();

        assert!(err.contains("new stem must not contain path components"));
        assert!(old.exists());
    }

    #[test]
    fn ext_renames_only_text_after_last_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("archive.tar.gz");
        let new = tempdir.path().join("archive.tar.xz");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--ext", "xz"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn ext_adds_extension_to_file_name_without_dot() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("index");
        let new = tempdir.path().join("index.html");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--ext", "html"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn ext_refuses_extension_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old.txt");
        fs::write(&old, "contents").unwrap();

        let err = run_renx(renx_args(&old, &["--ext", "subdir/new"])).unwrap_err();

        assert!(err.contains("new extension must not contain path components"));
        assert!(old.exists());
    }

    #[test]
    fn name_matches_ren_behavior() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old.txt");
        let new = tempdir.path().join("new");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--name", "new"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn renx_applies_type_filter() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run_renx(renx_args(&old, &["--append", ".new", "-t", "d"])).unwrap_err();

        assert!(err.contains("type does not match"));
        assert!(old.exists());
    }

    #[test]
    fn renx_refuses_to_overwrite_existing_entry() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("old.new");
        fs::write(&old, "old contents").unwrap();
        fs::write(&new, "new contents").unwrap();

        let err = run_renx(renx_args(&old, &["--append", ".new"])).unwrap_err();

        assert!(err.contains("already exists"));
        assert_eq!(fs::read_to_string(old).unwrap(), "old contents");
        assert_eq!(fs::read_to_string(new).unwrap(), "new contents");
    }

    #[test]
    fn renx_refuses_result_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run_renx(renx_args(&old, &["--prepend", "subdir/"])).unwrap_err();

        assert!(err.contains("new name must not contain path components"));
        assert!(old.exists());
    }

    #[test]
    fn regex_replaces_first_match_and_expands_capture() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("chapter-12-draft-34.txt");
        let new = tempdir.path().join("chapter-[12]-draft-34.txt");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(
            &old,
            &["--sr-regex", "-s", "([0-9]+)", "-r", "[$1]"],
        ))
        .unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn regex_global_case_insensitive_preserves_capture_text() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("Cat-cat.txt");
        let new = tempdir.path().join("Cat!-cat!.txt");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(
            &old,
            &["--sr-regex=ig", "-s", "(?P<word>cat)", "-r", "${word}!"],
        ))
        .unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn regex_no_match_succeeds_without_renaming() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old.txt");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(
            &old,
            &["--sr-regex=g", "-s", "missing", "-r", "new"],
        ))
        .unwrap();

        assert_eq!(fs::read_to_string(old).unwrap(), "contents");
    }

    #[test]
    fn invalid_regex_leaves_source_unchanged() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old.txt");
        fs::write(&old, "contents").unwrap();

        let err = run_renx(renx_args(&old, &["--sr-regex", "-s", "(", "-r", "new"])).unwrap_err();

        assert!(err.contains("invalid regular expression"));
        assert_eq!(fs::read_to_string(old).unwrap(), "contents");
    }

    #[test]
    fn fixed_replaces_first_match_by_default() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("one-one-one.txt");
        let new = tempdir.path().join("two-one-one.txt");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--sr-fixed", "-s", "one", "-r", "two"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn fixed_replaces_selected_occurrence_ranges() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("a-a-a-a-a");
        let new = tempdir.path().join("a-x-x-a-x");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(
            &old,
            &["--sr-fixed=2-3,5-", "-s", "a", "-r", "x"],
        ))
        .unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn fixed_global_case_insensitive_uses_unicode_matching() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("K-k-K.txt");
        let new = tempdir.path().join("x-x-x.txt");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--sr-fixed=ig", "-s", "k", "-r", "x"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn fixed_matches_are_non_overlapping_and_replacement_is_not_rescanned() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("aaaa");
        let new = tempdir.path().join("aaaaaa");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(&old, &["--sr-fixed=g", "-s", "aa", "-r", "aaa"])).unwrap();

        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new).unwrap(), "contents");
    }

    #[test]
    fn fixed_out_of_range_selection_succeeds_unchanged() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("one-one");
        fs::write(&old, "contents").unwrap();

        run_renx(renx_args(
            &old,
            &["--sr-fixed=3-", "-s", "one", "-r", "two"],
        ))
        .unwrap();

        assert_eq!(fs::read_to_string(old).unwrap(), "contents");
    }

    #[test]
    fn fixed_rejects_empty_search() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        fs::write(&old, "contents").unwrap();

        let err = run_renx(renx_args(&old, &["--sr-fixed", "-s", "", "-r", "new"])).unwrap_err();

        assert!(err.contains("must not be empty"));
        assert!(old.exists());
    }

    #[cfg(any(unix, target_os = "redox"))]
    #[test]
    fn search_replace_preserves_unmatched_non_utf8_bytes() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let file_name = OsString::from_vec(vec![b'a', 0xff, b'A']);
        let options: FixedOptions = "ig".parse().unwrap();
        let result =
            fixed_replace_name(&file_name, OsStr::new("a"), OsStr::new("x"), &options).unwrap();

        assert_eq!(result.as_bytes(), &[b'x', 0xff, b'x']);
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

    #[test]
    fn renx_parses_options_interspersed_with_path() {
        let args = RenxArgs::try_parse_from([
            "renx", "old", "--append", ".new", "-t", "f", "--type", "dir",
        ])
        .unwrap();

        assert_eq!(args.path, PathBuf::from("old"));
        assert_eq!(args.operation.append, Some(OsString::from(".new")));
        assert_eq!(
            args.filters.filetype,
            Some(vec![FileType::File, FileType::Directory])
        );
    }

    #[test]
    fn renx_requires_exactly_one_operation() {
        let missing = RenxArgs::try_parse_from(["renx", "old"]).unwrap_err();
        let conflicting =
            RenxArgs::try_parse_from(["renx", "--append", "x", "--prepend", "y", "old"])
                .unwrap_err();

        assert_eq!(
            missing.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        assert_eq!(conflicting.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn search_modes_require_search_and_replacement() {
        let missing_search =
            RenxArgs::try_parse_from(["renx", "--sr-regex", "-r", "new", "old"]).unwrap_err();
        let missing_replacement =
            RenxArgs::try_parse_from(["renx", "--sr-fixed", "-s", "old", "old.txt"]).unwrap_err();
        let unexpected_search =
            RenxArgs::try_parse_from(["renx", "--append", "x", "-s", "old", "old.txt"])
                .unwrap_err();

        assert_eq!(
            missing_search.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            missing_replacement.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            unexpected_search.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn search_mode_options_require_equals() {
        let err = RenxArgs::try_parse_from([
            "renx",
            "--sr-regex",
            "ig",
            "-s",
            "old",
            "-r",
            "new",
            "old.txt",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn parses_and_merges_fixed_ranges() {
        let options: FixedOptions = "3-5,1,2-4,8-".parse().unwrap();

        assert_eq!(
            options.ranges,
            OccurrenceRanges::Selected(vec![
                OccurrenceRange { start: 1, end: 5 },
                OccurrenceRange {
                    start: 8,
                    end: usize::MAX
                }
            ])
        );
    }

    #[test]
    fn rejects_invalid_fixed_ranges() {
        for value in ["0", "+1", "-", "3-2", "1--2", "g,2", "i:"] {
            assert!(value.parse::<FixedOptions>().is_err(), "accepted {value}");
        }
    }

    #[test]
    fn unren_renames_file_within_same_parent() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&new, "contents").unwrap();

        run_unren(unren_args(&old, "new", None)).unwrap();

        assert!(!new.exists());
        assert_eq!(fs::read_to_string(old).unwrap(), "contents");
    }

    #[test]
    fn unren_renames_file_in_subdirectory() {
        let tempdir = tempfile::tempdir().unwrap();
        let subdir = tempdir.path().join("sub");
        fs::create_dir(&subdir).unwrap();
        let old = subdir.join("old");
        let new = subdir.join("new");
        fs::write(&new, "contents").unwrap();

        run_unren(unren_args(&old, "new", None)).unwrap();

        assert!(!new.exists());
        assert_eq!(fs::read_to_string(old).unwrap(), "contents");
    }

    #[test]
    fn unren_undoes_ren() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "contents").unwrap();

        run(rename_args(&old, "new", None)).unwrap();
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(&new).unwrap(), "contents");

        run_unren(unren_args(&old, "new", None)).unwrap();
        assert!(!new.exists());
        assert_eq!(fs::read_to_string(&old).unwrap(), "contents");
    }

    #[test]
    fn unren_undoes_ren_in_subdirectory() {
        let tempdir = tempfile::tempdir().unwrap();
        let subdir = tempdir.path().join("sub");
        fs::create_dir(&subdir).unwrap();
        let old = subdir.join("old");
        let new = subdir.join("new");
        fs::write(&old, "contents").unwrap();

        run(rename_args(&old, "new", None)).unwrap();
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(&new).unwrap(), "contents");

        run_unren(unren_args(&old, "new", None)).unwrap();
        assert!(!new.exists());
        assert_eq!(fs::read_to_string(&old).unwrap(), "contents");
    }

    #[test]
    fn unren_refuses_to_overwrite_existing_entry() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&old, "old").unwrap();
        fs::write(&new, "new").unwrap();

        let err = run_unren(unren_args(&old, "new", None)).unwrap_err();

        assert!(err.contains("already exists"));
        assert_eq!(fs::read_to_string(old).unwrap(), "old");
        assert_eq!(fs::read_to_string(new).unwrap(), "new");
    }

    #[test]
    fn unren_fails_when_source_does_not_exist() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");

        let err = run_unren(unren_args(&old, "new", None)).unwrap_err();

        assert!(err.contains("cannot read"));
    }

    #[test]
    fn unren_refuses_name_with_path_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");

        let err = run_unren(unren_args(&old, "subdir/new", None)).unwrap_err();

        assert!(err.contains("name must not contain path components"));
    }

    #[test]
    fn unren_refuses_path_without_file_name() {
        let err = run_unren(unren_args(Path::new("/"), "new", None)).unwrap_err();

        assert!(err.contains("path has no file name"));
    }

    #[test]
    fn unren_accepts_matching_type_filter() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&new, "contents").unwrap();

        run_unren(unren_args(&old, "new", Some(vec![FileType::File]))).unwrap();

        assert!(old.exists());
        assert!(!new.exists());
    }

    #[test]
    fn unren_refuses_non_matching_type_filter() {
        let tempdir = tempfile::tempdir().unwrap();
        let old = tempdir.path().join("old");
        let new = tempdir.path().join("new");
        fs::write(&new, "contents").unwrap();

        let err = run_unren(unren_args(&old, "new", Some(vec![FileType::Directory]))).unwrap_err();

        assert!(err.contains("type does not match"));
        assert!(new.exists());
        assert!(!old.exists());
    }

    #[test]
    fn unren_parses_type_filter_multiple_times() {
        let args =
            UnrenArgs::try_parse_from(["unren", "-tf", "--type", "dir", "old", "new"]).unwrap();

        assert_eq!(
            args.filters.filetype,
            Some(vec![FileType::File, FileType::Directory])
        );
        assert_eq!(args.path, PathBuf::from("old"));
        assert_eq!(args.name, OsString::from("new"));
    }

    #[test]
    fn unren_rejects_compact_multiple_type_specs() {
        let err = UnrenArgs::try_parse_from(["unren", "-tfd", "old", "new"]).unwrap_err();

        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    fn rename_args(path: &Path, name: &str, filetype: Option<Vec<FileType>>) -> RenameArgs {
        RenameArgs {
            filters: TypeFilterArgs { filetype },
            path: path.to_path_buf(),
            name: OsString::from(name),
        }
    }

    fn unren_args(path: &Path, name: &str, filetype: Option<Vec<FileType>>) -> UnrenArgs {
        UnrenArgs {
            filters: TypeFilterArgs { filetype },
            path: path.to_path_buf(),
            name: OsString::from(name),
        }
    }

    fn renx_args(path: &Path, args: &[&str]) -> RenxArgs {
        let mut argv = vec![OsString::from("renx")];
        argv.extend(args.iter().map(OsString::from));
        argv.push(path.as_os_str().to_os_string());
        RenxArgs::try_parse_from(argv).unwrap()
    }
}
