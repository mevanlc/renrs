use std::process::ExitCode;

use clap::Parser;
use ren::RenxArgs;

fn main() -> ExitCode {
    match ren::run_renx(RenxArgs::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("renx: {err}");
            ExitCode::FAILURE
        }
    }
}
