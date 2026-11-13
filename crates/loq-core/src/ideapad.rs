use std::os::unix::fs::PermissionsExt;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::sysfs::Sysfs;
use crate::write::{self, WriteReport};

#[derive(Debug, Clone, Serialize)]
pub struct IdeapadAttribute {
    pub name: String,
    pub value: Option<String>,
    pub writable: bool,
    pub write_requires_root: bool,
    pub known: bool,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IdeapadInfo {
    pub base: Option<String>,
    pub attributes: Vec<IdeapadAttribute>,
}

const KNOWN: &[(&str, &str)] = &[
    (
        "fan_mode",
        "EC 风扇模式：0 静音 / 1 标准 / 2 除尘 / 4 强散热（读回值可能不可靠，且重启不保留）",
    ),
    ("fn_lock", "Fn Lock 状态"),
    (
        "conservation_mode",
        "电池保养模式（已弃用，等价于 charge_types 的 Long_Life）",
    ),
    ("camera_power", "摄像头供电"),
    ("touchpad", "触摸板供电（部分机型需要模块参数）"),
    ("usb_charging", "关机 USB 充电"),
];

/// Infrastructure files exposed by the driver core; not useful as controls.
const SKIP: &[&str] = &[
    "driver",
    "driver_override",
    "modalias",
    "power",
    "subsystem",
    "uevent",
];

fn known(name: &str) -> Option<(&'static str, &'static str)> {
    KNOWN.iter().find(|(attr, _)| *attr == name).copied()
}

pub fn find_base(fs: &Sysfs) -> Option<String> {
    const PATTERNS: &[&str] = &[
        "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:*",
        "/sys/bus/platform/devices/VPC2004:*",
    ];

    for pattern in PATTERNS {
        if let Some(path) = fs.glob(pattern).into_iter().next() {
            return Some(fs.display_path(&path));
        }
    }

    None
}

pub fn detect(fs: &Sysfs) -> IdeapadInfo {
    let Some(base) = find_base(fs) else {
        return IdeapadInfo::default();
    };

    let mut attributes = Vec::new();

    for name in fs.list_dir(&base) {
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        let path = format!("{base}/{name}");
        let full = fs.path(&path);

        let Ok(metadata) = std::fs::metadata(&full) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }

        let value = fs.read(&path);
        let mode = metadata.permissions().mode();
        let writable = mode & 0o222 != 0;
        let (known, description) = match known(&name) {
            Some((_, description)) => (true, description.to_string()),
            None => (false, String::new()),
        };

        attributes.push(IdeapadAttribute {
            name,
            value,
            writable,
            write_requires_root: true,
            known,
            description,
            path,
        });
    }

    IdeapadInfo {
        base: Some(base),
        attributes,
    }
}

/// Documented values for `fan_mode`.
pub const FAN_MODES: &[(i64, &str)] = &[
    (0, "Super Silent"),
    (1, "Standard"),
    (2, "Dust Cleaning"),
    (4, "Efficient Thermal Dissipation"),
];

pub fn fan_mode_label(mode: i64) -> String {
    FAN_MODES
        .iter()
        .find(|(value, _)| *value == mode)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| format!("unknown ({mode})"))
}

fn attribute_path(fs: &Sysfs, name: &str) -> Result<String> {
    let base =
        find_base(fs).ok_or_else(|| Error::NotFound("ideapad_acpi interface".to_string()))?;
    let path = format!("{base}/{name}");
    if !fs.exists(&path) {
        return Err(Error::NotFound(format!("ideapad attribute {name}")));
    }
    Ok(path)
}

/// Write one of the whitelisted ideapad attributes.
pub fn set_known(fs: &Sysfs, name: &str, value: &str, dry_run: bool) -> Result<WriteReport> {
    if known(name).is_none() {
        return Err(Error::NotWritable(format!(
            "ideapad attribute {name} is not in the safe write list"
        )));
    }
    let path = attribute_path(fs, name)?;
    write::write_verified(fs, &path, value, dry_run, write::exact)
}

/// Change the EC fan mode.
///
/// The readback is known to be unreliable on this generation, so a mismatch
/// is reported but does not fail the operation. `2` (dust cleaning) and `4`
/// (full speed) are loud; the callers must confirm them explicitly.
pub fn set_fan_mode(fs: &Sysfs, mode: i64, dry_run: bool) -> Result<WriteReport> {
    if !FAN_MODES.iter().any(|(value, _)| *value == mode) {
        return Err(Error::InvalidValue(format!(
            "unknown fan mode {mode}; expected one of 0, 1, 2, 4"
        )));
    }

    let path = attribute_path(fs, "fan_mode")?;
    write::write_tolerant(fs, &path, &mode.to_string(), dry_run)
}

/// Toggle `fn_lock` or `conservation_mode`.
pub fn set_flag(fs: &Sysfs, name: &str, enabled: bool, dry_run: bool) -> Result<WriteReport> {
    if name != "fn_lock" && name != "conservation_mode" {
        return Err(Error::NotWritable(format!(
            "ideapad flag {name} is not supported"
        )));
    }
    let path = attribute_path(fs, name)?;
    write::write_verified(
        fs,
        &path,
        if enabled { "1" } else { "0" },
        dry_run,
        write::exact,
    )
}
