//! Guarded sysfs writes.
//!
//! Every helper in this module validates its input, refuses to touch paths
//! that do not exist, and supports a dry-run mode so the GUI/CLI can preview
//! a change without root privileges.

use crate::error::{Error, Result};
use crate::sysfs::Sysfs;

/// Result of a single attribute write.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WriteReport {
    pub target: String,
    pub path: String,
    pub previous: Option<String>,
    pub value: String,
    pub dry_run: bool,
    pub verified: bool,
}

/// Write `value` to `path`, then read it back and compare.
///
/// `compare` decides whether a readback counts as success; several sysfs
/// attributes normalize values (for example `charge_types` and `fan_mode`).
pub fn write_verified(
    fs: &Sysfs,
    path: &str,
    value: &str,
    dry_run: bool,
    compare: impl Fn(&str, &str) -> bool,
) -> Result<WriteReport> {
    if !fs.exists(path) {
        return Err(Error::NotFound(path.to_string()));
    }

    let previous = fs.read(path);
    if dry_run {
        return Ok(WriteReport {
            target: path.to_string(),
            path: path.to_string(),
            previous,
            value: value.to_string(),
            dry_run: true,
            verified: false,
        });
    }

    let full = fs.path(path);
    std::fs::write(&full, format!("{value}\n")).map_err(|source| Error::Write {
        path: full.clone(),
        source,
    })?;

    let actual = fs.read(path).unwrap_or_default();
    let verified = compare(&actual, value);
    if !verified {
        return Err(Error::Verification {
            path: path.to_string(),
            expected: value.to_string(),
            actual,
        });
    }

    Ok(WriteReport {
        target: path.to_string(),
        path: path.to_string(),
        previous,
        value: value.to_string(),
        dry_run: false,
        verified,
    })
}

/// Write `value` to `path` without requiring the readback to match.
///
/// Needed for attributes whose readback is known to be unreliable, such as
/// the ideapad `fan_mode` node.
pub fn write_tolerant(fs: &Sysfs, path: &str, value: &str, dry_run: bool) -> Result<WriteReport> {
    if !fs.exists(path) {
        return Err(Error::NotFound(path.to_string()));
    }

    let previous = fs.read(path);
    if dry_run {
        return Ok(WriteReport {
            target: path.to_string(),
            path: path.to_string(),
            previous,
            value: value.to_string(),
            dry_run: true,
            verified: false,
        });
    }

    let full = fs.path(path);
    std::fs::write(&full, format!("{value}\n"))
        .map_err(|source| Error::Write { path: full, source })?;

    let actual = fs.read(path).unwrap_or_default();
    Ok(WriteReport {
        target: path.to_string(),
        path: path.to_string(),
        previous,
        value: value.to_string(),
        dry_run: false,
        verified: exact(&actual, value),
    })
}

/// Exact (case-insensitive, whitespace-trimmed) comparison.
pub fn exact(actual: &str, expected: &str) -> bool {
    actual.trim().eq_ignore_ascii_case(expected.trim())
}

/// Comparison for attributes such as `charge_types` that echo the full
/// choice list with the active entry wrapped in brackets.
///
/// Only the bracketed entry counts; a plain value is compared exactly.
pub fn current_choice(actual: &str, expected: &str) -> bool {
    match actual
        .split_whitespace()
        .find(|token| token.starts_with('['))
    {
        Some(active) => active
            .trim_matches(['[', ']'])
            .eq_ignore_ascii_case(expected),
        None => exact(actual, expected),
    }
}

/// Check an integer against optional bounds and step.
pub fn check_range(
    what: &str,
    value: i64,
    min: Option<i64>,
    max: Option<i64>,
    step: Option<i64>,
) -> Result<()> {
    if let Some(min) = min
        && value < min
    {
        return Err(Error::OutOfRange {
            what: what.to_string(),
            value,
            min,
            max: max.unwrap_or(min),
        });
    }
    if let Some(max) = max
        && value > max
    {
        return Err(Error::OutOfRange {
            what: what.to_string(),
            value,
            min: min.unwrap_or(max),
            max,
        });
    }
    if let Some(step) = step.filter(|step| *step > 1) {
        let base = min.unwrap_or(0);
        if (value - base).rem_euclid(step) != 0 {
            return Err(Error::InvalidStep {
                what: what.to_string(),
                value,
                step,
            });
        }
    }
    Ok(())
}
