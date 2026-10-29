//! Colour and spacing tokens, the three themes built from them, and the
//! system that applies the active theme and UI scale to egui.
//!
//! Plot and status colours come from the Okabe-Ito colour-blind-safe set,
//! each shade tuned per theme so lines keep contrast against that theme's
//! background (the light theme uses darker shades than the dark ones).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::{WindowTheme, WindowThemeChanged};
use bevy_egui::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, StrokeKind, TextStyle, Theme,
};
use bevy_egui::{EguiContext, EguiContextSettings};
use serde::{Deserialize, Serialize};

use crate::dock::Detached;

/// What the user picked in the toolbar; `System` follows the OS.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
    HighContrast,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 4] = [
        ThemeChoice::System,
        ThemeChoice::Light,
        ThemeChoice::Dark,
        ThemeChoice::HighContrast,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Follow system",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::HighContrast => "High contrast",
        }
    }
}

/// A theme after `System` has been looked up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResolvedTheme {
    Light,
    Dark,
    HighContrast,
}

/// Every colour and metric the UI and the 3D scene take from the theme.
pub struct Palette {
    // Surfaces and text.
    pub panel: Color32,
    pub surface: Color32,
    pub widget: Color32,
    pub text: Color32,
    pub border: Color32,
    // Roles.
    pub accent: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub link: Color32,
    pub muted: Color32,
    // 3D scene, at full room brightness.
    pub scene_bg: Color32,
    pub floor: Color32,
    // Plot series.
    pub wing_r: Color32,
    pub wing_l: Color32,
    pub ch_r: Color32,
    pub ch_g: Color32,
    pub ch_b: Color32,
    pub white_led: Color32,
    pub accel_x: Color32,
    pub accel_y: Color32,
    pub accel_z: Color32,
    pub heap_ok: Color32,
    pub heap_high: Color32,
    pub heap_over: Color32,
    // Plot event markers.
    pub marker_motion: Color32,
    pub marker_sound: Color32,
    pub marker_idle: Color32,
    pub marker_restart: Color32,
    // Accelerometer threshold and clip lines.
    pub line_nudge: Color32,
    pub line_animation: Color32,
    pub line_hard: Color32,
    pub line_clip: Color32,
    // Metrics.
    pub item_spacing: egui::Vec2,
    pub rounding: u8,
    pub stroke_width: f32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const LIGHT: Palette = Palette {
    panel: rgb(0xF3F3F5),
    surface: rgb(0xFFFFFF),
    widget: rgb(0xE4E5E9),
    text: rgb(0x1A1B1E),
    border: rgb(0xC4C6CC),
    accent: rgb(0x0072B2),
    ok: rgb(0x007A5A),
    warn: rgb(0x9A6700),
    error: rgb(0xB83A00),
    link: rgb(0x0059A0),
    muted: rgb(0x5C6068),
    scene_bg: rgb(0xB4BCC8),
    floor: rgb(0x7C828D),
    wing_r: rgb(0xC77700),
    wing_l: rgb(0x0072B2),
    ch_r: rgb(0xD55E00),
    ch_g: rgb(0x009E73),
    ch_b: rgb(0x2B5CC8),
    white_led: rgb(0x6B6F78),
    accel_x: rgb(0xB04A86),
    accel_y: rgb(0x1F8A5B),
    accel_z: rgb(0x111111),
    heap_ok: rgb(0x007A5A),
    heap_high: rgb(0x9A6700),
    heap_over: rgb(0xB83A00),
    marker_motion: rgb(0xB59B00),
    marker_sound: rgb(0xA64CA6),
    marker_idle: rgb(0x00875F),
    marker_restart: rgb(0x6B6F78),
    line_nudge: rgb(0xA68A00),
    line_animation: rgb(0xC26A00),
    line_hard: rgb(0xC4301E),
    line_clip: rgb(0x6B6F78),
    item_spacing: egui::vec2(8.0, 6.0),
    rounding: 4,
    stroke_width: 1.0,
};

const DARK: Palette = Palette {
    panel: rgb(0x1E1F22),
    surface: rgb(0x16171A),
    widget: rgb(0x2E3036),
    text: rgb(0xE6E7EA),
    border: rgb(0x3A3C42),
    accent: rgb(0x56B4E9),
    ok: rgb(0x2FBF8F),
    warn: rgb(0xEBA81A),
    error: rgb(0xF2704A),
    link: rgb(0x6CB6F0),
    muted: rgb(0x9A9EA6),
    scene_bg: rgb(0x121419),
    floor: rgb(0x2E3036),
    wing_r: rgb(0xE69F00),
    wing_l: rgb(0x56B4E9),
    ch_r: rgb(0xE8692B),
    ch_g: rgb(0x2FBF8F),
    ch_b: rgb(0x5B9BE8),
    white_led: rgb(0xDCDCDC),
    accel_x: rgb(0xCC79A7),
    accel_y: rgb(0x3FB58C),
    accel_z: rgb(0xFFFFFF),
    heap_ok: rgb(0x2FBF8F),
    heap_high: rgb(0xEBA81A),
    heap_over: rgb(0xF2704A),
    marker_motion: rgb(0xF0E442),
    marker_sound: rgb(0xCC79A7),
    marker_idle: rgb(0x2FBF8F),
    marker_restart: rgb(0x9A9EA6),
    line_nudge: rgb(0xF0E442),
    line_animation: rgb(0xE69F00),
    line_hard: rgb(0xF2704A),
    line_clip: rgb(0x9A9EA6),
    item_spacing: egui::vec2(8.0, 6.0),
    rounding: 4,
    stroke_width: 1.0,
};

const HIGH_CONTRAST: Palette = Palette {
    panel: rgb(0x000000),
    surface: rgb(0x000000),
    widget: rgb(0x1A1A1A),
    text: rgb(0xFFFFFF),
    border: rgb(0xFFFFFF),
    accent: rgb(0xFFFF00),
    ok: rgb(0x00E0A0),
    warn: rgb(0xFFB000),
    error: rgb(0xFF6A4D),
    link: rgb(0x66CCFF),
    muted: rgb(0xC8C8C8),
    scene_bg: rgb(0x05060A),
    floor: rgb(0x2E3036),
    wing_r: rgb(0xFFB000),
    wing_l: rgb(0x66CCFF),
    ch_r: rgb(0xFF6A4D),
    ch_g: rgb(0x00E0A0),
    ch_b: rgb(0x6699FF),
    white_led: rgb(0xFFFFFF),
    accel_x: rgb(0xFF8AD0),
    accel_y: rgb(0x80FF80),
    accel_z: rgb(0xFFFFFF),
    heap_ok: rgb(0x00E0A0),
    heap_high: rgb(0xFFB000),
    heap_over: rgb(0xFF6A4D),
    marker_motion: rgb(0xFFFF00),
    marker_sound: rgb(0xFF8AD0),
    marker_idle: rgb(0x00E0A0),
    marker_restart: rgb(0xC8C8C8),
    line_nudge: rgb(0xFFFF00),
    line_animation: rgb(0xFFB000),
    line_hard: rgb(0xFF6A4D),
    line_clip: rgb(0xC8C8C8),
    item_spacing: egui::vec2(10.0, 8.0),
    rounding: 2,
    stroke_width: 2.0,
};

impl ResolvedTheme {
    fn palette(self) -> &'static Palette {
        match self {
            ResolvedTheme::Light => &LIGHT,
            ResolvedTheme::Dark => &DARK,
            ResolvedTheme::HighContrast => &HIGH_CONTRAST,
        }
    }

    /// Which of egui's two base styles this theme is built on.
    fn egui_theme(self) -> Theme {
        match self {
            ResolvedTheme::Light => Theme::Light,
            ResolvedTheme::Dark | ResolvedTheme::HighContrast => Theme::Dark,
        }
    }
}

/// UI scale limits, also applied to a value loaded from the prefs file.
pub const SCALE_MIN: f32 = 0.75;
pub const SCALE_MAX: f32 = 2.0;
pub const SCALE_STEP: f32 = 0.1;

/// The theme in force and the UI scale. Written only on a real change so the
/// apply system can lean on change detection.
#[derive(Resource)]
pub struct ActiveTheme {
    pub choice: ThemeChoice,
    pub resolved: ResolvedTheme,
    pub palette: &'static Palette,
    pub scale: f32,
    /// Smooth transitions on; off sets egui's animation time to zero.
    animations: bool,
    /// What the OS currently prefers; used while `choice` is `System`.
    os_dark: bool,
}

impl ActiveTheme {
    pub fn new(choice: ThemeChoice, scale: f32) -> Self {
        let os_dark = os_prefers_dark();
        let mut theme = Self {
            choice,
            resolved: ResolvedTheme::Dark,
            palette: &DARK,
            scale: clamp_scale(scale),
            animations: true,
            os_dark,
        };
        theme.resolve();
        theme
    }

    /// Starts with animations as saved in the prefs.
    pub fn with_animations(mut self, on: bool) -> Self {
        self.animations = on;
        self
    }

    pub fn animations(&self) -> bool {
        self.animations
    }

    pub fn set_animations(&mut self, on: bool) {
        self.animations = on;
    }

    fn resolve(&mut self) {
        self.resolved = match self.choice {
            ThemeChoice::System if self.os_dark => ResolvedTheme::Dark,
            ThemeChoice::System | ThemeChoice::Light => ResolvedTheme::Light,
            ThemeChoice::Dark => ResolvedTheme::Dark,
            ThemeChoice::HighContrast => ResolvedTheme::HighContrast,
        };
        self.palette = self.resolved.palette();
    }

    /// Sets the pick; call only when it differs so change detection stays honest.
    pub fn set_choice(&mut self, choice: ThemeChoice) {
        self.choice = choice;
        self.resolve();
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = clamp_scale(scale);
    }

    fn set_os_dark(&mut self, dark: bool) {
        self.os_dark = dark;
        self.resolve();
    }
}

/// Keeps a scale inside the limits and rounds it to a whole percent, so
/// repeated Ctrl+= / Ctrl+- presses and the slider never accumulate float drift.
pub fn clamp_scale(scale: f32) -> f32 {
    let scale = if scale.is_finite() { scale } else { 1.0 };
    ((scale * 100.0).round() / 100.0).clamp(SCALE_MIN, SCALE_MAX)
}

/// The scale one keyboard step (`dir` = +1 or -1) from `scale`, on the 10% grid.
pub fn step_scale(scale: f32, dir: i32) -> f32 {
    clamp_scale(((scale / SCALE_STEP).round() + dir as f32) * SCALE_STEP)
}

/// Windows' "app mode" setting; any failure or another OS gives dark, the
/// look the simulator had before themes existed.
#[cfg(windows)]
fn os_prefers_dark() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain([0]).collect() };
    let subkey = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = wide("AppsUseLightTheme");
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both names are NUL-terminated; `data` and `size` are valid for
    // the 4 bytes a REG_DWORD needs, and the type out pointer may be null.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    // 0 is ERROR_SUCCESS. 1 means light mode.
    status != 0 || data == 0
}

#[cfg(not(windows))]
fn os_prefers_dark() -> bool {
    true
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (follow_os_theme, apply_theme).chain());
    }
}

/// Re-resolves `System` when the OS flips between light and dark.
fn follow_os_theme(mut events: MessageReader<WindowThemeChanged>, mut theme: ResMut<ActiveTheme>) {
    let Some(latest) = events.read().last() else {
        return;
    };
    let dark = latest.theme == WindowTheme::Dark;
    if theme.os_dark != dark {
        theme.set_os_dark(dark);
    }
}

/// Pushes the style and zoom into egui, once at start and then only when the
/// resolved theme or the scale changes, not every frame.
fn apply_theme(
    mut contexts: Query<(
        Entity,
        &mut EguiContext,
        &mut EguiContextSettings,
        Option<&ChildOf>,
    )>,
    detached: Query<&Detached>,
    theme: Res<ActiveTheme>,
    mut applied: Local<HashMap<Entity, (ResolvedTheme, f32, bool)>>,
) {
    // Windows that are gone must not keep a stale entry, or a reused entity id
    // would be taken for already themed.
    applied.retain(|entity, _| contexts.contains(*entity));
    for (entity, mut context, mut settings, parent) in &mut contexts {
        // Every window has its own scale, so a detached one can be zoomed alone.
        // A context lives on a camera; a detached window's camera is its child.
        let scale = parent
            .and_then(|p| detached.get(p.parent()).ok())
            .map_or(theme.scale, |d| d.scale);
        let wanted = (theme.resolved, scale, theme.animations);
        if applied.get(&entity) == Some(&wanted) {
            continue;
        }
        let ctx = context.get_mut();
        apply(ctx, theme.palette, theme.resolved, theme.animations);
        // bevy_egui resets pixels-per-point every frame from this setting, so
        // an egui zoom factor would be overwritten; the scale has to live here.
        if settings.scale_factor != scale {
            settings.scale_factor = scale;
        }
        // Ctrl+= / Ctrl+- / Ctrl+0 are handled by the simulator so the scale is saved.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        applied.insert(entity, wanted);
    }
}

/// Builds the egui visuals and style for a palette and installs them.
pub fn apply(ctx: &egui::Context, palette: &Palette, resolved: ResolvedTheme, animations: bool) {
    let egui_theme = resolved.egui_theme();
    ctx.set_theme(egui_theme);
    ctx.set_style_of(egui_theme, style(palette, resolved, animations));
}

fn style(p: &Palette, resolved: ResolvedTheme, animations: bool) -> egui::Style {
    let mut style = egui::Style {
        visuals: visuals(p, resolved),
        ..egui::Style::default()
    };
    if !animations {
        style.animation_time = 0.0;
    }
    style.spacing.item_spacing = p.item_spacing;
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = 24.0;
    style.spacing.window_margin = Margin::same(10);
    style.spacing.indent = 18.0;
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
    ]
    .into();
    style
}

/// Where `focus_ring` remembers whether the last key was a navigation key.
const FOCUS_VISIBLE: &str = "focus_visible";

/// Makes the focus ring show, as after a keyboard move that egui doesn't see
/// as one (F6 hands focus to a tab itself).
pub fn show_focus(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(FOCUS_VISIBLE), true));
}

/// Draws a ring round the widget that has keyboard focus, but only after the
/// keyboard moved focus there (like CSS `:focus-visible`): a click hides it.
/// Call once per context, after everything is drawn. Two strokes, so it shows
/// on both a plain widget and an accent-filled (selected) one.
pub fn focus_ring(ctx: &egui::Context, palette: &Palette) {
    let id = egui::Id::new(FOCUS_VISIBLE);
    let (keyed, clicked) = ctx.input(|i| {
        let keyed = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::Tab
                        | egui::Key::ArrowUp
                        | egui::Key::ArrowDown
                        | egui::Key::ArrowLeft
                        | egui::Key::ArrowRight
                        | egui::Key::F6,
                    pressed: true,
                    ..
                }
            )
        });
        let clicked = i
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. }));
        (keyed, clicked)
    });
    let visible = ctx.data_mut(|d| {
        let mut visible = d.get_temp::<bool>(id).unwrap_or(false);
        if clicked {
            visible = false;
        }
        if keyed {
            visible = true;
        }
        d.insert_temp(id, visible);
        visible
    });
    if !visible {
        return;
    }
    let Some(resp) = ctx
        .memory(|m| m.focused())
        .and_then(|focused| ctx.read_response(focused))
    else {
        return;
    };
    let painter = ctx
        .layer_painter(resp.layer_id)
        .with_clip_rect(resp.interact_rect.expand(4.0));
    let radius = CornerRadius::same(palette.rounding.saturating_add(2));
    painter.rect_stroke(
        resp.rect.expand(2.0),
        radius,
        Stroke::new(2.0, palette.accent),
        StrokeKind::Outside,
    );
    painter.rect_stroke(
        resp.rect.expand(1.0),
        CornerRadius::same(palette.rounding),
        Stroke::new(1.0, palette.surface),
        StrokeKind::Outside,
    );
}

fn visuals(p: &Palette, resolved: ResolvedTheme) -> egui::Visuals {
    let mut v = match resolved.egui_theme() {
        Theme::Light => egui::Visuals::light(),
        Theme::Dark => egui::Visuals::dark(),
    };
    let radius = CornerRadius::same(p.rounding);
    let stroke = |color| Stroke::new(p.stroke_width, color);
    // Hover and press shift the fill towards the text colour, which works in
    // any theme without more tokens.
    let hover_fill = p.widget.lerp_to_gamma(p.text, 0.12);
    let active_fill = p.widget.lerp_to_gamma(p.text, 0.25);

    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = stroke(p.border);
    v.window_corner_radius = radius;
    v.menu_corner_radius = radius;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.panel.lerp_to_gamma(p.text, 0.05);
    v.code_bg_color = p.widget;
    v.hyperlink_color = p.link;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.accent.linear_multiply(0.35);
    // egui draws a selected button's text in this colour, over the accent tint,
    // so it must be the text colour: the accent itself would vanish into the
    // tint.
    v.selection.stroke = Stroke::new(p.stroke_width.max(1.5), p.text);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    // Panel and separator lines stay subtle except in high contrast.
    w.noninteractive.bg_stroke = stroke(p.border);
    w.noninteractive.fg_stroke = stroke(p.text);
    w.inactive.bg_fill = p.widget;
    w.inactive.weak_bg_fill = p.widget;
    w.inactive.bg_stroke = stroke(p.border);
    w.inactive.fg_stroke = stroke(p.text);
    w.hovered.bg_fill = hover_fill;
    w.hovered.weak_bg_fill = hover_fill;
    w.hovered.bg_stroke = stroke(p.accent);
    w.hovered.fg_stroke = stroke(p.text);
    // The pressed look; the keyboard focus ring is drawn by `focus_ring`.
    w.active.bg_fill = active_fill;
    w.active.weak_bg_fill = active_fill;
    w.active.bg_stroke = Stroke::new(p.stroke_width.max(1.5), p.accent);
    w.active.fg_stroke = stroke(p.text);
    w.open.bg_fill = hover_fill;
    w.open.weak_bg_fill = hover_fill;
    w.open.bg_stroke = stroke(p.border);
    w.open.fg_stroke = stroke(p.text);
    for state in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        state.corner_radius = radius;
    }
    v
}
