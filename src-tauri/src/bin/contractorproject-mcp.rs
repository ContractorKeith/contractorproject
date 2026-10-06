//! The MCP stdio helper. All logic lives in `contractorproject_lib::mcp` so it
//! can be tested; this binary is only the entry point and the exit code.

use std::process::ExitCode;

fn main() -> ExitCode {
    match contractorproject_lib::mcp::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
