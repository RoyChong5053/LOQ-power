use serde::Serialize;

use crate::sysfs::Sysfs;

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
