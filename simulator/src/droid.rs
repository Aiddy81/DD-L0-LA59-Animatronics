//! Placeholder L0-LA59 built from primitives, used when there is no
//! `assets/droid.glb`: a red saucer head whose two white top panels are the
//! servo-driven "wings", hinged along the centre spine, over a gear ring,
//! black stem and round wooden base.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::scene::{Glow, GlowLight, GlowSource, Side, Wing};

// The head is an ellipsoid: radii RX (side), RY (height), RZ (front-back).
const RX: f32 = 1.25;
const RY: f32 = 0.42;
const RZ: f32 = 1.25;
const CY: f32 = 1.6;
/// Half-width of the centre spine; the wing panels hinge along its edges.
const SPINE: f32 = 0.16;
/// Main eye, as a point on the head's top surface (x, z).
const MAIN_EYE: Vec2 = Vec2::new(0.0, 0.74);
const SMALL_EYE: Vec2 = Vec2::new(-0.88, 0.56);

fn shell_y(x: f32, z: f32) -> f32 {
    let r2 = (x / RX).powi(2) + (z / RZ).powi(2);
    CY + RY * (1.0 - r2).max(0.0).sqrt()
}

fn shell_normal(x: f32, z: f32) -> Vec3 {
    Vec3::new(
        x / (RX * RX),
        (shell_y(x, z) - CY) / (RY * RY),
        z / (RZ * RZ),
    )
    .normalize()
}

fn on_shell(at: Vec2, lift: f32) -> (Vec3, Vec3) {
    let n = shell_normal(at.x, at.y);
    (Vec3::new(at.x, shell_y(at.x, at.y), at.y) + n * lift, n)
}

/// Radius of the clearance around the main eye's bezel.
const EYE_CLEAR: f32 = 0.47;
/// Underside of the saucer is flatter than the top, giving the lens shape.
const RY_BOTTOM: f32 = 0.16;

/// How far (in x) the eye's clearance circle reaches at depth `z`.
fn eye_half_width(z: f32) -> f32 {
    let dz = z - MAIN_EYE.y;
    (EYE_CLEAR * EYE_CLEAR - dz * dz).max(0.0).sqrt()
}

/// x half-width of the head's top at depth `z`, within radius fraction `r2`.
fn half_width(z: f32, r2: f32) -> f32 {
    RX * (r2 - (z / RZ).powi(2)).max(0.0).sqrt()
}

/// 0..1 for step `i` of `n`, packed towards both ends where the head's
/// surface turns steep, so the rim doesn't facet.
fn cosine_step(i: usize, n: usize) -> f32 {
    (1.0 - (std::f32::consts::PI * i as f32 / n as f32).cos()) / 2.0
}

/// A patch of the head's top surface, built in rows along z so its outline
/// stays smooth: row `z` spans x from `span(z).0` to `span(z).1`. Raised by
/// `lift` and expressed relative to `pivot` so it can rotate about a hinge.
fn shell_patch(
    lift: f32,
    pivot: Vec3,
    z0: f32,
    z1: f32,
    span: impl Fn(f32) -> Option<(f32, f32)>,
) -> Mesh {
    const NZ: usize = 180;
    const NX: usize = 64;
    let (mut positions, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
    let mut rows = Vec::with_capacity(NZ + 1);
    for i in 0..=NZ {
        let z = z0 + (z1 - z0) * cosine_step(i, NZ);
        let row = span(z).filter(|(lo, hi)| hi > lo).map(|(lo, hi)| {
            let base = positions.len() as u32;
            for j in 0..=NX {
                let x = lo + (hi - lo) * cosine_step(j, NX);
                let n = shell_normal(x, z);
                let p = Vec3::new(x, shell_y(x, z), z) + n * lift - pivot;
                positions.push(p.to_array());
                normals.push(n.to_array());
                uvs.push([j as f32 / NX as f32, i as f32 / NZ as f32]);
            }
            base
        });
        rows.push(row);
    }
    let mut indices = Vec::new();
    for pair in rows.windows(2) {
        let (Some(a), Some(b)) = (pair[0], pair[1]) else {
            continue;
        };
        for j in 0..NX as u32 {
            // Counter-clockwise seen from above, so the outside is the front face.
            indices.extend([a + j, b + j + 1, a + j + 1, a + j, b + j, b + j + 1]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// A cylinder running from `a` to `b`.
fn rod(a: Vec3, b: Vec3) -> Transform {
    Transform::from_translation((a + b) / 2.0)
        .with_rotation(Quat::from_rotation_arc(Vec3::Y, (b - a).normalize()))
}

struct Palette {
    /// Everything spawns under this, so the model can be swapped out whole.
    root: Entity,
    red: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    black: Handle<StandardMaterial>,
    steel: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
}

fn matte(
    materials: &mut Assets<StandardMaterial>,
    r: f32,
    g: f32,
    b: f32,
    rough: f32,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        perceptual_roughness: rough,
        double_sided: true,
        cull_mode: None,
        ..default()
    })
}

pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
) {
    let p = Palette {
        root,
        red: matte(materials, 0.5, 0.1, 0.08, 0.6),
        white: matte(materials, 0.86, 0.85, 0.81, 0.55),
        black: matte(materials, 0.05, 0.05, 0.055, 0.5),
        steel: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.6, 0.62),
            metallic: 0.85,
            perceptual_roughness: 0.35,
            ..default()
        }),
        wood: matte(materials, 0.24, 0.09, 0.06, 0.7),
    };

    spawn_base(commands, meshes, &p);
    spawn_head(commands, meshes, &p);
    spawn_wings(commands, meshes, &p);
    spawn_details(commands, meshes, &p);

    spawn_eye(
        commands,
        meshes,
        materials,
        &p,
        GlowSource::MainEye,
        MAIN_EYE,
        0.36,
        0.3,
    );
    spawn_eye(
        commands,
        meshes,
        materials,
        &p,
        GlowSource::SmallEye,
        SMALL_EYE,
        0.14,
        0.11,
    );
    // The white "data processing" LED: a small lamp on the front of the gear ring.
    let led_at = Vec3::new(0.32, CY - 0.18, 0.97);
    spawn_lamp(
        commands,
        meshes,
        materials,
        root,
        GlowSource::WhiteLed,
        led_at,
        led_at.with_y(0.0).normalize(),
        0.04,
    );
}

fn spawn_base(commands: &mut Commands, meshes: &mut Assets<Mesh>, p: &Palette) {
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(1.05, 0.12))),
        MeshMaterial3d(p.black.clone()),
        Transform::from_xyz(0.0, 0.06, 0.0),
    ));
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(0.97, 0.2))),
        MeshMaterial3d(p.wood.clone()),
        Transform::from_xyz(0.0, 0.22, 0.0),
    ));
    // The white emblem, simplified to its outer ring and central shield.
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Torus::new(0.66, 0.72))),
        MeshMaterial3d(p.white.clone()),
        Transform::from_xyz(0.0, 0.32, 0.0).with_scale(Vec3::new(1.0, 0.25, 1.0)),
    ));
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Sphere::new(0.17))),
        MeshMaterial3d(p.wood.clone()),
        Transform::from_xyz(0.0, 0.32, 0.12).with_scale(Vec3::new(1.0, 0.15, 1.35)),
    ));

    // Stem and its fins.
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(0.17, CY - 0.69))),
        MeshMaterial3d(p.black.clone()),
        Transform::from_xyz(0.0, (CY - 0.69) / 2.0 + 0.32, 0.0),
    ));
    let fin = meshes.add(Cuboid::new(0.07, 0.42, 0.3));
    for k in 0..4 {
        let angle = std::f32::consts::FRAC_PI_4 + k as f32 * std::f32::consts::FRAC_PI_2;
        let dir = Vec3::new(angle.sin(), 0.0, angle.cos());
        commands.spawn((
            ChildOf(p.root),
            Mesh3d(fin.clone()),
            MeshMaterial3d(p.black.clone()),
            Transform::from_translation(Vec3::new(0.0, 0.6, 0.0) + dir * 0.25)
                .with_rotation(Quat::from_rotation_y(angle)),
        ));
    }
}

fn spawn_head(commands: &mut Commands, meshes: &mut Assets<Mesh>, p: &Palette) {
    // Red saucer: a domed top over a flatter underside.
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(shell_patch(0.0, Vec3::ZERO, -RZ, RZ, |z| {
            let w = half_width(z, 1.0);
            Some((-w, w))
        }))),
        MeshMaterial3d(p.red.clone()),
    ));
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(96, 48))),
        MeshMaterial3d(p.red.clone()),
        Transform::from_xyz(0.0, CY, 0.0).with_scale(Vec3::new(RX * 0.998, RY_BOTTOM, RZ * 0.998)),
    ));

    // Underside: black lip, grey band, and the exposed gear ring.
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(1.02, 0.1))),
        MeshMaterial3d(p.black.clone()),
        Transform::from_xyz(0.0, CY - 0.18, 0.0),
    ));
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Torus::new(0.97, 1.03))),
        MeshMaterial3d(p.steel.clone()),
        Transform::from_xyz(0.0, CY - 0.24, 0.0).with_scale(Vec3::new(1.0, 0.6, 1.0)),
    ));
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(0.86, 0.12))),
        MeshMaterial3d(p.black.clone()),
        Transform::from_xyz(0.0, CY - 0.31, 0.0),
    ));
    let tooth = meshes.add(Cuboid::new(0.07, 0.1, 0.05));
    for k in 0..56 {
        let angle = k as f32 / 56.0 * std::f32::consts::TAU;
        commands.spawn((
            ChildOf(p.root),
            Mesh3d(tooth.clone()),
            MeshMaterial3d(p.black.clone()),
            Transform::from_xyz(angle.sin() * 0.88, CY - 0.31, angle.cos() * 0.88)
                .with_rotation(Quat::from_rotation_y(angle)),
        ));
    }

    // Raised centre spine, running front to back and stopping at the eye.
    let w = SPINE - 0.025;
    let spine_front = MAIN_EYE.y - (EYE_CLEAR * EYE_CLEAR - w * w).sqrt();
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(shell_patch(
            0.035,
            Vec3::ZERO,
            -RZ * 0.92,
            spine_front,
            |z| {
                let edge = half_width(z, 0.86).min(w);
                Some((-edge, edge))
            },
        ))),
        MeshMaterial3d(p.white.clone()),
    ));
}

fn spawn_wings(commands: &mut Commands, meshes: &mut Assets<Mesh>, p: &Palette) {
    for (side, sign) in [(Side::Right, 1.0_f32), (Side::Left, -1.0)] {
        let pivot = Vec3::new(sign * SPINE, shell_y(SPINE, 0.0) + 0.02, 0.0);
        // White top: out to 86% of the head's radius, cut around the eye.
        let reach = RZ * 0.74_f32.sqrt();
        let panel = shell_patch(0.02, pivot, -reach, reach, move |z| {
            let lo = SPINE.max(eye_half_width(z));
            let hi = half_width(z, 0.74);
            if hi <= lo {
                return None;
            }
            Some(if sign > 0.0 { (lo, hi) } else { (-hi, -lo) })
        });
        commands
            .spawn((
                ChildOf(p.root),
                Transform::from_translation(pivot),
                Visibility::default(),
                Wing {
                    side,
                    rest: Quat::IDENTITY,
                },
            ))
            .with_children(|hinge| {
                hinge.spawn((Mesh3d(meshes.add(panel)), MeshMaterial3d(p.white.clone())));
            });
    }
}

fn spawn_details(commands: &mut Commands, meshes: &mut Assets<Mesh>, p: &Palette) {
    // Antenna at the back of the spine, red sleeve near the tip.
    let (root, _) = on_shell(Vec2::new(0.0, -0.86), 0.04);
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cuboid::new(0.14, 0.09, 0.22))),
        MeshMaterial3d(p.white.clone()),
        Transform::from_translation(root),
    ));
    let tip = root + Vec3::new(0.0, 0.55, -0.12);
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(0.012, root.distance(tip)))),
        MeshMaterial3d(p.steel.clone()),
        rod(root, tip),
    ));
    let sleeve_a = root.lerp(tip, 0.72);
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(0.028, sleeve_a.distance(tip)))),
        MeshMaterial3d(p.red.clone()),
        rod(sleeve_a, tip),
    ));

    // Plug arm hanging from under the right side of the head.
    let top = Vec3::new(0.8, CY - 0.2, 0.42);
    let bottom = Vec3::new(1.0, 0.95, 0.6);
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cylinder::new(0.03, top.distance(bottom)))),
        MeshMaterial3d(p.black.clone()),
        rod(top, bottom),
    ));
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cone::new(0.09, 0.2))),
        MeshMaterial3d(p.black.clone()),
        Transform::from_translation(bottom + Vec3::new(0.0, -0.06, 0.0)),
    ));
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cuboid::new(0.1, 0.3, 0.07))),
        MeshMaterial3d(p.black.clone()),
        Transform::from_translation(bottom + Vec3::new(0.0, -0.32, 0.0)),
    ));
    let prong = meshes.add(Cylinder::new(0.008, 0.14));
    for dx in [-0.025_f32, 0.025] {
        commands.spawn((
            ChildOf(p.root),
            Mesh3d(prong.clone()),
            MeshMaterial3d(p.steel.clone()),
            Transform::from_translation(bottom + Vec3::new(dx, -0.53, 0.0)),
        ));
    }
}

/// An eye: steel bezel on the head surface around a glowing glass dome.
#[allow(clippy::too_many_arguments)]
fn spawn_eye(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    p: &Palette,
    source: GlowSource,
    at: Vec2,
    bezel: f32,
    lens: f32,
) {
    let (centre, normal) = on_shell(at, 0.0);
    let facing = Quat::from_rotation_arc(Vec3::Y, normal);
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Torus::new(lens, bezel))),
        MeshMaterial3d(p.steel.clone()),
        Transform::from_translation(centre + normal * 0.02)
            .with_rotation(facing)
            .with_scale(Vec3::new(1.0, 0.8, 1.0)),
    ));
    spawn_lamp(
        commands, meshes, materials, p.root, source, centre, normal, lens,
    );
}

/// A glowing dome with a point light in front of it.
#[allow(clippy::too_many_arguments)]
fn spawn_lamp(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    root: Entity,
    source: GlowSource,
    at: Vec3,
    normal: Vec3,
    radius: f32,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.04, 0.04, 0.05),
        perceptual_roughness: 0.05,
        reflectance: 0.8,
        ..default()
    });
    commands
        .spawn((
            ChildOf(root),
            Mesh3d(meshes.add(Sphere::new(radius).mesh().uv(32, 16))),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(at)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, normal))
                .with_scale(Vec3::new(1.0, 0.55, 1.0)),
            Glow::placeholder(source, material),
        ))
        .with_children(|lamp| {
            lamp.spawn((
                PointLight {
                    intensity: 0.0,
                    range: 3.0,
                    radius,
                    shadows_enabled: false,
                    ..default()
                },
                // Parent is squashed on Y; this sits just outside the dome.
                Transform::from_xyz(0.0, 3.0 * radius, 0.0),
                GlowLight(source),
            ));
        });
}
