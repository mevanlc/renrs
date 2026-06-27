use std::process::ExitCode;

use clap::Parser;
use ren::ExtArgs;

fn main() -> ExitCode {
    match ren::run_ext(ExtArgs::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ren-ext: {err}");
            ExitCode::FAILURE
        }
    }
}
