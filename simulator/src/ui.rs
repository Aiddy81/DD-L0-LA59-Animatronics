//! Control panels: run controls, motion input, channel readouts, plot, log.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Pos2, Stroke};
use bevy_egui::EguiContexts;

use crate::capture::{unix_secs, ViewControls};
use crate::config::SimConfig;
use crate::hw::{hw, LogKind, LogLine};
use crate::pyhost::PyHost;
use crate::scene::{glow_rgb, CameraView, GlowSource, ModelChoice};

/// A gentle bump: one accelerometer read above code.py's 1.1 g threshold.
const NUDGE_G: f32 = 1.3;
/// A hard knock. Note code.py sets RANGE_2_G, so the sensor clips this to ~2 g.
const SHAKE_G: f32 = 3.0;
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

pub fn nudge() {
    hw().accel_pulses.push_back([0.0, 0.0, NUDGE_G]);
}

#[derive(Resource)]
pub struct Host(pub PyHost);

#[derive(Resource)]
pub struct UiState {
    speed: f64,
    tipped: bool,
    auto_reload: bool,
    code_mtime: Option<SystemTime>,
    last_check: Instant,
    history: VecDeque<Sample>,
    started: Instant,
    log_filter: String,
    show_firmware: bool,
    show_sim: bool,
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
    pub fn new(code_mtime: Option<SystemTime>) -> Self {
        Self {
            speed: 1.0,
            tipped: false,
            auto_reload: true,
            code_mtime,
            last_check: Instant::now(),
            history: VecDeque::new(),
            started: Instant::now(),
            log_filter: String::new(),
            show_firmware: true,
            show_sim: true,
        }
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (record_history, hot_reload, panels).chain())
            .add_systems(Last, stop_on_exit);
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

fn hot_reload(mut ui: ResMut<UiState>, mut host: ResMut<Host>) {
    if ui.last_check.elapsed().as_millis() < 500 {
        return;
    }
    ui.last_check = Instant::now();
    let mtime = firmware_mtime(host.0.code_path());
    if mtime != ui.code_mtime {
        ui.code_mtime = mtime;
        if ui.auto_reload {
            hw().push_line("[sim] firmware changed - restarting".into());
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

    egui::TopBottomPanel::top("toolbar").show(ctx, |bar| {
        bar.horizontal(|row| {
            let running = host.0.is_running();
            if row
                .button(if running { "Restart" } else { "Run" })
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
                .on_hover_text("Ends the idle wait so attention_animation or trench_run runs")
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
            row.checkbox(&mut ui.auto_reload, "Restart when firmware is saved");
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
    });

    egui::SidePanel::right("hardware")
        .default_width(300.0)
        .show(ctx, |side| {
            side.heading("Motion (LIS3DH)");
            side.horizontal_wrapped(|row| {
                if row
                    .button("Nudge")
                    .on_hover_text("One reading at 1.3 g (or click the head)")
                    .clicked()
                {
                    nudge();
                }
                if row
                    .button("Hard shake")
                    .on_hover_text("One reading at 3 g. RANGE_2_G clips it to ~2 g!")
                    .clicked()
                {
                    hw().accel_pulses.push_back([0.0, 0.0, SHAKE_G]);
                }
                if row.checkbox(&mut ui.tipped, "Tipped over").changed() {
                    hw().accel_g[2] = if ui.tipped { -1.0 } else { 1.0 };
                }
            });
            let mut z = hw().accel_g[2];
            if side
                .add(egui::Slider::new(&mut z, -3.0..=3.0).text("held z (g)"))
                .changed()
            {
                hw().accel_g[2] = z;
            }

            side.separator();
            side.heading("PCA9685");
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
                        "R wing", "L wing", "main R", "main G", "main B", "small R", "small G",
                        "small B",
                    ];
                    for (ch, name) in names.iter().enumerate() {
                        // Before hw() is locked below: these helpers lock it too.
                        let view = channel_view(&cfg, ch);
                        let s = hw();
                        grid.label(ch.to_string());
                        grid.label(*name);
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

            side.separator();
            side.heading("Board");
            board_panel(side);

            side.separator();
            side.heading(format!("Last {PLOT_SECONDS:.0} s"));
            plot(side, &ui.history);
        });

    egui::TopBottomPanel::bottom("log")
        .resizable(true)
        .default_height(170.0)
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
