pub mod battery;
pub mod cpu;
pub mod ec;
pub mod error;
pub mod gpu;
pub mod hwmon;
pub mod ideapad;
pub mod leds;
pub mod legion;
pub mod platform;
pub mod probe;
pub mod services;
pub mod sysfs;
pub mod tlp;

pub use error::{Error, Result};
pub use probe::{Capabilities, probe, probe_with};
pub use sysfs::Sysfs;
