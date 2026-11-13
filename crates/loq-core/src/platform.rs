use serde::Serialize;

use crate::error::{Error, Result};
use crate::sysfs::Sysfs;
use crate::write::{self, WriteReport};

/// Thermal / platform profile state.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PlatformInfo {
    /// Effective profile: gamezone first, then the generic ACPI interface.
    pub current: Option<String>,
    pub choices: Vec<String>,
    /// Path of the gamezone `profile` attribute (the only one that accepts
    /// `custom` on this machine).
    pub gamezone_profile: Option<String>,
    /// Generic `/sys/firmware/acpi/platform_profile` value.
    pub acpi_current: Option<String>,
    pub acpi_choices: Vec<String>,
}

const GAMEZONE_PATTERN: &str =
    "/sys/bus/wmi/drivers/lenovo_wmi_gamezone/*/platform-profile/platform-profile-0/profile";

pub fn find_gamezone_profile(fs: &Sysfs) -> Option<String> {
    fs.glob(GAMEZONE_PATTERN)
        .into_iter()
        .next()
        .map(|path| fs.display_path(&path))
}

pub fn detect(fs: &Sysfs) -> PlatformInfo {
    let gamezone_profile = find_gamezone_profile(fs);

    let current = gamezone_profile
        .as_deref()
        .and_then(|path| fs.read(path))
        .or_else(|| fs.read("/sys/firmware/acpi/platform_profile"));

    let choices = gamezone_profile
        .as_deref()
        .and_then(|path| {
            let dir = path.rsplit_once('/').map(|(dir, _)| dir.to_string())?;
            fs.read(&format!("{dir}/choices"))
        })
        .map(|value| value.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();

    let acpi_current = fs.read("/sys/firmware/acpi/platform_profile");
    let acpi_choices = fs
        .read("/sys/firmware/acpi/platform_profile_choices")
        .map(|value| value.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();

    PlatformInfo {
        current,
        choices,
        gamezone_profile,
        acpi_current,
        acpi_choices,
    }
}

pub fn is_custom(fs: &Sysfs) -> bool {
    detect(fs).current.as_deref() == Some("custom")
}

/// Fan basis used before entering CUSTOM.
///
/// Switching to `custom` does not change the EC fan policy, so the fan keeps
/// whatever `Fn+Q` last selected. `performance`/`max-power` pin the fans at
/// full speed on Linux, so `balanced` is the only sane base.
pub const CUSTOM_FAN_BASIS: &str = "balanced";

/// Switch the thermal mode through the gamezone interface.
pub fn set_profile(fs: &Sysfs, profile: &str, dry_run: bool) -> Result<WriteReport> {
    let info = detect(fs);
    let path = info
        .gamezone_profile
        .as_deref()
        .ok_or_else(|| Error::NotFound("gamezone platform profile".to_string()))?;

    if !info.choices.is_empty() && !info.choices.iter().any(|choice| choice == profile) {
        return Err(Error::InvalidValue(format!(
            "unknown thermal mode {profile}; expected one of {}",
            info.choices.join(", ")
        )));
    }

    write::write_verified(fs, path, profile, dry_run, write::exact)
}

/// Enter CUSTOM mode with a balanced fan basis, unless already there.
///
/// Returns the reports for the writes that were performed; an empty vector
/// means the system was already in CUSTOM mode.
pub fn ensure_custom(fs: &Sysfs, dry_run: bool) -> Result<Vec<WriteReport>> {
    if is_custom(fs) {
        return Ok(Vec::new());
    }

    let mut reports = vec![set_profile(fs, CUSTOM_FAN_BASIS, dry_run)?];
    reports.push(set_profile(fs, "custom", dry_run)?);

    if !dry_run && !is_custom(fs) {
        return Err(Error::Other(
            "switched the platform profile but CUSTOM mode was not confirmed".to_string(),
        ));
    }

    Ok(reports)
}
