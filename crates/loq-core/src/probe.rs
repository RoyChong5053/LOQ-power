use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::battery::{self, PowerSupplyInfo};
use crate::cpu::{self, CpuInfo};
use crate::ec::{self, EcInfo};
use crate::gpu::{self, GpuInfo};
use crate::hwmon::{self, HwmonDevice};
use crate::ideapad::{self, IdeapadInfo};
use crate::leds::{self, LedInfo};
use crate::legion::{self, LegionInfo};
use crate::platform::{self, PlatformInfo};
use crate::services;
use crate::sysfs::Sysfs;
use crate::tlp::{self, TlpInfo};

#[derive(Debug, Clone, Default, Serialize)]
pub struct DmiInfo {
    pub sys_vendor: Option<String>,
    pub product_name: Option<String>,
    pub product_version: Option<String>,
    pub board_name: Option<String>,
    pub bios_version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ServicesInfo {
    pub tlp: Option<bool>,
    pub tlp_pd: Option<bool>,
    pub nvidia_powerd: Option<bool>,
    pub power_profiles_daemon: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub generated_at_unix: u64,
    pub kernel: Option<String>,
    pub hostname: Option<String>,
    pub dmi: DmiInfo,
    pub ec: EcInfo,
    pub platform: PlatformInfo,
    pub power_supply: PowerSupplyInfo,
    pub cpu: CpuInfo,
    pub gpu: Option<GpuInfo>,
    pub hwmon: Vec<HwmonDevice>,
    pub ideapad: IdeapadInfo,
    pub leds: Vec<LedInfo>,
    pub tlp: TlpInfo,
    pub services: ServicesInfo,
    pub legion: LegionInfo,
}

fn read_dmi(fs: &Sysfs) -> DmiInfo {
    let read = |name: &str| fs.read(&format!("/sys/class/dmi/id/{name}"));
    DmiInfo {
        sys_vendor: read("sys_vendor"),
        product_name: read("product_name"),
        product_version: read("product_version"),
        board_name: read("board_name"),
        bios_version: read("bios_version"),
    }
}

pub fn probe() -> Capabilities {
    probe_with(&Sysfs::new())
}

pub fn probe_with(fs: &Sysfs) -> Capabilities {
    let generated_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();

    Capabilities {
        generated_at_unix,
        kernel: fs.read("/proc/sys/kernel/osrelease"),
        hostname: fs
            .read("/etc/hostname")
            .or_else(|| std::env::var("HOSTNAME").ok()),
        dmi: read_dmi(fs),
        ec: ec::discover(fs),
        platform: platform::detect(fs),
        power_supply: battery::detect(fs),
        cpu: cpu::detect(fs),
        gpu: gpu::detect(),
        hwmon: hwmon::detect(fs),
        ideapad: ideapad::detect(fs),
        leds: leds::detect(fs),
        tlp: tlp::detect(fs),
        services: ServicesInfo {
            tlp: services::is_active("tlp.service"),
            tlp_pd: services::is_active("tlp-pd.service"),
            nvidia_powerd: services::is_active("nvidia-powerd.service"),
            power_profiles_daemon: services::is_active("power-profiles-daemon.service"),
        },
        legion: legion::detect(fs),
    }
}
