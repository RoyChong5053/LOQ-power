use std::collections::BTreeSet;

use serde::Serialize;

use crate::sysfs::Sysfs;

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
