use eutheto_cli::run_from;
use std::env;
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let stdout = io::stdout();
    let stderr = io::stderr();
    let code = run_from(env::args_os(), stdout.lock(), stderr.lock());
    ExitCode::from(code.value())
}
