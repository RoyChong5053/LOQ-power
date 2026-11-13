//! `loq-power-helper` — the privileged fallback used when `loq-powerd` is not
//! running.
//!
//! Two protocols:
//! - no arguments: a single JSON [`ApplyRequest`] on stdin, a JSON
//!   [`ApplyResponse`] on stdout;
//! - `--takeover` / `--restore-tlp`: the TLP hand-over wizard steps, reported
//!   as a JSON [`loq_core::takeover::Report`] on stdout.
//!
//! Safety rules:
//! - refuses to run without root (it is meant to be launched through pkexec);
//! - reads the request once, with a size cap;
//! - never executes a shell or trusts a path from the client;
//! - every write goes through `loq-core`, which re-discovers sysfs paths and
//!   validates ranges/choices on the live tree.

use std::io::Read;
use std::process::ExitCode;

use loq_core::request::{ApplyRequest, ApplyResponse};
use loq_core::takeover::{self, Report};
use loq_core::{Sysfs, apply};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

fn main() -> ExitCode {
    let uid = unsafe { libc::geteuid() };
    if uid != 0 {
        println!(
            "{}",
            serde_json::to_string(&ApplyResponse::error(
                "loq-power-helper must be started through pkexec"
            ))
            .unwrap_or_default()
        );
        return ExitCode::from(2);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--takeover") {
        let dry_run = args.iter().any(|arg| arg == "--dry-run");
        let force = args.iter().any(|arg| arg == "--force");
        let import = !args.iter().any(|arg| arg == "--no-import");
        return match takeover::takeover(&Sysfs::new(), dry_run, import, force) {
            Ok(report) => report_exit(&report),
            Err(error) => fail_takeover(error.to_string()),
        };
    }
    if args.iter().any(|arg| arg == "--restore-tlp") {
        let dry_run = args.iter().any(|arg| arg == "--dry-run");
        return match takeover::restore_tlp(dry_run) {
            Ok(report) => report_exit(&report),
            Err(error) => fail_takeover(error.to_string()),
        };
    }

    let mut input = String::new();
    if let Err(error) = std::io::stdin()
        .take(MAX_REQUEST_BYTES)
        .read_to_string(&mut input)
    {
        return fail(format!("cannot read request: {error}"));
    }

    if input.trim().is_empty() {
        return fail("empty request".to_string());
    }

    let request: ApplyRequest = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => return fail(format!("invalid request: {error}")),
    };

    let response = apply::apply(&Sysfs::new(), &request);
    print(&response);

    if response.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

fn fail(message: String) -> ExitCode {
    print(&ApplyResponse::error(message));
    ExitCode::from(2)
}

fn print(response: &ApplyResponse) {
    match serde_json::to_string(response) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            println!("{{\"ok\":false,\"errors\":[\"cannot serialize response: {error}\"]}}")
        }
    }
}

fn report_exit(report: &Report) -> ExitCode {
    match serde_json::to_string(report) {
        Ok(json) => println!("{json}"),
        Err(error) => return fail_takeover(format!("cannot serialize report: {error}")),
    }
    if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

fn fail_takeover(message: String) -> ExitCode {
    let report = Report {
        errors: vec![message],
        ..Default::default()
    };
    match serde_json::to_string(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => println!("{{\"errors\":[\"cannot serialize report: {error}\"]}}"),
    }
    ExitCode::from(2)
}
