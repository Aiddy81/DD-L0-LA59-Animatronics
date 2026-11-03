//! Placing and editing parts in the app (#65 stage 4). The Parts panel lists
//! the four kinds; one dragged onto the droid lands on the surface under the
//! pointer. With "Edit parts" on, clicking a part selects it (never a boop)
//! and dragging it slides it over the surface. Every change is one undo step
//! and is written back to `simulator/parts.toml`, keeping the file's comments,
//! order and any entries the app didn't load.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::SystemTime;

use bevy::ecs::system::SystemParam;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui::{self, Color32, Stroke};
use bevy_egui::EguiContexts;
use serde::Deserialize;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::config::{PartCfg, PartKind, PartShape, PixelLayout};
use crate::dock::ViewportRect;
use crate::hw::hw;
use crate::overlay::to_egui;
use crate::parts::{part_kinds, Part, Parts, PartsFileText, MAX_PARTS, PARTS_FILE};
use crate::prefs::Prefs;
use crate::scene::{DroidRoot, OrbitCam, Side, Wing};
use crate::theme::{ActiveTheme, Palette};

/// Changes kept for undo.
pub const MAX_UNDO: usize = 50;
/// How far (points) the pointer moves before a press on a part becomes a slide.
const SLIDE_START: f32 = 3.0;
const KINDS: [PartKind; 4] = [
    PartKind::Led,
    PartKind::Pixels,
    PartKind::ServoPart,
    PartKind::Button,
];
/// The top of a parts file the app creates.
const HEADER: &str = "# Extra hardware on the 3D droid (#65), placed with the simulator's Parts
# panel. The README's \"Parts file\" section lists every field. Hand edits are
# fine: when the app saves, it keeps comments and entries it doesn't use.
";

/// Something the Parts panel or a key asked for, run by `sync_parts`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartCmd {
    Undo,
    Redo,
    Delete,
    Save,
    ToggleEdit,
}

/// One part in the working list. `id` stays with the part through renames
/// and undo, so saving finds its entry in the file.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    id: u64,
    cfg: PartCfg,
}

/// A press on a part in edit mode: becomes a slide once the pointer moves.
#[derive(Clone, Copy, Debug)]
struct Slide {
    id: u64,
    from: Option<Vec2>,
    moving: bool,
}

/// Where on the droid the pointer is.
#[derive(Clone, Debug)]
struct Spot {
    anchor: String,
    /// In the anchor's space.
    pos: Vec3,
    normal: Vec3,
    world: Vec3,
    world_normal: Vec3,
}

/// The parts as the panel edits them. `Parts` follows `entries` once a frame.
#[derive(Resource, Default)]
pub struct PartEdit {
    /// Clicks on the droid select and slide parts instead of booping.
    pub editing: bool,
    pub autosave: bool,
    entries: Vec<Entry>,
    /// `entries` as last handed to `Parts`.
    applied: Vec<Entry>,
    selected: Option<u64>,
    undo: Vec<Vec<Entry>>,
    redo: Vec<Vec<Entry>>,
    /// A change is under way (a drag, typing in a field): it is one undo step.
    gesture: bool,
    /// The next change is an undo or redo, not a new step.
    skip_record: bool,
    /// Each part's name in the file, as last loaded or saved.
    in_file: HashMap<u64, String>,
    next_id: u64,
    dirty: bool,
    save_error: Option<String>,
    queue: Vec<PartCmd>,
    /// A kind being dragged from the panel.
    carrying: Option<PartKind>,
    slide: Option<Slide>,
    /// Where a carried part would land.
    preview: Option<Spot>,
    /// Text being typed into the name and pin fields: (part id, text).
    name_buf: Option<(u64, String)>,
    pin_buf: Option<(u64, String)>,
    /// Nudge step in model units.
    step: f32,
    /// The parts file's time stamp after the app last wrote it, so writing it
    /// doesn't count as an outside edit (no restart).
    pub written: Option<SystemTime>,
}

impl PartEdit {
    fn new(autosave: bool) -> Self {
        Self {
            autosave,
            step: 0.01,
            ..default()
        }
    }

    pub fn push(&mut self, cmd: PartCmd) {
        self.queue.push(cmd);
    }

    /// A part is being slid with the mouse: the camera must not orbit.
    pub fn sliding(&self) -> bool {
        self.slide.is_some()
    }

    fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn cfgs(&self) -> Vec<PartCfg> {
        self.entries.iter().map(|e| e.cfg.clone()).collect()
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    fn selected_index(&self) -> Option<usize> {
        self.selected.and_then(|id| self.index(id))
    }

    /// Takes the parts just loaded from the file as the new starting point.
    fn adopt(&mut self, parts: &[PartCfg]) {
        self.entries = parts
            .iter()
            .map(|cfg| Entry {
                id: {
                    self.next_id += 1;
                    self.next_id
                },
                cfg: cfg.clone(),
            })
            .collect();
        self.applied = self.entries.clone();
        self.in_file = self
            .entries
            .iter()
            .map(|e| (e.id, e.cfg.name.clone()))
            .collect();
        self.undo.clear();
        self.redo.clear();
        self.selected = None;
        self.slide = None;
        self.dirty = false;
        self.save_error = None;
    }

    /// Notes a change to `entries` as an undo step and returns whether there
    /// was one. All changes while `interacting` (a mouse button held, a text
    /// field focused) make one step.
    fn record(&mut self, interacting: bool) -> bool {
        let changed = self.entries != self.applied;
        if changed {
            if self.skip_record {
                self.skip_record = false;
            } else if !self.gesture {
                self.undo.push(self.applied.clone());
                if self.undo.len() > MAX_UNDO {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.gesture = true;
            }
            self.applied = self.entries.clone();
            self.dirty = true;
            self.save_error = None;
        }
        if !interacting {
            self.gesture = false;
        }
        changed
    }

    fn undo(&mut self) {
        if let Some(before) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.entries, before));
            self.skip_record = true;
            self.gesture = false;
        }
    }

    fn redo(&mut self) {
        if let Some(after) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.entries, after));
            self.skip_record = true;
            self.gesture = false;
        }
    }

    fn delete_selected(&mut self) {
        if let Some(i) = self.selected_index() {
            self.entries.remove(i);
            self.selected = None;
        }
    }

    /// Adds a part of `kind` at `spot`, named after its kind, and selects it.
    fn add(&mut self, kind: PartKind, spot: &Spot) -> Result<(), String> {
        if self.entries.len() >= MAX_PARTS {
            return Err(format!("at most {MAX_PARTS} parts"));
        }
        let cfgs = self.cfgs();
        let mut cfg = new_part(kind, unique_name(&cfgs, kind));
        cfg.anchor = spot.anchor.clone();
        cfg.pos = tidy3(spot.pos, 1e4);
        cfg.rot_deg = surface_rot(spot.normal, 0.0);
        let id = self.fresh_id();
        self.entries.push(Entry { id, cfg });
        self.selected = Some(id);
        self.editing = true;
        Ok(())
    }

    /// Moves part `id` to `spot`, keeping how far it is turned about its normal.
    fn slide_to(&mut self, id: u64, spot: &Spot) {
        if let Some(i) = self.index(id) {
            let cfg = &mut self.entries[i].cfg;
            let spin = spin_of(cfg.rot_deg);
            cfg.anchor = spot.anchor.clone();
            cfg.pos = tidy3(spot.pos, 1e4);
            cfg.rot_deg = surface_rot(spot.normal, spin);
        }
    }

    /// Writes the parts to `path` and remembers them as the file's contents.
    /// Returns the text written.
    fn save(&mut self, path: &Path) -> Result<String, String> {
        let old = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.to_string()),
        };
        let text = write_parts_text(&old, &self.in_file, &self.entries)?;
        write_atomic(path, &text).map_err(|e| e.to_string())?;
        self.in_file = self
            .entries
            .iter()
            .map(|e| (e.id, e.cfg.name.clone()))
            .collect();
        self.dirty = false;
        self.save_error = None;
        Ok(text)
    }
}

pub struct PartEditPlugin;

impl Plugin for PartEditPlugin {
    fn build(&self, app: &mut App) {
        let autosave = app
            .world()
            .get_resource::<Prefs>()
            .is_none_or(|p| p.parts_autosave);
        app.insert_resource(PartEdit::new(autosave)).add_systems(
            Update,
            (place_parts, sync_parts, draw_marks)
                .chain()
                .after(crate::ui::UiSet)
                .after(crate::scene::orbit_camera),
        );
    }
}

// ---------------------------------------------------------------- pure helpers

/// A new part's defaults: sized and coloured to be visible on the droid.
fn new_part(kind: PartKind, name: String) -> PartCfg {
    let mut cfg = PartCfg {
        name,
        kind,
        ..default()
    };
    match kind {
        PartKind::Led => cfg.colour = [1.0, 0.2, 0.1],
        PartKind::Pixels => {
            cfg.size = 0.025;
            cfg.spacing = 0.07;
            cfg.colour = [0.2, 0.6, 1.0];
        }
        PartKind::ServoPart => {
            cfg.size = 0.06;
            cfg.colour = [0.85, 0.85, 0.8];
        }
        PartKind::Button => {
            cfg.size = 0.06;
            cfg.colour = [0.9, 0.75, 0.1];
        }
    }
    cfg
}

/// `led_1`, `led_2`, ...: the first name of that form no part has.
pub fn unique_name(parts: &[PartCfg], kind: PartKind) -> String {
    let taken: HashSet<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    (1..=parts.len() + 1)
        .map(|n| format!("{}_{n}", kind.name()))
        .find(|name| !taken.contains(name.as_str()))
        .unwrap_or_else(|| format!("{}_{}", kind.name(), parts.len() + 1))
}

/// Why part `index` can't be called `name`, if it can't.
pub fn name_problem(parts: &[PartCfg], index: usize, name: &str) -> Option<String> {
    let probe = PartCfg {
        name: name.to_owned(),
        ..default()
    };
    if let Err(e) = probe.check() {
        return Some(e);
    }
    parts
        .iter()
        .enumerate()
        .any(|(i, p)| i != index && p.name == name)
        .then(|| "another part already has this name".to_owned())
}

fn round(x: f32, scale: f32) -> f32 {
    (x * scale).round() / scale
}

fn tidy3(v: Vec3, scale: f32) -> [f32; 3] {
    [round(v.x, scale), round(v.y, scale), round(v.z, scale)]
}

/// A part's rotation, as `parts.rs` builds it.
fn part_quat(rot_deg: [f32; 3]) -> Quat {
    let [x, y, z] = rot_deg.map(f32::to_radians);
    Quat::from_euler(EulerRot::XYZ, x, y, z)
}

/// `rot_deg` that points local +Y along `normal`, turned `spin` radians about it.
pub fn surface_rot(normal: Vec3, spin: f32) -> [f32; 3] {
    let n = normal.try_normalize().unwrap_or(Vec3::Y);
    let q = Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(spin);
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    [x, y, z].map(|r| round(r.to_degrees(), 100.0))
}

/// How far (radians) a part is turned about its own +Y, the surface normal.
fn spin_of(rot_deg: [f32; 3]) -> f32 {
    let q = part_quat(rot_deg);
    let twist = Quat::from_rotation_arc(Vec3::Y, q * Vec3::Y).inverse() * q;
    let x = twist * Vec3::X;
    (-x.z).atan2(x.x)
}

/// An f32 as the shortest decimal that reads back as the same f32, so the
/// file says 0.1, not 0.10000000149011612.
fn short(x: f32) -> f64 {
    x.to_string().parse().unwrap_or(f64::from(x))
}

fn num(x: f32) -> Value {
    Value::from(short(x))
}

fn vec3(v: [f32; 3]) -> Value {
    Value::Array(v.iter().map(|&x| short(x)).collect::<Array>())
}

fn layout_name(l: PixelLayout) -> &'static str {
    match l {
        PixelLayout::Row => "row",
        PixelLayout::Ring => "ring",
    }
}

fn shape_name(s: PartShape) -> &'static str {
    match s {
        PartShape::Box => "box",
        PartShape::Cylinder => "cylinder",
    }
}

/// Each key a part can have in the file and the value it should have; `None`
/// for a default value or a field its kind doesn't use, which is left out.
fn fields(c: &PartCfg) -> Vec<(&'static str, Option<Value>)> {
    let d = PartCfg::default();
    let pixels = c.kind == PartKind::Pixels;
    let servo = c.kind == PartKind::ServoPart;
    let lit = matches!(c.kind, PartKind::Led | PartKind::Pixels);
    let hinge = servo.then(|| {
        let (h, dh) = (&c.hinge, &d.hinge);
        let mut t = InlineTable::new();
        let mut put = |k: &str, v: f32, dv: f32| {
            if v != dv {
                t.insert(k, num(v));
            }
        };
        put("neutral_deg", h.neutral_deg, dh.neutral_deg);
        put("direction", h.direction, dh.direction);
        put("gain", h.gain, dh.gain);
        put("min_deg", h.min_deg, dh.min_deg);
        put("max_deg", h.max_deg, dh.max_deg);
        if h.hinge_axis != dh.hinge_axis {
            t.insert("hinge_axis", vec3(h.hinge_axis));
        }
        t
    });
    vec![
        ("name", Some(c.name.as_str().into())),
        ("kind", Some(c.kind.name().into())),
        (
            "pca",
            c.pca.map(|ch| Value::from(i64::try_from(ch).unwrap_or(0))),
        ),
        ("pin", c.pin.as_deref().map(Value::from)),
        (
            "anchor",
            (c.anchor != d.anchor).then(|| c.anchor.as_str().into()),
        ),
        (
            "full_duty",
            (lit && c.full_duty != d.full_duty).then(|| num(c.full_duty)),
        ),
        (
            "layout",
            (pixels && c.layout != d.layout).then(|| layout_name(c.layout).into()),
        ),
        (
            "count",
            (pixels && c.count != d.count)
                .then(|| Value::from(i64::try_from(c.count).unwrap_or(1))),
        ),
        (
            "spacing",
            (pixels && c.spacing != d.spacing).then(|| num(c.spacing)),
        ),
        (
            "shape",
            (servo && c.shape != d.shape).then(|| shape_name(c.shape).into()),
        ),
        ("pos", Some(vec3(c.pos))),
        ("rot_deg", (c.rot_deg != d.rot_deg).then(|| vec3(c.rot_deg))),
        ("size", (c.size != d.size).then(|| num(c.size))),
        ("colour", (c.colour != d.colour).then(|| vec3(c.colour))),
        (
            "hinge",
            hinge.filter(|t| !t.is_empty()).map(Value::InlineTable),
        ),
    ]
}

/// The part an existing `[[part]]` table describes, if it is a valid one.
fn table_cfg(t: &Table) -> Option<PartCfg> {
    let parsed: toml::Table = toml::from_str(&t.to_string()).ok()?;
    PartCfg::deserialize(toml::Value::Table(parsed)).ok()
}

/// Brings one table up to date with `cfg`, touching only the keys whose value
/// changed, so comments and the user's formatting elsewhere stay.
fn update_table(t: &mut Table, cfg: &PartCfg) {
    let had = table_cfg(t).map(|old| fields(&old));
    for (i, (key, want)) in fields(cfg).into_iter().enumerate() {
        let text = |v: &Option<Value>| v.as_ref().map(Value::to_string);
        let same = had
            .as_ref()
            .and_then(|h| h.get(i))
            .is_some_and(|(_, old)| text(old) == text(&want));
        if same {
            continue;
        }
        match want {
            Some(v) => match t.get_mut(key) {
                Some(Item::Value(old)) => {
                    let decor = old.decor().clone();
                    *old = v;
                    *old.decor_mut() = decor;
                }
                _ => {
                    t.insert(key, Item::Value(v));
                }
            },
            None => {
                t.remove(key);
            }
        }
    }
}

fn new_table(cfg: &PartCfg) -> Table {
    let mut t = Table::new();
    for (key, value) in fields(cfg) {
        if let Some(v) = value {
            t.insert(key, Item::Value(v));
        }
    }
    t.decor_mut().set_prefix("\n");
    t
}

/// `old` (a parts file's text) with its entries brought in line with
/// `entries`. `in_file` maps a part's id to its name in `old`. An entry whose
/// name the app knows but no part has any more was deleted and goes; entries
/// the app never loaded (invalid ones, duplicates) are kept as they are.
fn write_parts_text(
    old: &str,
    in_file: &HashMap<u64, String>,
    entries: &[Entry],
) -> Result<String, String> {
    let mut doc: DocumentMut = old.parse().map_err(|e: toml_edit::TomlError| {
        format!(
            "{PARTS_FILE} isn't valid TOML, so it was left alone: {}",
            e.message()
        )
    })?;
    let by_name: HashMap<&str, &Entry> = entries
        .iter()
        .filter_map(|e| in_file.get(&e.id).map(|n| (n.as_str(), e)))
        .collect();
    let known: HashSet<&str> = in_file.values().map(String::as_str).collect();
    if !doc.contains_key("part") {
        doc.insert("part", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let tables = doc["part"]
        .as_array_of_tables_mut()
        .ok_or("`part` in the parts file must be [[part]] entries")?;
    let mut placed: HashSet<u64> = HashSet::new();
    let mut gone = Vec::new();
    for (i, t) in tables.iter_mut().enumerate() {
        let name = t.get("name").and_then(Item::as_str).map(str::to_owned);
        match name.as_deref().and_then(|n| by_name.get(n)) {
            Some(e) => {
                if placed.insert(e.id) {
                    update_table(t, &e.cfg);
                }
            }
            None if name.as_deref().is_some_and(|n| known.contains(n)) => gone.push(i),
            None => {}
        }
    }
    for i in gone.into_iter().rev() {
        tables.remove(i);
    }
    for e in entries.iter().filter(|e| !placed.contains(&e.id)) {
        tables.push(new_table(&e.cfg));
    }
    let text = doc.to_string();
    Ok(if old.trim().is_empty() {
        format!("{HEADER}{text}")
    } else {
        text
    })
}

/// Writes `text` to a temporary file next to `path`, then renames it over
/// `path`, so a crash mid-write never leaves a half-written parts file.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

// --------------------------------------------------------------- 3D picking

/// What it takes to find the droid's surface under the pointer.
#[derive(SystemParam)]
struct Surface<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    cams: Query<'w, 's, (&'static Camera, &'static Transform), With<OrbitCam>>,
    ray: MeshRayCast<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
    parts: Query<'w, 's, (), With<Part>>,
    roots: Query<'w, 's, (), With<DroidRoot>>,
    wings: Query<'w, 's, &'static Wing>,
    globals: Query<'w, 's, &'static GlobalTransform>,
}

impl Surface<'_, '_> {
    fn cursor(&self) -> Option<Vec2> {
        self.windows.single().ok()?.cursor_position()
    }

    /// The droid's surface under the pointer, in the space of the node a part
    /// dropped there sits on: a wing if it's on one, else the droid itself.
    /// Parts are see-through to this, so a part can slide over another.
    fn under_cursor(&mut self) -> Option<Spot> {
        let cursor = self.cursor()?;
        let (camera, transform) = self.cams.single().ok()?;
        // Its Transform is this frame's; its GlobalTransform a frame behind.
        let ray = camera
            .viewport_to_world(&GlobalTransform::from(*transform), cursor)
            .ok()?;
        let Self {
            ray: caster,
            parents,
            parts,
            roots,
            wings,
            globals,
            ..
        } = self;
        let on_droid = |e: Entity| {
            for a in std::iter::once(e).chain(parents.iter_ancestors(e)) {
                if parts.contains(a) {
                    return false;
                }
                if roots.contains(a) {
                    return true;
                }
            }
            false
        };
        let settings = MeshRayCastSettings::default()
            .with_filter(&on_droid)
            .never_early_exit();
        let (entity, hit) = caster.cast_ray(ray, &settings).first()?.clone();
        let (anchor_e, anchor) = std::iter::once(entity)
            .chain(parents.iter_ancestors(entity))
            .find_map(|a| {
                if let Ok(w) = wings.get(a) {
                    let name = match w.side {
                        Side::Right => "wing_right",
                        Side::Left => "wing_left",
                    };
                    Some((a, name))
                } else {
                    roots.contains(a).then_some((a, PartCfg::BODY))
                }
            })?;
        let m = globals.get(anchor_e).ok()?.to_matrix();
        let normal = (Mat3::from_mat4(m).transpose() * hit.normal)
            .try_normalize()
            .unwrap_or(Vec3::Y);
        Some(Spot {
            anchor: anchor.to_owned(),
            pos: m.inverse().transform_point3(hit.point),
            normal,
            world: hit.point,
            world_normal: hit.normal.try_normalize().unwrap_or(Vec3::Y),
        })
    }
}

/// Drops a kind carried from the panel and slides the part being dragged.
fn place_parts(
    mut edit: ResMut<PartEdit>,
    mouse: Res<ButtonInput<MouseButton>>,
    viewport: Res<ViewportRect>,
    mut surface: Surface,
) {
    edit.preview = None;
    if let Some(kind) = edit.carrying {
        let spot = viewport.hovered.then(|| surface.under_cursor()).flatten();
        if mouse.just_released(MouseButton::Left) || !mouse.pressed(MouseButton::Left) {
            edit.carrying = None;
            if let Some(spot) = spot.filter(|_| mouse.just_released(MouseButton::Left)) {
                if let Err(e) = edit.add(kind, &spot) {
                    hw().push_line(format!("[sim] part not added: {e}"));
                }
            }
        } else {
            edit.preview = spot;
        }
    }
    let Some(mut slide) = edit.slide else {
        return;
    };
    if !mouse.pressed(MouseButton::Left) || !edit.editing {
        edit.slide = None;
        return;
    }
    let Some(cursor) = surface.cursor() else {
        return;
    };
    let from = *slide.from.get_or_insert(cursor);
    slide.moving |= cursor.distance(from) > SLIDE_START;
    edit.slide = Some(slide);
    if slide.moving {
        if let Some(spot) = surface.under_cursor() {
            edit.slide_to(slide.id, &spot);
        }
    }
}

/// A press on a part in edit mode (from its observer in `parts.rs`): selects
/// it and may start a slide.
pub fn press_part(edit: &mut PartEdit, name: &str) {
    if let Some(id) = edit
        .entries
        .iter()
        .find(|e| e.cfg.name == name)
        .map(|e| e.id)
    {
        edit.selected = Some(id);
        edit.slide = Some(Slide {
            id,
            from: None,
            moving: false,
        });
    }
}

/// Runs queued commands, hands changes to `Parts` (which redraws them) and
/// the simhw part calls, and saves.
fn sync_parts(
    mut edit: ResMut<PartEdit>,
    mut parts: ResMut<Parts>,
    mut file_text: ResMut<PartsFileText>,
    paths: Res<crate::Paths>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
) {
    // The file was (re)loaded: start again from what it says.
    if !parts.0.iter().eq(edit.applied.iter().map(|e| &e.cfg)) {
        if edit.dirty {
            hw().push_line(format!(
                "[sim] {PARTS_FILE} was reloaded; unsaved part changes were dropped"
            ));
        }
        edit.adopt(&parts.0);
    }
    let mut save_now = false;
    for cmd in std::mem::take(&mut edit.queue) {
        match cmd {
            PartCmd::Undo => edit.undo(),
            PartCmd::Redo => edit.redo(),
            PartCmd::Delete => edit.delete_selected(),
            PartCmd::Save => save_now = true,
            PartCmd::ToggleEdit => {
                edit.editing = !edit.editing;
                edit.slide = None;
            }
        }
    }
    let typing = contexts
        .ctx_mut()
        .is_ok_and(|ctx| ctx.wants_keyboard_input());
    let interacting = mouse.get_pressed().next().is_some() || typing;
    if edit.record(interacting) {
        let list = edit.cfgs();
        let mut s = hw();
        // None means a restart is under way; `reload_parts` publishes then.
        if s.part_kinds.is_some() {
            s.part_kinds = Some(part_kinds(&list));
        }
        drop(s);
        parts.0 = list;
    }
    let auto = edit.autosave && edit.dirty && !interacting && edit.save_error.is_none();
    if !(save_now || auto) {
        return;
    }
    let path = paths.simulator.join(PARTS_FILE);
    match edit.save(&path) {
        Ok(text) => {
            // So neither the loader nor the hot reload treats it as an outside edit.
            file_text.0 = Some(text);
            edit.written = crate::ui::file_mtime(&path);
        }
        Err(e) => {
            hw().push_line(format!("[sim] {PARTS_FILE} not saved: {e}"));
            edit.save_error = Some(e);
        }
    }
}

/// Rings the selected part and shows where a carried one would land.
fn draw_marks(
    mut contexts: EguiContexts,
    edit: Res<PartEdit>,
    viewport: Res<ViewportRect>,
    theme: Res<ActiveTheme>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cams: Query<(&Camera, &Transform), With<OrbitCam>>,
    parts: Query<(&Part, &GlobalTransform)>,
) {
    let Some(rect) = viewport.rect else {
        return;
    };
    let (Ok(window), Ok((camera, cam_transform))) = (windows.single(), cams.single()) else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let cam_world = GlobalTransform::from(*cam_transform);
    let (scale, ppp) = (window.scale_factor(), viewport.ppp);
    let project = |p: Vec3| {
        camera
            .world_to_viewport(&cam_world, p)
            .ok()
            .map(|logical| to_egui(logical, scale, ppp))
    };
    let accent = theme.palette.accent;
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("part_marks"),
        ))
        .with_clip_rect(rect);
    // A ring on the surface and a stalk along the normal.
    let mark = |at: Vec3, normal: Vec3, radius: f32, colour: Color32| {
        let (Some(c), Some(tip)) = (project(at), project(at + normal * radius * 3.0)) else {
            return;
        };
        let r = project(at + normal.any_orthonormal_vector() * radius)
            .map_or(8.0, |p| p.distance(c).max(6.0));
        painter.circle_stroke(c, r, Stroke::new(2.0, colour));
        painter.line_segment([c, tip], Stroke::new(2.0, colour));
    };
    if let Some(cfg) = edit
        .selected_index()
        .and_then(|i| edit.entries.get(i))
        .map(|e| &e.cfg)
    {
        for (part, at) in &parts {
            if part.0 == cfg.name {
                let size = cfg.size.max(0.02) * at.scale().max_element();
                mark(at.translation(), at.up().as_vec3(), size * 1.6, accent);
            }
        }
    }
    if let Some(kind) = edit.carrying {
        if let Some(spot) = &edit.preview {
            mark(spot.world, spot.world_normal, 0.05, accent);
        }
        if let Some(pos) = ctx.pointer_latest_pos() {
            let text = match edit.preview {
                Some(_) => format!("+ {}", kind.name()),
                None => format!("{} - drop it on the droid", kind.name()),
            };
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("part_carry"),
            ))
            .text(
                pos + egui::vec2(14.0, 10.0),
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::proportional(14.0),
                theme.palette.text,
            );
        }
    }
}

// --------------------------------------------------------------- the panel

fn source_text(c: &PartCfg) -> String {
    match (c.pca, &c.pin) {
        (Some(ch), _) => format!("PCA {ch}"),
        (None, Some(pin)) => format!("pin {pin}"),
        (None, None) => "shim".to_owned(),
    }
}

fn kind_help(kind: PartKind) -> &'static str {
    match kind {
        PartKind::Led => "A glowing sphere: brightness or RGB",
        PartKind::Pixels => "A row or ring of glowing dots (a NeoPixel strip or ring)",
        PartKind::ServoPart => "A shape a servo turns about a hinge, like the wings",
        PartKind::Button => "A cap you press with the mouse; the firmware reads it as a pin",
    }
}

/// The Parts dock pane: the kinds to drag, the list, and the selected part's settings.
pub fn panel(ui: &mut egui::Ui, edit: &mut PartEdit, palette: &Palette) {
    ui.horizontal_wrapped(|row| {
        let mut editing = edit.editing;
        if row
            .toggle_value(&mut editing, "Edit parts")
            .on_hover_text(
                "On: click a part to select it and drag it to slide it over the droid; clicks never boop and button parts don't press",
            )
            .changed()
        {
            edit.push(PartCmd::ToggleEdit);
        }
        if row
            .add_enabled(!edit.undo.is_empty(), egui::Button::new("Undo"))
            .on_hover_text("Undo the last part change (Ctrl+Z)")
            .clicked()
        {
            edit.push(PartCmd::Undo);
        }
        if row
            .add_enabled(!edit.redo.is_empty(), egui::Button::new("Redo"))
            .on_hover_text("Redo (Ctrl+Y)")
            .clicked()
        {
            edit.push(PartCmd::Redo);
        }
        if row
            .add_enabled(edit.dirty, egui::Button::new("Save"))
            .on_hover_text(format!("Write the parts to simulator/{PARTS_FILE} (Ctrl+S)"))
            .clicked()
        {
            edit.push(PartCmd::Save);
        }
        row.checkbox(&mut edit.autosave, "Auto-save")
            .on_hover_text(format!(
                "Write simulator/{PARTS_FILE} after each change"
            ));
        if let Some(e) = &edit.save_error {
            row.colored_label(palette.error, "not saved").on_hover_text(e);
        } else if edit.dirty {
            row.colored_label(palette.warn, "unsaved changes");
        }
    });
    ui.separator();
    ui.label("Drag onto the droid:");
    ui.horizontal_wrapped(|row| {
        for kind in KINDS {
            let button = egui::Button::new(kind.name()).sense(egui::Sense::click_and_drag());
            let r = row.add(button).on_hover_text(kind_help(kind));
            if r.drag_started() {
                edit.carrying = Some(kind);
            }
        }
    });
    ui.separator();
    if edit.entries.is_empty() {
        ui.weak("No parts yet.");
    }
    let mut pick = None;
    for e in &edit.entries {
        let text = format!(
            "{}   {} · {}",
            e.cfg.name,
            e.cfg.kind.name(),
            source_text(&e.cfg)
        );
        if ui
            .selectable_label(edit.selected == Some(e.id), text)
            .clicked()
        {
            pick = Some(e.id);
        }
    }
    if let Some(id) = pick {
        edit.selected = (edit.selected != Some(id)).then_some(id);
    }
    if let Some(i) = edit.selected_index() {
        ui.separator();
        settings(ui, edit, i, palette);
    }
}

/// One labelled row of the settings grid.
fn row(grid: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    grid.label(label);
    grid.horizontal_wrapped(add);
    grid.end_row();
}

fn drag(v: &mut f32, speed: f64, range: std::ops::RangeInclusive<f32>) -> egui::DragValue<'_> {
    egui::DragValue::new(v)
        .speed(speed)
        .range(range)
        .max_decimals(4)
}

/// The selected part's settings. Edits a copy, applied when it is valid.
fn settings(ui: &mut egui::Ui, edit: &mut PartEdit, i: usize, palette: &Palette) {
    let id = edit.entries[i].id;
    let mut c = edit.entries[i].cfg.clone();
    let others = edit.cfgs();
    let mut problem: Option<String> = None;
    let step = &mut edit.step;
    let name_buf = edit.name_buf.get_or_insert_with(|| (id, c.name.clone()));
    if name_buf.0 != id {
        *name_buf = (id, c.name.clone());
    }
    let pin_buf = edit
        .pin_buf
        .get_or_insert_with(|| (id, c.pin.clone().unwrap_or_default()));
    if pin_buf.0 != id {
        *pin_buf = (id, c.pin.clone().unwrap_or_default());
    }
    egui::Grid::new("part_settings")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |grid| {
            row(grid, "Name", |r| {
                let field =
                    r.add(egui::TextEdit::singleline(&mut name_buf.1).desired_width(140.0));
                let bad = name_problem(&others, i, &name_buf.1);
                if field.changed() && bad.is_none() {
                    c.name = name_buf.1.clone();
                } else if !field.has_focus() {
                    // Undo or a rename elsewhere wins over a stale or invalid edit.
                    name_buf.1 = c.name.clone();
                }
                if let Some(e) = bad.filter(|_| name_buf.1 != c.name) {
                    r.colored_label(palette.error, e);
                }
            });
            row(grid, "Kind", |r| {
                r.label(c.kind.name());
                r.weak(format!("on {}", c.anchor));
            });
            row(grid, "Source", |r| {
                let mut src = match (c.pca, &c.pin) {
                    (Some(_), _) => 0,
                    (None, Some(_)) => 1,
                    (None, None) => 2,
                };
                let names = ["PCA channel", "Pin", "Shim only"];
                egui::ComboBox::from_id_salt("part_source")
                    .selected_text(names[src])
                    .show_ui(r, |m| {
                        for (n, label) in names.iter().enumerate() {
                            // A button is an input: it can't follow a PCA output.
                            if n == 0 && c.kind == PartKind::Button {
                                continue;
                            }
                            m.selectable_value(&mut src, n, *label);
                        }
                    });
                match src {
                    0 => {
                        let mut ch = c.pca.unwrap_or(8);
                        r.add(egui::DragValue::new(&mut ch).range(0..=15));
                        c.pca = Some(ch);
                        c.pin = None;
                    }
                    1 => {
                        c.pca = None;
                        if pin_buf.1.is_empty() {
                            pin_buf.1 = "D5".into();
                        }
                        let field = r
                            .add(egui::TextEdit::singleline(&mut pin_buf.1).desired_width(90.0))
                            .on_hover_text(
                                "A board pin name, as code.py uses it: D5, A0, EXTERNAL_NEOPIXELS",
                            );
                        pin_buf.1 = pin_buf.1.to_ascii_uppercase();
                        match &c.pin {
                            Some(pin) if !field.changed() && !field.has_focus() => {
                                pin_buf.1 = pin.clone();
                            }
                            _ => c.pin = Some(pin_buf.1.clone()),
                        }
                    }
                    _ => {
                        c.pca = None;
                        c.pin = None;
                    }
                }
            });
            row(grid, "Size", |r| {
                r.add(drag(&mut c.size, 0.001, 0.001..=2.0));
                r.label("Colour");
                // The button converts through HSV every frame, which drifts the
                // value; only a real pick counts.
                let mut rgb = c.colour;
                if egui::color_picker::color_edit_button_rgb(r, &mut rgb).changed() {
                    c.colour = rgb.map(|v| round(v, 1000.0));
                }
            });
            let lit = matches!(c.kind, PartKind::Led | PartKind::Pixels);
            if lit && (c.pca.is_some() || c.pin.is_some()) {
                row(grid, "Full at duty", |r| {
                    r.add(drag(&mut c.full_duty, 0.005, 0.001..=1.0))
                        .on_hover_text("Duty that counts as full brightness; 0.3275 for an LED driven like the eyes (0-6550 us)");
                });
            }
            if c.kind == PartKind::Pixels {
                row(grid, "Pixels", |r| {
                    r.add(egui::DragValue::new(&mut c.count).range(1..=PartCfg::MAX_PIXELS));
                    egui::ComboBox::from_id_salt("part_layout")
                        .selected_text(layout_name(c.layout))
                        .show_ui(r, |m| {
                            m.selectable_value(&mut c.layout, PixelLayout::Row, "row");
                            m.selectable_value(&mut c.layout, PixelLayout::Ring, "ring");
                        });
                    r.label("spacing");
                    r.add(drag(&mut c.spacing, 0.001, 0.001..=2.0));
                });
            }
            if c.kind == PartKind::ServoPart {
                servo_rows(grid, &mut c);
            }
            row(grid, "Position", |r| {
                for v in &mut c.pos {
                    r.add(drag(v, 0.002, -10.0..=10.0));
                }
            });
            row(grid, "Rotation°", |r| {
                for v in &mut c.rot_deg {
                    r.add(
                        egui::DragValue::new(v)
                            .speed(0.5)
                            .range(-360.0..=360.0)
                            .max_decimals(2),
                    );
                }
            });
            row(grid, "Nudge", |r| nudges(r, &mut c, step));
        });
    if c != edit.entries[i].cfg {
        match c.check() {
            Ok(()) => edit.entries[i].cfg = c,
            Err(e) => problem = Some(e),
        }
    }
    if let Some(e) = problem {
        ui.colored_label(palette.error, e);
    }
    if ui
        .button("Delete part")
        .on_hover_text("Remove it from the droid (Del); Undo brings it back")
        .clicked()
    {
        edit.push(PartCmd::Delete);
    }
}

fn servo_rows(grid: &mut egui::Ui, c: &mut PartCfg) {
    row(grid, "Shape", |r| {
        egui::ComboBox::from_id_salt("part_shape")
            .selected_text(shape_name(c.shape))
            .show_ui(r, |m| {
                m.selectable_value(&mut c.shape, PartShape::Box, "box");
                m.selectable_value(&mut c.shape, PartShape::Cylinder, "cylinder");
            });
    });
    let h = &mut c.hinge;
    row(grid, "Hinge axis", |r| {
        for (label, axis) in [
            ("X", [1.0, 0.0, 0.0]),
            ("Y", [0.0, 1.0, 0.0]),
            ("Z", [0.0, 0.0, 1.0]),
        ] {
            if r.selectable_label(h.hinge_axis == axis, label).clicked() {
                h.hinge_axis = axis;
            }
        }
        for v in &mut h.hinge_axis {
            r.add(drag(v, 0.01, -1.0..=1.0));
        }
    });
    row(grid, "Neutral°", |r| {
        r.add(drag(&mut h.neutral_deg, 0.5, 0.0..=360.0))
            .on_hover_text("Servo angle at which the part sits as placed");
        r.label("direction");
        egui::ComboBox::from_id_salt("part_direction")
            .selected_text(if h.direction < 0.0 { "-1" } else { "+1" })
            .show_ui(r, |m| {
                m.selectable_value(&mut h.direction, 1.0, "+1");
                m.selectable_value(&mut h.direction, -1.0, "-1");
            });
        r.label("gain");
        r.add(drag(&mut h.gain, 0.01, -10.0..=10.0));
    });
    row(grid, "Limits°", |r| {
        let max = h.max_deg;
        r.add(drag(&mut h.min_deg, 0.5, -360.0..=max));
        let min = h.min_deg;
        r.add(drag(&mut h.max_deg, 0.5, min..=360.0));
    });
}

/// Small moves in the part's own frame: along the surface (X, Z), out of it
/// (Y), and turns about the normal.
fn nudges(r: &mut egui::Ui, c: &mut PartCfg, step: &mut f32) {
    let q = part_quat(c.rot_deg);
    let mut shift = |r: &mut egui::Ui, label: &str, dir: Vec3, tip: &str| {
        if r.small_button(label).on_hover_text(tip).clicked() {
            let p = Vec3::from(c.pos) + q * dir * *step;
            c.pos = tidy3(p, 1e4);
        }
    };
    shift(r, "X-", Vec3::NEG_X, "Along the surface, the part's -X");
    shift(r, "X+", Vec3::X, "Along the surface, the part's +X");
    shift(r, "Z-", Vec3::NEG_Z, "Along the surface, the part's -Z");
    shift(r, "Z+", Vec3::Z, "Along the surface, the part's +Z");
    shift(r, "In", Vec3::NEG_Y, "Into the surface");
    shift(r, "Out", Vec3::Y, "Out of the surface");
    for (label, deg) in [("⟲15°", 15.0f32), ("⟳15°", -15.0)] {
        if r.small_button(label)
            .on_hover_text("Turn about the surface normal")
            .clicked()
        {
            let turned = q * Quat::from_rotation_y(deg.to_radians());
            let (x, y, z) = turned.to_euler(EulerRot::XYZ);
            c.rot_deg = [x, y, z].map(|v| round(v.to_degrees(), 100.0));
        }
    }
    egui::ComboBox::from_id_salt("part_step")
        .selected_text(format!("step {}", short(*step)))
        .show_ui(r, |m| {
            for s in [0.002, 0.01, 0.05] {
                m.selectable_value(step, s, format!("{s}"));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parts::parse_parts;

    fn led(name: &str, pos: [f32; 3]) -> PartCfg {
        PartCfg {
            name: name.into(),
            kind: PartKind::Led,
            pos,
            ..default()
        }
    }

    fn entries(parts: &[PartCfg]) -> Vec<Entry> {
        parts
            .iter()
            .enumerate()
            .map(|(i, cfg)| Entry {
                id: i as u64 + 1,
                cfg: cfg.clone(),
            })
            .collect()
    }

    fn in_file(list: &[Entry]) -> HashMap<u64, String> {
        list.iter().map(|e| (e.id, e.cfg.name.clone())).collect()
    }

    fn reparse(text: &str) -> Vec<PartCfg> {
        let (parts, warnings) = parse_parts(text).expect("valid toml");
        assert!(warnings.is_empty(), "{warnings:?}");
        parts
    }

    #[test]
    fn names_are_unique() {
        let parts = vec![led("led_1", [0.0; 3]), led("led_3", [0.0; 3])];
        assert_eq!(unique_name(&parts, PartKind::Led), "led_2");
        assert_eq!(unique_name(&parts, PartKind::Button), "button_1");
        assert_eq!(unique_name(&[], PartKind::Pixels), "pixels_1");
        assert!(name_problem(&parts, 0, "led_3").is_some());
        assert!(name_problem(&parts, 1, "led_3").is_none());
        assert!(name_problem(&parts, 0, "lamp").is_none());
        assert!(name_problem(&parts, 0, "").is_some());
        assert!(name_problem(&parts, 0, "has space").is_some());
    }

    #[test]
    fn a_new_file_round_trips() {
        let mut servo = new_part(PartKind::ServoPart, "flap".into());
        servo.pca = Some(0);
        servo.hinge.gain = 0.45;
        servo.hinge.hinge_axis = [0.0, 0.0, 1.0];
        let mut ring = new_part(PartKind::Pixels, "ring".into());
        ring.layout = PixelLayout::Ring;
        ring.count = 12;
        ring.pin = Some("D6".into());
        ring.full_duty = 0.3275;
        let mut button = new_part(PartKind::Button, "go".into());
        button.anchor = "wing_left".into();
        button.rot_deg = [90.0, 0.0, -12.5];
        let list = entries(&[led("lamp", [0.1, 1.45, -0.35]), servo, ring, button]);
        let text = write_parts_text("", &HashMap::new(), &list).expect("saves");
        assert!(text.starts_with("# Extra hardware"));
        assert!(text.contains("pos = [0.1, 1.45, -0.35]"), "{text}");
        let cfgs: Vec<PartCfg> = list.iter().map(|e| e.cfg.clone()).collect();
        assert_eq!(reparse(&text), cfgs);
    }

    #[test]
    fn saving_keeps_comments_order_and_unknown_entries() {
        let old = "# My parts\n\n# The lamp on the back.\n[[part]]\nname = \"lamp\"\nkind = \"led\" # glows\npos = [0.0, 1.0, 0.0]\n\n# Not loaded: bad kind.\n[[part]]\nname = \"odd\"\nkind = \"laser\"\n\n[[part]]\nname = \"gone\"\nkind = \"led\"\n";
        let (loaded, warnings) = parse_parts(old).expect("toml");
        assert_eq!(warnings.len(), 1);
        let mut list = entries(&loaded);
        let names = in_file(&list);
        // Edit lamp, delete gone, add a new one.
        list[0].cfg.size = 0.2;
        list.remove(1);
        list.push(Entry {
            id: 99,
            cfg: led("new", [1.0, 2.0, 3.0]),
        });
        let text = write_parts_text(old, &names, &list).expect("saves");
        assert!(text.starts_with("# My parts\n\n# The lamp on the back.\n[[part]]"));
        assert!(text.contains("kind = \"led\" # glows"), "{text}");
        assert!(text.contains("size = 0.2"));
        assert!(text.contains("# Not loaded: bad kind.\n[[part]]\nname = \"odd\""));
        assert!(!text.contains("gone"));
        assert!(text.find("odd") < text.find("\"new\""));
        let (parts, warnings) = parse_parts(&text).expect("toml");
        assert_eq!(warnings.len(), 1, "the bad entry is still there");
        let want: Vec<PartCfg> = list.iter().map(|e| e.cfg.clone()).collect();
        assert_eq!(parts, want);
    }

    #[test]
    fn renames_and_source_changes_update_the_same_entry() {
        let old = "[[part]]\nname = \"a\"\nkind = \"led\"\npca = 3\npos = [0.0, 0.0, 0.0]\n";
        let mut list = entries(&reparse(old));
        let names = in_file(&list);
        list[0].cfg.name = "b".into();
        list[0].cfg.pca = None;
        list[0].cfg.pin = Some("D5".into());
        let text = write_parts_text(old, &names, &list).expect("saves");
        assert_eq!(text.matches("[[part]]").count(), 1, "{text}");
        assert!(!text.contains("pca"));
        assert_eq!(reparse(&text)[0], list[0].cfg);
    }

    #[test]
    fn invalid_toml_is_left_alone() {
        assert!(write_parts_text("[[part]\nname=", &HashMap::new(), &[]).is_err());
        assert!(write_parts_text("part = 3\n", &HashMap::new(), &[]).is_err());
    }

    #[test]
    fn save_writes_through_a_temp_file() {
        let dir = std::env::temp_dir().join(format!("ddl0-part-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join(PARTS_FILE);
        let _ = std::fs::remove_file(&path);
        let mut edit = PartEdit::new(true);
        edit.adopt(&[]);
        edit.entries.push(Entry {
            id: 7,
            cfg: led("lamp", [0.0, 1.0, 0.0]),
        });
        edit.record(false);
        assert!(edit.dirty);
        let text = edit.save(&path).expect("saves");
        assert!(!edit.dirty);
        assert_eq!(std::fs::read_to_string(&path).expect("read"), text);
        assert!(!dir.join("parts.toml.tmp").exists());
        // Saved again after a rename: still one entry.
        edit.entries[0].cfg.name = "light".into();
        edit.record(false);
        let text = edit.save(&path).expect("saves");
        assert_eq!(reparse(&text), vec![led("light", [0.0, 1.0, 0.0])]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_and_redo_step_through_changes() {
        let mut edit = PartEdit::new(true);
        edit.adopt(&[led("a", [0.0; 3])]);
        // A drag: many frames, one step.
        for x in 1..5 {
            edit.entries[0].cfg.pos[0] = x as f32;
            edit.record(true);
        }
        edit.record(false);
        edit.entries[0].cfg.size = 0.3;
        edit.record(false);
        assert_eq!(edit.undo.len(), 2);
        edit.undo();
        edit.record(false);
        assert_eq!(edit.entries[0].cfg.size, PartCfg::default().size);
        assert_eq!(edit.entries[0].cfg.pos[0], 4.0);
        edit.undo();
        edit.record(false);
        assert_eq!(edit.entries[0].cfg.pos[0], 0.0);
        edit.redo();
        edit.record(false);
        assert_eq!(edit.entries[0].cfg.pos[0], 4.0);
        assert_eq!(edit.redo.len(), 1);
        // A new change drops what could be redone.
        edit.entries[0].cfg.name = "b".into();
        edit.record(false);
        assert!(edit.redo.is_empty());
    }

    #[test]
    fn delete_is_undone() {
        let mut edit = PartEdit::new(true);
        edit.adopt(&[led("a", [0.0; 3]), led("b", [0.0; 3])]);
        edit.selected = Some(edit.entries[0].id);
        edit.delete_selected();
        edit.record(false);
        assert_eq!(edit.cfgs(), vec![led("b", [0.0; 3])]);
        edit.undo();
        edit.record(false);
        assert_eq!(edit.cfgs().len(), 2);
        assert_eq!(edit.entries[0].cfg.name, "a");
    }

    #[test]
    fn undo_is_capped() {
        let mut edit = PartEdit::new(true);
        edit.adopt(&[led("a", [0.0; 3])]);
        for x in 0..MAX_UNDO + 10 {
            edit.entries[0].cfg.pos[1] = x as f32 + 1.0;
            edit.record(false);
        }
        assert_eq!(edit.undo.len(), MAX_UNDO);
    }

    #[test]
    fn surface_rotation_points_up_out_of_the_surface() {
        for n in [Vec3::Y, Vec3::NEG_Y, Vec3::X, Vec3::new(0.3, -0.5, 0.8)] {
            let n = n.normalize();
            for spin in [0.0f32, 0.7, -2.0] {
                let rot = surface_rot(n, spin);
                let q = part_quat(rot);
                assert!((q * Vec3::Y).distance(n) < 1e-3, "{n} {rot:?}");
                let back = spin_of(rot);
                let d = (back - spin).rem_euclid(std::f32::consts::TAU);
                assert!(
                    d < 1e-3 || d > std::f32::consts::TAU - 1e-3,
                    "{spin} {back}"
                );
            }
        }
    }

    #[test]
    fn short_numbers() {
        assert_eq!(num(0.1).to_string(), "0.1");
        assert_eq!(num(1.0).to_string(), "1.0");
        assert_eq!(vec3([0.0, -0.35, 2.5]).to_string(), "[0.0, -0.35, 2.5]");
    }
}
