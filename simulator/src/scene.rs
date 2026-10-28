//! The 3D droid: a placeholder built from primitives, or `assets/droid.glb`
//! if present. Joints and lights are driven from the simulated hardware.

use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::SimConfig;
use crate::dock::ViewportRect;
use crate::hw::hw;
use crate::theme::ActiveTheme;
use crate::Paths;

pub const MODEL_FILE: &str = "droid.glb";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Right,
    Left,
}

impl Side {
    const BOTH: [Side; 2] = [Side::Right, Side::Left];

    fn index(self) -> usize {
        match self {
            Side::Right => 0,
            Side::Left => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Side::Right => "right",
            Side::Left => "left",
        }
    }
}

/// A lag smaller than this (servo degrees) counts as keeping up.
const LAG_WARN_DEG: f32 = 10.0;
/// How long (virtual seconds) the lag must last to be worth a warning. One
/// snap across the servo's whole travel catches up sooner than this.
const LAG_SUSTAIN_S: f64 = 0.4;

/// Moves `actual` toward `target` by at most `max_step` degrees.
fn slew(actual: f32, target: f32, max_step: f32) -> f32 {
    actual + (target - actual).clamp(-max_step, max_step)
}

/// Where one physical servo is, as opposed to where code.py told it to go.
#[derive(Default)]
struct ServoTrack {
    /// Servo degrees; `None` until the first command (it then starts there).
    actual: Option<f32>,
    /// Virtual time the current lag began.
    lag_since: Option<f64>,
    warned: bool,
}

impl ServoTrack {
    /// Follows `target` for one frame. Returns how far behind it is and for
    /// how long, once per stretch of falling more than LAG_WARN_DEG behind
    /// for longer than LAG_SUSTAIN_S.
    fn follow(&mut self, target: f32, max_step: f32, now: f64) -> Option<(f32, f64)> {
        let actual = self.actual.map_or(target, |a| slew(a, target, max_step));
        self.actual = Some(actual);
        let lag = (target - actual).abs();
        if lag <= LAG_WARN_DEG {
            self.lag_since = None;
            self.warned = false;
            return None;
        }
        let behind_s = now - *self.lag_since.get_or_insert(now);
        if self.warned || behind_s < LAG_SUSTAIN_S {
            return None;
        }
        self.warned = true;
        Some((lag, behind_s))
    }
}

/// The wing servos' real positions, turning no faster than `[servo]
/// sec_per_60deg` in virtual time: a pause freezes a move part-way, and at
/// high clock speeds the servo gets that much robot time to catch up.
#[derive(Resource, Default)]
pub struct ServoSlew {
    tracks: [ServoTrack; 2],
    last_now: Option<f64>,
}

#[derive(Component)]
pub struct Wing {
    pub side: Side,
    pub rest: Quat,
    /// The model's own hinge axis; `None` uses `hinge_axis` from sim.toml.
    pub axis: Option<Vec3>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GlowSource {
    MainEye,
    SmallEye,
    WhiteLed,
}

/// Something that lights up: its materials glow and its point light shines.
#[derive(Component)]
pub struct Glow {
    source: GlowSource,
    materials: Vec<Handle<StandardMaterial>>,
    /// glTF nodes get their materials swapped for private copies once loaded.
    adopted: bool,
}

impl Glow {
    pub fn source(&self) -> GlowSource {
        self.source
    }

    pub fn placeholder(source: GlowSource, material: Handle<StandardMaterial>) -> Self {
        Self {
            source,
            materials: vec![material],
            adopted: true,
        }
    }
}

#[derive(Component)]
pub struct GlowLight(pub GlowSource);

/// How far the mouse has moved since the last button press, so a drag to orbit
/// the camera that starts on the head isn't mistaken for a boop.
#[derive(Resource, Default)]
pub(crate) struct DragDistance(f32);

#[derive(Component)]
pub struct OrbitCam {
    yaw: f32,
    pitch: f32,
    distance: f32,
    focus: Vec3,
}

/// Where the camera looks by default: roughly the droid's chest.
const HOME_FOCUS: Vec3 = Vec3::new(0.0, 1.05, 0.0);

impl OrbitCam {
    fn home() -> Self {
        Self {
            yaw: 0.35,
            pitch: 0.38,
            distance: 5.2,
            focus: HOME_FOCUS,
        }
    }
}

/// Toolbar camera presets.
#[derive(Clone, Copy)]
pub enum CameraView {
    Front,
    Side,
    Top,
    Eyes,
    Reset,
}

/// Mouse state the orbit camera reads; bundled to keep `orbit_camera` within
/// clippy's argument limit.
#[derive(SystemParam)]
struct MouseInput<'w> {
    buttons: Res<'w, ButtonInput<MouseButton>>,
    motion: Res<'w, AccumulatedMouseMotion>,
    scroll: Res<'w, AccumulatedMouseScroll>,
}

/// How lit the room around the droid is. Dim and Dark show how the eyes and
/// white LED look (and light the droid) in a darker room.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomLight {
    #[default]
    Bright,
    Dim,
    Dark,
}

impl RoomLight {
    pub const ALL: [RoomLight; 3] = [RoomLight::Bright, RoomLight::Dim, RoomLight::Dark];

    pub fn label(self) -> &'static str {
        match self {
            RoomLight::Bright => "Bright",
            RoomLight::Dim => "Dim",
            RoomLight::Dark => "Dark",
        }
    }

    /// (ambient brightness, overhead lamp lux, background brightness factor).
    /// The factor scales the theme's scene background, so Dim and Dark stay
    /// darker than Bright in every theme.
    fn levels(self) -> (f32, f32, f32) {
        match self {
            RoomLight::Bright => (250.0, 4_000.0, 1.0),
            RoomLight::Dim => (25.0, 300.0, 0.36),
            RoomLight::Dark => (0.0, 0.0, 0.06),
        }
    }
}

/// The room's overhead lamp, dimmed by `RoomLight`.
#[derive(Component)]
struct RoomLamp;

/// The floor plane's material, so a theme change can recolour it.
#[derive(Resource)]
struct FloorMaterial(Handle<StandardMaterial>);

/// Which droid model is shown. Switching it swaps the model live.
#[derive(Resource)]
pub struct ModelChoice {
    pub use_gltf: bool,
    /// Whether `assets/droid.glb` exists.
    pub gltf_available: bool,
}

/// Parent of everything that makes up the droid model.
#[derive(Component)]
struct DroidRoot;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragDistance>()
            .init_resource::<ServoSlew>()
            .init_resource::<RoomLight>()
            .insert_resource(AmbientLight {
                color: Color::WHITE,
                brightness: 250.0,
                ..default()
            })
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (
                    spawn_model.run_if(resource_changed::<ModelChoice>),
                    bind_gltf_nodes,
                    adopt_glow_materials,
                    (slew_servos, drive_wings).chain(),
                    drive_glow,
                    apply_room_light
                        .run_if(resource_changed::<RoomLight>.or(resource_changed::<ActiveTheme>)),
                    orbit_camera.after(crate::ui::UiSet),
                    apply_viewport.after(crate::ui::UiSet),
                ),
            );
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    paths: Res<Paths>,
    cfg: Res<SimConfig>,
    prefs: Option<Res<crate::prefs::Prefs>>,
    theme: Res<ActiveTheme>,
) {
    let gltf_available = paths.assets.join(MODEL_FILE).exists();
    // A saved choice beats sim.toml, which only sets the default.
    if let Some(p) = &prefs {
        commands.insert_resource(p.room_light);
    }
    let wanted = prefs
        .and_then(|p| p.model_gltf)
        .unwrap_or(cfg.model.kind != "primitive");
    commands.insert_resource(ModelChoice {
        use_gltf: gltf_available && wanted,
        gltf_available,
    });
    commands.spawn((
        Camera3d::default(),
        Camera {
            hdr: true,
            ..default()
        },
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        Transform::default(),
        OrbitCam::home(),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 4_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        RoomLamp,
    ));
    let floor = materials.add(StandardMaterial {
        base_color: to_color(theme.palette.floor),
        perceptual_roughness: 0.9,
        ..default()
    });
    commands.insert_resource(FloorMaterial(floor.clone()));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(floor),
    ));
}

/// (Re)builds the droid as the chosen model: the real one from droid.glb or
/// the primitive one from droid.rs.
fn spawn_model(
    mut commands: Commands,
    choice: Res<ModelChoice>,
    existing: Query<Entity, With<DroidRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    for root in &existing {
        commands.entity(root).despawn();
    }
    let root = commands
        .spawn((DroidRoot, Transform::default(), Visibility::default()))
        .observe(boop)
        .id();
    if choice.use_gltf && choice.gltf_available {
        commands.entity(root).insert(SceneRoot(
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL_FILE)),
        ));
    } else {
        crate::droid::spawn(&mut commands, &mut meshes, &mut materials, root);
    }
}

/// A click at or under this long is the lightest boop.
const BOOP_TAP_S: f32 = 0.1;
/// A press this long or longer is the hardest boop.
const BOOP_HARD_S: f32 = 1.0;

/// Boop strength in g for a press of `held_s` seconds: NUDGE_DELTA_G for a
/// tap up to SHAKE_DELTA_G for a long press, so clicking can reach every
/// firmware threshold.
fn boop_strength(held_s: f32) -> f32 {
    let t = ((held_s - BOOP_TAP_S) / (BOOP_HARD_S - BOOP_TAP_S)).clamp(0.0, 1.0);
    crate::ui::NUDGE_DELTA_G + (crate::ui::SHAKE_DELTA_G - crate::ui::NUDGE_DELTA_G) * t
}

/// The sensor-frame direction of a world-space surface normal. DroidRoot is
/// unrotated, so sensor x = world +X, y = world -Z and z = world +Y (up).
/// None if the normal is missing or degenerate.
fn sensor_dir(normal: Option<Vec3>) -> Option<[f32; 3]> {
    let n = normal?.try_normalize()?;
    Some([n.x, -n.z, n.y])
}

/// Clicking the droid is a nose boop. The reading spikes along the surface
/// normal where you clicked, harder the longer you hold. A drag to orbit the
/// camera that happens to start on it doesn't count. Clicks on any part
/// bubble up to the model's root, which has this observer.
fn boop(click: Trigger<Pointer<Click>>, drag: Res<DragDistance>, viewport: Res<ViewportRect>) {
    // A click on a panel or window drawn over the droid is not a boop.
    if !viewport.hovered || drag.0 >= 4.0 {
        return;
    }
    let dir = sensor_dir(click.hit.normal).unwrap_or_else(|| crate::hw::hw().nudge_dir);
    let delta = boop_strength(click.duration.as_secs_f32());
    crate::ui::boop(dir, delta);
    crate::hw::hw().push_line(format!(
        "[sim] boop {delta:.1} g from {}",
        crate::ui::nearest_direction(dir)
    ));
}

/// glTF models: attach behaviour to nodes by name once the scene spawns.
fn bind_gltf_nodes(mut commands: Commands, named: Query<(Entity, &Name, &Transform), Added<Name>>) {
    for (entity, name, transform) in &named {
        let glow = match name.as_str() {
            "wing_right" | "wing_left" => {
                let side = if name.as_str() == "wing_right" {
                    Side::Right
                } else {
                    Side::Left
                };
                commands.entity(entity).insert(Wing {
                    side,
                    rest: transform.rotation,
                    axis: None,
                });
                continue;
            }
            "eye_main" => GlowSource::MainEye,
            "eye_small" => GlowSource::SmallEye,
            "led_white" => GlowSource::WhiteLed,
            _ => continue,
        };
        commands
            .entity(entity)
            .insert(Glow {
                source: glow,
                materials: Vec::new(),
                adopted: false,
            })
            .with_children(|p| {
                p.spawn((
                    PointLight {
                        intensity: 0.0,
                        range: 3.0,
                        shadows_enabled: false,
                        ..default()
                    },
                    GlowLight(glow),
                ));
            });
    }
}

/// Gives each glowing glTF node private material copies, so lighting one eye
/// doesn't light every mesh that shares its material.
fn adopt_glow_materials(
    mut glows: Query<(Entity, &mut Glow)>,
    children: Query<&Children>,
    mut mesh_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut glow) in &mut glows {
        if glow.adopted {
            continue;
        }
        let targets: Vec<Entity> = std::iter::once(entity)
            .chain(children.iter_descendants(entity))
            .collect();
        for target in targets {
            let Ok(mut handle) = mesh_materials.get_mut(target) else {
                continue;
            };
            let Some(original) = materials.get(&handle.0).cloned() else {
                continue;
            };
            let copy = materials.add(original);
            handle.0 = copy.clone();
            glow.materials.push(copy);
        }
        // Mesh children appear a frame or two after the named node.
        glow.adopted = !glow.materials.is_empty();
    }
}

fn wing_cfg(cfg: &SimConfig, side: Side) -> &crate::config::WingCfg {
    if side == Side::Right {
        &cfg.wing_right
    } else {
        &cfg.wing_left
    }
}

/// The servo angle code.py is asking for; `None` while the servo is unpowered.
fn commanded_angle(cfg: &SimConfig, side: Side) -> Option<f32> {
    let pulse = hw().pulse_us(wing_cfg(cfg, side).channel)?;
    (pulse > 0.0).then(|| cfg.servo.angle_for_pulse(pulse))
}

/// Moves each servo toward its command at the servo's rated speed and warns
/// when code.py keeps asking for more than that. Tracked even with the
/// toolbar's limit off, so the warning still shows.
fn slew_servos(cfg: Res<SimConfig>, mut slew: ResMut<ServoSlew>) {
    let now = hw().clock.now();
    let last = slew.last_now.replace(now).unwrap_or(now);
    if now < last {
        // A restart reset the clock. The servos stay where they physically
        // were; only the lag timing starts over.
        for track in &mut slew.tracks {
            track.lag_since = None;
            track.warned = false;
        }
    }
    let dt = (now - last).max(0.0) as f32;
    let max_step = cfg
        .servo
        .max_speed_dps()
        .map_or(f32::INFINITY, |dps| dps * dt);
    for side in Side::BOTH {
        let Some(target) = commanded_angle(&cfg, side) else {
            continue; // unpowered: the servo stays put
        };
        let track = &mut slew.tracks[side.index()];
        if let Some((lag, behind_s)) = track.follow(target, max_step, now) {
            let at = track.actual.unwrap_or(target);
            hw().push_line(format!(
                "[sim] {} wing servo can't keep up: asked for {target:.0}\u{b0}, still at {at:.0}\u{b0} ({lag:.0}\u{b0} behind for {behind_s:.1} s); it turns 60\u{b0} in {} s",
                side.name(),
                cfg.servo.sec_per_60deg
            ));
        }
    }
}

fn drive_wings(
    cfg: Res<SimConfig>,
    slew: Res<ServoSlew>,
    mut wings: Query<(&Wing, &mut Transform)>,
) {
    let limited = hw().servo_speed;
    for (wing, mut transform) in &mut wings {
        let servo = if limited {
            slew.tracks[wing.side.index()].actual
        } else {
            commanded_angle(&cfg, wing.side)
        };
        let Some(servo) = servo else {
            continue; // unpowered: leave the wing where it is
        };
        let wing_cfg = wing_cfg(&cfg, wing.side);
        let elevation = wing_cfg.elevation_deg(servo);
        let axis = wing.axis.unwrap_or_else(|| {
            Vec3::from(wing_cfg.hinge_axis)
                .try_normalize()
                .unwrap_or(Vec3::Z)
        });
        transform.rotation = wing.rest * Quat::from_axis_angle(axis, elevation.to_radians());
    }
}

/// Linear RGB (0..1 duty per channel) currently driving a light source.
pub fn glow_rgb(cfg: &SimConfig, source: GlowSource) -> Vec3 {
    let s = hw();
    let duty = |ch: usize| s.duty_fraction(ch).unwrap_or(0.0);
    match source {
        GlowSource::MainEye => Vec3::new(
            duty(cfg.eyes.main[0]),
            duty(cfg.eyes.main[1]),
            duty(cfg.eyes.main[2]),
        ),
        GlowSource::SmallEye => Vec3::new(
            duty(cfg.eyes.small[0]),
            duty(cfg.eyes.small[1]),
            duty(cfg.eyes.small[2]),
        ),
        GlowSource::WhiteLed => Vec3::splat(s.white_duty as f32 / 65535.0),
    }
}

fn drive_glow(
    cfg: Res<SimConfig>,
    glows: Query<&Glow>,
    mut lights: Query<(&GlowLight, &mut PointLight)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for glow in &glows {
        let rgb = glow_rgb(&cfg, glow.source);
        for handle in &glow.materials {
            if let Some(m) = materials.get_mut(handle) {
                m.emissive = LinearRgba::rgb(rgb.x, rgb.y, rgb.z) * 40.0;
            }
        }
    }
    for (light, mut point) in &mut lights {
        let rgb = glow_rgb(&cfg, light.0);
        let peak = rgb.max_element();
        point.intensity = peak * 60_000.0;
        if peak > 0.0 {
            point.color = Color::linear_rgb(rgb.x / peak, rgb.y / peak, rgb.z / peak);
        }
    }
}

/// An egui colour as a Bevy sRGB one.
fn to_color(c: bevy_egui::egui::Color32) -> Color {
    Color::srgb_u8(c.r(), c.g(), c.b())
}

/// Room brightness and the theme's scene colours; runs when either changes.
fn apply_room_light(
    room: Res<RoomLight>,
    theme: Res<ActiveTheme>,
    floor: Option<Res<FloorMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ambient: ResMut<AmbientLight>,
    mut lamps: Query<&mut DirectionalLight, With<RoomLamp>>,
    mut cameras: Query<&mut Camera, With<Camera3d>>,
) {
    let (brightness, lux, factor) = room.levels();
    let bg = to_color(theme.palette.scene_bg).to_srgba();
    let background = Color::srgb(bg.red * factor, bg.green * factor, bg.blue * factor);
    if let Some(material) = floor.and_then(|f| materials.get_mut(&f.0)) {
        material.base_color = to_color(theme.palette.floor);
    }
    ambient.brightness = brightness;
    for mut lamp in &mut lamps {
        lamp.illuminance = lux;
    }
    for mut camera in &mut cameras {
        camera.clear_color = ClearColorConfig::Custom(background);
    }
}

fn orbit_camera(
    viewport: Res<ViewportRect>,
    input: MouseInput,
    mut cams: Query<(&mut OrbitCam, &mut Transform)>,
    mut drag: ResMut<DragDistance>,
    mut controls: ResMut<crate::capture::ViewControls>,
    glows: Query<(&Glow, &GlobalTransform)>,
) {
    let MouseInput {
        buttons,
        motion,
        scroll,
    } = input;
    if buttons.get_just_pressed().next().is_some() {
        drag.0 = 0.0;
    }
    drag.0 += motion.delta.length();
    let preset = controls.camera.take();
    // Aim at the main eye's current position, so it works for either model.
    let eye_focus = glows
        .iter()
        .find(|(glow, _)| glow.source() == GlowSource::MainEye)
        .map_or(HOME_FOCUS, |(_, at)| at.translation());
    for (mut cam, mut transform) in &mut cams {
        if let Some(view) = preset {
            apply_view(&mut cam, view, eye_focus);
        }
        if viewport.hovered {
            if buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) {
                cam.yaw -= motion.delta.x * 0.006;
                cam.pitch = (cam.pitch + motion.delta.y * 0.006).clamp(-0.2, 1.45);
            }
            if buttons.pressed(MouseButton::Middle) {
                let right = transform.right();
                let up = transform.up();
                let scale = cam.distance * 0.0015;
                cam.focus += (-right * motion.delta.x + up * motion.delta.y) * scale;
            }
            let lines = match scroll.unit {
                MouseScrollUnit::Line => scroll.delta.y,
                MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
            };
            cam.distance = (cam.distance * (1.0 - lines * 0.1)).clamp(1.5, 20.0);
        }
        let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
        transform.translation = cam.focus + rot * Vec3::new(0.0, 0.0, cam.distance);
        transform.look_at(cam.focus, Vec3::Y);
    }
}

fn apply_view(cam: &mut OrbitCam, view: CameraView, eye_focus: Vec3) {
    let preset = |yaw, pitch| OrbitCam {
        yaw,
        pitch,
        distance: 5.2,
        focus: HOME_FOCUS,
    };
    *cam = match view {
        CameraView::Front => preset(0.0, 0.15),
        CameraView::Side => preset(std::f32::consts::FRAC_PI_2, 0.15),
        // 1.45 is the pitch clamp used when orbiting.
        CameraView::Top => preset(0.0, 1.45),
        CameraView::Eyes => OrbitCam {
            yaw: 0.0,
            pitch: 0.6,
            distance: 1.6,
            focus: eye_focus,
        },
        CameraView::Reset => OrbitCam::home(),
    };
}

/// Points the 3D camera at the dock's 3D-view rectangle, so the scene renders
/// only there and egui's panels never sit on top of it. With the tab hidden
/// there is nothing to draw into, so the camera is switched off.
fn apply_viewport(
    rect: Res<ViewportRect>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<Camera3d>>,
) {
    let wanted = rect.rect.and_then(|r| {
        let window = windows.single().ok()?;
        let (max_w, max_h) = (window.physical_width(), window.physical_height());
        if max_w == 0 || max_h == 0 {
            return None;
        }
        let ppp = rect.ppp;
        // Clamped inside the window: wgpu rejects a viewport that overhangs the target.
        let x = ((r.min.x * ppp).round().max(0.0) as u32).min(max_w - 1);
        let y = ((r.min.y * ppp).round().max(0.0) as u32).min(max_h - 1);
        let w = ((r.width() * ppp).round().max(1.0) as u32).min(max_w - x);
        let h = ((r.height() * ppp).round().max(1.0) as u32).min(max_h - y);
        Some(bevy::render::camera::Viewport {
            physical_position: UVec2::new(x, y),
            physical_size: UVec2::new(w, h),
            ..default()
        })
    });
    for mut camera in &mut cameras {
        let active = wanted.is_some();
        let same_viewport = match (&camera.viewport, &wanted) {
            (Some(a), Some(b)) => {
                a.physical_position == b.physical_position && a.physical_size == b.physical_size
            }
            (None, None) => true,
            _ => false,
        };
        if !same_viewport {
            camera.viewport = wanted.clone();
        }
        if camera.is_active != active {
            camera.is_active = active;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FS90: 60 degrees in 0.12 s.
    const DPS: f32 = 500.0;
    const FRAME_S: f64 = 1.0 / 60.0;

    /// Feeds `target` for `frames` frames from `t0`; returns the warnings.
    fn run(track: &mut ServoTrack, target: f32, frames: usize, t0: f64) -> Vec<(f32, f64)> {
        (1..=frames)
            .filter_map(|i| {
                let now = t0 + i as f64 * FRAME_S;
                track.follow(target, DPS * FRAME_S as f32, now)
            })
            .collect()
    }

    #[test]
    fn slew_limits_both_directions() {
        assert_eq!(slew(50.0, 90.0, 10.0), 60.0);
        assert_eq!(slew(90.0, 50.0, 10.0), 80.0);
        assert_eq!(slew(50.0, 55.0, 10.0), 55.0);
        assert_eq!(slew(50.0, 90.0, f32::INFINITY), 90.0);
        assert_eq!(slew(50.0, 90.0, 0.0), 50.0);
    }

    #[test]
    fn first_command_starts_there() {
        let mut track = ServoTrack::default();
        assert!(track.follow(120.0, 0.0, 0.0).is_none());
        assert_eq!(track.actual, Some(120.0));
    }

    #[test]
    fn full_travel_snap_does_not_warn() {
        let mut track = ServoTrack::default();
        track.follow(0.0, 0.0, 0.0);
        assert!(run(&mut track, 180.0, 60, 0.0).is_empty());
        assert_eq!(track.actual, Some(180.0));
    }

    #[test]
    fn sustained_lag_warns_once() {
        // Command a move the servo can't finish: actual held back by a
        // much slower servo (60 degrees in 2 s).
        let mut track = ServoTrack::default();
        track.follow(0.0, 0.0, 0.0);
        let slow = 30.0 * FRAME_S as f32;
        let warnings: Vec<_> = (1..=60)
            .filter_map(|i| track.follow(90.0, slow, i as f64 * FRAME_S))
            .collect();
        assert_eq!(warnings.len(), 1);
        let (lag, behind_s) = warnings[0];
        assert!(lag > LAG_WARN_DEG);
        assert!(behind_s >= LAG_SUSTAIN_S);
    }

    #[test]
    fn catching_up_allows_a_new_warning() {
        let mut track = ServoTrack::default();
        track.follow(0.0, 0.0, 0.0);
        let slow = 30.0 * FRAME_S as f32;
        let first = (1..=60)
            .filter_map(|i| track.follow(90.0, slow, i as f64 * FRAME_S))
            .count();
        assert_eq!(first, 1);
        // Let it arrive, then lag again.
        let arrived = track.actual.unwrap_or(0.0);
        assert!(run(&mut track, arrived, 1, 1.0).is_empty());
        assert!(track.lag_since.is_none() && !track.warned);
        let again = (1..=60)
            .filter_map(|i| track.follow(0.0, slow, 2.0 + i as f64 * FRAME_S))
            .count();
        assert_eq!(again, 1);
    }
}
