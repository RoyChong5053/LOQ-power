//! Request/response types shared by the CLI, the GUI and the privileged
//! helper.
//!
//! The helper only accepts this fixed schema. Every field is validated
//! against the live sysfs tree before anything is written.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::write::WriteReport;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_khz: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_khz: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boost: Option<bool>,
}

impl CpuRequest {
    pub fn is_empty(&self) -> bool {
        self.governor.is_none()
            && self.epp.is_none()
            && self.min_khz.is_none()
            && self.max_khz.is_none()
            && self.boost.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatteryRequest {
    /// Battery name, defaults to the first battery found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub charge_type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdeapadRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_mode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fn_lock: Option<bool>,
}

impl IdeapadRequest {
    pub fn is_empty(&self) -> bool {
        self.fan_mode.is_none() && self.fn_lock.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegionRequest {
    /// Firmware fan full-speed switch (requires LenovoLegionLinux).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fullspeed: Option<bool>,
    /// LLL power mode: 0 balanced, 1 performance, 2 quiet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub powermode: Option<i64>,
    /// Complete fan curve; only valid when `legion_hwmon` is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_curve: Option<crate::legion::FanCurve>,
}

impl LegionRequest {
    pub fn is_empty(&self) -> bool {
        self.fullspeed.is_none() && self.powermode.is_none() && self.fan_curve.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedRequest {
    pub name: String,
    pub brightness: i64,
}

/// One atomic unit of work.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplyRequest {
    /// EC attribute name to value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ec: BTreeMap<String, i64>,
    /// Final thermal mode. `custom` keeps EC write access enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<CpuRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<BatteryRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ideapad: Option<IdeapadRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led: Option<LedRequest>,
    /// Optional LenovoLegionLinux controls; ignored (with an error) when the
    /// out-of-tree module is not loaded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legion: Option<LegionRequest>,
    #[serde(default)]
    pub dry_run: bool,
}

impl ApplyRequest {
    pub fn is_empty(&self) -> bool {
        self.ec.is_empty()
            && self.platform.is_none()
            && self.cpu.as_ref().is_none_or(CpuRequest::is_empty)
            && self.battery.is_none()
            && self.ideapad.as_ref().is_none_or(IdeapadRequest::is_empty)
            && self.led.is_none()
            && self.legion.as_ref().is_none_or(LegionRequest::is_empty)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplyResponse {
    pub ok: bool,
    pub dry_run: bool,
    pub writes: Vec<WriteReport>,
    #[serde(default)]
    pub skipped: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
}

impl ApplyResponse {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            dry_run: false,
            writes: Vec::new(),
            skipped: Vec::new(),
            errors: vec![message.into()],
        }
    }
}
