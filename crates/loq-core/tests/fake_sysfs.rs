use std::fs;
use std::path::Path;

use loq_core::{Sysfs, battery, cpu, ec, hwmon, ideapad, leds, platform};

fn write(root: &Path, path: &str, content: &str) {
    let full = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

#[test]
fn discovers_ec_attributes_in_known_order() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/gpu_nv_ctgp/current_value",
        "55\n",
    );
    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/gpu_nv_ctgp/min_value",
        "55\n",
    );
    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/gpu_nv_ctgp/max_value",
        "105\n",
    );
    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/gpu_nv_ctgp/default_value",
        "60\n",
    );

    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/ppt_pl1_spl/current_value",
        "45\n",
    );
    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/ppt_pl1_spl/min_value",
        "40\n",
    );
    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/ppt_pl1_spl/max_value",
        "75\n",
    );
    write(
        root,
        "/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes/ppt_pl1_spl/scalar_increment",
        "1\n",
    );

    let fs = Sysfs::with_root(root);
    let info = ec::discover(&fs);

    assert_eq!(
        info.base.as_deref(),
        Some("/sys/class/firmware-attributes/lenovo-wmi-other-0/attributes")
    );
    assert_eq!(info.attributes.len(), 2);
    assert_eq!(info.attributes[0].name, "gpu_nv_ctgp");
    assert_eq!(info.attributes[0].display_name, "GPU cTGP");
    assert_eq!(info.attributes[0].current, Some(55));
    assert_eq!(info.attributes[0].max, Some(105));
    assert_eq!(info.attributes[0].unit, "W");
    assert_eq!(info.attributes[1].name, "ppt_pl1_spl");
    assert_eq!(info.attributes[1].step, Some(1));
}

#[test]
fn parses_battery_charge_types() {
    let (current, choices) = battery::parse_charge_types("Fast Standard [Long_Life]");
    assert_eq!(current.as_deref(), Some("Long_Life"));
    assert_eq!(choices, ["Fast", "Standard", "Long_Life"]);
}

#[test]
fn detects_battery_and_ac() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    write(root, "/sys/class/power_supply/BAT1/type", "Battery\n");
    write(root, "/sys/class/power_supply/BAT1/capacity", "76\n");
    write(
        root,
        "/sys/class/power_supply/BAT1/status",
        "Not charging\n",
    );
    write(
        root,
        "/sys/class/power_supply/BAT1/charge_types",
        "Fast Standard [Long_Life]\n",
    );
    write(
        root,
        "/sys/class/power_supply/BAT1/energy_now",
        "46540000\n",
    );
    write(root, "/sys/class/power_supply/ACAD/type", "Mains\n");
    write(root, "/sys/class/power_supply/ACAD/online", "1\n");

    let fs = Sysfs::with_root(root);
    let info = battery::detect(&fs);

    assert_eq!(info.ac_online, Some(true));
    assert_eq!(info.ac_name.as_deref(), Some("ACAD"));
    assert_eq!(info.batteries.len(), 1);
    assert_eq!(info.batteries[0].capacity, Some(76));
    assert_eq!(info.batteries[0].charge_type.as_deref(), Some("Long_Life"));
    assert_eq!(info.batteries[0].energy_now_wh, Some(46.54));
}

#[test]
fn detects_gamezone_platform_profile() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let base = "/sys/bus/wmi/drivers/lenovo_wmi_gamezone/887B54E3-DDDC-4B2C-8B88-68A26A8835D0-21/platform-profile/platform-profile-0";

    write(root, &format!("{base}/profile"), "balanced\n");
    write(
        root,
        &format!("{base}/choices"),
        "low-power balanced performance max-power custom\n",
    );

    let fs = Sysfs::with_root(root);
    let info = platform::detect(&fs);

    assert_eq!(info.current.as_deref(), Some("balanced"));
    assert_eq!(
        info.choices,
        [
            "low-power",
            "balanced",
            "performance",
            "max-power",
            "custom"
        ]
    );
    assert!(info.gamezone_profile.is_some());
}

#[test]
fn detects_cpu_frequency_settings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    write(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver",
        "amd-pstate-epp\n",
    );
    write(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
        "performance\n",
    );
    write(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors",
        "performance powersave\n",
    );
    write(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference",
        "performance\n",
    );
    write(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_available_preferences",
        "performance balance_performance\n",
    );
    write(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq",
        "1800000\n",
    );
    write(
        root,
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq",
        "4000000\n",
    );
    write(
        root,
        "/sys/devices/system/cpu/cpu1/cpufreq/scaling_governor",
        "performance\n",
    );
    write(root, "/sys/devices/system/cpu/cpufreq/boost", "1\n");

    let fs = Sysfs::with_root(root);
    let info = cpu::detect(&fs);

    assert_eq!(info.cpus, 2);
    assert_eq!(info.driver.as_deref(), Some("amd-pstate-epp"));
    assert_eq!(info.governor.as_deref(), Some("performance"));
    assert_eq!(info.scaling_min_khz, Some(1_800_000));
    assert_eq!(info.scaling_max_khz, Some(4_000_000));
    assert_eq!(info.boost, Some(true));
    assert_eq!(info.available_epp, ["balance_performance", "performance"]);
}

#[test]
fn detects_ideapad_controls_and_skips_plumbing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let base = "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00";

    write(root, &format!("{base}/fan_mode"), "0\n");
    write(root, &format!("{base}/fn_lock"), "1\n");
    write(root, &format!("{base}/uevent"), "DRIVER=ideapad_acpi\n");
    write(root, &format!("{base}/modalias"), "acpi:VPC2004:\n");

    let fs = Sysfs::with_root(root);
    let info = ideapad::detect(&fs);

    assert_eq!(info.attributes.len(), 2);
    let fan_mode = info
        .attributes
        .iter()
        .find(|attribute| attribute.name == "fan_mode")
        .unwrap();
    assert!(fan_mode.known);
    assert_eq!(fan_mode.value.as_deref(), Some("0"));
    assert!(fan_mode.writable);
}

#[test]
fn detects_hwmon_sensors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let base = "/sys/class/hwmon/hwmon0";

    write(root, &format!("{base}/name"), "k10temp\n");
    write(root, &format!("{base}/temp1_input"), "69900\n");
    write(root, &format!("{base}/temp1_label"), "Tctl\n");
    write(root, &format!("{base}/fan1_input"), "2800\n");

    let fs = Sysfs::with_root(root);
    let devices = hwmon::detect(&fs);

    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "k10temp");
    assert_eq!(devices[0].temperatures[0].label, "Tctl");
    assert!((devices[0].temperatures[0].value - 69.9).abs() < 0.01);
    assert_eq!(devices[0].fans[0].value, 2800.0);
}

#[test]
fn detects_platform_leds() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let base = "/sys/class/leds/platform::kbd_backlight";

    write(root, &format!("{base}/brightness"), "1\n");
    write(root, &format!("{base}/max_brightness"), "2\n");

    let fs = Sysfs::with_root(root);
    let leds = leds::detect(&fs);

    assert_eq!(leds.len(), 1);
    assert_eq!(leds[0].name, "platform::kbd_backlight");
    assert_eq!(leds[0].brightness, Some(1));
    assert_eq!(leds[0].max_brightness, Some(2));
}

#[test]
fn charge_type_readback_matches_kernel_echo() {
    // The kernel keeps the full choice list and wraps the active entry.
    assert!(loq_core::write::current_choice(
        "Fast Standard [Long_Life]",
        "Long_Life"
    ));
    assert!(loq_core::write::current_choice(
        "[Fast] Standard Long_Life",
        "fast"
    ));
    assert!(!loq_core::write::current_choice(
        "[Fast] Standard Long_Life",
        "Long_Life"
    ));
}
