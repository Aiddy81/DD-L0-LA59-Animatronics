//! Generic parts (#65): LEDs, pixel strips and rings, servo-turned shapes and
//! buttons, added to the droid as data instead of new drawing code. Each one
//! shows, or for a button sets, the value under its name in `HwState::parts`.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::config::{HingeCfg, PartCfg, PartKind, PartShape, PixelLayout, SimConfig};
use crate::dock::ViewportRect;
use crate::droid::{lamp_light, lamp_material};
use crate::hw::{hw, PartValue};
use crate::scene::{glow_emissive, servo_frame, shine, DroidRoot, ServoTrack};

/// Keeps a mistyped count from spawning millions of dots.
const MAX_PIXELS: usize = 1024;

/// The parts on the droid. Empty by default, which leaves the model as it is.
#[derive(Resource, Default, Clone, Debug)]
pub struct Parts(pub Vec<PartCfg>);

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
            .add_systems(
                Update,
                (
                    spawn_parts,
                    release_held_button,
                    drive_part_lights,
                    (slew_parts, drive_part_motion).chain(),
                ),
            );
    }
}

/// (Re)builds every part when the list changes or a model is spawned.
fn spawn_parts(
    mut commands: Commands,
    parts: Res<Parts>,
    roots: Query<Entity, With<DroidRoot>>,
    new_roots: Query<(), Added<DroidRoot>>,
    existing: Query<Entity, With<Part>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !parts.is_changed() && new_roots.is_empty() {
        return;
    }
    for part in &existing {
        commands.entity(part).try_despawn();
    }
    for root in &roots {
        for cfg in &parts.0 {
            spawn_part(&mut commands, &mut meshes, &mut materials, root, cfg);
        }
    }
}

fn spawn_part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    cfg: &PartCfg,
) {
    let [rx, ry, rz] = cfg.rot_deg.map(f32::to_radians);
    let part = commands
        .spawn((
            Part,
            ChildOf(root),
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
    let n = cfg.count.min(MAX_PIXELS);
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
        assert_eq!(pixel_positions(&cfg).len(), MAX_PIXELS);
    }
}
