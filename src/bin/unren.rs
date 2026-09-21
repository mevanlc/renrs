use std::process::ExitCode;

use clap::Parser;
use ren::UnrenArgs;

fn main() -> ExitCode {
    match ren::run_unren(UnrenArgs::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("unren: {err}");
            ExitCode::FAILURE
        }
    }
}
