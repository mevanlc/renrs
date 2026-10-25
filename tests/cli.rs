use std::fs;
use std::process::Command;

#[test]
fn commands_report_their_own_name_and_package_version() {
    for (name, binary) in [
        ("ren", env!("CARGO_BIN_EXE_ren")),
        ("renx", env!("CARGO_BIN_EXE_renx")),
        ("unren", env!("CARGO_BIN_EXE_unren")),
    ] {
        let output = Command::new(binary).arg("--version").output().unwrap();
        assert!(output.status.success(), "{name}: {output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            format!("{name} {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn installed_command_names_dispatch_rename_operations() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original.txt");
    fs::write(&original, "contents").unwrap();

    for (binary, args, expected) in [
        (
            env!("CARGO_BIN_EXE_ren"),
            vec!["original.txt", "renamed.txt"],
            "renamed.txt",
        ),
        (
            env!("CARGO_BIN_EXE_unren"),
            vec!["original.txt", "renamed.txt"],
            "original.txt",
        ),
        (
            env!("CARGO_BIN_EXE_renx"),
            vec!["--append", ".bak", "original.txt"],
            "original.txt.bak",
        ),
    ] {
        let output = Command::new(binary)
            .args(args)
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{binary}: {output:?}");
        assert_eq!(
            fs::read_to_string(directory.path().join(expected)).unwrap(),
            "contents"
        );
    }
    assert!(!original.exists());
}

#[cfg(unix)]
mod current_directory {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        // macOS's temporary directory spelling can itself contain a symlink.
        let old = temp.path().canonicalize().unwrap().join("old");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("contents"), "preserved").unwrap();
        (temp, old)
    }

    fn ren(cwd: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ren"));
        command.current_dir(cwd).env_remove("PWD");
        command
    }

    fn assert_refused(output: std::process::Output, message: &str, old: &Path) {
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{output:?}"
        );
        assert_eq!(
            fs::read_to_string(old.join("contents")).unwrap(),
            "preserved"
        );
        assert!(!old.with_file_name("new").exists());
    }

    #[test]
    fn renames_current_directory_with_or_without_pwd() {
        for with_pwd in [true, false] {
            let (_temp, old) = fixture();
            let mut command = ren(&old);
            if with_pwd {
                command.env("PWD", &old);
            }
            let output = command.args(["-td", ".", "new"]).output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert!(!old.exists());
            assert_eq!(
                fs::read_to_string(old.with_file_name("new").join("contents")).unwrap(),
                "preserved"
            );
        }
    }

    #[test]
    fn preserves_no_overwrite_guarantee() {
        let (_temp, old) = fixture();
        let target = old.with_file_name("new");
        fs::create_dir(&target).unwrap();
        let output = ren(&old).args([".", "new"]).output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
        assert!(old.join("contents").exists());
        assert_eq!(fs::read_dir(target).unwrap().count(), 0);
    }

    #[test]
    fn preserves_name_validation_and_type_filters() {
        let (_temp, old) = fixture();
        for (args, message) in [
            (vec![".", "../new"], "must not contain path components"),
            (vec!["-tf", ".", "new"], "type does not match"),
        ] {
            assert_refused(ren(&old).args(args).output().unwrap(), message, &old);
        }
    }

    #[test]
    fn refuses_symlinks_in_pwd_leaf_or_ancestors() {
        let (_temp, old) = fixture();
        let link = old.with_file_name("link");
        symlink(&old, &link).unwrap();
        let child = old.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("contents"), "preserved").unwrap();
        for (cwd, pwd) in [(&old, link.clone()), (&child, link.join("child"))] {
            assert_refused(
                ren(cwd)
                    .env("PWD", pwd)
                    .args([".", "new"])
                    .output()
                    .unwrap(),
                "contains symlink",
                cwd,
            );
        }
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    }

    #[test]
    fn refuses_stale_missing_or_relative_pwd() {
        let (_temp, old) = fixture();
        for pwd in [
            old.parent().unwrap().to_path_buf(),
            old.with_file_name("missing"),
            PathBuf::from("old"),
        ] {
            assert_refused(
                ren(&old)
                    .env("PWD", pwd)
                    .args([".", "new"])
                    .output()
                    .unwrap(),
                "ren:",
                &old,
            );
        }
    }

    #[test]
    fn only_literal_dot_in_ren_opts_in() {
        let (_temp, old) = fixture();
        for source in ["./", "./.", "../old/."] {
            let output = ren(&old).args([source, "new"]).output().unwrap();
            assert_refused(output, "ren:", &old);
        }
        for (binary, args) in [
            (env!("CARGO_BIN_EXE_renx"), vec!["--append=-new", "."]),
            (env!("CARGO_BIN_EXE_unren"), vec![".", "new"]),
        ] {
            let output = Command::new(binary)
                .current_dir(&old)
                .env("PWD", &old)
                .args(args)
                .output()
                .unwrap();
            assert_refused(output, "path has no file name", &old);
        }
    }

    #[test]
    fn refuses_filesystem_root() {
        let output = ren(Path::new("/")).args([".", "new"]).output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("filesystem root"));
    }
}

#[cfg(windows)]
#[test]
fn literal_dot_is_unsupported_on_windows() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ren"))
        .current_dir(temp.path())
        .args([".", "new"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("not supported on Windows"));
    assert!(temp.path().is_dir());
}
