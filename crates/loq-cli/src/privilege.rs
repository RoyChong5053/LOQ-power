//! Privileged execution.
//!
//! When the process is root (the systemd daemon case, or `sudo loq-power ...`)
//! the request runs in-process. Otherwise a dry run is served locally and a
//! real write goes through `pkexec loq-power-helper`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use loq_core::request::{ApplyRequest, ApplyResponse};
use loq_core::{Sysfs, apply};

pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

fn find_helper() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("LOQ_POWER_HELPER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "LOQ_POWER_HELPER points to {}, which is not a file",
            path.display()
        );
    }

    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join("loq-power-helper"))
                .collect()
        })
        .unwrap_or_default();
    candidates.push(PathBuf::from("/usr/local/bin/loq-power-helper"));
    candidates.push(PathBuf::from("/usr/bin/loq-power-helper"));

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "loq-power-helper is not installed.\n\
                 Install it, or run this command as root, or start loq-powerd."
            )
        })
}

/// Run `request` with the least privilege that can satisfy it.
pub fn execute(request: ApplyRequest) -> Result<ApplyResponse> {
    if request.dry_run || is_root() {
        return Ok(apply::apply(&Sysfs::new(), &request));
    }

    let helper = find_helper()?;
    let payload = serde_json::to_vec(&request).context("cannot serialize request")?;

    let mut child = Command::new("pkexec")
        .arg(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run pkexec {}", helper.display()))?;

    child
        .stdin
        .as_mut()
        .context("pkexec did not provide stdin")?
        .write_all(&payload)
        .context("cannot send request to the helper")?;

    let output = child.wait_with_output().context("helper did not finish")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed = stdout.trim();

    if trimmed.is_empty() {
        bail!(
            "helper returned no output{}",
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            }
        );
    }

    let response: ApplyResponse = serde_json::from_str(trimmed)
        .with_context(|| format!("cannot parse helper response: {}", trimmed))?;

    if !output.status.success() && response.ok {
        bail!(
            "helper failed without reporting an error: {}",
            stderr.trim()
        );
    }

    Ok(response)
}
