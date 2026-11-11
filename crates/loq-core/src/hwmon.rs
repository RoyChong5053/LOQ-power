use serde::Serialize;

use crate::sysfs::Sysfs;

#[derive(Debug, Clone, Serialize)]
pub struct Sensor {
    pub label: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HwmonDevice {
    pub name: String,
    pub path: String,
    pub temperatures: Vec<Sensor>,
    pub fans: Vec<Sensor>,
    pub powers: Vec<Sensor>,
}

fn label_for(fs: &Sysfs, base: &str, prefix: &str, fallback: &str) -> String {
    fs.read(&format!("{base}/{prefix}_label"))
        .unwrap_or_else(|| fallback.to_string())
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

        sensors.push(Sensor {
            label: label_for(fs, base, prefix, prefix),
            value: raw / scale,
            unit: unit.to_string(),
        });
    }

    sensors
}

pub fn detect(fs: &Sysfs) -> Vec<HwmonDevice> {
    let mut devices = Vec::new();

    for entry in fs.list_dir("/sys/class/hwmon") {
        let base = format!("/sys/class/hwmon/{entry}");
        let Some(name) = fs.read(&format!("{base}/name")) else {
            continue;
        };

        devices.push(HwmonDevice {
            name,
            path: base.clone(),
            temperatures: read_sensors(fs, &base, "temp", "°C", 1000.0),
            fans: read_sensors(fs, &base, "fan", "RPM", 1.0),
            powers: read_sensors(fs, &base, "power", "W", 1_000_000.0),
        });
    }

    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices
}
