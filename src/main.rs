use std::process::ExitCode;

use clap::Parser;
use ren::RenameArgs;

fn main() -> ExitCode {
    match ren::run(RenameArgs::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ren: {err}");
            ExitCode::FAILURE
        }
    }
}
