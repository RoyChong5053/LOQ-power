//! Explicit hand-over from TLP to LOQ Power.
//!
//! Nothing here runs automatically. The user calls `loq-power takeover` from
//! the post-install wizard (or `loq-power restore-tlp` to roll back), and the
//! current TLP state is written to `/etc/loq-power/takeover.json` first so the
//! rollback restores exactly what was there before.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::sysfs::Sysfs;
use crate::tlp;

/// Unit masks we are allowed to touch.
pub const TLP_UNITS: [&str; 2] = ["tlp.service", "tlp-pd.service"];
pub const DAEMON_UNIT: &str = "loq-powerd.service";

pub fn state_path() -> PathBuf {
    if let Some(path) = std::env::var_os("LOQ_POWER_TAKEOVER") {
        return PathBuf::from(path);
    }
    PathBuf::from("/etc/loq-power/takeover.json")
}

pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("LOQ_POWER_CONFIG") {
        return PathBuf::from(path);
    }
    crate::profile::system_path()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnitState {
    pub unit: String,
    pub enabled: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Backup {
    pub created: u64,
    pub tlp: Vec<UnitState>,
    /// Whether `/etc/loq-power/profiles.toml` existed before the takeover and
    /// its previous contents, so a rollback can restore the exact file.
    pub profiles_existed: bool,
    pub profiles_backup: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub action: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub dry_run: bool,
    pub steps: Vec<Step>,
    pub notes: Vec<String>,
    pub errors: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn run(program: &str, args: &[&str], dry_run: bool) -> (bool, String) {
    if dry_run {
        return (true, format!("would run {program} {}", args.join(" ")));
    }
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => (true, String::new()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let message = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            };
            (false, message)
        }
        Err(error) => (false, format!("cannot run {program}: {error}")),
    }
}

fn unit_state(unit: &str) -> UnitState {
    UnitState {
        unit: unit.to_string(),
        enabled: crate::services::is_enabled(unit).unwrap_or(false),
        active: crate::services::is_active(unit).unwrap_or(false),
    }
}

fn daemon_installed() -> bool {
    crate::services::command_exists("systemctl")
        && Command::new("systemctl")
            .args(["cat", DAEMON_UNIT])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
}

/// Write the rollback record before anything is disabled.
fn write_backup(path: &Path, backup: &Backup) -> Result<()> {
    let content = serde_json::to_string_pretty(backup)
        .map_err(|error| Error::Other(format!("cannot serialize takeover backup: {error}")))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, content).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn read_backup(path: &Path) -> Result<Backup> {
    let content = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&content)
        .map_err(|error| Error::InvalidValue(format!("cannot parse {}: {error}", path.display())))
}

/// Stop and disable TLP, then enable and start `loq-powerd`.
///
/// `import_profiles` rewrites `/etc/loq-power/profiles.toml` from the current
/// TLP configuration. It is only done when the file does not exist yet, unless
/// `force` is set, and the previous contents are kept in the backup.
pub fn takeover(fs: &Sysfs, dry_run: bool, import_profiles: bool, force: bool) -> Result<Report> {
    let mut report = Report {
        dry_run,
        ..Default::default()
    };

    let config = config_path();
    let backup_path = state_path();

    if backup_path.exists() && !force {
        return Err(Error::InvalidValue(format!(
            "{} already exists; run `loq-power restore-tlp` first, or pass --force",
            backup_path.display()
        )));
    }

    if !dry_run && !daemon_installed() {
        report.notes.push(format!(
            "{DAEMON_UNIT} 未安装；接管后会停用 TLP，但 LOQ Power 守护进程无法自动启动"
        ));
    }

    let tlp_state: Vec<UnitState> = TLP_UNITS.iter().map(|unit| unit_state(unit)).collect();

    let profiles_existed = config.exists();
    let profiles_backup = if profiles_existed {
        std::fs::read_to_string(&config).ok()
    } else {
        None
    };

    let backup = Backup {
        created: now(),
        tlp: tlp_state.clone(),
        profiles_existed,
        profiles_backup,
    };

    if dry_run {
        let json = serde_json::to_string_pretty(&backup).unwrap_or_default();
        report.steps.push(Step {
            action: format!("write {}", backup_path.display()),
            ok: true,
            detail: json,
        });
    } else {
        write_backup(&backup_path, &backup)?;
        report.steps.push(Step {
            action: format!("write {}", backup_path.display()),
            ok: true,
            detail: "rollback record saved".to_string(),
        });
    }

    if import_profiles && (force || !profiles_existed) {
        let imported = crate::tlp_import::import(fs);
        report.notes.extend(imported.notes.clone());

        if dry_run {
            report.steps.push(Step {
                action: format!("write {}", config.display()),
                ok: true,
                detail: format!(
                    "would import {} TLP profiles",
                    imported.profiles.profile.len()
                ),
            });
        } else {
            match imported.profiles.save(&config) {
                Ok(()) => report.steps.push(Step {
                    action: format!("write {}", config.display()),
                    ok: true,
                    detail: format!("imported {} TLP profiles", imported.profiles.profile.len()),
                }),
                Err(error) => {
                    report
                        .errors
                        .push(format!("cannot write {}: {error}", config.display()));
                }
            }
        }
    } else if import_profiles {
        report.notes.push(format!(
            "{} 已存在，未覆盖（使用 --force 重新导入）",
            config.display()
        ));
    }

    for unit in TLP_UNITS {
        let (stopped, detail) = run("systemctl", &["stop", unit], dry_run);
        let (disabled, detail2) = run("systemctl", &["disable", unit], dry_run);
        report.steps.push(Step {
            action: format!("stop {unit}"),
            ok: stopped,
            detail,
        });
        report.steps.push(Step {
            action: format!("disable {unit}"),
            ok: disabled,
            detail: detail2,
        });
        if !stopped || !disabled {
            report
                .errors
                .push(format!("cannot hand over {unit}; TLP state was saved"));
        }
    }

    if dry_run || daemon_installed() {
        let (enabled, detail) = run("systemctl", &["enable", DAEMON_UNIT], dry_run);
        report.steps.push(Step {
            action: format!("enable {DAEMON_UNIT}"),
            ok: enabled,
            detail,
        });
        if !enabled {
            report.errors.push(format!("cannot enable {DAEMON_UNIT}"));
        }

        let (started, detail) = run("systemctl", &["start", DAEMON_UNIT], dry_run);
        report.steps.push(Step {
            action: format!("start {DAEMON_UNIT}"),
            ok: started,
            detail,
        });
        if !started {
            report.errors.push(format!("cannot start {DAEMON_UNIT}"));
        }
    }

    report
        .notes
        .push("TLP 已停用。反向操作：`sudo loq-power restore-tlp`".to_string());

    Ok(report)
}

/// Undo a takeover using the saved rollback record.
pub fn restore_tlp(dry_run: bool) -> Result<Report> {
    restore_inner(dry_run, true)
}

/// Same as [`restore_tlp`], but leaves the running daemon alive so it can
/// answer the D-Bus call that asked for the rollback. The daemon stops itself
/// afterwards.
pub fn restore_tlp_keep_daemon(dry_run: bool) -> Result<Report> {
    restore_inner(dry_run, false)
}

fn restore_inner(dry_run: bool, stop_daemon: bool) -> Result<Report> {
    let mut report = Report {
        dry_run,
        ..Default::default()
    };

    let backup_path = state_path();
    let backup = if backup_path.exists() {
        read_backup(&backup_path)?
    } else {
        report.notes.push(format!(
            "{} 不存在；仅恢复 TLP 单元，不还原配置",
            backup_path.display()
        ));
        Backup::default()
    };

    if stop_daemon {
        let (_, detail) = run("systemctl", &["stop", DAEMON_UNIT], dry_run);
        report.steps.push(Step {
            action: format!("stop {DAEMON_UNIT}"),
            ok: true,
            detail,
        });
    }
    let (_, detail) = run("systemctl", &["disable", DAEMON_UNIT], dry_run);
    report.steps.push(Step {
        action: format!("disable {DAEMON_UNIT}"),
        ok: true,
        detail,
    });

    if backup.tlp.is_empty() {
        for unit in TLP_UNITS {
            let (enabled, detail) = run("systemctl", &["enable", unit], dry_run);
            report.steps.push(Step {
                action: format!("enable {unit}"),
                ok: enabled,
                detail,
            });
            let (started, detail) = run("systemctl", &["start", unit], dry_run);
            report.steps.push(Step {
                action: format!("start {unit}"),
                ok: started,
                detail,
            });
        }
    } else {
        for unit in &backup.tlp {
            if unit.enabled {
                let (ok, detail) = run("systemctl", &["enable", &unit.unit], dry_run);
                report.steps.push(Step {
                    action: format!("enable {}", unit.unit),
                    ok,
                    detail,
                });
                if !ok {
                    report.errors.push(format!("cannot enable {}", unit.unit));
                }
            }
            if unit.active {
                let (ok, detail) = run("systemctl", &["start", &unit.unit], dry_run);
                report.steps.push(Step {
                    action: format!("start {}", unit.unit),
                    ok,
                    detail,
                });
                if !ok {
                    report.errors.push(format!("cannot start {}", unit.unit));
                }
            }
        }
    }

    let config = config_path();
    if let Some(previous) = &backup.profiles_backup {
        if dry_run {
            report.steps.push(Step {
                action: format!("restore {}", config.display()),
                ok: true,
                detail: "would restore the pre-takeover file".to_string(),
            });
        } else {
            std::fs::write(&config, previous).map_err(|source| Error::Write {
                path: config.clone(),
                source,
            })?;
            report.steps.push(Step {
                action: format!("restore {}", config.display()),
                ok: true,
                detail: "pre-takeover file restored".to_string(),
            });
        }
    } else if !backup.profiles_existed && !dry_run {
        match std::fs::remove_file(&config) {
            Ok(()) => report.steps.push(Step {
                action: format!("remove {}", config.display()),
                ok: true,
                detail: "file was created by the takeover".to_string(),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => report
                .errors
                .push(format!("cannot remove {}: {error}", config.display())),
        }
    }

    if !dry_run {
        match std::fs::remove_file(&backup_path) {
            Ok(()) => report.steps.push(Step {
                action: format!("remove {}", backup_path.display()),
                ok: true,
                detail: "rollback complete".to_string(),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => report
                .errors
                .push(format!("cannot remove {}: {error}", backup_path.display())),
        }
    }

    Ok(report)
}

/// Quick summary used by `status` and the GUI.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TakeoverStatus {
    pub taken_over: bool,
    pub rollback_available: bool,
    pub tlp: tlp::TlpInfo,
}

pub fn status(fs: &Sysfs) -> TakeoverStatus {
    let tlp = tlp::detect(fs);
    let rollback = state_path().exists();
    TakeoverStatus {
        taken_over: !rollback
            && tlp.installed
            && tlp.service_active == Some(false)
            && crate::services::is_active(DAEMON_UNIT) == Some(true),
        rollback_available: rollback,
        tlp,
    }
}
