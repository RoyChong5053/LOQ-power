use std::time::Duration;

use crate::interfaces::publish;
use crate::state::{PowerSource, SharedState};

/// Poll the AC adapter state and apply the mapped profile when it changes.
///
/// Polling keeps the daemon dependency-free; `power_supply` changes are rare
/// and a 3 second latency is irrelevant for AC/battery switching.
pub async fn watch(state: SharedState, conn: zbus::Connection) {
    let mut last = PowerSource::detect(&state.read().expect("state lock poisoned").fs);

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;

        let current = PowerSource::detect(&state.read().expect("state lock poisoned").fs);
        if current == last {
            continue;
        }
        last = current;

        let (profile_name, auto_switch) = {
            let mut state = state.write().expect("state lock poisoned");
            state.power_source = current;
            let power = &state.profiles.power;
            (
                match current {
                    PowerSource::Ac => power.ac.clone(),
                    PowerSource::Battery => power.battery.clone(),
                    PowerSource::Unknown => String::new(),
                },
                power.auto_switch,
            )
        };

        tracing::info!(
            source = current.as_str(),
            profile = %profile_name,
            auto_switch,
            "power source changed"
        );

        if auto_switch && !profile_name.is_empty() {
            let (response, warnings) = {
                let state = state.read().expect("state lock poisoned");
                state.apply_profile(&profile_name, false)
            };

            for warning in warnings {
                tracing::warn!("{warning}");
            }
            if response.ok {
                state.write().expect("state lock poisoned").active_profile = profile_name.clone();
            } else {
                tracing::error!(
                    profile = %profile_name,
                    errors = ?response.errors,
                    "automatic profile switch failed"
                );
            }
        }

        if let Err(error) = publish(&conn, &state).await {
            tracing::warn!("cannot publish state change: {error}");
        }
    }
}
