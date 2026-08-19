//! Turns a ContractorCRM hand-off envelope into a ContractorProject job.
//!
//! Usage: `handoff-import --envelope <path> --database <path> [--timezone <tz>]`
//!
//! On success it prints one JSON line — `{"jobId","jobName","createdAt"}` — so
//! the caller can link the job back in its own system. All logic lives in
//! `contractorproject_lib::handoff_import` so it is testable without spawning
//! a process.

use std::process::ExitCode;

use contractorproject_lib::handoff_import::{self, USAGE};

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let options = match handoff_import::parse_arguments(&arguments) {
        Ok(Some(options)) => options,
        // --help is a successful run that imports nothing.
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("handoff-import: {message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    match handoff_import::import(&options) {
        Ok(job) => match serde_json::to_string(&job) {
            Ok(line) => {
                println!("{line}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("handoff-import: could not report the result: {error}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprintln!("handoff-import: {message}");
            ExitCode::FAILURE
        }
    }
}
