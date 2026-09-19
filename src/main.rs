mod cli;

use cli::Cli;
use std::process::ExitCode;

fn main() -> ExitCode {
    match Cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}
