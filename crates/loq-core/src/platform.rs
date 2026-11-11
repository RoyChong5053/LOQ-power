use serde::Serialize;

use crate::sysfs::Sysfs;

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
