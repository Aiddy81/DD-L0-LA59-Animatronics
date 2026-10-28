//! The dockable layout: the 3D view and every control panel are tabs the user
//! can drag, split and close. This module owns the layout and the 3D view's
//! on-screen rectangle; what each panel draws stays in `ui.rs`.

use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::window::{
    PrimaryWindow, WindowCloseRequested, WindowPosition, WindowRef, WindowResolution,
};
use bevy_egui::egui;
use egui_dock::{DockState, NodeIndex, SurfaceIndex, TabStyle, TabViewer};
use serde::{Deserialize, Serialize};

use crate::theme::Palette;

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
    /// Focus mode: a one-pane layout shown in place of `state`, which is left
    /// untouched so leaving focus mode restores it exactly.
    pub focus: Option<DockState<Pane>>,
}

/// Settings every `DockState` gets, however it was made or loaded.
pub fn prepare(state: &mut DockState<Pane>) {
    state.translations.tab_context_menu.eject_button = "Float".into();
}

/// The dock's look, from the theme's tokens so it follows theme changes.
pub fn dock_style(egui_style: &egui::Style, palette: &Palette) -> egui_dock::Style {
    let mut style = egui_dock::Style::from_egui(egui_style);

    style.tab_bar.bg_fill = palette.panel;
    style.tab_bar.hline_color = palette.border;

    let tab = &mut style.tab;
    for t in [&mut tab.active, &mut tab.focused] {
        t.bg_fill = palette.surface;
        t.text_color = palette.text;
    }
    tab.focused.outline_color = palette.accent;
    tab.inactive.text_color = palette.muted;
    tab.hovered.text_color = palette.text;
    tab.tab_body.bg_fill = palette.panel;

    style.separator.color_idle = palette.border;
    style.separator.color_hovered = palette.accent;
    style.separator.color_dragged = palette.accent;

    style.overlay.selection_color = palette.accent.gamma_multiply(0.35);
    style.overlay.button_color = palette.accent;
    style
}

/// Whether `pane` is open in any surface.
pub fn is_shown(state: &DockState<Pane>, pane: Pane) -> bool {
    state.find_tab(&pane).is_some()
}

/// Opens or closes `pane`. The 3D view can't be hidden: the camera and mouse
/// handling assume it exists.
pub fn set_shown(state: &mut DockState<Pane>, pane: Pane, show: bool) {
    if pane == Pane::Viewport {
        return;
    }
    if !show {
        if let Some(at) = state.find_tab(&pane) {
            state.remove_tab(at);
        }
    } else if !is_shown(state, pane) {
        if pane == Pane::Shortcuts {
            state.add_window(vec![pane]);
        } else if state.focused_leaf().is_some_and(|(s, _)| !s.is_main()) {
            // A panel dropped into a small floating window would be easy to miss.
            state.main_surface_mut().push_to_focused_leaf(pane);
        } else {
            state.push_to_focused_leaf(pane);
        }
    }
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
    prepare(&mut state);
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

/// A pane torn out into its own OS window. The window's egui context draws
/// the pane; `scale` is that window's own UI scale.
#[derive(Component)]
pub struct Detached {
    pub pane: Pane,
    pub scale: f32,
}

/// Where and how big a detached window opens.
pub struct DetachedPlace {
    /// `None` lets the OS place it.
    pub position: Option<IVec2>,
    /// In logical pixels.
    pub size: Vec2,
    pub scale: f32,
}

impl DetachedPlace {
    pub fn new(scale: f32) -> Self {
        Self {
            position: None,
            size: Vec2::new(480.0, 600.0),
            scale,
        }
    }
}

/// Opens an OS window for `pane`. `clear` is the colour its camera clears to:
/// egui draws onto the window without clearing it first.
pub fn spawn_detached(
    commands: &mut Commands,
    pane: Pane,
    place: &DetachedPlace,
    clear: Color,
) -> Entity {
    let window = commands
        .spawn((
            Window {
                title: format!("DD-L0-LA59 \u{2014} {}", pane.title()),
                resolution: WindowResolution::new(place.size.x, place.size.y),
                position: place
                    .position
                    .map_or(WindowPosition::Automatic, WindowPosition::At),
                // A second vsync'd swap chain makes each frame wait for two
                // vblanks; the main window's vsync already paces the app.
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            },
            Detached {
                pane,
                scale: place.scale,
            },
        ))
        .id();
    commands.entity(window).with_child((
        Camera2d,
        Camera {
            target: RenderTarget::Window(WindowRef::Entity(window)),
            clear_color: ClearColorConfig::Custom(clear),
            ..default()
        },
    ));
    window
}

/// Closes a detached window and puts its pane back in the main dock.
pub fn redock(dock: &mut Dock, commands: &mut Commands, window: Entity, pane: Pane) {
    commands.entity(window).despawn();
    set_shown(&mut dock.state, pane, true);
}

/// Something a tab's context menu asked for, applied once the dock is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaneAction {
    Detach,
}

/// Draws the tabs: the 3D view itself is only measured (the camera renders
/// into its rect); every other pane goes to `draw`.
pub struct Viewer<'a, F: FnMut(&mut egui::Ui, Pane)> {
    pub draw: F,
    pub viewport: &'a mut ViewportRect,
    pub actions: &'a mut Vec<(Pane, PaneAction)>,
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

    // The 3D view fills its tab edge to edge.
    fn tab_style_override(&self, tab: &Pane, global: &TabStyle) -> Option<TabStyle> {
        (*tab == Pane::Viewport).then(|| {
            let mut style = global.clone();
            style.tab_body.inner_margin = egui::Margin::ZERO;
            style
        })
    }

    fn allowed_in_windows(&self, tab: &mut Pane) -> bool {
        *tab != Pane::Viewport
    }

    fn context_menu(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut Pane,
        _surface: SurfaceIndex,
        _node: NodeIndex,
    ) {
        if self.allowed_in_windows(tab) && ui.button("Detach to window").clicked() {
            self.actions.push((*tab, PaneAction::Detach));
            ui.close_menu();
        }
    }
}

/// The OS close button: quits for the main window, docks a detached pane back.
/// Bevy's own close handling is off so a detached pane isn't just lost.
fn close_windows(
    mut requests: EventReader<WindowCloseRequested>,
    mut commands: Commands,
    mut dock: ResMut<Dock>,
    primary: Query<(), With<PrimaryWindow>>,
    detached: Query<&Detached>,
    mut exit: EventWriter<AppExit>,
) {
    for request in requests.read() {
        let window = request.window;
        if primary.contains(window) {
            exit.write(AppExit::Success);
        } else if let Ok(d) = detached.get(window) {
            redock(&mut dock, &mut commands, window, d.pane);
        } else {
            commands.entity(window).despawn();
        }
    }
}

pub struct DockPlugin;

impl Plugin for DockPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Dock {
            state: default_layout(),
            focus: None,
        })
        .insert_resource(ViewportRect::default())
        .add_systems(Update, close_windows);
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
