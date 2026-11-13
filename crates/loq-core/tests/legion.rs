use std::fs;
use std::path::Path;

use loq_core::Sysfs;
use loq_core::legion::{self, FanCurve};

fn write(root: &Path, path: &str, content: &str) {
    let full = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path.trim_start_matches('/'))).unwrap()
}

/// A three point curve for two fans and three temperature sensors.
fn legion_tree(root: &Path) {
    let base = "/sys/class/hwmon/hwmon5";
    write(root, &format!("{base}/name"), "legion_hwmon\n");
    write(root, &format!("{base}/fan1_input"), "1800\n");
    write(root, &format!("{base}/fan2_input"), "1900\n");
    write(root, &format!("{base}/temp1_input"), "45000\n");
    write(root, &format!("{base}/temp2_input"), "42000\n");

    for point in 1..=3 {
        write(
            root,
            &format!("{base}/pwm1_auto_point{point}_pwm"),
            &format!("{}\n", 60 * point),
        );
        write(
            root,
            &format!("{base}/pwm2_auto_point{point}_pwm"),
            &format!("{}\n", 55 * point),
        );
        write(
            root,
            &format!("{base}/pwm1_auto_point{point}_temp"),
            &format!("{}\n", 40 + 10 * point),
        );
        write(
            root,
            &format!("{base}/pwm2_auto_point{point}_temp"),
            &format!("{}\n", 45 + 10 * point),
        );
        write(
            root,
            &format!("{base}/pwm3_auto_point{point}_temp"),
            &format!("{}\n", 50 + 10 * point),
        );
    }

    write(root, "/sys/bus/platform/devices/legion/powermode", "0\n");
    write(
        root,
        "/sys/bus/platform/devices/legion/fan_fullspeed",
        "0\n",
    );
}

#[test]
fn detects_curve_and_controls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    legion_tree(root);

    let info = legion::detect(&Sysfs::with_root(root));
    assert!(info.installed);
    assert_eq!(info.fullspeed, Some(false));
    assert_eq!(info.powermode.as_deref(), Some("0"));
    assert_eq!(info.hwmon_path.as_deref(), Some("/sys/class/hwmon/hwmon5"));

    let curve = info.fan_curve.unwrap();
    assert_eq!(curve.points, 3);
    assert_eq!(curve.fan1_pwm, vec![60, 120, 180]);
    assert_eq!(curve.fan2_pwm, Some(vec![55, 110, 165]));
    assert_eq!(curve.cpu_temp, vec![50, 60, 70]);
    assert_eq!(curve.gpu_temp, Some(vec![55, 65, 75]));
    assert_eq!(curve.ic_temp, Some(vec![60, 70, 80]));
}

#[test]
fn applies_a_curve_to_the_fake_sysfs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    legion_tree(root);
    let fs = Sysfs::with_root(root);

    let curve = FanCurve {
        points: 3,
        fan1_pwm: vec![70, 140, 210],
        fan2_pwm: Some(vec![65, 130, 195]),
        cpu_temp: vec![48, 58, 68],
        gpu_temp: Some(vec![53, 63, 73]),
        ic_temp: Some(vec![58, 68, 78]),
    };

    let reports = legion::apply_curve(&fs, &curve, false).unwrap();
    assert_eq!(reports.len(), 3 * 3 + 3 * 2);
    assert!(reports.iter().all(|report| report.verified));

    assert_eq!(
        read(root, "/sys/class/hwmon/hwmon5/pwm1_auto_point1_pwm"),
        "70\n"
    );
    assert_eq!(
        read(root, "/sys/class/hwmon/hwmon5/pwm2_auto_point3_pwm"),
        "195\n"
    );
    assert_eq!(
        read(root, "/sys/class/hwmon/hwmon5/pwm3_auto_point2_temp"),
        "68\n"
    );

    // The curve reads back identically.
    let info = legion::detect(&fs);
    let readback = info.fan_curve.unwrap();
    assert_eq!(readback.fan1_pwm, curve.fan1_pwm);
    assert_eq!(readback.cpu_temp, curve.cpu_temp);
}

#[test]
fn dry_run_does_not_touch_the_curve() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    legion_tree(root);
    let fs = Sysfs::with_root(root);

    let curve = FanCurve {
        points: 3,
        fan1_pwm: vec![255, 255, 255],
        fan2_pwm: Some(vec![255, 255, 255]),
        cpu_temp: vec![40, 40, 40],
        gpu_temp: Some(vec![40, 40, 40]),
        ic_temp: Some(vec![40, 40, 40]),
    };

    let reports = legion::apply_curve(&fs, &curve, true).unwrap();
    assert!(reports.iter().all(|report| report.dry_run));
    assert_eq!(
        read(root, "/sys/class/hwmon/hwmon5/pwm1_auto_point1_pwm"),
        "60\n"
    );
}

#[test]
fn rejects_curves_that_do_not_match_the_hardware() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    legion_tree(root);
    let fs = Sysfs::with_root(root);

    let mut curve = legion::detect(&fs).fan_curve.unwrap();
    curve.points = 2;
    assert!(legion::apply_curve(&fs, &curve, true).is_err());

    let mut curve = legion::detect(&fs).fan_curve.unwrap();
    curve.cpu_temp = vec![70, 60, 50];
    let error = legion::apply_curve(&fs, &curve, true).unwrap_err();
    assert!(error.to_string().contains("decrease"), "{error}");

    let mut curve = legion::detect(&fs).fan_curve.unwrap();
    curve.fan1_pwm = vec![0, 0];
    assert!(legion::apply_curve(&fs, &curve, true).is_err());
}

#[test]
fn reports_missing_module() {
    let temp = tempfile::tempdir().unwrap();
    let fs = Sysfs::with_root(temp.path());
    assert!(!legion::detect(&fs).installed);
    assert!(legion::set_fullspeed(&fs, true, true).is_err());
}
