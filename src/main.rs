use clap::CommandFactory as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    clap_complete::env::CompleteEnv::with_factory(aiw::Cli::command)
        .completer("aiw")
        .complete();
    match aiw::Cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}
