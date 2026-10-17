"""Builds simulator/assets/droid.glb, the L0-LA59 model the simulator shows.

Run with Blender 5.x (no other setup needed):

    blender --background --factory-startup --python simulator/assets/src/build_lola.py -- simulator/assets/droid.glb

The simulator drives these nodes by name: wing_right, wing_left (the white top
panels, origin on their hinge along the spine), eye_main, eye_small and
led_white (glowing lenses). Blender is Z-up with the droid facing -Y; the glTF
exporter converts that to the simulator's Y-up, facing +Z.
"""
import math
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

OUT = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "droid.glb"

# Head: an ellipsoid dome over a flatter underside (a lens-shaped saucer).
R = 1.25          # radius
H_TOP = 0.42      # dome height
H_BOT = 0.16      # underside depth
CZ = 1.6          # rim height above the ground
SPINE = 0.16      # half-width of the centre spine; the panels hinge on its edges
EYE = (0.0, -0.74)        # main eye on the dome (x, y)
EYE_CLEAR = 0.47
SMALL_EYE = (-0.88, -0.56)


def top_z(x, y):
    return CZ + H_TOP * math.sqrt(max(0.0, 1.0 - (x / R) ** 2 - (y / R) ** 2))


def normal(x, y):
    return Vector((x / R ** 2, y / R ** 2, (top_z(x, y) - CZ) / H_TOP ** 2)).normalized()


def half_width(y, r2):
    return R * math.sqrt(max(0.0, r2 - (y / R) ** 2))


def eye_half_width(y):
    dy = y - EYE[1]
    return math.sqrt(max(0.0, EYE_CLEAR ** 2 - dy * dy))


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


RED = WHITE = BLACK = STEEL = WOOD = DARK = None


def make_materials():
    global RED, WHITE, BLACK, STEEL, WOOD, DARK
    RED = material("red", (0.42, 0.06, 0.045), rough=0.55)
    WHITE = material("white", (0.80, 0.79, 0.74), rough=0.6)
    BLACK = material("black", (0.025, 0.025, 0.028), rough=0.45)
    STEEL = material("steel", (0.62, 0.62, 0.64), metallic=0.9, rough=0.3)
    WOOD = material("wood", (0.20, 0.07, 0.04), rough=0.65)
    DARK = material("dark", (0.06, 0.06, 0.065), rough=0.5)


def lens_material(name):
    # Each lens has its own material; the simulator lights it from code.py.
    return material(name, (0.02, 0.02, 0.025), rough=0.06)


# ---------------------------------------------------------------- helpers

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


# Mesh data gets a "_mesh" suffix: the simulator binds behaviour by node name,
# and Bevy also names mesh entities after their mesh, which would bind twice.
def new_object(name, bm):
    mesh = bpy.data.meshes.new(name + "_mesh")
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
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


def surface_patch(name, y0, y1, span, lift, pivot, ny=180, nx=48):
    """A patch of the dome, built row by row so its outline stays smooth;
    vertices relative to `pivot` (the object's origin)."""
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


def rod(name, a, b, radius, mat, parent=None, vertices=16):
    a, b = Vector(a), Vector(b)
    d = b - a
    return primitive(bpy.ops.mesh.primitive_cylinder_add, name, mat, parent=parent,
                     vertices=vertices, radius=radius, depth=d.length,
                     location=(a + b) / 2, rotation=facing(d.normalized()))


# ---------------------------------------------------------------- parts

def build_head():
    bpy.ops.mesh.primitive_uv_sphere_add(segments=128, ring_count=64, radius=1.0, location=(0, 0, 0))
    saucer = bpy.context.active_object
    saucer.name = "head"
    saucer.data.name = "head_mesh"
    for v in saucer.data.vertices:
        v.co.x *= R
        v.co.y *= R
        v.co.z *= H_TOP if v.co.z > 0 else H_BOT
    saucer.location = (0, 0, CZ)
    finish(saucer, RED)

    # Centre spine, raised, from the antenna to the eye.
    w = SPINE - 0.025
    front = EYE[1] + math.sqrt(EYE_CLEAR ** 2 - w * w)
    spine = surface_patch(
        "spine", front, R * 0.92,
        lambda y: (-min(w, half_width(y, 0.86)), min(w, half_width(y, 0.86))),
        lift=0.05, pivot=Vector((0, 0, CZ)))
    add_modifier(spine, "SOLIDIFY", thickness=0.045, offset=-1.0)
    bevelled(spine, 0.01)
    finish(spine, WHITE)

    # Two slots on the spine, like the real one.
    for dx in (-0.045, 0.045):
        z = top_z(dx, -0.05) + 0.05
        primitive(bpy.ops.mesh.primitive_cube_add, "spine_slot", DARK, smooth=False,
                  size=1, location=(dx, -0.05, z), scale=(0.018, 0.22, 0.012))


def build_wings():
    reach = R * math.sqrt(0.74)
    for name, sign in (("wing_right", 1.0), ("wing_left", -1.0)):
        pivot = Vector((sign * SPINE, 0.0, top_z(SPINE, 0.0) + 0.02))

        def span(y, sign=sign):
            lo = max(SPINE + 0.008, eye_half_width(y))
            hi = half_width(y, 0.74)
            if hi <= lo:
                return None
            return (lo, hi) if sign > 0 else (-hi, -lo)

        panel = surface_patch(name, -reach, reach, span, lift=0.03, pivot=pivot)
        add_modifier(panel, "SOLIDIFY", thickness=0.035, offset=-1.0)
        bevelled(panel, 0.012, 3)
        finish(panel, WHITE)

        # Panel details that move with the wing: a long groove and a port.
        # Seated in the panel's top face (which sits 0.03 off the dome).
        gx = sign * 0.95
        for k in range(10):
            y = -0.15 + k * 0.06
            at = Vector((gx, y, top_z(gx, y))) + normal(gx, y) * 0.03
            primitive(bpy.ops.mesh.primitive_cube_add, name + "_groove", DARK, smooth=False,
                      parent=panel, size=1, location=at, scale=(0.025, 0.065, 0.012),
                      rotation=facing(normal(gx, y)))
        px, py = sign * 0.62, 0.25
        primitive(bpy.ops.mesh.primitive_cylinder_add, name + "_port", DARK, parent=panel,
                  vertices=24, radius=0.045, depth=0.02,
                  location=Vector((px, py, top_z(px, py))) + normal(px, py) * 0.03,
                  rotation=facing(normal(px, py)))


def build_eye(name, at, bezel_r, lens_r):
    n = normal(*at)
    centre = Vector((at[0], at[1], top_z(*at)))
    primitive(bpy.ops.mesh.primitive_torus_add, name + "_bezel", STEEL,
              major_radius=bezel_r, minor_radius=bezel_r * 0.14,
              major_segments=64, minor_segments=16,
              location=centre + n * 0.02, rotation=facing(n))
    primitive(bpy.ops.mesh.primitive_cylinder_add, name + "_socket", DARK,
              vertices=48, radius=bezel_r * 0.98, depth=0.06,
              location=centre + n * 0.0, rotation=facing(n))
    lens = primitive(bpy.ops.mesh.primitive_uv_sphere_add, name, lens_material(name + "_lens"),
                     segments=48, ring_count=24, radius=lens_r,
                     location=centre + n * 0.01, rotation=facing(n))
    lens.scale = (1.0, 1.0, 0.55)
    return lens


def build_underside():
    primitive(bpy.ops.mesh.primitive_cylinder_add, "lip", BLACK,
              vertices=96, radius=1.02, depth=0.08, location=(0, 0, CZ - 0.18))
    primitive(bpy.ops.mesh.primitive_torus_add, "band", STEEL,
              major_radius=1.0, minor_radius=0.03, major_segments=96, minor_segments=12,
              location=(0, 0, CZ - 0.24))
    primitive(bpy.ops.mesh.primitive_cylinder_add, "gear", BLACK,
              vertices=96, radius=0.86, depth=0.12, location=(0, 0, CZ - 0.31))
    bm = bmesh.new()
    for k in range(64):
        a = k / 64 * math.tau
        m = (Matrix.Translation((math.sin(a) * 0.885, math.cos(a) * 0.885, CZ - 0.31))
             @ Matrix.Rotation(-a, 4, "Z") @ Matrix.Diagonal((0.05, 0.06, 0.1, 1.0)))
        bmesh.ops.create_cube(bm, size=1.0, matrix=m)
    finish(new_object("gear_teeth", bm), BLACK, smooth=False)

    # White "data processing" LED on the front of the gear ring.
    n = Vector((0.32, -0.95, 0.0)).normalized()
    led = primitive(bpy.ops.mesh.primitive_uv_sphere_add, "led_white", lens_material("led_lens"),
                    segments=24, ring_count=12, radius=0.04,
                    location=Vector((0.32, -0.97, CZ - 0.18)), rotation=facing(n))
    led.scale = (1.0, 1.0, 0.55)


def build_antenna():
    y = 0.86
    z = top_z(0, y) + 0.06
    bevelled(primitive(bpy.ops.mesh.primitive_cube_add, "antenna_base", WHITE, smooth=False,
                       size=1, location=(0, y, z), scale=(0.14, 0.24, 0.1)), 0.015)
    root = Vector((0, y, z + 0.05))
    tip = root + Vector((0, 0.1, 0.55))
    rod("antenna", root, tip, 0.011, STEEL)
    rod("antenna_sleeve", root.lerp(tip, 0.7), tip, 0.026, RED)
    # The little wire hook at the top.
    pts = [tip + Vector((0, 0.06 * math.sin(t), 0.06 * (1 - math.cos(t)) + 0.01))
           for t in [i / 12 * math.pi * 1.4 for i in range(13)]]
    for i, (a, b) in enumerate(zip(pts, pts[1:])):
        rod("antenna_hook_%d" % i, a, b, 0.008, STEEL, vertices=8)


def build_stand():
    top = CZ - 0.37
    primitive(bpy.ops.mesh.primitive_cylinder_add, "stem", BLACK,
              vertices=48, radius=0.17, depth=top - 0.32, location=(0, 0, (top + 0.32) / 2))
    primitive(bpy.ops.mesh.primitive_cylinder_add, "stem_collar", BLACK,
              vertices=48, radius=0.24, depth=0.12, location=(0, 0, top - 0.04))
    for k in range(4):
        a = math.pi / 4 + k * math.pi / 2
        bevelled(primitive(bpy.ops.mesh.primitive_cube_add, "fin", BLACK, smooth=False,
                           size=1, location=(math.sin(a) * 0.27, math.cos(a) * 0.27, 0.62),
                           rotation=(0, 0, -a), scale=(0.07, 0.32, 0.46)), 0.01)


def ribbon(name, centre, radius, a0, a1, width, z, mat, n=32):
    """A flat white stroke along a circular arc, for the base emblem."""
    bm = bmesh.new()
    inner, outer = [], []
    for i in range(n + 1):
        a = a0 + (a1 - a0) * i / n
        taper = math.sin(math.pi * i / n) ** 0.5
        for ring, r in ((inner, radius - width * taper / 2), (outer, radius + width * taper / 2)):
            ring.append(bm.verts.new((centre[0] + r * math.cos(a), centre[1] + r * math.sin(a), z)))
    for i in range(n):
        bm.faces.new((inner[i], inner[i + 1], outer[i + 1], outer[i]))
    obj = new_object(name, bm)
    add_modifier(obj, "SOLIDIFY", thickness=0.015, offset=1.0)
    return finish(obj, mat, smooth=False)


def build_base():
    primitive(bpy.ops.mesh.primitive_cylinder_add, "base_rim", BLACK,
              vertices=96, radius=1.05, depth=0.12, location=(0, 0, 0.06))
    bevelled(primitive(bpy.ops.mesh.primitive_cylinder_add, "base", WOOD,
                       vertices=96, radius=0.97, depth=0.2, location=(0, 0, 0.22)), 0.02, 3)
    z = 0.322
    primitive(bpy.ops.mesh.primitive_torus_add, "emblem_ring", WHITE,
              major_radius=0.7, minor_radius=0.035, major_segments=96, minor_segments=8,
              location=(0, 0, z - 0.02))
    # Swept wings either side of a central shield, three strokes each.
    for side in (-1, 1):
        for j in range(3):
            r = 0.3 + 0.12 * j
            c = (side * 0.06, -0.1 - 0.05 * j)
            a0, a1 = (math.radians(200), math.radians(275)) if side < 0 else (math.radians(-95), math.radians(-20))
            ribbon("emblem_wing", c, r, a0, a1, 0.07 - 0.012 * j, z, WHITE)
    shield = primitive(bpy.ops.mesh.primitive_uv_sphere_add, "emblem_shield", WOOD,
                       segments=32, ring_count=16, radius=0.17, location=(0, -0.12, z))
    shield.scale = (0.85, 1.3, 0.15)


def build_plug_arm():
    top = Vector((0.8, -0.42, CZ - 0.2))
    bottom = Vector((1.0, -0.6, 0.95))
    rod("arm", top, bottom, 0.03, BLACK)
    primitive(bpy.ops.mesh.primitive_cone_add, "arm_cone", BLACK, vertices=24,
              radius1=0.09, radius2=0.035, depth=0.2, location=bottom + Vector((0, 0, -0.06)),
              rotation=(math.pi, 0, 0))
    bevelled(primitive(bpy.ops.mesh.primitive_cube_add, "plug", BLACK, smooth=False,
                       size=1, location=bottom + Vector((0, 0, -0.32)), scale=(0.1, 0.07, 0.3)), 0.012)
    for dx in (-0.025, 0.025):
        rod("plug_prong", bottom + Vector((dx, 0, -0.47)), bottom + Vector((dx, 0, -0.6)), 0.008, STEEL, vertices=8)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    make_materials()
    build_head()
    build_wings()
    build_eye("eye_main", EYE, 0.36, 0.3)
    build_eye("eye_small", SMALL_EYE, 0.14, 0.11)
    build_underside()
    build_antenna()
    build_stand()
    build_base()
    build_plug_arm()
    bpy.ops.export_scene.gltf(filepath=OUT, export_format="GLB", export_yup=True, export_apply=True)
    print("wrote", OUT)


main()
