use eutheto_cli::{CliError, run_from};
use std::env;
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let stdout = io::stdout();
    let output = stdout.lock();

    match run_from(env::args_os(), output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Arguments(error)) => {
            let exit_code = error.exit_code();
            if error.print().is_err() {
                return ExitCode::FAILURE;
            }
            match u8::try_from(exit_code) {
                Ok(code) => ExitCode::from(code),
                Err(_) => ExitCode::FAILURE,
            }
        }
        Err(error) => {
            eprintln!("optimizer: {error}");
            ExitCode::FAILURE
        }
    }
}
