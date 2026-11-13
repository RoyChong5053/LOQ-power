use std::fs;
use std::sync::{Mutex, MutexGuard};

use loq_core::Sysfs;
use loq_core::takeover;

/// The takeover paths are process-wide environment variables.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_env() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

#[test]
fn dry_run_writes_no_files_and_lists_every_step() {
    let _guard = lock_env();
    let temp = tempfile::tempdir().unwrap();
    let record = temp.path().join("takeover.json");
    let config = temp.path().join("profiles.toml");
    unsafe {
        std::env::set_var("LOQ_POWER_TAKEOVER", &record);
        std::env::set_var("LOQ_POWER_CONFIG", &config);
    }

    let report = takeover::takeover(&Sysfs::with_root(temp.path()), true, true, false).unwrap();

    assert!(report.ok(), "errors: {:?}", report.errors);
    assert!(
        !record.exists(),
        "dry run must not write the rollback record"
    );
    assert!(!config.exists(), "dry run must not write profiles");
    let actions: Vec<&str> = report
        .steps
        .iter()
        .map(|step| step.action.as_str())
        .collect();
    assert!(actions.iter().any(|action| action.starts_with("write ")));
    assert!(actions.contains(&"stop tlp.service"));
    assert!(actions.contains(&"disable tlp-pd.service"));
    assert!(actions.contains(&"enable loq-powerd.service"));
}

#[test]
fn status_reports_a_rollback_record() {
    let _guard = lock_env();
    let temp = tempfile::tempdir().unwrap();
    let record = temp.path().join("takeover.json");
    fs::write(&record, "{}").unwrap();
    unsafe {
        std::env::set_var("LOQ_POWER_TAKEOVER", &record);
        std::env::set_var("LOQ_POWER_CONFIG", temp.path().join("profiles.toml"));
    }

    let status = takeover::status(&Sysfs::with_root(temp.path()));
    assert!(status.rollback_available);
    assert!(!status.taken_over, "TLP is still active on the test host");
}

#[test]
fn refuses_to_overwrite_an_existing_rollback_record() {
    let _guard = lock_env();
    let temp = tempfile::tempdir().unwrap();
    let record = temp.path().join("takeover.json");
    fs::write(&record, "{}").unwrap();
    unsafe {
        std::env::set_var("LOQ_POWER_TAKEOVER", &record);
        std::env::set_var("LOQ_POWER_CONFIG", temp.path().join("profiles.toml"));
    }

    let error = takeover::takeover(&Sysfs::with_root(temp.path()), true, true, false).unwrap_err();
    assert!(
        error.to_string().contains("already exists"),
        "unexpected error: {error}"
    );
}
