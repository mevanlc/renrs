use std::ffi::OsStr;
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use ren::{RenameArgs, UnrenArgs};

fn main() -> ExitCode {
    let is_unren = std::env::args_os()
        .next()
        .as_ref()
        .map(Path::new)
        .and_then(|path| path.file_stem())
        .and_then(OsStr::to_str)
        == Some("unren");

    let result = if is_unren {
        ren::run_unren(UnrenArgs::parse()).map_err(|err| format!("unren: {err}"))
    } else {
        ren::run(RenameArgs::parse()).map_err(|err| format!("ren: {err}"))
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
