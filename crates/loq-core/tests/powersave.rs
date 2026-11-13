use std::fs;
use std::path::Path;

use loq_core::Sysfs;
use loq_core::powersave;
use loq_core::profile::PowerSaving;

fn write(root: &Path, path: &str, content: &str) {
    let full = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path.trim_start_matches('/'))).unwrap()
}

fn power_tree(root: &Path) {
    write(root, "/sys/bus/usb/devices/1-1/power/control", "on\n");
    write(root, "/sys/bus/usb/devices/1-1/uevent", "DRIVER=usb\n");
    // Audio devices must be excluded from autosuspend.
    write(root, "/sys/bus/usb/devices/1-2/power/control", "on\n");
    write(
        root,
        "/sys/bus/usb/devices/1-2/uevent",
        "DRIVER=snd-usb-audio\n",
    );
    write(
        root,
        "/sys/module/pcie_aspm/parameters/policy",
        "[default] performance powersave powersupersave\n",
    );
    write(root, "/proc/sys/kernel/nmi_watchdog", "0\n");
    write(root, "/sys/block/nvme0n1/device/power/control", "on\n");
    write(
        root,
        "/sys/module/snd_hda_intel/parameters/power_save",
        "0\n",
    );
    write(
        root,
        "/sys/module/snd_hda_intel/parameters/power_save_controller",
        "N\n",
    );
}

#[test]
fn detects_power_saving_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    power_tree(root);

    let state = powersave::detect(&Sysfs::with_root(root));
    assert_eq!(state.wifi_interface, None);
    assert_eq!(state.usb_autosuspend_devices, 1);
    assert_eq!(state.usb_autosuspend_enabled, 0);
    assert_eq!(state.pcie_aspm.as_deref(), Some("default"));
    assert_eq!(
        state.pcie_aspm_choices,
        vec!["default", "performance", "powersave", "powersupersave"]
    );
    assert_eq!(state.nmi_watchdog, Some(false));
    assert_eq!(state.nvme_runtime_pm, vec!["on"]);
    assert_eq!(state.audio_power_save, Some(false));
}

#[test]
fn applies_power_saving_settings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    power_tree(root);
    let fs = Sysfs::with_root(root);

    let settings = PowerSaving {
        wifi_power_save: None,
        usb_autosuspend: Some(true),
        pcie_aspm: Some("powersupersave".to_string()),
        nmi_watchdog: Some(true),
        nvme_runtime_pm: Some("auto".to_string()),
        audio_power_save: Some(true),
    };

    let report = powersave::apply(&fs, &settings, false);
    assert!(report.ok(), "errors: {:?}", report.errors);
    assert!(report.skipped.is_empty(), "skips: {:?}", report.skipped);
    assert_eq!(report.writes.len(), 6);

    assert_eq!(
        read(root, "/sys/bus/usb/devices/1-1/power/control"),
        "auto\n"
    );
    assert_eq!(read(root, "/sys/bus/usb/devices/1-2/power/control"), "on\n");
    assert_eq!(
        read(root, "/sys/module/pcie_aspm/parameters/policy"),
        "powersupersave\n"
    );
    assert_eq!(read(root, "/proc/sys/kernel/nmi_watchdog"), "1\n");
    assert_eq!(
        read(root, "/sys/block/nvme0n1/device/power/control"),
        "auto\n"
    );
    assert_eq!(
        read(root, "/sys/module/snd_hda_intel/parameters/power_save"),
        "1\n"
    );
    assert_eq!(
        read(
            root,
            "/sys/module/snd_hda_intel/parameters/power_save_controller"
        ),
        "Y\n"
    );
}

#[test]
fn dry_run_and_invalid_pcie_policy() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    power_tree(root);
    let fs = Sysfs::with_root(root);

    let settings = PowerSaving {
        pcie_aspm: Some("powersupersave".to_string()),
        ..Default::default()
    };
    let report = powersave::apply(&fs, &settings, true);
    assert!(report.ok());
    assert!(report.writes.iter().all(|write| write.dry_run));
    assert_eq!(
        read(root, "/sys/module/pcie_aspm/parameters/policy"),
        "[default] performance powersave powersupersave\n"
    );

    let settings = PowerSaving {
        pcie_aspm: Some("bogus".to_string()),
        ..Default::default()
    };
    let report = powersave::apply(&fs, &settings, true);
    assert!(!report.ok());
    assert!(report.errors[0].contains("bogus"));
}
