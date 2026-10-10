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
