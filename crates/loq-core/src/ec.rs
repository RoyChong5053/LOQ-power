use serde::Serialize;

use crate::error::{Error, Result};
use crate::platform;
use crate::sysfs::Sysfs;
use crate::write::{self, WriteReport};

/// One EC firmware attribute exposed by `lenovo-wmi-other`.
#[derive(Debug, Clone, Serialize)]
pub struct EcAttribute {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub unit: String,
    pub current: Option<i64>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub default: Option<i64>,
    pub step: Option<i64>,
    /// Whether the kernel exposes a write path. Actual writes still require
    /// root and CUSTOM thermal mode.
    pub writable: bool,
    pub write_requires_root: bool,
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EcInfo {
    pub base: Option<String>,
    pub attributes: Vec<EcAttribute>,
}

/// Known attributes, in the order the UI should show them.
const KNOWN: &[(&str, &str, &str, &str)] = &[
    ("gpu_nv_ctgp", "GPU cTGP", "可配置总图形功率", "W"),
    ("gpu_nv_ppab", "GPU PPAB", "功率加速", "W"),
    (
        "gpu_nv_ac_offset",
        "GPU AC Offset",
        "适配器功率偏移(仅插电有效)",
        "W",
    ),
    ("gpu_nv_cpu_boost", "GPU→CPU Boost", "动态加速", "W"),
    ("gpu_temp", "GPU 温度限制", "GPU 热负载限制", "°C"),
    ("ppt_pl1_spl", "CPU PL1", "持续功率限制", "W"),
    ("ppt_pl2_sppt", "CPU PL2", "短时功率限制", "W"),
    ("ppt_pl3_fppt", "CPU PL3", "快速功率跟踪", "W"),
    ("ppt_cpu_cl", "CPU 交叉负载", "交叉负载功率", "W"),
    ("cpu_temp", "CPU 温度限制", "CPU 热负载限制", "°C"),
];

fn known(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    KNOWN
        .iter()
        .find(|(attr, _, _, _)| *attr == name)
        .map(|(_, display_name, description, unit)| (*display_name, *description, *unit))
}

fn infer_unit(name: &str) -> &'static str {
    if name.ends_with("_temp") {
        "°C"
    } else if name.starts_with("gpu_nv_") || name.starts_with("ppt_") {
        "W"
    } else {
        ""
    }
}

fn sort_key(name: &str) -> (usize, String) {
    match KNOWN.iter().position(|(attr, _, _, _)| *attr == name) {
        Some(index) => (index, String::new()),
        None => (KNOWN.len(), name.to_string()),
    }
}

/// Locate the `lenovo-wmi-other` attributes directory.
pub fn find_base(fs: &Sysfs) -> Option<String> {
    const PATTERNS: &[&str] = &[
        "/sys/class/firmware-attributes/lenovo-wmi-other-*/attributes",
        "/sys/devices/virtual/firmware-attributes/lenovo-wmi-other-*/attributes",
    ];

    for pattern in PATTERNS {
        if let Some(path) = fs.glob(pattern).into_iter().next() {
            return Some(fs.display_path(&path));
        }
    }

    None
}

fn read_attr(fs: &Sysfs, base: &str, name: &str) -> EcAttribute {
    let dir = format!("{base}/{name}");
    let current_path = format!("{dir}/current_value");
    let step = fs
        .read_i64(&format!("{dir}/scalar_increment"))
        .filter(|value| *value > 0);

    let (display_name, description, unit) = match known(name) {
        Some((display_name, description, unit)) => (
            display_name.to_string(),
            description.to_string(),
            unit.to_string(),
        ),
        None => (
            name.to_string(),
            String::new(),
            infer_unit(name).to_string(),
        ),
    };

    EcAttribute {
        name: name.to_string(),
        display_name,
        description,
        unit,
        current: fs.read_i64(&current_path),
        min: fs.read_i64(&format!("{dir}/min_value")),
        max: fs.read_i64(&format!("{dir}/max_value")),
        default: fs.read_i64(&format!("{dir}/default_value")),
        step,
        writable: fs.exists(&current_path),
        write_requires_root: true,
        path: current_path,
    }
}

/// Discover every EC attribute currently exposed by the kernel.
pub fn discover(fs: &Sysfs) -> EcInfo {
    let Some(base) = find_base(fs) else {
        return EcInfo::default();
    };

    let mut names = fs.list_dir(&base);
    names.sort_by_key(|name| sort_key(name));

    let attributes = names
        .into_iter()
        .filter(|name| fs.exists(&format!("{base}/{name}/current_value")))
        .map(|name| read_attr(fs, &base, &name))
        .collect();

    EcInfo {
        base: Some(base),
        attributes,
    }
}

/// Convenience lookup used by the daemon and CLI.
pub fn current(fs: &Sysfs, name: &str) -> Option<i64> {
    let base = find_base(fs)?;
    fs.read_i64(&format!("{base}/{name}/current_value"))
}

/// Look up a single attribute without walking the whole tree.
pub fn attribute(fs: &Sysfs, name: &str) -> Option<EcAttribute> {
    let base = find_base(fs)?;
    if !fs.exists(&format!("{base}/{name}/current_value")) {
        return None;
    }
    Some(read_attr(fs, &base, name))
}

/// Validate and write one EC attribute.
///
/// The kernel only accepts these writes while the platform is in the
/// `custom` thermal mode, so the guard runs before anything is touched.
pub fn set_attribute(fs: &Sysfs, name: &str, value: i64, dry_run: bool) -> Result<WriteReport> {
    let attribute =
        attribute(fs, name).ok_or_else(|| Error::NotFound(format!("EC attribute {name}")))?;

    write::check_range(
        &attribute.display_name,
        value,
        attribute.min,
        attribute.max,
        attribute.step,
    )?;

    if !dry_run && !platform::is_custom(fs) {
        return Err(Error::RequiresCustomMode(
            "switch to CUSTOM before writing EC limits".to_string(),
        ));
    }

    write::write_verified(
        fs,
        &attribute.path,
        &value.to_string(),
        dry_run,
        |actual, _| actual.trim().parse::<i64>().ok() == Some(value),
    )
}
