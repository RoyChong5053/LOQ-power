mod interfaces;
mod polkit;
mod state;
mod watcher;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result, bail};
use loq_core::Sysfs;
use loq_core::profile::ProfileFile;
use tracing_subscriber::EnvFilter;

use crate::state::State;

pub fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("LOQ_POWER_CONFIG") {
        return PathBuf::from(path);
    }
    loq_core::profile::system_path()
}

/// Keep trying to own `net.hadess.PowerProfiles`.
///
/// The name is usually held by `tlp-pd` until the takeover wizard stops it.
/// Retrying means a daemon that was already running during the takeover picks
/// the name up without needing a restart.
async fn claim_power_profiles(connection: zbus::Connection) {
    loop {
        match connection.request_name("net.hadess.PowerProfiles").await {
            Ok(()) => {
                tracing::info!("owning net.hadess.PowerProfiles");
                return;
            }
            Err(error) => {
                tracing::debug!(%error, "net.hadess.PowerProfiles is owned elsewhere; retrying");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if unsafe { libc::geteuid() } != 0 {
        bail!("loq-powerd must run as root (start it through systemd)");
    }

    let path = config_path();
    let profiles = if path.exists() {
        ProfileFile::load(&path).with_context(|| format!("cannot load {}", path.display()))?
    } else {
        tracing::warn!(
            path = %path.display(),
            "configuration file not found, using built-in profiles"
        );
        ProfileFile::builtin()
    };

    let state = Arc::new(RwLock::new(State::new(Sysfs::new(), profiles)));

    let connection = zbus::connection::Builder::system()?
        .name("org.loq.Power1")?
        .serve_at(
            "/org/loq/Power1",
            interfaces::PowerIface {
                state: state.clone(),
            },
        )?
        .serve_at(
            "/net/hadess/PowerProfiles",
            interfaces::PpdIface {
                state: state.clone(),
            },
        )?
        .build()
        .await
        .context("cannot connect to the system bus")?;

    tokio::spawn(claim_power_profiles(connection.clone()));

    tokio::spawn(watcher::watch(state.clone(), connection.clone()));
    tokio::spawn(watcher::watch_resume(state.clone(), connection.clone()));

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %path.display(),
        "loq-powerd ready"
    );

    std::future::pending::<()>().await;
    Ok(())
}
