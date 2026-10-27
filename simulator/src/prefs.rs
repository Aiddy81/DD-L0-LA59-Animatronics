//! User preferences kept between runs in `ddl0-sim.prefs.toml` next to the exe:
//! window and panel sizes, the toolbar toggles, section open/closed state and
//! the model and room-light choices.

use std::path::Path;

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::scene::RoomLight;

/// Everything remembered between runs. Missing keys fall back to the defaults
/// so a file written by an older build still loads.
///
/// The speed slider is deliberately not saved: a run always starts at 1x, so a
/// forgotten 200x can't make the next launch look broken.
#[derive(Resource, Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Prefs {
    pub window_width: f32,
    pub window_height: f32,
    pub panel_width: f32,
    pub log_height: f32,
    /// `None` follows `[model] kind` in sim.toml; `Some` is the user's choice.
    pub model_gltf: Option<bool>,
    pub room_light: RoomLight,
    pub strict: bool,
    pub board_speed: bool,
    pub servo_speed: bool,
    pub auto_reload: bool,
    pub open_motion: bool,
    pub open_pca: bool,
    pub open_board: bool,
    pub open_plot: bool,
    pub open_settings: bool,
    pub open_state: bool,
    pub show_firmware: bool,
    pub show_sim: bool,
    /// The plot's time window, in seconds.
    pub plot_window_s: f32,
    /// Plot series that are unticked, by name ("Accel x"), so adding a
    /// series later doesn't shift the others.
    pub plot_hidden: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            window_width: 1500.0,
            window_height: 920.0,
            panel_width: 300.0,
            log_height: 170.0,
            model_gltf: None,
            room_light: RoomLight::default(),
            strict: true,
            board_speed: true,
            servo_speed: true,
            auto_reload: true,
            open_motion: true,
            open_pca: true,
            open_board: true,
            open_plot: true,
            open_settings: true,
            open_state: true,
            show_firmware: true,
            show_sim: true,
            plot_window_s: 20.0,
            plot_hidden: Vec::new(),
        }
    }
}

/// Loads the preferences. A missing file is normal (first run); an unreadable
/// or invalid one gives the defaults plus a warning for the log.
pub fn load(path: &Path) -> (Prefs, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(prefs) => (prefs, None),
            Err(e) => (
                Prefs::default(),
                Some(format!(
                    "[sim] {} is invalid, using defaults: {e}",
                    path.display()
                )),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Prefs::default(), None),
        Err(e) => (
            Prefs::default(),
            Some(format!(
                "[sim] {} is invalid, using defaults: {e}",
                path.display()
            )),
        ),
    }
}

impl Prefs {
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}
