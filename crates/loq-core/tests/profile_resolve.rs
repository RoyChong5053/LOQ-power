use std::fs;
use std::path::Path;

use loq_core::Sysfs;
use loq_core::profile::{self, Profile};
use loq_core::request::CpuRequest;

fn write(root: &Path, path: &str, content: &str) {
    let full = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

fn cpufreq_tree(root: &Path) {
    let base = "/sys/devices/system/cpu/cpu0/cpufreq";
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
        "performance\n",
    );
    write(root, &format!("{base}/scaling_min_freq"), "1800000\n");
    write(root, &format!("{base}/scaling_max_freq"), "4000000\n");
    write(root, &format!("{base}/cpuinfo_min_freq"), "411505\n");
    write(root, &format!("{base}/cpuinfo_max_freq"), "4553983\n");
    write(root, "/sys/devices/system/cpu/cpufreq/boost", "1\n");
}

#[test]
fn drops_epp_values_the_kernel_does_not_support() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    cpufreq_tree(root);

    let profile = Profile {
        platform: Some("low-power".to_string()),
        cpu: Some(CpuRequest {
            governor: Some("powersave".to_string()),
            epp: Some("power".to_string()),
            min_khz: Some(100_000),
            max_khz: Some(9_000_000),
            boost: Some(true),
        }),
        ..Default::default()
    };

    let resolved = profile::resolve(&Sysfs::with_root(root), &profile);
    let cpu = resolved.request.cpu.unwrap();

    // `powersave` is available, so it is kept.
    assert_eq!(cpu.governor.as_deref(), Some("powersave"));
    // `power` is not available on the fake (AC) tree, so it is dropped.
    assert_eq!(cpu.epp, None);
    // Frequencies are clamped to the hardware range.
    assert_eq!(cpu.min_khz, Some(411_505));
    assert_eq!(cpu.max_khz, Some(4_553_983));

    assert!(
        resolved
            .warnings
            .iter()
            .any(|warning| warning.contains("EPP power")),
        "warnings: {:?}",
        resolved.warnings
    );
    assert!(
        resolved
            .warnings
            .iter()
            .any(|warning| warning.contains("最低频率")),
        "warnings: {:?}",
        resolved.warnings
    );
}

#[test]
fn drops_unknown_governors_with_a_warning() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    cpufreq_tree(root);

    let profile = Profile {
        cpu: Some(CpuRequest {
            governor: Some("schedutil".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = profile::resolve(&Sysfs::with_root(root), &profile);
    assert_eq!(resolved.request.cpu.unwrap().governor, None);
    assert!(
        resolved
            .warnings
            .iter()
            .any(|warning| warning.contains("schedutil")),
        "warnings: {:?}",
        resolved.warnings
    );
}
