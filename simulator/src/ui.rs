//! Control panels: run controls, motion input, channel readouts, plot, log.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Stroke};
use bevy_egui::EguiContexts;
use egui_plot::{HLine, Line, LineStyle, Plot, PlotBounds, PlotPoint, Text, VLine};

use crate::audio::{AudioMix, NowPlaying, Sounds};
use crate::capture::{unix_secs, ViewControls};
use crate::config::SimConfig;
use crate::deploy;
use crate::editor;
use crate::hw::{hw, AudioCmd, LogKind, LogLine, MarkerKind};
use crate::prefs::Prefs;
use crate::pyhost::{PyHost, ScenarioRun};
use crate::scene::{glow_rgb, CameraView, GlowSource, ModelChoice, RoomLight};
use crate::settings::SettingsState;
use crate::theme::{self, ActiveTheme, Palette, ThemeChoice};

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

/// What a key press does; the shortcut table maps keys to these.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cmd {
    Pause,
    Step,
    Skip,
    Restart,
    Idle,
    Nudge,
    Shake,
    ToggleHelp,
    CloseHelp,
    ScaleUp,
    ScaleDown,
    ScaleReset,
}

/// One line of the Shortcuts window. Mouse-only controls have no binds or
/// cmd; the window and the key handling both read `SHORTCUTS`, so they can't
/// drift apart.
struct Shortcut {
    group: &'static str,
    keys: &'static str,
    action: &'static str,
    binds: &'static [(egui::Modifiers, egui::Key)],
    cmd: Option<Cmd>,
}

const fn mouse(group: &'static str, keys: &'static str, action: &'static str) -> Shortcut {
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
    bound(
        "Run",
        "I",
        "Trigger idle",
        &[(NO_MODS, egui::Key::I)],
        Cmd::Idle,
    ),
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
    mouse(
        "Motion",
        "Click the droid",
        "Boop (longer press = harder, 0.3-2 g)",
    ),
    mouse("Camera", "Left or right drag", "Orbit"),
    mouse("Camera", "Middle drag", "Pan"),
    mouse("Camera", "Scroll", "Zoom"),
    mouse("Plot", "Drag", "Pan"),
    mouse("Plot", "Ctrl+scroll", "Zoom"),
    mouse("Plot", "Right-drag", "Zoom to the box drawn"),
    mouse("Plot", "Double-click", "Back to following"),
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
];

/// Hover text for a control. When `cmd` has a key binding in `SHORTCUTS` the
/// keys are appended, so a tooltip can't disagree with the Shortcuts window.
/// Also shown while the control is disabled, when egui would otherwise show
/// nothing.
fn tip(resp: egui::Response, text: &str, cmd: Option<Cmd>) -> egui::Response {
    let keys = cmd
        .and_then(|c| SHORTCUTS.iter().find(|row| row.cmd == Some(c)))
        .map(|row| row.keys);
    let text = match keys {
        Some(keys) => format!("{text} ({keys})"),
        None => text.to_owned(),
    };
    resp.on_hover_text(text.clone())
        .on_disabled_hover_text(text)
}

/// A collapsible right-panel section with the shared heading style. Returns
/// whether it is open, for the caller to remember.
fn section(ui: &mut egui::Ui, title: &str, open: bool, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let heading = egui::RichText::new(title).text_style(egui::TextStyle::Heading);
    let shown = egui::CollapsingHeader::new(heading)
        .id_salt(title)
        .default_open(open)
        .show(ui, body);
    shown.openness > 0.5
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
    open_motion: bool,
    open_pca: bool,
    open_board: bool,
    open_audio: bool,
    open_plot: bool,
    open_settings: bool,
    open_state: bool,
    settings: SettingsState,
    panel_width: f32,
    log_height: f32,
    /// Names (file stems) of simulator/scenarios/*.toml, re-listed with hot_reload.
    scenarios: Vec<String>,
    scenario_choice: String,
    /// Log text -> traceback location; canonicalize hits the disk, so not per frame.
    traceback_cache: HashMap<String, Option<(PathBuf, u32)>>,
    log_tab: LogTab,
    drive_file: DriveFile,
    /// The Shortcuts window; not saved to prefs.
    show_shortcuts: bool,
    deploy: DeployDlg,
    /// UI scale (percent) while its slider is being dragged. Zooming the UI
    /// under the cursor mid-drag would move the slider away from it, so the
    /// scale is applied on release.
    scale_drag: Option<f32>,
}

/// What the bottom panel shows: the console log, or a file on the simulated drive.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LogTab {
    Console,
    File(&'static str),
}

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
            open_motion: prefs.open_motion,
            open_pca: prefs.open_pca,
            open_board: prefs.open_board,
            open_audio: prefs.open_audio,
            open_plot: prefs.open_plot,
            open_settings: prefs.open_settings,
            open_state: prefs.open_state,
            settings: SettingsState::default(),
            panel_width: prefs.panel_width,
            log_height: prefs.log_height,
            scenarios: Vec::new(),
            scenario_choice: String::new(),
            traceback_cache: HashMap::new(),
            log_tab: LogTab::Console,
            drive_file: DriveFile::default(),
            show_shortcuts: false,
            deploy: DeployDlg::Closed,
            scale_drag: None,
        }
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (record_history, hot_reload, panels).chain())
            .add_systems(Last, (stop_on_exit, save_prefs));
    }
}

/// Smallest window size worth remembering; a minimised window reports ~0.
const MIN_SAVED_WINDOW: f32 = 200.0;
const SAVE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
struct SaveState {
    last_write: Option<Instant>,
    /// Whether the last write failed, so the error is logged once, not every second.
    failed: bool,
}

/// Writes the preferences when they change (at most once a second, so a drag
/// on a panel edge isn't a write per frame) and once more on exit. Runs in
/// `Last` so the AppExit event is still readable.
fn save_prefs(
    ui: Res<UiState>,
    (model, room, mix, theme): (
        Option<Res<ModelChoice>>,
        Res<RoomLight>,
        Res<AudioMix>,
        Res<ActiveTheme>,
    ),
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut saved: ResMut<Prefs>,
    paths: Res<crate::Paths>,
    mut state: Local<SaveState>,
    mut exits: EventReader<AppExit>,
) {
    let exiting = exits.read().count() > 0;
    // The window may already be gone on exit, or minimised to nothing.
    let (width, height) = windows
        .single()
        .map(|w| (w.resolution.width(), w.resolution.height()))
        .ok()
        .filter(|&(w, h)| w >= MIN_SAVED_WINDOW && h >= MIN_SAVED_WINDOW)
        .unwrap_or((saved.window_width, saved.window_height));
    let (strict, board_speed, servo_speed) = {
        let s = hw();
        (s.strict, s.board_speed, s.servo_speed)
    };
    let current = Prefs {
        window_width: width,
        window_height: height,
        panel_width: ui.panel_width,
        log_height: ui.log_height,
        model_gltf: model.map(|m| m.use_gltf).or(saved.model_gltf),
        room_light: *room,
        theme: theme.choice,
        ui_scale: theme.scale,
        strict,
        board_speed,
        servo_speed,
        auto_reload: ui.auto_reload,
        open_motion: ui.open_motion,
        open_pca: ui.open_pca,
        open_board: ui.open_board,
        open_audio: ui.open_audio,
        open_plot: ui.open_plot,
        open_settings: ui.open_settings,
        open_state: ui.open_state,
        volume: mix.volume,
        muted: mix.muted,
        show_firmware: ui.show_firmware,
        show_sim: ui.show_sim,
        plot_window_s: ui.plot.window_s,
        plot_hidden: ui.plot.hidden(),
    };
    if current == *saved {
        return;
    }
    let recent = state
        .last_write
        .is_some_and(|t| t.elapsed() < SAVE_INTERVAL);
    if recent && !exiting {
        return;
    }
    state.last_write = Some(Instant::now());
    match current.save(&paths.prefs) {
        Ok(()) => {
            state.failed = false;
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

fn servo_angle(cfg: &SimConfig, channel: usize) -> Option<f32> {
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
    let scaled = (duty_rgb / EYE_PEAK_DUTY).clamp(Vec3::ZERO, Vec3::ONE);
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
    response.on_hover_text(format!(
        "R {:.0}%  G {:.0}%  B {:.0}% of peak",
        scaled.x * 100.0,
        scaled.y * 100.0,
        scaled.z * 100.0
    ))
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

/// Where saved files go, plus the camera/screenshot requests; bundled to keep
/// `panels` within clippy's argument limit.
#[derive(SystemParam)]
struct Captures<'w> {
    paths: Res<'w, crate::Paths>,
    controls: ResMut<'w, ViewControls>,
}

fn panels(
    mut contexts: EguiContexts,
    mut ui: ResMut<UiState>,
    mut host: ResMut<Host>,
    model: Option<ResMut<ModelChoice>>,
    mut room: ResMut<RoomLight>,
    (cfg, mut audio, mut theme): (Res<SimConfig>, AudioUi, ResMut<ActiveTheme>),
    mut captures: Captures,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let palette = theme.palette;
    let scale = theme.scale;

    // Consumed (not just read) so a button that kept focus after a click
    // doesn't also activate on Space. Skipped while a text field has focus.
    if !ctx.wants_keyboard_input() {
        let window_open = ui.show_shortcuts;
        let cmds: Vec<Cmd> = ctx.input_mut(|inp| {
            let mut cmds = Vec::new();
            for row in SHORTCUTS {
                let Some(cmd) = row.cmd else { continue };
                // Esc is only ours while the window is open, else it is left alone.
                if cmd == Cmd::CloseHelp && !window_open {
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
        });
        for cmd in cmds {
            match cmd {
                Cmd::Pause => toggle_pause(),
                Cmd::Step => step_clock(STEP_LONG_S),
                Cmd::Skip => toggle_skip(),
                Cmd::Restart => host.0.start(),
                Cmd::Idle => hw().trigger_idle(),
                Cmd::Nudge => {
                    let dir = hw().nudge_dir;
                    nudge(dir);
                }
                Cmd::Shake => {
                    let dir = hw().nudge_dir;
                    shake(dir);
                }
                Cmd::ToggleHelp => ui.show_shortcuts = !ui.show_shortcuts,
                Cmd::CloseHelp => ui.show_shortcuts = false,
                Cmd::ScaleUp => theme.set_scale(theme::step_scale(scale, 1)),
                Cmd::ScaleDown => theme.set_scale(theme::step_scale(scale, -1)),
                Cmd::ScaleReset => theme.set_scale(1.0),
            }
        }
    }

    egui::TopBottomPanel::top("toolbar").show(ctx, |bar| {
        bar.horizontal_wrapped(|row| {
            let running = host.0.is_running();
            if tip(
                row.button(if running { "Restart" } else { "Run" }),
                "Run code.py from the start",
                Some(Cmd::Restart),
            )
            .clicked()
            {
                host.0.start();
            }
            if tip(
                row.add_enabled(running, egui::Button::new("Stop")),
                "Stop code.py",
                None,
            )
            .clicked()
            {
                host.0.stop();
            }
            let paused = hw().clock.is_paused();
            if tip(
                row.button(if paused { "Resume" } else { "Pause" }),
                "Freeze code.py's clock",
                Some(Cmd::Pause),
            )
            .clicked()
            {
                toggle_pause();
            }
            if tip(
                row.add_enabled(paused, egui::Button::new("+10 ms")),
                "Run the paused clock for 10 ms",
                None,
            )
            .clicked()
            {
                step_clock(STEP_SHORT_S);
            }
            if tip(
                row.add_enabled(paused, egui::Button::new("+100 ms")),
                "Run the paused clock for 100 ms",
                Some(Cmd::Step),
            )
            .clicked()
            {
                step_clock(STEP_LONG_S);
            }
            row.label(if running { "running" } else { "stopped" });
            row.separator();
            row.label("Speed");
            let (mut speed, skipping) = {
                let s = hw();
                (s.clock.speed, s.skipping)
            };
            // Read back every frame so the slider follows presets and the end of a skip.
            let slider = row.add_enabled(
                !skipping,
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
                && !skipping
            {
                hw().set_speed(speed);
            }
            for preset in SPEED_PRESETS {
                if tip(
                    row.button(format!("{preset}x")),
                    &format!("Run at {preset}x"),
                    None,
                )
                .clicked()
                {
                    hw().set_speed(preset);
                }
            }
            if tip(
                row.button(if skipping { "Stop skip" } else { "Skip to next event" }),
                "Run flat out until a behaviour starts (a sound plays or a hardware.py event changes), then drop back to 1x",
                Some(Cmd::Skip),
            )
            .clicked()
            {
                toggle_skip();
            }
            if tip(
                row.button("Trigger idle now"),
                "Ends the idle wait so attention_animation or trench_run runs",
                Some(Cmd::Idle),
            )
            .clicked()
            {
                hw().trigger_idle();
            }
            row.separator();
            let mut strict = hw().strict;
            if tip(
                row.checkbox(&mut strict, "CircuitPython strict"),
                "Only CircuitPython 9 modules and APIs, and its float precision for time.monotonic(). Applies on restart.",
                None,
            )
            .changed()
            {
                hw().strict = strict;
            }
            let mut board_speed = hw().board_speed;
            if tip(
                row.checkbox(&mut board_speed, "Board speed"),
                "Each servo/LED write, accelerometer read and scheduler pass takes as long as on the robot ([board] in sim.toml).",
                None,
            )
            .changed()
            {
                hw().board_speed = board_speed;
            }
            let mut servo_speed = hw().servo_speed;
            if tip(
                row.checkbox(&mut servo_speed, "Servo speed limit"),
                "The 3D wings turn no faster than the real servo (sec_per_60deg in sim.toml [servo]). The log warns about moves it can't follow either way.",
                None,
            )
            .changed()
            {
                hw().servo_speed = servo_speed;
            }
            log_mode_checkbox(row, &captures.paths.fs);
            tip(
                row.checkbox(&mut ui.auto_reload, "Restart when firmware is saved"),
                "Also when a shim in dist\\shims\\ is added, saved or deleted.",
                None,
            );
            if let Some(mut model) = model {
                row.separator();
                row.label("Model");
                let mut use_gltf = model.use_gltf;
                row.add_enabled_ui(model.gltf_available, |r| {
                    let lola = r.selectable_value(&mut use_gltf, true, "Lola");
                    if model.gltf_available {
                        lola.on_hover_text("The droid model from assets/droid.glb");
                    } else {
                        lola.on_disabled_hover_text("Build simulator/assets/droid.glb first (see README)");
                    }
                });
                row.selectable_value(&mut use_gltf, false, "Primitive")
                    .on_hover_text("The simple built-in shapes");
                // Only touch the resource on a real change, so the model isn't rebuilt every frame.
                if use_gltf != model.use_gltf {
                    model.use_gltf = use_gltf;
                }
            }
            let deploying = matches!(ui.deploy, DeployDlg::Running(_));
            if tip(
                row.add_enabled(!deploying, egui::Button::new("Deploy to CIRCUITPY")),
                "Copy the changed firmware files to the board (never lib/ or boot_out.txt)",
                None,
            )
            .clicked()
            {
                let fw_dir = captures.paths.code.parent().unwrap_or(Path::new("."));
                ui.deploy = open_deploy(fw_dir);
            }
            if tip(
                row.button("Shortcuts"),
                "List every key and mouse control",
                Some(Cmd::ToggleHelp),
            )
            .clicked()
            {
                ui.show_shortcuts = !ui.show_shortcuts;
            }
            row.label(format!("sim time {:.1} s", hw().clock.now()));
        });
        bar.horizontal_wrapped(|row| {
            let controls = &mut captures.controls;
            row.label("View");
            for (label, view) in [
                ("Front", CameraView::Front),
                ("Side", CameraView::Side),
                ("Top", CameraView::Top),
            ] {
                if tip(
                    row.button(label),
                    &format!("Look at the droid from the {}", label.to_lowercase()),
                    None,
                )
                .clicked()
                {
                    controls.camera = Some(view);
                }
            }
            if tip(row.button("Eyes"), "Close-up of the eyes", None).clicked() {
                controls.camera = Some(CameraView::Eyes);
            }
            if tip(row.button("Reset"), "Back to the starting view", None).clicked() {
                controls.camera = Some(CameraView::Reset);
            }
            row.separator();
            if tip(
                row.button("Screenshot"),
                "Save a PNG of the window to the captures folder next to the exe",
                None,
            )
            .clicked()
            {
                controls.screenshot = true;
            }
            let record_label = if controls.is_recording() {
                "Stop recording"
            } else {
                "Record"
            };
            if tip(
                row.button(record_label),
                "Save a PNG every 0.1 s, up to 30 s, for making a GIF",
                None,
            )
            .clicked()
            {
                controls.toggle_record = true;
            }
            row.separator();
            row.label("Light")
                .on_hover_text("Room lighting, to judge how bright the eyes and white LED look in a dim room");
            let mut light = *room;
            for choice in RoomLight::ALL {
                tip(
                    row.selectable_value(&mut light, choice, choice.label()),
                    &format!("{} room lighting", choice.label()),
                    None,
                );
            }
            // Only touch the resource on a real change, so the lights aren't reset every frame.
            if light != *room {
                *room = light;
            }
            row.separator();
            let (picked, new_scale) =
                appearance_controls(row, theme.choice, theme.scale, &mut ui.scale_drag);
            // Written only on a real change so the apply system's change check stays quiet.
            if let Some(choice) = picked.filter(|c| *c != theme.choice) {
                theme.set_choice(choice);
            }
            if let Some(s) = new_scale {
                theme.set_scale(s);
            }
        });
        // Cloned out so the lock isn't held while a click writes the request.
        let (behaviours, running) = {
            let s = hw();
            (s.behaviours.clone(), s.behaviour_running.clone())
        };
        if !behaviours.is_empty() {
            bar.horizontal_wrapped(|row| {
                row.label("Behaviours")
                    .on_hover_text("Each no-argument async function in behaviours/ that code.py doesn't already run as a task");
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
        if !ui.scenarios.is_empty() {
            bar.horizontal(|row| {
                scenario_row(row, &mut ui, &mut host.0, &cfg, &captures.paths, palette)
            });
        }
    });

    let fw_dir = captures.paths.code.parent().unwrap_or(Path::new("."));
    let side_panel = egui::SidePanel::right("hardware")
        .default_width(ui.panel_width)
        .show(ctx, |side| {
            let motion_open = section(side, "Motion (LIS3DH)", ui.open_motion, |side| {
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
                            row.checkbox(&mut ui.tipped, "Tipped over"),
                            "Hold the sensor upside down (z reads -1 g)",
                            None,
                        )
                        .changed()
                        {
                            hw().accel_g = [0.0, 0.0, if ui.tipped { -1.0 } else { 1.0 }];
                        }
                        if tip(row.button("Rest"), "Back to lying flat (z reads +1 g)", None).clicked() {
                            hw().accel_g = [0.0, 0.0, 1.0];
                            ui.tipped = false;
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
                });
            ui.open_motion = motion_open;

            side.separator();
            let state_open = section(side, "State", ui.open_state, |side| {
                state_panel(side, palette)
            });
            ui.open_state = state_open;

            side.separator();
            let pca_open = section(side, "PCA9685", ui.open_pca, |side| {
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
                                "R wing", "L wing", "main R", "main G", "main B", "small R",
                                "small G", "small B",
                            ];
                            for ch in 0..16 {
                                // Spare channels appear once a custom shim writes to them.
                                let name = names.get(ch).copied().unwrap_or("spare");
                                if ch >= names.len() && hw().pca_duty[ch].is_none() {
                                    continue;
                                }
                                // Before hw() is locked below: these helpers lock it too.
                                let view = channel_view(&cfg, ch);
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
                        side.label(format!(
                            "White LED duty {:.1}%",
                            s.white_duty as f32 / 655.35
                        ));
                        side.label(format!(
                            "Onboard LED {}   External power {}",
                            if s.onboard_led { "on" } else { "off" },
                            if s.external_power { "on" } else { "off" }
                        ));
                    }
                    side.label(
                        egui::RichText::new(
                            "Eye channels peak at 32.8% duty: set_pulse_width_range(0, 6550) at 50 Hz.",
                        )
                        .small()
                        .weak(),
                    );
                });
            ui.open_pca = pca_open;

            side.separator();
            let board_open = section(side, "Board", ui.open_board, |side| {
                board_panel(side, palette)
            });
            ui.open_board = board_open;

            side.separator();
            let audio_open = section(side, "Audio", ui.open_audio, |side| {
                audio_panel(side, &mut audio, fw_dir, palette)
            });
            ui.open_audio = audio_open;
            // The panel's sound list is only re-scanned while it is showing.
            audio.sounds.wanted = ui.open_audio;

            side.separator();
            let plot_open = section(side, "Plot", ui.open_plot, |side| {
                    let limits = AccelLimits::read(&ui.settings);
                    let heap_budget_kb = hw().board.heap_budget_kb as f32;
                    let state: &mut UiState = &mut ui;
                    plot_panel(
                        side,
                        &mut state.plot,
                        &state.history,
                        &state.markers,
                        &limits,
                        heap_budget_kb,
                        palette,
                    )
                });
            ui.open_plot = plot_open;

            side.separator();
            let auto_reload = ui.auto_reload;
            let settings_open = section(side, "Settings (settings.toml)", ui.open_settings, |side| {
                settings_panel(side, &mut ui.settings, fw_dir, auto_reload, palette);
            });
            ui.open_settings = settings_open;
        });
    ui.panel_width = side_panel.response.rect.width();

    let log_panel = egui::TopBottomPanel::bottom("log")
        .resizable(true)
        .default_height(ui.log_height)
        .show(ctx, |bottom| {
            let fs_dir = &captures.paths.fs;
            bottom.horizontal(|row| {
                for (tab, label) in [
                    (LogTab::Console, "Log"),
                    (LogTab::File(LOG_FILES[0]), LOG_FILES[0]),
                    (LogTab::File(LOG_FILES[1]), LOG_FILES[1]),
                ] {
                    let hint = match tab {
                        LogTab::Console => {
                            "The console output of code.py and the simulator".to_owned()
                        }
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
                    let note = match save_log(&captures.paths.captures, &text) {
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
            let cache = &mut ui.traceback_cache;
            if cache.len() > 512 {
                cache.clear();
            }
            let log_scroll = egui::ScrollArea::vertical()
                .stick_to_bottom(true)
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
                        if let Some((path, number)) = location {
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
                        } else if line.kind == LogKind::Error {
                            area.label(
                                egui::RichText::new(&line.text)
                                    .monospace()
                                    .color(palette.error),
                            );
                        } else {
                            area.monospace(&line.text);
                        }
                    }
                    clicked
                });
            // After the log lock is dropped: push_line takes it too.
            if let Some((path, number)) = log_scroll.inner {
                if let Err(e) = editor::open_in_editor(&cfg.editor.command, &path, number) {
                    hw().push_line(format!("[sim] {e}"));
                }
            }
        });
    ui.log_height = log_panel.response.rect.height();

    let mut open = ui.show_shortcuts;
    egui::Window::new("Shortcuts")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, shortcuts_window);
    ui.show_shortcuts = open;

    let fw_dir = captures.paths.code.parent().unwrap_or(Path::new("."));
    deploy_window(ctx, &mut ui.deploy, fw_dir);
}

/// The window body, read from the same table the key handling uses.
fn shortcuts_window(win: &mut egui::Ui) {
    let mut groups: Vec<&str> = Vec::new();
    for row in SHORTCUTS {
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
                for row in SHORTCUTS.iter().filter(|r| r.group == group) {
                    grid.label(egui::RichText::new(row.keys).monospace().strong());
                    grid.label(row.action);
                    grid.end_row();
                }
            });
        win.add_space(4.0);
    }
    win.label(
        egui::RichText::new(
            "Keys are ignored while a text field (log search, Settings) has focus.",
        )
        .small()
        .weak(),
    );
}

/// Theme picker and UI scale controls for the toolbar. Returns what changed,
/// for the caller to write to the `ActiveTheme` resource.
fn appearance_controls(
    row: &mut egui::Ui,
    choice: ThemeChoice,
    scale: f32,
    scale_drag: &mut Option<f32>,
) -> (Option<ThemeChoice>, Option<f32>) {
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
        new_scale = Some(1.0);
    }
    (Some(picked), new_scale)
}

/// Scenario picker, Run scenario button and the current scenario's verdict.
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
fn log_mode_checkbox(row: &mut egui::Ui, fs_dir: &Path) {
    let flag = fs_dir.join("log_mode");
    let mut on = flag.is_file();
    if !row
        .checkbox(&mut on, "log_mode")
        .on_hover_text("Puts log_mode on the simulated drive (dist\\fs\\), so log.py also writes log.txt. Applies on restart, like a reset on the board.")
        .changed()
    {
        return;
    }
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
    for group in GROUPS {
        ui.horizontal(|row| {
            row.label(egui::RichText::new(group).small());
            for (i, s) in SERIES.iter().enumerate().filter(|(_, s)| s.group == group) {
                row.checkbox(
                    &mut state.visible[i],
                    egui::RichText::new(s.short).color((s.color)(palette)),
                );
            }
        });
    }

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
                    accel_limits(p, limits, last.rest_z, x1, palette);
                }
                if band == Band::Heap {
                    heap_limits(p, heap_budget_kb, x1, palette);
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
/// scale. Each pair is labelled once, on its upper line.
fn accel_limits(
    p: &mut egui_plot::PlotUi,
    limits: &AccelLimits,
    rest_z: f32,
    right_x: f64,
    palette: &Palette,
) {
    let lines = [
        (rest_z, limits.nudge, "nudge", palette.line_nudge),
        (
            rest_z,
            limits.animation,
            "animation",
            palette.line_animation,
        ),
        (0.0, limits.hard, "hard", palette.line_hard),
    ];
    for (centre, offset, name, color) in lines {
        for y in [centre + offset, centre - offset] {
            p.hline(HLine::new(name, y).color(color).width(1.0));
        }
        p.text(
            Text::new(
                name,
                PlotPoint::new(right_x, centre + offset),
                egui::RichText::new(name).small(),
            )
            .color(color)
            .anchor(egui::Align2::RIGHT_BOTTOM),
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
fn heap_limits(p: &mut egui_plot::PlotUi, budget_kb: f32, right_x: f64, palette: &Palette) {
    for (y, name, color) in [
        (budget_kb * 0.8, "80%", palette.heap_high),
        (budget_kb, "budget", palette.heap_over),
    ] {
        p.hline(HLine::new(name, y).color(color).width(1.0));
        p.text(
            Text::new(
                name,
                PlotPoint::new(right_x, f64::from(y)),
                egui::RichText::new(name).small(),
            )
            .color(color)
            .anchor(egui::Align2::RIGHT_BOTTOM),
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
    if let Some(error) = &state.error {
        ui.colored_label(red, error);
    }
    // Cloned so `state` can be changed while the rows are drawn.
    let tunables = state.tunables.clone();
    egui::ScrollArea::vertical()
        .id_salt("settings")
        .max_height(320.0)
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
        ui.label(egui::RichText::new(&task.chain).monospace().small());
        if !task.numbers.is_empty() {
            let numbers: Vec<String> = task
                .numbers
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect();
            ui.indent(("task_numbers", i), |vars| {
                vars.label(egui::RichText::new(numbers.join("  ")).small().weak())
                    .on_hover_text("Every number variable in this task's firmware functions");
            });
        }
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
    let colour = if used >= 1.0 {
        palette.heap_over
    } else if used >= 0.8 {
        palette.heap_high
    } else {
        palette.heap_ok
    };
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
