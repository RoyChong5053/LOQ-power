use serde::Serialize;

use crate::sysfs::Sysfs;

#[derive(Debug, Clone, Default, Serialize)]
pub struct BatteryInfo {
    pub name: String,
    pub capacity: Option<i64>,
    pub status: Option<String>,
    pub voltage_v: Option<f64>,
    pub energy_now_wh: Option<f64>,
    pub energy_full_wh: Option<f64>,
    pub energy_full_design_wh: Option<f64>,
    pub power_w: Option<f64>,
    pub cycle_count: Option<i64>,
    pub charge_type: Option<String>,
    pub charge_types: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PowerSupplyInfo {
    pub batteries: Vec<BatteryInfo>,
    pub ac_online: Option<bool>,
    pub ac_name: Option<String>,
}

/// Parse `"Fast Standard [Long_Life]"` into `(Some("Long_Life"), [Fast, Standard, Long_Life])`.
pub fn parse_charge_types(value: &str) -> (Option<String>, Vec<String>) {
    let mut current = None;
    let mut choices = Vec::new();

    for token in value.split_whitespace() {
        if let Some(stripped) = token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            current = Some(stripped.to_string());
            choices.push(stripped.to_string());
        } else {
            choices.push(token.to_string());
        }
    }

    (current, choices)
}

fn read_battery(fs: &Sysfs, name: &str) -> BatteryInfo {
    let base = format!("/sys/class/power_supply/{name}");
    let charge_types_raw = fs.read(&format!("{base}/charge_types"));
    let (charge_type, charge_types) = charge_types_raw
        .as_deref()
        .map(parse_charge_types)
        .unwrap_or_default();

    BatteryInfo {
        name: name.to_string(),
        capacity: fs.read_i64(&format!("{base}/capacity")),
        status: fs.read(&format!("{base}/status")),
        voltage_v: fs
            .read_f64(&format!("{base}/voltage_now"))
            .map(|value| value / 1_000_000.0),
        energy_now_wh: fs
            .read_f64(&format!("{base}/energy_now"))
            .map(|value| value / 1_000_000.0),
        energy_full_wh: fs
            .read_f64(&format!("{base}/energy_full"))
            .map(|value| value / 1_000_000.0),
        energy_full_design_wh: fs
            .read_f64(&format!("{base}/energy_full_design"))
            .map(|value| value / 1_000_000.0),
        power_w: fs
            .read_f64(&format!("{base}/power_now"))
            .map(|value| value / 1_000_000.0),
        cycle_count: fs.read_i64(&format!("{base}/cycle_count")),
        charge_type,
        charge_types,
    }
}

pub fn detect(fs: &Sysfs) -> PowerSupplyInfo {
    let mut batteries = Vec::new();
    let mut ac_online = None;
    let mut ac_name = None;

    for entry in fs.list_dir("/sys/class/power_supply") {
        let base = format!("/sys/class/power_supply/{entry}");
        match fs.read(&format!("{base}/type")).as_deref() {
            Some("Battery") => batteries.push(read_battery(fs, &entry)),
            Some("Mains") => {
                ac_name = Some(entry.clone());
                ac_online = fs.read(&format!("{base}/online")).map(|value| value == "1");
            }
            _ => {}
        }
    }

    PowerSupplyInfo {
        batteries,
        ac_online,
        ac_name,
    }
}
