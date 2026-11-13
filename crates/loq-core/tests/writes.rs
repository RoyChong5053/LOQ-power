use std::fs;
use std::path::Path;

use loq_core::{Sysfs, battery, cpu, ec, ideapad, leds, platform};

fn write(root: &Path, path: &str, content: &str) {
    let full = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path.trim_start_matches('/')))
        .unwrap()
        .trim()
        .to_string()
}

const EC_BASE: &str = "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes";
const GAMEZONE: &str = "/sys/bus/wmi/drivers/lenovo_wmi_gamezone/887B54E3-DDDC-4B2C-8B88-68A26A8835D0-21/platform-profile/platform-profile-0";

fn ec_fixture(root: &Path, profile: &str) {
    write(
        root,
        &format!("{EC_BASE}/gpu_nv_ctgp/current_value"),
        "55\n",
    );
    write(root, &format!("{EC_BASE}/gpu_nv_ctgp/min_value"), "55\n");
    write(root, &format!("{EC_BASE}/gpu_nv_ctgp/max_value"), "105\n");
    write(
        root,
        &format!("{EC_BASE}/gpu_nv_ctgp/scalar_increment"),
        "5\n",
    );

    write(
        root,
        &format!("{GAMEZONE}/profile"),
        &format!("{profile}\n"),
    );
    write(
        root,
        &format!("{GAMEZONE}/choices"),
        "low-power balanced performance max-power custom\n",
    );
}

#[test]
fn ec_write_rejects_out_of_range_values() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    ec_fixture(root, "custom");
    let sysfs = Sysfs::with_root(root);

    let error = ec::set_attribute(&sysfs, "gpu_nv_ctgp", 120, false).unwrap_err();
    assert!(matches!(
        error,
        loq_core::Error::OutOfRange { value: 120, .. }
    ));

    let error = ec::set_attribute(&sysfs, "gpu_nv_ctgp", 57, false).unwrap_err();
    assert!(matches!(error, loq_core::Error::InvalidStep { .. }));
}

#[test]
fn ec_write_requires_custom_mode() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    ec_fixture(root, "balanced");
    let sysfs = Sysfs::with_root(root);

    let error = ec::set_attribute(&sysfs, "gpu_nv_ctgp", 60, false).unwrap_err();
    assert!(matches!(error, loq_core::Error::RequiresCustomMode(_)));
    assert_eq!(
        read(root, &format!("{EC_BASE}/gpu_nv_ctgp/current_value")),
        "55"
    );

    let report = ec::set_attribute(&sysfs, "gpu_nv_ctgp", 60, true).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.previous.as_deref(), Some("55"));
    assert_eq!(
        read(root, &format!("{EC_BASE}/gpu_nv_ctgp/current_value")),
        "55"
    );
}

#[test]
fn ec_write_succeeds_in_custom_mode() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    ec_fixture(root, "custom");
    let sysfs = Sysfs::with_root(root);

    let report = ec::set_attribute(&sysfs, "gpu_nv_ctgp", 65, false).unwrap();
    assert!(report.verified);
    assert_eq!(
        read(root, &format!("{EC_BASE}/gpu_nv_ctgp/current_value")),
        "65"
    );
}

#[test]
fn ensure_custom_goes_through_balanced_fan_basis() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    ec_fixture(root, "performance");
    let sysfs = Sysfs::with_root(root);

    let reports = platform::ensure_custom(&sysfs, false).unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(reports[0].value, "balanced");
    assert_eq!(reports[1].value, "custom");
    assert_eq!(read(root, &format!("{GAMEZONE}/profile")), "custom");

    let again = platform::ensure_custom(&sysfs, false).unwrap();
    assert!(again.is_empty());
}

#[test]
fn platform_rejects_unknown_mode() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    ec_fixture(root, "balanced");
    let sysfs = Sysfs::with_root(root);

    let error = platform::set_profile(&sysfs, "turbo", false).unwrap_err();
    assert!(matches!(error, loq_core::Error::InvalidValue(_)));
}

#[test]
fn charge_type_must_be_advertised() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let base = "/sys/class/power_supply/BAT1";
    write(root, &format!("{base}/type"), "Battery\n");
    write(
        root,
        &format!("{base}/charge_types"),
        "Fast Standard [Long_Life]\n",
    );
    let sysfs = Sysfs::with_root(root);

    let error = battery::set_charge_type(&sysfs, "BAT1", "Turbo", false).unwrap_err();
    assert!(matches!(error, loq_core::Error::InvalidValue(_)));

    // Real hardware keeps the whole choice list on readback, which is why the
    // comparison looks for the bracketed entry instead of an exact match.
    let sysfs = Sysfs::with_root(root);
    let report = battery::set_charge_type(&sysfs, "BAT1", "standard", true).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.value, "Standard");
}

#[test]
fn fan_mode_accepts_only_documented_values() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let base = "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00";
    write(root, &format!("{base}/fan_mode"), "1\n");
    write(root, &format!("{base}/fn_lock"), "0\n");
    let sysfs = Sysfs::with_root(root);

    assert!(ideapad::set_fan_mode(&sysfs, 3, false).is_err());

    let report = ideapad::set_fan_mode(&sysfs, 2, false).unwrap();
    assert!(report.verified);
    assert_eq!(read(root, &format!("{base}/fan_mode")), "2");

    let report = ideapad::set_flag(&sysfs, "fn_lock", true, false).unwrap();
    assert!(report.verified);
    assert_eq!(read(root, &format!("{base}/fn_lock")), "1");

    let error = ideapad::set_flag(&sysfs, "camera_power", true, false).unwrap_err();
    assert!(matches!(error, loq_core::Error::NotWritable(_)));
}

#[test]
fn cpu_writes_cover_every_core() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for cpu in 0..2 {
        let base = format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq");
        write(root, &format!("{base}/scaling_governor"), "performance\n");
        write(
            root,
            &format!("{base}/scaling_available_governors"),
            "performance powersave\n",
        );
        write(
            root,
            &format!("{base}/energy_performance_preference"),
            "performance\n",
        );
        write(
            root,
            &format!("{base}/energy_performance_available_preferences"),
            "performance powersave\n",
        );
        write(root, &format!("{base}/scaling_min_freq"), "1800000\n");
        write(root, &format!("{base}/scaling_max_freq"), "4000000\n");
        write(root, &format!("{base}/cpuinfo_min_freq"), "120000\n");
        write(root, &format!("{base}/cpuinfo_max_freq"), "4000000\n");
    }
    write(root, "/sys/devices/system/cpu/cpufreq/boost", "1\n");
    let sysfs = Sysfs::with_root(root);

    let reports = cpu::set_governor(&sysfs, "powersave", false).unwrap();
    assert_eq!(reports.len(), 2);
    for cpu in 0..2 {
        assert_eq!(
            read(
                root,
                &format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_governor")
            ),
            "powersave"
        );
    }

    let reports = cpu::set_frequency_range(&sysfs, Some(400_000), Some(2_800_000), false).unwrap();
    assert_eq!(reports.len(), 4);
    assert_eq!(
        read(
            root,
            "/sys/devices/system/cpu/cpu1/cpufreq/scaling_max_freq"
        ),
        "2800000"
    );

    assert!(cpu::set_frequency_range(&sysfs, Some(3_000_000), Some(2_800_000), false).is_err());
    assert!(cpu::set_epp(&sysfs, "low-power", false).is_err());

    cpu::set_boost(&sysfs, false, false).unwrap();
    assert_eq!(read(root, "/sys/devices/system/cpu/cpufreq/boost"), "0");
}

#[test]
fn led_brightness_is_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let base = "/sys/class/leds/platform::kbd_backlight";
    write(root, &format!("{base}/brightness"), "1\n");
    write(root, &format!("{base}/max_brightness"), "2\n");
    let sysfs = Sysfs::with_root(root);

    assert!(leds::set_brightness(&sysfs, "platform::kbd_backlight", 3, false).is_err());
    leds::set_brightness(&sysfs, "platform::kbd_backlight", 2, false).unwrap();
    assert_eq!(read(root, &format!("{base}/brightness")), "2");
}

#[test]
fn write_rejects_missing_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let sysfs = Sysfs::with_root(root);

    let error = leds::set_brightness(&sysfs, "platform::none", 1, false).unwrap_err();
    assert!(matches!(error, loq_core::Error::NotFound(_)));
}
