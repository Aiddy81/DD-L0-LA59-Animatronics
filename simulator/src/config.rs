//! `sim.toml`: how servo pulses map onto the 3D model's joints.

use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;

#[derive(Resource, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SimConfig {
    pub servo: ServoCfg,
    pub wing_right: WingCfg,
    pub wing_left: WingCfg,
    pub eyes: EyesCfg,
}

/// The physical servo: which pulse widths it maps onto its travel.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ServoCfg {
    pub min_pulse_us: f32,
    pub max_pulse_us: f32,
    pub range_deg: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct WingCfg {
    pub channel: usize,
    /// Servo angle at which the wing sits level.
    pub neutral_deg: f32,
    /// +1 if a larger servo angle raises the wing, -1 if it lowers it.
    pub direction: f32,
    /// Wing degrees per servo degree (the linkage ratio).
    pub gain: f32,
    /// Hinge axis in the wing node's local space.
    pub hinge_axis: [f32; 3],
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct EyesCfg {
    pub main: [usize; 3],
    pub small: [usize; 3],
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            servo: ServoCfg::default(),
            wing_right: WingCfg {
                channel: 0,
                neutral_deg: 60.0,
                direction: -1.0,
                gain: 1.5,
                hinge_axis: [0.0, 0.0, 1.0],
            },
            wing_left: WingCfg {
                channel: 1,
                neutral_deg: 40.0,
                direction: 1.0,
                gain: 1.5,
                hinge_axis: [0.0, 0.0, -1.0],
            },
            eyes: EyesCfg::default(),
        }
    }
}

impl Default for ServoCfg {
    fn default() -> Self {
        Self {
            min_pulse_us: 544.0,
            max_pulse_us: 2400.0,
            range_deg: 180.0,
        }
    }
}

impl Default for WingCfg {
    fn default() -> Self {
        SimConfig::default().wing_right
    }
}

impl Default for EyesCfg {
    fn default() -> Self {
        Self {
            main: [2, 3, 4],
            small: [5, 6, 7],
        }
    }
}

impl ServoCfg {
    /// The angle a physical servo would move to for this pulse width.
    pub fn angle_for_pulse(&self, pulse_us: f32) -> f32 {
        let span = (self.max_pulse_us - self.min_pulse_us).max(1.0);
        ((pulse_us - self.min_pulse_us) / span * self.range_deg).clamp(0.0, self.range_deg)
    }
}

impl WingCfg {
    pub fn elevation_deg(&self, servo_deg: f32) -> f32 {
        (servo_deg - self.neutral_deg) * self.direction * self.gain
    }
}

impl SimConfig {
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(cfg) => (cfg, None),
                Err(e) => (
                    Self::default(),
                    Some(format!(
                        "[sim] {} is invalid, using defaults: {e}",
                        path.display()
                    )),
                ),
            },
            Err(_) => (Self::default(), None),
        }
    }
}
