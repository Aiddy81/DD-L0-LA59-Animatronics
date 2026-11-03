//! Colour and spacing tokens, the three themes built from them, and the
//! system that applies the active theme and UI scale to egui.
//!
//! Plot and status colours come from the Okabe-Ito colour-blind-safe set,
//! each shade tuned per theme so lines keep contrast against that theme's
//! background (the light theme uses darker shades than the dark ones).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::WindowThemeChanged;
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
    /// Separators and other decorative lines; may be faint.
    pub border: Color32,
    /// Control and window edges: at least 3:1 against the panel (WCAG 1.4.11).
    pub outline: Color32,
    /// Fill of a selected item, and the text drawn on it; the fill is 3:1
    /// against the panel and the text 4.5:1 against the fill.
    pub selection: Color32,
    pub selected_text: Color32,
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
    outline: rgb(0x878A91),
    selection: rgb(0x3D92C3),
    selected_text: rgb(0x1A1B1E),
    accent: rgb(0x006CAA),
    ok: rgb(0x007656),
    warn: rgb(0x8C5E00),
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
    marker_motion: rgb(0xA08800),
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
    outline: rgb(0x71747C),
    selection: rgb(0x3D718F),
    selected_text: rgb(0xFFFFFF),
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
    outline: rgb(0xFFFFFF),
    selection: rgb(0x666600),
    selected_text: rgb(0xFFFFFF),
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

/// A palette for unit tests that draw UI.
#[cfg(test)]
pub fn test_palette() -> &'static Palette {
    &DARK
}

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
    /// Whether the user picked the scale / animations; if not, they follow
    /// Windows and aren't saved.
    scale_chosen: bool,
    animations_chosen: bool,
    /// What Windows currently asks for.
    os: OsPrefs,
}

impl ActiveTheme {
    /// A saved scale or animations setting wins; `None` follows Windows.
    pub fn new(choice: ThemeChoice, scale: Option<f32>, animations: Option<bool>) -> Self {
        Self::with_os(choice, scale, animations, read_os())
    }

    fn with_os(
        choice: ThemeChoice,
        scale: Option<f32>,
        animations: Option<bool>,
        os: OsPrefs,
    ) -> Self {
        let mut theme = Self {
            choice,
            resolved: ResolvedTheme::Dark,
            palette: &DARK,
            scale: clamp_scale(scale.unwrap_or(os.text_scale)),
            animations: animations.unwrap_or(os.animations),
            scale_chosen: scale.is_some(),
            animations_chosen: animations.is_some(),
            os,
        };
        theme.resolve();
        theme
    }

    pub fn animations(&self) -> bool {
        self.animations
    }

    pub fn set_animations(&mut self, on: bool) {
        self.animations = on;
        self.animations_chosen = true;
    }

    /// The scale and animations to save: only what the user picked.
    pub fn saved_scale(&self) -> Option<f32> {
        self.scale_chosen.then_some(self.scale)
    }

    pub fn saved_animations(&self) -> Option<bool> {
        self.animations_chosen.then_some(self.animations)
    }

    fn resolve(&mut self) {
        self.resolved = resolve(self.choice, &self.os);
        self.palette = self.resolved.palette();
    }

    /// Sets the pick; call only when it differs so change detection stays honest.
    pub fn set_choice(&mut self, choice: ThemeChoice) {
        self.choice = choice;
        self.resolve();
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = clamp_scale(scale);
        self.scale_chosen = true;
    }

    /// Drops the picked scale, so it follows the Windows text size again.
    pub fn reset_scale(&mut self) {
        self.scale = self.os.text_scale;
        self.scale_chosen = false;
    }

    /// Drops the picked animations setting, so it follows Windows' Animation effects again.
    pub fn reset_animations(&mut self) {
        self.animations = self.os.animations;
        self.animations_chosen = false;
    }

    fn set_os(&mut self, os: OsPrefs) {
        self.os = os;
        if !self.scale_chosen {
            self.scale = os.text_scale;
        }
        if !self.animations_chosen {
            self.animations = os.animations;
        }
        self.resolve();
    }
}

/// The Windows settings the simulator follows.
#[derive(Clone, Copy, PartialEq, Debug)]
struct OsPrefs {
    /// Apps use dark mode.
    dark: bool,
    /// A contrast theme is on.
    high_contrast: bool,
    /// "Animation effects" is on.
    animations: bool,
    /// The "Text size" setting as a UI scale.
    text_scale: f32,
}

/// What another OS, or a failed read, gives: the look the simulator had
/// before it followed Windows.
impl Default for OsPrefs {
    fn default() -> Self {
        Self {
            dark: true,
            high_contrast: false,
            animations: true,
            text_scale: 1.0,
        }
    }
}

/// The theme a pick stands for. `System` honours a contrast theme first, as
/// Windows apps do; an explicit pick ignores the OS.
fn resolve(choice: ThemeChoice, os: &OsPrefs) -> ResolvedTheme {
    match choice {
        ThemeChoice::System if os.high_contrast => ResolvedTheme::HighContrast,
        ThemeChoice::System if os.dark => ResolvedTheme::Dark,
        ThemeChoice::System | ThemeChoice::Light => ResolvedTheme::Light,
        ThemeChoice::Dark => ResolvedTheme::Dark,
        ThemeChoice::HighContrast => ResolvedTheme::HighContrast,
    }
}

/// Windows' text size (a percentage, 100–225) as a UI scale; missing or zero
/// means unset.
fn text_scale(percent: Option<u32>) -> f32 {
    match percent {
        Some(p) if p > 0 => clamp_scale(p as f32 / 100.0),
        _ => 1.0,
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

/// Reads the Windows settings; one that can't be read keeps its default.
#[cfg(windows)]
fn read_os() -> OsPrefs {
    let defaults = OsPrefs::default();
    OsPrefs {
        // 1 means light mode.
        dark: reg_dword(
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "AppsUseLightTheme",
        )
        .map_or(defaults.dark, |light| light == 0),
        high_contrast: os_high_contrast(),
        animations: os_animations().unwrap_or(defaults.animations),
        text_scale: text_scale(reg_dword(
            r"Software\Microsoft\Accessibility",
            "TextScaleFactor",
        )),
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn read_os() -> OsPrefs {
    OsPrefs::default()
}

/// The GNOME settings, as last read. `gsettings` is a subprocess, too slow
/// for the frame loop that polls this, so the first call (at startup) reads
/// it directly and then a background thread keeps the copy current.
#[cfg(target_os = "linux")]
fn read_os() -> OsPrefs {
    use std::sync::{Mutex, OnceLock};
    static LATEST: OnceLock<Mutex<OsPrefs>> = OnceLock::new();
    let mut found = false;
    let latest = LATEST.get_or_init(|| {
        let (os, any) = read_gsettings();
        found = any;
        Mutex::new(os)
    });
    // No GNOME settings at all (another desktop): nothing to follow.
    if found {
        let poll = move || loop {
            std::thread::sleep(std::time::Duration::from_secs_f64(OS_POLL_S));
            let (os, _) = read_gsettings();
            if let Ok(mut l) = latest.lock() {
                *l = os;
            }
        };
        // Without the thread the startup values just stay.
        let _ = std::thread::Builder::new()
            .name("os-settings".into())
            .spawn(poll);
    }
    latest.lock().map(|l| *l).unwrap_or_default()
}

/// The GNOME settings, and whether any of them could be read.
#[cfg(target_os = "linux")]
fn read_gsettings() -> (OsPrefs, bool) {
    fn get(schema: &str, key: &str) -> Option<String> {
        let out = crate::pyhost::command("gsettings")
            .args(["get", schema, key])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }
    const UI: &str = "org.gnome.desktop.interface";
    let values = [
        get(UI, "color-scheme"),
        get("org.gnome.desktop.a11y.interface", "high-contrast"),
        get(UI, "enable-animations"),
        get(UI, "text-scaling-factor"),
    ];
    let [scheme, contrast, animations, scale] = values.each_ref().map(Option::as_deref);
    let any = values.iter().any(Option::is_some);
    (parse_gsettings(scheme, contrast, animations, scale), any)
}

/// GNOME's settings as `gsettings get` prints them; one that is missing or
/// unrecognised keeps its default.
#[cfg(any(target_os = "linux", test))]
fn parse_gsettings(
    color_scheme: Option<&str>,
    high_contrast: Option<&str>,
    animations: Option<&str>,
    text_scaling: Option<&str>,
) -> OsPrefs {
    let defaults = OsPrefs::default();
    let flag = |v: Option<&str>| match v.map(str::trim) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    };
    let dark = match color_scheme.map(str::trim) {
        Some("'prefer-dark'") => Some(true),
        // 'default' is GNOME's light look.
        Some("'prefer-light'" | "'default'") => Some(false),
        _ => None,
    };
    let percent = text_scaling
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|f| f.is_finite() && *f > 0.0)
        .map(|f| (f * 100.0).round() as u32);
    OsPrefs {
        dark: dark.unwrap_or(defaults.dark),
        high_contrast: flag(high_contrast).unwrap_or(defaults.high_contrast),
        animations: flag(animations).unwrap_or(defaults.animations),
        text_scale: text_scale(percent),
    }
}

/// Whether a Windows contrast theme is on.
#[cfg(windows)]
fn os_high_contrast() -> bool {
    use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETHIGHCONTRAST};

    let mut hc = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    // SAFETY: `hc` is a HIGHCONTRASTW with cbSize set, as this action needs;
    // Windows only writes to it.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            (&mut hc as *mut HIGHCONTRASTW).cast(),
            0,
        )
    };
    ok != 0 && hc.dwFlags & HCF_HIGHCONTRASTON != 0
}

/// Windows' "Animation effects" setting.
#[cfg(windows)]
fn os_animations() -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
    };

    let mut on: i32 = 1;
    // SAFETY: this action writes one BOOL (an i32) through the pointer.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut on as *mut i32).cast(),
            0,
        )
    };
    (ok != 0).then_some(on != 0)
}

/// A REG_DWORD under HKEY_CURRENT_USER, or `None` if it can't be read.
#[cfg(windows)]
fn reg_dword(subkey: &str, value: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain([0]).collect() };
    let subkey = wide(subkey);
    let value = wide(value);
    let mut data: u32 = 0;
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
    // 0 is ERROR_SUCCESS.
    (status == 0).then_some(data)
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (follow_os_settings, apply_theme).chain());
    }
}

/// How often the Windows settings are re-read.
const OS_POLL_S: f64 = 2.0;

/// Follows the Windows settings. winit reports a theme change only when its
/// own light/dark verdict flips, so a contrast theme turned on in light mode,
/// "Animation effects" and the text size raise no event: they are polled.
fn follow_os_settings(
    mut events: MessageReader<WindowThemeChanged>,
    time: Res<Time<Real>>,
    mut next_read: Local<f64>,
    mut theme: ResMut<ActiveTheme>,
) {
    let event = events.read().count() > 0;
    let now = time.elapsed_secs_f64();
    if !event && now < *next_read {
        return;
    }
    *next_read = now + OS_POLL_S;
    let os = read_os();
    // Written only on a real change so the apply system's change check stays quiet.
    if theme.os != os {
        theme.set_os(os);
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
    // The ring sits up to 4 px outside the widget; a widget that touches the
    // window's edge (the 3D view) gets it pulled in so no side is lost.
    let rect = resp.rect.intersect(ctx.content_rect().shrink(4.0));
    let radius = CornerRadius::same(palette.rounding.saturating_add(2));
    painter.rect_stroke(
        rect.expand(2.0),
        radius,
        Stroke::new(2.0, palette.accent),
        StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect.expand(1.0),
        CornerRadius::same(palette.rounding),
        Stroke::new(1.0, palette.surface),
        StrokeKind::Outside,
    );
}

/// egui's weak and disabled text opacity, raised from its 0.6 and 0.5.
const DIM_TEXT_ALPHA: f32 = 0.65;

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
    v.window_stroke = stroke(p.outline);
    v.window_corner_radius = radius;
    v.menu_corner_radius = radius;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.panel.lerp_to_gamma(p.text, 0.05);
    v.code_bg_color = p.widget;
    v.hyperlink_color = p.link;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.selection;
    // egui draws a selected button's text in this colour, over the selection
    // fill, and a focused text field's border in it.
    v.selection.stroke = Stroke::new(p.stroke_width.max(1.5), p.selected_text);
    // Dimmed just enough that weak and disabled text still read at 4.5:1.
    v.weak_text_alpha = DIM_TEXT_ALPHA;
    v.disabled_alpha = DIM_TEXT_ALPHA;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    // Panel and separator lines stay subtle except in high contrast.
    w.noninteractive.bg_stroke = stroke(p.border);
    w.noninteractive.fg_stroke = stroke(p.text);
    w.inactive.bg_fill = p.widget;
    w.inactive.weak_bg_fill = p.widget;
    w.inactive.bg_stroke = stroke(p.outline);
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
    w.open.bg_stroke = stroke(p.outline);
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

#[cfg(test)]
mod tests {
    use super::*;

    const THEMES: [ResolvedTheme; 3] = [
        ResolvedTheme::Light,
        ResolvedTheme::Dark,
        ResolvedTheme::HighContrast,
    ];

    /// WCAG 2.2 relative luminance of an opaque sRGB colour.
    fn luminance(c: Color32) -> f64 {
        let channel = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// `fg` painted over the opaque `bg`. egui keeps colours premultiplied
    /// and blends in gamma space, so this is what lands on screen.
    fn over(fg: Color32, bg: Color32) -> Color32 {
        let keep = 1.0 - f32::from(fg.a()) / 255.0;
        let mix = |f: u8, b: u8| (f32::from(f) + f32::from(b) * keep).round().min(255.0) as u8;
        Color32::from_rgb(
            mix(fg.r(), bg.r()),
            mix(fg.g(), bg.g()),
            mix(fg.b(), bg.b()),
        )
    }

    /// Every colour pair the UI draws that WCAG 2.2 AA covers, taken from the
    /// visuals egui is given rather than the palette alone. Text needs 4.5:1
    /// (1.4.3); focus rings, borders, selection and plot lines need 3:1 (1.4.11).
    fn contrast_report(resolved: ResolvedTheme) -> Vec<(String, f64, f64)> {
        const TEXT: f64 = 4.5;
        const PART: f64 = 3.0;
        let p = resolved.palette();
        let v = style(p, resolved, true).visuals;
        let w = &v.widgets;
        let mut out = Vec::new();
        let mut pair = |what: &str, fg: Color32, bg: Color32, need: f64| {
            let bg = over(bg, v.panel_fill);
            out.push((what.to_owned(), contrast(over(fg, bg), bg), need));
        };

        let backgrounds = [
            ("panel", v.panel_fill),
            ("window", v.window_fill),
            ("field", v.extreme_bg_color),
            ("striped row", v.faint_bg_color),
        ];
        let texts = [
            ("text", v.text_color()),
            ("weak text", v.weak_text_color()),
            ("disabled text", v.disable(v.text_color())),
            ("link", v.hyperlink_color),
            ("warn", v.warn_fg_color),
            ("error", v.error_fg_color),
            ("ok", p.ok),
            ("muted", p.muted),
            ("accent text", p.accent),
        ];
        for (bg_name, bg) in backgrounds {
            for (fg_name, fg) in texts {
                pair(&format!("{fg_name} on {bg_name}"), fg, bg, TEXT);
            }
        }
        for (name, state) in [
            ("inactive", &w.inactive),
            ("hovered", &w.hovered),
            ("active", &w.active),
            ("open", &w.open),
        ] {
            pair(
                &format!("{name} button text"),
                state.fg_stroke.color,
                state.weak_bg_fill,
                TEXT,
            );
            pair(
                &format!("{name} button text on bg_fill"),
                state.fg_stroke.color,
                state.bg_fill,
                TEXT,
            );
        }
        // A disabled widget is painted at reduced opacity, fill and text alike.
        let disabled_fill = over(v.disable(w.inactive.weak_bg_fill), v.panel_fill);
        pair(
            "disabled button text",
            v.disable(w.inactive.fg_stroke.color),
            disabled_fill,
            TEXT,
        );
        pair(
            "selected text",
            v.selection.stroke.color,
            v.selection.bg_fill,
            TEXT,
        );

        pair(
            "selection on panel",
            v.selection.bg_fill,
            v.panel_fill,
            PART,
        );
        for (bg_name, bg) in [
            ("panel", v.panel_fill),
            ("field", v.extreme_bg_color),
            ("button", w.inactive.weak_bg_fill),
        ] {
            pair(&format!("focus ring on {bg_name}"), p.accent, bg, PART);
        }
        for (name, stroke) in [
            ("button border", w.inactive.bg_stroke),
            ("open button border", w.open.bg_stroke),
            ("hovered border", w.hovered.bg_stroke),
            ("pressed border", w.active.bg_stroke),
            ("window border", v.window_stroke),
        ] {
            pair(
                &format!("{name} on panel"),
                stroke.color,
                v.panel_fill,
                PART,
            );
            pair(
                &format!("{name} on field"),
                stroke.color,
                v.extreme_bg_color,
                PART,
            );
        }
        // egui_plot fills the plot with the field colour.
        for (name, line) in [
            ("wing_r", p.wing_r),
            ("wing_l", p.wing_l),
            ("ch_r", p.ch_r),
            ("ch_g", p.ch_g),
            ("ch_b", p.ch_b),
            ("white_led", p.white_led),
            ("accel_x", p.accel_x),
            ("accel_y", p.accel_y),
            ("accel_z", p.accel_z),
            ("heap_ok", p.heap_ok),
            ("heap_high", p.heap_high),
            ("heap_over", p.heap_over),
            ("marker_motion", p.marker_motion),
            ("marker_sound", p.marker_sound),
            ("marker_idle", p.marker_idle),
            ("marker_restart", p.marker_restart),
            ("line_nudge", p.line_nudge),
            ("line_animation", p.line_animation),
            ("line_hard", p.line_hard),
            ("line_clip", p.line_clip),
        ] {
            pair(&format!("plot {name}"), line, v.extreme_bg_color, PART);
        }
        out
    }

    #[test]
    fn every_theme_meets_wcag_aa_contrast() {
        let mut failures = Vec::new();
        for resolved in THEMES {
            for (what, ratio, need) in contrast_report(resolved) {
                // Printed for the review table: `ddl0-sim-tests theme --nocapture`.
                println!("{resolved:?}: {what} {ratio:.2} (need {need})");
                if ratio < need {
                    failures.push(format!("{resolved:?}: {what} {ratio:.2} < {need}"));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn contrast_maths_matches_wcag() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(rgb(0x777777), Color32::WHITE) - 4.48).abs() < 0.01);
        let half_black = Color32::from_black_alpha(128);
        assert_eq!(over(half_black, Color32::WHITE), rgb(0x7F7F7F));
    }

    fn os(dark: bool, high_contrast: bool) -> OsPrefs {
        OsPrefs {
            dark,
            high_contrast,
            ..OsPrefs::default()
        }
    }

    #[test]
    fn system_follows_contrast_theme_then_dark_mode() {
        use ResolvedTheme as R;
        assert_eq!(
            resolve(ThemeChoice::System, &os(true, true)),
            R::HighContrast
        );
        assert_eq!(
            resolve(ThemeChoice::System, &os(false, true)),
            R::HighContrast
        );
        assert_eq!(resolve(ThemeChoice::System, &os(true, false)), R::Dark);
        assert_eq!(resolve(ThemeChoice::System, &os(false, false)), R::Light);
        // A picked theme ignores Windows.
        assert_eq!(resolve(ThemeChoice::Light, &os(true, true)), R::Light);
        assert_eq!(resolve(ThemeChoice::Dark, &os(false, true)), R::Dark);
        assert_eq!(
            resolve(ThemeChoice::HighContrast, &os(false, false)),
            R::HighContrast
        );
    }

    #[test]
    fn text_size_maps_to_a_clamped_scale() {
        assert_eq!(text_scale(None), 1.0);
        assert_eq!(text_scale(Some(0)), 1.0);
        assert_eq!(text_scale(Some(100)), 1.0);
        assert_eq!(text_scale(Some(125)), 1.25);
        assert_eq!(text_scale(Some(225)), SCALE_MAX);
        assert_eq!(text_scale(Some(50)), SCALE_MIN);
    }

    #[test]
    fn gnome_settings_parse_and_bad_ones_keep_defaults() {
        let os = parse_gsettings(
            Some("'prefer-light'\n"),
            Some("true\n"),
            Some("false\n"),
            Some("1.25\n"),
        );
        assert_eq!(
            os,
            OsPrefs {
                dark: false,
                high_contrast: true,
                animations: false,
                text_scale: 1.25,
            }
        );
        assert!(!parse_gsettings(Some("'default'"), None, None, None).dark);
        assert!(parse_gsettings(Some("'prefer-dark'"), None, None, None).dark);
        assert_eq!(parse_gsettings(None, None, None, None), OsPrefs::default());
        assert_eq!(
            parse_gsettings(Some("'odd'"), Some("1"), Some(""), Some("NaN")),
            OsPrefs::default()
        );
        let scale = |v| parse_gsettings(None, None, None, Some(v)).text_scale;
        assert_eq!(scale("-2.0"), 1.0);
        assert_eq!(scale("5.0"), SCALE_MAX);
    }

    #[test]
    fn unsaved_settings_follow_windows_and_saved_ones_win() {
        let windows = OsPrefs {
            dark: false,
            high_contrast: false,
            animations: false,
            text_scale: 1.5,
        };
        let mut fresh = ActiveTheme::with_os(ThemeChoice::System, None, None, windows);
        assert_eq!(fresh.scale, 1.5);
        assert!(!fresh.animations());
        assert_eq!(fresh.resolved, ResolvedTheme::Light);
        assert_eq!(
            (fresh.saved_scale(), fresh.saved_animations()),
            (None, None)
        );

        let changed = OsPrefs {
            high_contrast: true,
            animations: true,
            text_scale: 1.25,
            ..windows
        };
        fresh.set_os(changed);
        assert_eq!(fresh.scale, 1.25);
        assert!(fresh.animations());
        assert_eq!(fresh.resolved, ResolvedTheme::HighContrast);

        let mut saved = ActiveTheme::with_os(ThemeChoice::Dark, Some(0.9), Some(true), windows);
        saved.set_os(changed);
        assert_eq!(saved.scale, 0.9);
        assert!(saved.animations());
        assert_eq!(saved.resolved, ResolvedTheme::Dark);
        assert_eq!(saved.saved_scale(), Some(0.9));

        // Picking a value saves it; resetting the scale follows Windows again.
        fresh.set_animations(false);
        assert_eq!(fresh.saved_animations(), Some(false));
        fresh.set_scale(1.8);
        assert_eq!(fresh.saved_scale(), Some(1.8));
        fresh.reset_scale();
        assert_eq!((fresh.scale, fresh.saved_scale()), (1.25, None));
    }

    #[test]
    fn reset_animations_follows_windows_again() {
        let windows = OsPrefs {
            dark: false,
            high_contrast: false,
            animations: false,
            text_scale: 1.0,
        };
        // A choice saved by an earlier run, or picked in the app.
        let mut theme = ActiveTheme::with_os(ThemeChoice::System, None, Some(true), windows);
        theme.set_os(windows);
        assert!(theme.animations());

        theme.reset_animations();
        assert!(!theme.animations());
        assert_eq!(theme.saved_animations(), None);

        theme.set_os(OsPrefs {
            animations: true,
            ..windows
        });
        assert!(theme.animations());
        assert_eq!(theme.saved_animations(), None);
    }

    /// Whether `focus_ring` painted its accent ring this pass.
    fn ring_drawn(ctx: &egui::Context, events: Vec<egui::Event>) -> bool {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            events,
            ..Default::default()
        };
        let palette = test_palette();
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let _ = ui.button("One");
                let _ = ui.button("Two");
            });
            focus_ring(ctx, palette);
        });
        out.shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::Shape::Rect(r)
                if r.stroke.color == palette.accent && r.stroke.width == 2.0)
        })
    }

    #[test]
    fn focus_ring_shows_after_tab_and_hides_after_a_click() {
        let ctx = egui::Context::default();
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(!ring_drawn(&ctx, vec![]));
        let _ = ring_drawn(&ctx, vec![tab]);
        assert!(
            ring_drawn(&ctx, vec![]),
            "Tab focuses a button and rings it"
        );
        let click = egui::Event::PointerButton {
            pos: egui::pos2(390.0, 290.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        assert!(!ring_drawn(&ctx, vec![click]), "a click hides the ring");
    }
}
