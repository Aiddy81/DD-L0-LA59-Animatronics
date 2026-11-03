//! Small cards at the bottom-right of the window for events that are easy to
//! miss in the log: a saved screenshot, a reload, a traceback.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_egui::egui;

use crate::theme::Palette;

const MAX_SHOWN: usize = 4;
const LIFE: Duration = Duration::from_secs(4);
/// Longer for toasts that point at a log line, so there is time to click.
const LIFE_JUMP: Duration = Duration::from_secs(8);
const MAX_WIDTH: f32 = 360.0;
/// How often an idle UI wakes up to let toasts expire.
const REPAINT: Duration = Duration::from_millis(250);

#[derive(Clone, Copy)]
pub enum ToastKind {
    Info,
    Ok,
    Error,
}

struct Toast {
    id: u64,
    text: String,
    kind: ToastKind,
    born: Instant,
    life: Duration,
    /// Log line (by sequence number) that a click scrolls to.
    jump: Option<u64>,
}

#[derive(Resource, Default)]
pub struct Toasts {
    items: VecDeque<Toast>,
    next_id: u64,
}

pub struct ToastPlugin;

impl Plugin for ToastPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toasts>();
    }
}

impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>, jump: Option<u64>) {
        if self.items.len() >= MAX_SHOWN {
            self.items.pop_front();
        }
        self.next_id += 1;
        self.items.push_back(Toast {
            id: self.next_id,
            text: text.into(),
            kind,
            born: Instant::now(),
            life: if jump.is_some() { LIFE_JUMP } else { LIFE },
            jump,
        });
    }

    /// Draws the toasts above `bottom_margin` and returns the log line of a
    /// clicked toast, if it has one.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        bottom_margin: f32,
    ) -> Option<u64> {
        let now = Instant::now();
        self.items.retain(|t| now.duration_since(t.born) < t.life);
        if self.items.is_empty() {
            return None;
        }
        let mut clicked: Option<(u64, Option<u64>)> = None;
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -(bottom_margin + 12.0)])
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(MAX_WIDTH);
                for t in self.items.iter_mut() {
                    let marker = match t.kind {
                        ToastKind::Info => palette.accent,
                        ToastKind::Ok => palette.ok,
                        ToastKind::Error => palette.error,
                    };
                    let card = egui::Frame::new()
                        .fill(palette.surface)
                        .stroke(egui::Stroke::new(
                            palette.stroke_width.max(1.0),
                            palette.border,
                        ))
                        .corner_radius(palette.rounding)
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |card| {
                            card.horizontal_top(|row| {
                                // Painted: egui's bundled fonts have no filled-circle glyph.
                                let size = row.text_style_height(&egui::TextStyle::Body);
                                let (dot, _) = row.allocate_exact_size(
                                    egui::vec2(size * 0.6, size),
                                    egui::Sense::hover(),
                                );
                                row.painter()
                                    .circle_filled(dot.center(), size * 0.25, marker);
                                row.add(
                                    egui::Label::new(
                                        egui::RichText::new(&t.text).color(palette.text),
                                    )
                                    .wrap(),
                                );
                            });
                        });
                    let hint = if t.jump.is_some() {
                        "Click to show it in the log"
                    } else {
                        "Click to dismiss"
                    };
                    let resp = ui
                        .interact(
                            card.response.rect,
                            ui.id().with(("toast", t.id)),
                            egui::Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(hint);
                    // Reading a toast shouldn't race its expiry.
                    if resp.hovered() {
                        t.born = now;
                    }
                    if resp.clicked() {
                        clicked = Some((t.id, t.jump));
                    }
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint_after(REPAINT);
        let (id, jump) = clicked?;
        self.items.retain(|t| t.id != id);
        jump
    }
}
