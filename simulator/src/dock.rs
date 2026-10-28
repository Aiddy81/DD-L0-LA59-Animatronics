//! The dockable layout: the 3D view and every control panel are tabs the user
//! can drag, split and close. This module owns the layout and the 3D view's
//! on-screen rectangle; what each panel draws stays in `ui.rs`.

use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::window::{
    Monitor, MonitorSelection, PrimaryMonitor, PrimaryWindow, WindowCloseRequested, WindowPosition,
    WindowRef, WindowResolution,
};
use bevy_egui::egui;
use egui_dock::{DockState, NodeIndex, SurfaceIndex, TabStyle, TabViewer};
use serde::{Deserialize, Serialize};

use crate::layout::{DetachedSave, FloatSave, Layout};
use crate::prefs::Prefs;
use crate::theme::{ActiveTheme, Palette};

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
    /// Where the floating windows were drawn last frame, kept by `panels` so a
    /// snapshot can save them.
    pub floats: Vec<FloatSave>,
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

/// Opens an OS window for `place.pane`. `clear` is the colour its camera clears to:
/// egui draws onto the window without clearing it first.
pub fn spawn_detached(commands: &mut Commands, place: &DetachedSave, clear: Color) -> Entity {
    let window = commands
        .spawn((
            Window {
                title: format!("DD-L0-LA59 \u{2014} {}", place.pane.title()),
                resolution: WindowResolution::new(place.w, place.h),
                position: place
                    .x
                    .zip(place.y)
                    .map_or(WindowPosition::Automatic, |(x, y)| {
                        WindowPosition::At(IVec2::new(x, y))
                    }),
                // A second vsync'd swap chain makes each frame wait for two
                // vblanks; the main window's vsync already paces the app.
                present_mode: bevy::window::PresentMode::AutoNoVsync,
                ..default()
            },
            Detached {
                pane: place.pane,
                scale: place.scale,
            },
            // The clear camera is a child; without these Bevy warns the hierarchy
            // is inconsistent.
            Transform::default(),
            Visibility::default(),
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

/// What `save_prefs` and the Layouts menu store for one detached window.
pub fn detached_save(pane: Pane, scale: f32, window: &Window) -> DetachedSave {
    let (x, y) = match window.position {
        WindowPosition::At(p) => (Some(p.x), Some(p.y)),
        _ => (None, None),
    };
    DetachedSave {
        pane,
        x,
        y,
        w: window.resolution.width(),
        h: window.resolution.height(),
        scale,
    }
}

/// The layout on screen now. Focus mode is not part of it: the real layout is
/// kept underneath. Detached windows are sorted so the result doesn't change
/// just because the ECS iterates them in another order.
pub fn snapshot(dock: &Dock, mut detached: Vec<DetachedSave>) -> Layout {
    detached.sort_by_key(|d| Pane::ALL.iter().position(|p| *p == d.pane));
    Layout {
        dock: dock.state.clone(),
        detached,
        floats: dock
            .floats
            .iter()
            .filter(|f| {
                matches!(
                    dock.state.get_surface(SurfaceIndex(f.surface)),
                    Some(egui_dock::Surface::Window(..))
                )
            })
            .cloned()
            .collect(),
    }
}

/// Where `state`'s floating windows were drawn, read from egui's memory:
/// egui_dock 0.16 never records it in its own `WindowState`. The id is the one
/// egui_dock gives its window areas (`format!("window {surface:?}")`). The
/// size is the content size, as `set_size` takes, so a restore doesn't grow
/// the window by its frame each time.
pub fn read_floats(ctx: &egui::Context, state: &DockState<Pane>) -> Vec<FloatSave> {
    let frame = egui::Frame::window(&ctx.style()).total_margin().sum();
    state
        .iter_surfaces()
        .enumerate()
        .filter(|(_, surface)| matches!(surface, egui_dock::Surface::Window(..)))
        .filter_map(|(n, _)| {
            let id = egui::Id::new(format!("window {:?}", SurfaceIndex(n)));
            let rect = ctx.memory(|m| m.area_rect(id))?;
            let size = rect.size() - frame;
            Some(FloatSave {
                surface: n,
                x: rect.min.x,
                y: rect.min.y,
                w: size.x,
                h: size.y,
            })
        })
        .collect()
}

/// Switches to `layout` (already validated): closes the open detached
/// windows, replaces the dock and opens the layout's own windows.
pub fn apply_layout(
    layout: Layout,
    dock: &mut Dock,
    commands: &mut Commands,
    open: impl IntoIterator<Item = Entity>,
    clear: Color,
) {
    for window in open {
        commands.entity(window).despawn();
    }
    dock.state = layout.dock;
    dock.focus = None;
    for place in &layout.detached {
        spawn_detached(commands, place, clear);
    }
}

/// Opens the detached windows the saved layout had.
fn spawn_saved_windows(
    mut commands: Commands,
    prefs: Res<Prefs>,
    theme: Res<ActiveTheme>,
    primary: Query<(), (With<PrimaryWindow>, With<bevy_egui::EguiContext>)>,
    mut done: Local<bool>,
) {
    // Not at Startup: bevy_egui sets up each window's render node on the
    // frame its context appears, and a second window created alongside the
    // primary one left the primary window with no egui drawn at all.
    if *done || primary.is_empty() {
        return;
    }
    *done = true;
    let clear = crate::scene::to_color(theme.palette.panel);
    for place in prefs.layout.iter().flat_map(|l| &l.detached) {
        spawn_detached(&mut commands, place, clear);
    }
}

/// Marks a window whose position and size have been checked against the
/// monitors, so it is only done once.
#[derive(Component)]
struct OnScreenChecked;

/// Smallest part of a window that must be over a monitor for it to count as
/// reachable (enough to grab the title bar), in physical pixels.
const MIN_VISIBLE: IVec2 = IVec2::new(64, 32);

/// Brings back a window a saved position left off-screen (a monitor that was
/// unplugged, a changed resolution) and shrinks one bigger than its monitor.
/// Runs for every window once, so it also covers windows a layout switch
/// opens.
fn keep_windows_on_screen(
    mut commands: Commands,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut windows: Query<(Entity, &mut Window), Without<OnScreenChecked>>,
) {
    // Bevy learns the monitors a moment after start-up.
    if monitors.is_empty() {
        return;
    }
    let primary = monitors
        .iter()
        .find_map(|(m, is_primary)| is_primary.then_some(m))
        .or_else(|| monitors.iter().next().map(|(m, _)| m));
    for (entity, mut window) in &mut windows {
        let mut monitor = primary;
        match window.position {
            WindowPosition::At(pos) => {
                let size = IVec2::new(
                    window.resolution.physical_width().min(i32::MAX as u32) as i32,
                    window.resolution.physical_height().min(i32::MAX as u32) as i32,
                );
                let visible = monitors
                    .iter()
                    .map(|(m, _)| m)
                    .filter_map(|m| {
                        let overlap = overlap(pos, size, m);
                        (overlap.x >= MIN_VISIBLE.x && overlap.y >= MIN_VISIBLE.y)
                            .then_some((i64::from(overlap.x) * i64::from(overlap.y), m))
                    })
                    .max_by_key(|(area, _)| *area);
                match visible {
                    Some((_, m)) => monitor = Some(m),
                    None => window.position = WindowPosition::Centered(MonitorSelection::Primary),
                }
            }
            // Not placed yet; looked at again once the OS has put it somewhere.
            WindowPosition::Automatic => continue,
            WindowPosition::Centered(_) => {}
        }
        if let Some(m) = monitor.filter(|m| m.scale_factor > 0.0) {
            let max_w = (f64::from(m.physical_width) / m.scale_factor) as f32;
            let max_h = (f64::from(m.physical_height) / m.scale_factor) as f32;
            let (w, h) = (window.resolution.width(), window.resolution.height());
            if w > max_w || h > max_h {
                window.resolution.set(w.min(max_w), h.min(max_h));
            }
        }
        commands.entity(entity).insert(OnScreenChecked);
    }
}

/// The size of the part of a window at `pos` (physical pixels) that is over
/// `monitor`; zero or negative in an axis when they don't overlap there.
fn overlap(pos: IVec2, size: IVec2, monitor: &Monitor) -> IVec2 {
    let mon_pos = monitor.physical_position;
    let mon_end = mon_pos.as_i64vec2() + monitor.physical_size().as_i64vec2();
    let end = pos.as_i64vec2() + size.as_i64vec2();
    let lo = pos.as_i64vec2().max(mon_pos.as_i64vec2());
    let hi = end.min(mon_end);
    (hi - lo)
        .clamp(
            bevy::math::I64Vec2::splat(i64::from(i32::MIN)),
            bevy::math::I64Vec2::splat(i64::from(i32::MAX)),
        )
        .as_ivec2()
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
            // Worked out by hand: a hover-only widget here never reports
            // hovered, because the dock's own tab-body widget wins egui's hit
            // test. The pointer counts when nothing (a floating panel, a menu)
            // is above the dock's layer.
            let ctx = ui.ctx();
            self.viewport.hovered = ctx.pointer_hover_pos().is_some_and(|pos| {
                rect.contains(pos) && ctx.layer_id_at(pos) == Some(ui.layer_id())
            });
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
        // The saved layout was checked when the prefs loaded.
        let saved = app
            .world()
            .get_resource::<Prefs>()
            .and_then(|prefs| prefs.layout.clone());
        // The saved floats are kept until the dock has measured its windows,
        // so quitting straight after start doesn't lose their places.
        let (state, floats) = saved.map_or_else(
            || (default_layout(), Vec::new()),
            |layout| (layout.dock, layout.floats),
        );
        app.insert_resource(Dock {
            state,
            focus: None,
            floats,
        })
        .insert_resource(ViewportRect::default())
        .add_systems(
            Update,
            (spawn_saved_windows, close_windows, keep_windows_on_screen),
        );
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
