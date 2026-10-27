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
    pub board: BoardCfg,
    pub model: ModelCfg,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ModelCfg {
    /// "gltf" (assets/droid.glb, the default) or "primitive". Switchable in the app.
    pub kind: String,
}

impl Default for ModelCfg {
    fn default() -> Self {
        Self {
            kind: "gltf".into(),
        }
    }
}

/// Costs on the real PropMaker Feather, from `simulator/board_bench.py` run on
/// the robot, or estimates until then.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct BoardCfg {
    /// True once the numbers come from board_bench.py rather than estimates.
    pub measured: bool,
    /// One servo/LED write through ServoKit to the PCA9685 over I2C.
    pub pca_write_ms: f64,
    /// One `pwmio.PWMOut.duty_cycle` write.
    pub pwm_write_ms: f64,
    /// One LIS3DH `acceleration` read over I2C.
    pub accel_read_ms: f64,
    /// asyncio overhead per scheduler pass.
    pub loop_ms: f64,
    /// Heap left for code.py once its libraries are imported.
    pub heap_budget_kb: f64,
    /// CircuitPython bytes per desktop-Python byte allocated (objects are smaller there).
    pub heap_scale: f64,
}

impl Default for BoardCfg {
    fn default() -> Self {
        Self {
            measured: false,
            pca_write_ms: 2.0,
            pwm_write_ms: 0.05,
            accel_read_ms: 1.5,
            loop_ms: 0.25,
            heap_budget_kb: 130.0,
            heap_scale: 0.45,
        }
    }
}

/// The physical servo: which pulse widths it maps onto its travel, and how
/// fast it can get there.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ServoCfg {
    pub min_pulse_us: f32,
    pub max_pulse_us: f32,
    pub range_deg: f32,
    /// Rated speed: seconds to turn 60 degrees. 0 or less means instant.
    pub sec_per_60deg: f32,
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
    /// Hinge axis in the glTF wing node's local space (Y up, Z forward),
    /// oriented so a positive angle lifts the panel. The primitive model
    /// computes its own matching hinge.
    pub hinge_axis: [f32; 3],
    /// Travel limits of the panel; 0 = closed flush on the head.
    pub min_deg: f32,
    pub max_deg: f32,
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
                neutral_deg: 90.0,
                direction: -1.0,
                gain: 0.45,
                hinge_axis: [0.924, -0.296, 0.244],
                min_deg: 0.0,
                max_deg: 75.0,
            },
            wing_left: WingCfg {
                channel: 1,
                neutral_deg: 10.0,
                direction: 1.0,
                gain: 0.45,
                hinge_axis: [0.924, 0.296, -0.244],
                min_deg: 0.0,
                max_deg: 75.0,
            },
            eyes: EyesCfg::default(),
            board: BoardCfg::default(),
            model: ModelCfg::default(),
        }
    }
}

impl Default for ServoCfg {
    fn default() -> Self {
        Self {
            min_pulse_us: 544.0,
            max_pulse_us: 2400.0,
            range_deg: 180.0,
            sec_per_60deg: 0.12,
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

    /// Fastest the servo can turn, in degrees per second; `None` = instant.
    pub fn max_speed_dps(&self) -> Option<f32> {
        (self.sec_per_60deg > 0.0).then(|| 60.0 / self.sec_per_60deg)
    }
}

impl WingCfg {
    pub fn elevation_deg(&self, servo_deg: f32) -> f32 {
        ((servo_deg - self.neutral_deg) * self.direction * self.gain)
            .clamp(self.min_deg, self.max_deg)
    }
}

impl SimConfig {
    /// Start-up load: a missing file means defaults, an invalid one defaults
    /// plus a warning.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match Self::read(path) {
            Ok(cfg) => (cfg.unwrap_or_default(), None),
            Err(e) => (
                Self::default(),
                Some(format!(
                    "[sim] {} is invalid, using defaults: {e}",
                    path.display()
                )),
            ),
        }
    }

    /// `Ok(None)` when the file doesn't exist; `Err` when it can't be read or parsed.
    pub fn read(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map(Some).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}
