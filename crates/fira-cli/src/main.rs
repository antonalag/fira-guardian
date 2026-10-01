//! FIRA Guardian CLI — first adapter entrypoint.
//!
//! Task 1 scope: a version shell only. It prints the product name and the
//! Frozen MVP Contract version. No `audit` subcommand, no capability access, no
//! process execution, no persistence — those arrive in Task 10.

fn main() {
    println!("FIRA Guardian — Independent Release Readiness Auditor");
    println!("contract-version: {}", fira_core::CONTRACT_VERSION);
    println!("status: skeleton (Task 1); audit command available from Task 10");
}
