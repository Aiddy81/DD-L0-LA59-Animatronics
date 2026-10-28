//! Saved dock layouts: the built-in ones, the user's named ones, and files to
//! share them. A layout file can be hand-edited or come from someone else, so
//! everything read is checked by [`validate`] before the dock sees it:
//! egui_dock indexes its trees without bounds checks and would panic on a
//! malformed one.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use egui_dock::{DockState, Node, NodeIndex, Surface, SurfaceIndex, Tree};
use serde::{Deserialize, Serialize};

use crate::dock::{self, Pane};
use crate::theme;

/// A layout file bigger than this is not a layout.
const MAX_FILE_BYTES: u64 = 256 * 1024;
/// Nodes per tree: depth 10 of the binary tree, far more than any real layout.
const MAX_NODES: usize = 1024;
/// Dock surfaces: the main one plus floating windows.
const MAX_SURFACES: usize = 32;
/// Tabs in a whole layout; each pane may appear once, so this is generous.
const MAX_TABS: usize = 256;
const MIN_FRACTION: f32 = 0.05;
const MAX_FRACTION: f32 = 0.95;
/// Detached window size limits, in logical pixels.
const MIN_DETACHED: f32 = 200.0;
const MAX_DETACHED: f32 = 8192.0;
/// Floating dock window limits, in points.
const MIN_FLOAT: f32 = 80.0;
const MAX_FLOAT_POS: f32 = 100_000.0;
const DEFAULT_DETACHED: (f32, f32) = (480.0, 600.0);
const MAX_NAME_CHARS: usize = 40;
const MAX_LISTED_FILES: usize = 100;
const FILE_SUFFIX: &str = ".layout.toml";

pub const BUILTINS: [&str; 3] = ["Default", "Debug", "Demo"];

/// Where one detached window opens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetachedSave {
    pub pane: Pane,
    /// Physical position; `None` lets the OS place the window.
    pub x: Option<i32>,
    pub y: Option<i32>,
    /// Logical size.
    pub w: f32,
    pub h: f32,
    /// That window's own UI scale.
    pub scale: f32,
}

impl DetachedSave {
    /// A fresh detach: the OS places it.
    pub fn new(pane: Pane, scale: f32) -> Self {
        Self {
            pane,
            x: None,
            y: None,
            w: DEFAULT_DETACHED.0,
            h: DEFAULT_DETACHED.1,
            scale,
        }
    }
}

/// One floating dock window: its surface index and egui rectangle (points).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatSave {
    pub surface: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The dock plus the panes torn out into their own OS windows.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layout {
    pub dock: DockState<Pane>,
    #[serde(default)]
    pub detached: Vec<DetachedSave>,
    /// Where the dock's floating windows were, by surface index. egui_dock does
    /// not keep this in its own state.
    #[serde(default)]
    pub floats: Vec<FloatSave>,
}

/// `DockState` has no `PartialEq`, so layouts compare by their TOML text: the
/// same form they are saved in.
impl PartialEq for Layout {
    fn eq(&self, other: &Self) -> bool {
        match (toml::to_string(self), toml::to_string(other)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}

/// One of [`BUILTINS`] by name.
pub fn builtin(name: &str) -> Option<Layout> {
    let dock = match name {
        "Default" => dock::default_layout(),
        "Debug" => debug_dock(),
        "Demo" => DockState::new(vec![Pane::Viewport]),
        _ => return None,
    };
    let mut layout = Layout {
        dock,
        detached: Vec::new(),
        floats: Vec::new(),
    };
    validate(&mut layout).ok()?;
    Some(layout)
}

/// Log and plot large: the 3D view over the log on the left, the plot over
/// the rest of the panels on the right.
fn debug_dock() -> DockState<Pane> {
    let mut state = DockState::new(vec![Pane::Viewport]);
    let tree = state.main_surface_mut();
    let [left, right] = tree.split_right(NodeIndex::root(), 0.55, vec![Pane::Plot]);
    tree.split_below(left, 0.4, vec![Pane::Log]);
    tree.split_below(
        right,
        0.55,
        vec![
            Pane::State,
            Pane::Pca,
            Pane::Board,
            Pane::Motion,
            Pane::Behaviours,
            Pane::Scenarios,
            Pane::Audio,
            Pane::Settings,
        ],
    );
    state
}

/// Makes `layout` safe to hand to the dock, or says why it can't be. On
/// error the layout is left half-checked and must be thrown away.
pub fn validate(layout: &mut Layout) -> Result<(), String> {
    check_structure(&mut layout.dock)?;

    // Dock first, then the detached windows; the first occurrence of a pane wins.
    while let Some(at) = find_unwanted(&layout.dock) {
        layout.dock.remove_tab(at);
    }
    if layout.dock.find_main_surface_tab(&Pane::Viewport).is_none() {
        layout.dock.push_to_first_leaf(Pane::Viewport);
    }
    reset_focus(&mut layout.dock);

    let mut seen: HashSet<Pane> = layout.dock.iter_all_tabs().map(|(_, pane)| *pane).collect();
    layout
        .detached
        .retain_mut(|d| d.pane != Pane::Viewport && seen.insert(d.pane) && sanitize_detached(d));

    restore_floats(layout);
    dock::prepare(&mut layout.dock);
    Ok(())
}

/// Keeps the floats that match a floating window, in range, and moves those
/// windows to them. Surface indices don't shift while validating: surfaces are
/// only emptied or removed from the end.
fn restore_floats(layout: &mut Layout) {
    let dock = &mut layout.dock;
    let mut seen = HashSet::new();
    layout.floats.retain_mut(|f| {
        let window = matches!(
            dock.get_surface(SurfaceIndex(f.surface)),
            Some(Surface::Window(..))
        );
        let finite = [f.x, f.y, f.w, f.h].iter().all(|v| v.is_finite());
        if !(window && finite && seen.insert(f.surface)) {
            return false;
        }
        f.x = f.x.clamp(-MAX_FLOAT_POS, MAX_FLOAT_POS);
        f.y = f.y.clamp(-MAX_FLOAT_POS, MAX_FLOAT_POS);
        f.w = f.w.clamp(MIN_FLOAT, MAX_DETACHED);
        f.h = f.h.clamp(MIN_FLOAT, MAX_DETACHED);
        true
    });
    for f in &layout.floats {
        if let Some(state) = dock.get_window_state_mut(SurfaceIndex(f.surface)) {
            state
                .set_position(egui_dock::egui::pos2(f.x, f.y))
                .set_size(egui_dock::egui::vec2(f.w, f.h));
        }
    }
}

/// Rejects trees egui_dock could index out of bounds, and fixes what is
/// merely out of range.
fn check_structure(state: &mut DockState<Pane>) -> Result<(), String> {
    let surfaces = state.surfaces_count();
    if surfaces == 0 || surfaces > MAX_SURFACES {
        return Err(format!("{surfaces} dock surfaces"));
    }
    let mut tabs = 0;
    for (i, surface) in state.iter_surfaces_mut().enumerate() {
        match (i, &*surface) {
            (0, Surface::Main(_)) | (1.., Surface::Window(..) | Surface::Empty) => {}
            _ => return Err("surface 0 must be the main one, and only it".into()),
        }
        let in_surface = match surface.node_tree_mut() {
            Some(tree) => check_tree(tree)?,
            None => 0,
        };
        // A floating window without tabs would make egui_dock panic.
        if i > 0 && in_surface == 0 {
            *surface = Surface::Empty;
        }
        tabs += in_surface;
    }
    if tabs > MAX_TABS {
        return Err(format!("{tabs} tabs"));
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Empty,
    Leaf,
    Split,
}

/// Checks one tree's shape and returns its tab count. The nodes are a binary
/// heap: the children of `i` are `2i+1` and `2i+2`.
fn check_tree(tree: &mut Tree<Pane>) -> Result<usize, String> {
    let len = tree.len();
    if len > MAX_NODES {
        return Err(format!("a tree has {len} nodes"));
    }
    let kinds: Vec<Kind> = tree
        .iter()
        .map(|node| match node {
            Node::Empty => Kind::Empty,
            Node::Leaf { .. } => Kind::Leaf,
            Node::Horizontal { .. } | Node::Vertical { .. } => Kind::Split,
        })
        .collect();
    let present = |i: usize| kinds.get(i).is_some_and(|k| *k != Kind::Empty);
    for (i, &kind) in kinds.iter().enumerate() {
        if i > 0 && kind != Kind::Empty && kinds[(i - 1) / 2] != Kind::Split {
            return Err(format!("node {i} has no parent"));
        }
        if kind == Kind::Split && !(present(2 * i + 1) && present(2 * i + 2)) {
            return Err(format!("split {i} is missing a child"));
        }
    }

    let mut tabs = 0;
    for node in tree.iter_mut() {
        match node {
            Node::Leaf {
                tabs: leaf_tabs,
                active,
                ..
            } => {
                if leaf_tabs.is_empty() {
                    return Err("a tab group is empty".into());
                }
                tabs += leaf_tabs.len();
                active.0 = active.0.min(leaf_tabs.len() - 1);
            }
            Node::Horizontal { fraction, .. } | Node::Vertical { fraction, .. } => {
                *fraction = if fraction.is_finite() {
                    fraction.clamp(MIN_FRACTION, MAX_FRACTION)
                } else {
                    0.5
                };
            }
            Node::Empty => {}
        }
    }
    Ok(tabs)
}

/// The first tab that must go: a pane already seen, or the 3D view anywhere
/// but the main surface (it can't float).
fn find_unwanted(
    state: &DockState<Pane>,
) -> Option<(SurfaceIndex, NodeIndex, egui_dock::TabIndex)> {
    let mut seen = HashSet::new();
    for (s, surface) in state.iter_surfaces().enumerate() {
        let Some(tree) = surface.node_tree() else {
            continue;
        };
        for (n, node) in tree.iter().enumerate() {
            for (t, pane) in node.tabs().unwrap_or_default().iter().enumerate() {
                if (s > 0 && *pane == Pane::Viewport) || !seen.insert(*pane) {
                    return Some((SurfaceIndex(s), NodeIndex(n), egui_dock::TabIndex(t)));
                }
            }
        }
    }
    None
}

/// Leaves only focus values egui_dock can index with. The focus is private,
/// so it is re-set rather than read.
fn reset_focus(state: &mut DockState<Pane>) {
    for surface in state.iter_surfaces_mut() {
        if let Some(tree) = surface.node_tree_mut() {
            if let Some(focused) = tree.focused_leaf() {
                tree.set_focused_node(focused);
            }
        }
    }
    let first_leaf = state.main_surface().iter().position(Node::is_leaf);
    state.set_focused_node_and_surface((
        SurfaceIndex::main(),
        NodeIndex(first_leaf.unwrap_or(usize::MAX)),
    ));
}

/// Pulls a detached window's numbers into range. Position is kept as is: it
/// is checked against the real monitors when the window opens.
fn sanitize_detached(d: &mut DetachedSave) -> bool {
    let size = |v: f32, default: f32| {
        if v.is_finite() {
            v.clamp(MIN_DETACHED, MAX_DETACHED)
        } else {
            default
        }
    };
    d.w = size(d.w, DEFAULT_DETACHED.0);
    d.h = size(d.h, DEFAULT_DETACHED.1);
    d.scale = theme::clamp_scale(d.scale);
    if d.x.is_none() || d.y.is_none() {
        d.x = None;
        d.y = None;
    }
    true
}

/// A name usable as a layout name and a file name: 1 to 40 letters, digits,
/// spaces, `_` or `-` and nothing else. Anything else is refused rather than
/// stripped, so `../x` can't quietly turn into a different name. Windows'
/// reserved device names (CON, NUL ...) are refused too, as files.
pub fn sanitize_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    let clean = !name.is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'));
    if !clean {
        return None;
    }
    let upper = name.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            upper
                .strip_prefix(p)
                .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
        });
    (!reserved).then(|| name.to_owned())
}

/// Whether `name` is one of the built-in layouts, which can't be overwritten.
pub fn is_builtin(name: &str) -> bool {
    BUILTINS.iter().any(|b| b.eq_ignore_ascii_case(name))
}

/// Writes `<dir>/<name>.layout.toml` and returns its path.
pub fn export(dir: &Path, name: &str, layout: &Layout) -> Result<PathBuf, String> {
    let name = sanitize_name(name).ok_or("use letters, digits, spaces, _ or - in the name")?;
    let text = toml::to_string(layout).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{name}{FILE_SUFFIX}"));
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The `*.layout.toml` files in `dir` as (name, path), sorted and capped. A
/// missing folder is just an empty list.
pub fn list_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .flatten()
        .take(10 * MAX_LISTED_FILES)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let stem = file_name.to_str()?.strip_suffix(FILE_SUFFIX)?;
            // Only names the Export button could have written.
            (sanitize_name(stem).as_deref() == Some(stem)).then(|| (stem.to_owned(), entry.path()))
        })
        .collect();
    files.sort();
    files.truncate(MAX_LISTED_FILES);
    files
}

/// Reads and checks a layout file.
pub fn import(path: &Path) -> Result<Layout, String> {
    // Only the file name: the full path and TOML's quoted source line made
    // the error toast fill the window.
    let shown = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{shown}: {e}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!("{shown} is too big to be a layout"));
    }
    // Capped again while reading: the file may have grown since the check.
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_string(&mut text))
        .map_err(|e| format!("{shown}: {e}"))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("{shown} is too big to be a layout"));
    }
    let mut layout: Layout =
        toml::from_str(&text).map_err(|e| format!("{shown}: {}", short_toml_error(&e)))?;
    validate(&mut layout).map_err(|e| format!("{shown} is not a usable layout: {e}"))?;
    Ok(layout)
}

/// A TOML error in one line: where it is and what is wrong, without the
/// quoted source line in between.
fn short_toml_error(e: &toml::de::Error) -> String {
    let text = e.to_string();
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or_default();
    match lines.next_back() {
        Some(last) if !last.starts_with('|') && !last.starts_with('^') => {
            format!("{first}: {last}")
        }
        _ => first.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(dock: DockState<Pane>) -> Layout {
        Layout {
            dock,
            detached: Vec::new(),
            floats: Vec::new(),
        }
    }

    fn panes(layout: &Layout) -> Vec<Pane> {
        let mut all: Vec<Pane> = layout.dock.iter_all_tabs().map(|(_, p)| *p).collect();
        all.extend(layout.detached.iter().map(|d| d.pane));
        all
    }

    /// Replaces every "nodes" array in a layout's TOML with its first `keep` entries.
    fn truncate_nodes(value: &mut toml::Value, keep: usize) {
        match value {
            toml::Value::Table(table) => {
                for (key, v) in table.iter_mut() {
                    match v {
                        toml::Value::Array(nodes) if key == "nodes" => nodes.truncate(keep),
                        _ => truncate_nodes(v, keep),
                    }
                }
            }
            toml::Value::Array(items) => items.iter_mut().for_each(|v| truncate_nodes(v, keep)),
            _ => {}
        }
    }

    #[test]
    fn names_must_already_be_clean() {
        assert_eq!(
            sanitize_name("  My layout-2_b "),
            Some("My layout-2_b".into())
        );
        assert_eq!(sanitize_name("../x"), None);
        assert_eq!(sanitize_name("a/b"), None);
        assert_eq!(sanitize_name("a.b"), None);
        assert_eq!(sanitize_name("   "), None);
        assert_eq!(sanitize_name(""), None);
        assert_eq!(sanitize_name(&"a".repeat(40)).map(|n| n.len()), Some(40));
        assert_eq!(sanitize_name(&"a".repeat(41)), None);
        assert_eq!(sanitize_name("con"), None);
        assert_eq!(sanitize_name("LPT3"), None);
        assert_eq!(sanitize_name("COM10"), Some("COM10".into()));
        assert_eq!(sanitize_name("caf\u{e9}"), None);
    }

    #[test]
    fn duplicate_panes_are_dropped_dock_first() {
        let mut state = DockState::new(vec![Pane::Viewport, Pane::Log]);
        state
            .main_surface_mut()
            .split_right(NodeIndex::root(), 0.5, vec![Pane::Log, Pane::Plot]);
        let mut l = layout(state);
        l.detached.push(DetachedSave::new(Pane::Plot, 1.0));
        l.detached.push(DetachedSave::new(Pane::Audio, 1.0));
        l.detached.push(DetachedSave::new(Pane::Audio, 1.0));
        validate(&mut l).expect("fixable");
        let mut all = panes(&l);
        all.sort_by_key(|p| *p as u8);
        assert_eq!(all, [Pane::Viewport, Pane::Audio, Pane::Plot, Pane::Log]);
        assert_eq!(l.detached.len(), 1);
    }

    #[test]
    fn missing_viewport_is_added_to_the_main_surface() {
        let mut l = layout(DockState::new(vec![Pane::Log]));
        l.detached.push(DetachedSave::new(Pane::Viewport, 1.0));
        validate(&mut l).expect("fixable");
        assert!(l.dock.find_main_surface_tab(&Pane::Viewport).is_some());
        assert!(l.detached.is_empty());
    }

    #[test]
    fn floating_viewport_moves_to_the_main_surface() {
        let mut state = DockState::new(vec![Pane::Log]);
        state.add_window(vec![Pane::Viewport]);
        let mut l = layout(state);
        validate(&mut l).expect("fixable");
        assert!(l.dock.find_main_surface_tab(&Pane::Viewport).is_some());
        // The window held only the 3D view, so it is gone.
        assert_eq!(
            l.dock.iter_surfaces().filter_map(|s| s.node_tree()).count(),
            1
        );
    }

    #[test]
    fn an_empty_main_surface_gets_the_viewport() {
        let mut state = DockState::new(vec![Pane::Log]);
        state.main_surface_mut().remove_leaf(NodeIndex::root());
        let mut l = layout(state);
        validate(&mut l).expect("fixable");
        assert_eq!(panes(&l), [Pane::Viewport]);
    }

    #[test]
    fn broken_trees_are_rejected() {
        // Children left without their parent.
        let mut orphans = layout(dock::default_layout());
        if let Some(root) = orphans.dock.main_surface_mut().iter_mut().next() {
            *root = Node::Empty;
        }
        assert!(validate(&mut orphans).is_err());

        // A split whose children are missing.
        let mut value = toml::Value::try_from(layout(dock::default_layout())).expect("to toml");
        truncate_nodes(&mut value, 2);
        let mut cut: Layout = value.try_into().expect("still parses");
        assert!(validate(&mut cut).is_err());

        // Too many nodes: ten splits down one side reach index 2046.
        let mut big = layout(DockState::new(vec![Pane::Viewport]));
        let mut at = NodeIndex::root();
        for _ in 0..10 {
            [at, _] = big
                .dock
                .main_surface_mut()
                .split_right(at, 0.5, vec![Pane::Log]);
        }
        assert!(validate(&mut big).is_err());
    }

    #[test]
    fn detached_numbers_are_clamped() {
        let mut l = layout(dock::default_layout());
        let mut d = DetachedSave::new(Pane::Shortcuts, 9.0);
        d.w = 1.0e9;
        d.h = f32::NAN;
        d.x = Some(5);
        l.detached.push(d);
        validate(&mut l).expect("fixable");
        let d = &l.detached[0];
        assert_eq!((d.w, d.h), (MAX_DETACHED, DEFAULT_DETACHED.1));
        assert_eq!(d.scale, theme::SCALE_MAX);
        assert_eq!((d.x, d.y), (None, None));
    }

    #[test]
    fn built_ins_are_valid_and_complete() {
        for name in BUILTINS {
            let l = builtin(name).expect(name);
            assert!(l.dock.find_main_surface_tab(&Pane::Viewport).is_some());
            assert!(l.detached.is_empty());
        }
        assert_eq!(panes(&builtin("Demo").expect("demo")), [Pane::Viewport]);
        assert_eq!(panes(&builtin("Debug").expect("debug")).len(), 11);
        assert!(builtin("nope").is_none());
    }

    #[test]
    fn files_export_list_and_import() {
        let dir = std::env::temp_dir().join(format!("ddl0-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let debug = builtin("Debug").expect("debug");
        let path = export(&dir, "My debug", &debug).expect("exports");
        assert!(path.ends_with("My debug.layout.toml"));
        assert!(export(&dir, "../evil", &debug).is_err());

        std::fs::write(dir.join("notes.txt"), "x").expect("writes");
        std::fs::write(dir.join("bad.name.layout.toml"), "x").expect("writes");
        std::fs::write(dir.join("junk.layout.toml"), "not a layout").expect("writes");
        std::fs::write(dir.join("huge.layout.toml"), vec![b'#'; 300 * 1024]).expect("writes");
        let names: Vec<String> = list_files(&dir).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["My debug", "huge", "junk"]);

        assert_eq!(import(&path).expect("imports"), debug);
        assert!(import(&dir.join("junk.layout.toml")).is_err());
        assert!(import(&dir.join("huge.layout.toml")).is_err());
        assert!(import(&dir.join("missing.layout.toml")).is_err());
        assert!(list_files(&dir.join("nope")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn floats_round_trip_and_are_checked() {
        let mut state = dock::default_layout();
        let first = state.add_window(vec![Pane::Shortcuts]);
        let second = state.add_window(vec![Pane::Pca]);
        let float = |surface: usize, x: f32, w: f32| FloatSave {
            surface,
            x,
            y: 20.0,
            w,
            h: 1.0e9,
        };
        let mut l = layout(state);
        l.floats = vec![
            float(first.0, 10.0, 300.0),
            float(second.0, f32::NAN, 300.0),
            float(0, 10.0, 300.0),
            float(99, 10.0, 300.0),
            float(first.0, 50.0, 300.0),
        ];
        validate(&mut l).expect("fixable");
        // Only the first float for a floating window survives; w and h are clamped.
        assert_eq!(
            l.floats,
            [float(first.0, 10.0, 300.0)].map(|mut f| {
                f.h = MAX_DETACHED;
                f
            })
        );
        let text = toml::to_string(&l).expect("serialises");
        let mut back: Layout = toml::from_str(&text).expect("parses");
        validate(&mut back).expect("valid");
        assert_eq!(l, back);
        assert_eq!(back.floats.len(), 1);
    }

    #[test]
    fn layout_round_trips_through_toml() {
        let mut state = dock::default_layout();
        let first = state.add_window(vec![Pane::Shortcuts]);
        state.add_window(vec![Pane::Pca]);
        state.remove_surface(first);
        let mut l = layout(state);
        l.detached.push(DetachedSave::new(Pane::Audio, 1.2));
        validate(&mut l).expect("valid");
        let text = toml::to_string(&l).expect("serialises");
        let mut back: Layout = toml::from_str(&text).expect("parses");
        validate(&mut back).expect("valid");
        assert_eq!(l, back);
    }
}
