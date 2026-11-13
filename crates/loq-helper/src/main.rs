//! `loq-power-helper` — the privileged fallback used when `loq-powerd` is not
//! running.
//!
//! Protocol: a single JSON [`ApplyRequest`] on stdin, a JSON
//! [`ApplyResponse`] on stdout.
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
