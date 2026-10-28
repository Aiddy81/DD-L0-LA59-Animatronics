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
    pub editor: EditorCfg,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct EditorCfg {
    /// Run when a traceback line in the log is clicked; `{file}` and `{line}`
    /// are filled in. Empty = reveal the file in the file manager.
    pub command: String,
}

impl Default for EditorCfg {
    fn default() -> Self {
        Self {
            command: "code -g {file}:{line}".into(),
        }
    }
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
    /// Warn when one task step holds the event loop longer than this; 0 turns it off.
    pub loop_hold_warn_ms: f64,
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
            loop_hold_warn_ms: 50.0,
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
            editor: EditorCfg::default(),
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
    pub fn hinge(&self) -> HingeCfg {
        HingeCfg {
            neutral_deg: self.neutral_deg,
            direction: self.direction,
            gain: self.gain,
            hinge_axis: self.hinge_axis,
            min_deg: self.min_deg,
            max_deg: self.max_deg,
        }
    }

    pub fn elevation_deg(&self, servo_deg: f32) -> f32 {
        self.hinge().elevation_deg(servo_deg)
    }
}

/// A joint turned by a servo through a linkage, like the wings.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct HingeCfg {
    /// Servo angle at which the joint is at 0 degrees.
    pub neutral_deg: f32,
    /// +1 if a larger servo angle turns the joint positive, -1 if negative.
    pub direction: f32,
    /// Joint degrees per servo degree.
    pub gain: f32,
    /// Axis in the part's local space; a positive angle turns right-handed about it.
    pub hinge_axis: [f32; 3],
    pub min_deg: f32,
    pub max_deg: f32,
}

impl Default for HingeCfg {
    fn default() -> Self {
        Self {
            neutral_deg: 90.0,
            direction: 1.0,
            gain: 1.0,
            hinge_axis: [1.0, 0.0, 0.0],
            min_deg: -90.0,
            max_deg: 90.0,
        }
    }
}

impl HingeCfg {
    /// Joint angle for a servo angle, within the travel limits.
    pub fn elevation_deg(&self, servo_deg: f32) -> f32 {
        ((servo_deg - self.neutral_deg) * self.direction * self.gain)
            .clamp(self.min_deg, self.max_deg)
    }
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    /// A glowing sphere: brightness 0-1 of `colour`, or an RGB value.
    #[default]
    Led,
    /// `count` glowing dots in a row or ring, one RGB value each.
    Pixels,
    /// A shape turned about a hinge by a servo angle.
    ServoPart,
    /// A cap on the model that can be pressed with the mouse (an input).
    Button,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PixelLayout {
    #[default]
    Row,
    Ring,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PartShape {
    #[default]
    Box,
    Cylinder,
}

/// One generic part on the droid. Fields that don't apply to its kind are
/// ignored, so a TOML entry only lists what it needs. Unknown fields are an
/// error, so a typo isn't silently dropped.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PartCfg {
    pub name: String,
    pub kind: PartKind,
    /// Follows this PCA9685 channel: brightness for an LED or pixels, servo
    /// angle for a servo part.
    pub pca: Option<usize>,
    /// Follows this board pin (`pwmio` or `digitalio`); a button drives it
    /// as an input instead.
    pub pin: Option<String>,
    /// Node the part sits on: `body` (the droid's root) or a node name such
    /// as `wing_right`.
    pub anchor: String,
    /// Duty fraction that counts as full brightness when following a channel
    /// or PWM pin; 0.3275 for an LED driven like the eyes (0-6550 us).
    pub full_duty: f32,
    /// Position relative to the anchor. The part's local +Y points out of the
    /// surface it sits on.
    pub pos: [f32; 3],
    /// Rotation as X, Y, Z Euler angles in degrees.
    pub rot_deg: [f32; 3],
    /// Radius of an LED, pixel or button cap; half-width of a servo shape.
    pub size: f32,
    /// LED colour at full brightness; body colour of a servo shape or button.
    pub colour: [f32; 3],
    /// Pixels: how many.
    pub count: usize,
    pub layout: PixelLayout,
    /// Pixels: distance between neighbouring dots.
    pub spacing: f32,
    /// Servo part: what it looks like. It extends along local +Z from the hinge.
    pub shape: PartShape,
    pub hinge: HingeCfg,
}

impl Default for PartCfg {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: PartKind::Led,
            pca: None,
            pin: None,
            anchor: PartCfg::BODY.into(),
            full_duty: 1.0,
            pos: [0.0; 3],
            rot_deg: [0.0; 3],
            size: 0.05,
            colour: [1.0, 1.0, 1.0],
            count: 8,
            layout: PixelLayout::Row,
            spacing: 0.15,
            shape: PartShape::Box,
            hinge: HingeCfg::default(),
        }
    }
}

/// Where a part's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartSource<'a> {
    Pca(usize),
    Pin(&'a str),
    /// Set only by a shim through `simhw` (or, for a button, the mouse).
    Shim,
}

impl PartCfg {
    /// The droid's root, the default anchor.
    pub const BODY: &'static str = "body";
    pub const MAX_NAME: usize = 32;
    pub const MAX_PIXELS: usize = 1024;

    pub fn source(&self) -> PartSource<'_> {
        match (self.pca, &self.pin) {
            (Some(ch), _) => PartSource::Pca(ch),
            (None, Some(pin)) => PartSource::Pin(pin),
            (None, None) => PartSource::Shim,
        }
    }

    /// Why this entry can't be used, if it can't.
    pub fn check(&self) -> Result<(), String> {
        let name_ok = !self.name.is_empty()
            && self.name.len() <= Self::MAX_NAME
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !name_ok {
            return Err(format!(
                "name must be 1-{} letters, digits, '_' or '-'",
                Self::MAX_NAME
            ));
        }
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
        if !finite(&self.pos) || !finite(&self.rot_deg) || !finite(&self.colour) {
            return Err("pos, rot_deg and colour must be numbers".into());
        }
        if !(self.size.is_finite() && self.size > 0.0) {
            return Err("size must be more than 0".into());
        }
        if self.kind == PartKind::Pixels {
            if !(1..=Self::MAX_PIXELS).contains(&self.count) {
                return Err(format!("count must be 1-{}", Self::MAX_PIXELS));
            }
            if !(self.spacing.is_finite() && self.spacing > 0.0) {
                return Err("spacing must be more than 0".into());
            }
        }
        if self.kind == PartKind::ServoPart {
            let h = &self.hinge;
            if !finite(&[h.neutral_deg, h.direction, h.gain, h.min_deg, h.max_deg])
                || !finite(&h.hinge_axis)
            {
                return Err("hinge values must be numbers".into());
            }
            if h.min_deg > h.max_deg {
                return Err("hinge min_deg is above max_deg".into());
            }
            if Vec3::from(h.hinge_axis).length_squared() < 1e-12 {
                return Err("hinge_axis can't be zero".into());
            }
        }
        if !(self.full_duty.is_finite() && self.full_duty > 0.0 && self.full_duty <= 1.0) {
            return Err("full_duty must be more than 0 and at most 1".into());
        }
        if self.pca.is_some() && self.pin.is_some() {
            return Err("give pca or pin, not both".into());
        }
        if let Some(ch) = self.pca {
            if ch > 15 {
                return Err("pca must be a channel 0-15".into());
            }
            if self.kind == PartKind::Button {
                return Err("a button is an input: give it a pin, or no source".into());
            }
        }
        if let Some(pin) = &self.pin {
            let pin_ok = !pin.is_empty()
                && pin.len() <= Self::MAX_NAME
                && pin
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if !pin_ok {
                return Err("pin must be a board pin name such as \"D5\"".into());
            }
        }
        if self.anchor.is_empty() {
            return Err("anchor can't be empty (\"body\" is the droid itself)".into());
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wing_elevation_unchanged() {
        let cfg = SimConfig::default();
        // Right wing: lower servo angle = higher wing.
        assert_eq!(cfg.wing_right.elevation_deg(90.0), 0.0);
        assert!((cfg.wing_right.elevation_deg(0.0) - 40.5).abs() < 1e-4);
        assert_eq!(cfg.wing_right.elevation_deg(180.0), 0.0);
        // Left wing: clamps at its 75 degree limit.
        assert!((cfg.wing_left.elevation_deg(100.0) - 40.5).abs() < 1e-4);
        assert_eq!(cfg.wing_left.elevation_deg(180.0), 75.0);
    }

    #[test]
    fn hinge_applies_neutral_direction_gain_and_limits() {
        let hinge = HingeCfg {
            neutral_deg: 45.0,
            direction: -1.0,
            gain: 2.0,
            min_deg: -30.0,
            max_deg: 60.0,
            ..HingeCfg::default()
        };
        assert_eq!(hinge.elevation_deg(45.0), 0.0);
        assert_eq!(hinge.elevation_deg(35.0), 20.0);
        assert_eq!(hinge.elevation_deg(0.0), 60.0);
        assert_eq!(hinge.elevation_deg(180.0), -30.0);
    }

    #[test]
    fn parts_read_from_toml() {
        #[derive(Deserialize)]
        struct File {
            part: Vec<PartCfg>,
        }
        let file: File = toml::from_str(
            r#"
            [[part]]
            name = "lamp"
            pos = [0.0, 2.0, 0.5]
            colour = [1.0, 0.5, 0.0]

            [[part]]
            name = "ring"
            kind = "pixels"
            count = 12
            layout = "ring"

            [[part]]
            name = "flap"
            kind = "servo_part"
            shape = "cylinder"
            hinge = { neutral_deg = 0.0, max_deg = 45.0 }

            [[part]]
            name = "go"
            kind = "button"
            "#,
        )
        .expect("valid parts");
        let [lamp, ring, flap, go] = &file.part[..] else {
            panic!("expected four parts");
        };
        assert_eq!(lamp.kind, PartKind::Led);
        assert_eq!(lamp.pos, [0.0, 2.0, 0.5]);
        assert_eq!(lamp.size, PartCfg::default().size);
        assert_eq!((ring.kind, ring.count), (PartKind::Pixels, 12));
        assert_eq!(ring.layout, PixelLayout::Ring);
        assert_eq!(
            (flap.kind, flap.shape),
            (PartKind::ServoPart, PartShape::Cylinder)
        );
        assert_eq!(flap.hinge.max_deg, 45.0);
        assert_eq!(flap.hinge.gain, 1.0);
        assert_eq!(go.kind, PartKind::Button);
        assert!(toml::from_str::<File>("[[part]]\nkind = \"display\"").is_err());
    }
}
