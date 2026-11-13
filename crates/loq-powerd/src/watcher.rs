use std::time::Duration;

use futures_util::StreamExt;

use crate::interfaces::publish;
use crate::state::{PowerSource, SharedState};

/// Apply the profile mapped to `source` and report the result.
fn switch_to(state: &SharedState, source: PowerSource) {
    let (profile_name, auto_switch, standby) = {
        let state = state.read().expect("state lock poisoned");
        let power = &state.profiles.power;
        (
            match source {
                PowerSource::Ac => power.ac.clone(),
                PowerSource::Battery => power.battery.clone(),
                PowerSource::Unknown => String::new(),
            },
            power.auto_switch,
            state.standby,
        )
    };

    if standby {
        tracing::debug!("standby: TLP still manages power; skipping automatic switch");
        return;
    }
    if !auto_switch || profile_name.is_empty() {
        return;
    }

    let (response, warnings) = {
        let state = state.read().expect("state lock poisoned");
        state.apply_profile(&profile_name, false)
    };
    for warning in warnings {
        tracing::warn!("{warning}");
    }

    if response.ok {
        state.write().expect("state lock poisoned").active_profile = profile_name.clone();
        tracing::info!(profile = %profile_name, "automatic profile applied");
    } else {
        tracing::error!(
            profile = %profile_name,
            errors = ?response.errors,
            "automatic profile switch failed"
        );
    }
}

/// Re-assert the active profile after the machine wakes up. Suspend/resume can
/// reset CPU and EC knobs; the daemon is the only writer, so it must recover
/// the state it owns.
pub async fn watch_resume(state: SharedState, conn: zbus::Connection) {
    let proxy = match zbus::Proxy::new(
        &conn,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(error) => {
            tracing::warn!(%error, "cannot watch logind; resume re-apply disabled");
            return;
        }
    };

    let mut signals = match proxy.receive_signal("PrepareForSleep").await {
        Ok(signals) => signals,
        Err(error) => {
            tracing::warn!(%error, "cannot subscribe to PrepareForSleep");
            return;
        }
    };

    while let Some(message) = signals.next().await {
        let going_to_sleep: bool = match message.body().deserialize() {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, "cannot parse PrepareForSleep");
                continue;
            }
        };
        if going_to_sleep {
            continue;
        }

        tracing::info!("resumed from suspend, re-applying profile");
        // Give drivers a moment to finish resuming before asserting EC state.
        tokio::time::sleep(Duration::from_secs(2)).await;

        let (profile, source, standby) = {
            let state = state.read().expect("state lock poisoned");
            (
                state.active_profile.clone(),
                state.power_source,
                state.standby,
            )
        };
        if standby {
            tracing::info!("standby: skipping resume re-apply while TLP is active");
            continue;
        }
        if !profile.is_empty() {
            let (response, warnings) = {
                let state = state.read().expect("state lock poisoned");
                state.apply_profile(&profile, false)
            };
            for warning in warnings {
                tracing::warn!("{warning}");
            }
            if response.ok {
                tracing::info!(profile = %profile, "profile re-applied after resume");
            } else {
                tracing::error!(profile = %profile, errors = ?response.errors, "resume re-apply failed");
            }
        } else {
            switch_to(&state, source);
        }

        if let Err(error) = publish(&conn, &state).await {
            tracing::warn!("cannot publish state after resume: {error}");
        }
    }
}

/// Poll the AC adapter state and apply the mapped profile when it changes.
///
/// Polling keeps the daemon dependency-free; `power_supply` changes are rare
/// and a 3 second latency is irrelevant for AC/battery switching.
pub async fn watch(state: SharedState, conn: zbus::Connection) {
    let mut last = {
        let state = state.read().expect("state lock poisoned");
        let source = PowerSource::detect(&state.fs);
        let auto = state.profiles.power.auto_switch;
        (source, auto)
    };

    // Assert the profile for the current power source on startup. Without this
    // a reboot into battery would keep the last profile used on AC.
    if last.1 {
        switch_to(&state, last.0);
        if let Err(error) = publish(&conn, &state).await {
            tracing::warn!("cannot publish startup state: {error}");
        }
    }
    if state.read().expect("state lock poisoned").standby {
        tracing::warn!(
            "TLP is still active; automatic profile switching is suspended until the takeover wizard runs"
        );
    }

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;

        let current = PowerSource::detect(&state.read().expect("state lock poisoned").fs);
        if current == last.0 {
            continue;
        }
        last.0 = current;

        {
            let mut state = state.write().expect("state lock poisoned");
            state.power_source = current;
        }

        tracing::info!(source = current.as_str(), "power source changed");
        switch_to(&state, current);

        if let Err(error) = publish(&conn, &state).await {
            tracing::warn!("cannot publish state change: {error}");
        }
    }
}
