use aiw::Cli;
use clap::CommandFactory as _;
use clap_complete::env::CompleteEnv;
use std::process::ExitCode;

fn main() -> ExitCode {
    CompleteEnv::with_factory(Cli::command)
        .completer("aiw")
        .complete();
    match Cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}
