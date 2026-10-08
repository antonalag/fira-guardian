//! FIRA Guardian CLI — first adapter entrypoint (`fira-guardian`).
//!
//! Provides the `audit` subcommand (Task 10): parse args → run the minimal audit
//! flow in the adapter → render JSON/Markdown → write to stdout or, via the
//! runtime's path-containment sink, a user `--output` path (never under the
//! project tree).

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use fira_adapters::cli::{run_audit, write_output, AuditArgs, Format};
use fira_presentation::{render_json, render_markdown};

#[derive(Debug, Parser)]
#[command(
    name = "fira-guardian",
    about = "Independent Release Readiness Auditor"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run an audit over a project.
    Audit(AuditArgs),
    /// Print the targeted frozen-contract version.
    Version,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Audit(args)) => run(args),
        Some(Commands::Version) | None => {
            println!("FIRA Guardian — Independent Release Readiness Auditor");
            println!("contract-version: {}", fira_core::CONTRACT_VERSION);
            ExitCode::SUCCESS
        }
    }
}

fn run(args: AuditArgs) -> ExitCode {
    let format = args.format;
    let output = args.output.clone();
    let project = args.project.clone();

    let out = match run_audit(args) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("audit failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    // The canonical audit-workspace artifacts (WS-1) are already persisted by the
    // adapter. Report that location first so the user knows the authoritative
    // output; stdout/`--output` are clearly labeled as exported copies (§9.4).
    eprintln!(
        "canonical audit-workspace artifacts: {}",
        out.canonical_location
    );

    let json = render_json(&out.report);
    let md = render_markdown(&out.report);
    let rendered = match format {
        Format::Json => json,
        Format::Md => md,
        Format::Both => format!("{json}\n\n{md}"),
    };

    match output {
        None => {
            println!("{rendered}");
            ExitCode::SUCCESS
        }
        Some(path) => match write_output(&project, &path, rendered.as_bytes()) {
            Ok(written) => {
                println!("exported copy written to {}", written.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
    }
}
