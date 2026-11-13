use std::fs;
use std::path::Path;

use loq_core::{Sysfs, tlp_import};

fn write(root: &Path, path: &str, content: &str) {
    let full = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

fn tlp_tree(root: &Path) {
    write(
        root,
        "/etc/tlp.conf",
        r#"
# Main configuration
CPU_SCALING_GOVERNOR_ON_AC=performance
CPU_SCALING_GOVERNOR_ON_BAT=powersave
CPU_ENERGY_PERF_POLICY_ON_AC=performance
CPU_ENERGY_PERF_POLICY_ON_BAT=power
CPU_MIN_PERF_ON_AC=40
CPU_MAX_PERF_ON_AC=100
CPU_MIN_PERF_ON_BAT=5
CPU_MAX_PERF_ON_BAT=60
CPU_BOOST_ON_AC=1
CPU_BOOST_ON_BAT=0
PLATFORM_PROFILE_ON_AC=balanced
PLATFORM_PROFILE_ON_BAT=low-power
WIFI_PWR_ON_AC=off
WIFI_PWR_ON_BAT=on
USB_AUTOSUSPEND_ON_AC=1
USB_AUTOSUSPEND_ON_BAT=1
PCIE_ASPM_ON_AC=default
PCIE_ASPM_ON_BAT=powersupersave
RUNTIME_PM_ON_AC=on
RUNTIME_PM_ON_BAT=auto
NMI_WATCHDOG=1
SOUND_POWER_SAVE_ON_AC=0
SOUND_POWER_SAVE_ON_BAT=1
STOP_CHARGE_THRESH_BAT0=80
"#,
    );
}

#[test]
fn imports_tlp_settings_into_ac_and_battery_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    tlp_tree(root);

    let imported = tlp_import::import(&Sysfs::with_root(root));
    let profiles = &imported.profiles;

    assert_eq!(profiles.power.ac, "TLP-AC");
    assert_eq!(profiles.power.battery, "TLP-BAT");
    assert!(profiles.power.auto_switch);

    let ac = &profiles.profile["TLP-AC"];
    assert_eq!(ac.platform.as_deref(), Some("balanced"));
    let ac_cpu = ac.cpu.as_ref().unwrap();
    assert_eq!(ac_cpu.governor.as_deref(), Some("performance"));
    assert_eq!(ac_cpu.epp.as_deref(), Some("performance"));
    // 40% of the 4 GHz fallback maximum.
    assert_eq!(ac_cpu.min_khz, Some(1_600_000));
    assert_eq!(ac_cpu.max_khz, Some(4_000_000));
    assert_eq!(ac_cpu.boost, Some(true));

    let ac_saving = ac.power_saving.as_ref().unwrap();
    assert_eq!(ac_saving.wifi_power_save, Some(false));
    assert_eq!(ac_saving.usb_autosuspend, Some(true));
    assert_eq!(ac_saving.pcie_aspm.as_deref(), Some("default"));
    assert_eq!(ac_saving.nvme_runtime_pm.as_deref(), Some("on"));
    assert_eq!(ac_saving.nmi_watchdog, Some(true));
    assert_eq!(ac_saving.audio_power_save, Some(false));
    assert_eq!(ac.battery_charge_type.as_deref(), Some("Long_Life"));

    let battery = &profiles.profile["TLP-BAT"];
    let battery_cpu = battery.cpu.as_ref().unwrap();
    assert_eq!(battery_cpu.governor.as_deref(), Some("powersave"));
    assert_eq!(battery_cpu.epp.as_deref(), Some("power"));
    assert_eq!(battery_cpu.boost, Some(false));
    let battery_saving = battery.power_saving.as_ref().unwrap();
    assert_eq!(battery_saving.pcie_aspm.as_deref(), Some("powersupersave"));
    assert_eq!(battery_saving.audio_power_save, Some(true));
    assert_eq!(battery.platform.as_deref(), Some("low-power"));
}

#[test]
fn reports_unrepresentable_charge_thresholds() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        "/etc/tlp.conf",
        "STOP_CHARGE_THRESH_BAT0=60\nCPU_SCALING_GOVERNOR_ON_AC=schedutil\n",
    );

    let imported = tlp_import::import(&Sysfs::with_root(root));
    let ac = &imported.profiles.profile["TLP-AC"];
    assert_eq!(ac.battery_charge_type.as_deref(), Some("Long_Life"));
    assert!(
        imported
            .notes
            .iter()
            .any(|note| note.contains("STOP_CHARGE_THRESH_BAT0=60")),
        "notes: {:?}",
        imported.notes
    );
    // Unknown governors are passed through; apply() rejects them later.
    assert_eq!(
        ac.cpu.as_ref().unwrap().governor.as_deref(),
        Some("schedutil")
    );
}

#[test]
fn percentage_values_above_one_hundred_are_treated_as_khz() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        "/etc/tlp.conf",
        "CPU_MIN_PERF_ON_AC=1200000\nCPU_MAX_PERF_ON_AC=100\n",
    );

    let imported = tlp_import::import(&Sysfs::with_root(root));
    let ac_cpu = imported.profiles.profile["TLP-AC"].cpu.as_ref().unwrap();
    assert_eq!(ac_cpu.min_khz, Some(1_200_000));
    assert_eq!(ac_cpu.max_khz, Some(4_000_000));
}
