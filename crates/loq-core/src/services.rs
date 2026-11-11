use std::process::Command;

/// Run a command and return trimmed stdout when it succeeds.
pub fn output(command: &str, args: &[&str]) -> Option<String> {
    let result = Command::new(command).args(args).output().ok()?;
    if !result.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

pub fn is_active(unit: &str) -> Option<bool> {
    output("systemctl", &["is-active", unit]).map(|value| value == "active")
}

pub fn is_enabled(unit: &str) -> Option<bool> {
    output("systemctl", &["is-enabled", unit]).map(|value| value == "enabled")
}

pub fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .map(|result| result.status.success())
        .unwrap_or(false)
}
