use std::sync::RwLock;

use loq_core::profile::{ProfileFile, Resolved};
use loq_core::request::{ApplyRequest, ApplyResponse};
use loq_core::{Sysfs, apply, battery, platform, powersave, profile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    Ac,
    Battery,
    Unknown,
}

impl PowerSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerSource::Ac => "ac",
            PowerSource::Battery => "battery",
            PowerSource::Unknown => "unknown",
        }
    }

    pub fn detect(fs: &Sysfs) -> Self {
        match battery::detect(fs).ac_online {
            Some(true) => PowerSource::Ac,
            Some(false) => PowerSource::Battery,
            None => PowerSource::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Hold {
    pub cookie: u32,
    pub profile: String,
    pub reason: String,
    pub application_id: String,
}

/// The three profiles exposed through `net.hadess.PowerProfiles`.
pub const PPD_PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];

pub fn ppd_to_thermal(profile: &str) -> Option<&'static str> {
    match profile {
        "power-saver" => Some("low-power"),
        "balanced" => Some("balanced"),
        "performance" => Some("performance"),
        _ => None,
    }
}

pub fn thermal_to_ppd(thermal: &str) -> &'static str {
    match thermal {
        "low-power" => "power-saver",
        "performance" | "max-power" => "performance",
        "balanced" | "custom" => "balanced",
        _ => "balanced",
    }
}

fn ppd_rank(profile: &str) -> u8 {
    match profile {
        "performance" => 2,
        "balanced" => 1,
        _ => 0,
    }
}

pub struct State {
    pub fs: Sysfs,
    pub profiles: ProfileFile,
    pub active_profile: String,
    pub power_source: PowerSource,
    /// Last profile explicitly selected through `net.hadess.PowerProfiles`.
    /// Kept separately so releasing a hold falls back to it instead of
    /// deriving the profile from the (already modified) thermal mode.
    pub ppd_base: String,
    pub holds: Vec<Hold>,
    /// True when TLP was still active when the daemon started. Automatic
    /// switching is suspended so the two managers never fight; the takeover
    /// wizard clears it (or a restart after TLP is disabled).
    pub standby: bool,
    next_cookie: u32,
}

impl State {
    pub fn new(fs: Sysfs, profiles: ProfileFile) -> Self {
        let power_source = PowerSource::detect(&fs);
        let active_profile = match power_source {
            PowerSource::Battery => profiles.power.battery.clone(),
            _ => profiles.power.ac.clone(),
        };
        let thermal = platform::detect(&fs)
            .current
            .unwrap_or_else(|| "balanced".to_string());
        let standby = loq_core::services::is_active("tlp.service") == Some(true);
        Self {
            fs,
            profiles,
            active_profile,
            power_source,
            ppd_base: thermal_to_ppd(&thermal).to_string(),
            holds: Vec::new(),
            standby,
            next_cookie: 1,
        }
    }

    pub fn thermal_mode(&self) -> String {
        platform::detect(&self.fs)
            .current
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn ppd_active(&self) -> String {
        let mut best = self.ppd_base.clone();
        for hold in &self.holds {
            if ppd_rank(&hold.profile) > ppd_rank(&best) {
                best = hold.profile.clone();
            }
        }
        best
    }

    /// Align the reported PPD profile with a thermal mode that was just set
    /// through the LOQ Power interface.
    pub fn sync_ppd_base(&mut self) {
        self.ppd_base = thermal_to_ppd(&self.thermal_mode()).to_string();
    }

    pub fn resolve(&self, name: &str) -> loq_core::Result<Resolved> {
        let profile = self.profiles.get(name)?;
        Ok(profile::resolve(&self.fs, profile))
    }

    pub fn apply_profile(&self, name: &str, dry_run: bool) -> (ApplyResponse, Vec<String>) {
        match self.resolve(name) {
            Ok(resolved) => {
                let mut request = resolved.request;
                request.dry_run = dry_run;
                let mut response = apply::apply(&self.fs, &request);

                // TLP-absorbed power saving runs after the EC/platform writes,
                // so a failure here never leaves the machine in CUSTOM mode.
                if let Some(settings) = self
                    .profiles
                    .get(name)
                    .ok()
                    .and_then(|profile| profile.power_saving.as_ref())
                {
                    let report = powersave::apply(&self.fs, settings, dry_run);
                    response.writes.extend(report.writes);
                    response.skipped.extend(report.skipped);
                    response.errors.extend(report.errors);
                    response.ok = response.errors.is_empty();
                }

                (response, resolved.warnings)
            }
            Err(error) => (ApplyResponse::error(error.to_string()), Vec::new()),
        }
    }

    pub fn apply_request(&self, request: ApplyRequest) -> ApplyResponse {
        apply::apply(&self.fs, &request)
    }

    pub fn hold_profile(&mut self, profile: &str, reason: &str, application_id: &str) -> u32 {
        let cookie = self.next_cookie;
        self.next_cookie = self.next_cookie.wrapping_add(1).max(1);
        self.holds.push(Hold {
            cookie,
            profile: profile.to_string(),
            reason: reason.to_string(),
            application_id: application_id.to_string(),
        });
        cookie
    }

    pub fn release_profile(&mut self, cookie: u32) -> bool {
        let before = self.holds.len();
        self.holds.retain(|hold| hold.cookie != cookie);
        self.holds.len() != before
    }

    pub fn snapshot_json(&self) -> String {
        let battery = battery::primary(&self.fs);
        serde_json::json!({
            "active_profile": self.active_profile,
            "power_source": self.power_source.as_str(),
            "thermal_mode": self.thermal_mode(),
            "ppd_profile": self.ppd_active(),
            "battery": battery,
            "holds": self.holds.len(),
            "standby": self.standby,
        })
        .to_string()
    }
}

/// Convenience alias used by the interfaces.
pub type SharedState = std::sync::Arc<RwLock<State>>;
