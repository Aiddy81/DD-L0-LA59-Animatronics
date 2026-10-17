//! Control panels: run controls, motion input, channel readouts, plot, log.

use std::collections::VecDeque;
use std::time::{Instant, SystemTime};

use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Pos2, Stroke};
use bevy_egui::EguiContexts;

use crate::config::SimConfig;
use crate::hw::hw;
use crate::pyhost::PyHost;
use crate::scene::{glow_rgb, GlowSource};

/// A gentle bump: one accelerometer read above code.py's 1.1 g threshold.
const NUDGE_G: f32 = 1.3;
/// A hard knock. Note code.py sets RANGE_2_G, so the sensor clips this to ~2 g.
const SHAKE_G: f32 = 3.0;
const PLOT_SECONDS: f32 = 20.0;

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
    let mtime = std::fs::metadata(host.0.code_path())
        .and_then(|m| m.modified())
        .ok();
    if mtime != ui.code_mtime {
        ui.code_mtime = mtime;
        if ui.auto_reload {
            hw().push_line("[sim] code.py changed - restarting".into());
            host.0.start();
        }
    }
}

fn stop_on_exit(mut exits: EventReader<AppExit>, mut host: ResMut<Host>) {
    if exits.read().next().is_some() {
        host.0.stop();
    }
}

fn panels(mut contexts: EguiContexts, mut ui: ResMut<UiState>, mut host: ResMut<Host>) {
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
                .on_hover_text("Ends the idle wait so attention_animation runs")
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
            row.checkbox(&mut ui.auto_reload, "Restart when code.py is saved");
            row.label(format!("sim time {:.1} s", hw().clock.now()));
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
                    grid.end_row();
                    let names = [
                        "R wing", "L wing", "main R", "main G", "main B", "small R", "small G",
                        "small B",
                    ];
                    for (ch, name) in names.iter().enumerate() {
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
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(bottom, |area| {
                    let s = hw();
                    for line in &s.log {
                        area.monospace(line);
                    }
                });
        });
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
