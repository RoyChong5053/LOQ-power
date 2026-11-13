use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use loq_core::request::{ApplyRequest, BatteryRequest, IdeapadRequest, LedRequest};
use loq_core::{Sysfs, apply};

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
const GAMEZONE: &str =
    "/sys/bus/wmi/drivers/lenovo_wmi_gamezone/abc/platform-profile/platform-profile-0";

fn fixture(root: &Path) {
    write(
        root,
        &format!("{EC_BASE}/ppt_pl1_spl/current_value"),
        "45\n",
    );
    write(root, &format!("{EC_BASE}/ppt_pl1_spl/min_value"), "40\n");
    write(root, &format!("{EC_BASE}/ppt_pl1_spl/max_value"), "75\n");
    write(
        root,
        &format!("{EC_BASE}/ppt_pl1_spl/scalar_increment"),
        "1\n",
    );

    write(root, &format!("{GAMEZONE}/profile"), "balanced\n");
    write(
        root,
        &format!("{GAMEZONE}/choices"),
        "low-power balanced performance max-power custom\n",
    );

    write(root, "/sys/class/power_supply/BAT1/type", "Battery\n");
    write(
        root,
        "/sys/class/power_supply/BAT1/charge_types",
        "[Standard] Fast Long_Life\n",
    );

    let ideapad = "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00";
    write(root, &format!("{ideapad}/fan_mode"), "1\n");
    write(root, &format!("{ideapad}/fn_lock"), "0\n");

    let led = "/sys/class/leds/platform::kbd_backlight";
    write(root, &format!("{led}/brightness"), "0\n");
    write(root, &format!("{led}/max_brightness"), "2\n");
}

#[test]
fn applies_ec_then_switches_to_target_mode() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fixture(root);
    let sysfs = Sysfs::with_root(root);

    let request = ApplyRequest {
        ec: BTreeMap::from([("ppt_pl1_spl".to_string(), 54)]),
        platform: Some("performance".to_string()),
        battery: Some(BatteryRequest {
            name: None,
            charge_type: "Long_Life".to_string(),
        }),
        ideapad: Some(IdeapadRequest {
            fan_mode: Some(1),
            fn_lock: Some(true),
        }),
        led: Some(LedRequest {
            name: "platform::kbd_backlight".to_string(),
            brightness: 2,
        }),
        ..Default::default()
    };

    let response = apply::apply(&sysfs, &request);
    assert!(response.ok, "errors: {:?}", response.errors);

    // EC value survives the switch back to performance.
    assert_eq!(
        read(root, &format!("{EC_BASE}/ppt_pl1_spl/current_value")),
        "54"
    );
    assert_eq!(read(root, &format!("{GAMEZONE}/profile")), "performance");
    assert_eq!(
        read(root, "/sys/class/power_supply/BAT1/charge_types"),
        "Long_Life"
    );
    assert_eq!(
        read(
            root,
            "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/fn_lock"
        ),
        "1"
    );
    assert_eq!(
        read(root, "/sys/class/leds/platform::kbd_backlight/brightness"),
        "2"
    );

    // Ordering: balanced (fan basis), custom (unlock), EC write, target.
    let values: Vec<&str> = response
        .writes
        .iter()
        .map(|report| report.value.as_str())
        .collect();
    assert_eq!(values[0], "balanced");
    assert_eq!(values[1], "custom");
}

#[test]
fn dry_run_touches_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fixture(root);
    let sysfs = Sysfs::with_root(root);

    let request = ApplyRequest {
        ec: BTreeMap::from([("ppt_pl1_spl".to_string(), 75)]),
        platform: Some("max-power".to_string()),
        dry_run: true,
        ..Default::default()
    };

    let response = apply::apply(&sysfs, &request);
    assert!(response.ok);
    assert!(response.writes.iter().all(|report| report.dry_run));
    assert_eq!(
        read(root, &format!("{EC_BASE}/ppt_pl1_spl/current_value")),
        "45"
    );
    assert_eq!(read(root, &format!("{GAMEZONE}/profile")), "balanced");
}

#[test]
fn skips_values_that_already_match() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fixture(root);
    let sysfs = Sysfs::with_root(root);

    let request = ApplyRequest {
        ec: BTreeMap::from([("ppt_pl1_spl".to_string(), 45)]),
        battery: Some(BatteryRequest {
            name: Some("BAT1".to_string()),
            charge_type: "Standard".to_string(),
        }),
        ..Default::default()
    };

    let response = apply::apply(&sysfs, &request);
    assert!(response.ok);
    assert!(response.writes.is_empty());
    assert_eq!(response.skipped.len(), 2);
    assert_eq!(read(root, &format!("{GAMEZONE}/profile")), "balanced");
}

#[test]
fn unknown_ec_attribute_is_reported_without_blocking_others() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fixture(root);
    let sysfs = Sysfs::with_root(root);

    let request = ApplyRequest {
        ec: BTreeMap::from([("does_not_exist".to_string(), 1)]),
        battery: Some(BatteryRequest {
            name: None,
            charge_type: "Fast".to_string(),
        }),
        ..Default::default()
    };

    let response = apply::apply(&sysfs, &request);
    assert!(!response.ok);
    assert!(response.errors[0].contains("does_not_exist"));
    assert_eq!(
        read(root, "/sys/class/power_supply/BAT1/charge_types"),
        "Fast"
    );
}

#[test]
fn empty_request_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fixture(root);
    let sysfs = Sysfs::with_root(root);

    let response = apply::apply(&sysfs, &ApplyRequest::default());
    assert!(!response.ok);
    assert!(response.errors[0].contains("nothing to apply"));
}
