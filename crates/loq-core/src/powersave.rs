//! Power-saving knobs absorbed from TLP.
//!
//! These are the settings that actually do something on this laptop. Every
//! setter is best-effort: a missing kernel interface is reported as a skip,
//! never as a failure that aborts a profile switch.

use std::process::Command;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::profile::PowerSaving;
use crate::sysfs::Sysfs;
use crate::write::{self, WriteReport};

#[derive(Debug, Clone, Default, Serialize)]
pub struct PowerSavingState {
    pub wifi_interface: Option<String>,
    pub wifi_power_save: Option<bool>,
    pub usb_autosuspend_devices: usize,
    pub usb_autosuspend_enabled: usize,
    pub pcie_aspm: Option<String>,
    pub pcie_aspm_choices: Vec<String>,
    pub nmi_watchdog: Option<bool>,
    pub nvme_runtime_pm: Vec<String>,
    pub audio_power_save: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PowerSavingReport {
    pub writes: Vec<WriteReport>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

impl PowerSavingReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

fn parse_choice(raw: &str) -> (Option<String>, Vec<String>) {
    let mut current = None;
    let mut choices = Vec::new();
    for token in raw.split_whitespace() {
        if let Some(stripped) = token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            current = Some(stripped.to_string());
            choices.push(stripped.to_string());
        } else {
            choices.push(token.to_string());
        }
    }
    (current, choices)
}

pub fn wifi_interface(fs: &Sysfs) -> Option<String> {
    fs.list_dir("/sys/class/net")
        .into_iter()
        .find(|name| fs.exists(&format!("/sys/class/net/{name}/phy80211")))
}

fn wifi_power_save(interface: &str) -> Option<bool> {
    let output = Command::new("iw")
        .args(["dev", interface, "get", "power_save"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if text.contains("Power save: on") {
        Some(true)
    } else if text.contains("Power save: off") {
        Some(false)
    } else {
        None
    }
}

fn usb_control_paths(fs: &Sysfs) -> Vec<String> {
    let mut paths = Vec::new();
    for entry in fs.list_dir("/sys/bus/usb/devices") {
        let base = format!("/sys/bus/usb/devices/{entry}");
        let control = format!("{base}/power/control");
        if !fs.exists(&control) {
            continue;
        }

        // Keep audio interfaces and printers powered, mirroring TLP's
        // exclusions: autosuspending them causes crackling or stalled jobs.
        let uevent = fs.read(&format!("{base}/uevent")).unwrap_or_default();
        if uevent.contains("DRIVER=snd-usb-audio") || uevent.contains("DRIVER=usblp") {
            continue;
        }
        paths.push(control);
    }
    paths
}

fn nvme_control_paths(fs: &Sysfs) -> Vec<String> {
    let mut paths = Vec::new();
    for entry in fs.list_dir("/sys/block") {
        if !entry.starts_with("nvme") || !entry.contains('n') {
            continue;
        }
        let base = format!("/sys/block/{entry}/device");
        let control = format!("{base}/power/control");
        if fs.exists(&control) {
            paths.push(control);
        }
    }
    paths
}

fn audio_paths(fs: &Sysfs) -> Vec<String> {
    [
        "/sys/module/snd_hda_intel/parameters/power_save",
        "/sys/module/snd_hda_intel/parameters/power_save_controller",
    ]
    .into_iter()
    .filter(|path| fs.exists(path))
    .map(str::to_string)
    .collect()
}

pub fn detect(fs: &Sysfs) -> PowerSavingState {
    let interface = wifi_interface(fs);
    let usb = usb_control_paths(fs);
    let usb_enabled = usb
        .iter()
        .filter(|path| fs.read(path).as_deref() == Some("auto"))
        .count();

    let (pcie_aspm, pcie_aspm_choices) = fs
        .read("/sys/module/pcie_aspm/parameters/policy")
        .map(|raw| parse_choice(&raw))
        .unwrap_or_default();

    let nvme_runtime_pm = nvme_control_paths(fs)
        .iter()
        .filter_map(|path| fs.read(path))
        .collect();

    PowerSavingState {
        wifi_power_save: interface.as_deref().and_then(wifi_power_save),
        wifi_interface: interface,
        usb_autosuspend_devices: usb.len(),
        usb_autosuspend_enabled: usb_enabled,
        pcie_aspm,
        pcie_aspm_choices,
        nmi_watchdog: fs
            .read("/proc/sys/kernel/nmi_watchdog")
            .map(|value| value == "1"),
        nvme_runtime_pm,
        audio_power_save: fs
            .read("/sys/module/snd_hda_intel/parameters/power_save")
            .map(|value| value != "0"),
    }
}

fn set_wifi(interface: &str, enabled: bool, dry_run: bool) -> Result<Option<WriteReport>> {
    if dry_run {
        return Ok(Some(WriteReport {
            target: format!("iw:{interface}"),
            path: format!("/sys/class/net/{interface}"),
            previous: wifi_power_save(interface).map(|value| {
                if value {
                    "on".to_string()
                } else {
                    "off".to_string()
                }
            }),
            value: if enabled { "on" } else { "off" }.to_string(),
            dry_run: true,
            verified: false,
        }));
    }

    let output = Command::new("iw")
        .args([
            "dev",
            interface,
            "set",
            "power_save",
            if enabled { "on" } else { "off" },
        ])
        .output()
        .map_err(|source| Error::Other(format!("cannot run iw: {source}")))?;

    if !output.status.success() {
        return Err(Error::Other(format!(
            "iw set power_save failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(Some(WriteReport {
        target: format!("iw:{interface}"),
        path: format!("/sys/class/net/{interface}"),
        previous: None,
        value: if enabled { "on" } else { "off" }.to_string(),
        dry_run: false,
        verified: wifi_power_save(interface) == Some(enabled),
    }))
}

/// Apply the TLP-absorbed knobs. Missing interfaces are reported as skips.
pub fn apply(fs: &Sysfs, settings: &PowerSaving, dry_run: bool) -> PowerSavingReport {
    let mut report = PowerSavingReport::default();

    if let Some(enabled) = settings.wifi_power_save {
        match wifi_interface(fs) {
            Some(interface) => match set_wifi(&interface, enabled, dry_run) {
                Ok(Some(write)) => report.writes.push(write),
                Ok(None) => {}
                Err(error) => report.errors.push(format!("wifi: {error}")),
            },
            None => report
                .skipped
                .push("wifi: no wireless interface".to_string()),
        }
    }

    if let Some(enabled) = settings.usb_autosuspend {
        let paths = usb_control_paths(fs);
        if paths.is_empty() {
            report
                .skipped
                .push("usb: no autosuspend controls".to_string());
        }
        for path in paths {
            match write::write_verified(
                fs,
                &path,
                if enabled { "auto" } else { "on" },
                dry_run,
                write::exact,
            ) {
                Ok(write) => report.writes.push(write),
                Err(error) => report.errors.push(format!("{path}: {error}")),
            }
        }
    }

    if let Some(policy) = &settings.pcie_aspm {
        let path = "/sys/module/pcie_aspm/parameters/policy";
        if !fs.exists(path) {
            report
                .skipped
                .push("pcie_aspm: kernel module not loaded".to_string());
        } else {
            let (_, choices) = fs
                .read(path)
                .map(|raw| parse_choice(&raw))
                .unwrap_or_default();
            if !choices.is_empty() && !choices.iter().any(|choice| choice == policy) {
                report.errors.push(format!(
                    "pcie_aspm: {policy} is not one of {}",
                    choices.join(", ")
                ));
            } else {
                match write::write_verified(fs, path, policy, dry_run, |actual, expected| {
                    parse_choice(actual).0.as_deref() == Some(expected)
                }) {
                    Ok(write) => report.writes.push(write),
                    Err(error) => report.errors.push(format!("pcie_aspm: {error}")),
                }
            }
        }
    }

    if let Some(enabled) = settings.nmi_watchdog {
        match write::write_verified(
            fs,
            "/proc/sys/kernel/nmi_watchdog",
            if enabled { "1" } else { "0" },
            dry_run,
            write::exact,
        ) {
            Ok(write) => report.writes.push(write),
            Err(error) => report.errors.push(format!("nmi_watchdog: {error}")),
        }
    }

    if let Some(mode) = &settings.nvme_runtime_pm {
        let paths = nvme_control_paths(fs);
        if paths.is_empty() {
            report
                .skipped
                .push("nvme: no runtime PM controls".to_string());
        }
        for path in paths {
            match write::write_verified(fs, &path, mode, dry_run, write::exact) {
                Ok(write) => report.writes.push(write),
                Err(error) => report.errors.push(format!("{path}: {error}")),
            }
        }
    }

    if let Some(enabled) = settings.audio_power_save {
        let values = if enabled { ["1", "Y"] } else { ["0", "N"] };
        for (path, value) in audio_paths(fs).iter().zip(values.iter()) {
            match write::write_verified(fs, path, value, dry_run, write::exact) {
                Ok(write) => report.writes.push(write),
                Err(error) => report.errors.push(format!("{path}: {error}")),
            }
        }
    }

    report
}
