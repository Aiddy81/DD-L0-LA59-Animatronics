//! Control panels: run controls, motion input, channel readouts, plot, log.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::ecs::change_detection::Mut;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::WindowPosition;
use bevy_egui::egui::{self, Color32, Stroke};
use bevy_egui::EguiContexts;
use egui_plot::{HLine, Line, LineStyle, Plot, PlotBounds, PlotPoint, Text, VLine};

use crate::audio::{AudioMix, NowPlaying, Sounds};
use crate::capture::{unix_secs, ViewControls};
use crate::config::SimConfig;
use crate::deploy;
use crate::dock::{self, Detached, Dock, Pane, PaneAction, Viewer, ViewportRect};
use crate::editor;
use crate::hw::{hw, AudioCmd, LogKind, LogLine, MarkerKind, TaskState};
use crate::layout::{self, DetachedSave, Layout};
use crate::overlay::{Overlay, Overlays};
use crate::palette;
use crate::prefs::{Prefs, MAX_WINDOW, MIN_WINDOW};
use crate::pyhost::{PyHost, ScenarioRun};
use crate::scene::{glow_rgb, to_color, CameraView, GlowSource, ModelChoice, RoomLight};
use crate::settings::SettingsState;
use crate::theme::{self, ActiveTheme, Palette, ThemeChoice};
use crate::toast::{ToastKind, Toasts};

/// A gentle bump, added to the held reading along the chosen direction. From
/// rest it clears the firmware's NUDGE_Z_MG (250 mg) threshold.
pub(crate) const NUDGE_DELTA_G: f32 = 0.3;
/// A hard knock. From rest on +Z it reads 3 g, above HARD_MOTION_MG (2500 mg).
pub(crate) const SHAKE_DELTA_G: f32 = 2.0;
/// Choices for the Motion panel's direction picker.
const DIRECTIONS: [(&str, [f32; 3]); 6] = [
    ("+X", [1.0, 0.0, 0.0]),
    ("\u{2212}X", [-1.0, 0.0, 0.0]),
    ("+Y", [0.0, 1.0, 0.0]),
    ("\u{2212}Y", [0.0, -1.0, 0.0]),
    ("+Z", [0.0, 0.0, 1.0]),
    ("\u{2212}Z", [0.0, 0.0, -1.0]),
];
/// History kept for the plot: its longest window.
const HISTORY_SECONDS: f32 = 300.0;
/// The plot's window choices, in seconds.
const PLOT_WINDOWS: [f32; 7] = [5.0, 10.0, 20.0, 30.0, 60.0, 120.0, HISTORY_SECONDS];

/// Folders next to code.py that hold no firmware source.
pub(crate) const NOT_FIRMWARE: [&str; 4] = ["lib", "sounds", "sd", "__pycache__"];

/// Newest modification time of the firmware: code.py, the modules beside it
/// and settings.toml. A change to any of them should restart the run.
pub fn firmware_mtime(code_path: &Path) -> Option<SystemTime> {
    fn newest(dir: &Path, top: bool) -> Option<SystemTime> {
        let mut latest = None;
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let mtime = if path.is_dir() {
                if NOT_FIRMWARE.contains(&name.as_ref()) {
                    continue;
                }
                newest(&path, false)
            } else if name.ends_with(".py") || (top && name == "settings.toml") {
                entry.metadata().and_then(|m| m.modified()).ok()
            } else {
                None
            };
            latest = latest.max(mtime);
        }
        latest
    }
    newest(code_path.parent()?, true)
}

/// Newest change under `dist\shims\`: any .py file, or a folder (so adding
/// or deleting a file counts too). None while the folder doesn't exist.
pub fn shims_mtime(dir: &Path) -> Option<SystemTime> {
    let mut latest = std::fs::metadata(dir).and_then(|m| m.modified()).ok()?;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let mtime = if path.is_dir() {
            if entry.file_name() == "__pycache__" {
                continue;
            }
            shims_mtime(&path)
        } else if path.extension().is_some_and(|e| e == "py") {
            entry.metadata().and_then(|m| m.modified()).ok()
        } else {
            None
        };
        latest = latest.max(mtime.unwrap_or(latest));
    }
    Some(latest)
}

/// Modification time of one file; None if it doesn't exist.
pub fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Queues one reading `delta` g past the held value along `dir`, and marks
/// it on the plot.
fn push_reading(dir: [f32; 3], delta: f32, label: String) {
    let mut s = hw();
    let held = s.accel_g;
    s.accel_pulses
        .push_back(std::array::from_fn(|i| held[i] + dir[i] * delta));
    s.mark(MarkerKind::Motion, label);
}

/// A head click of any strength.
pub fn boop(dir: [f32; 3], delta: f32) {
    push_reading(dir, delta, format!("boop {delta:.1} g"));
}

pub fn nudge(dir: [f32; 3]) {
    push_reading(dir, NUDGE_DELTA_G, "nudge".to_owned());
}

pub fn shake(dir: [f32; 3]) {
    push_reading(dir, SHAKE_DELTA_G, "shake".to_owned());
}

/// Label of the axis direction that `dir` points closest to.
pub fn nearest_direction(dir: [f32; 3]) -> &'static str {
    let dot = |d: &[f32; 3]| d.iter().zip(dir).map(|(a, b)| a * b).sum::<f32>();
    DIRECTIONS
        .iter()
        .max_by(|(_, a), (_, b)| dot(a).total_cmp(&dot(b)))
        .map_or("", |(name, _)| *name)
}

fn toggle_pause() {
    let mut s = hw();
    let paused = s.clock.is_paused();
    s.clock.set_paused(!paused);
}

/// Steps of the paused clock offered on the toolbar, in virtual seconds.
const STEP_SHORT_S: f64 = 0.01;
const STEP_LONG_S: f64 = 0.1;
const SPEED_PRESETS: [f64; 3] = [1.0, 10.0, 100.0];

/// Advances a paused clock by `dt`; does nothing while it runs.
fn step_clock(dt: f64) {
    let mut s = hw();
    if s.clock.is_paused() {
        s.clock.step(dt);
    }
}

fn toggle_skip() {
    let mut s = hw();
    if s.skipping {
        s.set_speed(1.0);
        s.push_line("[sim] skip cancelled".to_owned());
    } else {
        s.start_skip();
    }
}

/// Everything the user can ask for, by key, toolbar control, menu or the
/// command palette. Every control pushes one of these and `dispatch` runs it,
/// so there is one place that does each thing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cmd {
    Pause,
    Step,
    StepShort,
    Skip,
    Restart,
    Stop,
    Idle,
    Nudge,
    Shake,
    ToggleHelp,
    CloseHelp,
    Focus,
    ScaleUp,
    ScaleDown,
    ScaleReset,
    /// An index into `SPEED_PRESETS`.
    Speed(usize),
    Camera(CameraView),
    Light(RoomLight),
    Screenshot,
    Record,
    Strict,
    BoardSpeed,
    ServoSpeed,
    LogMode,
    AutoReload,
    /// Show or hide one readout drawn on the 3D model.
    Overlay(Overlay),
    /// Smooth transitions on or off.
    Animations,
    /// True for the Lola glTF model, false for the primitive shapes.
    Model(bool),
    Deploy,
    TogglePane(Pane),
    Theme(ThemeChoice),
    /// A built-in layout, by name.
    Layout(&'static str),
    /// These three act on the main dock's focused tab.
    FloatPanel,
    DetachPanel,
    ClosePanel,
    DockBack,
    Palette,
    /// Move keyboard focus to the next or previous dock tab (F6).
    NextPanel,
    PrevPanel,
}

/// One line of the shortcut table. The Shortcuts window, the key handling,
/// the tooltips and the command palette all read `SHORTCUTS`, so they can't
/// drift apart. A row with no `cmd` only describes a mouse or keyboard
/// gesture; one with no `keys` is reachable from the palette alone.
struct Shortcut {
    group: &'static str,
    keys: &'static str,
    /// Empty for the rows whose text comes from their `cmd` (see `label`).
    action: &'static str,
    binds: &'static [(egui::Modifiers, egui::Key)],
    cmd: Option<Cmd>,
}

impl Shortcut {
    /// What the row does, in words.
    fn label(&self) -> Cow<'static, str> {
        if !self.action.is_empty() {
            return Cow::Borrowed(self.action);
        }
        match self.cmd {
            Some(Cmd::TogglePane(pane)) => Cow::Borrowed(pane.title()),
            Some(Cmd::Theme(choice)) => Cow::Borrowed(choice.label()),
            Some(Cmd::Light(light)) => Cow::Borrowed(light.label()),
            Some(Cmd::Overlay(overlay)) => Cow::Owned(format!("Overlay: {}", overlay.label())),
            Some(Cmd::Speed(i)) => SPEED_PRESETS
                .get(i)
                .map_or(Cow::Borrowed(""), |s| Cow::Owned(format!("{s}x"))),
            Some(Cmd::Layout(name)) => Cow::Borrowed(name),
            _ => Cow::Borrowed(""),
        }
    }
}

/// A mouse or keyboard gesture with no command behind it.
const fn info(group: &'static str, keys: &'static str, action: &'static str) -> Shortcut {
    Shortcut {
        group,
        keys,
        action,
        binds: &[],
        cmd: None,
    }
}

const fn bound(
    group: &'static str,
    keys: &'static str,
    action: &'static str,
    binds: &'static [(egui::Modifiers, egui::Key)],
    cmd: Cmd,
) -> Shortcut {
    Shortcut {
        group,
        keys,
        action,
        binds,
        cmd: Some(cmd),
    }
}

/// A command with no key; `action` may be empty when `Shortcut::label` can derive it.
const fn action(group: &'static str, label: &'static str, cmd: Cmd) -> Shortcut {
    bound(group, "", label, &[], cmd)
}

const NO_MODS: egui::Modifiers = egui::Modifiers::NONE;
const CTRL: egui::Modifiers = egui::Modifiers::CTRL;

const SHORTCUTS: &[Shortcut] = &[
    bound(
        "Run",
        "Space",
        "Pause / resume",
        &[(NO_MODS, egui::Key::Space)],
        Cmd::Pause,
    ),
    bound(
        "Run",
        ".",
        "Step the paused clock 100 ms",
        &[(NO_MODS, egui::Key::Period)],
        Cmd::Step,
    ),
    action("Run", "Step the paused clock 10 ms", Cmd::StepShort),
    bound(
        "Run",
        "F",
        "Skip to next event",
        &[(NO_MODS, egui::Key::F)],
        Cmd::Skip,
    ),
    bound(
        "Run",
        "R",
        "Restart code.py",
        &[(NO_MODS, egui::Key::R)],
        Cmd::Restart,
    ),
    action("Run", "Stop code.py", Cmd::Stop),
    bound(
        "Run",
        "I",
        "Trigger idle",
        &[(NO_MODS, egui::Key::I)],
        Cmd::Idle,
    ),
    action("Run", "Deploy to CIRCUITPY", Cmd::Deploy),
    bound(
        "Motion",
        "N",
        "Nudge",
        &[(NO_MODS, egui::Key::N)],
        Cmd::Nudge,
    ),
    bound(
        "Motion",
        "S",
        "Hard shake",
        &[(NO_MODS, egui::Key::S)],
        Cmd::Shake,
    ),
    info(
        "Motion",
        "Click the droid",
        "Boop (longer press = harder, 0.3-2 g)",
    ),
    action("Speed", "", Cmd::Speed(0)),
    action("Speed", "", Cmd::Speed(1)),
    action("Speed", "", Cmd::Speed(2)),
    action("Camera", "Front view", Cmd::Camera(CameraView::Front)),
    action("Camera", "Side view", Cmd::Camera(CameraView::Side)),
    action("Camera", "Top view", Cmd::Camera(CameraView::Top)),
    action("Camera", "Eyes close-up", Cmd::Camera(CameraView::Eyes)),
    action(
        "Camera",
        "Back to the starting view",
        Cmd::Camera(CameraView::Reset),
    ),
    info("Camera", "Left or right drag", "Orbit"),
    info("Camera", "Middle drag", "Pan"),
    info("Camera", "Scroll", "Zoom"),
    action("Light", "", Cmd::Light(RoomLight::Bright)),
    action("Light", "", Cmd::Light(RoomLight::Dim)),
    action("Light", "", Cmd::Light(RoomLight::Dark)),
    action("Capture", "Screenshot", Cmd::Screenshot),
    action("Capture", "Start / stop recording", Cmd::Record),
    action("Options", "CircuitPython strict", Cmd::Strict),
    action("Options", "Board speed", Cmd::BoardSpeed),
    action("Options", "Servo speed limit", Cmd::ServoSpeed),
    action("Options", "log_mode on the simulated drive", Cmd::LogMode),
    action("Options", "Restart when firmware is saved", Cmd::AutoReload),
    action("View", "Overlay: wing angles", Cmd::Overlay(Overlay::Wings)),
    action("View", "Overlay: eye colours", Cmd::Overlay(Overlay::Eyes)),
    action(
        "View",
        "Overlay: white LED duty",
        Cmd::Overlay(Overlay::Led),
    ),
    action(
        "View",
        "Overlay: accelerometer arrow",
        Cmd::Overlay(Overlay::Accel),
    ),
    action("View", "Animations", Cmd::Animations),
    action("Options", "Use the Lola model", Cmd::Model(true)),
    action("Options", "Use the primitive model", Cmd::Model(false)),
    action("Panel", "", Cmd::TogglePane(Pane::Motion)),
    action("Panel", "", Cmd::TogglePane(Pane::State)),
    action("Panel", "", Cmd::TogglePane(Pane::Pca)),
    action("Panel", "", Cmd::TogglePane(Pane::Board)),
    action("Panel", "", Cmd::TogglePane(Pane::Audio)),
    action("Panel", "", Cmd::TogglePane(Pane::Plot)),
    action("Panel", "", Cmd::TogglePane(Pane::Settings)),
    action("Panel", "", Cmd::TogglePane(Pane::Log)),
    action("Panel", "", Cmd::TogglePane(Pane::Shortcuts)),
    action("Panel", "", Cmd::TogglePane(Pane::Behaviours)),
    action("Panel", "", Cmd::TogglePane(Pane::Scenarios)),
    action("Panel", "Float the focused panel", Cmd::FloatPanel),
    action(
        "Panel",
        "Detach the focused panel to its own window",
        Cmd::DetachPanel,
    ),
    action("Panel", "Close the focused panel", Cmd::ClosePanel),
    action("Layout", "", Cmd::Layout(layout::BUILTINS[0])),
    action("Layout", "", Cmd::Layout(layout::BUILTINS[1])),
    action("Layout", "", Cmd::Layout(layout::BUILTINS[2])),
    action("Layout", "Reset layout", Cmd::Layout(layout::BUILTINS[0])),
    action("Theme", "", Cmd::Theme(ThemeChoice::System)),
    action("Theme", "", Cmd::Theme(ThemeChoice::Light)),
    action("Theme", "", Cmd::Theme(ThemeChoice::Dark)),
    action("Theme", "", Cmd::Theme(ThemeChoice::HighContrast)),
    info("Plot", "Drag", "Pan"),
    info("Plot", "Ctrl+scroll", "Zoom"),
    info("Plot", "Right-drag", "Zoom to the box drawn"),
    info("Plot", "Double-click", "Back to following"),
    info("Keyboard", "Tab / Shift+Tab", "Next / previous control"),
    info("Keyboard", "Arrow keys", "Move between controls"),
    info("Keyboard", "Enter", "Press the focused control"),
    bound(
        "Window",
        "Ctrl+K",
        "Command palette",
        &[(CTRL, egui::Key::K)],
        Cmd::Palette,
    ),
    bound(
        "Window",
        "? or F1",
        "Show or hide this window",
        // bevy_egui maps keys by physical position, so `?` arrives as Slash
        // with Shift held, never as Questionmark. A SHIFT pattern keeps a
        // plain `/` from matching; Questionmark covers any source that
        // reports it logically.
        &[
            (egui::Modifiers::SHIFT, egui::Key::Slash),
            (NO_MODS, egui::Key::Questionmark),
            (NO_MODS, egui::Key::F1),
        ],
        Cmd::ToggleHelp,
    ),
    bound(
        "Window",
        "Esc",
        "Close it",
        &[(NO_MODS, egui::Key::Escape)],
        Cmd::CloseHelp,
    ),
    bound(
        "Window",
        "F11",
        "Focus mode: show only the focused panel (or the 3D view); again to restore",
        &[(NO_MODS, egui::Key::F11)],
        Cmd::Focus,
    ),
    // Shift+F6 first: egui lets a pattern ignore extra Shift, so plain F6
    // would also take Shift+F6 if it were read first.
    bound(
        "Window",
        "Shift+F6",
        "Previous panel",
        &[(egui::Modifiers::SHIFT, egui::Key::F6)],
        Cmd::PrevPanel,
    ),
    bound(
        "Window",
        "F6",
        "Next panel",
        &[(NO_MODS, egui::Key::F6)],
        Cmd::NextPanel,
    ),
    bound(
        "Window",
        "Ctrl+=",
        "Larger UI",
        // Plus covers a layout where = needs Shift; egui ignores extra Shift.
        &[(CTRL, egui::Key::Equals), (CTRL, egui::Key::Plus)],
        Cmd::ScaleUp,
    ),
    bound(
        "Window",
        "Ctrl+-",
        "Smaller UI",
        &[(CTRL, egui::Key::Minus)],
        Cmd::ScaleDown,
    ),
    bound(
        "Window",
        "Ctrl+0",
        "Reset UI scale to 100%",
        &[(CTRL, egui::Key::Num0)],
        Cmd::ScaleReset,
    ),
    action(
        "Window",
        "Dock back: put this window's panel in the main window",
        Cmd::DockBack,
    ),
];

/// Hover text for a control. When `cmd` has a key binding in `SHORTCUTS` the
/// keys are appended, so a tooltip can't disagree with the Shortcuts window.
/// Also shown while the control is disabled, when egui would otherwise show
/// nothing.
fn tip(resp: egui::Response, text: &str, cmd: Option<Cmd>) -> egui::Response {
    let keys = cmd
        .and_then(|c| SHORTCUTS.iter().find(|row| row.cmd == Some(c)))
        .map(|row| row.keys)
        .filter(|keys| !keys.is_empty());
    let text = match keys {
        Some(keys) => format!("{text} ({keys})"),
        None => text.to_owned(),
    };
    resp.on_hover_text(text.clone())
        .on_disabled_hover_text(text)
}

/// What the controls and commands depend on, read fresh whenever it matters.
struct Enabled {
    running: bool,
    paused: bool,
    skipping: bool,
    deploying: bool,
    gltf_available: bool,
    /// The command comes from a detached panel window, not the main one.
    detached_window: bool,
    focus_mode: bool,
    /// The main dock has a focused tab that isn't the 3D view.
    panel_focused: bool,
}

fn read_enabled(
    host: &PyHost,
    deploy: &DeployDlg,
    gltf_available: bool,
    dock: &mut Dock,
    detached_window: bool,
) -> Enabled {
    let (paused, skipping) = {
        let s = hw();
        (s.clock.is_paused(), s.skipping)
    };
    Enabled {
        running: host.is_running(),
        paused,
        skipping,
        deploying: matches!(deploy, DeployDlg::Running(_)),
        gltf_available,
        detached_window,
        focus_mode: dock.focus.is_some(),
        panel_focused: focused_pane(dock).is_some(),
    }
}

/// Whether `cmd` can run now. The toolbar greys controls by this and
/// `dispatch` refuses a disabled one, so a key can't get past it.
fn cmd_enabled(cmd: Cmd, en: &Enabled) -> bool {
    match cmd {
        Cmd::Stop => en.running,
        Cmd::Step | Cmd::StepShort => en.paused,
        Cmd::Speed(_) => !en.skipping,
        Cmd::Deploy => !en.deploying,
        Cmd::Model(true) => en.gltf_available,
        Cmd::Focus => !en.detached_window,
        Cmd::FloatPanel | Cmd::DetachPanel | Cmd::ClosePanel => {
            !en.detached_window && !en.focus_mode && en.panel_focused
        }
        Cmd::DockBack => en.detached_window,
        Cmd::NextPanel | Cmd::PrevPanel => !en.detached_window && !en.focus_mode,
        Cmd::TogglePane(Pane::Viewport) => false,
        _ => true,
    }
}

#[derive(Resource)]
pub struct Host(pub PyHost);

#[derive(Resource)]
pub struct UiState {
    tipped: bool,
    auto_reload: bool,
    code_mtime: Option<SystemTime>,
    shims_mtime: Option<SystemTime>,
    sim_toml_mtime: Option<SystemTime>,
    last_check: Instant,
    history: VecDeque<Sample>,
    markers: VecDeque<Marker>,
    plot: PlotState,
    started: Instant,
    log_filter: String,
    show_firmware: bool,
    show_sim: bool,
    settings: SettingsState,
    /// Names (file stems) of simulator/scenarios/*.toml, re-listed with hot_reload.
    scenarios: Vec<String>,
    scenario_choice: String,
    /// Log text -> traceback location; canonicalize hits the disk, so not per frame.
    traceback_cache: HashMap<String, Option<(PathBuf, u32)>>,
    log_tab: LogTab,
    /// Seq of the newest traceback already announced with a toast.
    seen_traceback: Option<u64>,
    /// Log line a toast click asked to scroll to; cleared once the log has drawn.
    log_jump: Option<u64>,
    /// The line last jumped to and when, so it stays marked for a moment.
    log_highlight: Option<(u64, Instant)>,
    drive_file: DriveFile,
    /// The Shortcuts window; not saved to prefs.
    deploy: DeployDlg,
    /// UI scale (percent) while its slider is being dragged. Zooming the UI
    /// under the cursor mid-drag would move the slider away from it, so the
    /// scale is applied on release.
    scale_drag: Option<f32>,
    /// Inline widths of the toolbar groups when last drawn inline, to decide which collapse.
    group_widths: [f32; TOOLBAR_GROUPS],
    /// Widths of the collapsed groups' menu buttons, and of the right-aligned toolbar area.
    menu_widths: [f32; TOOLBAR_GROUPS],
    right_width: f32,
    /// The Layouts menu's name box, shared by Save and Export.
    layout_name: String,
    /// The exported layout files, re-listed at most once a second while the Import menu is open.
    layout_files: Vec<(String, PathBuf)>,
    layout_files_at: Option<Instant>,
}

/// What the bottom panel shows: the console log, or a file on the simulated drive.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LogTab {
    Console,
    File(&'static str),
}

/// How long a log line stays marked after a toast took the user to it.
const LOG_HIGHLIGHT: Duration = Duration::from_secs(2);

/// What PropMaker_Feather/log.py writes, and what it rolls over to.
const LOG_FILES: [&str; 2] = ["log.txt", "log.old.txt"];
/// log.py's MAX_BYTES: log.txt rolls over past this.
const LOG_MAX_BYTES: u64 = 256 * 1024;

/// A file on the simulated drive (dist/fs), re-read only when it changes.
#[derive(Default)]
struct DriveFile {
    name: &'static str,
    /// Modification time and size when last read; None if the file is absent.
    stamp: Option<(SystemTime, u64)>,
    lines: Vec<String>,
    checked: Option<Instant>,
}

impl DriveFile {
    fn refresh(&mut self, dir: &Path, name: &'static str) {
        let switched = self.name != name;
        if !switched && self.checked.is_some_and(|t| t.elapsed() < DRIVE_POLL) {
            return;
        }
        self.checked = Some(Instant::now());
        let path = dir.join(name);
        let stamp = std::fs::metadata(&path)
            .and_then(|m| Ok((m.modified()?, m.len())))
            .ok();
        if !switched && stamp == self.stamp {
            return;
        }
        self.name = name;
        self.stamp = stamp;
        self.lines = match (stamp, std::fs::read(&path)) {
            (Some(_), Ok(bytes)) => String::from_utf8_lossy(&bytes)
                .lines()
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
    }
}

const DRIVE_POLL: Duration = Duration::from_millis(500);

struct Sample {
    t: f32,
    wings: [Option<f32>; 2],
    main: Vec3,
    small: Vec3,
    white: f32,
    /// What the firmware read from the sensor (after range clipping), in g.
    accel: [f32; 3],
    /// The held z reading, which the firmware treats as "at rest".
    rest_z: f32,
    /// Estimated CircuitPython heap in use, in KB.
    heap_kb: f32,
}

/// An input or event drawn on the plot at the run time it happened.
struct Marker {
    t: f32,
    kind: MarkerKind,
    label: String,
}

impl UiState {
    pub fn new(
        code_mtime: Option<SystemTime>,
        shims_mtime: Option<SystemTime>,
        sim_toml_mtime: Option<SystemTime>,
        prefs: &Prefs,
    ) -> Self {
        Self {
            tipped: false,
            auto_reload: prefs.auto_reload,
            code_mtime,
            shims_mtime,
            sim_toml_mtime,
            last_check: Instant::now(),
            history: VecDeque::new(),
            markers: VecDeque::new(),
            plot: PlotState::new(prefs),
            started: Instant::now(),
            log_filter: String::new(),
            show_firmware: prefs.show_firmware,
            show_sim: prefs.show_sim,
            settings: SettingsState::default(),
            scenarios: Vec::new(),
            scenario_choice: String::new(),
            traceback_cache: HashMap::new(),
            log_tab: LogTab::Console,
            seen_traceback: None,
            log_jump: None,
            log_highlight: None,
            drive_file: DriveFile::default(),
            deploy: DeployDlg::Closed,
            scale_drag: None,
            group_widths: [0.0; TOOLBAR_GROUPS],
            menu_widths: [DEFAULT_MENU_WIDTH; TOOLBAR_GROUPS],
            right_width: 0.0,
            layout_name: String::new(),
            layout_files: Vec::new(),
            layout_files_at: None,
        }
    }
}

/// The control panels, for systems that need the dock's rectangle up to date.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct UiSet;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (record_history, hot_reload, panels.in_set(UiSet)).chain(),
        )
        .add_systems(Last, (stop_on_exit, save_prefs));
    }
}

const SAVE_INTERVAL: Duration = Duration::from_secs(1);

/// Where the pointer sits on a torn-off panel's new window, in screen pixels
/// from its top-left corner: a little way along its title bar.
const TEAR_OFF_GRAB: IVec2 = IVec2::new(80, 12);

#[derive(Default)]
struct SaveState {
    /// When the preferences were last compared with the saved ones.
    last_check: Option<Instant>,
    /// The saved preferences were edited elsewhere since the last write.
    dirty: bool,
    /// Whether the last write failed, so the error is logged once, not every second.
    failed: bool,
    /// The detached windows at the last check, for an exit that finds them gone.
    detached: Vec<DetachedSave>,
}

/// The main window, the detached ones and the dock, which `save_prefs` reads.
type WindowState<'w, 's> = (
    Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    Query<'w, 's, (&'static Detached, &'static Window)>,
    Res<'w, Dock>,
);

type SavedState<'w> = (
    Option<Res<'w, ModelChoice>>,
    Res<'w, RoomLight>,
    Res<'w, AudioMix>,
    Res<'w, ActiveTheme>,
    Res<'w, Overlays>,
);

/// Writes the preferences when they change (checked once a second, so a drag
/// on a panel edge isn't a write per frame, and building the layouts to
/// compare isn't a per-frame cost) and once more on exit. Runs in `Last` so
/// the AppExit event is still readable.
fn save_prefs(
    ui: Res<UiState>,
    (model, room, mix, theme, overlays): SavedState,
    (windows, detached, dock): WindowState,
    mut saved: ResMut<Prefs>,
    paths: Res<crate::Paths>,
    mut state: Local<SaveState>,
    mut exits: EventReader<AppExit>,
) {
    let exiting = exits.read().count() > 0;
    // The Layouts menu edits the saved layouts in place, which a comparison with
    // what was last written would not see, so a change flags a write.
    if saved.is_changed() && !saved.is_added() {
        state.dirty = true;
    }
    if !exiting
        && state
            .last_check
            .is_some_and(|t| t.elapsed() < SAVE_INTERVAL)
    {
        return;
    }
    state.last_check = Some(Instant::now());

    // The window may already be gone on exit, or minimised to nothing.
    let primary = windows
        .single()
        .ok()
        .filter(|w| w.resolution.width() >= MIN_WINDOW && w.resolution.height() >= MIN_WINDOW);
    let (width, height) = primary
        .map(|w| (w.resolution.width(), w.resolution.height()))
        .unwrap_or((saved.window_width, saved.window_height));
    let (window_x, window_y) = match primary.map(|w| w.position) {
        Some(WindowPosition::At(p)) => (Some(p.x), Some(p.y)),
        _ => (saved.window_x, saved.window_y),
    };
    if !windows.is_empty() {
        state.detached = detached
            .iter()
            .map(|(d, window)| dock::detached_save(d.pane, d.scale, window))
            .collect();
    }
    let (strict, board_speed, servo_speed) = {
        let s = hw();
        (s.strict, s.board_speed, s.servo_speed)
    };
    let current = Prefs {
        window_width: width.min(MAX_WINDOW),
        window_height: height.min(MAX_WINDOW),
        window_x,
        window_y,
        model_gltf: model.map(|m| m.use_gltf).or(saved.model_gltf),
        room_light: *room,
        theme: theme.choice,
        ui_scale: theme.scale,
        animations: theme.animations(),
        strict,
        board_speed,
        servo_speed,
        auto_reload: ui.auto_reload,
        overlay_wings: overlays.get(Overlay::Wings),
        overlay_eyes: overlays.get(Overlay::Eyes),
        overlay_led: overlays.get(Overlay::Led),
        overlay_accel: overlays.get(Overlay::Accel),
        volume: mix.volume,
        muted: mix.muted,
        show_firmware: ui.show_firmware,
        show_sim: ui.show_sim,
        plot_window_s: ui.plot.window_s,
        plot_hidden: ui.plot.hidden(),
        layout: Some(dock::snapshot(&dock, state.detached.clone())),
        layouts: saved.layouts.clone(),
    };
    if !state.dirty && current == *saved {
        return;
    }
    match current.save(&paths.prefs) {
        Ok(()) => {
            state.failed = false;
            state.dirty = false;
            *saved = current;
        }
        Err(e) => {
            if !state.failed {
                hw().push_line(format!("[sim] could not save preferences: {e}"));
            }
            state.failed = true;
        }
    }
}

pub(crate) fn servo_angle(cfg: &SimConfig, channel: usize) -> Option<f32> {
    let pulse = hw().pulse_us(channel)?;
    (pulse > 0.0).then(|| cfg.servo.angle_for_pulse(pulse))
}

/// Duty at which an eye channel is at full brightness: set_pulse_width_range(0, 6550)
/// at 50 Hz is a 20 000 us period, so the channel never usefully exceeds 32.75%.
const EYE_PEAK_DUTY: f32 = 6550.0 / 20_000.0;

/// What the "view" column of the PCA9685 grid shows for one channel.
enum ChannelView {
    Angle(Option<f32>),
    Eye(Vec3),
    Blank,
}

/// Picks the readout for a channel. Locks hw() via the helpers, so call it
/// while no hw() guard is held.
fn channel_view(cfg: &SimConfig, ch: usize) -> ChannelView {
    if ch == cfg.wing_right.channel || ch == cfg.wing_left.channel {
        ChannelView::Angle(servo_angle(cfg, ch))
    } else if ch == cfg.eyes.main[0] {
        ChannelView::Eye(glow_rgb(cfg, GlowSource::MainEye))
    } else if ch == cfg.eyes.small[0] {
        ChannelView::Eye(glow_rgb(cfg, GlowSource::SmallEye))
    } else {
        ChannelView::Blank
    }
}

/// Colour swatch of an eye's duty-cycle RGB, scaled so the peak duty is full
/// brightness (the raw duty tops out near a third, which would look dim).
fn eye_swatch(ui: &mut egui::Ui, duty_rgb: Vec3, outline: Color32) -> egui::Response {
    let scaled = eye_scaled(duty_rgb);
    // egui::Rgba is linear; converting to Color32 applies the sRGB gamma.
    let colour = Color32::from(egui::Rgba::from_rgb(scaled.x, scaled.y, scaled.z));
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, colour);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, outline),
        egui::StrokeKind::Inside,
    );
    response.on_hover_text(eye_rgb_text(duty_rgb))
}

/// An eye's duty-cycle RGB as a fraction of the peak duty.
fn eye_scaled(duty_rgb: Vec3) -> Vec3 {
    (duty_rgb / EYE_PEAK_DUTY).clamp(Vec3::ZERO, Vec3::ONE)
}

/// The eye readout shared by the PCA9685 panel's swatch tooltip and the 3D
/// view's overlay.
pub(crate) fn eye_rgb_text(duty_rgb: Vec3) -> String {
    let scaled = eye_scaled(duty_rgb);
    format!(
        "R {:.0}%  G {:.0}%  B {:.0}% of peak",
        scaled.x * 100.0,
        scaled.y * 100.0,
        scaled.z * 100.0
    )
}

/// The white LED readout shared by the PCA9685 panel and the 3D overlay.
pub(crate) fn white_led_text(duty: u16) -> String {
    format!("White LED duty {:.1}%", duty as f32 / 655.35)
}

fn record_history(cfg: Res<SimConfig>, mut ui: ResMut<UiState>) {
    // A frozen clock means flat lines; recording would just scroll the real
    // history out of the plot.
    if hw().clock.is_paused() {
        return;
    }
    let t = ui.started.elapsed().as_secs_f32();
    // Markers raised while paused wait until now, which is also when the
    // firmware gets to act on them.
    let (accel, rest_z, heap_kb, new_markers) = {
        let mut s = hw();
        let heap_kb = (s.heap_bytes / 1024.0) as f32;
        (
            s.last_accel,
            s.accel_g[2],
            heap_kb,
            std::mem::take(&mut s.markers),
        )
    };
    ui.markers.extend(
        new_markers
            .into_iter()
            .map(|(kind, label)| Marker { t, kind, label }),
    );
    while ui
        .markers
        .front()
        .is_some_and(|m| t - m.t > HISTORY_SECONDS)
    {
        ui.markers.pop_front();
    }
    let sample = Sample {
        t,
        wings: [
            servo_angle(&cfg, cfg.wing_right.channel),
            servo_angle(&cfg, cfg.wing_left.channel),
        ],
        main: glow_rgb(&cfg, GlowSource::MainEye),
        small: glow_rgb(&cfg, GlowSource::SmallEye),
        white: glow_rgb(&cfg, GlowSource::WhiteLed).x,
        accel,
        rest_z,
        heap_kb,
    };
    ui.history.push_back(sample);
    while ui
        .history
        .front()
        .is_some_and(|s| t - s.t > HISTORY_SECONDS)
    {
        ui.history.pop_front();
    }
}

/// Sorted file stems of the `.toml` files in `dir`; empty if it doesn't exist.
fn list_scenarios(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .filter_map(|p| Some(p.file_stem()?.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

/// Restarts code.py with `simulator/scenarios/<name>.toml` played against it.
fn run_scenario(host: &mut PyHost, cfg: &SimConfig, paths: &crate::Paths, name: &str) {
    let path = paths
        .simulator
        .join("scenarios")
        .join(format!("{name}.toml"));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            hw().push_line(format!("[sim] could not read {}: {e}", path.display()));
            return;
        }
    };
    let [r, g, b] = cfg.eyes.main;
    let [sr, sg, sb] = cfg.eyes.small;
    let channels = [
        ("right_wing", cfg.wing_right.channel),
        ("left_wing", cfg.wing_left.channel),
        ("main_r", r),
        ("main_g", g),
        ("main_b", b),
        ("small_r", sr),
        ("small_g", sg),
        ("small_b", sb),
    ];
    host.start_scenario(ScenarioRun {
        name: name.to_owned(),
        text,
        servo: (
            cfg.servo.min_pulse_us,
            cfg.servo.max_pulse_us,
            cfg.servo.range_deg,
        ),
        channels: channels.map(|(n, c)| (n.to_owned(), c)).to_vec(),
    });
}

fn hot_reload(
    mut ui: ResMut<UiState>,
    mut host: ResMut<Host>,
    mut cfg: ResMut<SimConfig>,
    paths: Res<crate::Paths>,
    mut toasts: ResMut<Toasts>,
) {
    if ui.last_check.elapsed().as_millis() < 500 {
        return;
    }
    ui.last_check = Instant::now();
    reload_sim_toml(&mut ui, &mut cfg, &paths.simulator.join("sim.toml"));
    ui.scenarios = list_scenarios(&paths.simulator.join("scenarios"));
    if let Some(dir) = paths.code.parent() {
        ui.settings.refresh(dir);
    }
    let mtime = firmware_mtime(host.0.code_path());
    let shims = shims_mtime(host.0.shim_dir());
    let changed = if mtime != ui.code_mtime {
        Some("firmware")
    } else if shims != ui.shims_mtime {
        Some("shims")
    } else {
        None
    };
    ui.code_mtime = mtime;
    ui.shims_mtime = shims;
    if let Some(what) = changed {
        if ui.auto_reload {
            hw().push_line(format!("[sim] {what} changed - restarting"));
            host.0.start();
            let name = if what == "shims" { "Shims" } else { "Firmware" };
            toasts.push(ToastKind::Info, format!("{name} reloaded"), None);
        }
    }
}

/// Applies a saved sim.toml without restarting code.py: everything reads
/// `SimConfig` and `hw().board` live. An invalid file keeps the old config.
/// Not gated on Auto-reload, which is about restarting the firmware.
fn reload_sim_toml(ui: &mut UiState, cfg: &mut SimConfig, path: &Path) {
    let mtime = file_mtime(path);
    if mtime == ui.sim_toml_mtime {
        return;
    }
    ui.sim_toml_mtime = mtime;
    let line = match SimConfig::read(path) {
        Ok(Some(new)) => {
            hw().board = new.board.clone();
            *cfg = new;
            "[sim] sim.toml reloaded".to_owned()
        }
        Ok(None) => "[sim] sim.toml is missing, keeping the previous config".to_owned(),
        Err(e) => format!("[sim] sim.toml is invalid, keeping the previous config: {e}"),
    };
    hw().push_line(line);
}

fn stop_on_exit(mut exits: EventReader<AppExit>, mut host: ResMut<Host>) {
    if exits.read().next().is_some() {
        host.0.stop();
    }
}

/// Where saved files go, the camera/screenshot requests, the dock and the
/// detached windows; bundled to keep `panels` within clippy's argument limit.
#[derive(SystemParam)]
struct Captures<'w, 's> {
    paths: Res<'w, crate::Paths>,
    controls: ResMut<'w, ViewControls>,
    diagnostics: Res<'w, DiagnosticsStore>,
    toasts: ResMut<'w, Toasts>,
    dock: ResMut<'w, Dock>,
    viewport: ResMut<'w, ViewportRect>,
    detached: Query<'w, 's, (Entity, &'static mut Detached)>,
    windows: Query<'w, 's, &'static Window, With<Detached>>,
    prefs: ResMut<'w, Prefs>,
    commands: Commands<'w, 's>,
}

impl Captures<'_, '_> {
    /// The dock and detached windows as they are now, for saving as a layout.
    fn current_layout(&self) -> Layout {
        let open = self
            .detached
            .iter()
            .filter_map(|(entity, d)| {
                let window = self.windows.get(entity).ok()?;
                Some(dock::detached_save(d.pane, d.scale, window))
            })
            .collect();
        dock::snapshot(&self.dock, open)
    }
}

const TOOLBAR_GROUPS: usize = 3;
const GROUP_RUN: usize = 0;
const GROUP_SPEED: usize = 1;
const GROUP_OPTIONS: usize = 2;
/// Which toolbar group collapses into a menu first when the window is narrow.
/// Run is never listed, so it always stays inline.
const COLLAPSE_ORDER: [usize; 2] = [GROUP_OPTIONS, GROUP_SPEED];
/// Menu-button width assumed until a collapsed group has been drawn once.
const DEFAULT_MENU_WIDTH: f32 = 80.0;
/// Room kept for the More button.
const MORE_WIDTH: f32 = 70.0;

/// Decides, from last frame's measured widths, which toolbar groups are drawn
/// as a menu button instead of inline, and records the widths drawn this frame.
struct GroupLayout {
    inline: [f32; TOOLBAR_GROUPS],
    menu: [f32; TOOLBAR_GROUPS],
    collapsed: [bool; TOOLBAR_GROUPS],
    /// Folded groups that still don't fit, moved into one "More" menu.
    in_more: [bool; TOOLBAR_GROUPS],
    /// Set while the More menu's contents are being drawn.
    more_pass: bool,
}

impl GroupLayout {
    fn new(
        inline: [f32; TOOLBAR_GROUPS],
        menu: [f32; TOOLBAR_GROUPS],
        avail: f32,
        gap: f32,
    ) -> Self {
        let mut collapsed = [false; TOOLBAR_GROUPS];
        let mut in_more = [false; TOOLBAR_GROUPS];
        let need = |collapsed: &[bool; TOOLBAR_GROUPS], in_more: &[bool; TOOLBAR_GROUPS]| {
            let groups: f32 = (0..TOOLBAR_GROUPS)
                .filter(|g| !in_more[*g])
                .map(|g| if collapsed[g] { menu[g] } else { inline[g] })
                .sum();
            let more = if in_more.contains(&true) {
                MORE_WIDTH
            } else {
                0.0
            };
            groups + more + gap * TOOLBAR_GROUPS as f32
        };
        // Until every group has been measured inline there is nothing to compare, so draw them all.
        if inline.iter().all(|w| *w > 0.0) {
            for group in COLLAPSE_ORDER {
                if need(&collapsed, &in_more) <= avail {
                    break;
                }
                collapsed[group] = true;
            }
            for group in COLLAPSE_ORDER {
                if need(&collapsed, &in_more) <= avail {
                    break;
                }
                in_more[group] = true;
            }
        }
        Self {
            inline,
            menu,
            collapsed,
            in_more,
            more_pass: false,
        }
    }

    fn crowded(&self) -> bool {
        self.in_more.contains(&true)
    }

    /// Draws one group inline, or as a menu button holding the same controls.
    /// Each pass (toolbar row, More menu) draws only its own groups.
    fn show(
        &mut self,
        row: &mut egui::Ui,
        group: usize,
        label: &str,
        hint: &str,
        body: impl FnOnce(&mut egui::Ui),
    ) {
        if self.in_more[group] != self.more_pass {
            return;
        }
        if group != GROUP_RUN && !self.more_pass {
            row.separator();
        }
        if self.collapsed[group] {
            let button = tip(row.menu_button(label, body).response, hint, None);
            // Inside the More menu the button is menu-wide, not its toolbar width.
            if !self.more_pass {
                self.menu[group] = button.rect.width();
            }
        } else {
            // Only measured while inline, so the collapse decision doesn't flicker.
            self.inline[group] = row.scope(body).response.rect.width();
        }
    }
}

fn run_group(row: &mut egui::Ui, en: &Enabled, cmds: &mut Vec<Cmd>) {
    if tip(
        row.button(if en.running {
            "🔄 Restart"
        } else {
            "▶ Run"
        }),
        "Run code.py from the start",
        Some(Cmd::Restart),
    )
    .clicked()
    {
        cmds.push(Cmd::Restart);
    }
    if tip(
        row.add_enabled(cmd_enabled(Cmd::Stop, en), egui::Button::new("⏹ Stop")),
        "Stop code.py",
        None,
    )
    .clicked()
    {
        cmds.push(Cmd::Stop);
    }
    if tip(
        row.button(if en.paused { "▶ Resume" } else { "⏸ Pause" }),
        "Freeze code.py's clock",
        Some(Cmd::Pause),
    )
    .clicked()
    {
        cmds.push(Cmd::Pause);
    }
    if tip(
        row.add_enabled(cmd_enabled(Cmd::StepShort, en), egui::Button::new("+10 ms")),
        "Run the paused clock for 10 ms",
        None,
    )
    .clicked()
    {
        cmds.push(Cmd::StepShort);
    }
    if tip(
        row.add_enabled(cmd_enabled(Cmd::Step, en), egui::Button::new("+100 ms")),
        "Run the paused clock for 100 ms",
        Some(Cmd::Step),
    )
    .clicked()
    {
        cmds.push(Cmd::Step);
    }
    if tip(
        row.button(if en.skipping {
            "⏹ Stop skip"
        } else {
            "⏩ Skip"
        }),
        "Run flat out until a behaviour starts (a sound plays or a hardware.py event changes), then drop back to 1x",
        Some(Cmd::Skip),
    )
    .clicked()
    {
        cmds.push(Cmd::Skip);
    }
    if tip(
        row.button("Trigger idle"),
        "Ends the idle wait so attention_animation or trench_run runs",
        Some(Cmd::Idle),
    )
    .clicked()
    {
        cmds.push(Cmd::Idle);
    }
}

fn speed_group(row: &mut egui::Ui, en: &Enabled, cmds: &mut Vec<Cmd>) {
    row.label("Speed");
    let mut speed = hw().clock.speed;
    let can_set = cmd_enabled(Cmd::Speed(0), en);
    // Read back every frame so the slider follows presets and the end of a skip.
    let slider = row.add_enabled(
        can_set,
        egui::Slider::new(&mut speed, 1.0..=200.0)
            .logarithmic(true)
            .suffix("x"),
    );
    if tip(
        slider,
        "How fast code.py's clock runs against real time (1x to 200x)",
        None,
    )
    .changed()
        && can_set
    {
        hw().set_speed(speed);
    }
    row.horizontal(|presets| {
        for (i, preset) in SPEED_PRESETS.iter().enumerate() {
            if tip(
                presets.add_enabled(
                    cmd_enabled(Cmd::Speed(i), en),
                    egui::Button::new(format!("{preset}x")),
                ),
                &format!("Run at {preset}x"),
                None,
            )
            .clicked()
            {
                cmds.push(Cmd::Speed(i));
            }
        }
    });
}

/// The mini-toolbar floating over the 3D view: camera views, room light,
/// capture and overlays. Pushes commands like every other control, and greys
/// them by the same `cmd_enabled` the main toolbar uses.
fn viewport_bar(
    ui: &mut egui::Ui,
    en: &Enabled,
    room: RoomLight,
    recording: bool,
    overlays: &Overlays,
    cmds: &mut Vec<Cmd>,
) {
    fn button(
        ui: &mut egui::Ui,
        en: &Enabled,
        cmds: &mut Vec<Cmd>,
        label: &str,
        hint: &str,
        cmd: Cmd,
    ) {
        let resp = ui.add_enabled(cmd_enabled(cmd, en), egui::Button::new(label));
        if tip(resp, hint, Some(cmd)).clicked() {
            cmds.push(cmd);
        }
    }
    ui.horizontal_wrapped(|ui| {
        for (label, view) in [
            ("Front", CameraView::Front),
            ("Side", CameraView::Side),
            ("Top", CameraView::Top),
        ] {
            button(
                ui,
                en,
                cmds,
                label,
                &format!("Look at the droid from the {}", label.to_lowercase()),
                Cmd::Camera(view),
            );
        }
        button(
            ui,
            en,
            cmds,
            "Eyes",
            "Close-up of the eyes",
            Cmd::Camera(CameraView::Eyes),
        );
        button(
            ui,
            en,
            cmds,
            "Reset",
            "Back to the starting view",
            Cmd::Camera(CameraView::Reset),
        );
        ui.separator();
        let light = ui.menu_button("Light", |m| {
            for choice in RoomLight::ALL {
                let cmd = Cmd::Light(choice);
                let resp = m.add_enabled(
                    cmd_enabled(cmd, en),
                    egui::SelectableLabel::new(room == choice, choice.label()),
                );
                if tip(
                    resp,
                    &format!("{} room lighting", choice.label()),
                    Some(cmd),
                )
                .clicked()
                {
                    cmds.push(cmd);
                    m.close_menu();
                }
            }
        });
        tip(
            light.response,
            "Room lighting, to judge how bright the eyes and white LED look in a dim room",
            None,
        );
        ui.separator();
        button(
            ui,
            en,
            cmds,
            "📷 Screenshot",
            "Save a PNG of the window to the captures folder next to the exe",
            Cmd::Screenshot,
        );
        button(
            ui,
            en,
            cmds,
            if recording {
                "⏹ Stop recording"
            } else {
                "⏺ Record"
            },
            "Save a PNG every 0.1 s, up to 30 s, for making a GIF",
            Cmd::Record,
        );
        ui.separator();
        let menu = ui.menu_button("Overlays", |m| {
            for overlay in Overlay::ALL {
                let cmd = Cmd::Overlay(overlay);
                let mut on = overlays.get(overlay);
                let resp = m.add_enabled(
                    cmd_enabled(cmd, en),
                    egui::Checkbox::new(&mut on, capitalised(overlay.label())),
                );
                if tip(resp, "Show this readout over the droid", Some(cmd)).changed() {
                    cmds.push(cmd);
                }
            }
        });
        tip(menu.response, "Readouts drawn over the droid", None);
    });
}

/// `text` with its first letter upper-cased, for menu entries.
fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `model` is whether the Lola glTF model is in use, or None where there is no choice to make.
fn options_group(
    row: &mut egui::Ui,
    en: &Enabled,
    auto_reload: bool,
    model: Option<bool>,
    fs_dir: &Path,
    cmds: &mut Vec<Cmd>,
) {
    let mut strict = hw().strict;
    if tip(
        row.checkbox(&mut strict, "CircuitPython strict"),
        "Only CircuitPython 9 modules and APIs, and its float precision for time.monotonic(). Applies on restart.",
        None,
    )
    .changed()
    {
        cmds.push(Cmd::Strict);
    }
    let mut board_speed = hw().board_speed;
    if tip(
        row.checkbox(&mut board_speed, "Board speed"),
        "Each servo/LED write, accelerometer read and scheduler pass takes as long as on the robot ([board] in sim.toml).",
        None,
    )
    .changed()
    {
        cmds.push(Cmd::BoardSpeed);
    }
    let mut servo_speed = hw().servo_speed;
    if tip(
        row.checkbox(&mut servo_speed, "Servo speed limit"),
        "The 3D wings turn no faster than the real servo (sec_per_60deg in sim.toml [servo]). The log warns about moves it can't follow either way.",
        None,
    )
    .changed()
    {
        cmds.push(Cmd::ServoSpeed);
    }
    log_mode_checkbox(row, fs_dir, cmds);
    let mut reload = auto_reload;
    if tip(
        row.checkbox(&mut reload, "Restart when firmware is saved"),
        "Also when a shim in dist\\shims\\ is added, saved or deleted.",
        None,
    )
    .changed()
    {
        cmds.push(Cmd::AutoReload);
    }
    if let Some(use_gltf) = model {
        row.separator();
        row.horizontal(|picker| {
            picker.label("Model");
            let mut picked = use_gltf;
            let gltf_available = cmd_enabled(Cmd::Model(true), en);
            picker.add_enabled_ui(gltf_available, |r| {
                let lola = r.selectable_value(&mut picked, true, "Lola");
                if gltf_available {
                    lola.on_hover_text("The droid model from assets/droid.glb");
                } else {
                    lola.on_disabled_hover_text(
                        "Build simulator/assets/droid.glb first (see README)",
                    );
                }
            });
            picker
                .selectable_value(&mut picked, false, "Primitive")
                .on_hover_text("The simple built-in shapes");
            // Only on a real change, so the model isn't rebuilt every frame.
            if picked != use_gltf {
                cmds.push(Cmd::Model(picked));
            }
        });
    }
}

/// The toolbar's Deploy button, kept on the right next to View rather than in
/// the Options menu, so it's one click. `short` drops the text when crowded.
fn deploy_button(row: &mut egui::Ui, en: &Enabled, cmds: &mut Vec<Cmd>, short: bool) {
    let label = if short { "\u{2b06}" } else { "\u{2b06} Deploy" };
    if tip(
        row.add_enabled(cmd_enabled(Cmd::Deploy, en), egui::Button::new(label)),
        "Deploy to CIRCUITPY: copy the changed firmware files to the board (never lib/ or boot_out.txt)",
        None,
    )
    .clicked()
    {
        cmds.push(Cmd::Deploy);
    }
}

fn panels(
    mut contexts: EguiContexts,
    mut ui: ResMut<UiState>,
    mut host: ResMut<Host>,
    mut model: Option<ResMut<ModelChoice>>,
    mut room: ResMut<RoomLight>,
    (cfg, mut audio, mut theme, mut overlays): (
        Res<SimConfig>,
        AudioUi,
        ResMut<ActiveTheme>,
        ResMut<Overlays>,
    ),
    mut captures: Captures,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    // Cloned (it is a handle) so `contexts` is free to hand out the detached windows' ones.
    let ctx = &egui::Context::clone(ctx);
    let palette = theme.palette;
    let scale = theme.scale;

    // Cloned so the lock is gone before anything else locks it.
    let traceback = hw().last_traceback.clone();
    if let Some((seq, msg)) = traceback {
        if ui.seen_traceback != Some(seq) {
            ui.seen_traceback = Some(seq);
            captures
                .toasts
                .push(ToastKind::Error, format!("Traceback: {msg}"), Some(seq));
        }
    }

    // Panes in their own windows, as (window, pane): the key handling, the
    // View menu and the Shortcuts toggle all need to know where a pane lives.
    let detached: Vec<(Entity, Pane)> = captures
        .detached
        .iter()
        .map(|(entity, d)| (entity, d.pane))
        .collect();
    let help_open = pane_shown(&captures.dock, &detached, Pane::Shortcuts);

    // Keys and controls only queue commands; `dispatch` runs them all once
    // the window is drawn.
    let mut cmds = read_cmds(ctx, help_open);
    let gltf_available = model.as_ref().is_some_and(|m| m.gltf_available);
    let en = read_enabled(
        &host.0,
        &ui.deploy,
        gltf_available,
        &mut captures.dock,
        false,
    );

    egui::TopBottomPanel::top("toolbar").show(ctx, |bar| {
        bar.horizontal(|row| {
            let gap = row.spacing().item_spacing.x * 3.0;
            let avail = row.available_width() - ui.right_width - gap;
            let mut groups = GroupLayout::new(ui.group_widths, ui.menu_widths, avail, gap);
            groups.show(
                row,
                GROUP_RUN,
                "▶ Run",
                "Run, stop, pause and step code.py",
                |g| run_group(g, &en, &mut cmds),
            );
            let crowded = groups.crowded();
            let mut rest = |row: &mut egui::Ui, groups: &mut GroupLayout| {
                groups.show(
                    row,
                    GROUP_SPEED,
                    "⏩ Speed",
                    "How fast code.py's clock runs",
                    |g| speed_group(g, &en, &mut cmds),
                );
                groups.show(
                    row,
                    GROUP_OPTIONS,
                    "Options",
                    "How faithfully the simulator copies the board, model choice and deploy",
                    |g| {
                        options_group(
                            g,
                            &en,
                            ui.auto_reload,
                            model.as_ref().map(|m| m.use_gltf),
                            &captures.paths.fs,
                            &mut cmds,
                        )
                    },
                );
            };
            rest(row, &mut groups);
            if crowded {
                row.separator();
                let more = row.menu_button("More", |m| {
                    groups.more_pass = true;
                    rest(m, &mut groups);
                    groups.more_pass = false;
                });
                tip(more.response, "The groups that don't fit this window", None);
            }
            ui.group_widths = groups.inline;
            ui.menu_widths = groups.menu;

            let right = row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r| {
                r.scope(|r| {
                    if tip(
                        r.button(if crowded { "⌨" } else { "⌨ Shortcuts" }),
                        "List every key and mouse control",
                        Some(Cmd::ToggleHelp),
                    )
                    .clicked()
                    {
                        cmds.push(Cmd::ToggleHelp);
                    }
                    let menu = r.menu_button(if crowded { "⚙" } else { "⚙ View" }, |m| {
                        if tip(
                            m.button("Command palette…"),
                            "Search and run any command",
                            Some(Cmd::Palette),
                        )
                        .clicked()
                        {
                            cmds.push(Cmd::Palette);
                            m.close_menu();
                        }
                        m.separator();
                        panels_menu(m, &captures.dock, &detached, &en, &mut cmds);
                        layouts_menu(
                            m,
                            &mut ui,
                            &mut captures,
                            &detached,
                            to_color(palette.panel),
                            &mut cmds,
                        );
                        m.separator();
                        appearance_controls(
                            m,
                            theme.choice,
                            theme.scale,
                            &mut ui.scale_drag,
                            theme.animations(),
                            &mut cmds,
                        )
                    });
                    tip(
                        menu.response,
                        "Panels, focus mode, theme and UI scale",
                        None,
                    );
                    // Written only on a real change so the apply system's change check stays quiet.
                    if let Some(Some(s)) = menu.inner {
                        theme.set_scale(s);
                    }
                    // Right to left, so this lands just left of View.
                    deploy_button(r, &en, &mut cmds, crowded);
                })
                .response
                .rect
                .width()
            });
            // Kept at its full width, so dropping the text can't flip the layout back.
            if !crowded {
                ui.right_width = right.inner;
            }
        });
    });

    // Declared before the dock so it spans the whole window's
    // bottom edge; egui places earlier panels outermost.
    let fps = captures
        .diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let status = egui::TopBottomPanel::bottom("status")
        .resizable(false)
        .show(ctx, |bar| {
            bar.horizontal(|row| {
                status_bar(
                    row,
                    host.0.is_running(),
                    fps,
                    captures.dock.focus.is_some(),
                    palette,
                )
            });
        });
    let status_height = status.response.rect.height();

    // Cleared first: the dock sets them only if the 3D view's tab is showing.
    captures.viewport.rect = None;
    captures.viewport.hovered = false;
    // The sound list is only re-scanned while the Audio tab is showing.
    audio.sounds.wanted = false;
    let mut cx = PaneCx {
        ui: &mut ui,
        host: &mut host,
        audio: &mut audio,
        cfg: &cfg,
        paths: &captures.paths,
        palette,
    };
    let mut actions = Vec::new();
    let now = ctx.input(|i| i.time);
    let animate = theme.animations();
    // A copy, so the viewer doesn't hold the dock resource while the dock is drawn.
    let fades = captures.dock.fades.clone();
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |area| {
            let style = dock::dock_style(area.style().as_ref(), palette);
            let bounds = area.max_rect();
            let recording = captures.controls.is_recording();
            let mut bar = |bar_ui: &mut egui::Ui| {
                egui::Frame::NONE
                    .fill(palette.panel)
                    .stroke(Stroke::new(palette.stroke_width, palette.border))
                    .corner_radius(palette.rounding)
                    .inner_margin(4.0)
                    .show(bar_ui, |f| {
                        viewport_bar(f, &en, *room, recording, &overlays, &mut cmds)
                    });
            };
            let mut viewer = Viewer {
                draw: |pane_ui, pane| draw_pane(pane_ui, pane, &mut cx),
                viewport: &mut captures.viewport,
                actions: &mut actions,
                bar: &mut bar,
                fades: &fades,
                now,
                animate,
            };
            if let Some(focus) = captures.dock.focus.as_mut() {
                egui_dock::DockArea::new(focus)
                    .id(egui::Id::new("dock_focus"))
                    .style(style)
                    .show_close_buttons(false)
                    .tab_context_menus(false)
                    .draggable_tabs(false)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_add_buttons(false)
                    .show_inside(area, &mut viewer);
            } else {
                egui_dock::DockArea::new(&mut captures.dock.state)
                    .id(dock::main_dock_id())
                    .style(style)
                    .show_add_buttons(false)
                    .window_bounds(bounds)
                    .show_inside(area, &mut viewer);
            }
        });
    if captures.dock.focus.is_none() {
        captures.dock.floats = dock::read_floats(ctx, &captures.dock.state);
    }
    // Tab close buttons ask for a fade-out; the pane goes once it is done.
    for (pane, _) in actions.iter().filter(|(_, a)| *a == PaneAction::Close) {
        set_pane_shown(
            &mut captures.dock,
            &detached,
            &mut captures.commands,
            *pane,
            false,
            Some(now),
        );
    }
    for pane in captures.dock.fades.settle(now) {
        dock::set_shown(&mut captures.dock.state, pane, false);
    }
    if !captures.dock.fades.is_empty() {
        ctx.request_repaint();
    }

    // Run after the dock is drawn: a command can edit the layout it was drawn from.
    // A floated panel opens near the middle; egui_dock sizes the window at 0.8 of this.
    let float_rect = egui::Rect::from_min_size(
        ctx.screen_rect().center() - egui::vec2(210.0, 160.0),
        egui::vec2(525.0, 400.0),
    );
    {
        let mut cmd_cx = CmdCx {
            ctx,
            host: &mut *cx.host,
            ui: &mut *cx.ui,
            dock: &mut captures.dock,
            commands: &mut captures.commands,
            controls: &mut captures.controls,
            toasts: &mut captures.toasts,
            prefs: captures.prefs.reborrow(),
            paths: &captures.paths,
            room: room.reborrow(),
            overlays: overlays.reborrow(),
            model: model.as_mut().map(|m| m.reborrow()),
            theme: theme.reborrow(),
            detached: &detached,
            float_rect,
        };
        palette_step(&mut cmd_cx, false, &mut cmds, palette);
        dispatch(cmds, &mut cmd_cx, Scope::Main);
    }

    // Applied after the dock is drawn: the tab's menu can't edit the layout
    // it is being drawn from.
    let dragging = captures
        .dock
        .focus
        .is_none()
        .then(|| dock::dragged_tab(&captures.dock.state, ctx))
        .flatten();
    // A tab let go outside the window opens in its own window under the
    // pointer, with its title bar where the pointer is.
    let torn = captures.dock.tear.update(dragging, ctx).map(|pane| {
        let mut place = DetachedSave::new(pane, scale);
        if let Some(at) = dock::cursor_screen_pos() {
            place.x = Some(at.x.saturating_sub(TEAR_OFF_GRAB.x));
            place.y = Some(at.y.saturating_sub(TEAR_OFF_GRAB.y));
        }
        place
    });
    let detaches = actions
        .iter()
        .filter(|(_, a)| *a == PaneAction::Detach)
        .map(|(pane, _)| (DetachedSave::new(*pane, scale), false))
        .chain(torn.map(|place| (place, true)));
    for (place, under_pointer) in detaches {
        detach_pane(
            &mut captures.dock,
            &mut captures.commands,
            &place,
            under_pointer,
            to_color(palette.panel),
        );
    }

    for (entity, mut window) in &mut captures.detached {
        // The window's context appears a frame after it is spawned.
        let Some(wctx) = contexts
            .try_ctx_for_entity_mut(entity)
            .map(|c| egui::Context::clone(c))
        else {
            continue;
        };
        let pane = window.pane;
        let mut wcmds = read_cmds(&wctx, help_open);
        egui::TopBottomPanel::top("detached_bar").show(&wctx, |bar| {
            bar.horizontal(|row| {
                row.strong(pane.title());
                row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |r| {
                    if tip(
                        r.button("Dock back"),
                        "Put this panel back in the main window",
                        None,
                    )
                    .clicked()
                    {
                        wcmds.push(Cmd::DockBack);
                    }
                    r.separator();
                    // Laid out right to left, so + comes first.
                    if tip(
                        r.small_button("+"),
                        "Larger UI in this window",
                        Some(Cmd::ScaleUp),
                    )
                    .clicked()
                    {
                        wcmds.push(Cmd::ScaleUp);
                    }
                    if tip(
                        r.small_button(format!("{:.0}%", window.scale * 100.0)),
                        "This window's UI scale; click to reset to 100%",
                        Some(Cmd::ScaleReset),
                    )
                    .clicked()
                    {
                        wcmds.push(Cmd::ScaleReset);
                    }
                    if tip(
                        r.small_button("−"),
                        "Smaller UI in this window",
                        Some(Cmd::ScaleDown),
                    )
                    .clicked()
                    {
                        wcmds.push(Cmd::ScaleDown);
                    }
                });
            });
        });
        egui::CentralPanel::default().show(&wctx, |body| {
            // The log and settings scroll themselves; the plot only up and down,
            // as it fits its width and pans sideways by dragging.
            if matches!(pane, Pane::Log | Pane::Settings) {
                draw_pane(body, pane, &mut cx);
            } else if pane == Pane::Plot {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(body, |scroll| draw_pane(scroll, pane, &mut cx));
            } else {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(body, |scroll| draw_pane(scroll, pane, &mut cx));
            }
        });
        let mut cmd_cx = CmdCx {
            ctx: &wctx,
            host: &mut *cx.host,
            ui: &mut *cx.ui,
            dock: &mut captures.dock,
            commands: &mut captures.commands,
            controls: &mut captures.controls,
            toasts: &mut captures.toasts,
            prefs: captures.prefs.reborrow(),
            paths: &captures.paths,
            room: room.reborrow(),
            overlays: overlays.reborrow(),
            model: model.as_mut().map(|m| m.reborrow()),
            theme: theme.reborrow(),
            detached: &detached,
            float_rect,
        };
        palette_step(&mut cmd_cx, true, &mut wcmds, palette);
        let scope = Scope::Window {
            entity,
            pane,
            scale: &mut window.scale,
        };
        dispatch(wcmds, &mut cmd_cx, scope);
        theme::focus_ring(&wctx, palette);
    }

    let fw_dir = captures.paths.code.parent().unwrap_or(Path::new("."));
    deploy_window(ctx, &mut ui.deploy, fw_dir);

    if let Some(seq) = captures.toasts.show(ctx, palette, status_height) {
        let filter = ui.log_filter.to_lowercase();
        // Looked up first: the lock must not be held while the filter is edited.
        let hidden = hw()
            .log
            .iter()
            .find(|l| l.seq == seq)
            .is_some_and(|l| !line_visible(l, true, ui.show_sim, &filter));
        ui.log_tab = LogTab::Console;
        ui.show_firmware = true;
        if hidden {
            ui.log_filter.clear();
        }
        ui.log_jump = Some(seq);
    }
    theme::focus_ring(ctx, palette);
}

/// Everything a command can touch, bundled for clippy's argument limit. Built
/// once per window (the main one and each detached one).
struct CmdCx<'a, 'w, 's> {
    /// The window the command came from.
    ctx: &'a egui::Context,
    host: &'a mut Host,
    ui: &'a mut UiState,
    dock: &'a mut Dock,
    commands: &'a mut Commands<'w, 's>,
    controls: &'a mut ViewControls,
    toasts: &'a mut Toasts,
    // The resources the scene or the saved prefs react to when written are
    // reborrowed as `Mut`, so they are only marked changed on a real change.
    prefs: Mut<'a, Prefs>,
    paths: &'a crate::Paths,
    room: Mut<'a, RoomLight>,
    overlays: Mut<'a, Overlays>,
    model: Option<Mut<'a, ModelChoice>>,
    theme: Mut<'a, ActiveTheme>,
    detached: &'a [(Entity, Pane)],
    /// Where a panel floated from the palette or a key opens.
    float_rect: egui::Rect,
}

/// Which window a command came from. Focus mode, the dock and the UI scale
/// are per window, so the same command means something different in each.
enum Scope<'a> {
    Main,
    Window {
        entity: Entity,
        pane: Pane,
        scale: &'a mut f32,
    },
}

/// What a palette row runs: a command, or a saved or exported layout.
#[derive(Clone)]
enum PaletteItem {
    Cmd(Cmd),
    Layout(LayoutAction),
}

impl CmdCx<'_, '_, '_> {
    fn enabled(&mut self, detached_window: bool) -> Enabled {
        let gltf_available = self.model.as_ref().is_some_and(|m| m.gltf_available);
        read_enabled(
            &self.host.0,
            &self.ui.deploy,
            gltf_available,
            self.dock,
            detached_window,
        )
    }

    /// The time to start a panel fade at, or `None` with animations off (the
    /// panel then opens or closes at once).
    fn fade_now(&self) -> Option<f64> {
        self.theme.animations().then(|| self.ctx.input(|i| i.time))
    }

    fn apply_layout(&mut self, action: LayoutAction) {
        let mut lcx = LayoutCx {
            dock: &mut *self.dock,
            commands: &mut *self.commands,
            prefs: self.prefs.reborrow(),
            toasts: &mut *self.toasts,
            dir: &self.paths.layouts,
            open: self.detached.iter().map(|&(window, _)| window).collect(),
            clear: to_color(self.theme.palette.panel),
        };
        apply_layout_action(action, None, &mut lcx);
    }

    fn run(&mut self, cmd: Cmd, scope: &mut Scope) {
        match cmd {
            Cmd::Pause => toggle_pause(),
            Cmd::Step => step_clock(STEP_LONG_S),
            Cmd::StepShort => step_clock(STEP_SHORT_S),
            Cmd::Skip => toggle_skip(),
            Cmd::Restart => self.host.0.start(),
            Cmd::Stop => self.host.0.stop(),
            Cmd::Idle => hw().trigger_idle(),
            Cmd::Nudge => {
                let dir = hw().nudge_dir;
                nudge(dir);
            }
            Cmd::Shake => {
                let dir = hw().nudge_dir;
                shake(dir);
            }
            Cmd::ToggleHelp => {
                let fade = self.fade_now();
                toggle_pane(
                    self.dock,
                    self.detached,
                    self.commands,
                    Pane::Shortcuts,
                    fade,
                )
            }
            Cmd::CloseHelp => {
                let fade = self.fade_now();
                set_pane_shown(
                    self.dock,
                    self.detached,
                    self.commands,
                    Pane::Shortcuts,
                    false,
                    fade,
                )
            }
            Cmd::Speed(i) => {
                if let Some(&speed) = SPEED_PRESETS.get(i) {
                    hw().set_speed(speed);
                }
            }
            Cmd::Camera(view) => self.controls.camera = Some(view),
            Cmd::Light(light) => {
                // Only touch the resource on a real change, so the lights aren't reset every frame.
                if *self.room != light {
                    *self.room = light;
                }
            }
            Cmd::Screenshot => self.controls.screenshot = true,
            Cmd::Record => self.controls.toggle_record = true,
            Cmd::Strict => {
                let mut s = hw();
                s.strict = !s.strict;
            }
            Cmd::BoardSpeed => {
                let mut s = hw();
                s.board_speed = !s.board_speed;
            }
            Cmd::ServoSpeed => {
                let mut s = hw();
                s.servo_speed = !s.servo_speed;
            }
            Cmd::LogMode => toggle_log_mode(&self.paths.fs),
            Cmd::AutoReload => self.ui.auto_reload = !self.ui.auto_reload,
            Cmd::Overlay(overlay) => self.overlays.toggle(overlay),
            Cmd::Animations => {
                let on = !self.theme.animations();
                self.theme.set_animations(on);
            }
            Cmd::Model(use_gltf) => {
                // Only touch the resource on a real change, so the model isn't rebuilt every frame.
                if let Some(model) = self.model.as_mut() {
                    if model.use_gltf != use_gltf {
                        model.use_gltf = use_gltf;
                    }
                }
            }
            Cmd::Deploy => {
                let fw_dir = self.paths.code.parent().unwrap_or(Path::new("."));
                self.ui.deploy = open_deploy(fw_dir);
            }
            Cmd::TogglePane(pane) => {
                let fade = self.fade_now();
                toggle_pane(self.dock, self.detached, self.commands, pane, fade)
            }
            Cmd::Theme(choice) => {
                if self.theme.choice != choice {
                    self.theme.set_choice(choice);
                }
            }
            Cmd::Layout(name) => self.apply_layout(LayoutAction::Builtin(name)),
            Cmd::FloatPanel => {
                if let Some(pane) = focused_pane(self.dock) {
                    if let Some(at) = self.dock.state.find_tab(&pane) {
                        if at.0.is_main() {
                            self.dock.state.detach_tab(at, self.float_rect);
                        }
                    }
                }
            }
            Cmd::DetachPanel => {
                if let Some(pane) = focused_pane(self.dock) {
                    let place = DetachedSave::new(pane, self.theme.scale);
                    let clear = to_color(self.theme.palette.panel);
                    detach_pane(self.dock, self.commands, &place, false, clear);
                }
            }
            Cmd::ClosePanel => {
                if let Some(pane) = focused_pane(self.dock) {
                    let fade = self.fade_now();
                    set_pane_shown(self.dock, self.detached, self.commands, pane, false, fade);
                }
            }
            Cmd::Palette => palette::open(self.ctx),
            Cmd::NextPanel | Cmd::PrevPanel => {
                let dir = if cmd == Cmd::NextPanel { 1 } else { -1 };
                if let (Scope::Main, Some(id)) =
                    (&*scope, dock::step_tab(&mut self.dock.state, dir))
                {
                    self.ctx.memory_mut(|m| m.request_focus(id));
                    theme::show_focus(self.ctx);
                }
            }
            Cmd::Focus => {
                if matches!(scope, Scope::Main) {
                    toggle_focus(self.dock);
                }
            }
            Cmd::ScaleUp | Cmd::ScaleDown | Cmd::ScaleReset => {
                let step = |scale: f32| match cmd {
                    Cmd::ScaleUp => theme::step_scale(scale, 1),
                    Cmd::ScaleDown => theme::step_scale(scale, -1),
                    _ => 1.0,
                };
                match scope {
                    Scope::Main => {
                        let next = step(self.theme.scale);
                        self.theme.set_scale(next);
                    }
                    Scope::Window { scale, .. } => **scale = step(**scale),
                }
            }
            Cmd::DockBack => {
                if let Scope::Window { entity, pane, .. } = scope {
                    dock::redock(self.dock, self.commands, *entity, *pane);
                }
            }
        }
    }
}

/// Runs every queued command, in order, skipping any that can't run now.
fn dispatch(cmds: Vec<Cmd>, cx: &mut CmdCx, mut scope: Scope) {
    let detached_window = matches!(scope, Scope::Window { .. });
    for cmd in cmds {
        // Read again per command: an earlier one may have changed what is allowed.
        let en = cx.enabled(detached_window);
        if cmd_enabled(cmd, &en) {
            cx.run(cmd, &mut scope);
        }
    }
}

/// The tab focused in the main dock, unless it is the 3D view, which can't be
/// floated, detached or closed.
fn focused_pane(dock: &mut Dock) -> Option<Pane> {
    dock.state
        .find_active_focused()
        .map(|(_, pane)| *pane)
        .filter(|pane| *pane != Pane::Viewport)
}

/// Moves a docked pane into its own OS window.
fn detach_pane(
    dock: &mut Dock,
    commands: &mut Commands,
    place: &DetachedSave,
    under_pointer: bool,
    clear: Color,
) {
    if let Some(at) = dock.state.find_tab(&place.pane) {
        dock.state.remove_tab(at);
        let window = dock::spawn_detached(commands, place, clear);
        if under_pointer {
            commands.entity(window).insert(dock::FitOnMonitor);
        }
    }
}

/// Whether an on/off command is on now, or `None` for one that isn't a toggle;
/// the palette ticks the ones that are on. `options` is the hardware toggles'
/// (strict, board speed, servo speed), read once for all the rows.
fn cmd_checked(cmd: Cmd, cx: &CmdCx, options: (bool, bool, bool)) -> Option<bool> {
    Some(match cmd {
        Cmd::Animations => cx.theme.animations(),
        Cmd::Overlay(overlay) => cx.overlays.get(overlay),
        Cmd::Strict => options.0,
        Cmd::BoardSpeed => options.1,
        Cmd::ServoSpeed => options.2,
        Cmd::AutoReload => cx.ui.auto_reload,
        Cmd::Focus => cx.dock.focus.is_some(),
        Cmd::TogglePane(pane) => pane_shown(cx.dock, cx.detached, pane),
        _ => return None,
    })
}

/// While this window's palette is open, builds its rows from the shortcut
/// table plus the layouts, draws it and queues what was picked.
fn palette_step(cx: &mut CmdCx, detached_window: bool, cmds: &mut Vec<Cmd>, palette: &Palette) {
    if !palette::is_open(cx.ctx) {
        return;
    }
    let en = cx.enabled(detached_window);
    let options = {
        let s = hw();
        (s.strict, s.board_speed, s.servo_speed)
    };
    let mut rows: Vec<palette::Row<PaletteItem>> = SHORTCUTS
        .iter()
        .filter_map(|row| {
            let cmd = row.cmd?;
            Some(palette::Row {
                group: row.group,
                label: row.label().into_owned(),
                keys: row.keys,
                enabled: cmd_enabled(cmd, &en),
                checked: cmd_checked(cmd, cx, options),
                item: PaletteItem::Cmd(cmd),
            })
        })
        .collect();
    rows.extend(cx.prefs.layouts.keys().map(|name| palette::Row {
        group: "Layout",
        label: name.clone(),
        keys: "",
        enabled: true,
        checked: None,
        item: PaletteItem::Layout(LayoutAction::User(name.clone())),
    }));
    if cx
        .ui
        .layout_files_at
        .is_none_or(|t| t.elapsed() > LAYOUT_LIST_REFRESH)
    {
        cx.ui.layout_files = layout::list_files(&cx.paths.layouts);
        cx.ui.layout_files_at = Some(Instant::now());
    }
    rows.extend(cx.ui.layout_files.iter().map(|(name, path)| palette::Row {
        group: "Import layout",
        label: name.clone(),
        keys: "",
        enabled: true,
        checked: None,
        item: PaletteItem::Layout(LayoutAction::Import(path.clone())),
    }));
    match palette::show(cx.ctx, &rows, palette) {
        Some(PaletteItem::Cmd(cmd)) => cmds.push(cmd),
        Some(PaletteItem::Layout(action)) => cx.apply_layout(action),
        None => {}
    }
}

/// What the panes need from `panels`, gathered so the dock's draw closure owns
/// one value instead of a dozen borrows.
struct PaneCx<'a> {
    ui: &'a mut UiState,
    host: &'a mut Host,
    audio: &'a mut AudioUi<'a>,
    cfg: &'a SimConfig,
    paths: &'a crate::Paths,
    palette: &'a Palette,
}

/// Draws one dock tab's body.
fn draw_pane(ui: &mut egui::Ui, pane: Pane, cx: &mut PaneCx) {
    let paths = cx.paths;
    let fw_dir = paths.code.parent().unwrap_or(Path::new("."));
    match pane {
        // Measured by the dock's viewer; the 3D scene is rendered behind egui.
        Pane::Viewport => {}
        Pane::Motion => motion_panel(ui, cx.ui),
        Pane::State => state_panel(ui, cx.palette),
        Pane::Pca => pca_panel(ui, cx.cfg, cx.palette),
        Pane::Board => board_panel(ui, cx.palette),
        Pane::Audio => {
            cx.audio.sounds.wanted = true;
            audio_panel(ui, cx.audio, fw_dir, cx.palette);
        }
        Pane::Plot => {
            let limits = AccelLimits::read(&cx.ui.settings);
            let heap_budget_kb = hw().board.heap_budget_kb as f32;
            let state: &mut UiState = cx.ui;
            plot_panel(
                ui,
                &mut state.plot,
                &state.history,
                &state.markers,
                &limits,
                heap_budget_kb,
                cx.palette,
            );
        }
        Pane::Settings => {
            let auto_reload = cx.ui.auto_reload;
            settings_panel(ui, &mut cx.ui.settings, fw_dir, auto_reload, cx.palette);
        }
        Pane::Log => log_panel(ui, cx.ui, cx.cfg, paths, cx.palette),
        Pane::Shortcuts => shortcuts_window(ui),
        Pane::Behaviours => behaviours_panel(ui),
        Pane::Scenarios => {
            if cx.ui.scenarios.is_empty() {
                ui.weak("No scenarios in simulator/scenarios/");
            } else {
                ui.horizontal_wrapped(|row| {
                    scenario_row(row, cx.ui, &mut cx.host.0, cx.cfg, paths, cx.palette)
                });
            }
        }
    }
}

fn motion_panel(side: &mut egui::Ui, state: &mut UiState) {
    side.horizontal_wrapped(|row| {
                        let dir = hw().nudge_dir;
                        if tip(
                            row.button("Nudge"),
                            "One reading 0.3 g past the held value, in the chosen direction. Clicking the droid also boops, harder the longer you hold, from the side you click",
                            Some(Cmd::Nudge),
                        )
                        .clicked()
                        {
                            nudge(dir);
                        }
                        if tip(
                            row.button("Hard shake"),
                            "One reading 2 g past the held value, in the chosen direction",
                            Some(Cmd::Shake),
                        )
                        .clicked()
                        {
                            shake(dir);
                        }
                        if tip(
                            row.checkbox(&mut state.tipped, "Tipped over"),
                            "Hold the sensor upside down (z reads -1 g)",
                            None,
                        )
                        .changed()
                        {
                            hw().accel_g = [0.0, 0.0, if state.tipped { -1.0 } else { 1.0 }];
                        }
                        if tip(row.button("Rest"), "Back to lying flat (z reads +1 g)", None).clicked() {
                            hw().accel_g = [0.0, 0.0, 1.0];
                            state.tipped = false;
                        }
                    });
    let current = hw().nudge_dir;
    let selected = DIRECTIONS
        .iter()
        .find(|(_, d)| *d == current)
        .map_or("", |(name, _)| *name);
    egui::ComboBox::from_label("Direction")
        .selected_text(selected)
        .show_ui(side, |menu| {
            for (name, d) in DIRECTIONS {
                if menu.selectable_label(current == d, name).clicked() {
                    hw().nudge_dir = d;
                }
            }
        })
        .response
        .on_hover_text(
            "The firmware reads only z, so \u{b1}X/\u{b1}Y nudges are ignored, as on the robot",
        );
    let mut held = hw().accel_g;
    let mut changed = false;
    for (value, label) in held
        .iter_mut()
        .zip(["held x (g)", "held y (g)", "held z (g)"])
    {
        changed |= tip(
            side.add(egui::Slider::new(value, -3.0..=3.0).text(label)),
            "The accelerometer reading held between nudges",
            None,
        )
        .changed();
    }
    if changed {
        hw().accel_g = held;
    }
}

fn pca_panel(side: &mut egui::Ui, cfg: &SimConfig, palette: &Palette) {
    egui::Grid::new("channels")
        .striped(true)
        .show(side, |grid| {
            grid.label("ch");
            grid.label("part");
            grid.label("duty");
            grid.label("pulse");
            grid.label("view");
            grid.end_row();
            let names = [
                "R wing", "L wing", "main R", "main G", "main B", "small R", "small G", "small B",
            ];
            for ch in 0..16 {
                // Spare channels appear once a custom shim writes to them.
                let name = names.get(ch).copied().unwrap_or("spare");
                if ch >= names.len() && hw().pca_duty[ch].is_none() {
                    continue;
                }
                // Before hw() is locked below: these helpers lock it too.
                let view = channel_view(cfg, ch);
                let s = hw();
                grid.label(ch.to_string());
                grid.label(name);
                match (s.duty_fraction(ch), s.pulse_us(ch)) {
                    (Some(d), Some(p)) => {
                        grid.label(format!("{:5.1}%", d * 100.0));
                        grid.label(format!("{p:6.0} us"));
                    }
                    _ => {
                        grid.label("-");
                        grid.label("-");
                    }
                }
                match view {
                    ChannelView::Angle(Some(a)) => grid.label(format!("{a:5.1}°")),
                    ChannelView::Angle(None) => grid.label("-"),
                    ChannelView::Eye(rgb) => eye_swatch(grid, rgb, palette.muted),
                    ChannelView::Blank => grid.label(""),
                };
                grid.end_row();
            }
        });
    {
        let s = hw();
        side.label(white_led_text(s.white_duty));
        side.label(format!(
            "Onboard LED {}   External power {}",
            if s.onboard_led { "on" } else { "off" },
            if s.external_power { "on" } else { "off" }
        ));
    }
    side.add_space(4.0);
    side.label(
        egui::RichText::new(
            "Eye channels peak at 32.8% duty: set_pulse_width_range(0, 6550) at 50 Hz.",
        )
        .color(palette.muted),
    );
}

fn log_panel(
    bottom: &mut egui::Ui,
    ui: &mut UiState,
    cfg: &SimConfig,
    paths: &crate::Paths,
    palette: &Palette,
) {
    let fw_dir = paths.code.parent().unwrap_or(Path::new("."));
    let fs_dir = &paths.fs;
    bottom.horizontal(|row| {
        for (tab, label) in [
            (LogTab::Console, "Log"),
            (LogTab::File(LOG_FILES[0]), LOG_FILES[0]),
            (LogTab::File(LOG_FILES[1]), LOG_FILES[1]),
        ] {
            let hint = match tab {
                LogTab::Console => "The console output of code.py and the simulator".to_owned(),
                LogTab::File(name) => format!("{name} on the simulated drive"),
            };
            tip(
                row.selectable_value(&mut ui.log_tab, tab, label),
                &hint,
                None,
            );
        }
        row.separator();
        if let LogTab::File(name) = ui.log_tab {
            drive_file_header(row, &ui.drive_file, fs_dir, name);
            return;
        }
        tip(
            row.add(
                egui::TextEdit::singleline(&mut ui.log_filter)
                    .hint_text("search")
                    .desired_width(180.0),
            ),
            "Show only the log lines containing this text",
            None,
        );
        tip(
            row.checkbox(&mut ui.show_firmware, "Firmware"),
            "Show lines printed by code.py",
            None,
        );
        tip(
            row.checkbox(&mut ui.show_sim, "[sim]"),
            "Show the simulator's own messages",
            None,
        );
        if tip(row.button("Clear"), "Empty the log", None).clicked() {
            hw().clear_log();
        }
        if tip(
            row.button("Copy"),
            "Copy the lines shown to the clipboard",
            None,
        )
        .clicked()
        {
            let filter = ui.log_filter.to_lowercase();
            // Released before copy_text so the lock is never held across egui calls.
            let text = hw()
                .log
                .iter()
                .filter(|l| line_visible(l, ui.show_firmware, ui.show_sim, &filter))
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            row.ctx().copy_text(text);
        }
        if tip(
            row.button("Save"),
            "Save the whole log to the captures folder next to the exe",
            None,
        )
        .clicked()
        {
            // Snapshot first: file IO must not run under the shared lock.
            let text = hw()
                .log
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let note = match save_log(&paths.captures, &text) {
                Ok(path) => format!("[sim] log saved to {}", path.display()),
                Err(e) => format!("[sim] could not save the log: {e}"),
            };
            hw().push_line(note);
        }
    });
    if let LogTab::File(name) = ui.log_tab {
        ui.drive_file.refresh(fs_dir, name);
        drive_file_body(bottom, &ui.drive_file);
        return;
    }
    let filter = ui.log_filter.to_lowercase();
    let (show_firmware, show_sim) = (ui.show_firmware, ui.show_sim);
    let jump = ui.log_jump;
    // A fresh jump marks its line straight away.
    let marked = jump.or(ui
        .log_highlight
        .filter(|(_, at)| at.elapsed() < LOG_HIGHLIGHT)
        .map(|(seq, _)| seq));
    let cache = &mut ui.traceback_cache;
    if cache.len() > 512 {
        cache.clear();
    }
    let log_scroll = egui::ScrollArea::vertical()
        // Following the end would undo the jump.
        .stick_to_bottom(jump.is_none())
        .auto_shrink([false, false])
        .show(bottom, |area| {
            let mut clicked = None;
            let s = hw();
            for line in s
                .log
                .iter()
                .filter(|l| line_visible(l, show_firmware, show_sim, &filter))
            {
                let location = if line.text.contains("File \"") {
                    if !cache.contains_key(&line.text) {
                        let found = editor::traceback_location(&line.text, fw_dir);
                        cache.insert(line.text.clone(), found);
                    }
                    cache.get(&line.text).cloned().flatten()
                } else {
                    None
                };
                let shown = if let Some((path, number)) = location {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let link = area
                        .add(
                            egui::Label::new(
                                egui::RichText::new(&line.text)
                                    .monospace()
                                    .color(palette.link),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!("Open {name} at line {number}"));
                    if link.clicked() {
                        clicked = Some((path, number));
                    }
                    link
                } else if line.kind == LogKind::Error {
                    area.label(
                        egui::RichText::new(&line.text)
                            .monospace()
                            .color(palette.error),
                    )
                } else {
                    area.monospace(&line.text)
                };
                if jump == Some(line.seq) {
                    shown.scroll_to_me(Some(egui::Align::Center));
                }
                if marked == Some(line.seq) {
                    area.painter().rect_filled(
                        shown.rect.expand(1.0),
                        2.0,
                        palette.accent.gamma_multiply(0.25),
                    );
                }
            }
            clicked
        });
    if let Some(seq) = jump {
        // One frame is enough to scroll; the mark fades on its own timer.
        ui.log_jump = None;
        ui.log_highlight = Some((seq, Instant::now()));
    }
    if marked.is_some() {
        bottom
            .ctx()
            .request_repaint_after(Duration::from_millis(200));
    }
    // After the log lock is dropped: push_line takes it too.
    if let Some((path, number)) = log_scroll.inner {
        if let Err(e) = editor::open_in_editor(&cfg.editor.command, &path, number) {
            hw().push_line(format!("[sim] {e}"));
        }
    }
}

/// The window body, read from the same table the key handling uses.
fn shortcuts_window(win: &mut egui::Ui) {
    // Rows with no keys are for the command palette only.
    let keyed = || SHORTCUTS.iter().filter(|r| !r.keys.is_empty());
    let mut groups: Vec<&str> = Vec::new();
    for row in keyed() {
        if !groups.contains(&row.group) {
            groups.push(row.group);
        }
    }
    for group in groups {
        win.heading(group);
        egui::Grid::new(("shortcuts", group))
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(win, |grid| {
                for row in keyed().filter(|r| r.group == group) {
                    grid.label(egui::RichText::new(row.keys).monospace().strong());
                    grid.label(row.label());
                    grid.end_row();
                }
            });
        win.add_space(4.0);
    }
    win.label(
        egui::RichText::new(
            "Keys are ignored while a text field or the command palette has focus.",
        )
        .small()
        .weak(),
    );
    win.label(
        egui::RichText::new("F6 moves between panels; Tab goes into the panel's controls.")
            .small()
            .weak(),
    );
}

/// Whether `pane` is open, docked or in a window of its own. A docked pane
/// fading out already counts as closed.
fn pane_shown(dock: &Dock, detached: &[(Entity, Pane)], pane: Pane) -> bool {
    detached.iter().any(|&(_, p)| p == pane)
        || (dock::is_shown(&dock.state, pane) && !dock.fades.is_closing(pane))
}

/// Opens or closes `pane` wherever it lives. Closing a detached pane closes
/// its window rather than docking it back; a pane never exists in both places.
/// With `fade` (the time to start at) a docked pane fades in or out; without
/// it the change is instant. Detached windows are always instant.
fn set_pane_shown(
    dock: &mut Dock,
    detached: &[(Entity, Pane)],
    commands: &mut Commands,
    pane: Pane,
    show: bool,
    fade: Option<f64>,
) {
    if let Some(&(window, _)) = detached.iter().find(|&&(_, p)| p == pane) {
        if !show {
            commands.entity(window).despawn();
        }
        return;
    }
    let Some(now) = fade.filter(|_| pane != Pane::Viewport) else {
        dock.fades.remove(pane);
        dock::set_shown(&mut dock.state, pane, show);
        return;
    };
    if show {
        let was_shown = dock::is_shown(&dock.state, pane);
        dock::set_shown(&mut dock.state, pane, true);
        // Also turns a close in progress back round.
        if !was_shown || dock.fades.is_closing(pane) {
            dock.fades.fade_in(pane, now);
        }
    } else if dock::is_shown(&dock.state, pane) {
        dock.fades.fade_out(pane, now);
    }
}

fn toggle_pane(
    dock: &mut Dock,
    detached: &[(Entity, Pane)],
    commands: &mut Commands,
    pane: Pane,
    fade: Option<f64>,
) {
    let show = !pane_shown(dock, detached, pane);
    set_pane_shown(dock, detached, commands, pane, show, fade);
}

/// Whether the widget with keyboard focus is a text field (a DragValue being
/// typed into counts). Not `wants_keyboard_input`: in egui 0.31 that is true
/// for any focused widget, so tabbing to a button would turn every key off.
fn text_field_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused())
        .is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some())
}

/// The shortcuts pressed in one window. Consumed (not just read) so a button
/// that kept focus after a click doesn't also activate on Space. None while a
/// text field has focus or the command palette is open, which owns the keys.
fn read_cmds(ctx: &egui::Context, help_open: bool) -> Vec<Cmd> {
    if text_field_focused(ctx) || palette::is_open(ctx) {
        return Vec::new();
    }
    ctx.input_mut(|inp| {
        let mut cmds = Vec::new();
        for row in SHORTCUTS {
            let Some(cmd) = row.cmd else { continue };
            // Esc is only ours while the window is open, else it is left alone.
            if cmd == Cmd::CloseHelp && !help_open {
                continue;
            }
            // Every bind is consumed (no short-circuit) so none is left behind.
            let hit = row
                .binds
                .iter()
                .fold(false, |hit, &(mods, key)| inp.consume_key(mods, key) | hit);
            if hit {
                cmds.push(cmd);
            }
        }
        cmds
    })
}

/// Shows only the focused panel, or the 3D view if none is focused; the main
/// layout is kept as it was.
fn toggle_focus(dock: &mut Dock) {
    if dock.focus.take().is_some() {
        return;
    }
    let pane = dock
        .state
        .find_active_focused()
        .map_or(Pane::Viewport, |(_, pane)| *pane);
    let mut focus = egui_dock::DockState::new(vec![pane]);
    dock::prepare(&mut focus);
    dock.focus = Some(focus);
}

/// The View menu's panel checkboxes and focus toggle.
fn panels_menu(
    m: &mut egui::Ui,
    dock: &Dock,
    detached: &[(Entity, Pane)],
    en: &Enabled,
    cmds: &mut Vec<Cmd>,
) {
    m.label("Panels");
    for pane in Pane::ALL {
        let mut shown = pane_shown(dock, detached, pane);
        let check = egui::Checkbox::new(&mut shown, pane.title());
        if !cmd_enabled(Cmd::TogglePane(pane), en) {
            m.add_enabled(false, check).on_disabled_hover_text(
                "The 3D view is always there (use focus mode to hide everything else)",
            );
        } else if m.add(check).changed() {
            cmds.push(Cmd::TogglePane(pane));
        }
    }
    let mut focused = dock.focus.is_some();
    if tip(
        m.checkbox(&mut focused, "Focus mode"),
        "Show only the focused panel (or the 3D view); again to restore",
        Some(Cmd::Focus),
    )
    .changed()
    {
        cmds.push(Cmd::Focus);
    }
}

/// Most layouts the user can keep, so the prefs file stays small.
const MAX_USER_LAYOUTS: usize = 50;
/// How often the Import list re-reads the layouts folder while it is open.
const LAYOUT_LIST_REFRESH: Duration = Duration::from_secs(1);

/// What a click in the Layouts menu or a palette row asked for.
#[derive(Clone)]
enum LayoutAction {
    Builtin(&'static str),
    User(String),
    Delete(String),
    Save(String),
    Export(String),
    Import(PathBuf),
}

/// What carrying out a `LayoutAction` touches.
struct LayoutCx<'a, 'w, 's> {
    dock: &'a mut Dock,
    commands: &'a mut Commands<'w, 's>,
    prefs: Mut<'a, Prefs>,
    toasts: &'a mut Toasts,
    /// Where layout files are exported to and imported from.
    dir: &'a Path,
    /// The detached windows, closed when a layout replaces them.
    open: Vec<Entity>,
    /// The colour new detached windows clear to.
    clear: Color,
}

/// Carries out a layout action. `now` is the current layout, which Save and
/// Export need and the other actions don't.
fn apply_layout_action(action: LayoutAction, now: Option<Layout>, cx: &mut LayoutCx) {
    let mut switch = None;
    match action {
        LayoutAction::Builtin(name) => switch = layout::builtin(name),
        LayoutAction::User(name) => switch = cx.prefs.layouts.get(&name).cloned(),
        LayoutAction::Delete(name) => {
            cx.prefs.layouts.remove(&name);
        }
        LayoutAction::Save(name) => {
            let Some(mut now) = now else { return };
            let result = layout::validate(&mut now).and_then(|()| {
                let room = cx.prefs.layouts.len() < MAX_USER_LAYOUTS
                    || cx.prefs.layouts.contains_key(&name);
                if room {
                    cx.prefs.layouts.insert(name.clone(), now);
                    Ok(())
                } else {
                    Err(format!("at most {MAX_USER_LAYOUTS} saved layouts"))
                }
            });
            match result {
                Ok(()) => cx
                    .toasts
                    .push(ToastKind::Ok, format!("Saved layout \"{name}\""), None),
                Err(e) => cx.toasts.push(
                    ToastKind::Error,
                    format!("Could not save layout: {e}"),
                    None,
                ),
            }
        }
        LayoutAction::Export(name) => {
            let Some(mut now) = now else { return };
            let result =
                layout::validate(&mut now).and_then(|()| layout::export(cx.dir, &name, &now));
            match result {
                Ok(path) => {
                    let file = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |f| f.to_string_lossy().into_owned(),
                    );
                    cx.toasts.push(
                        ToastKind::Ok,
                        format!("Exported {file} to the layouts folder"),
                        None,
                    );
                }
                Err(e) => cx.toasts.push(
                    ToastKind::Error,
                    format!("Could not export layout: {e}"),
                    None,
                ),
            }
        }
        LayoutAction::Import(path) => match layout::import(&path) {
            Ok(imported) => {
                cx.toasts.push(ToastKind::Ok, "Imported layout", None);
                switch = Some(imported);
            }
            Err(e) => cx.toasts.push(
                ToastKind::Error,
                format!("Could not import layout: {e}"),
                None,
            ),
        },
    }
    if let Some(next) = switch {
        let open = std::mem::take(&mut cx.open);
        dock::apply_layout(next, cx.dock, cx.commands, open, cx.clear);
    }
}

/// The View menu's Layouts submenu: switch between built-in and saved
/// layouts, save the current one, and export or import layout files.
fn layouts_menu(
    m: &mut egui::Ui,
    ui: &mut UiState,
    captures: &mut Captures,
    detached: &[(Entity, Pane)],
    clear: Color,
    cmds: &mut Vec<Cmd>,
) {
    let mut action = None;
    let mut builtin = None;
    let folder = captures.paths.layouts.display().to_string();
    m.menu_button("Layouts", |m| {
        for name in layout::BUILTINS {
            if m.button(name).clicked() {
                builtin = Some(name);
            }
        }
        if !captures.prefs.layouts.is_empty() {
            m.separator();
        }
        for name in captures.prefs.layouts.keys() {
            m.horizontal(|row| {
                if row.button(name).clicked() {
                    action = Some(LayoutAction::User(name.clone()));
                }
                if tip(row.small_button("✖"), "Delete this layout", None).clicked() {
                    action = Some(LayoutAction::Delete(name.clone()));
                }
            });
        }
        m.separator();

        let name = layout::sanitize_name(&ui.layout_name);
        m.horizontal(|row| {
            row.label("Save current as");
            row.add(
                egui::TextEdit::singleline(&mut ui.layout_name)
                    .hint_text("name")
                    .char_limit(40)
                    .desired_width(110.0),
            );
        });
        m.horizontal(|row| {
            let can_save = name.as_deref().is_some_and(|n| !layout::is_builtin(n));
            let save_tip = if can_save {
                "Keep the current layout under this name"
            } else {
                "Type a name first: letters, digits, spaces, _ and -, and not Default, Debug or Demo"
            };
            if tip(
                row.add_enabled(can_save, egui::Button::new("Save")),
                save_tip,
                None,
            )
            .clicked()
            {
                action = name.clone().map(LayoutAction::Save);
            }
            let export_tip = if name.is_some() {
                format!("Write the current layout to {folder}")
            } else {
                "Type a name first: letters, digits, spaces, _ and -".to_owned()
            };
            if tip(
                row.add_enabled(name.is_some(), egui::Button::new("Export")),
                &export_tip,
                None,
            )
            .clicked()
            {
                action = name.clone().map(LayoutAction::Export);
            }
        });

        // Listed inline rather than as a third menu level: next to the screen
        // edge a nested submenu had no room and squeezed the names.
        m.separator();
        m.label("Import from layouts\\");
        if ui
            .layout_files_at
            .is_none_or(|t| t.elapsed() > LAYOUT_LIST_REFRESH)
        {
            ui.layout_files = layout::list_files(&captures.paths.layouts);
            ui.layout_files_at = Some(Instant::now());
        }
        if ui.layout_files.is_empty() {
            m.weak("No .layout.toml files yet");
        }
        for (name, path) in &ui.layout_files {
            if tip(
                m.button(format!("  {name}")),
                "Load this layout file",
                None,
            )
            .clicked()
            {
                action = Some(LayoutAction::Import(path.clone()));
            }
        }

        m.separator();
        if tip(m.button("Reset layout"), "Back to the Default layout", None).clicked() {
            builtin = Some(layout::BUILTINS[0]);
        }
    });

    if let Some(name) = builtin {
        m.close_menu();
        cmds.push(Cmd::Layout(name));
    }
    let Some(action) = action else {
        return;
    };
    // Deleting leaves the menu open so several can go in one visit.
    if !matches!(action, LayoutAction::Delete(_)) {
        m.close_menu();
    }
    let now = matches!(action, LayoutAction::Save(_) | LayoutAction::Export(_))
        .then(|| captures.current_layout());
    let mut lcx = LayoutCx {
        dock: &mut captures.dock,
        commands: &mut captures.commands,
        prefs: captures.prefs.reborrow(),
        toasts: &mut captures.toasts,
        dir: &captures.paths.layouts,
        open: detached.iter().map(|&(window, _)| window).collect(),
        clear,
    };
    apply_layout_action(action, now, &mut lcx);
}

/// Theme picker and UI scale controls for the toolbar. A picked theme or the
/// reset button queue a command; the slider is continuous, so it returns the
/// scale it set for the caller to write to the `ActiveTheme` resource.
fn appearance_controls(
    row: &mut egui::Ui,
    choice: ThemeChoice,
    scale: f32,
    scale_drag: &mut Option<f32>,
    animations: bool,
    cmds: &mut Vec<Cmd>,
) -> Option<f32> {
    row.label("Theme");
    let mut picked = choice;
    let combo = egui::ComboBox::from_id_salt("theme")
        .selected_text(choice.label())
        .show_ui(row, |list| {
            for option in ThemeChoice::ALL {
                list.selectable_value(&mut picked, option, option.label());
            }
        });
    tip(
        combo.response,
        "Follow system tracks the Windows light/dark setting",
        None,
    );

    row.label("UI scale");
    let mut percent = scale_drag.unwrap_or(scale * 100.0);
    let slider = row.add(
        egui::Slider::new(
            &mut percent,
            theme::SCALE_MIN * 100.0..=theme::SCALE_MAX * 100.0,
        )
        .suffix("%")
        .integer(),
    );
    let slider = tip(slider, "Scale the whole interface", None);
    let mut new_scale = None;
    if slider.dragged() {
        *scale_drag = Some(percent);
    } else if slider.drag_stopped() || slider.changed() {
        *scale_drag = None;
        new_scale = Some(percent / 100.0);
    }
    if tip(
        row.button("Reset scale"),
        "Reset the UI scale",
        Some(Cmd::ScaleReset),
    )
    .clicked()
    {
        *scale_drag = None;
        cmds.push(Cmd::ScaleReset);
    }
    let mut animated = animations;
    if tip(
        row.checkbox(&mut animated, "Animations"),
        "Smooth camera moves and panel fades",
        Some(Cmd::Animations),
    )
    .changed()
    {
        cmds.push(Cmd::Animations);
    }
    if picked != choice {
        cmds.push(Cmd::Theme(picked));
    }
    new_scale
}

/// Scenario picker, Run scenario button and the current scenario's verdict.
/// One button per behaviour, to run it straight away without its trigger.
fn behaviours_panel(ui: &mut egui::Ui) {
    // Cloned out so the lock isn't held while a click writes the request.
    let (behaviours, running) = {
        let s = hw();
        (s.behaviours.clone(), s.behaviour_running.clone())
    };
    if behaviours.is_empty() {
        ui.weak("Run code.py to list the behaviours in behaviours/");
        return;
    }
    ui.horizontal_wrapped(|row| {
        for name in behaviours {
            let clicked = row
                .add_enabled(running.is_none(), egui::Button::new(&name))
                .on_hover_text(format!("Run {name}() once now, with breathing paused"))
                .on_disabled_hover_text(format!(
                    "Wait for {}() to finish",
                    running.as_deref().unwrap_or_default()
                ))
                .clicked();
            if clicked {
                hw().behaviour_request = Some(name);
            }
        }
    });
}

fn scenario_row(
    row: &mut egui::Ui,
    ui: &mut UiState,
    host: &mut PyHost,
    cfg: &SimConfig,
    paths: &crate::Paths,
    palette: &Palette,
) {
    row.label("Scenarios")
        .on_hover_text("Scripted inputs and expected behaviour, from simulator/scenarios/*.toml");
    if !ui.scenarios.contains(&ui.scenario_choice) {
        ui.scenario_choice = ui.scenarios[0].clone();
    }
    let picker = egui::ComboBox::from_id_salt("scenario")
        .selected_text(&ui.scenario_choice)
        .show_ui(row, |list| {
            for name in &ui.scenarios {
                list.selectable_value(&mut ui.scenario_choice, name.clone(), name);
            }
        });
    tip(picker.response, "Which scenario Run scenario plays", None);
    if row
        .button("Run scenario")
        .on_hover_text("Restart code.py and play the scenario against it")
        .clicked()
    {
        run_scenario(host, cfg, paths, &ui.scenario_choice);
    }
    let Some(status) = hw().scenario.clone() else {
        return;
    };
    let passed = status.results.iter().filter(|(_, s)| s == "pass").count();
    let total = status.results.len();
    let (text, colour) = match status.state.as_str() {
        "pass" => (format!("{} PASS {passed}/{total}", status.name), palette.ok),
        "running" => (
            format!("{} running {passed}/{total}", status.name),
            palette.warn,
        ),
        "error" => (format!("{} ERROR", status.name), palette.error),
        _ => (
            format!("{} FAIL {passed}/{total}", status.name),
            palette.error,
        ),
    };
    let details = status
        .results
        .iter()
        .map(|(label, state)| format!("{:7} {label}", state.to_uppercase()))
        .collect::<Vec<_>>()
        .join("\n");
    row.label(egui::RichText::new(text).color(colour).strong())
        .on_hover_text(egui::RichText::new(details).monospace());
}

/// Whether a log line passes the Firmware / [sim] toggles and the search box
/// (`filter` is already lowercase).
fn line_visible(line: &LogLine, show_firmware: bool, show_sim: bool, filter: &str) -> bool {
    let from_sim = match line.kind {
        LogKind::Sim => true,
        // Sim errors are Error-kind but still belong to the [sim] toggle.
        LogKind::Error => line.text.starts_with("[sim]"),
        LogKind::Firmware => false,
    };
    let shown = if from_sim { show_sim } else { show_firmware };
    shown && (filter.is_empty() || line.text.to_lowercase().contains(filter))
}

/// Toolbar toggle for the log_mode file on the simulated drive, which makes
/// boot.py remount it writable and log.py write log.txt.
fn log_mode_checkbox(row: &mut egui::Ui, fs_dir: &Path, cmds: &mut Vec<Cmd>) {
    let mut on = fs_dir.join("log_mode").is_file();
    if row
        .checkbox(&mut on, "log_mode")
        .on_hover_text("Puts log_mode on the simulated drive (dist\\fs\\), so log.py also writes log.txt. Applies on restart, like a reset on the board.")
        .changed()
    {
        cmds.push(Cmd::LogMode);
    }
}

/// Adds or removes the log_mode file, whichever flips the toggle.
fn toggle_log_mode(fs_dir: &Path) {
    let flag = fs_dir.join("log_mode");
    let on = !flag.is_file();
    let result = if on {
        std::fs::create_dir_all(fs_dir).and_then(|()| std::fs::write(&flag, b""))
    } else {
        std::fs::remove_file(&flag)
    };
    let note = match result {
        Ok(()) => format!(
            "[sim] log_mode {}; restart to apply",
            if on { "added" } else { "removed" }
        ),
        Err(e) => format!("[sim] could not change log_mode: {e}"),
    };
    hw().push_line(note);
}

/// Size, Copy and Delete for a log file tab.
fn drive_file_header(row: &mut egui::Ui, file: &DriveFile, fs_dir: &Path, name: &str) {
    match file.stamp {
        Some((_, size)) => {
            row.label(format!(
                "{:.1} / {} KB",
                size as f64 / 1024.0,
                LOG_MAX_BYTES / 1024
            ))
            .on_hover_text("log.py rolls log.txt over to log.old.txt past this size");
        }
        None => {
            row.label("not written yet");
        }
    }
    if row
        .button("Copy")
        .on_hover_text(format!("Copy {name} to the clipboard"))
        .clicked()
    {
        row.ctx().copy_text(file.lines.join("\n"));
    }
    if row
        .button("Delete")
        .on_hover_text("Delete log.txt and log.old.txt from the simulated drive")
        .clicked()
    {
        let failed: Vec<String> = LOG_FILES
            .iter()
            .filter_map(|n| match std::fs::remove_file(fs_dir.join(n)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Some(format!("{n}: {e}")),
                _ => None,
            })
            .collect();
        hw().push_line(if failed.is_empty() {
            "[sim] log files deleted".to_owned()
        } else {
            format!("[sim] could not delete {}", failed.join(", "))
        });
    }
    row.weak(fs_dir.join(name).display().to_string());
}

/// The file's lines; only the visible rows are laid out, since log.txt can
/// reach 256 KB.
fn drive_file_body(bottom: &mut egui::Ui, file: &DriveFile) {
    if file.stamp.is_none() {
        bottom.label(format!(
            "No {} yet. Tick log_mode in the toolbar and restart; log.py writes it once boot.py has made the drive writable.",
            file.name
        ));
        return;
    }
    let row_height = bottom.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show_rows(bottom, row_height, file.lines.len(), |area, rows| {
            for line in &file.lines[rows] {
                area.monospace(line);
            }
        });
}

/// Writes the log to `dir/sim-log-<unix seconds>.txt`, creating `dir` if needed.
fn save_log(dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("sim-log-{}.txt", unix_secs()));
    std::fs::write(&path, text)?;
    Ok(path)
}

/// The firmware's accelerometer thresholds in g, plus the sensor's clip level.
struct AccelLimits {
    nudge: f32,
    animation: f32,
    hard: f32,
    clip: f32,
}

impl AccelLimits {
    /// Thresholds as settings.toml (or config.py's default) gives them, so
    /// the lines move when a setting is edited.
    fn read(settings: &SettingsState) -> Self {
        let g = |key: &str, fallback_mg: i64| {
            settings.saved_value(key).unwrap_or(fallback_mg) as f32 / 1000.0
        };
        Self {
            nudge: g("NUDGE_Z_MG", 250),
            animation: g("ANIMATION_NUDGE_Z_MG", 800),
            hard: g("HARD_MOTION_MG", 2500),
            clip: hw().accel_range_g,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Band {
    Wings,
    Leds,
    Accel,
    Heap,
}

impl Band {
    fn format(self, v: f32) -> String {
        match self {
            Band::Wings => format!("{v:.0}\u{b0}"),
            Band::Leds => format!("{:.1}%", v * 100.0),
            Band::Accel => format!("{v:.2} g"),
            Band::Heap => format!("{v:.0} KB"),
        }
    }
}

/// One plotted trace. Its group and short name label the checkbox rows and
/// the hover readout.
struct SeriesDef {
    group: &'static str,
    short: &'static str,
    band: Band,
    /// Looked up in the active palette when drawn.
    color: fn(&Palette) -> Color32,
    /// The small eye is dashed so it reads apart from the main eye.
    dashed: bool,
    get: fn(&Sample) -> Option<f32>,
}

const fn series(
    group: &'static str,
    short: &'static str,
    band: Band,
    color: fn(&Palette) -> Color32,
    get: fn(&Sample) -> Option<f32>,
) -> SeriesDef {
    SeriesDef {
        group,
        short,
        band,
        color,
        dashed: false,
        get,
    }
}

const fn dashed(s: SeriesDef) -> SeriesDef {
    SeriesDef { dashed: true, ..s }
}

const SERIES: [SeriesDef; 13] = [
    series("Wings", "R", Band::Wings, |p| p.wing_r, |s| s.wings[0]),
    series("Wings", "L", Band::Wings, |p| p.wing_l, |s| s.wings[1]),
    series("Main eye", "R", Band::Leds, |p| p.ch_r, |s| Some(s.main.x)),
    series("Main eye", "G", Band::Leds, |p| p.ch_g, |s| Some(s.main.y)),
    series("Main eye", "B", Band::Leds, |p| p.ch_b, |s| Some(s.main.z)),
    dashed(series(
        "Small eye",
        "R",
        Band::Leds,
        |p| p.ch_r,
        |s| Some(s.small.x),
    )),
    dashed(series(
        "Small eye",
        "G",
        Band::Leds,
        |p| p.ch_g,
        |s| Some(s.small.y),
    )),
    dashed(series(
        "Small eye",
        "B",
        Band::Leds,
        |p| p.ch_b,
        |s| Some(s.small.z),
    )),
    series(
        "White LED",
        "W",
        Band::Leds,
        |p| p.white_led,
        |s| Some(s.white),
    ),
    // z bright; x and y dim, since the firmware ignores side boops.
    series(
        "Accel",
        "x",
        Band::Accel,
        |p| p.accel_x,
        |s| Some(s.accel[0]),
    ),
    series(
        "Accel",
        "y",
        Band::Accel,
        |p| p.accel_y,
        |s| Some(s.accel[1]),
    ),
    series(
        "Accel",
        "z",
        Band::Accel,
        |p| p.accel_z,
        |s| Some(s.accel[2]),
    ),
    // The heap bar's colour in the Board panel.
    series("Heap", "KB", Band::Heap, |p| p.heap_ok, |s| Some(s.heap_kb)),
];

/// The checkbox rows and readout groups, in display order.
const GROUPS: [&str; 6] = [
    "Wings",
    "Main eye",
    "Small eye",
    "White LED",
    "Accel",
    "Heap",
];

/// Pane width, in points, from which the plot's series toggles fit three
/// groups to a row (the widest group is a label and three checkboxes).
const PLOT_TOGGLES_3_COLUMNS: f32 = 560.0;

/// The groups in the tickbox grid: the three LED groups first, so they share a
/// row when three fit, then wings, accelerometer and heap.
const TOGGLE_ORDER: [&str; 6] = [
    "White LED",
    "Main eye",
    "Small eye",
    "Wings",
    "Accel",
    "Heap",
];

/// What the plot shows; kept in `UiState` so it lasts between frames.
struct PlotState {
    window_s: f32,
    /// While set, the view ends at this run time; recording carries on.
    frozen_at: Option<f32>,
    /// True while the view tracks the window. Zooming or dragging clears it;
    /// Reset or a double-click sets it again.
    following: bool,
    visible: [bool; SERIES.len()],
    show_markers: bool,
    /// The x range the plot showed last frame.
    view_x: (f64, f64),
}

/// The name the Events checkbox is saved under in `plot_hidden`.
const MARKERS_NAME: &str = "Events";

impl PlotState {
    fn new(prefs: &Prefs) -> Self {
        let window_s = prefs.plot_window_s.clamp(PLOT_WINDOWS[0], HISTORY_SECONDS);
        Self {
            window_s,
            frozen_at: None,
            following: true,
            visible: std::array::from_fn(|i| !prefs.plot_hidden.contains(&series_name(i))),
            show_markers: !prefs.plot_hidden.iter().any(|n| n == MARKERS_NAME),
            view_x: (-f64::from(window_s), 0.0),
        }
    }

    /// Names of the unticked series (and Events), for the preferences file.
    fn hidden(&self) -> Vec<String> {
        (0..SERIES.len())
            .filter(|&i| !self.visible[i])
            .map(series_name)
            .chain((!self.show_markers).then(|| MARKERS_NAME.to_owned()))
            .collect()
    }
}

fn marker_style(kind: MarkerKind, palette: &Palette) -> (Color32, bool) {
    match kind {
        MarkerKind::Motion => (palette.marker_motion, false),
        MarkerKind::Sound => (palette.marker_sound, false),
        MarkerKind::Idle => (palette.marker_idle, false),
        MarkerKind::Restart => (palette.marker_restart, true),
    }
}

/// A vertical line per marker in view; `labels` adds their names along the
/// top edge (only the top band, so they aren't repeated three times).
fn draw_markers(p: &mut egui_plot::PlotUi, markers: &[&Marker], labels: bool, palette: &Palette) {
    let top_y = labels.then(|| p.plot_bounds().max()[1]);
    for m in markers {
        let (color, dashed) = marker_style(m.kind, palette);
        let x = f64::from(m.t);
        let mut line = VLine::new(m.label.as_str(), x).color(color).width(1.0);
        if dashed {
            line = line.style(LineStyle::dashed_dense());
        }
        p.vline(line);
        if let Some(y) = top_y {
            p.text(
                Text::new(
                    m.label.as_str(),
                    PlotPoint::new(x, y),
                    egui::RichText::new(&m.label).small(),
                )
                .color(color)
                .anchor(egui::Align2::LEFT_TOP),
            );
        }
    }
}

fn series_name(i: usize) -> String {
    format!("{} {}", SERIES[i].group, SERIES[i].short)
}

fn window_label(seconds: f32) -> String {
    if seconds >= 60.0 {
        format!("{:.0} min", seconds / 60.0)
    } else {
        format!("{seconds:.0} s")
    }
}

/// Keeps at most two points (the min and the max, in time order) per bucket,
/// so a long window stays cheap to draw without losing short spikes.
fn decimate(samples: &[&Sample], get: fn(&Sample) -> Option<f32>, buckets: usize) -> Vec<[f64; 2]> {
    let point = |s: &Sample| get(s).map(|v| [f64::from(s.t), f64::from(v)]);
    if samples.len() <= buckets * 2 {
        return samples.iter().filter_map(|s| point(s)).collect();
    }
    let mut out = Vec::with_capacity(buckets * 2);
    for part in samples.chunks(samples.len().div_ceil(buckets)) {
        let mut lo: Option<[f64; 2]> = None;
        let mut hi: Option<[f64; 2]> = None;
        for p in part.iter().filter_map(|s| point(s)) {
            if lo.is_none_or(|l| p[1] < l[1]) {
                lo = Some(p);
            }
            if hi.is_none_or(|h| p[1] > h[1]) {
                hi = Some(p);
            }
        }
        if let (Some(lo), Some(hi)) = (lo, hi) {
            let (first, second) = if lo[0] <= hi[0] { (lo, hi) } else { (hi, lo) };
            out.push(first);
            if second != first {
                out.push(second);
            }
        }
    }
    out
}

/// Window, Freeze and Reset controls, the series checkboxes, four stacked
/// plots with linked time axes, and a readout of every ticked series at the
/// time under the pointer.
fn plot_panel(
    ui: &mut egui::Ui,
    state: &mut PlotState,
    history: &VecDeque<Sample>,
    markers: &VecDeque<Marker>,
    limits: &AccelLimits,
    heap_budget_kb: f32,
    palette: &Palette,
) {
    ui.horizontal_wrapped(|row| {
        egui::ComboBox::from_id_salt("plot_window")
            .selected_text(window_label(state.window_s))
            .show_ui(row, |menu| {
                for w in PLOT_WINDOWS {
                    if menu
                        .selectable_value(&mut state.window_s, w, window_label(w))
                        .clicked()
                    {
                        state.following = true;
                    }
                }
            });
        let mut frozen = state.frozen_at.is_some();
        if row
            .toggle_value(&mut frozen, "Freeze")
            .on_hover_text("Hold the view still. Recording carries on, so nothing is lost.")
            .changed()
        {
            state.frozen_at = if frozen {
                history.back().map(|s| s.t)
            } else {
                None
            };
        }
        if row
            .add_enabled(!state.following, egui::Button::new("Reset"))
            .on_hover_text("Go back to following the window (or double-click a plot)")
            .clicked()
        {
            state.following = true;
        }
        tip(
            row.checkbox(&mut state.show_markers, MARKERS_NAME),
            "Lines where a nudge, shake or boop, a sound, an idle trigger or a restart happened",
            None,
        );
    });
    // The series toggles as a grid, three groups to a row when the pane is wide
    // enough and two otherwise, instead of one group per line. Each group is a
    // label cell and a checkbox cell, so the boxes line up down each column.
    let per_row = if ui.available_width() >= PLOT_TOGGLES_3_COLUMNS {
        3
    } else {
        2
    };
    egui::Grid::new("plot_series")
        .num_columns(per_row * 2)
        .spacing([8.0, 2.0])
        .show(ui, |grid| {
            for (n, group) in TOGGLE_ORDER.iter().enumerate() {
                grid.label(egui::RichText::new(*group).small());
                grid.horizontal(|row| {
                    for (i, s) in SERIES.iter().enumerate().filter(|(_, s)| s.group == *group) {
                        row.checkbox(
                            &mut state.visible[i],
                            egui::RichText::new(s.short).color((s.color)(palette)),
                        );
                    }
                });
                if (n + 1) % per_row == 0 {
                    grid.end_row();
                }
            }
        });

    let Some(last) = history.back() else {
        ui.label("No samples yet.");
        return;
    };
    let end = f64::from(state.frozen_at.unwrap_or(last.t));
    let (x0, x1) = if state.following {
        (end - f64::from(state.window_s), end)
    } else {
        state.view_x
    };
    // The samples in view plus one either side, so lines run to the edges.
    // Nothing after a freeze is shown.
    let i0 = history
        .partition_point(|s| f64::from(s.t) < x0)
        .saturating_sub(1);
    let mut i1 = history.partition_point(|s| f64::from(s.t) <= x1.min(end));
    if history.get(i1).is_some_and(|s| f64::from(s.t) <= end) {
        i1 += 1;
    }
    let shown: Vec<&Sample> = history.range(i0..i1.max(i0)).collect();
    let in_view: Vec<&Marker> = if state.show_markers {
        markers
            .iter()
            .filter(|m| (x0..=x1.min(end)).contains(&f64::from(m.t)))
            .collect()
    } else {
        Vec::new()
    };
    let buckets = (ui.available_width() as usize).max(50);

    let clip = f64::from(limits.clip);
    let bands = [
        (Band::Wings, "plot_wings", "deg", 0.0, 180.0),
        (Band::Leds, "plot_leds", "duty", 0.0, 1.0),
        (Band::Accel, "plot_accel", "g", -clip, clip),
        (
            Band::Heap,
            "plot_heap",
            "KB",
            0.0,
            f64::from(heap_budget_kb) * 1.1,
        ),
    ];
    let mut hover_t = None;
    let mut moved = false;
    let mut reset = false;
    for (n, (band, id, unit, y0, y1)) in bands.into_iter().enumerate() {
        let bottom = n == bands.len() - 1;
        let following = state.following;
        let visible = state.visible;
        let shown = &shown;
        let in_view = &in_view;
        let response = Plot::new(id)
            .height(110.0)
            .link_axis("sim_plot", [true, false])
            .link_cursor("sim_plot", [true, false])
            // The panel's own scrolling keeps the mouse wheel; Ctrl+scroll
            // and right-drag zoom, drag pans.
            .allow_scroll(false)
            .allow_double_click_reset(false)
            .show_axes([bottom, true])
            .y_axis_label(unit)
            .y_axis_min_width(28.0)
            .show(ui, |p| {
                if following {
                    p.set_plot_bounds(PlotBounds::from_min_max([x0, y0], [x1, y1]));
                }
                for (i, s) in SERIES.iter().enumerate() {
                    if s.band != band || !visible[i] {
                        continue;
                    }
                    let mut line = Line::new(series_name(i), decimate(shown, s.get, buckets))
                        .color((s.color)(palette))
                        .width(1.5);
                    if s.dashed {
                        line = line.style(LineStyle::dashed_dense());
                    }
                    p.line(line);
                }
                if band == Band::Accel {
                    accel_limits(p, limits, last.rest_z, (x0, x1), palette);
                }
                if band == Band::Heap {
                    heap_limits(p, heap_budget_kb, (x0, x1), palette);
                }
                draw_markers(p, in_view, n == 0, palette);
                p.response()
                    .hovered()
                    .then(|| p.pointer_coordinate())
                    .flatten()
                    .map(|c| c.x)
            });
        hover_t = hover_t.or(response.inner);
        reset |= response.response.double_clicked();
        let b = response.transform.bounds();
        if n == 0 {
            state.view_x = (b.min()[0], b.max()[0]);
        }
        let off = |a: f64, b: f64| (a - b).abs() > 1e-6 * (x1 - x0).abs().max(1.0);
        if following && (off(b.min()[0], x0) || off(b.max()[0], x1)) {
            moved = true;
        }
        if following && (off(b.min()[1], y0) || off(b.max()[1], y1)) {
            moved = true;
        }
    }
    if moved {
        state.following = false;
    }
    if reset {
        state.following = true;
    }
    readout(ui, state, history, hover_t, end);
}

/// The firmware's thresholds: it compares abs(z - rest) for the nudges and
/// abs(z) for hard motion. Plus the sensor's clip level, one LSB below full
/// scale. Each pair is labelled once, on its upper line; the labels sit at the
/// right, middle and left of the view, as the lines are close together.
fn accel_limits(
    p: &mut egui_plot::PlotUi,
    limits: &AccelLimits,
    rest_z: f32,
    (left_x, right_x): (f64, f64),
    palette: &Palette,
) {
    let mid_x = (left_x + right_x) / 2.0;
    let lines = [
        (
            rest_z,
            limits.nudge,
            "nudge",
            palette.line_nudge,
            right_x,
            egui::Align2::RIGHT_BOTTOM,
        ),
        (
            rest_z,
            limits.animation,
            "animation",
            palette.line_animation,
            mid_x,
            egui::Align2::CENTER_BOTTOM,
        ),
        (
            0.0,
            limits.hard,
            "hard",
            palette.line_hard,
            left_x,
            egui::Align2::LEFT_BOTTOM,
        ),
    ];
    for (centre, offset, name, color, label_x, anchor) in lines {
        for y in [centre + offset, centre - offset] {
            p.hline(HLine::new(name, y).color(color).width(1.0));
        }
        p.text(
            Text::new(
                name,
                PlotPoint::new(label_x, centre + offset),
                egui::RichText::new(name)
                    .text_style(egui::TextStyle::Body)
                    .color(color),
            )
            .color(color)
            .anchor(anchor),
        );
    }
    let clip = limits.clip * (1.0 - 1.0 / 32768.0);
    for y in [clip, -clip] {
        p.hline(
            HLine::new("clip", y)
                .color(palette.line_clip)
                .width(1.0)
                .style(LineStyle::dashed_dense()),
        );
    }
}

/// The heap bar's thresholds: amber from 80% of the budget, red at the budget.
/// Labelled at opposite ends of the view, as the two lines are close. The
/// budget's label hangs below its line, which is at the top of the band.
fn heap_limits(
    p: &mut egui_plot::PlotUi,
    budget_kb: f32,
    (left_x, right_x): (f64, f64),
    palette: &Palette,
) {
    for (y, name, color, label_x, anchor) in [
        (
            budget_kb * 0.8,
            "80%",
            palette.heap_high,
            right_x,
            egui::Align2::RIGHT_BOTTOM,
        ),
        (
            budget_kb,
            "budget",
            palette.heap_over,
            left_x,
            egui::Align2::LEFT_TOP,
        ),
    ] {
        p.hline(HLine::new(name, y).color(color).width(1.0));
        p.text(
            Text::new(
                name,
                PlotPoint::new(label_x, f64::from(y)),
                egui::RichText::new(name)
                    .text_style(egui::TextStyle::Body)
                    .color(color),
            )
            .color(color)
            .anchor(anchor),
        );
    }
}

/// Every ticked series' value at the time under the pointer, grouped like
/// the checkboxes; a hint while the pointer is elsewhere.
fn readout(
    ui: &mut egui::Ui,
    state: &PlotState,
    history: &VecDeque<Sample>,
    hover_t: Option<f64>,
    end: f64,
) {
    let small = |text: String| egui::RichText::new(text).small();
    let sample = hover_t.and_then(|t| {
        let t = t.min(end);
        let i = history.partition_point(|s| f64::from(s.t) < t);
        let after = history.get(i).filter(|s| f64::from(s.t) <= end);
        let before = i.checked_sub(1).and_then(|j| history.get(j));
        match (before, after) {
            (Some(a), Some(b)) => Some(if t - f64::from(a.t) <= f64::from(b.t) - t {
                a
            } else {
                b
            }),
            (a, b) => a.or(b),
        }
    });
    let Some(s) = sample else {
        ui.label(small(
            "Hover for values. Drag to pan; Ctrl+scroll or right-drag to zoom.".to_owned(),
        ));
        return;
    };
    ui.label(small(format!("t = {:.2} s", s.t)));
    for group in GROUPS {
        let values: Vec<String> = SERIES
            .iter()
            .enumerate()
            .filter(|(i, d)| d.group == group && state.visible[*i])
            .map(|(_, d)| match (d.get)(s) {
                Some(v) => format!("{} {}", d.short, d.band.format(v)),
                None => format!("{} -", d.short),
            })
            .collect();
        if !values.is_empty() {
            ui.label(small(format!("{group}: {}", values.join("  "))));
        }
    }
}

/// Every tunable config.py reads, with its value in settings.toml and its
/// default. Edits stay pending until Save writes them into settings.toml. The
/// left-wing calibration is shown but locked: it matches the physical build.
fn settings_panel(
    ui: &mut egui::Ui,
    state: &mut SettingsState,
    dir: &Path,
    auto_reload: bool,
    palette: &Palette,
) {
    let pending = palette.warn;
    let red = palette.error;
    // Above the list, so they stay in reach while the list scrolls.
    ui.horizontal(|row| {
        let dirty = !state.edits.is_empty();
        let save = row
            .add_enabled(dirty, egui::Button::new("Save"))
            .on_hover_text("Write the changes into settings.toml, keeping its comments");
        if save.clicked() {
            let note = match state.save(dir) {
                Ok(n) if auto_reload => format!("[sim] settings.toml saved ({n} changed)"),
                Ok(n) => format!("[sim] settings.toml saved ({n} changed); restart (R) to apply"),
                Err(e) => format!("[sim] could not save settings.toml: {e}"),
            };
            hw().push_line(note);
        }
        if row
            .add_enabled(dirty, egui::Button::new("Revert"))
            .on_hover_text("Forget the unsaved changes")
            .clicked()
        {
            state.revert(dir);
        }
        if dirty {
            row.colored_label(pending, format!("{} unsaved", state.edits.len()));
        }
    });
    if let Some(error) = &state.error {
        ui.colored_label(red, error);
    }
    // Cloned so `state` can be changed while the rows are drawn.
    let tunables = state.tunables.clone();
    // Fills the pane, so its scroll bar is on the pane's edge.
    egui::ScrollArea::vertical()
        .id_salt("settings")
        .auto_shrink([false, false])
        .show(ui, |area| {
            egui::Grid::new("settings_grid")
                .striped(true)
                .num_columns(4)
                .show(area, |grid| {
                    grid.label("key");
                    grid.label("value");
                    grid.label("default");
                    grid.end_row();
                    for t in &tunables {
                        let saved = state.saved(t);
                        let edit = state.edits.get(&t.key).copied();
                        let invalid = state.invalid(t).map(str::to_owned);
                        let mut help = t.help.clone();
                        let mut name = egui::RichText::new(&t.key).monospace().small();
                        if edit.is_some() {
                            name = name.color(pending);
                        } else if invalid.is_some() {
                            name = name.color(red);
                        } else if saved != t.default {
                            name = name.strong();
                        }
                        if let Some(bad) = &invalid {
                            help.push_str(&format!(
                                "\n\nsettings.toml has {bad}, which isn't a whole number, so the default is used."
                            ));
                        }
                        if t.read_only {
                            help.push_str("\n\nCalibration for the physical build: read-only here.");
                        }
                        grid.label(name).on_hover_text(&help);
                        let default = t.default.map_or("?".to_owned(), |d| d.to_string());
                        if t.read_only {
                            grid.label(saved.map_or("?".to_owned(), |v| v.to_string()))
                                .on_hover_text(&help);
                            grid.label(default);
                            grid.label(egui::RichText::new("locked").small().weak());
                        } else {
                            let mut value = edit.or(saved).unwrap_or(0);
                            // Big values (seconds in hours) would take forever at 1 per pixel.
                            let speed = (value.abs() as f64 / 200.0).max(1.0);
                            let changed = grid
                                .add(egui::DragValue::new(&mut value).speed(speed))
                                .on_hover_text(&help)
                                .changed();
                            if changed {
                                state.set(t, value);
                            }
                            grid.label(default);
                            let reset = grid
                                .add_enabled(
                                    t.default.is_some_and(|d| d != value),
                                    egui::Button::new("Reset").small(),
                                )
                                .on_hover_text("Back to the default in config.py")
                                .clicked();
                            if let (true, Some(d)) = (reset, t.default) {
                                state.set(t, d);
                            }
                        }
                        grid.end_row();
                    }
                });
        });
}

/// Remaining seconds to `deadline` as a bar that empties towards it.
fn countdown(
    ui: &mut egui::Ui,
    now: f64,
    deadline: f64,
    length: f64,
    text: String,
) -> egui::Response {
    let left = (deadline - now).max(0.0);
    let fraction = if length > 0.0 { left / length } else { 0.0 };
    ui.add(egui::ProgressBar::new(fraction as f32).text(format!("{text} {left:.1} s")))
}

fn state_panel(ui: &mut egui::Ui, palette: &Palette) {
    let (state, now) = {
        let s = hw();
        (s.fw_state.clone(), s.clock.now())
    };
    if state.events.is_empty() && state.tasks.is_empty() {
        ui.label(egui::RichText::new("code.py isn't running").weak());
        return;
    }
    ui.horizontal_wrapped(|row| {
        for (name, set) in &state.events {
            let text = egui::RichText::new(name).monospace();
            if *set {
                row.label(text.color(palette.ok).strong())
                    .on_hover_text("set");
            } else {
                row.label(text.weak()).on_hover_text("clear");
            }
        }
    });

    match state.angry {
        Some(a) => {
            countdown(
                ui,
                now,
                a.deadline,
                a.length,
                format!("nudges {} / {} \u{b7} window", a.count, a.needed),
            )
            .on_hover_text(
                "Nudges counted in the current angry window. motion_sense starts a new \
                 window (count back to 0) the next time it checks after this runs out.",
            );
        }
        None => {
            ui.label(egui::RichText::new("angry window: not started").weak());
        }
    }
    match state.idle {
        Some(i) => {
            countdown(ui, now, i.deadline, i.length, "idle trigger in".into())
                .on_hover_text("A nudge restarts this wait with a new random length.");
        }
        None => {
            ui.label(egui::RichText::new("idle trigger: not waiting").weak())
                .on_hover_text("idle_attention is running its animation, or hasn't started.");
        }
    }

    for (i, task) in state.tasks.iter().enumerate() {
        ui.add_space(4.0);
        // The arrow sim_runtime.py joins the links with isn't in egui's
        // bundled fonts (it drew as a box), so the links are laid out here.
        ui.horizontal_wrapped(|row| {
            row.spacing_mut().item_spacing.x = 4.0;
            let mut links = task.chain.split(CHAIN_SEP);
            if let Some(first) = links.next() {
                row.label(egui::RichText::new(first).monospace().strong());
            }
            for link in links {
                row.label(
                    egui::RichText::new("\u{bb}")
                        .monospace()
                        .color(palette.muted),
                );
                row.label(egui::RichText::new(link).monospace());
            }
        });
        if !task.numbers.is_empty() {
            ui.indent(("task_numbers", i), |vars| {
                // A two-column list, name then value, one variable per row.
                egui::Grid::new(("task_numbers_grid", i))
                    .num_columns(2)
                    .spacing([16.0, 2.0])
                    .striped(true)
                    .show(vars, |grid| {
                        for (name, value) in &task.numbers {
                            grid.label(egui::RichText::new(name).monospace().color(palette.muted))
                                .on_hover_text(
                                    "A number variable in this task's firmware functions",
                                );
                            grid.label(egui::RichText::new(value).monospace().color(palette.text));
                            grid.end_row();
                        }
                    });
            });
        }
    }
}

/// Heap bar/readout colour; shared so the Board panel and status bar agree.
fn heap_colour(palette: &Palette, used: f32) -> Color32 {
    if used >= 1.0 {
        palette.heap_over
    } else if used >= 0.8 {
        palette.heap_high
    } else {
        palette.heap_ok
    }
}

/// Separator `sim_runtime.py` puts between the links of a task chain.
const CHAIN_SEP: &str = " → ";

/// What code.py is doing now: the button-started behaviour if any, else the
/// function the deepest task's top-level coroutine called (chain
/// `motion_sense → nose_boops → sleep` gives `nose_boops`). Tasks that are all
/// just waiting (`name → sleep`) mean idle breathing.
fn current_behaviour(running: Option<&str>, tasks: &[TaskState]) -> String {
    if let Some(name) = running {
        return name.to_owned();
    }
    let links = |t: &TaskState| t.chain.split(CHAIN_SEP).count();
    // The LED scheduler runs alongside the wing behaviours; it would otherwise
    // tie with them and hide what the droid is doing.
    let behaviours = tasks
        .iter()
        .filter(|t| !t.chain.starts_with("random_led_scheduler"));
    let deepest = behaviours.fold(None::<&TaskState>, |best, t| match best {
        Some(b) if links(b) >= links(t) => Some(b),
        _ => Some(t),
    });
    if let Some(name) = deepest
        .filter(|t| links(t) >= 3)
        .and_then(|t| t.chain.split(CHAIN_SEP).nth(1))
    {
        return name.to_owned();
    }
    if tasks
        .iter()
        .any(|t| t.chain.starts_with("ambient_breathing"))
    {
        "breathing".to_owned()
    } else {
        "\u{2014}".to_owned()
    }
}

/// `1`, `2.5`, `100`: up to one decimal, no trailing ".0".
fn trim_speed(speed: f64) -> String {
    let text = format!("{speed:.1}");
    text.strip_suffix(".0").unwrap_or(&text).to_owned()
}

/// One-row footer of live readouts, from the same sources as the panels.
/// `hw()` is locked once and released before drawing.
fn status_bar(ui: &mut egui::Ui, running: bool, fps: Option<f64>, focus: bool, palette: &Palette) {
    let (paused, skipping, scenario, now, speed, behaviour, heap_bytes, budget_kb, used) = {
        let s = hw();
        let budget = s.heap_budget();
        (
            s.clock.is_paused(),
            s.skipping,
            s.scenario.as_ref().map(|sc| sc.name.clone()),
            s.clock.now(),
            s.clock.speed,
            current_behaviour(s.behaviour_running.as_deref(), &s.fw_state.tasks),
            s.heap_bytes,
            s.board.heap_budget_kb,
            (s.heap_bytes / budget) as f32,
        )
    };

    let (state, colour) = if !running {
        ("stopped", None)
    } else if skipping {
        ("skipping", Some(palette.warn))
    } else if paused {
        ("paused", Some(palette.warn))
    } else {
        ("running", Some(palette.ok))
    };
    let text = match &scenario {
        Some(name) => format!("{state} · scenario {name}"),
        None => state.to_owned(),
    };
    let text = match colour {
        Some(c) => egui::RichText::new(text).color(c),
        None => egui::RichText::new(text).weak(),
    };
    tip(
        ui.label(text),
        "Whether code.py is running, and whether its clock is paused",
        None,
    );
    ui.separator();
    tip(
        ui.label(format!("sim time {now:.1} s")),
        "code.py's clock (time.monotonic), in virtual seconds",
        None,
    );
    ui.separator();
    tip(
        ui.label(format!("{}x", trim_speed(speed))),
        "How fast code.py's clock runs against real time",
        None,
    );
    ui.separator();
    tip(
        ui.label(behaviour),
        "What code.py is doing now, from the task chains in the State panel",
        None,
    );
    ui.separator();
    tip(
        ui.label(
            egui::RichText::new(format!(
                "heap ~{:.0} / {budget_kb:.0} KB",
                heap_bytes / 1024.0
            ))
            .color(heap_colour(palette, used)),
        ),
        "Estimated heap use; see the Board panel for the peak and how it is estimated",
        None,
    );
    ui.separator();
    let fps_text = fps.map_or_else(|| "— FPS".to_owned(), |f| format!("{f:.0} FPS"));
    tip(ui.label(fps_text), "Frames drawn per second", None);
    if focus {
        ui.separator();
        tip(
            ui.label(egui::RichText::new("Focus — F11 to restore").color(palette.accent)),
            "Only one panel is showing; F11 brings the full layout back",
            Some(Cmd::Focus),
        );
    }
}

fn board_panel(ui: &mut egui::Ui, palette: &Palette) {
    let s = hw();
    let b = &s.board;
    let source = if b.measured {
        "measured on the robot"
    } else {
        "estimates; run board_bench.py on the robot"
    };
    ui.label(format!("Timing: {source}"));
    ui.label(
        egui::RichText::new(format!(
            "servo/LED write {:.2} ms, accel read {:.2} ms, scheduler {:.2} ms",
            b.pca_write_ms, b.accel_read_ms, b.loop_ms
        ))
        .small()
        .weak(),
    );

    let budget = s.heap_budget();
    let used = (s.heap_bytes / budget) as f32;
    let colour = heap_colour(palette, used);
    ui.add(
        egui::ProgressBar::new(used.min(1.0))
            .fill(colour)
            .text(format!(
                "heap ~{:.0} of {:.0} KB (peak {:.0})",
                s.heap_bytes / 1024.0,
                b.heap_budget_kb,
                s.heap_peak / 1024.0
            )),
    )
    .on_hover_text("Estimated from what code.py allocates, scaled to CircuitPython's smaller objects. Catches clear overruns, not the last few KB.");

    let limit = b.loop_hold_warn_ms;
    let (text, over) = match &s.longest_hold {
        Some((task, ms)) => (
            format!("longest step ~{ms:.0} ms ({task})"),
            limit > 0.0 && *ms > limit,
        ),
        None => ("longest step: none over 1 ms yet".to_owned(), false),
    };
    let text = egui::RichText::new(text);
    ui.label(if over { text.color(palette.warn) } else { text })
        .on_hover_text(format!(
            "The longest any task ran between two awaits this run, in board time. Nothing else runs meanwhile, so over {limit:.0} ms (loop_hold_warn_ms in sim.toml) the other behaviours visibly stall."
        ));

    if !s.unsimulated.is_empty() {
        ui.label(
            egui::RichText::new(format!("Not simulated: {}", s.unsimulated.join(", ")))
                .color(palette.warn),
        )
        .on_hover_text("These libraries are stand-ins that accept every call and do nothing.");
    }
}

/// The audio resources the Audio panel reads and changes, bundled to keep
/// `panels` within clippy's argument limit.
#[derive(SystemParam)]
struct AudioUi<'w> {
    now: Res<'w, NowPlaying>,
    mix: ResMut<'w, AudioMix>,
    sounds: ResMut<'w, Sounds>,
}

fn audio_panel(ui: &mut egui::Ui, audio: &mut AudioUi, fw_dir: &Path, palette: &Palette) {
    let mixer = hw().mixer;
    ui.label(match mixer {
        Some(m) => format!(
            "Mixer: {} Hz \u{b7} {} ch \u{b7} {}-bit {}",
            m.rate,
            m.channels,
            m.bits,
            if m.signed { "signed" } else { "unsigned" }
        ),
        None => "Mixer: not created yet".to_owned(),
    });

    let playing = audio.now.0.as_ref().map(|p| (p.name.clone(), p.looped));
    ui.horizontal(|row| {
        row.label(match &playing {
            Some((name, true)) => format!("Now playing: {name} (loop)"),
            Some((name, false)) => format!("Now playing: {name}"),
            None => "Nothing playing".to_owned(),
        });
        if row
            .add_enabled(playing.is_some(), egui::Button::new("Stop"))
            .on_hover_text("Stops the sound in the simulator; the firmware isn't told")
            .clicked()
        {
            hw().audio.push(AudioCmd::Stop);
        }
    });
    if playing.is_some() {
        // The meter follows the droid's output, so the simulator's own volume
        // and mute don't move it.
        let level = (audio.now.level() * audio.mix.level).clamp(0.0, 1.0);
        ui.add(
            egui::ProgressBar::new(level)
                .desired_height(10.0)
                .fill(palette.ok),
        );
        ui.ctx().request_repaint();
    }

    let mut muted = audio.mix.muted;
    let mut percent = audio.mix.volume * 100.0;
    let (mute_changed, volume_changed) = ui
        .horizontal(|row| {
            let mute = row.checkbox(&mut muted, "Mute");
            let volume = row.add(egui::Slider::new(&mut percent, 0.0..=100.0).suffix(" %"));
            (mute.changed(), volume.changed())
        })
        .inner;
    ui.label(
        egui::RichText::new(
            "Simulator output only: the firmware's own mixer level still applies on top.",
        )
        .small()
        .weak(),
    );
    // Written only on a change so the resource isn't marked changed every frame.
    if mute_changed {
        audio.mix.muted = muted;
    }
    if volume_changed {
        audio.mix.volume = percent / 100.0;
    }

    ui.separator();
    let error = ui.visuals().error_fg_color;
    let sounds_dir = fw_dir.join("sounds");
    for sound in &audio.sounds.scan.sounds {
        ui.horizontal_wrapped(|row| {
            if row
                .small_button("Play")
                .on_hover_text("Plays it in the simulator only; code.py doesn't see it")
                .clicked()
            {
                hw().audio.push(AudioCmd::Play {
                    path: sounds_dir.join(&sound.name).to_string_lossy().into_owned(),
                    level: 1.0,
                    looped: false,
                });
            }
            row.label(&sound.name);
            if let Some((rate, channels, bits)) = sound.format {
                row.label(
                    egui::RichText::new(format!(
                        "{rate} Hz \u{b7} {channels} ch \u{b7} {bits}-bit"
                    ))
                    .small()
                    .weak(),
                );
            }
            if let Some(problem) = &sound.problem {
                row.colored_label(error, problem);
            }
        });
    }
    if audio.sounds.scan.sounds.is_empty() {
        ui.label(egui::RichText::new("No .wav files in sounds/").weak());
    }
    for (file, place) in &audio.sounds.scan.missing {
        ui.colored_label(error, format!("{file} \u{2014} missing ({place})"));
    }
}

/// State of the Deploy to CIRCUITPY window.
enum DeployDlg {
    Closed,
    /// A result or problem, with a Close button.
    Note(String),
    Confirm {
        drive: PathBuf,
        plan: Vec<deploy::Copy>,
    },
    Running(Arc<Mutex<DeployRun>>),
}

/// Progress shared with the copy thread, so USB writes never block the UI.
struct DeployRun {
    done: usize,
    total: usize,
    /// Set last by the thread: the message to show, or the error.
    outcome: Option<Result<String, String>>,
}

fn lock_run(run: &Mutex<DeployRun>) -> MutexGuard<'_, DeployRun> {
    run.lock().unwrap_or_else(|e| e.into_inner())
}

/// "E:" from "E:\\", the way Explorer names the drive.
fn drive_name(drive: &Path) -> String {
    drive.to_string_lossy().trim_end_matches('\\').to_string()
}

/// Finds the board and works out what would change; cheap enough to do on click.
fn open_deploy(fw_dir: &Path) -> DeployDlg {
    let Some(drive) = deploy::find_circuitpy() else {
        return DeployDlg::Note("No CIRCUITPY drive found. Plug the board in over USB.".into());
    };
    match deploy::plan(fw_dir, &drive) {
        Err(e) => DeployDlg::Note(format!(
            "Cannot compare with CIRCUITPY ({}): {e}",
            drive_name(&drive)
        )),
        Ok(plan) if plan.is_empty() => {
            DeployDlg::Note(format!("CIRCUITPY ({}) is up to date.", drive_name(&drive)))
        }
        Ok(plan) => DeployDlg::Confirm { drive, plan },
    }
}

fn start_deploy(fw_dir: &Path, drive: PathBuf, plan: Vec<deploy::Copy>) -> DeployDlg {
    let run = Arc::new(Mutex::new(DeployRun {
        done: 0,
        total: plan.len(),
        outcome: None,
    }));
    let (shared, fw_dir) = (Arc::clone(&run), fw_dir.to_path_buf());
    let spawned = std::thread::Builder::new()
        .name("deploy".into())
        .spawn(move || {
            let mut failed = None;
            deploy::copy_all(&fw_dir, &drive, &plan, |r| match r {
                Ok(rel) => {
                    hw().push_line(format!("[sim] deployed {rel}"));
                    lock_run(&shared).done += 1;
                }
                Err(e) => failed = Some(e),
            });
            let outcome = match failed {
                None => {
                    let msg = format!("deployed {} files to {}", plan.len(), drive.display());
                    hw().push_line(format!("[sim] {msg}"));
                    Ok(format!(
                        "Deployed {} files to CIRCUITPY ({}).",
                        plan.len(),
                        drive_name(&drive)
                    ))
                }
                Some(e) => {
                    hw().push_line(format!("[sim] deploy error: {e}"));
                    Err(e)
                }
            };
            lock_run(&shared).outcome = Some(outcome);
        });
    match spawned {
        Ok(_) => DeployDlg::Running(run),
        Err(e) => DeployDlg::Note(format!("Cannot start the copy: {e}")),
    }
}

fn deploy_window(ctx: &egui::Context, dlg: &mut DeployDlg, fw_dir: &Path) {
    if matches!(dlg, DeployDlg::Closed) {
        return;
    }
    let mut next = None;
    let mut open = true;
    let mut window = egui::Window::new("Deploy to CIRCUITPY")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);
    // The copy can't be cancelled, so the X only exists while nothing is running.
    if !matches!(dlg, DeployDlg::Running(_)) {
        window = window.open(&mut open);
    }
    window.show(ctx, |win| match &*dlg {
        DeployDlg::Closed => {}
        DeployDlg::Note(text) => {
            win.label(text.as_str());
            if win.button("Close").clicked() {
                next = Some(DeployDlg::Closed);
            }
        }
        DeployDlg::Confirm { drive, plan } => {
            win.label(format!(
                "Copy {} files to CIRCUITPY ({})?",
                plan.len(),
                drive_name(drive)
            ));
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(win, |list| {
                    for item in plan {
                        list.horizontal(|row| {
                            row.label(item.rel.as_str());
                            row.weak(if item.new { "new" } else { "changed" });
                        });
                    }
                });
            win.weak(
                "lib/ and boot_out.txt are never touched; files only on the board are left alone. \
                 The board restarts when code.py is written.",
            );
            win.horizontal(|row| {
                if row.button("Copy").clicked() {
                    next = Some(start_deploy(fw_dir, drive.clone(), plan.clone()));
                }
                if row.button("Cancel").clicked() {
                    next = Some(DeployDlg::Closed);
                }
            });
        }
        DeployDlg::Running(run) => {
            let run = lock_run(run);
            match &run.outcome {
                None => {
                    win.horizontal(|row| {
                        row.spinner();
                        row.label(format!("Copying… {} of {}", run.done, run.total));
                    });
                    ctx.request_repaint();
                }
                Some(Ok(msg)) | Some(Err(msg)) => next = Some(DeployDlg::Note(msg.clone())),
            }
        }
    });
    if let Some(next) = next {
        *dlg = next;
    } else if !open {
        *dlg = DeployDlg::Closed;
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;

    fn task(chain: &str) -> TaskState {
        TaskState {
            chain: chain.to_owned(),
            numbers: Vec::new(),
        }
    }

    #[test]
    fn behaviour_from_deepest_chain() {
        let tasks = [
            task("ambient_breathing → sleep"),
            task("motion_sense → nose_boops → sine_flaps → sleep"),
        ];
        assert_eq!(current_behaviour(None, &tasks), "nose_boops");
    }

    #[test]
    fn behaviour_idle_and_button() {
        let waiting = [
            task("ambient_breathing → sleep"),
            task("motion_sense → sleep"),
        ];
        assert_eq!(current_behaviour(None, &waiting), "breathing");
        assert_eq!(current_behaviour(None, &[]), "\u{2014}");
        assert_eq!(current_behaviour(Some("angry"), &waiting), "angry");
    }

    #[test]
    fn speed_trims_trailing_zero() {
        assert_eq!(trim_speed(1.0), "1");
        assert_eq!(trim_speed(2.5), "2.5");
        assert_eq!(trim_speed(100.0), "100");
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;

    #[test]
    fn no_two_rows_share_a_bind() {
        let mut seen: Vec<(egui::Modifiers, egui::Key)> = Vec::new();
        for bind in SHORTCUTS.iter().flat_map(|row| row.binds) {
            assert!(!seen.contains(bind), "{bind:?} is bound twice");
            seen.push(*bind);
        }
    }

    #[test]
    fn every_command_row_has_a_label() {
        for row in SHORTCUTS.iter().filter(|row| row.cmd.is_some()) {
            assert!(!row.label().is_empty(), "{:?} has no label", row.cmd);
        }
    }

    #[test]
    fn families_have_a_row_each() {
        let has = |cmd: Cmd| SHORTCUTS.iter().any(|row| row.cmd == Some(cmd));
        for pane in Pane::ALL.into_iter().filter(|p| *p != Pane::Viewport) {
            assert!(has(Cmd::TogglePane(pane)), "{pane:?}");
        }
        for choice in ThemeChoice::ALL {
            assert!(has(Cmd::Theme(choice)), "{choice:?}");
        }
        for light in RoomLight::ALL {
            assert!(has(Cmd::Light(light)), "{light:?}");
        }
        for overlay in Overlay::ALL {
            assert!(has(Cmd::Overlay(overlay)), "{overlay:?}");
        }
        for i in 0..SPEED_PRESETS.len() {
            assert!(has(Cmd::Speed(i)), "speed {i}");
        }
        for name in layout::BUILTINS {
            assert!(has(Cmd::Layout(name)), "{name}");
        }
    }
}
