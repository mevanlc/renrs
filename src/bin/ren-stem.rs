use std::process::ExitCode;

use clap::Parser;
use ren::StemArgs;

fn main() -> ExitCode {
    match ren::run_stem(StemArgs::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("ren-stem: {err}");
            ExitCode::FAILURE
        }
    }
}
