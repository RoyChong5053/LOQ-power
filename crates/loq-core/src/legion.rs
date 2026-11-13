//! Optional LenovoLegionLinux (LLL) integration.
//!
//! LLL is an out-of-tree kernel module. LOQ Power never loads it; the module is
//! only used when `legion_hwmon` already exists. On machines without it every
//! function here reports "not available" and the GUI hides the fan curve.
//!
//! Fan curves are written through the *standard hwmon attributes* the module
//! exposes (`pwmY_auto_pointZ_pwm`, `pwmY_auto_pointZ_temp`). The debugfs file
//! is read-only and is only used for display.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::hwmon::Sensor;
use crate::sysfs::Sysfs;
use crate::write::{self, WriteReport};

/// A fan curve as exposed by `legion_hwmon`.
///
/// `fan1_pwm` is fan 1, `fan2_pwm` fan 2. Temperatures are the upper threshold
/// of each point for the CPU/GPU/IC sensors. All vectors have `points` items.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FanCurve {
    pub points: u8,
    pub fan1_pwm: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fan2_pwm: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cpu_temp: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_temp: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ic_temp: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LegionInfo {
    pub installed: bool,
    pub hwmon_path: Option<String>,
    /// Read-only debugfs dump, when the module provides it.
    pub fancurve_path: Option<String>,
    pub fancurve: Option<String>,
    pub powermode_path: Option<String>,
    pub powermode: Option<String>,
    pub fullspeed_path: Option<String>,
    pub fullspeed: Option<bool>,
    pub fan_curve: Option<FanCurve>,
    pub temperatures: Vec<Sensor>,
    pub fans: Vec<Sensor>,
}

fn read_sensors(fs: &Sysfs, base: &str, kind: &str, unit: &str, scale: f64) -> Vec<Sensor> {
    let mut sensors = Vec::new();

    for name in fs.list_dir(base) {
        if !name.starts_with(kind) || !name.ends_with("_input") {
            continue;
        }
        let prefix = name.trim_end_matches("_input");
        let Some(raw) = fs.read_f64(&format!("{base}/{name}")) else {
            continue;
        };
        let label = fs
            .read(&format!("{base}/{prefix}_label"))
            .unwrap_or_else(|| prefix.to_string());
        sensors.push(Sensor {
            label,
            value: raw / scale,
            unit: unit.to_string(),
        });
    }

    sensors
}

fn legion_hwmon(fs: &Sysfs) -> Option<String> {
    for entry in fs.list_dir("/sys/class/hwmon") {
        let base = format!("/sys/class/hwmon/{entry}");
        if fs.read(&format!("{base}/name")).as_deref() == Some("legion_hwmon") {
            return Some(base);
        }
    }
    None
}

fn first_glob(fs: &Sysfs, patterns: &[&str]) -> Option<String> {
    for pattern in patterns {
        if let Some(path) = fs.glob(pattern).into_iter().next() {
            return Some(fs.display_path(&path));
        }
    }
    None
}

fn read_u8(fs: &Sysfs, path: &str) -> Option<u8> {
    fs.read(path)?.trim().parse().ok()
}

/// Read the curve from the hwmon auto-point attributes.
fn read_curve(fs: &Sysfs, base: &str) -> Option<FanCurve> {
    let points = (1..=10)
        .take_while(|point| fs.exists(&format!("{base}/pwm1_auto_point{point}_pwm")))
        .count() as u8;
    if points == 0 {
        return None;
    }

    let mut fan1_pwm = Vec::with_capacity(points as usize);
    let mut fan2_pwm = Vec::new();
    let mut cpu_temp = Vec::new();
    let mut gpu_temp = Vec::new();
    let mut ic_temp = Vec::new();

    for point in 1..=points {
        fan1_pwm.push(read_u8(fs, &format!("{base}/pwm1_auto_point{point}_pwm"))?);
        if fs.exists(&format!("{base}/pwm2_auto_point1_pwm")) {
            fan2_pwm.push(read_u8(fs, &format!("{base}/pwm2_auto_point{point}_pwm"))?);
        }
        if fs.exists(&format!("{base}/pwm1_auto_point1_temp")) {
            cpu_temp.push(read_u8(fs, &format!("{base}/pwm1_auto_point{point}_temp"))?);
        }
        if fs.exists(&format!("{base}/pwm2_auto_point1_temp")) {
            gpu_temp.push(read_u8(fs, &format!("{base}/pwm2_auto_point{point}_temp"))?);
        }
        if fs.exists(&format!("{base}/pwm3_auto_point1_temp")) {
            ic_temp.push(read_u8(fs, &format!("{base}/pwm3_auto_point{point}_temp"))?);
        }
    }

    Some(FanCurve {
        points,
        fan1_pwm,
        fan2_pwm: (!fan2_pwm.is_empty()).then_some(fan2_pwm),
        cpu_temp,
        gpu_temp: (!gpu_temp.is_empty()).then_some(gpu_temp),
        ic_temp: (!ic_temp.is_empty()).then_some(ic_temp),
    })
}

/// Detect the optional out-of-tree LenovoLegionLinux driver.
pub fn detect(fs: &Sysfs) -> LegionInfo {
    let mut info = LegionInfo::default();

    if let Some(base) = legion_hwmon(fs) {
        info.installed = true;
        info.temperatures = read_sensors(fs, &base, "temp", "°C", 1000.0);
        info.fans = read_sensors(fs, &base, "fan", "RPM", 1.0);
        info.fan_curve = read_curve(fs, &base);
        info.hwmon_path = Some(base);
    }

    let fancurve = "/sys/kernel/debug/legion/fancurve";
    if fs.exists(fancurve) {
        info.installed = true;
        info.fancurve_path = Some(fancurve.to_string());
        info.fancurve = fs.read(fancurve);
    }

    if let Some(path) = first_glob(
        fs,
        &[
            "/sys/module/legion_laptop/drivers/*/*/powermode",
            "/sys/bus/platform/devices/*/powermode",
        ],
    ) {
        info.installed = true;
        info.powermode = fs.read(&path);
        info.powermode_path = Some(path);
    }

    if let Some(path) = first_glob(
        fs,
        &[
            "/sys/module/legion_laptop/drivers/*/*/fan_fullspeed",
            "/sys/bus/platform/devices/*/fan_fullspeed",
        ],
    ) {
        info.installed = true;
        info.fullspeed = fs.read(&path).map(|raw| raw.trim() == "1");
        info.fullspeed_path = Some(path);
    }

    info
}

/// Enable or disable the firmware's "fan full speed" switch.
pub fn set_fullspeed(fs: &Sysfs, enabled: bool, dry_run: bool) -> Result<WriteReport> {
    let path = first_glob(
        fs,
        &[
            "/sys/module/legion_laptop/drivers/*/*/fan_fullspeed",
            "/sys/bus/platform/devices/*/fan_fullspeed",
        ],
    )
    .ok_or_else(|| Error::NotFound("legion fan_fullspeed".to_string()))?;

    write::write_verified(
        fs,
        &path,
        if enabled { "1" } else { "0" },
        dry_run,
        write::exact,
    )
}

/// Set the LLL power mode (0 balanced, 1 performance, 2 quiet).
///
/// LLL reloads the firmware fan curve when this changes, so callers that also
/// apply a curve must do so afterwards.
pub fn set_powermode(fs: &Sysfs, mode: i64, dry_run: bool) -> Result<WriteReport> {
    if !(0..=2).contains(&mode) {
        return Err(Error::InvalidValue(format!(
            "legion powermode must be 0, 1 or 2, not {mode}"
        )));
    }
    let path = first_glob(
        fs,
        &[
            "/sys/module/legion_laptop/drivers/*/*/powermode",
            "/sys/bus/platform/devices/*/powermode",
        ],
    )
    .ok_or_else(|| Error::NotFound("legion powermode".to_string()))?;

    write::write_verified(fs, &path, &mode.to_string(), dry_run, write::exact)
}

/// LLL rounds PWM writes to a fan level on some models (`u = 5`). Accept a
/// readback that matches the nearest level so verification does not fail on
/// hardware where the value is quantized by the firmware.
fn pwm_matches(actual: &str, expected: &str) -> bool {
    if write::exact(actual, expected) {
        return true;
    }
    let (Ok(actual), Ok(expected)) = (actual.trim().parse::<i64>(), expected.trim().parse::<i64>())
    else {
        return false;
    };
    let level = (expected as f64 * 10.0 / 255.0).round();
    let quantized = (level * 255.0 / 10.0).round() as i64;
    actual == quantized
}

fn check_vector(name: &str, values: &[u8], points: u8, max: u8) -> Result<()> {
    if values.len() != points as usize {
        return Err(Error::InvalidValue(format!(
            "{name} has {} values but the curve has {points} points",
            values.len()
        )));
    }
    if let Some(value) = values.iter().find(|value| **value > max) {
        return Err(Error::OutOfRange {
            what: name.to_string(),
            value: i64::from(*value),
            min: 0,
            max: i64::from(max),
        });
    }
    if values.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(Error::InvalidValue(format!(
            "{name} must not decrease from one point to the next"
        )));
    }
    Ok(())
}

/// Write a complete fan curve.
///
/// Temperatures are written before fan speeds so the fan never ramps before
/// the thresholds it is supposed to follow. Every write is verified.
pub fn apply_curve(fs: &Sysfs, curve: &FanCurve, dry_run: bool) -> Result<Vec<WriteReport>> {
    let base =
        legion_hwmon(fs).ok_or_else(|| Error::NotFound("legion_hwmon fan curve".to_string()))?;

    let live = read_curve(fs, &base)
        .ok_or_else(|| Error::NotFound(format!("{base}/pwm1_auto_point1_pwm")))?;
    if curve.points == 0 || curve.points != live.points {
        return Err(Error::InvalidValue(format!(
            "curve has {} points but the hardware expects {}",
            curve.points, live.points
        )));
    }

    check_vector("fan1_pwm", &curve.fan1_pwm, curve.points, 255)?;
    if let Some(values) = &curve.fan2_pwm {
        check_vector("fan2_pwm", values, curve.points, 255)?;
    }
    if live.gpu_temp.is_some() != curve.gpu_temp.is_some()
        || live.ic_temp.is_some() != curve.ic_temp.is_some()
    {
        return Err(Error::InvalidValue(
            "curve does not match the temperature sensors this machine exposes".to_string(),
        ));
    }
    check_vector("cpu_temp", &curve.cpu_temp, curve.points, 127)?;
    if let Some(values) = &curve.gpu_temp {
        check_vector("gpu_temp", values, curve.points, 127)?;
    }
    if let Some(values) = &curve.ic_temp {
        check_vector("ic_temp", values, curve.points, 127)?;
    }

    let mut reports = Vec::new();

    for (sensor, values) in [
        ("pwm1", Some(&curve.cpu_temp)),
        ("pwm2", curve.gpu_temp.as_ref()),
        ("pwm3", curve.ic_temp.as_ref()),
    ] {
        let Some(values) = values else { continue };
        for (index, value) in values.iter().enumerate() {
            let point = index + 1;
            let path = format!("{base}/{sensor}_auto_point{point}_temp");
            reports.push(write::write_verified(
                fs,
                &path,
                &value.to_string(),
                dry_run,
                write::exact,
            )?);
        }
    }

    let mut fans: Vec<(&str, &Vec<u8>)> = vec![("pwm1", &curve.fan1_pwm)];
    if let Some(values) = &curve.fan2_pwm {
        fans.push(("pwm2", values));
    }
    for (sensor, values) in fans {
        for (index, value) in values.iter().enumerate() {
            let point = index + 1;
            let path = format!("{base}/{sensor}_auto_point{point}_pwm");
            reports.push(write::write_verified(
                fs,
                &path,
                &value.to_string(),
                dry_run,
                pwm_matches,
            )?);
        }
    }

    Ok(reports)
}

/// Persist a curve as TOML.
pub fn save_curve(curve: &FanCurve, path: &std::path::Path) -> Result<()> {
    let content = toml::to_string_pretty(curve)
        .map_err(|error| Error::Other(format!("cannot serialize fan curve: {error}")))?;
    std::fs::write(path, content).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Load a curve from TOML.
pub fn load_curve(path: &std::path::Path) -> Result<FanCurve> {
    let content = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&content)
        .map_err(|error| Error::InvalidValue(format!("cannot parse {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::pwm_matches;

    #[test]
    fn accepts_exact_and_level_quantized_pwm() {
        assert!(pwm_matches("70\n", "70"));
        // u = 5 hardware rounds 70 to level 3 and reads back 77.
        assert!(pwm_matches("77\n", "70"));
        // 255 is a valid level and stays exact.
        assert!(pwm_matches("255\n", "255"));
    }

    #[test]
    fn rejects_unrelated_pwm_readback() {
        assert!(!pwm_matches("200\n", "70"));
        assert!(!pwm_matches("not a number", "70"));
    }
}
