//! User preferences kept between runs in `ddl0-sim.prefs.toml` next to the exe:
//! window size, the toolbar toggles and the model and room-light choices.

use std::collections::BTreeMap;
use std::path::Path;

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::layout::{self, Layout};
use crate::scene::RoomLight;
use crate::theme::{self, ThemeChoice};

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
    /// Physical position of the main window; `None` lets the OS place it.
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    /// `None` follows `[model] kind` in sim.toml; `Some` is the user's choice.
    pub model_gltf: Option<bool>,
    pub room_light: RoomLight,
    pub theme: ThemeChoice,
    /// egui zoom factor; kept inside theme::SCALE_MIN..=SCALE_MAX on load.
    pub ui_scale: f32,
    /// Smooth transitions (camera moves, panel fades).
    pub animations: bool,
    pub strict: bool,
    pub board_speed: bool,
    pub servo_speed: bool,
    pub auto_reload: bool,
    /// Which readouts are drawn on the 3D model.
    pub overlay_wings: bool,
    pub overlay_eyes: bool,
    pub overlay_led: bool,
    pub overlay_accel: bool,
    /// The simulator's output volume (0..1) and mute; the firmware's own
    /// mixer level still applies on top.
    pub volume: f32,
    pub muted: bool,
    pub show_firmware: bool,
    pub show_sim: bool,
    /// The plot's time window, in seconds.
    pub plot_window_s: f32,
    /// Plot series that are unticked, by name ("Accel x"), so adding a
    /// series later doesn't shift the others.
    pub plot_hidden: Vec<String>,
    // The layouts are tables, so they stay last: TOML wants plain values first.
    /// The dock layout in use; `None` is the Default layout.
    pub layout: Option<Layout>,
    /// The user's named layouts.
    pub layouts: BTreeMap<String, Layout>,
}

/// The window size range worth remembering. A minimised window reports ~0,
/// and a size in the tens of thousands would not survive window creation.
pub const MIN_WINDOW: f32 = 200.0;
pub const MAX_WINDOW: f32 = 8192.0;

impl Default for Prefs {
    fn default() -> Self {
        Self {
            window_width: 1500.0,
            window_height: 920.0,
            window_x: None,
            window_y: None,
            model_gltf: None,
            room_light: RoomLight::default(),
            theme: ThemeChoice::default(),
            ui_scale: 1.0,
            animations: true,
            strict: true,
            board_speed: true,
            servo_speed: true,
            auto_reload: true,
            overlay_wings: false,
            overlay_eyes: false,
            overlay_led: false,
            overlay_accel: false,
            volume: 1.0,
            muted: false,
            show_firmware: true,
            show_sim: true,
            plot_window_s: 20.0,
            plot_hidden: Vec::new(),
            layout: None,
            layouts: BTreeMap::new(),
        }
    }
}

/// Loads the preferences. A missing file is normal (first run); an unreadable
/// or invalid one gives the defaults plus a warning for the log.
pub fn load(path: &Path) -> (Prefs, Option<String>) {
    let invalid = |e: &dyn std::fmt::Display| {
        (
            Prefs::default(),
            Some(format!(
                "[sim] {} is invalid, using defaults: {e}",
                path.display()
            )),
        )
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Prefs::default(), None),
        Err(e) => return invalid(&e),
    };
    let (mut prefs, mut notes) = match toml::from_str::<Prefs>(&text) {
        Ok(prefs) => (prefs, Vec::new()),
        Err(e) => match without_layouts(&text) {
            // A layout this build can't read (say, a pane it no longer has)
            // must not cost the user every other setting.
            Some(prefs) => (prefs, vec![format!("layouts dropped, {e}")]),
            None => return invalid(&e),
        },
    };
    prefs.ui_scale = theme::clamp_scale(prefs.ui_scale);
    prefs.window_width = clamp_window(prefs.window_width, Prefs::default().window_width);
    prefs.window_height = clamp_window(prefs.window_height, Prefs::default().window_height);
    check_layouts(&mut prefs, &mut notes);
    let warning =
        (!notes.is_empty()).then(|| format!("[sim] {}: {}", path.display(), notes.join("; ")));
    (prefs, warning)
}

fn clamp_window(size: f32, default: f32) -> f32 {
    if size.is_finite() {
        size.clamp(MIN_WINDOW, MAX_WINDOW)
    } else {
        default
    }
}

/// The file's settings with the layout tables removed, for when the layouts
/// are what failed to parse.
fn without_layouts(text: &str) -> Option<Prefs> {
    let mut table = toml::from_str::<toml::Table>(text).ok()?;
    table.remove("layout");
    table.remove("layouts");
    table.try_into().ok()
}

/// Drops the layouts that fail [`layout::validate`] and notes why.
fn check_layouts(prefs: &mut Prefs, notes: &mut Vec<String>) {
    if let Some(current) = prefs.layout.as_mut() {
        if let Err(e) = layout::validate(current) {
            notes.push(format!("the current layout was dropped: {e}"));
            prefs.layout = None;
        }
    }
    prefs.layouts.retain(|name, saved| {
        let result = match layout::sanitize_name(name) {
            Some(n) if n == *name && !layout::is_builtin(name) => layout::validate(saved),
            _ => Err("unusable name".into()),
        };
        if let Err(e) = &result {
            notes.push(format!("layout \"{name}\" was dropped: {e}"));
        }
        result.is_ok()
    });
}

impl Prefs {
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_text(tag: &str, text: &str) -> (Prefs, Option<String>) {
        let path =
            std::env::temp_dir().join(format!("ddl0-prefs-{}-{tag}.toml", std::process::id()));
        std::fs::write(&path, text).expect("writes the temp file");
        let loaded = load(&path);
        let _ = std::fs::remove_file(&path);
        loaded
    }

    #[test]
    fn absurd_window_sizes_are_clamped() {
        let (prefs, warning) = load_text("size", "window_width = 100.0\nwindow_height = 65000.0\n");
        assert_eq!(prefs.window_width, MIN_WINDOW);
        assert_eq!(prefs.window_height, MAX_WINDOW);
        assert!(warning.is_none());
    }

    #[test]
    fn animations_default_on_and_round_trip() {
        let (prefs, _) = load_text("anim-missing", "ui_scale = 1.5\n");
        assert!(prefs.animations);
        let (prefs, _) = load_text("anim-off", "animations = false\n");
        assert!(!prefs.animations);
        let text = toml::to_string(&prefs).expect("serialises");
        let (again, _) = load_text("anim-again", &text);
        assert!(!again.animations);
    }

    #[test]
    fn an_unreadable_layout_keeps_the_other_settings() {
        let (prefs, warning) = load_text("layout", "ui_scale = 1.5\n[layout]\nbogus = 1\n");
        assert_eq!(prefs.ui_scale, 1.5);
        assert!(prefs.layout.is_none());
        assert!(warning.is_some());
    }
}
