//! Generic parts (#65): LEDs, pixel strips and rings, servo-turned shapes and
//! buttons, added to the droid as data instead of new drawing code. Each one
//! shows, or for a button sets, the value under its name in `HwState::parts`.
//!
//! The parts come from `simulator/parts.toml`, read again whenever code.py
//! (re)starts. A part with a `pca` or `pin` source follows that output; one
//! without is left to shims.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::path::Path;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use serde::Deserialize;

use crate::config::{
    HingeCfg, PartCfg, PartKind, PartShape, PartSource, PixelLayout, ServoCfg, SimConfig,
};
use crate::dock::ViewportRect;
use crate::droid::{lamp_light, lamp_material};
use crate::hw::{hw, HwState, PartValue, PinValue, PWM_PERIOD_US};
use crate::scene::{
    glow_emissive, servo_frame, shine, DroidRoot, Glow, GlowSource, ServoTrack, Side, Wing,
};

/// The parts file, next to sim.toml.
pub const PARTS_FILE: &str = "parts.toml";
/// Keeps a runaway file from flooding the model.
const MAX_PARTS: usize = 64;

/// The parts on the droid. Empty by default, which leaves the model as it is.
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct Parts(pub Vec<PartCfg>);

/// On a droid root once its model has finished spawning, so anchors can be
/// found. The primitive model is ready at once; glTF when its scene is.
#[derive(Component)]
struct ModelReady;

/// The top entity of one part, a child of the droid's root, so switching
/// model removes it along with the old root.
#[derive(Component)]
pub struct Part;

#[derive(Component)]
struct PartLed {
    name: String,
    colour: Vec3,
    material: Handle<StandardMaterial>,
    light: Entity,
}

#[derive(Component)]
struct PartPixel {
    name: String,
    index: usize,
    colour: Vec3,
    material: Handle<StandardMaterial>,
}

#[derive(Component)]
struct PartHinge {
    name: String,
    axis: Vec3,
    hinge: HingeCfg,
}

#[derive(Component)]
struct PartButton {
    name: String,
    rest_y: f32,
    /// How far the cap sinks while pressed.
    travel: f32,
}

/// Where each servo part's servo physically is, like `ServoSlew` for the wings.
#[derive(Resource, Default)]
struct PartSlew {
    tracks: HashMap<String, ServoTrack>,
    last_now: Option<f64>,
}

/// The button part being held down with the mouse, if any.
#[derive(Resource, Default)]
struct HeldButton(Option<String>);

pub struct PartsPlugin;

impl Plugin for PartsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Parts>()
            .init_resource::<PartSlew>()
            .init_resource::<HeldButton>()
            .add_observer(mark_scene_ready)
            .add_systems(
                Update,
                (
                    reload_parts,
                    spawn_parts,
                    follow_sources,
                    release_held_button,
                    drive_part_lights,
                    slew_parts,
                    drive_part_motion,
                )
                    .chain(),
            );
    }
}

/// Reads the parts file whenever code.py (re)starts: at launch, on Restart
/// (R) and on Auto-reload, which `ui::hot_reload` also triggers when the file
/// is saved. An unchanged file is left alone, so its warnings show once.
fn reload_parts(
    paths: Res<crate::Paths>,
    mut parts: ResMut<Parts>,
    mut seen: Local<(Option<u64>, Option<String>)>,
) {
    let runs = hw().runs;
    if seen.0 == Some(runs) {
        return;
    }
    seen.0 = Some(runs);
    let path = paths.simulator.join(PARTS_FILE);
    let text = match read_parts_file(&path) {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => {
            hw().push_line(format!(
                "[sim] could not read {PARTS_FILE}, keeping the previous parts: {e}"
            ));
            return;
        }
    };
    if seen.1.as_deref() == Some(text.as_str()) {
        return;
    }
    let (list, warnings) = match parse_parts(&text) {
        Ok(parsed) => parsed,
        Err(e) => {
            hw().push_line(format!(
                "[sim] {PARTS_FILE} is invalid, keeping the previous parts: {e}"
            ));
            seen.1 = Some(text);
            return;
        }
    };
    seen.1 = Some(text);
    let mut s = hw();
    for w in warnings {
        s.push_line(w);
    }
    if list != parts.0 {
        s.push_line(format!(
            "[sim] {PARTS_FILE}: {} part{} loaded",
            list.len(),
            if list.len() == 1 { "" } else { "s" }
        ));
    }
    drop(s);
    parts.set_if_neq(Parts(list));
}

/// `Ok(None)` when there is no parts file, which means no parts.
fn read_parts_file(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The usable parts in a parts file, plus a `[sim]` warning for each entry
/// that was skipped. `Err` only when the file isn't TOML at all.
pub fn parse_parts(text: &str) -> Result<(Vec<PartCfg>, Vec<String>), String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    for key in table.keys().filter(|k| *k != "part") {
        warnings.push(format!(
            "[sim] {PARTS_FILE}: `{key}` ignored; each part is a [[part]] entry"
        ));
    }
    let entries = match table.get("part") {
        None => return Ok((Vec::new(), warnings)),
        Some(toml::Value::Array(entries)) => entries,
        Some(_) => return Err("`part` must be [[part]] entries".into()),
    };
    let mut parts: Vec<PartCfg> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let label = match entry.get("name").and_then(toml::Value::as_str) {
            Some(name) => format!("part {} (\"{name}\")", i + 1),
            None => format!("part {}", i + 1),
        };
        let cfg = PartCfg::deserialize(entry.clone())
            .map_err(|e| e.message().trim().to_owned())
            .and_then(|cfg| cfg.check().map(|()| cfg));
        let problem = match cfg {
            Err(e) => e,
            Ok(cfg) if parts.iter().any(|p| p.name == cfg.name) => {
                "another part already has this name".into()
            }
            Ok(_) if parts.len() >= MAX_PARTS => format!("more than {MAX_PARTS} parts"),
            Ok(cfg) => {
                parts.push(cfg);
                continue;
            }
        };
        warnings.push(format!("[sim] {PARTS_FILE} {label} skipped: {problem}"));
    }
    Ok((parts, warnings))
}

/// What a part's source last output, before it's turned into a value.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reading {
    /// A PWM output: duty fraction and pulse width.
    Duty {
        fraction: f32,
        pulse_us: f32,
    },
    Digital(bool),
}

fn reading(cfg: &PartCfg, s: &HwState) -> Option<Reading> {
    match cfg.source() {
        PartSource::Pca(ch) => s.duty_fraction(ch).map(|fraction| Reading::Duty {
            fraction,
            pulse_us: fraction * PWM_PERIOD_US,
        }),
        PartSource::Pin(pin) => match s.pins.get(pin)? {
            PinValue::Digital(on) => Some(Reading::Digital(*on)),
            PinValue::Pwm { duty, freq } => {
                let fraction = f32::from(*duty) / 65535.0;
                Some(Reading::Duty {
                    fraction,
                    pulse_us: fraction * 1e6 / (*freq).max(1) as f32,
                })
            }
        },
        PartSource::Shim => None,
    }
}

/// What a part shows for its source's output: brightness for an LED or
/// pixels (`full_duty` = fully lit), the servo angle for a servo part, as the
/// wings work it out. `None` while there is nothing to show (an unpowered
/// servo, or a servo on a digital pin).
fn value_for(cfg: &PartCfg, reading: Reading, servo: &ServoCfg) -> Option<PartValue> {
    match (cfg.kind, reading) {
        (PartKind::Led | PartKind::Pixels, Reading::Duty { fraction, .. }) => {
            Some(PartValue::Level((fraction / cfg.full_duty).clamp(0.0, 1.0)))
        }
        (PartKind::Led | PartKind::Pixels, Reading::Digital(on)) => {
            Some(PartValue::Level(if on { 1.0 } else { 0.0 }))
        }
        (PartKind::ServoPart, Reading::Duty { pulse_us, .. }) => {
            (pulse_us > 0.0).then(|| PartValue::Angle(servo.angle_for_pulse(pulse_us)))
        }
        _ => None,
    }
}

/// Copies each part's source into `HwState::parts`, and each pin button's
/// state onto its pin for `digitalio` to read.
fn follow_sources(parts: Res<Parts>, cfg: Res<SimConfig>) {
    if parts.0.is_empty() {
        return;
    }
    let mut s = hw();
    for part in &parts.0 {
        match (part.kind, part.source()) {
            (_, PartSource::Shim) => {}
            (PartKind::Button, PartSource::Pin(pin)) => {
                let pressed = matches!(s.parts.get(&part.name), Some(PartValue::Pressed(true)));
                s.button_pins.insert(pin.to_owned(), pressed);
            }
            _ => match reading(part, &s).and_then(|r| value_for(part, r, &cfg.servo)) {
                Some(value) => {
                    s.parts.insert(part.name.clone(), value);
                }
                None => {
                    s.parts.remove(&part.name);
                }
            },
        }
    }
}

/// glTF droids are ready once their scene has spawned, with its named nodes.
fn mark_scene_ready(
    ready: Trigger<SceneInstanceReady>,
    roots: Query<(), With<DroidRoot>>,
    mut commands: Commands,
) {
    if roots.contains(ready.target()) {
        commands.entity(ready.target()).insert(ModelReady);
    }
}

/// A droid root that was just spawned or just finished loading.
type NewModel = Or<(Added<DroidRoot>, Added<ModelReady>)>;

/// The droid models and the nodes parts can sit on.
#[derive(SystemParam)]
struct Model<'w, 's> {
    roots: Query<'w, 's, (Entity, Has<SceneRoot>, Has<ModelReady>), With<DroidRoot>>,
    new_roots: Query<'w, 's, (), NewModel>,
    children: Query<'w, 's, &'static Children>,
    names: Query<'w, 's, &'static Name>,
    wings: Query<'w, 's, &'static Wing>,
    glows: Query<'w, 's, &'static Glow>,
}

impl Model<'_, '_> {
    /// The node called `anchor` under `root`: a node name first (glTF), then
    /// the wing and glow nodes both models have.
    fn anchor(&self, root: Entity, anchor: &str) -> Option<Entity> {
        if anchor == PartCfg::BODY {
            return Some(root);
        }
        let below: Vec<Entity> = self.children.iter_descendants(root).collect();
        let named = below
            .iter()
            .find(|&&e| self.names.get(e).is_ok_and(|n| n.as_str() == anchor));
        let side = match anchor {
            "wing_right" => Some(Side::Right),
            "wing_left" => Some(Side::Left),
            _ => None,
        };
        let glow = match anchor {
            "eye_main" => Some(GlowSource::MainEye),
            "eye_small" => Some(GlowSource::SmallEye),
            "led_white" => Some(GlowSource::WhiteLed),
            _ => None,
        };
        let known = below.iter().find(|&&e| {
            side.is_some_and(|side| self.wings.get(e).is_ok_and(|w| w.side == side))
                || glow.is_some_and(|g| self.glows.get(e).is_ok_and(|w| w.source() == g))
        });
        named.or(known).copied()
    }
}

/// (Re)builds every part when the list changes or a model finishes spawning.
fn spawn_parts(
    mut commands: Commands,
    parts: Res<Parts>,
    model: Model,
    existing: Query<Entity, With<Part>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !parts.is_changed() && model.new_roots.is_empty() {
        return;
    }
    for part in &existing {
        commands.entity(part).try_despawn();
    }
    for (root, is_scene, ready) in &model.roots {
        if is_scene && !ready {
            continue; // spawned once the scene is ready
        }
        for cfg in &parts.0 {
            let Some(anchor) = model.anchor(root, &cfg.anchor) else {
                hw().push_line(format!(
                    "[sim] part {} skipped: this model has no node \"{}\"",
                    cfg.name, cfg.anchor
                ));
                continue;
            };
            spawn_part(&mut commands, &mut meshes, &mut materials, anchor, cfg);
        }
    }
}

fn spawn_part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    anchor: Entity,
    cfg: &PartCfg,
) {
    let [rx, ry, rz] = cfg.rot_deg.map(f32::to_radians);
    let part = commands
        .spawn((
            Part,
            ChildOf(anchor),
            Transform::from_translation(Vec3::from(cfg.pos)).with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                rx,
                ry,
                rz,
            )),
            Visibility::default(),
        ))
        .id();
    let name = cfg.name.clone();
    let colour = Vec3::from(cfg.colour);
    let s = cfg.size.max(0.001);
    let body = || StandardMaterial {
        base_color: Color::srgb(cfg.colour[0], cfg.colour[1], cfg.colour[2]),
        perceptual_roughness: 0.5,
        ..default()
    };
    match cfg.kind {
        PartKind::Led => {
            let material = lamp_material(materials);
            let light = commands
                .spawn((
                    ChildOf(part),
                    lamp_light(s),
                    Transform::from_xyz(0.0, 2.0 * s, 0.0),
                ))
                .id();
            commands.spawn((
                ChildOf(part),
                Mesh3d(meshes.add(Sphere::new(s).mesh().uv(24, 12))),
                MeshMaterial3d(material.clone()),
                PartLed {
                    name,
                    colour,
                    material,
                    light,
                },
            ));
        }
        PartKind::Pixels => {
            let mesh = meshes.add(Sphere::new(s).mesh().uv(16, 8));
            for (index, at) in pixel_positions(cfg).into_iter().enumerate() {
                let material = lamp_material(materials);
                commands.spawn((
                    ChildOf(part),
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(at),
                    PartPixel {
                        name: name.clone(),
                        index,
                        colour,
                        material,
                    },
                ));
            }
        }
        PartKind::ServoPart => {
            // The shape runs from the hinge along +Z, so it swings about its edge.
            let (mesh, turn): (Mesh, Quat) = match cfg.shape {
                PartShape::Box => (
                    Cuboid::new(2.0 * s, 0.4 * s, 4.0 * s).into(),
                    Quat::IDENTITY,
                ),
                PartShape::Cylinder => (
                    Cylinder::new(s, 4.0 * s).into(),
                    Quat::from_rotation_x(TAU / 4.0),
                ),
            };
            let axis = Vec3::from(cfg.hinge.hinge_axis)
                .try_normalize()
                .unwrap_or(Vec3::X);
            let hinge = commands
                .spawn((
                    ChildOf(part),
                    Transform::default(),
                    Visibility::default(),
                    PartHinge {
                        name,
                        axis,
                        hinge: cfg.hinge.clone(),
                    },
                ))
                .id();
            commands.spawn((
                ChildOf(hinge),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(body())),
                Transform::from_xyz(0.0, 0.0, 2.0 * s).with_rotation(turn),
            ));
        }
        PartKind::Button => {
            let height = 0.5 * s;
            let press_name = name.clone();
            commands
                .spawn((
                    ChildOf(part),
                    Mesh3d(meshes.add(Cylinder::new(s, height))),
                    MeshMaterial3d(materials.add(body())),
                    Transform::from_xyz(0.0, height / 2.0, 0.0),
                    PartButton {
                        name,
                        rest_y: height / 2.0,
                        travel: 0.6 * height,
                    },
                ))
                // Press and click stop here, so pressing a button is never also
                // a boop (that observer is on the droid's root). The release is
                // `release_held_button`'s: the sunken cap may no longer be
                // under the pointer when the mouse button comes up.
                .observe(
                    move |mut press: Trigger<Pointer<Pressed>>,
                          viewport: Res<ViewportRect>,
                          mut held: ResMut<HeldButton>| {
                        press.propagate(false);
                        if press.button == PointerButton::Primary && viewport.hovered {
                            set_pressed(&press_name, true);
                            held.0 = Some(press_name.clone());
                        }
                    },
                )
                .observe(|mut click: Trigger<Pointer<Click>>| click.propagate(false));
        }
    }
}

fn set_pressed(name: &str, pressed: bool) {
    hw().parts
        .insert(name.to_owned(), PartValue::Pressed(pressed));
}

/// Lets go of the button held with the mouse once the mouse button is up,
/// wherever the pointer is by then.
fn release_held_button(mouse: Res<ButtonInput<MouseButton>>, mut held: ResMut<HeldButton>) {
    if mouse.pressed(MouseButton::Left) {
        return;
    }
    if let Some(name) = held.0.take() {
        set_pressed(&name, false);
    }
}

/// Dot centres in the part's local space: a row along X, centred, or a ring
/// in the XZ plane (around +Y, out of the surface) starting at +X.
fn pixel_positions(cfg: &PartCfg) -> Vec<Vec3> {
    let n = cfg.count.min(PartCfg::MAX_PIXELS);
    match cfg.layout {
        PixelLayout::Row => {
            let mid = (n as f32 - 1.0) / 2.0;
            (0..n)
                .map(|i| Vec3::X * (i as f32 - mid) * cfg.spacing)
                .collect()
        }
        PixelLayout::Ring => {
            let radius = n as f32 * cfg.spacing / TAU;
            (0..n)
                .map(|i| {
                    let a = i as f32 / n as f32 * TAU;
                    Vec3::new(radius * a.cos(), 0.0, radius * a.sin())
                })
                .collect()
        }
    }
}

fn unit_rgb(c: [f32; 3]) -> Vec3 {
    Vec3::from(c).clamp(Vec3::ZERO, Vec3::ONE)
}

/// What an LED shows: a brightness of its colour, or an RGB value. Dark
/// until set.
fn led_rgb(value: Option<&PartValue>, colour: Vec3) -> Vec3 {
    match value {
        Some(PartValue::Level(level)) => colour * level.clamp(0.0, 1.0),
        Some(PartValue::Rgb(c)) => unit_rgb(*c),
        _ => Vec3::ZERO,
    }
}

/// What one pixel shows: its own entry in a pixel list, or, for a single
/// brightness or RGB value, the same as every other pixel.
fn pixel_rgb(value: Option<&PartValue>, index: usize, colour: Vec3) -> Vec3 {
    match value {
        Some(PartValue::Pixels(pixels)) => pixels.get(index).map_or(Vec3::ZERO, |c| unit_rgb(*c)),
        other => led_rgb(other, colour),
    }
}

/// The servo angle a part was given; `None` while unpowered.
fn part_angle(value: Option<&PartValue>) -> Option<f32> {
    match value {
        Some(PartValue::Angle(deg)) => Some(*deg),
        _ => None,
    }
}

fn drive_part_lights(
    leds: Query<&PartLed>,
    pixels: Query<&PartPixel>,
    mut lights: Query<&mut PointLight>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if leds.is_empty() && pixels.is_empty() {
        return;
    }
    let s = hw();
    for led in &leds {
        let rgb = led_rgb(s.parts.get(&led.name), led.colour);
        if let Some(m) = materials.get_mut(&led.material) {
            m.emissive = glow_emissive(rgb);
        }
        if let Ok(mut point) = lights.get_mut(led.light) {
            shine(&mut point, rgb);
        }
    }
    for pixel in &pixels {
        let rgb = pixel_rgb(s.parts.get(&pixel.name), pixel.index, pixel.colour);
        if let Some(m) = materials.get_mut(&pixel.material) {
            m.emissive = glow_emissive(rgb);
        }
    }
}

/// Turns each servo part's servo toward its angle at the servo's rated speed,
/// as `scene::slew_servos` does for the wings.
fn slew_parts(cfg: Res<SimConfig>, mut slew: ResMut<PartSlew>, hinges: Query<&PartHinge>) {
    if hinges.is_empty() {
        return;
    }
    let (now, max_step, restarted) = servo_frame(&cfg, &mut slew.last_now);
    for hinge in &hinges {
        let target = part_angle(hw().parts.get(&hinge.name));
        let track = slew.tracks.entry(hinge.name.clone()).or_default();
        if restarted {
            track.restart();
        }
        let Some(target) = target else {
            continue; // unpowered: the servo stays put
        };
        if let Some((lag, behind_s)) = track.follow(target, max_step, now) {
            let at = track.actual.unwrap_or(target);
            hw().push_line(format!(
                "[sim] part {} servo can't keep up: asked for {target:.0}\u{b0}, still at {at:.0}\u{b0} ({lag:.0}\u{b0} behind for {behind_s:.1} s); it turns 60\u{b0} in {} s",
                hinge.name, cfg.servo.sec_per_60deg
            ));
        }
    }
}

fn drive_part_motion(
    slew: Res<PartSlew>,
    mut hinges: Query<(&PartHinge, &mut Transform), Without<PartButton>>,
    mut buttons: Query<(&PartButton, &mut Transform), Without<PartHinge>>,
) {
    if hinges.is_empty() && buttons.is_empty() {
        return;
    }
    let s = hw();
    for (hinge, mut transform) in &mut hinges {
        let servo = if s.servo_speed {
            slew.tracks.get(&hinge.name).and_then(|t| t.actual)
        } else {
            part_angle(s.parts.get(&hinge.name))
        };
        let Some(servo) = servo else {
            continue; // unpowered: leave it where it is
        };
        let angle = hinge.hinge.elevation_deg(servo).to_radians();
        transform.rotation = Quat::from_axis_angle(hinge.axis, angle);
    }
    for (button, mut transform) in &mut buttons {
        let down = matches!(s.parts.get(&button.name), Some(PartValue::Pressed(true)));
        transform.translation.y = button.rest_y - if down { button.travel } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        a.abs_diff_eq(b, 1e-5)
    }

    fn part(kind: PartKind) -> PartCfg {
        PartCfg {
            name: "p".into(),
            kind,
            ..PartCfg::default()
        }
    }

    #[test]
    fn example_file_loads_cleanly() {
        let (parts, warnings) =
            parse_parts(include_str!("../parts.example.toml")).expect("valid TOML");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].source(), PartSource::Pca(0));
        assert_eq!(parts[1].source(), PartSource::Pin("EXTERNAL_NEOPIXELS"));
        assert_eq!(parts[2].anchor, "wing_right");
        assert_eq!(parts[4].kind, PartKind::Button);
    }

    #[test]
    fn no_file_contents_means_no_parts() {
        assert_eq!(parse_parts("").expect("empty is fine"), (vec![], vec![]));
        assert!(parse_parts("[[part]\nname=").is_err());
        assert!(parse_parts("part = 3").is_err());
    }

    #[test]
    fn bad_entries_are_skipped_with_a_warning() {
        let (parts, warnings) = parse_parts(
            r#"
            stray = 1
            [[part]]
            name = "ok"
            [[part]]
            name = "disp"
            kind = "display"
            [[part]]
            name = "typo"
            colur = [1, 0, 0]
            [[part]]
            name = "has space"
            [[part]]
            name = "ok"
            [[part]]
            name = "none"
            kind = "pixels"
            count = 0
            [[part]]
            name = "huge"
            kind = "pixels"
            count = 100000
            [[part]]
            name = "both"
            pca = 1
            pin = "D5"
            [[part]]
            name = "ch99"
            pca = 99
            [[part]]
            name = "btn"
            kind = "button"
            pca = 8
            [[part]]
            name = "pin"
            pin = "d5"
            [[part]]
            name = "duty"
            full_duty = 0
            [[part]]
            name = "flat"
            size = -1
            [[part]]
            name = "stuck"
            kind = "servo_part"
            hinge = { min_deg = 10, max_deg = 0 }
            [[part]]
            name = "spin"
            kind = "servo_part"
            hinge = { hinge_axis = [0, 0, 0] }
            [[part]]
            name = "hinge_typo"
            kind = "servo_part"
            hinge = { gian = 2 }
            [[part]]
            kind = "led"
            [[part]]
            name = "last"
            kind = "button"
            pin = "D6"
            "#,
        )
        .expect("valid TOML");
        let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["ok", "last"]);
        // One for `stray`, one per skipped entry.
        assert_eq!(warnings.len(), 17, "{warnings:#?}");
        assert!(warnings.iter().all(|w| w.starts_with("[sim] parts.toml")));
        assert!(warnings[1].contains("part 2 (\"disp\")"));
        assert!(warnings[2].contains("colur"));
        assert!(warnings[4].contains("already has this name"));
    }

    #[test]
    fn parts_beyond_the_limit_are_skipped() {
        let text: String = (0..MAX_PARTS + 2)
            .map(|i| format!("[[part]]\nname = \"p{i}\"\n"))
            .collect();
        let (parts, warnings) = parse_parts(&text).expect("valid TOML");
        assert_eq!(parts.len(), MAX_PARTS);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn leds_follow_duty_scaled_by_full_duty() {
        let servo = ServoCfg::default();
        let mut led = part(PartKind::Led);
        let half = Reading::Duty {
            fraction: 0.5,
            pulse_us: 10_000.0,
        };
        assert_eq!(value_for(&led, half, &servo), Some(PartValue::Level(0.5)));
        led.full_duty = 0.25;
        assert_eq!(value_for(&led, half, &servo), Some(PartValue::Level(1.0)));
        let on = Reading::Digital(true);
        let pixels = part(PartKind::Pixels);
        assert_eq!(value_for(&pixels, on, &servo), Some(PartValue::Level(1.0)));
        assert_eq!(
            value_for(&pixels, Reading::Digital(false), &servo),
            Some(PartValue::Level(0.0))
        );
    }

    #[test]
    fn servo_parts_follow_the_pulse_like_the_wings() {
        let servo = ServoCfg::default();
        let flap = part(PartKind::ServoPart);
        let mid_us = (servo.min_pulse_us + servo.max_pulse_us) / 2.0;
        let mid = Reading::Duty {
            fraction: mid_us / PWM_PERIOD_US,
            pulse_us: mid_us,
        };
        assert_eq!(value_for(&flap, mid, &servo), Some(PartValue::Angle(90.0)));
        let off = Reading::Duty {
            fraction: 0.0,
            pulse_us: 0.0,
        };
        assert_eq!(value_for(&flap, off, &servo), None);
        assert_eq!(value_for(&flap, Reading::Digital(true), &servo), None);
        let button = part(PartKind::Button);
        assert_eq!(value_for(&button, mid, &servo), None);
    }

    #[test]
    fn readings_come_from_the_channel_or_pin() {
        let mut s = HwState::new();
        let mut on_pca = part(PartKind::ServoPart);
        on_pca.pca = Some(8);
        assert_eq!(reading(&on_pca, &s), None);
        s.pca_duty[8] = Some(6554); // 10%: 2000 us at 50 Hz
        let Some(Reading::Duty { pulse_us, .. }) = reading(&on_pca, &s) else {
            panic!("expected a duty reading");
        };
        assert!((pulse_us - 2000.0).abs() < 0.5);

        let mut on_pin = part(PartKind::ServoPart);
        on_pin.pin = Some("D5".into());
        assert_eq!(reading(&on_pin, &s), None);
        s.pins.insert(
            "D5".into(),
            PinValue::Pwm {
                duty: 4915, // 7.5% at 50 Hz: 1500 us
                freq: 50,
            },
        );
        let Some(Reading::Duty { pulse_us, .. }) = reading(&on_pin, &s) else {
            panic!("expected a duty reading");
        };
        assert!((pulse_us - 1500.0).abs() < 0.5);
        s.pins.insert("D5".into(), PinValue::Digital(true));
        assert_eq!(reading(&on_pin, &s), Some(Reading::Digital(true)));

        assert_eq!(reading(&part(PartKind::Led), &s), None);
    }

    #[test]
    fn led_shows_level_or_rgb_and_is_dark_until_set() {
        let orange = Vec3::new(1.0, 0.5, 0.0);
        assert_eq!(led_rgb(None, orange), Vec3::ZERO);
        assert!(close(
            led_rgb(Some(&PartValue::Level(0.5)), orange),
            Vec3::new(0.5, 0.25, 0.0)
        ));
        assert!(close(led_rgb(Some(&PartValue::Level(3.0)), orange), orange));
        assert!(close(
            led_rgb(Some(&PartValue::Rgb([0.2, 2.0, -1.0])), orange),
            Vec3::new(0.2, 1.0, 0.0)
        ));
        assert_eq!(led_rgb(Some(&PartValue::Angle(90.0)), orange), Vec3::ZERO);
    }

    #[test]
    fn pixels_take_their_own_entry_or_a_shared_value() {
        let list = PartValue::Pixels(vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        assert_eq!(pixel_rgb(Some(&list), 1, Vec3::ONE), Vec3::Y);
        assert_eq!(pixel_rgb(Some(&list), 5, Vec3::ONE), Vec3::ZERO);
        assert_eq!(pixel_rgb(Some(&PartValue::Level(1.0)), 3, Vec3::X), Vec3::X);
    }

    #[test]
    fn only_an_angle_moves_a_servo_part() {
        assert_eq!(part_angle(Some(&PartValue::Angle(42.0))), Some(42.0));
        assert_eq!(part_angle(Some(&PartValue::Level(1.0))), None);
        assert_eq!(part_angle(None), None);
    }

    #[test]
    fn row_is_centred_and_evenly_spaced() {
        let cfg = PartCfg {
            count: 3,
            spacing: 0.1,
            ..PartCfg::default()
        };
        let row = pixel_positions(&cfg);
        assert_eq!(row.len(), 3);
        assert!(close(row[0], Vec3::new(-0.1, 0.0, 0.0)));
        assert!(close(row[1], Vec3::ZERO));
        assert!(close(row[2], Vec3::new(0.1, 0.0, 0.0)));
    }

    #[test]
    fn ring_circumference_matches_spacing() {
        let cfg = PartCfg {
            count: 12,
            spacing: 0.1,
            layout: PixelLayout::Ring,
            ..PartCfg::default()
        };
        let ring = pixel_positions(&cfg);
        assert_eq!(ring.len(), 12);
        let radius = 1.2 / TAU;
        assert!(ring
            .iter()
            .all(|p| (p.length() - radius).abs() < 1e-5 && p.y == 0.0));
        assert!(close(ring[0], Vec3::new(radius, 0.0, 0.0)));
        // Neighbours sit a chord apart, just under the spacing (the arc).
        let chord = 2.0 * radius * (TAU / 24.0).sin();
        assert!((ring[0].distance(ring[1]) - chord).abs() < 1e-5);
    }

    #[test]
    fn pixel_count_is_capped() {
        let cfg = PartCfg {
            count: usize::MAX,
            ..PartCfg::default()
        };
        assert_eq!(pixel_positions(&cfg).len(), PartCfg::MAX_PIXELS);
    }
}
