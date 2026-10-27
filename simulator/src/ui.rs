//! Control panels: run controls, motion input, channel readouts, plot, log.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Pos2, Stroke};
use bevy_egui::EguiContexts;

use crate::capture::{unix_secs, ViewControls};
use crate::config::SimConfig;
use crate::hw::{hw, LogKind, LogLine};
use crate::prefs::Prefs;
use crate::pyhost::{PyHost, ScenarioRun};
use crate::scene::{glow_rgb, CameraView, GlowSource, ModelChoice};
use crate::settings::SettingsState;

/// A gentle bump, added to the held reading along the chosen direction. From
/// rest it clears the firmware's NUDGE_Z_MG (250 mg) threshold.
const NUDGE_DELTA_G: f32 = 0.3;
/// A hard knock. From rest on +Z it reads 3 g, above HARD_MOTION_MG (2500 mg).
const SHAKE_DELTA_G: f32 = 2.0;
/// Choices for the Motion panel's direction picker.
const DIRECTIONS: [(&str, [f32; 3]); 6] = [
    ("+X", [1.0, 0.0, 0.0]),
    ("\u{2212}X", [-1.0, 0.0, 0.0]),
    ("+Y", [0.0, 1.0, 0.0]),
    ("\u{2212}Y", [0.0, -1.0, 0.0]),
    ("+Z", [0.0, 0.0, 1.0]),
    ("\u{2212}Z", [0.0, 0.0, -1.0]),
];
const PLOT_SECONDS: f32 = 20.0;

/// Folders next to code.py that hold no firmware source.
const NOT_FIRMWARE: [&str; 4] = ["lib", "sounds", "sd", "__pycache__"];

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

/// Queues one reading `delta` g past the held value along `dir`.
fn bump(dir: [f32; 3], delta: f32) {
    let mut s = hw();
    let held = s.accel_g;
    s.accel_pulses
        .push_back(std::array::from_fn(|i| held[i] + dir[i] * delta));
}

pub fn nudge(dir: [f32; 3]) {
    bump(dir, NUDGE_DELTA_G);
}

pub fn shake(dir: [f32; 3]) {
    bump(dir, SHAKE_DELTA_G);
}

fn toggle_pause() {
    let mut s = hw();
    let paused = s.clock.is_paused();
    s.clock.set_paused(!paused);
}

#[derive(Resource)]
pub struct Host(pub PyHost);

#[derive(Resource)]
pub struct UiState {
    speed: f64,
    tipped: bool,
    auto_reload: bool,
    code_mtime: Option<SystemTime>,
    shims_mtime: Option<SystemTime>,
    last_check: Instant,
    history: VecDeque<Sample>,
    started: Instant,
    log_filter: String,
    show_firmware: bool,
    show_sim: bool,
    open_motion: bool,
    open_pca: bool,
    open_board: bool,
    open_plot: bool,
    open_settings: bool,
    open_state: bool,
    settings: SettingsState,
    panel_width: f32,
    log_height: f32,
    /// Names (file stems) of simulator/scenarios/*.toml, re-listed with hot_reload.
    scenarios: Vec<String>,
    scenario_choice: String,
}

type Series = fn(&Sample) -> f32;

struct Sample {
    t: f32,
    wings: [Option<f32>; 2],
    main: Vec3,
    small: Vec3,
    white: f32,
}

impl UiState {
    pub fn new(
        code_mtime: Option<SystemTime>,
        shims_mtime: Option<SystemTime>,
        prefs: &Prefs,
    ) -> Self {
        Self {
            speed: 1.0,
            tipped: false,
            auto_reload: prefs.auto_reload,
            code_mtime,
            shims_mtime,
            last_check: Instant::now(),
            history: VecDeque::new(),
            started: Instant::now(),
            log_filter: String::new(),
            show_firmware: prefs.show_firmware,
            show_sim: prefs.show_sim,
            open_motion: prefs.open_motion,
            open_pca: prefs.open_pca,
            open_board: prefs.open_board,
            open_plot: prefs.open_plot,
            open_settings: prefs.open_settings,
            open_state: prefs.open_state,
            settings: SettingsState::default(),
            panel_width: prefs.panel_width,
            log_height: prefs.log_height,
            scenarios: Vec::new(),
            scenario_choice: String::new(),
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
    model: Option<Res<ModelChoice>>,
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
    let (strict, board_speed) = {
        let s = hw();
        (s.strict, s.board_speed)
    };
    let current = Prefs {
        window_width: width,
        window_height: height,
        panel_width: ui.panel_width,
        log_height: ui.log_height,
        model_gltf: model.map(|m| m.use_gltf).or(saved.model_gltf),
        strict,
        board_speed,
        auto_reload: ui.auto_reload,
        open_motion: ui.open_motion,
        open_pca: ui.open_pca,
        open_board: ui.open_board,
        open_plot: ui.open_plot,
        open_settings: ui.open_settings,
        open_state: ui.open_state,
        show_firmware: ui.show_firmware,
        show_sim: ui.show_sim,
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
fn eye_swatch(ui: &mut egui::Ui, duty_rgb: Vec3) -> egui::Response {
    let scaled = (duty_rgb / EYE_PEAK_DUTY).clamp(Vec3::ZERO, Vec3::ONE);
    // egui::Rgba is linear; converting to Color32 applies the sRGB gamma.
    let colour = Color32::from(egui::Rgba::from_rgb(scaled.x, scaled.y, scaled.z));
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 14.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, colour);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, Color32::GRAY),
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
    let sample = Sample {
        t,
        wings: [
            servo_angle(&cfg, cfg.wing_right.channel),
            servo_angle(&cfg, cfg.wing_left.channel),
        ],
        main: glow_rgb(&cfg, GlowSource::MainEye),
        small: glow_rgb(&cfg, GlowSource::SmallEye),
        white: glow_rgb(&cfg, GlowSource::WhiteLed).x,
    };
    ui.history.push_back(sample);
    while ui.history.front().is_some_and(|s| t - s.t > PLOT_SECONDS) {
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

fn hot_reload(mut ui: ResMut<UiState>, mut host: ResMut<Host>, paths: Res<crate::Paths>) {
    if ui.last_check.elapsed().as_millis() < 500 {
        return;
    }
    ui.last_check = Instant::now();
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
    cfg: Res<SimConfig>,
    mut captures: Captures,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    // Consumed (not just read) so a button that kept focus after a click
    // doesn't also activate on Space. Skipped while a text field has focus.
    if !ctx.wants_keyboard_input() {
        let (space, n, s, r, i) = ctx.input_mut(|inp| {
            let mut key = |k| inp.consume_key(egui::Modifiers::NONE, k);
            (
                key(egui::Key::Space),
                key(egui::Key::N),
                key(egui::Key::S),
                key(egui::Key::R),
                key(egui::Key::I),
            )
        });
        if space {
            toggle_pause();
        }
        if n {
            let dir = hw().nudge_dir;
            nudge(dir);
        }
        if s {
            let dir = hw().nudge_dir;
            shake(dir);
        }
        if r {
            host.0.start();
        }
        if i {
            hw().idle_trigger = true;
        }
    }

    egui::TopBottomPanel::top("toolbar").show(ctx, |bar| {
        bar.horizontal(|row| {
            let running = host.0.is_running();
            if row
                .button(if running { "Restart" } else { "Run" })
                .on_hover_text("Run code.py from the start (R)")
                .clicked()
            {
                host.0.start();
            }
            if row
                .add_enabled(running, egui::Button::new("Stop"))
                .clicked()
            {
                host.0.stop();
            }
            let paused = hw().clock.is_paused();
            if row
                .button(if paused { "Resume" } else { "Pause" })
                .on_hover_text("Freeze code.py's clock (Space)")
                .clicked()
            {
                toggle_pause();
            }
            row.label(if running { "running" } else { "stopped" });
            row.separator();
            row.label("Speed");
            if row
                .add(
                    egui::Slider::new(&mut ui.speed, 1.0..=200.0)
                        .logarithmic(true)
                        .suffix("x"),
                )
                .changed()
            {
                hw().clock.set_speed(ui.speed);
            }
            if row
                .button("Trigger idle now")
                .on_hover_text("Ends the idle wait so attention_animation or trench_run runs (I)")
                .clicked()
            {
                hw().idle_trigger = true;
            }
            row.separator();
            let mut strict = hw().strict;
            if row
                .checkbox(&mut strict, "CircuitPython strict")
                .on_hover_text("Only CircuitPython 9 modules and APIs, and its float precision for time.monotonic(). Applies on restart.")
                .changed()
            {
                hw().strict = strict;
            }
            let mut board_speed = hw().board_speed;
            if row
                .checkbox(&mut board_speed, "Board speed")
                .on_hover_text("Each servo/LED write, accelerometer read and scheduler pass takes as long as on the robot ([board] in sim.toml).")
                .changed()
            {
                hw().board_speed = board_speed;
            }
            row.checkbox(&mut ui.auto_reload, "Restart when firmware is saved")
                .on_hover_text("Also when a shim in dist\\shims\\ is added, saved or deleted.");
            if let Some(mut model) = model {
                row.separator();
                row.label("Model");
                let mut use_gltf = model.use_gltf;
                row.add_enabled_ui(model.gltf_available, |r| {
                    r.selectable_value(&mut use_gltf, true, "Lola")
                        .on_disabled_hover_text("Build simulator/assets/droid.glb first (see README)");
                });
                row.selectable_value(&mut use_gltf, false, "Primitive");
                // Only touch the resource on a real change, so the model isn't rebuilt every frame.
                if use_gltf != model.use_gltf {
                    model.use_gltf = use_gltf;
                }
            }
            row.label(format!("sim time {:.1} s", hw().clock.now()));
        });
        bar.horizontal(|row| {
            let controls = &mut captures.controls;
            row.label("View");
            for (label, view) in [
                ("Front", CameraView::Front),
                ("Side", CameraView::Side),
                ("Top", CameraView::Top),
            ] {
                if row.button(label).clicked() {
                    controls.camera = Some(view);
                }
            }
            if row
                .button("Eyes")
                .on_hover_text("Close-up of the eyes")
                .clicked()
            {
                controls.camera = Some(CameraView::Eyes);
            }
            if row.button("Reset").clicked() {
                controls.camera = Some(CameraView::Reset);
            }
            row.separator();
            if row
                .button("Screenshot")
                .on_hover_text("Save a PNG of the window to the captures folder next to the exe")
                .clicked()
            {
                controls.screenshot = true;
            }
            let record_label = if controls.is_recording() {
                "Stop recording"
            } else {
                "Record"
            };
            if row
                .button(record_label)
                .on_hover_text("Save a PNG every 0.1 s, up to 30 s, for making a GIF")
                .clicked()
            {
                controls.toggle_record = true;
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
            bar.horizontal(|row| scenario_row(row, &mut ui, &mut host.0, &cfg, &captures.paths));
        }
    });

    let fw_dir = captures.paths.code.parent().unwrap_or(Path::new("."));
    let side_panel = egui::SidePanel::right("hardware")
        .default_width(ui.panel_width)
        .show(ctx, |side| {
            let motion = egui::CollapsingHeader::new("Motion (LIS3DH)")
                .default_open(ui.open_motion)
                .show(side, |side| {
                    side.horizontal_wrapped(|row| {
                        let dir = hw().nudge_dir;
                        if row
                            .button("Nudge")
                            .on_hover_text("One reading 0.3 g past the held value, in the chosen direction (or click the head) (N)")
                            .clicked()
                        {
                            nudge(dir);
                        }
                        if row
                            .button("Hard shake")
                            .on_hover_text("One reading 2 g past the held value, in the chosen direction (S)")
                            .clicked()
                        {
                            shake(dir);
                        }
                        if row.checkbox(&mut ui.tipped, "Tipped over").changed() {
                            hw().accel_g = [0.0, 0.0, if ui.tipped { -1.0 } else { 1.0 }];
                        }
                        if row.button("Rest").clicked() {
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
                        changed |= side
                            .add(egui::Slider::new(value, -3.0..=3.0).text(label))
                            .changed();
                    }
                    if changed {
                        hw().accel_g = held;
                    }
                });
            ui.open_motion = motion.openness > 0.5;

            side.separator();
            let state = egui::CollapsingHeader::new("State")
                .default_open(ui.open_state)
                .show(side, state_panel);
            ui.open_state = state.openness > 0.5;

            side.separator();
            let pca = egui::CollapsingHeader::new("PCA9685")
                .default_open(ui.open_pca)
                .show(side, |side| {
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
                                    ChannelView::Eye(rgb) => eye_swatch(grid, rgb),
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
            ui.open_pca = pca.openness > 0.5;

            side.separator();
            let board = egui::CollapsingHeader::new("Board")
                .default_open(ui.open_board)
                .show(side, board_panel);
            ui.open_board = board.openness > 0.5;

            side.separator();
            let plot_section = egui::CollapsingHeader::new(format!("Last {PLOT_SECONDS:.0} s"))
                .default_open(ui.open_plot)
                .show(side, |side| plot(side, &ui.history));
            ui.open_plot = plot_section.openness > 0.5;

            side.separator();
            let auto_reload = ui.auto_reload;
            let settings = egui::CollapsingHeader::new("Settings (settings.toml)")
                .default_open(ui.open_settings)
                .show(side, |side| {
                    settings_panel(side, &mut ui.settings, fw_dir, auto_reload);
                });
            ui.open_settings = settings.openness > 0.5;
        });
    ui.panel_width = side_panel.response.rect.width();

    let log_panel = egui::TopBottomPanel::bottom("log")
        .resizable(true)
        .default_height(ui.log_height)
        .show(ctx, |bottom| {
            bottom.horizontal(|row| {
                row.label("Log");
                row.add(
                    egui::TextEdit::singleline(&mut ui.log_filter)
                        .hint_text("search")
                        .desired_width(180.0),
                );
                row.checkbox(&mut ui.show_firmware, "Firmware");
                row.checkbox(&mut ui.show_sim, "[sim]");
                if row.button("Clear").clicked() {
                    hw().clear_log();
                }
                if row
                    .button("Copy")
                    .on_hover_text("Copy the lines shown to the clipboard")
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
                if row
                    .button("Save")
                    .on_hover_text("Save the whole log to the captures folder next to the exe")
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
            let filter = ui.log_filter.to_lowercase();
            let (show_firmware, show_sim) = (ui.show_firmware, ui.show_sim);
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(bottom, |area| {
                    let s = hw();
                    for line in s
                        .log
                        .iter()
                        .filter(|l| line_visible(l, show_firmware, show_sim, &filter))
                    {
                        if line.kind == LogKind::Error {
                            area.label(
                                egui::RichText::new(&line.text)
                                    .monospace()
                                    .color(Color32::from_rgb(235, 90, 80)),
                            );
                        } else {
                            area.monospace(&line.text);
                        }
                    }
                });
        });
    ui.log_height = log_panel.response.rect.height();
}

/// Scenario picker, Run scenario button and the current scenario's verdict.
fn scenario_row(
    row: &mut egui::Ui,
    ui: &mut UiState,
    host: &mut PyHost,
    cfg: &SimConfig,
    paths: &crate::Paths,
) {
    row.label("Scenarios")
        .on_hover_text("Scripted inputs and expected behaviour, from simulator/scenarios/*.toml");
    if !ui.scenarios.contains(&ui.scenario_choice) {
        ui.scenario_choice = ui.scenarios[0].clone();
    }
    egui::ComboBox::from_id_salt("scenario")
        .selected_text(&ui.scenario_choice)
        .show_ui(row, |list| {
            for name in &ui.scenarios {
                list.selectable_value(&mut ui.scenario_choice, name.clone(), name);
            }
        });
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
        "pass" => (
            format!("{} PASS {passed}/{total}", status.name),
            Color32::from_rgb(70, 170, 90),
        ),
        "running" => (
            format!("{} running {passed}/{total}", status.name),
            Color32::from_rgb(230, 170, 40),
        ),
        "error" => (
            format!("{} ERROR", status.name),
            Color32::from_rgb(220, 60, 60),
        ),
        _ => (
            format!("{} FAIL {passed}/{total}", status.name),
            Color32::from_rgb(220, 60, 60),
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

/// Writes the log to `dir/sim-log-<unix seconds>.txt`, creating `dir` if needed.
fn save_log(dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("sim-log-{}.txt", unix_secs()));
    std::fs::write(&path, text)?;
    Ok(path)
}

fn plot(ui: &mut egui::Ui, history: &VecDeque<Sample>) {
    let (response, painter) = ui.allocate_painter(
        egui::vec2(ui.available_width(), 220.0),
        egui::Sense::hover(),
    );
    let rect = response.rect;
    painter.rect_filled(rect, 4.0, Color32::from_gray(24));
    let Some(last) = history.back() else { return };
    let half = rect.height() / 2.0;
    let x = |t: f32| rect.right() - (last.t - t) / PLOT_SECONDS * rect.width();
    // Top half: wing servo angles 0..180. Bottom half: LED duty 0..1.
    let wing_y = |deg: f32| rect.top() + half * (1.0 - deg / 180.0);
    let led_y = |v: f32| rect.bottom() - v * half;

    let line = |points: Vec<Pos2>, color: Color32| {
        if points.len() > 1 {
            painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
        }
    };
    for (i, color) in [
        (0, Color32::from_rgb(240, 170, 60)),
        (1, Color32::from_rgb(90, 200, 230)),
    ] {
        line(
            history
                .iter()
                .filter_map(|s| s.wings[i].map(|a| Pos2::new(x(s.t), wing_y(a))))
                .collect(),
            color,
        );
    }
    let channels: [(Series, Color32); 4] = [
        (|s| s.main.x, Color32::from_rgb(230, 60, 60)),
        (|s| s.main.y, Color32::from_rgb(60, 210, 90)),
        (|s| s.main.z, Color32::from_rgb(80, 120, 255)),
        (|s| s.white, Color32::from_gray(220)),
    ];
    for (get, color) in channels {
        line(
            history
                .iter()
                .map(|s| Pos2::new(x(s.t), led_y(get(s))))
                .collect(),
            color,
        );
    }
    let small = Color32::from_rgb(200, 120, 255);
    line(
        history
            .iter()
            .map(|s| Pos2::new(x(s.t), led_y(s.small.max_element())))
            .collect(),
        small,
    );

    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.top() + half),
            Pos2::new(rect.right(), rect.top() + half),
        ],
        Stroke::new(1.0, Color32::from_gray(60)),
    );
    let font = egui::FontId::proportional(11.0);
    painter.text(
        rect.left_top() + egui::vec2(4.0, 2.0),
        egui::Align2::LEFT_TOP,
        "wings (deg): R orange, L cyan",
        font.clone(),
        Color32::GRAY,
    );
    painter.text(
        Pos2::new(rect.left() + 4.0, rect.top() + half + 2.0),
        egui::Align2::LEFT_TOP,
        "main eye R/G/B, small eye (max) purple, white LED",
        font,
        Color32::GRAY,
    );
}

/// Every tunable config.py reads, with its value in settings.toml and its
/// default. Edits stay pending until Save writes them into settings.toml. The
/// left-wing calibration is shown but locked: it matches the physical build.
fn settings_panel(ui: &mut egui::Ui, state: &mut SettingsState, dir: &Path, auto_reload: bool) {
    let pending = Color32::from_rgb(230, 170, 40);
    let red = Color32::from_rgb(220, 60, 60);
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

fn state_panel(ui: &mut egui::Ui) {
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
                row.label(text.color(Color32::from_rgb(70, 170, 90)).strong())
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

    for task in &state.tasks {
        ui.label(egui::RichText::new(task).monospace().small());
    }
}

fn board_panel(ui: &mut egui::Ui) {
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
        Color32::from_rgb(220, 60, 60)
    } else if used >= 0.8 {
        Color32::from_rgb(230, 170, 40)
    } else {
        Color32::from_rgb(70, 170, 90)
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

    if !s.unsimulated.is_empty() {
        ui.label(
            egui::RichText::new(format!("Not simulated: {}", s.unsimulated.join(", ")))
                .color(Color32::from_rgb(230, 170, 40)),
        )
        .on_hover_text("These libraries are stand-ins that accept every call and do nothing.");
    }
}
