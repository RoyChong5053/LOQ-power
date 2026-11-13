//! Profile model, TOML persistence and resolution into an [`ApplyRequest`].
//!
//! Profiles describe a full power state. Values are clamped against the live
//! sysfs ranges before anything is written, so importing an old profile from
//! a different machine cannot brick the current one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::battery;
use crate::cpu;
use crate::ec;
use crate::error::{Error, Result};
use crate::request::{ApplyRequest, BatteryRequest, CpuRequest, IdeapadRequest};
use crate::sysfs::Sysfs;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerSaving {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_power_save: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usb_autosuspend: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcie_aspm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nmi_watchdog: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvme_runtime_pm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_power_save: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ec: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<CpuRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_charge_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ideapad: Option<IdeapadRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_saving: Option<PowerSaving>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerConfig {
    #[serde(default)]
    pub auto_switch: bool,
    #[serde(default = "default_ac_profile")]
    pub ac: String,
    #[serde(default = "default_battery_profile")]
    pub battery: String,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            auto_switch: false,
            ac: default_ac_profile(),
            battery: default_battery_profile(),
        }
    }
}

fn default_ac_profile() -> String {
    "性能".to_string()
}

fn default_battery_profile() -> String {
    "省电".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileFile {
    #[serde(default)]
    pub power: PowerConfig,
    #[serde(default)]
    pub profile: BTreeMap<String, Profile>,
}

impl ProfileFile {
    /// Built-in profiles mirroring the legacy Python defaults.
    pub fn builtin() -> Self {
        let mut profile = BTreeMap::new();
        profile.insert(
            "省电".to_string(),
            Profile {
                platform: Some("low-power".to_string()),
                ec: ec_map(&[
                    ("gpu_nv_ctgp", 55),
                    ("gpu_nv_ppab", 10),
                    ("gpu_nv_ac_offset", 10),
                    ("gpu_nv_cpu_boost", 5),
                    ("gpu_temp", 75),
                    ("ppt_pl1_spl", 40),
                    ("ppt_pl2_sppt", 45),
                    ("ppt_pl3_fppt", 55),
                    ("ppt_cpu_cl", 30),
                    ("cpu_temp", 85),
                ]),
                cpu: Some(CpuRequest {
                    governor: Some("powersave".to_string()),
                    epp: Some("power".to_string()),
                    min_khz: Some(411_000),
                    max_khz: Some(2_800_000),
                    boost: Some(true),
                }),
                battery_charge_type: Some("Long_Life".to_string()),
                ideapad: Some(IdeapadRequest {
                    fan_mode: Some(0),
                    fn_lock: None,
                }),
                power_saving: Some(PowerSaving {
                    wifi_power_save: Some(true),
                    usb_autosuspend: Some(true),
                    pcie_aspm: Some("powersupersave".to_string()),
                    nmi_watchdog: Some(true),
                    nvme_runtime_pm: Some("auto".to_string()),
                    audio_power_save: Some(true),
                }),
            },
        );
        profile.insert(
            "平衡".to_string(),
            Profile {
                platform: Some("balanced".to_string()),
                ec: ec_map(&[
                    ("gpu_nv_ctgp", 60),
                    ("gpu_nv_ppab", 15),
                    ("gpu_nv_ac_offset", 45),
                    ("gpu_nv_cpu_boost", 5),
                    ("gpu_temp", 85),
                    ("ppt_pl1_spl", 45),
                    ("ppt_pl2_sppt", 50),
                    ("ppt_pl3_fppt", 55),
                    ("ppt_cpu_cl", 45),
                    ("cpu_temp", 95),
                ]),
                cpu: Some(CpuRequest {
                    governor: Some("performance".to_string()),
                    epp: Some("performance".to_string()),
                    min_khz: Some(1_800_000),
                    max_khz: Some(4_000_000),
                    boost: Some(true),
                }),
                battery_charge_type: Some("Standard".to_string()),
                ideapad: Some(IdeapadRequest {
                    fan_mode: Some(1),
                    fn_lock: None,
                }),
                power_saving: Some(PowerSaving {
                    wifi_power_save: Some(false),
                    usb_autosuspend: Some(true),
                    pcie_aspm: Some("default".to_string()),
                    nmi_watchdog: Some(true),
                    nvme_runtime_pm: Some("auto".to_string()),
                    audio_power_save: Some(false),
                }),
            },
        );
        profile.insert(
            "性能".to_string(),
            Profile {
                platform: Some("performance".to_string()),
                ec: ec_map(&[
                    ("gpu_nv_ctgp", 80),
                    ("gpu_nv_ppab", 25),
                    ("gpu_nv_ac_offset", 55),
                    ("gpu_nv_cpu_boost", 10),
                    ("gpu_temp", 87),
                    ("ppt_pl1_spl", 54),
                    ("ppt_pl2_sppt", 65),
                    ("ppt_pl3_fppt", 80),
                    ("ppt_cpu_cl", 45),
                    ("cpu_temp", 100),
                ]),
                cpu: Some(CpuRequest {
                    governor: Some("performance".to_string()),
                    epp: Some("performance".to_string()),
                    min_khz: Some(1_800_000),
                    max_khz: Some(4_553_000),
                    boost: Some(true),
                }),
                battery_charge_type: Some("Fast".to_string()),
                ideapad: Some(IdeapadRequest {
                    fan_mode: Some(4),
                    fn_lock: None,
                }),
                power_saving: Some(PowerSaving {
                    wifi_power_save: Some(false),
                    usb_autosuspend: Some(false),
                    pcie_aspm: Some("performance".to_string()),
                    nmi_watchdog: Some(false),
                    nvme_runtime_pm: Some("on".to_string()),
                    audio_power_save: Some(false),
                }),
            },
        );
        profile.insert(
            "自定义".to_string(),
            Profile {
                platform: Some("custom".to_string()),
                ec: ec_map(&[
                    ("gpu_nv_ctgp", 60),
                    ("gpu_nv_ppab", 15),
                    ("gpu_nv_ac_offset", 45),
                    ("gpu_nv_cpu_boost", 5),
                    ("gpu_temp", 85),
                    ("ppt_pl1_spl", 45),
                    ("ppt_pl2_sppt", 50),
                    ("ppt_pl3_fppt", 55),
                    ("ppt_cpu_cl", 45),
                    ("cpu_temp", 90),
                ]),
                cpu: Some(CpuRequest {
                    governor: Some("performance".to_string()),
                    epp: Some("balance_power".to_string()),
                    min_khz: Some(1_800_000),
                    max_khz: Some(4_000_000),
                    boost: Some(true),
                }),
                battery_charge_type: None,
                ideapad: None,
                power_saving: None,
            },
        );

        Self {
            power: PowerConfig::default(),
            profile,
        }
    }

    /// Load a profile file, falling back to the built-in set. Unknown keys in
    /// an existing file are ignored so older configurations keep working.
    pub fn load_or_builtin(path: &Path) -> Self {
        match Self::load(path) {
            Ok(file) => file,
            Err(_) => Self::builtin(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&content).map_err(|error| {
            Error::InvalidValue(format!("cannot parse {}: {error}", path.display()))
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|error| Error::Other(format!("cannot serialize profiles: {error}")))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        std::fs::write(path, content).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn names(&self) -> Vec<String> {
        self.profile.keys().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Result<&Profile> {
        self.profile
            .get(name)
            .ok_or_else(|| Error::NotFound(format!("profile {name}")))
    }
}

fn ec_map(values: &[(&str, i64)]) -> BTreeMap<String, i64> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_string(), *value))
        .collect()
}

/// Result of resolving a profile against the live hardware.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Resolved {
    pub request: ApplyRequest,
    pub warnings: Vec<String>,
}

/// Turn a profile into a validated request, clamping every value to what the
/// current machine actually supports.
pub fn resolve(fs: &Sysfs, profile: &Profile) -> Resolved {
    let mut resolved = Resolved::default();
    let mut request = ApplyRequest {
        platform: profile.platform.clone(),
        ..Default::default()
    };

    let attributes = ec::discover(fs).attributes;
    for (name, value) in &profile.ec {
        let Some(attribute) = attributes.iter().find(|attribute| &attribute.name == name) else {
            resolved
                .warnings
                .push(format!("EC 属性 {name} 在本机不存在，已跳过"));
            continue;
        };

        let clamped = match (attribute.min, attribute.max) {
            (Some(min), Some(max)) => (*value).clamp(min, max),
            (Some(min), None) => (*value).max(min),
            (None, Some(max)) => (*value).min(max),
            (None, None) => *value,
        };
        if clamped != *value {
            resolved.warnings.push(format!(
                "{}: {} 超出范围，已调整为 {clamped}",
                attribute.display_name, value
            ));
        }
        request.ec.insert(name.clone(), clamped);
    }

    if let Some(cpu_profile) = &profile.cpu {
        let info = cpu::detect(fs);
        let mut cpu_request = cpu_profile.clone();
        if let (Some(min), Some(value)) = (info.cpuinfo_min_khz, cpu_request.min_khz)
            && value < min
        {
            cpu_request.min_khz = Some(min);
            resolved
                .warnings
                .push(format!("CPU 最低频率已提升到硬件下限 {min} kHz"));
        }
        if let (Some(max), Some(value)) = (info.cpuinfo_max_khz, cpu_request.max_khz)
            && value > max
        {
            cpu_request.max_khz = Some(max);
            resolved
                .warnings
                .push(format!("CPU 最高频率已限制到硬件上限 {max} kHz"));
        }
        request.cpu = Some(cpu_request);
    }

    if let Some(charge_type) = &profile.battery_charge_type {
        request.battery = Some(BatteryRequest {
            name: battery::primary(fs).map(|battery| battery.name),
            charge_type: charge_type.clone(),
        });
    }

    request.ideapad = profile.ideapad.clone();

    resolved.request = request;
    resolved
}

/// `~/.config/loq-power/profiles.toml`, or `$LOQ_POWER_PROFILES`.
pub fn user_path() -> PathBuf {
    if let Some(path) = std::env::var_os("LOQ_POWER_PROFILES") {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root"));
    home.join(".config/loq-power/profiles.toml")
}

/// `/etc/loq-power/profiles.toml`.
pub fn system_path() -> PathBuf {
    PathBuf::from("/etc/loq-power/profiles.toml")
}
