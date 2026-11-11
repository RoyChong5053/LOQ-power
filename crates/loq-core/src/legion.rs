use serde::Serialize;

use crate::hwmon::Sensor;
use crate::sysfs::Sysfs;

#[derive(Debug, Clone, Default, Serialize)]
pub struct LegionInfo {
    pub installed: bool,
    pub hwmon_path: Option<String>,
    pub fancurve_path: Option<String>,
    pub fancurve: Option<String>,
    pub powermode_path: Option<String>,
    pub powermode: Option<String>,
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

/// Detect the optional out-of-tree LenovoLegionLinux driver.
pub fn detect(fs: &Sysfs) -> LegionInfo {
    let mut info = LegionInfo::default();

    for entry in fs.list_dir("/sys/class/hwmon") {
        let base = format!("/sys/class/hwmon/{entry}");
        if fs.read(&format!("{base}/name")).as_deref() == Some("legion_hwmon") {
            info.installed = true;
            info.hwmon_path = Some(base.clone());
            info.temperatures = read_sensors(fs, &base, "temp", "°C", 1000.0);
            info.fans = read_sensors(fs, &base, "fan", "RPM", 1.0);
            break;
        }
    }

    let fancurve = "/sys/kernel/debug/legion/fancurve";
    if fs.exists(fancurve) {
        info.installed = true;
        info.fancurve_path = Some(fancurve.to_string());
        info.fancurve = fs.read(fancurve);
    }

    if let Some(path) = fs
        .glob("/sys/module/legion_laptop/drivers/*/*/powermode")
        .into_iter()
        .next()
    {
        info.installed = true;
        let display = fs.display_path(&path);
        info.powermode = fs.read(&display);
        info.powermode_path = Some(display);
    }

    info
}
