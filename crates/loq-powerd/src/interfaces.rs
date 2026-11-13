use std::collections::HashMap;

use loq_core::profile::ProfileFile;
use loq_core::request::{ApplyRequest, BatteryRequest, IdeapadRequest};
use zbus::message::Header;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, fdo};

use crate::polkit;
use crate::state::{PPD_PROFILES, SharedState, ppd_to_thermal, thermal_to_ppd};

const LOQ_PATH: &str = "/org/loq/Power1";
const LOQ_IFACE: &str = "org.loq.Power1";
const PPD_PATH: &str = "/net/hadess/PowerProfiles";
const PPD_IFACE: &str = "net.hadess.PowerProfiles";

async fn authorize(
    conn: &Connection,
    header: Option<&Header<'_>>,
    action: &str,
) -> fdo::Result<()> {
    let sender = header.and_then(|header| header.sender().map(|name| name.to_string()));

    match polkit::check(conn, sender.as_deref(), action).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(fdo::Error::AccessDenied(
            "polkit denied this request".to_string(),
        )),
        Err(error) => Err(fdo::Error::Failed(format!("cannot reach polkit: {error}"))),
    }
}

async fn emit_loq_state(conn: &Connection, snapshot: String) -> zbus::Result<()> {
    zbus::object_server::SignalEmitter::new(conn, LOQ_PATH)?
        .state_changed(snapshot)
        .await
}

async fn emit_loq_properties(
    conn: &Connection,
    active_profile: String,
    power_source: String,
    thermal_mode: String,
) -> zbus::Result<()> {
    let changed: HashMap<&str, Value<'static>> = HashMap::from([
        ("ActiveProfile", Value::from(active_profile)),
        ("PowerSource", Value::from(power_source)),
        ("ThermalMode", Value::from(thermal_mode)),
    ]);

    conn.emit_signal(
        None::<&str>,
        LOQ_PATH,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
        &(LOQ_IFACE, changed, Vec::<&str>::new()),
    )
    .await
}

async fn emit_ppd_properties(conn: &Connection, active: String) -> zbus::Result<()> {
    let changed: HashMap<&str, Value<'static>> =
        HashMap::from([("ActiveProfile", Value::from(active))]);

    conn.emit_signal(
        None::<&str>,
        PPD_PATH,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
        &(PPD_IFACE, changed, Vec::<&str>::new()),
    )
    .await
}

pub(crate) async fn publish(conn: &Connection, state: &SharedState) -> zbus::Result<()> {
    let (snapshot, active, source, thermal) = {
        let state = state.read().expect("state lock poisoned");
        (
            state.snapshot_json(),
            state.active_profile.clone(),
            state.power_source.as_str().to_string(),
            state.thermal_mode(),
        )
    };

    emit_loq_state(conn, snapshot).await?;
    emit_loq_properties(conn, active, source, thermal.clone()).await?;
    emit_ppd_properties(conn, thermal_to_ppd(&thermal).to_string()).await
}

/// Apply the thermal mode implied by the effective PPD profile when holds or
/// an explicit PPD selection changed it.
fn apply_ppd_effective(state: &SharedState) -> Option<loq_core::ApplyResponse> {
    let (profile, current) = {
        let state = state.read().expect("state lock poisoned");
        (state.ppd_active(), state.thermal_mode())
    };
    let target = ppd_to_thermal(&profile).unwrap_or("balanced");
    if target == current {
        return None;
    }

    let request = ApplyRequest {
        platform: Some(target.to_string()),
        ..Default::default()
    };
    Some(
        state
            .read()
            .expect("state lock poisoned")
            .apply_request(request),
    )
}

fn sync_ppd(state: &SharedState) {
    state.write().expect("state lock poisoned").sync_ppd_base();
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| {
        format!("{{\"ok\":false,\"errors\":[\"cannot serialize: {error}\"]}}")
    })
}

pub struct PowerIface {
    pub state: SharedState,
}

#[zbus::interface(name = "org.loq.Power1")]
impl PowerIface {
    /// Current profile name from the profile file.
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.state
            .read()
            .expect("state lock poisoned")
            .active_profile
            .clone()
    }

    /// `ac`, `battery` or `unknown`.
    #[zbus(property)]
    fn power_source(&self) -> String {
        self.state
            .read()
            .expect("state lock poisoned")
            .power_source
            .as_str()
            .to_string()
    }

    /// Raw platform profile reported by the EC/gamezone driver.
    #[zbus(property)]
    fn thermal_mode(&self) -> String {
        self.state
            .read()
            .expect("state lock poisoned")
            .thermal_mode()
    }

    #[zbus(property)]
    fn daemon_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Full state snapshot as JSON.
    fn get_state(&self) -> String {
        self.state
            .read()
            .expect("state lock poisoned")
            .snapshot_json()
    }

    /// Complete read-only capability manifest as JSON.
    fn probe(&self) -> String {
        to_json(&loq_core::probe())
    }

    /// Apply a named profile. Enters CUSTOM mode when EC limits change.
    async fn apply_profile(
        &self,
        name: String,
        dry_run: bool,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        if !dry_run {
            authorize(conn, Some(&header), "org.loq.power.apply").await?;
        }

        let (response, warnings) = {
            let state = self.state.read().expect("state lock poisoned");
            state.apply_profile(&name, dry_run)
        };

        if response.ok && !dry_run {
            {
                let mut state = self.state.write().expect("state lock poisoned");
                state.active_profile = name;
            }
            sync_ppd(&self.state);
            let _ = publish(conn, &self.state).await;
        }

        let mut json: serde_json::Value = serde_json::from_str(&to_json(&response))
            .unwrap_or_else(|_| serde_json::json!({ "ok": response.ok }));
        if !warnings.is_empty() {
            json["warnings"] = serde_json::json!(warnings);
        }
        Ok(json.to_string())
    }

    /// Write one or more EC attributes.
    async fn set_ec(
        &self,
        values: HashMap<String, i64>,
        dry_run: bool,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        if !dry_run {
            authorize(conn, Some(&header), "org.loq.power.apply").await?;
        }

        let request = ApplyRequest {
            ec: values.into_iter().collect(),
            dry_run,
            ..Default::default()
        };
        let response = self
            .state
            .read()
            .expect("state lock poisoned")
            .apply_request(request);

        if response.ok && !dry_run {
            sync_ppd(&self.state);
            let _ = publish(conn, &self.state).await;
        }
        Ok(to_json(&response))
    }

    /// Switch the thermal mode (same five values as gamezone).
    async fn set_platform(
        &self,
        mode: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        authorize(conn, Some(&header), "org.loq.power.apply").await?;

        let request = ApplyRequest {
            platform: Some(mode),
            ..Default::default()
        };
        let response = self
            .state
            .read()
            .expect("state lock poisoned")
            .apply_request(request);

        if response.ok {
            sync_ppd(&self.state);
            let _ = publish(conn, &self.state).await;
        }
        Ok(to_json(&response))
    }

    /// Change the battery charge type.
    async fn set_charge_type(
        &self,
        charge_type: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        authorize(conn, Some(&header), "org.loq.power.apply").await?;

        let request = ApplyRequest {
            battery: Some(BatteryRequest {
                name: None,
                charge_type,
            }),
            ..Default::default()
        };
        let response = self
            .state
            .read()
            .expect("state lock poisoned")
            .apply_request(request);

        if response.ok {
            let _ = publish(conn, &self.state).await;
        }
        Ok(to_json(&response))
    }

    /// Change the EC fan mode (0, 1, 2 or 4).
    async fn set_fan_mode(
        &self,
        mode: u32,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        authorize(conn, Some(&header), "org.loq.power.apply").await?;

        let request = ApplyRequest {
            ideapad: Some(IdeapadRequest {
                fan_mode: Some(i64::from(mode)),
                fn_lock: None,
            }),
            ..Default::default()
        };
        let response = self
            .state
            .read()
            .expect("state lock poisoned")
            .apply_request(request);

        if response.ok {
            let _ = publish(conn, &self.state).await;
        }
        Ok(to_json(&response))
    }

    /// Apply a full JSON `ApplyRequest` (used by the CLI and the GUI).
    async fn apply(
        &self,
        request_json: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        let request: ApplyRequest = serde_json::from_str(&request_json)
            .map_err(|error| fdo::Error::InvalidArgs(error.to_string()))?;

        if !request.dry_run {
            authorize(conn, Some(&header), "org.loq.power.apply").await?;
        }

        let dry_run = request.dry_run;
        let response = self
            .state
            .read()
            .expect("state lock poisoned")
            .apply_request(request);

        if response.ok && !dry_run {
            sync_ppd(&self.state);
            let _ = publish(conn, &self.state).await;
        }
        Ok(to_json(&response))
    }

    /// Re-read the daemon configuration file.
    async fn reload(
        &self,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<String> {
        authorize(conn, Some(&header), "org.loq.power.apply").await?;

        let path = crate::config_path();
        let profiles = ProfileFile::load_or_builtin(&path);
        {
            let mut state = self.state.write().expect("state lock poisoned");
            state.profiles = profiles;
        }
        let _ = publish(conn, &self.state).await;
        Ok(format!("{{\"ok\":true,\"path\":\"{}\"}}", path.display()))
    }

    #[zbus(signal)]
    async fn state_changed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        snapshot: String,
    ) -> zbus::Result<()>;
}

pub struct PpdIface {
    pub state: SharedState,
}

/// `net.hadess.PowerProfiles` compatibility layer, so GNOME/KDE power widgets
/// keep working after `tlp-pd` is disabled.
#[zbus::interface(name = "net.hadess.PowerProfiles")]
impl PpdIface {
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.state.read().expect("state lock poisoned").ppd_active()
    }

    #[zbus(property)]
    fn performance_degraded(&self) -> String {
        "none".to_string()
    }

    #[zbus(property)]
    fn profiles(&self) -> Vec<HashMap<String, OwnedValue>> {
        PPD_PROFILES
            .iter()
            .map(|profile| {
                HashMap::from([
                    (
                        "profile".to_string(),
                        OwnedValue::try_from(Value::from(profile.to_string()))
                            .expect("string is a valid value"),
                    ),
                    (
                        "driver".to_string(),
                        OwnedValue::try_from(Value::from("loq-power"))
                            .expect("string is a valid value"),
                    ),
                    (
                        "platform_driver".to_string(),
                        OwnedValue::try_from(Value::from("lenovo-wmi-gamezone"))
                            .expect("string is a valid value"),
                    ),
                ])
            })
            .collect()
    }

    #[zbus(property)]
    fn actions(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn active_profile_holds(&self) -> Vec<HashMap<String, OwnedValue>> {
        self.state
            .read()
            .expect("state lock poisoned")
            .holds
            .iter()
            .map(|hold| {
                HashMap::from([
                    (
                        "profile".to_string(),
                        OwnedValue::try_from(Value::from(hold.profile.clone()))
                            .expect("string is a valid value"),
                    ),
                    (
                        "reason".to_string(),
                        OwnedValue::try_from(Value::from(hold.reason.clone()))
                            .expect("string is a valid value"),
                    ),
                    (
                        "application_id".to_string(),
                        OwnedValue::try_from(Value::from(hold.application_id.clone()))
                            .expect("string is a valid value"),
                    ),
                ])
            })
            .collect()
    }

    /// Setting the property behaves like a one-shot profile request.
    #[zbus(property)]
    async fn set_active_profile(
        &self,
        profile: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Option<Header<'_>>,
    ) -> fdo::Result<()> {
        let Some(thermal) = ppd_to_thermal(&profile) else {
            return Err(fdo::Error::InvalidArgs(format!(
                "unknown profile {profile}"
            )));
        };

        authorize(conn, header.as_ref(), "org.loq.power.apply").await?;

        let request = ApplyRequest {
            platform: Some(thermal.to_string()),
            ..Default::default()
        };
        let response = self
            .state
            .read()
            .expect("state lock poisoned")
            .apply_request(request);

        if !response.ok {
            return Err(fdo::Error::Failed(response.errors.join("; ")));
        }

        self.state.write().expect("state lock poisoned").ppd_base = profile;

        let _ = publish(conn, &self.state).await;
        Ok(())
    }

    /// Hold `profile` until the returned cookie is released.
    async fn hold_profile(
        &self,
        profile: String,
        reason: String,
        application_id: String,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<u32> {
        if ppd_to_thermal(&profile).is_none() {
            return Err(fdo::Error::InvalidArgs(format!(
                "unknown profile {profile}"
            )));
        }
        authorize(conn, Some(&header), "org.loq.power.apply").await?;

        let cookie = self
            .state
            .write()
            .expect("state lock poisoned")
            .hold_profile(&profile, &reason, &application_id);

        if let Some(response) = apply_ppd_effective(&self.state)
            && !response.ok
        {
            tracing::warn!(errors = ?response.errors, "cannot apply held profile");
        }

        let _ = publish(conn, &self.state).await;
        Ok(cookie)
    }

    /// Release a previously taken hold.
    async fn release_profile(
        &self,
        cookie: u32,
        #[zbus(connection)] conn: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        authorize(conn, Some(&header), "org.loq.power.apply").await?;

        if !self
            .state
            .write()
            .expect("state lock poisoned")
            .release_profile(cookie)
        {
            return Err(fdo::Error::InvalidArgs(format!(
                "unknown hold cookie {cookie}"
            )));
        }

        if let Some(response) = apply_ppd_effective(&self.state)
            && !response.ok
        {
            tracing::warn!(errors = ?response.errors, "cannot restore profile after hold");
        }

        let _ = publish(conn, &self.state).await;
        Ok(())
    }
}
