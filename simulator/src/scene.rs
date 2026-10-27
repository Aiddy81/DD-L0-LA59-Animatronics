//! The 3D droid: a placeholder built from primitives, or `assets/droid.glb`
//! if present. Joints and lights are driven from the simulated hardware.

use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::config::SimConfig;
use crate::hw::hw;
use crate::Paths;

pub const MODEL_FILE: &str = "droid.glb";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Right,
    Left,
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
                    drive_wings,
                    drive_glow,
                    orbit_camera,
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
) {
    let gltf_available = paths.assets.join(MODEL_FILE).exists();
    // A saved choice beats sim.toml, which only sets the default.
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
            clear_color: ClearColorConfig::Custom(Color::srgb(0.07, 0.08, 0.1)),
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
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.19, 0.21),
            perceptual_roughness: 0.9,
            ..default()
        })),
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

/// Clicking the droid is a nose boop: one gentle accelerometer nudge. A drag
/// to orbit the camera that happens to start on it doesn't count. Clicks on
/// any part bubble up to the model's root, which has this observer.
fn boop(_: Trigger<Pointer<Click>>, drag: Res<DragDistance>) {
    if drag.0 < 4.0 {
        crate::ui::nudge();
    }
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

fn wing_angle(cfg: &SimConfig, side: Side) -> Option<f32> {
    let wing = if side == Side::Right {
        &cfg.wing_right
    } else {
        &cfg.wing_left
    };
    let pulse = hw().pulse_us(wing.channel)?;
    if pulse <= 0.0 {
        return None; // unpowered servo: leave the wing where it is
    }
    Some(wing.elevation_deg(cfg.servo.angle_for_pulse(pulse)))
}

fn drive_wings(cfg: Res<SimConfig>, mut wings: Query<(&Wing, &mut Transform)>) {
    for (wing, mut transform) in &mut wings {
        let Some(elevation) = wing_angle(&cfg, wing.side) else {
            continue;
        };
        let wing_cfg = if wing.side == Side::Right {
            &cfg.wing_right
        } else {
            &cfg.wing_left
        };
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

fn orbit_camera(
    mut contexts: EguiContexts,
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
    let over_ui = contexts
        .try_ctx_mut()
        .is_some_and(|ctx| ctx.wants_pointer_input() || ctx.is_pointer_over_area());
    for (mut cam, mut transform) in &mut cams {
        if let Some(view) = preset {
            apply_view(&mut cam, view, eye_focus);
        }
        if !over_ui {
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
