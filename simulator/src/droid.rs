//! The primitive L0-LA59 (the toolbar's "Primitive" model): a red saucer
//! head with a white spine, a fixed face plate around the eyes and two rear
//! white panels as the servo-driven "wings". As on the real droid, the
//! panels hinge along their front edge (the Y-shaped groove), so their backs
//! lift. Below: gear ring, black stem and round wooden base.
//!
//! Bevy axes: Y up, +Z forward (the droid faces the default camera).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::scene::{Glow, GlowLight, GlowSource, Side, Wing};

// The head is an ellipsoid: radii RX (side), RY (height), RZ (front-back).
const RX: f32 = 1.25;
const RY: f32 = 0.5;
const RZ: f32 = 1.25;
const CY: f32 = 1.6;
/// Half-width of the centre spine.
const SPINE: f32 = 0.11;
/// Where the spine ends and the Y-shaped grooves fork (z; negative is back).
const SPINE_FRONT: f32 = -0.08;
/// Where the wing panels' front edge meets the rim (z).
const WING_FRONT_AT_RIM: f32 = 0.2;
/// White panels reach this fraction of the head's radius, squared.
const WHITE_R2: f32 = 0.9;
/// Gap between panels.
const GROOVE: f32 = 0.02;
/// Main eye, as a point on the head's top surface (x, z).
const MAIN_EYE: Vec2 = Vec2::new(0.0, 0.66);
const SMALL_EYE: Vec2 = Vec2::new(-0.86, 0.5);

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

/// Underside of the saucer is flatter than the top, giving the lens shape.
const RY_BOTTOM: f32 = 0.16;

/// |x| of the wing panels' front edge (the Y-shaped groove) at depth `z`.
fn wing_edge_x(z: f32) -> f32 {
    let rim = half_width(WING_FRONT_AT_RIM, WHITE_R2);
    let t = (z - SPINE_FRONT) / (WING_FRONT_AT_RIM - SPINE_FRONT);
    SPINE + t.max(0.0) * (rim - SPINE)
}

/// A wing's hinge: the straight line along its front edge, from where the
/// groove leaves the spine to where it meets the rim. Returns (pivot, axis),
/// with the axis oriented so a positive angle lifts the back of the panel.
fn hinge(sign: f32, lift: f32) -> (Vec3, Vec3) {
    let rim = half_width(WING_FRONT_AT_RIM, WHITE_R2);
    let start = Vec3::new(
        sign * SPINE,
        shell_y(SPINE, SPINE_FRONT) + lift,
        SPINE_FRONT,
    );
    let end = Vec3::new(
        sign * rim,
        shell_y(rim, WING_FRONT_AT_RIM) + lift,
        WING_FRONT_AT_RIM,
    );
    let axis = if sign > 0.0 { end - start } else { start - end };
    (start.lerp(end, 0.45), axis.normalize())
}

/// A slot pressed into a panel: a thin dark bar lying on the head's surface,
/// running front to back. `offset` is subtracted from its position.
fn slot_transform(x: f32, z: f32, lift: f32, length: f32, offset: Vec3) -> Transform {
    let (at, n) = on_shell(Vec2::new(x, z), lift);
    Transform::from_translation(at - offset)
        .with_rotation(Quat::from_rotation_arc(Vec3::Y, n))
        .with_scale(Vec3::new(0.022, 0.014, length))
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

    let back = -RZ * WHITE_R2.sqrt();

    // Dark underlay, so the grooves between the white panels read as dark
    // lines. It stops where the face plate does, so red shows below the eyes.
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(shell_patch(
            0.008,
            Vec3::ZERO,
            back * 1.02,
            MAIN_EYE.y + 0.01,
            |z| {
                let w = half_width(z, WHITE_R2 + 0.02);
                Some((-w, w))
            },
        ))),
        MeshMaterial3d(p.black.clone()),
    ));

    // Spine: from the back rim to where the grooves fork, with two slots.
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(shell_patch(
            0.045,
            Vec3::ZERO,
            back * 0.99,
            SPINE_FRONT,
            |z| {
                let w = (SPINE - GROOVE).min(half_width(z, WHITE_R2));
                Some((-w, w))
            },
        ))),
        MeshMaterial3d(p.white.clone()),
    ));
    let slot = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for x in [-0.04, 0.04] {
        commands.spawn((
            ChildOf(p.root),
            Mesh3d(slot.clone()),
            MeshMaterial3d(p.black.clone()),
            slot_transform(x, -0.42, 0.045, 0.16, Vec3::ZERO),
        ));
    }

    // Fixed face plate at the front, around the eyes, in front of the grooves.
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(shell_patch(
            0.03,
            Vec3::ZERO,
            SPINE_FRONT + GROOVE,
            MAIN_EYE.y + 0.02,
            |z| {
                let w = half_width(z, WHITE_R2).min(wing_edge_x(z) - GROOVE);
                (w > 0.0).then_some((-w, w))
            },
        ))),
        MeshMaterial3d(p.white.clone()),
    ));
}

fn spawn_wings(commands: &mut Commands, meshes: &mut Assets<Mesh>, p: &Palette) {
    let back = -RZ * WHITE_R2.sqrt();
    let slot = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for (side, sign) in [(Side::Right, 1.0_f32), (Side::Left, -1.0)] {
        let (pivot, axis) = hinge(sign, 0.03);
        // Behind the Y groove: in front of the fork, only outboard of it.
        let panel = shell_patch(0.03, pivot, back, WING_FRONT_AT_RIM + 0.05, move |z| {
            let mut lo = SPINE + GROOVE / 2.0;
            if z > SPINE_FRONT {
                lo = lo.max(wing_edge_x(z) + GROOVE / 2.0);
            }
            let hi = half_width(z, WHITE_R2);
            (hi > lo).then_some(if sign > 0.0 { (lo, hi) } else { (-hi, -lo) })
        });
        commands
            .spawn((
                ChildOf(p.root),
                Transform::from_translation(pivot),
                Visibility::default(),
                Wing {
                    side,
                    rest: Quat::IDENTITY,
                    axis: Some(axis),
                },
            ))
            .with_children(|hinge| {
                hinge.spawn((Mesh3d(meshes.add(panel)), MeshMaterial3d(p.white.clone())));
                hinge.spawn((
                    Mesh3d(slot.clone()),
                    MeshMaterial3d(p.black.clone()),
                    slot_transform(sign * 0.6, -0.62, 0.03, 0.2, pivot),
                ));
            });
    }
}

fn spawn_details(commands: &mut Commands, meshes: &mut Assets<Mesh>, p: &Palette) {
    // Antenna at the back of the spine, red sleeve near the tip.
    let (root, _) = on_shell(Vec2::new(0.0, -1.0), 0.06);
    commands.spawn((
        ChildOf(p.root),
        Mesh3d(meshes.add(Cuboid::new(0.14, 0.09, 0.22))),
        MeshMaterial3d(p.white.clone()),
        Transform::from_translation(root),
    ));
    let tip = root + Vec3::new(0.0, 0.5, -0.22);
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
