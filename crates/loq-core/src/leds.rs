use serde::Serialize;

use crate::error::{Error, Result};
use crate::sysfs::Sysfs;
use crate::write::{self, WriteReport};

#[derive(Debug, Clone, Serialize)]
pub struct LedInfo {
    pub name: String,
    pub brightness: Option<i64>,
    pub max_brightness: Option<i64>,
    pub path: String,
}

/// Discover platform / keyboard LEDs.
pub fn detect(fs: &Sysfs) -> Vec<LedInfo> {
    let mut leds = Vec::new();

    for name in fs.list_dir("/sys/class/leds") {
        let interesting = name.starts_with("platform::") || name.contains("kbd");
        if !interesting {
            continue;
        }

        let base = format!("/sys/class/leds/{name}");
        leds.push(LedInfo {
            name,
            brightness: fs.read_i64(&format!("{base}/brightness")),
            max_brightness: fs.read_i64(&format!("{base}/max_brightness")),
            path: base,
        });
    }

    leds
}

/// Set the brightness of a platform LED.
pub fn set_brightness(
    fs: &Sysfs,
    name: &str,
    brightness: i64,
    dry_run: bool,
) -> Result<WriteReport> {
    let path = format!("/sys/class/leds/{name}/brightness");
    if !fs.exists(&path) {
        return Err(Error::NotFound(format!("LED {name}")));
    }

    let max = fs
        .read_i64(&format!("/sys/class/leds/{name}/max_brightness"))
        .unwrap_or(i64::MAX);
    write::check_range(name, brightness, Some(0), Some(max), None)?;

    write::write_verified(fs, &path, &brightness.to_string(), dry_run, |actual, _| {
        actual.trim().parse::<i64>().ok() == Some(brightness)
    })
}
