//! The dockable layout: the 3D view and every control panel are tabs the user
//! can drag, split and close. This module owns the layout and the 3D view's
//! on-screen rectangle; what each panel draws stays in `ui.rs`.

use bevy::prelude::*;
use bevy_egui::egui;
use egui_dock::{DockState, TabViewer};
use serde::{Deserialize, Serialize};

/// Everything that can sit in a dock tab.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pane {
    Viewport,
    Motion,
    State,
    Pca,
    Board,
    Audio,
    Plot,
    Settings,
    Log,
    Shortcuts,
}

impl Pane {
    // Not used until the View menu lists the panes.
    #[allow(dead_code)]
    pub const ALL: [Pane; 10] = [
        Pane::Viewport,
        Pane::Motion,
        Pane::State,
        Pane::Pca,
        Pane::Board,
        Pane::Audio,
        Pane::Plot,
        Pane::Settings,
        Pane::Log,
        Pane::Shortcuts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Pane::Viewport => "3D view",
            Pane::Motion => "Motion (LIS3DH)",
            Pane::State => "State",
            Pane::Pca => "PCA9685",
            Pane::Board => "Board",
            Pane::Audio => "Audio",
            Pane::Plot => "Plot",
            Pane::Settings => "Settings (settings.toml)",
            Pane::Log => "Log",
            Pane::Shortcuts => "Shortcuts",
        }
    }
}

#[derive(Resource)]
pub struct Dock {
    pub state: DockState<Pane>,
}

/// The 3D view in the top-left, controls on the right, log and plot below it.
/// Shortcuts is left out: it is opened on demand.
pub fn default_layout() -> DockState<Pane> {
    let mut state = DockState::new(vec![Pane::Viewport]);
    let tree = state.main_surface_mut();
    let [view, right] = tree.split_right(
        egui_dock::NodeIndex::root(),
        0.76,
        vec![Pane::Motion, Pane::Audio, Pane::Settings],
    );
    tree.split_below(right, 0.5, vec![Pane::State, Pane::Pca, Pane::Board]);
    tree.split_below(view, 0.78, vec![Pane::Log, Pane::Plot]);
    state
}

/// Where the 3D view is this frame, for the camera viewport and the mouse
/// handling. Reset by the UI each frame; `None` when the tab is hidden.
#[derive(Resource, Default)]
pub struct ViewportRect {
    /// In egui points.
    pub rect: Option<egui::Rect>,
    /// The pointer is over the view with no panel or window on top of it.
    pub hovered: bool,
    /// Pixels per point, to convert `rect` to physical pixels.
    pub ppp: f32,
}

/// Draws the tabs: the 3D view itself is only measured (the camera renders
/// into its rect); every other pane goes to `draw`.
pub struct Viewer<'a, F: FnMut(&mut egui::Ui, Pane)> {
    pub draw: F,
    pub viewport: &'a mut ViewportRect,
}

impl<F: FnMut(&mut egui::Ui, Pane)> TabViewer for Viewer<'_, F> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Pane) {
        if *tab == Pane::Viewport {
            let rect = ui.max_rect();
            self.viewport.rect = Some(rect);
            self.viewport.hovered = ui
                .interact(rect, ui.id().with("viewport"), egui::Sense::hover())
                .hovered();
            self.viewport.ppp = ui.ctx().pixels_per_point();
        } else {
            (self.draw)(ui, *tab);
        }
    }

    fn closeable(&mut self, tab: &mut Pane) -> bool {
        *tab != Pane::Viewport
    }

    // The scene is drawn behind egui, so the view's tab must not paint over it.
    fn clear_background(&self, tab: &Pane) -> bool {
        *tab != Pane::Viewport
    }

    // The log and plot scroll and zoom themselves.
    fn scroll_bars(&self, tab: &Pane) -> [bool; 2] {
        match tab {
            Pane::Viewport | Pane::Log | Pane::Plot => [false, false],
            _ => [true, true],
        }
    }

    fn allowed_in_windows(&self, tab: &mut Pane) -> bool {
        *tab != Pane::Viewport
    }
}

pub struct DockPlugin;

impl Plugin for DockPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Dock {
            state: default_layout(),
        })
        .insert_resource(ViewportRect::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the layout could be saved in the prefs file as TOML.
    #[test]
    fn layout_round_trips_through_toml() {
        let layout = default_layout();
        let text = toml::to_string(&layout).expect("layout serialises to TOML");
        let back: DockState<Pane> = toml::from_str(&text).expect("layout parses back");
        // Tree's Debug output is opaque, so compare the re-serialised text instead.
        assert_eq!(text, toml::to_string(&back).expect("re-serialises"));
    }
}
