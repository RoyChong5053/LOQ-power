//! Orchestration of a validated [`ApplyRequest`].
//!
//! Ordering matters on this machine:
//! 1. EC limits can only be written in CUSTOM mode, so the platform is moved
//!    to `balanced` (fan basis) and then `custom` first.
//! 2. EC attributes are written and read back.
//! 3. CPU/battery/ideapad/LED settings are applied.
//! 4. The requested thermal mode is asserted last, which keeps the EC values
//!    but restores the fan policy.

use crate::battery;
use crate::cpu;
use crate::ec;
use crate::error::Result;
use crate::ideapad;
use crate::leds;
use crate::legion;
use crate::platform;
use crate::request::{ApplyRequest, ApplyResponse};
use crate::sysfs::Sysfs;
use crate::write::WriteReport;

fn record<T>(response: &mut ApplyResponse, result: Result<T>, step: &str)
where
    T: IntoWriteReports,
{
    match result {
        Ok(value) => response.writes.extend(value.into_reports()),
        Err(error) => response.errors.push(format!("{step}: {error}")),
    }
}

/// Helper trait so `WriteReport` and `Vec<WriteReport>` can share the same
/// error-recording path.
pub trait IntoWriteReports {
    fn into_reports(self) -> Vec<WriteReport>;
}

impl IntoWriteReports for WriteReport {
    fn into_reports(self) -> Vec<WriteReport> {
        vec![self]
    }
}

impl IntoWriteReports for Vec<WriteReport> {
    fn into_reports(self) -> Vec<WriteReport> {
        self
    }
}

pub fn apply(fs: &Sysfs, request: &ApplyRequest) -> ApplyResponse {
    let mut response = ApplyResponse {
        ok: true,
        dry_run: request.dry_run,
        ..Default::default()
    };

    if request.is_empty() {
        return ApplyResponse::error("nothing to apply");
    }

    // 1. EC limits, with the CUSTOM-mode dance first.
    if !request.ec.is_empty() {
        let mut pending = Vec::new();
        for (name, value) in &request.ec {
            match ec::current(fs, name) {
                Some(current) if current == *value => {
                    response.skipped.push(format!("{name} already at {value}"));
                }
                Some(_) => pending.push((name.clone(), *value)),
                None => response
                    .errors
                    .push(format!("ec: attribute {name} not found on this system")),
            }
        }

        if !pending.is_empty() {
            match platform::ensure_custom(fs, request.dry_run) {
                Ok(reports) => response.writes.extend(reports),
                Err(error) => {
                    response.errors.push(format!("platform: {error}"));
                    // Without CUSTOM mode every EC write would fail, so stop
                    // here instead of producing a pile of identical errors.
                    pending.clear();
                }
            }

            for (name, value) in pending {
                record(
                    &mut response,
                    ec::set_attribute(fs, &name, value, request.dry_run),
                    &name,
                );
            }
        }
    }

    // 2. CPU tuning.
    if let Some(cpu_request) = &request.cpu {
        if let Some(governor) = &cpu_request.governor {
            record(
                &mut response,
                cpu::set_governor(fs, governor, request.dry_run),
                "cpu.governor",
            );
        }
        if let Some(epp) = &cpu_request.epp {
            record(
                &mut response,
                cpu::set_epp(fs, epp, request.dry_run),
                "cpu.epp",
            );
        }
        if cpu_request.min_khz.is_some() || cpu_request.max_khz.is_some() {
            record(
                &mut response,
                cpu::set_frequency_range(
                    fs,
                    cpu_request.min_khz,
                    cpu_request.max_khz,
                    request.dry_run,
                ),
                "cpu.frequency",
            );
        }
        if let Some(boost) = cpu_request.boost {
            record(
                &mut response,
                cpu::set_boost(fs, boost, request.dry_run),
                "cpu.boost",
            );
        }
    }

    // 3. Battery charge type.
    if let Some(battery_request) = &request.battery {
        let name = battery_request
            .name
            .clone()
            .or_else(|| battery::primary(fs).map(|battery| battery.name))
            .unwrap_or_else(|| "BAT1".to_string());

        let unchanged = battery::primary(fs)
            .map(|battery| {
                battery
                    .charge_type
                    .as_deref()
                    .map(|current| current.eq_ignore_ascii_case(&battery_request.charge_type))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if unchanged {
            response.skipped.push(format!(
                "charge_type already {}",
                battery_request.charge_type
            ));
        } else {
            record(
                &mut response,
                battery::set_charge_type(fs, &name, &battery_request.charge_type, request.dry_run),
                "battery.charge_type",
            );
        }
    }

    // 4. Ideapad controls.
    if let Some(ideapad_request) = &request.ideapad {
        if let Some(fan_mode) = ideapad_request.fan_mode {
            record(
                &mut response,
                ideapad::set_fan_mode(fs, fan_mode, request.dry_run),
                "ideapad.fan_mode",
            );
        }
        if let Some(fn_lock) = ideapad_request.fn_lock {
            record(
                &mut response,
                ideapad::set_flag(fs, "fn_lock", fn_lock, request.dry_run),
                "ideapad.fn_lock",
            );
        }
    }

    // 5. LED brightness.
    if let Some(led) = &request.led {
        record(
            &mut response,
            leds::set_brightness(fs, &led.name, led.brightness, request.dry_run),
            "led",
        );
    }

    // 6. Final thermal mode. Skipped when it is already active so that
    //    writing EC limits in CUSTOM does not bounce the firmware needlessly.
    if let Some(target) = &request.platform {
        if platform::detect(fs).current.as_deref() != Some(target.as_str()) {
            record(
                &mut response,
                platform::set_profile(fs, target, request.dry_run),
                "platform",
            );
        } else {
            response.skipped.push(format!("platform already {target}"));
        }
    }

    // 7. Optional LenovoLegionLinux controls. The fan curve is written last
    //    because an LLL power-mode change reloads the firmware curve.
    if let Some(legion_request) = &request.legion {
        if let Some(fullspeed) = legion_request.fullspeed {
            record(
                &mut response,
                legion::set_fullspeed(fs, fullspeed, request.dry_run),
                "legion.fullspeed",
            );
        }
        if let Some(powermode) = legion_request.powermode {
            record(
                &mut response,
                legion::set_powermode(fs, powermode, request.dry_run),
                "legion.powermode",
            );
        }
        if let Some(curve) = &legion_request.fan_curve {
            record(
                &mut response,
                legion::apply_curve(fs, curve, request.dry_run),
                "legion.fan_curve",
            );
        }
    }

    response.ok = response.errors.is_empty();
    response
}
