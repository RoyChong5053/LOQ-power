//! Translate an existing TLP configuration into LOQ Power profiles.
//!
//! Only settings that have a real effect on this laptop are mapped. Anything
//! that cannot be represented (disk APM levels, ThinkPad-only knobs, charge
//! thresholds finer than the three EC charge modes) is reported as a note so
//! the user sees what will change after the takeover.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::cpu;
use crate::profile::{PowerConfig, PowerSaving, Profile, ProfileFile};
use crate::request::{CpuRequest, IdeapadRequest};
use crate::sysfs::Sysfs;
use crate::tlp;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Imported {
    pub profiles: ProfileFile,
    pub notes: Vec<String>,
}

fn value<'a>(map: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    map.get(key).map(String::as_str)
}

fn boolean(map: &BTreeMap<String, String>, key: &str) -> Option<bool> {
    value(map, key).and_then(|raw| match raw.trim().to_ascii_uppercase().as_str() {
        "1" | "Y" | "YES" | "ON" | "TRUE" => Some(true),
        "0" | "N" | "NO" | "OFF" | "FALSE" => Some(false),
        _ => None,
    })
}

fn percentage_to_khz(
    map: &BTreeMap<String, String>,
    key: &str,
    hardware_max: u64,
    hardware_min: u64,
) -> Option<u64> {
    let raw = value(map, key)?;
    // TLP accepts percentages; a value above 100 is already a frequency.
    let khz = match raw.parse::<u64>().ok()? {
        0..=100 => hardware_max * raw.parse::<u64>().ok()? / 100,
        absolute => absolute,
    };
    Some(khz.clamp(hardware_min, hardware_max))
}

fn platform_profile(map: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let raw = value(map, key)?;
    let normalized = match raw {
        "low-power" | "quiet" | "power-saver" => "low-power",
        "balanced" | "balanced-performance" => "balanced",
        "performance" => "performance",
        "max-power" => "max-power",
        other => other,
    };
    Some(normalized.to_string())
}

fn charge_type(map: &BTreeMap<String, String>) -> (Option<String>, Option<String>) {
    let Some(stop) = value(map, "STOP_CHARGE_THRESH_BAT0") else {
        return (None, None);
    };
    let threshold = stop.parse::<u64>().ok();

    match threshold {
        Some(100) => (
            Some("Standard".to_string()),
            Some("STOP_CHARGE_THRESH_BAT0=100 映射为 Standard".to_string()),
        ),
        Some(80) => (
            Some("Long_Life".to_string()),
            Some("STOP_CHARGE_THRESH_BAT0=80 映射为 Long_Life".to_string()),
        ),
        Some(60) => (
            Some("Long_Life".to_string()),
            Some("STOP_CHARGE_THRESH_BAT0=60 没有对应档位，使用 Long_Life".to_string()),
        ),
        Some(other) => (
            Some("Long_Life".to_string()),
            Some(format!(
                "STOP_CHARGE_THRESH_BAT0={other} 无法精确表达，使用 Long_Life"
            )),
        ),
        None => (
            None,
            Some("STOP_CHARGE_THRESH_BAT0 不是数字，已忽略".to_string()),
        ),
    }
}

fn build_profile(
    map: &BTreeMap<String, String>,
    suffix: &str,
    hardware_min: u64,
    hardware_max: u64,
) -> (Profile, Vec<String>) {
    let mut notes = Vec::new();

    let governor = value(map, &format!("CPU_SCALING_GOVERNOR_ON_{suffix}")).map(str::to_string);
    let epp = value(map, &format!("CPU_ENERGY_PERF_POLICY_ON_{suffix}")).map(str::to_string);
    let min_khz = percentage_to_khz(
        map,
        &format!("CPU_MIN_PERF_ON_{suffix}"),
        hardware_max,
        hardware_min,
    );
    let max_khz = percentage_to_khz(
        map,
        &format!("CPU_MAX_PERF_ON_{suffix}"),
        hardware_max,
        hardware_min,
    );
    let boost = boolean(map, &format!("CPU_BOOST_ON_{suffix}"));
    let platform = platform_profile(map, &format!("PLATFORM_PROFILE_ON_{suffix}"));

    let wifi_power_save =
        value(map, &format!("WIFI_PWR_ON_{suffix}")).map(|raw| raw.eq_ignore_ascii_case("on"));
    let usb_autosuspend = boolean(map, &format!("USB_AUTOSUSPEND_ON_{suffix}"));
    let pcie_aspm = value(map, &format!("PCIE_ASPM_ON_{suffix}")).map(str::to_string);
    let nvme_runtime_pm = value(map, &format!("RUNTIME_PM_ON_{suffix}")).map(str::to_string);

    let (charge, charge_note) = charge_type(map);
    if let Some(note) = charge_note {
        notes.push(note);
    }

    let profile = Profile {
        platform,
        ec: BTreeMap::new(),
        cpu: Some(CpuRequest {
            governor,
            epp,
            min_khz,
            max_khz,
            boost,
        }),
        battery_charge_type: charge,
        ideapad: Some(IdeapadRequest::default()),
        power_saving: Some(PowerSaving {
            wifi_power_save,
            usb_autosuspend,
            pcie_aspm,
            nmi_watchdog: boolean(map, "NMI_WATCHDOG"),
            nvme_runtime_pm,
            audio_power_save: boolean(map, &format!("SOUND_POWER_SAVE_ON_{suffix}")),
        }),
    };

    notes.push(format!(
        "TLP 的 EC 功率限制没有对应项，{suffix} 配置档沿用内置方案"
    ));

    (profile, notes)
}

/// Build profiles from the effective TLP configuration.
pub fn import(fs: &Sysfs) -> Imported {
    let effective = tlp::detect(fs).effective;
    let info = cpu::detect(fs);
    let hardware_max = info.cpuinfo_max_khz.unwrap_or(4_000_000);
    let hardware_min = info.cpuinfo_min_khz.unwrap_or(400_000);

    let (ac, mut notes) = build_profile(&effective, "AC", hardware_min, hardware_max);
    let (battery, battery_notes) = build_profile(&effective, "BAT", hardware_min, hardware_max);
    notes.extend(battery_notes);

    let mut profiles = BTreeMap::new();
    profiles.insert("TLP-AC".to_string(), ac);
    profiles.insert("TLP-BAT".to_string(), battery);

    if effective.is_empty() {
        notes.push("未找到 TLP 配置，导入结果为空".to_string());
    }

    Imported {
        profiles: ProfileFile {
            power: PowerConfig {
                auto_switch: true,
                ac: "TLP-AC".to_string(),
                battery: "TLP-BAT".to_string(),
            },
            profile: profiles,
        },
        notes,
    }
}
