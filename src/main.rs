use std::process::ExitCode;

fn main() -> ExitCode {
    match aiw::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}
