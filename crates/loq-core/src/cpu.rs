use std::collections::BTreeSet;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::sysfs::Sysfs;
use crate::write::{self, WriteReport};

#[derive(Debug, Clone, Default, Serialize)]
pub struct CpuInfo {
    pub cpus: usize,
    pub driver: Option<String>,
    pub governor: Option<String>,
    pub available_governors: Vec<String>,
    pub epp: Option<String>,
    pub available_epp: Vec<String>,
    pub scaling_min_khz: Option<u64>,
    pub scaling_max_khz: Option<u64>,
    pub scaling_cur_khz: Option<u64>,
    pub cpuinfo_min_khz: Option<u64>,
    pub cpuinfo_max_khz: Option<u64>,
    pub boost: Option<bool>,
    pub amd_pstate_status: Option<String>,
    pub dynamic_epp: Option<String>,
    pub prefcore: Option<String>,
}

fn cpu_count(fs: &Sysfs) -> usize {
    fs.list_dir("/sys/devices/system/cpu")
        .into_iter()
        .filter(|name| {
            name.strip_prefix("cpu")
                .map(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
        })
        .count()
}

pub fn detect(fs: &Sysfs) -> CpuInfo {
    let cpus = cpu_count(fs);
    let cpu0 = "/sys/devices/system/cpu/cpu0/cpufreq";

    let mut epp_values = BTreeSet::new();
    let mut governor_values = BTreeSet::new();
    for index in 0..cpus {
        let base = format!("/sys/devices/system/cpu/cpu{index}/cpufreq");
        if let Some(value) = fs.read(&format!("{base}/energy_performance_available_preferences")) {
            for item in value.split_whitespace() {
                epp_values.insert(item.to_string());
            }
        }
        if let Some(value) = fs.read(&format!("{base}/scaling_available_governors")) {
            for item in value.split_whitespace() {
                governor_values.insert(item.to_string());
            }
        }
    }

    CpuInfo {
        cpus,
        driver: fs.read(&format!("{cpu0}/scaling_driver")),
        governor: fs.read(&format!("{cpu0}/scaling_governor")),
        available_governors: governor_values.into_iter().collect(),
        epp: fs.read(&format!("{cpu0}/energy_performance_preference")),
        available_epp: epp_values.into_iter().collect(),
        scaling_min_khz: fs.read_u64(&format!("{cpu0}/scaling_min_freq")),
        scaling_max_khz: fs.read_u64(&format!("{cpu0}/scaling_max_freq")),
        scaling_cur_khz: fs.read_u64(&format!("{cpu0}/scaling_cur_freq")),
        cpuinfo_min_khz: fs.read_u64(&format!("{cpu0}/cpuinfo_min_freq")),
        cpuinfo_max_khz: fs.read_u64(&format!("{cpu0}/cpuinfo_max_freq")),
        boost: fs
            .read("/sys/devices/system/cpu/cpufreq/boost")
            .map(|value| value == "1"),
        amd_pstate_status: fs.read("/sys/devices/system/cpu/amd_pstate/status"),
        dynamic_epp: fs.read("/sys/devices/system/cpu/amd_pstate/dynamic_epp"),
        prefcore: fs.read("/sys/devices/system/cpu/amd_pstate/prefcore"),
    }
}

/// Paths of every per-CPU `cpufreq` directory.
fn cpufreq_dirs(fs: &Sysfs) -> Vec<String> {
    fs.glob("/sys/devices/system/cpu/cpu*/cpufreq")
        .into_iter()
        .map(|path| fs.display_path(&path))
        .collect()
}

fn write_all(
    fs: &Sysfs,
    file: &str,
    value: &str,
    dry_run: bool,
    compare: impl Fn(&str, &str) -> bool + Copy,
) -> Result<Vec<WriteReport>> {
    let dirs = cpufreq_dirs(fs);
    if dirs.is_empty() {
        return Err(Error::NotFound("cpufreq".to_string()));
    }

    let mut reports = Vec::new();
    for dir in dirs {
        let path = format!("{dir}/{file}");
        if !fs.exists(&path) {
            continue;
        }
        reports.push(write::write_verified(fs, &path, value, dry_run, compare)?);
    }

    if reports.is_empty() {
        return Err(Error::NotFound(format!("cpufreq attribute {file}")));
    }
    Ok(reports)
}

/// Change the scaling governor on every CPU.
pub fn set_governor(fs: &Sysfs, governor: &str, dry_run: bool) -> Result<Vec<WriteReport>> {
    let info = detect(fs);
    if !info.available_governors.is_empty()
        && !info
            .available_governors
            .iter()
            .any(|value| value == governor)
    {
        return Err(Error::InvalidValue(format!(
            "unknown governor {governor}; expected one of {}",
            info.available_governors.join(", ")
        )));
    }
    write_all(fs, "scaling_governor", governor, dry_run, write::exact)
}

/// Change the energy performance preference on every CPU.
pub fn set_epp(fs: &Sysfs, epp: &str, dry_run: bool) -> Result<Vec<WriteReport>> {
    let info = detect(fs);
    if !info.available_epp.is_empty() && !info.available_epp.iter().any(|value| value == epp) {
        return Err(Error::InvalidValue(format!(
            "unknown EPP {epp}; this kernel reports {}",
            info.available_epp.join(", ")
        )));
    }
    write_all(
        fs,
        "energy_performance_preference",
        epp,
        dry_run,
        write::exact,
    )
}

/// Clamp the scaling frequency range (in kHz) on every CPU.
///
/// The write order avoids transient `min > max` states: when the range moves
/// down, `scaling_max_freq` is written first, and when it moves up, `min`
/// goes first.
pub fn set_frequency_range(
    fs: &Sysfs,
    min_khz: Option<u64>,
    max_khz: Option<u64>,
    dry_run: bool,
) -> Result<Vec<WriteReport>> {
    let info = detect(fs);
    let hardware_min = info.cpuinfo_min_khz.unwrap_or(0);
    let hardware_max = info.cpuinfo_max_khz.unwrap_or(u64::MAX);

    if let Some(min) = min_khz
        && (min < hardware_min || min > hardware_max)
    {
        return Err(Error::InvalidValue(format!(
            "minimum frequency {min} kHz outside hardware range {hardware_min}..{hardware_max}"
        )));
    }
    if let Some(max) = max_khz
        && (max < hardware_min || max > hardware_max)
    {
        return Err(Error::InvalidValue(format!(
            "maximum frequency {max} kHz outside hardware range {hardware_min}..{hardware_max}"
        )));
    }
    if let (Some(min), Some(max)) = (min_khz, max_khz)
        && min > max
    {
        return Err(Error::InvalidValue(format!(
            "minimum frequency {min} kHz is above the maximum {max} kHz"
        )));
    }

    let lowering = max_khz
        .zip(info.scaling_min_khz)
        .map(|(max, current_min)| max < current_min)
        .unwrap_or(false);

    let mut reports = Vec::new();
    if lowering {
        if let Some(max) = max_khz {
            reports.extend(write_all(
                fs,
                "scaling_max_freq",
                &max.to_string(),
                dry_run,
                write::exact,
            )?);
        }
        if let Some(min) = min_khz {
            reports.extend(write_all(
                fs,
                "scaling_min_freq",
                &min.to_string(),
                dry_run,
                write::exact,
            )?);
        }
    } else {
        if let Some(min) = min_khz {
            reports.extend(write_all(
                fs,
                "scaling_min_freq",
                &min.to_string(),
                dry_run,
                write::exact,
            )?);
        }
        if let Some(max) = max_khz {
            reports.extend(write_all(
                fs,
                "scaling_max_freq",
                &max.to_string(),
                dry_run,
                write::exact,
            )?);
        }
    }

    Ok(reports)
}

/// Enable or disable CPU boost (`/sys/devices/system/cpu/cpufreq/boost`).
pub fn set_boost(fs: &Sysfs, enabled: bool, dry_run: bool) -> Result<WriteReport> {
    write::write_verified(
        fs,
        "/sys/devices/system/cpu/cpufreq/boost",
        if enabled { "1" } else { "0" },
        dry_run,
        write::exact,
    )
}
