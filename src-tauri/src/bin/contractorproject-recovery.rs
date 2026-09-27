use std::process::ExitCode;

use contractorproject_lib::recovery_cli::{self, USAGE};

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let options = match recovery_cli::parse_arguments(&arguments) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("contractorproject-recovery: {message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    match recovery_cli::run(&options).and_then(|result| {
        serde_json::to_string(&result)
            .map_err(|_| "could not encode the recovery result".to_owned())
    }) {
        Ok(result) => {
            println!("{result}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("contractorproject-recovery: {message}");
            ExitCode::FAILURE
        }
    }
}
