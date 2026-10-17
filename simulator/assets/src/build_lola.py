"""Builds simulator/assets/droid.glb, the L0-LA59 model the simulator shows.

Run with Blender 5.x (no other setup needed):

    blender --background --factory-startup --python simulator/assets/src/build_lola.py -- simulator/assets/droid.glb

Modelled from photos of the real prop. The simulator drives these nodes by
name: wing_right and wing_left (the two rear white panels, origin on their
hinge along the spine), eye_main, eye_small and led_white (glowing lenses).
Blender is Z-up with the droid facing -Y; the glTF exporter converts that to
the simulator's Y-up, facing +Z.
"""
import math
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

OUT = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "droid.glb"

# Upper shell: an ellipsoid dome over a shallow underside, rim at height CZ.
R = 1.25
H_TOP = 0.5
H_BOT = 0.07
CZ = 1.5
SPINE = 0.11              # half-width of the centre spine; panels hinge on its edges
SPINE_FRONT = 0.08        # where the spine ends and the Y-shaped grooves begin
WING_FRONT_AT_RIM = -0.2  # where the panels' front edge meets the rim
WHITE_R2 = 0.9            # white panels reach this fraction of radius squared
GROOVE = 0.02             # gap between panels
MAIN_EYE = (0.0, -0.66)
SMALL_EYE = (-0.86, -0.5)

# Below the shell: band and gear ring, then the lower red disc and plate.
LOWER_DISC_Z = CZ - 0.22
UNDERSIDE_Z = CZ - 0.3
BASE_TOP = 0.34
ROD_AT = (0.0, 0.5)       # the support rod, toward the back


def top_z(x, y):
    return CZ + H_TOP * math.sqrt(max(0.0, 1.0 - (x / R) ** 2 - (y / R) ** 2))


def normal(x, y):
    return Vector((x / R ** 2, y / R ** 2, (top_z(x, y) - CZ) / H_TOP ** 2)).normalized()


def half_width(y, r2):
    return R * math.sqrt(max(0.0, r2 - (y / R) ** 2))


def wing_edge_x(y):
    """|x| of the panels' front edge at depth y (the Y-shaped grooves)."""
    rim = half_width(WING_FRONT_AT_RIM, WHITE_R2)
    t = (SPINE_FRONT - y) / (SPINE_FRONT - WING_FRONT_AT_RIM)
    return SPINE + max(0.0, t) * (rim - SPINE)


def cos_step(i, n):
    return (1.0 - math.cos(math.pi * i / n)) / 2.0


# ---------------------------------------------------------------- materials

def material(name, rgb, metallic=0.0, rough=0.5):
    mat = bpy.data.materials.new(name)
    if mat.node_tree is None:
        mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = rough
    return mat


M = {}


def make_materials():
    M["red"] = material("red", (0.36, 0.05, 0.04), rough=0.6)
    M["white"] = material("white", (0.78, 0.77, 0.73), rough=0.6)
    M["black"] = material("black", (0.025, 0.025, 0.028), rough=0.5)
    M["iron"] = material("iron", (0.12, 0.12, 0.13), metallic=0.7, rough=0.45)
    M["steel"] = material("steel", (0.6, 0.6, 0.62), metallic=0.9, rough=0.3)
    M["grey"] = material("grey", (0.62, 0.62, 0.62), rough=0.55)
    M["bronze"] = material("bronze", (0.42, 0.27, 0.13), metallic=0.4, rough=0.55)
    M["dark"] = material("dark", (0.03, 0.03, 0.035), rough=0.6)


def lens_material(name):
    # Each lens has its own material; the simulator lights it from code.py.
    return material(name, (0.02, 0.02, 0.025), rough=0.06)


# ---------------------------------------------------------------- helpers

# Mesh data gets a "_mesh" suffix: the simulator binds behaviour by node name,
# and Bevy also names mesh entities after their mesh, which would bind twice.
def new_object(name, bm):
    mesh = bpy.data.meshes.new(name + "_mesh")
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def finish(obj, mat, smooth=True, parent=None):
    obj.data.materials.clear()
    obj.data.materials.append(mat)
    if smooth:
        for poly in obj.data.polygons:
            poly.use_smooth = True
    if parent is not None:
        bpy.context.view_layer.update()
        obj.parent = parent
        obj.matrix_parent_inverse = parent.matrix_world.inverted()
    return obj


def add_modifier(obj, kind, **props):
    mod = obj.modifiers.new(kind.title(), kind)
    for key, value in props.items():
        setattr(mod, key, value)
    return mod


def bevelled(obj, width=0.008, segments=2):
    add_modifier(obj, "BEVEL", width=width, segments=segments, limit_method="ANGLE")
    return obj


def primitive(op, name, mat, smooth=True, parent=None, **kwargs):
    op(**kwargs)
    obj = bpy.context.active_object
    obj.name = name
    obj.data.name = name + "_mesh"
    return finish(obj, mat, smooth, parent)


def facing(n):
    return Vector((0, 0, 1)).rotation_difference(n).to_euler()


def surface_patch(name, y0, y1, span, lift, pivot, ny=160, nx=48):
    """A patch of the dome built row by row (rows along y, each spanning
    span(y) in x) so its outline stays smooth; vertices relative to pivot."""
    bm = bmesh.new()
    rows = []
    for i in range(ny + 1):
        y = y0 + (y1 - y0) * cos_step(i, ny)
        s = span(y)
        if s is None or s[1] <= s[0]:
            rows.append(None)
            continue
        row = []
        for j in range(nx + 1):
            x = s[0] + (s[1] - s[0]) * cos_step(j, nx)
            p = Vector((x, y, top_z(x, y))) + normal(x, y) * lift - pivot
            row.append(bm.verts.new(p))
        rows.append(row)
    for a, b in zip(rows, rows[1:]):
        if a is None or b is None:
            continue
        for j in range(nx):
            bm.faces.new((a[j], a[j + 1], b[j + 1], b[j]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    obj = new_object(name, bm)
    obj.location = pivot
    return obj


def panel(name, y0, y1, span, pivot, thickness=0.035, lift=0.045):
    obj = surface_patch(name, y0, y1, span, lift, pivot)
    add_modifier(obj, "SOLIDIFY", thickness=thickness, offset=-1.0)
    bevelled(obj, 0.012, 3)
    return finish(obj, M["white"])


def on_dome(x, y, lift):
    return Vector((x, y, top_z(x, y))) + normal(x, y) * lift


def slot(name, x, y, length, parent=None, lift=0.045, along_y=True):
    """A dark slot pressed into a panel's top face."""
    scale = (0.022, length, 0.014) if along_y else (length, 0.022, 0.014)
    return primitive(bpy.ops.mesh.primitive_cube_add, name, M["dark"], smooth=False,
                     parent=parent, size=1, location=on_dome(x, y, lift), scale=scale,
                     rotation=facing(normal(x, y)))


def rod(name, a, b, radius, mat, parent=None, vertices=16):
    a, b = Vector(a), Vector(b)
    d = b - a
    return primitive(bpy.ops.mesh.primitive_cylinder_add, name, mat, parent=parent,
                     vertices=vertices, radius=radius, depth=d.length,
                     location=(a + b) / 2, rotation=facing(d.normalized()))


def cylinder(name, mat, radius, depth, z, x=0.0, y=0.0, vertices=96, bevel=0.0):
    obj = primitive(bpy.ops.mesh.primitive_cylinder_add, name, mat,
                    vertices=vertices, radius=radius, depth=depth, location=(x, y, z))
    if bevel:
        bevelled(obj, bevel, 3)
    return obj


def box(name, mat, location, scale, rotation=(0, 0, 0), bevel=0.01, parent=None):
    obj = primitive(bpy.ops.mesh.primitive_cube_add, name, mat, smooth=False, parent=parent,
                    size=1, location=location, scale=scale, rotation=rotation)
    if bevel:
        bevelled(obj, bevel)
    return obj


# ---------------------------------------------------------------- head

def build_upper_shell():
    bpy.ops.mesh.primitive_uv_sphere_add(segments=128, ring_count=64, radius=1.0)
    shell = bpy.context.active_object
    shell.name = "head"
    shell.data.name = "head_mesh"
    for v in shell.data.vertices:
        v.co.x *= R
        v.co.y *= R
        v.co.z *= H_TOP if v.co.z > 0 else H_BOT
    shell.location = (0, 0, CZ)
    finish(shell, M["red"])

    # Dark underlay, so the grooves between the white panels read as dark lines.
    # It stops where the face plate does, so red still shows below the eyes.
    underlay = surface_patch("panel_underlay", MAIN_EYE[1] - 0.01, R * 0.95,
                             lambda y: (-half_width(y, WHITE_R2 + 0.02), half_width(y, WHITE_R2 + 0.02)),
                             lift=0.008, pivot=Vector((0, 0, CZ)))
    finish(underlay, M["dark"])

    # Spine: from the back rim to where the grooves fork.
    back = R * math.sqrt(WHITE_R2) * 0.99
    spine = panel("spine", SPINE_FRONT, back,
                  lambda y: (-min(SPINE - GROOVE, half_width(y, WHITE_R2)),
                             min(SPINE - GROOVE, half_width(y, WHITE_R2))),
                  pivot=Vector((0, 0, CZ)), thickness=0.045, lift=0.065)
    for dx in (-0.04, 0.04):
        slot("spine_slot", dx, 0.42, 0.16, lift=0.065)

    # Fixed face plate at the front, around the eyes, below the grooves.
    front = MAIN_EYE[1] - 0.02

    def face_span(y):
        hi = min(half_width(y, WHITE_R2), wing_edge_x(y) - GROOVE)
        return (-hi, hi) if hi > 0 else None

    panel("face_plate", front, SPINE_FRONT - GROOVE, face_span, pivot=Vector((0, 0, CZ)))
    return spine


def build_wings():
    back = R * math.sqrt(WHITE_R2)
    for name, sign in (("wing_right", 1.0), ("wing_left", -1.0)):
        pivot = Vector((sign * SPINE, 0.0, top_z(SPINE, 0.0) + 0.03))

        def span(y, sign=sign):
            # Behind the Y groove: in front of the fork, only outboard of it.
            lo = SPINE + GROOVE / 2
            if y < SPINE_FRONT:
                lo = max(lo, wing_edge_x(y) + GROOVE / 2)
            hi = half_width(y, WHITE_R2)
            if hi <= lo:
                return None
            return (lo, hi) if sign > 0 else (-hi, -lo)

        wing = panel(name, WING_FRONT_AT_RIM - 0.05, back, span, pivot)
        slot(name + "_slot", sign * 0.6, 0.62, 0.2, parent=wing)


def build_eye(name, at, bezel_r, lens_r, collar=True):
    n = normal(*at)
    centre = Vector((at[0], at[1], top_z(*at)))
    if collar:
        primitive(bpy.ops.mesh.primitive_torus_add, name + "_collar", M["white"],
                  major_radius=bezel_r * 1.12, minor_radius=bezel_r * 0.1,
                  major_segments=64, minor_segments=12,
                  location=centre + n * 0.05, rotation=facing(n))
    primitive(bpy.ops.mesh.primitive_torus_add, name + "_bezel", M["steel"],
              major_radius=bezel_r, minor_radius=bezel_r * 0.13,
              major_segments=64, minor_segments=16,
              location=centre + n * 0.07, rotation=facing(n))
    primitive(bpy.ops.mesh.primitive_cylinder_add, name + "_socket", M["dark"],
              vertices=48, radius=bezel_r * 0.97, depth=0.1,
              location=centre + n * 0.03, rotation=facing(n))
    lens = primitive(bpy.ops.mesh.primitive_uv_sphere_add, name, lens_material(name + "_lens"),
                     segments=48, ring_count=24, radius=lens_r,
                     location=centre + n * 0.07, rotation=facing(n))
    lens.scale = (1.0, 1.0, 0.6)
    return lens


def build_middle_and_lower():
    # Gap under the shell: dark gear ring with a silver band.
    cylinder("gear_ring", M["black"], 1.02, 0.1, CZ - 0.1)
    bm = bmesh.new()
    for k in range(72):
        a = k / 72 * math.tau
        m = (Matrix.Translation((math.sin(a) * 1.035, math.cos(a) * 1.035, CZ - 0.1))
             @ Matrix.Rotation(-a, 4, "Z") @ Matrix.Diagonal((0.045, 0.05, 0.08, 1.0)))
        bmesh.ops.create_cube(bm, size=1.0, matrix=m)
    finish(new_object("gear_teeth", bm), M["black"], smooth=False)
    cylinder("band", M["steel"], 1.1, 0.04, CZ - 0.16, bevel=0.01)

    # Lower red disc with its rim holes, then the flat grey underside plate.
    cylinder("lower_disc", M["red"], 1.2, 0.09, LOWER_DISC_Z, bevel=0.025)
    for centre_deg in (20, 60, 120, 160, 200, 250, 290, 340):
        for k in (-1, 1):
            a = math.radians(centre_deg + k * 3.5)
            n = Vector((math.cos(a), math.sin(a), 0.0))
            primitive(bpy.ops.mesh.primitive_cylinder_add, "rim_hole", M["dark"],
                      vertices=12, radius=0.022, depth=0.04,
                      location=n * 1.19 + Vector((0, 0, LOWER_DISC_Z)), rotation=facing(n))
    cylinder("underside_plate", M["grey"], 0.92, 0.05, UNDERSIDE_Z + 0.03, bevel=0.015)
    plate = primitive(bpy.ops.mesh.primitive_uv_sphere_add, "underside_dome", M["grey"],
                      segments=64, ring_count=32, radius=0.9, location=(0, 0, UNDERSIDE_Z + 0.03))
    plate.scale = (1.0, 1.0, 0.07)

    # White "data processing" LED, peeking out of the gap at the front.
    n = Vector((0.3, -0.95, 0.0)).normalized()
    led = primitive(bpy.ops.mesh.primitive_uv_sphere_add, "led_white", lens_material("led_lens"),
                    segments=24, ring_count=12, radius=0.04,
                    location=n * 1.06 + Vector((0, 0, CZ - 0.09)), rotation=facing(n))
    led.scale = (1.0, 1.0, 0.55)


def build_antenna():
    y = R * math.sqrt(WHITE_R2) * 0.86
    root = on_dome(0, y, 0.11)
    primitive(bpy.ops.mesh.primitive_cylinder_add, "antenna_base", M["white"],
              vertices=24, radius=0.045, depth=0.08, location=root, rotation=facing(normal(0, y)))
    # A bent wire leaning back, with a red sleeve near the tip.
    pts = [root + Vector((0, 0.22 * t * t, 0.48 * t)) for t in [i / 8 for i in range(9)]]
    pts.append(pts[-1] + Vector((0, 0.06, 0.03)))
    for i, (a, b) in enumerate(zip(pts, pts[1:])):
        rod("antenna_wire_%d" % i, a, b, 0.009, M["steel"], vertices=8)
    rod("antenna_sleeve_low", pts[0], pts[1], 0.022, M["red"])
    rod("antenna_sleeve_top", pts[-4], pts[-2], 0.02, M["red"])


def build_plug_arm():
    a = Vector((1.0, -0.3, CZ - 0.1))
    b = Vector((1.78, -0.48, CZ - 0.16))
    rod("arm", a, b, 0.028, M["iron"])
    d = (b - a).normalized()
    primitive(bpy.ops.mesh.primitive_cone_add, "arm_bulb", M["iron"], vertices=24,
              radius1=0.08, radius2=0.03, depth=0.16, location=b + d * 0.05, rotation=facing(-d))
    box("plug", M["iron"], b + d * 0.15 + Vector((0, 0, -0.17)), (0.08, 0.06, 0.24))
    for dx in (-0.02, 0.02):
        rod("plug_prong", b + d * 0.15 + Vector((dx, 0, -0.3)),
            b + d * 0.15 + Vector((dx, 0, -0.42)), 0.007, M["steel"], vertices=8)


# ---------------------------------------------------------------- stand

def build_rod_and_base():
    cylinder("base_rim", M["black"], 1.08, 0.16, 0.08, bevel=0.02)
    cylinder("base_lip", M["black"], 1.03, 0.06, 0.19)
    cylinder("base", M["bronze"], 0.95, 0.2, 0.22, bevel=0.01)
    build_emblem(BASE_TOP)

    x, y = ROD_AT
    primitive(bpy.ops.mesh.primitive_cone_add, "rod_socket", M["black"], vertices=32,
              radius1=0.17, radius2=0.09, depth=0.22, location=(x, y, BASE_TOP + 0.11))
    rod("support_rod", (x, y, BASE_TOP + 0.2), (x, y, UNDERSIDE_Z + 0.02), 0.045, M["black"], vertices=24)


def ribbon(name, centre, radius, a0, a1, width, z, n=40, taper=True):
    """A raised white stroke along a circular arc, for the base emblem."""
    bm = bmesh.new()
    inner, outer = [], []
    for i in range(n + 1):
        a = a0 + (a1 - a0) * i / n
        w = width * (math.sin(math.pi * i / n) ** 0.6 if taper else 1.0)
        for ring, r in ((inner, radius - w / 2), (outer, radius + w / 2)):
            ring.append(bm.verts.new((centre[0] + r * math.cos(a), centre[1] + r * math.sin(a), z)))
    for i in range(n):
        bm.faces.new((inner[i], inner[i + 1], outer[i + 1], outer[i]))
    obj = new_object(name, bm)
    add_modifier(obj, "SOLIDIFY", thickness=0.02, offset=1.0)
    return finish(obj, M["white"], smooth=False)


def build_emblem(z):
    # Outer crescent around the back, opening toward the front.
    ribbon("emblem_crescent", (0, 0.1), 0.72, math.radians(-30), math.radians(210), 0.26, z)
    # Three swept strokes each side, curling out from the shield.
    for side in (-1, 1):
        for j in range(3):
            r = 0.36 + 0.13 * j
            c = (side * 0.02, 0.18 - 0.03 * j)
            span = (math.radians(-80), math.radians(-25)) if side > 0 else \
                (math.radians(205), math.radians(260))
            ribbon("emblem_stroke", c, r, span[0], span[1], 0.08 - 0.012 * j, z)
    # The bronze shield in the middle, outlined in white.
    outline = primitive(bpy.ops.mesh.primitive_uv_sphere_add, "emblem_shield_outline", M["white"],
                        segments=32, ring_count=16, radius=0.2, location=(0, -0.22, z))
    outline.scale = (0.85, 1.0, 0.08)
    shield = primitive(bpy.ops.mesh.primitive_uv_sphere_add, "emblem_shield", M["bronze"],
                       segments=32, ring_count=16, radius=0.17, location=(0, -0.22, z + 0.008))
    shield.scale = (0.85, 1.0, 0.1)


def build_legs():
    # Hip block under the head, two jointed legs dangling above the base.
    hip_z = UNDERSIDE_Z - 0.06
    box("hip", M["iron"], (0, -0.12, hip_z), (0.24, 0.13, 0.1), bevel=0.015)
    rod("hip_axle", (-0.36, -0.12, hip_z), (0.36, -0.12, hip_z), 0.03, M["iron"])
    for side in (-1, 1):
        x = side * 0.34
        joint = Vector((x, -0.12, hip_z))
        rod("hip_joint", joint - Vector((0.05, 0, 0)), joint + Vector((0.05, 0, 0)), 0.06, M["iron"], vertices=20)
        knee = Vector((x, -0.2, hip_z - 0.18))
        rod("thigh", joint, knee, 0.03, M["iron"])
        rod("knee_joint", knee - Vector((0.04, 0, 0)), knee + Vector((0.04, 0, 0)), 0.05, M["iron"], vertices=20)
        shin_top = knee + Vector((0, 0.0, -0.06))
        shin_bot = shin_top + Vector((0, 0.0, -0.36))
        rod("shin", shin_top, shin_bot, 0.085, M["iron"], vertices=24)
        for k in range(3):
            hz = shin_top.z - 0.07 - k * 0.08
            primitive(bpy.ops.mesh.primitive_cylinder_add, "shin_hole", M["dark"], vertices=10,
                      radius=0.018, depth=0.02, location=(x, shin_top.y - 0.083, hz),
                      rotation=(math.pi / 2, 0, 0))
        ankle = shin_bot + Vector((0, 0, -0.1))
        rod("ankle", shin_bot, ankle, 0.025, M["iron"])
        box("foot", M["iron"], ankle + Vector((0, -0.05, -0.02)), (0.15, 0.3, 0.045), bevel=0.01)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    make_materials()
    build_upper_shell()
    build_wings()
    build_eye("eye_main", MAIN_EYE, 0.34, 0.29)
    build_eye("eye_small", SMALL_EYE, 0.13, 0.1, collar=False)
    build_middle_and_lower()
    build_antenna()
    build_plug_arm()
    build_rod_and_base()
    build_legs()
    bpy.ops.export_scene.gltf(filepath=OUT, export_format="GLB", export_yup=True, export_apply=True)
    print("wrote", OUT)


main()
