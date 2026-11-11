use std::collections::BTreeMap;

use serde::Serialize;

use crate::services;
use crate::sysfs::Sysfs;

#[derive(Debug, Clone, Default, Serialize)]
pub struct TlpInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub service_active: Option<bool>,
    pub service_enabled: Option<bool>,
    pub pd_active: Option<bool>,
    pub pd_enabled: Option<bool>,
    pub config_files: Vec<String>,
    /// Effective `KEY=VALUE` map after reading `/etc/tlp.conf` and
    /// `/etc/tlp.d/*.conf` in order (later files win).
    pub effective: BTreeMap<String, String>,
}

fn parse_config_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let value = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();
    Some((key.to_string(), value))
}

fn read_config_file(fs: &Sysfs, path: &str, effective: &mut BTreeMap<String, String>) {
    let Some(content) = fs.read(path) else {
        return;
    };
    for line in content.lines() {
        if let Some((key, value)) = parse_config_line(line) {
            effective.insert(key, value);
        }
    }
}

pub fn detect(fs: &Sysfs) -> TlpInfo {
    let version = services::output("tlp", &["--version"])
        .or_else(|| services::output("tlp-stat", &["--version"]))
        .and_then(|value| value.lines().next().map(str::to_string));

    let installed = version.is_some() || services::command_exists("tlp-stat");

    let mut config_files = Vec::new();
    let mut effective = BTreeMap::new();

    if fs.exists("/etc/tlp.conf") {
        config_files.push("/etc/tlp.conf".to_string());
        read_config_file(fs, "/etc/tlp.conf", &mut effective);
    }

    for path in fs.glob("/etc/tlp.d/*.conf") {
        let display = fs.display_path(&path);
        config_files.push(display.clone());
        read_config_file(fs, &display, &mut effective);
    }

    TlpInfo {
        installed,
        version,
        service_active: services::is_active("tlp.service"),
        service_enabled: services::is_enabled("tlp.service"),
        pd_active: services::is_active("tlp-pd.service"),
        pd_enabled: services::is_enabled("tlp-pd.service"),
        config_files,
        effective,
    }
}
