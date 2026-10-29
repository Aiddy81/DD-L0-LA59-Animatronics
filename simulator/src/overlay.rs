//! Readouts drawn on the 3D model: wing angles, eye colours, the white LED's
//! duty and an arrow for the accelerometer. Each is switched on its own, from
//! the command palette. The values come from the same functions the PCA9685
//! and Motion panels use, so the two can't disagree.

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::{self, Color32, FontId, Stroke};
use bevy_egui::EguiContexts;

use crate::config::SimConfig;
use crate::dock::ViewportRect;
use crate::hw::hw;
use crate::prefs::Prefs;
use crate::scene::{glow_rgb, DroidRoot, Glow, GlowSource, Side, Wing, HOME_FOCUS};
use crate::theme::{ActiveTheme, Palette};
use crate::ui::{eye_rgb_text, servo_angle, white_led_text};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    Wings,
    Eyes,
    Led,
    Accel,
}

impl Overlay {
    pub const ALL: [Overlay; 4] = [Overlay::Wings, Overlay::Eyes, Overlay::Led, Overlay::Accel];

    pub fn label(self) -> &'static str {
        match self {
            Overlay::Wings => "wing angles",
            Overlay::Eyes => "eye colours",
            Overlay::Led => "white LED duty",
            Overlay::Accel => "accelerometer arrow",
        }
    }
}

/// Which overlays are on. The live copy; `save_prefs` writes it back.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Overlays {
    wings: bool,
    eyes: bool,
    led: bool,
    accel: bool,
}

impl Overlays {
    pub fn from_prefs(prefs: &Prefs) -> Self {
        Self {
            wings: prefs.overlay_wings,
            eyes: prefs.overlay_eyes,
            led: prefs.overlay_led,
            accel: prefs.overlay_accel,
        }
    }

    pub fn get(&self, overlay: Overlay) -> bool {
        match overlay {
            Overlay::Wings => self.wings,
            Overlay::Eyes => self.eyes,
            Overlay::Led => self.led,
            Overlay::Accel => self.accel,
        }
    }

    pub fn toggle(&mut self, overlay: Overlay) {
        let flag = match overlay {
            Overlay::Wings => &mut self.wings,
            Overlay::Eyes => &mut self.eyes,
            Overlay::Led => &mut self.led,
            Overlay::Accel => &mut self.accel,
        };
        *flag = !*flag;
    }

    fn any(&self) -> bool {
        Overlay::ALL.into_iter().any(|o| self.get(o))
    }
}

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            draw_overlays
                .after(crate::ui::UiSet)
                .after(crate::scene::orbit_camera)
                .after(crate::scene::apply_viewport),
        );
    }
}

/// `Camera::world_to_viewport` gives logical pixels from the window's corner
/// (Bevy adds the viewport's offset itself); egui wants points. A point
/// is `ppp` physical pixels and a logical pixel is `window_scale` of them.
pub(crate) fn to_egui(logical: Vec2, window_scale: f32, ppp: f32) -> egui::Pos2 {
    let k = window_scale / ppp;
    egui::pos2(logical.x * k, logical.y * k)
}

/// World units of arrow per g, and the most g it is drawn for.
const ARROW_PER_G: f32 = 0.6;
const ARROW_MAX_G: f32 = 3.0;
const PAD: f32 = 4.0;
const DOT: f32 = 3.0;

/// A small readout joined to `anchor` by a leader line.
struct Tag<'a> {
    anchor: egui::Pos2,
    /// -1 or +1: which side of the anchor the label sits.
    side: f32,
    /// How far the label is lifted above the anchor, in points.
    rise: f32,
    text: String,
    /// One colour dot per entry, drawn before the text; the first also marks the anchor.
    dots: &'a [Color32],
}

/// Draws `tag`, moved down past any label in `placed` it would cover (two
/// eyes can line up in some views), and adds it to `placed`.
fn draw_tag(painter: &egui::Painter, palette: &Palette, tag: &Tag, placed: &mut Vec<egui::Rect>) {
    let galley = painter.layout_no_wrap(tag.text.clone(), FontId::proportional(12.0), palette.text);
    let dots_w = tag.dots.len() as f32 * (DOT * 2.0 + 2.0);
    let size = egui::vec2(galley.size().x + dots_w + PAD * 2.0, galley.size().y + PAD);
    let gap = egui::vec2(tag.side * 26.0, -tag.rise);
    let mut min = tag.anchor + gap - egui::vec2(0.0, size.y / 2.0);
    if tag.side < 0.0 {
        min.x -= size.x;
    }
    let mut rect = egui::Rect::from_min_size(min, size);
    for _ in 0..placed.len() {
        match placed.iter().find(|r| r.expand(1.0).intersects(rect)) {
            Some(r) => rect = rect.translate(egui::vec2(0.0, r.max.y + 2.0 - rect.min.y)),
            None => break,
        }
    }
    placed.push(rect);
    let edge = egui::pos2(
        if tag.side < 0.0 {
            rect.max.x
        } else {
            rect.min.x
        },
        rect.center().y,
    );
    let first = tag.dots.first().copied().unwrap_or(palette.muted);
    painter.line_segment([tag.anchor, edge], Stroke::new(palette.stroke_width, first));
    painter.circle_filled(tag.anchor, DOT, first);
    let rounding = egui::CornerRadius::same(palette.rounding);
    painter.rect_filled(rect, rounding, palette.panel);
    painter.rect_stroke(
        rect,
        rounding,
        Stroke::new(palette.stroke_width, palette.border),
        egui::StrokeKind::Inside,
    );
    let mut x = rect.min.x + PAD;
    for dot in tag.dots {
        painter.circle_filled(egui::pos2(x + DOT, rect.center().y), DOT, *dot);
        x += DOT * 2.0 + 2.0;
    }
    let text_pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, palette.text);
}

fn draw_arrow(painter: &egui::Painter, palette: &Palette, from: egui::Pos2, to: egui::Pos2) {
    let stroke = Stroke::new(palette.stroke_width * 2.0, palette.accent);
    painter.line_segment([from, to], stroke);
    let back = (from - to).normalized();
    if !back.x.is_finite() || !back.y.is_finite() {
        return;
    }
    for angle in [0.45_f32, -0.45] {
        let (s, c) = angle.sin_cos();
        let rotated = egui::vec2(back.x * c - back.y * s, back.x * s + back.y * c);
        painter.line_segment([to, to + rotated * 10.0], stroke);
    }
}

type WindowCamera<'w, 's> = (
    Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    Query<'w, 's, (&'static Camera, &'static Transform), With<Camera3d>>,
);

type SceneParts<'w, 's> = (
    Query<'w, 's, (Entity, &'static Wing, &'static GlobalTransform)>,
    Query<'w, 's, (Entity, &'static Glow, &'static GlobalTransform)>,
    Query<'w, 's, &'static GlobalTransform, With<DroidRoot>>,
);

/// The meshes under a part, to find where it really is on screen.
type PartMeshes<'w, 's> = (
    Query<'w, 's, &'static Children>,
    Query<'w, 's, (&'static Aabb, &'static GlobalTransform)>,
);

/// The middle of a part's meshes in world space, weighted by size so a screw
/// doesn't pull it off the shell. The hinge or light node itself can sit well
/// away from what you see (a wing's pivot is at its edge).
fn part_centre(part: Entity, (children, meshes): &PartMeshes) -> Option<Vec3> {
    let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
    for e in std::iter::once(part).chain(children.iter_descendants(part)) {
        if let Ok((aabb, at)) = meshes.get(e) {
            let w = (at.affine().matrix3 * Vec3::from(aabb.half_extents)).length();
            sum += at.transform_point(Vec3::from(aabb.center)) * w;
            weight += w;
        }
    }
    (weight > 0.0).then(|| sum / weight)
}

fn draw_overlays(
    mut contexts: EguiContexts,
    (overlays, viewport, theme, cfg): (
        Res<Overlays>,
        Res<ViewportRect>,
        Res<ActiveTheme>,
        Res<SimConfig>,
    ),
    (wings, glows, root): SceneParts,
    (windows, cams): WindowCamera,
    meshes: PartMeshes,
) {
    let Some(rect) = viewport.rect.filter(|_| overlays.any()) else {
        return;
    };
    let (Ok(window), Ok((camera, cam_transform))) = (windows.single(), cams.single()) else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    // The camera has no parent, so its Transform is its world matrix, and it
    // is this frame's: its GlobalTransform would be a frame behind.
    let cam_world = GlobalTransform::from(*cam_transform);
    let (scale, ppp) = (window.scale_factor(), viewport.ppp);
    let project = |p: Vec3| {
        camera
            .world_to_viewport(&cam_world, p)
            .ok()
            .map(|logical| to_egui(logical, scale, ppp))
    };
    let palette = theme.palette;
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("viewport_overlays"),
        ))
        .with_clip_rect(rect);

    let root_at = root.single().map_or(Vec3::ZERO, |t| t.translation());
    let centre = project(root_at + HOME_FOCUS);
    let mut placed: Vec<egui::Rect> = Vec::new();

    if overlays.get(Overlay::Wings) {
        for (entity, wing, at) in &wings {
            let (channel, name, colour) = match wing.side {
                Side::Right => (0, "R", palette.wing_r),
                Side::Left => (1, "L", palette.wing_l),
            };
            let world = part_centre(entity, &meshes).unwrap_or(at.translation());
            let Some(anchor) = project(world) else {
                continue;
            };
            let text = match servo_angle(&cfg, channel) {
                Some(a) => format!("{name} wing {a:.1}\u{b0}"),
                None => format!("{name} wing -"),
            };
            let tag = Tag {
                anchor,
                side: side_away_from(anchor, centre),
                rise: 0.0,
                text,
                dots: &[colour],
            };
            draw_tag(&painter, palette, &tag, &mut placed);
        }
    }

    let rgb_dots = [palette.ch_r, palette.ch_g, palette.ch_b];
    // The first light of each kind; a model may have several nodes per source.
    let mut seen: Vec<GlowSource> = Vec::new();
    for (entity, glow, at) in &glows {
        let source = glow.source();
        let wanted = match source {
            GlowSource::MainEye | GlowSource::SmallEye => overlays.get(Overlay::Eyes),
            GlowSource::WhiteLed => overlays.get(Overlay::Led),
        };
        if !wanted || seen.contains(&source) {
            continue;
        }
        seen.push(source);
        let world = part_centre(entity, &meshes).unwrap_or(at.translation());
        let Some(anchor) = project(world) else {
            continue;
        };
        let (text, dots, rise): (String, &[Color32], f32) = match source {
            GlowSource::MainEye => (
                format!("main eye  {}", eye_rgb_text(glow_rgb(&cfg, source))),
                &rgb_dots,
                14.0,
            ),
            GlowSource::SmallEye => (
                format!("small eye  {}", eye_rgb_text(glow_rgb(&cfg, source))),
                &rgb_dots,
                -14.0,
            ),
            GlowSource::WhiteLed => {
                let duty = hw().white_duty;
                (
                    white_led_text(duty),
                    std::slice::from_ref(&palette.white_led),
                    28.0,
                )
            }
        };
        let tag = Tag {
            anchor,
            side: side_away_from(anchor, centre),
            rise,
            text,
            dots,
        };
        draw_tag(&painter, palette, &tag, &mut placed);
    }

    if overlays.get(Overlay::Accel) {
        // What the firmware last read, as the Plot's accelerometer band shows:
        // the held value at rest, a nudge or shake while it is being read.
        let g = hw().last_accel;
        // Sensor [x, y, z] is world (x, z, -y); see scene::sensor_dir.
        let world = Vec3::new(g[0], g[2], -g[1]);
        let magnitude = world.length();
        if let Some(dir) = world.try_normalize() {
            let origin = root_at + HOME_FOCUS;
            let tip = origin + dir * magnitude.min(ARROW_MAX_G) * ARROW_PER_G;
            if let (Some(from), Some(to)) = (project(origin), project(tip)) {
                draw_arrow(&painter, palette, from, to);
                let tag = Tag {
                    anchor: to,
                    side: side_away_from(to, centre),
                    rise: 0.0,
                    text: format!("accel {magnitude:.2} g"),
                    dots: std::slice::from_ref(&palette.accent),
                };
                draw_tag(&painter, palette, &tag, &mut placed);
            }
        }
    }
}

/// -1 if the anchor is left of the model's centre on screen, else +1, so a
/// label sits outside the model.
fn side_away_from(anchor: egui::Pos2, centre: Option<egui::Pos2>) -> f32 {
    match centre {
        Some(c) if anchor.x < c.x => -1.0,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_pixels_become_points() {
        // Window scale and egui's pixels-per-point equal: logical px are points.
        assert_eq!(
            to_egui(Vec2::new(110.0, 70.0), 1.5, 1.5),
            egui::pos2(110.0, 70.0)
        );
        // egui zoomed to 3 pixels per point on a 1.5x window: half as many points.
        assert_eq!(
            to_egui(Vec2::new(230.0, 160.0), 1.5, 3.0),
            egui::pos2(115.0, 80.0)
        );
    }

    #[test]
    fn toggling_one_leaves_the_rest() {
        let mut o = Overlays::default();
        assert!(!o.any());
        o.toggle(Overlay::Led);
        assert!(o.get(Overlay::Led) && !o.get(Overlay::Wings));
        o.toggle(Overlay::Led);
        assert!(!o.any());
    }
}
