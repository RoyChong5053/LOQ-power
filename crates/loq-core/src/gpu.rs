use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub temperature_c: Option<f64>,
    pub utilization_percent: Option<f64>,
    pub memory_utilization_percent: Option<f64>,
    pub memory_used_mib: Option<f64>,
    pub memory_total_mib: Option<f64>,
    pub power_draw_w: Option<f64>,
    pub power_limit_w: Option<f64>,
    pub power_min_limit_w: Option<f64>,
    pub power_max_limit_w: Option<f64>,
    pub power_default_limit_w: Option<f64>,
    pub clocks_graphics_mhz: Option<f64>,
    pub clocks_max_graphics_mhz: Option<f64>,
    pub clocks_memory_mhz: Option<f64>,
    pub clocks_max_memory_mhz: Option<f64>,
}

fn parse_number(value: &str) -> Option<f64> {
    let value = value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    if value.eq_ignore_ascii_case("N/A") || value.is_empty() {
        return None;
    }
    value.parse().ok()
}

/// Query the first NVIDIA GPU with a single `nvidia-smi` call.
pub fn detect() -> Option<GpuInfo> {
    let fields = [
        "name",
        "temperature.gpu",
        "utilization.gpu",
        "utilization.memory",
        "memory.used",
        "memory.total",
        "power.draw",
        "clocks.gr",
        "clocks.max.gr",
        "clocks.mem",
        "clocks.max.mem",
        "power.limit",
        "power.min_limit",
        "power.max_limit",
        "power.default_limit",
    ];

    let output = Command::new("nvidia-smi")
        .arg(format!("--query-gpu={}", fields.join(",")))
        .args(["--format=csv,noheader,nounits"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }

    let columns: Vec<&str> = line.split(',').map(str::trim).collect();
    if columns.len() < fields.len() {
        return None;
    }

    Some(GpuInfo {
        name: columns[0].to_string(),
        temperature_c: parse_number(columns[1]),
        utilization_percent: parse_number(columns[2]),
        memory_utilization_percent: parse_number(columns[3]),
        memory_used_mib: parse_number(columns[4]),
        memory_total_mib: parse_number(columns[5]),
        power_draw_w: parse_number(columns[6]),
        clocks_graphics_mhz: parse_number(columns[7]),
        clocks_max_graphics_mhz: parse_number(columns[8]),
        clocks_memory_mhz: parse_number(columns[9]),
        clocks_max_memory_mhz: parse_number(columns[10]),
        power_limit_w: parse_number(columns[11]),
        power_min_limit_w: parse_number(columns[12]),
        power_max_limit_w: parse_number(columns[13]),
        power_default_limit_w: parse_number(columns[14]),
    })
}
