//! DD-L0-LA59 simulator: runs PropMaker_Feather/code.py unmodified against
//! simulated hardware and shows the droid in 3D.
// The test exe stays a console app, so it prints its results and the caller
// waits for its exit code.
#![cfg_attr(all(not(debug_assertions), not(test)), windows_subsystem = "windows")]

mod a11y;
mod announce;
mod audio;
mod capture;
mod config;
mod deploy;
mod dock;
mod droid;
mod editor;
mod hw;
mod layout;
mod overlay;
mod palette;
mod part_edit;
mod parts;
mod prefs;
mod pyhost;
mod scene;
mod settings;
mod theme;
mod toast;
mod ui;
mod wav;

use std::path::{Path, PathBuf};

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy::window::WindowPosition;
use bevy_egui::EguiPlugin;

use config::SimConfig;
use hw::hw;
use pyhost::PyHost;

/// Where the firmware, simulator config and 3D assets live.
#[derive(Resource, Clone)]
pub struct Paths {
    pub code: PathBuf,
    pub simulator: PathBuf,
    pub assets: PathBuf,
    /// Files the user saves from the app (logs, screenshots). Next to the exe,
    /// so they stay out of the repo's tracked files.
    pub captures: PathBuf,
    /// Saved preferences. Next to the exe, which lives in the gitignored
    /// simulator/dist, so they never land in the repo.
    pub prefs: PathBuf,
    /// Drop-in shims that override or add to the built-in ones. Next to the
    /// exe; rebuilding keeps it.
    pub shims: PathBuf,
    /// The writable layer of the simulated CIRCUITPY drive (log.txt, log_mode).
    /// Next to the exe; rebuilding keeps it.
    pub fs: PathBuf,
    /// Layout files the user exports and imports. Next to the exe; rebuilding keeps it.
    pub layouts: PathBuf,
}

impl Paths {
    /// The simulator's own files are found from the exe, so the firmware can
    /// live anywhere. `code` is the firmware's code.py, if one was found.
    fn new(exe_dir: &Path, code: PathBuf) -> Self {
        // The exe lives in simulator/dist; a copy elsewhere falls back to its parent.
        let simulator = exe_dir
            .ancestors()
            .map(|dir| dir.join("simulator"))
            .find(|dir| dir.join("assets").is_dir())
            .unwrap_or_else(|| exe_dir.parent().unwrap_or(exe_dir).to_path_buf());
        Self {
            code,
            assets: simulator.join("assets"),
            captures: exe_dir.join("captures"),
            prefs: exe_dir.join("ddl0-sim.prefs.toml"),
            shims: exe_dir.join("shims"),
            fs: exe_dir.join("fs"),
            layouts: exe_dir.join("layouts"),
            simulator,
        }
    }

    /// The firmware folder: the one code.py is in.
    pub fn fw_dir(&self) -> &Path {
        self.code.parent().unwrap_or(Path::new("."))
    }
}

/// The code.py inside `dir`, or why `dir` can't be the firmware folder.
pub fn firmware_code_path(dir: &Path) -> Result<PathBuf, String> {
    let code = dir.join("code.py");
    if !code.is_file() {
        return Err(format!("No code.py in {}", dir.display()));
    }
    std::fs::canonicalize(&code)
        .map(strip_verbatim)
        .map_err(|e| format!("{}: {e}", code.display()))
}

/// Where code.py is, first match wins: `ddl0-sim.exe [path\to\code.py]`, the
/// folder saved in the prefs (if it still has code.py), then
/// PropMaker_Feather/code.py in a folder above the exe.
fn find_firmware(exe_dir: &Path, saved: Option<&Path>) -> Result<Option<PathBuf>, String> {
    if let Some(arg) = std::env::args_os().nth(1) {
        let code = PathBuf::from(arg);
        return std::fs::canonicalize(&code)
            .map(|c| Some(strip_verbatim(c)))
            .map_err(|e| format!("{}: {e}", code.display()));
    }
    if let Some(code) = saved.and_then(|dir| firmware_code_path(dir).ok()) {
        return Ok(Some(code));
    }
    Ok(exe_dir
        .ancestors()
        .map(|dir| dir.join("PropMaker_Feather"))
        .find_map(|dir| firmware_code_path(&dir).ok()))
}

/// Asks for the firmware folder until one with code.py is picked; `None` if
/// the user cancels. Runs before the app window exists, so blocking is fine.
fn ask_for_firmware(exe_dir: &Path) -> Option<PathBuf> {
    let mut start = exe_dir.to_path_buf();
    loop {
        let dir = rfd::FileDialog::new()
            .set_title("Choose the firmware folder (the one with code.py)")
            .set_directory(&start)
            .pick_folder()?;
        match firmware_code_path(&dir) {
            Ok(code) => return Some(code),
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("DD-L0-LA59 Simulator")
                    .set_description(e)
                    .show();
                start = dir;
            }
        }
    }
}

/// Shows a startup error. The release exe has no console, so eprintln alone
/// would vanish.
fn fail(msg: &str) -> ! {
    eprintln!("ddl0-sim: {msg}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("DD-L0-LA59 Simulator")
        .set_description(msg)
        .show();
    std::process::exit(2);
}

/// canonicalize() returns `\\?\C:\...` on Windows, which Python's
/// tracebacks and some loaders print awkwardly; plain drive paths are fine.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}

fn main() {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    // The prefs come first: they may say where the firmware is.
    let mut paths = Paths::new(&exe_dir, PathBuf::new());
    let (mut prefs, prefs_warning) = prefs::load(&paths.prefs);
    if let Some(w) = prefs_warning {
        hw().push_line(w);
    }
    paths.code = match find_firmware(&exe_dir, prefs.firmware_dir.as_deref()) {
        Ok(Some(code)) => code,
        Ok(None) => match ask_for_firmware(&exe_dir) {
            Some(code) => {
                // Saved now: save_prefs only writes what changed while running.
                prefs.firmware_dir = code.parent().map(Path::to_path_buf);
                if let Err(e) = prefs.save(&paths.prefs) {
                    hw().push_line(format!("[sim] could not save preferences: {e}"));
                }
                code
            }
            None => fail(&format!(
                "No firmware folder chosen, and no PropMaker_Feather/code.py above {}.",
                exe_dir.display()
            )),
        },
        Err(e) => fail(&e),
    };
    hw().push_line(format!("[sim] firmware: {}", paths.fw_dir().display()));
    let sim_toml = paths.simulator.join("sim.toml");
    let (cfg, cfg_warning) = SimConfig::load(&sim_toml);
    let sim_toml_mtime = ui::file_mtime(&sim_toml);
    if let Some(w) = cfg_warning {
        hw().push_line(w);
    }
    hw().board = cfg.board.clone();
    {
        let mut s = hw();
        s.strict = prefs.strict;
        s.board_speed = prefs.board_speed;
        s.servo_speed = prefs.servo_speed;
    }

    pyhost::init_python();
    let mut host = PyHost::new(paths.code.clone(), paths.shims.clone(), paths.fs.clone());
    host.start();
    let code_mtime = ui::firmware_mtime(&paths.code);
    let shims_mtime = ui::shims_mtime(&paths.shims);
    let parts_mtime = ui::file_mtime(&paths.simulator.join(parts::PARTS_FILE));

    // Single-pass: `ui::panels` draws the main window and every detached one
    // from `Update`. Multi-pass would need a schedule per window; the option is
    // deprecated but still supported in bevy_egui 0.38.
    #[allow(deprecated)]
    let egui_plugin = EguiPlugin {
        enable_multipass_for_primary_context: false,
        ..default()
    };

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "DD-L0-LA59 Simulator".into(),
                        resolution: dock::resolution(prefs.window_width, prefs.window_height),
                        position: match (prefs.window_x, prefs.window_y) {
                            (Some(x), Some(y)) => WindowPosition::At(IVec2::new(x, y)),
                            _ => WindowPosition::Automatic,
                        },
                        ..default()
                    }),
                    // Closing is handled in dock.rs: a detached pane's window
                    // docks back instead of vanishing, the main one exits.
                    close_when_requested: false,
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: paths.assets.to_string_lossy().into_owned(),
                    ..default()
                }),
        )
        .add_plugins((
            MeshPickingPlugin,
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
            egui_plugin,
        ))
        .insert_resource(cfg)
        .insert_resource(paths)
        .insert_resource(ui::Host(host))
        .insert_resource(ui::UiState::new(
            code_mtime,
            shims_mtime,
            sim_toml_mtime,
            parts_mtime,
            &prefs,
        ))
        .insert_resource(audio::AudioMix::new(prefs.volume, prefs.muted))
        .insert_resource(theme::ActiveTheme::new(
            prefs.theme,
            prefs.ui_scale,
            prefs.animations,
        ))
        .insert_resource(overlay::Overlays::from_prefs(&prefs))
        .insert_resource(prefs)
        .add_plugins((
            a11y::A11yPlugin,
            theme::ThemePlugin,
            scene::ScenePlugin,
            parts::PartsPlugin,
            part_edit::PartEditPlugin,
            audio::AudioPlugin,
            capture::CapturePlugin,
            toast::ToastPlugin,
            ui::UiPlugin,
            dock::DockPlugin,
            overlay::OverlayPlugin,
        ))
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder, removed again when dropped.
    struct Tmp(PathBuf);
    impl Tmp {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("ddl0-fw-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn firmware_folder_needs_code_py() {
        let t = Tmp::new("empty");
        let err = firmware_code_path(&t.0).unwrap_err();
        assert!(err.starts_with("No code.py in"), "{err}");
    }

    #[test]
    fn code_py_must_be_a_file() {
        let t = Tmp::new("dir");
        std::fs::create_dir(t.0.join("code.py")).unwrap();
        assert!(firmware_code_path(&t.0).is_err());
    }

    #[test]
    fn firmware_folder_gives_plain_code_path() {
        let t = Tmp::new("ok");
        std::fs::write(t.0.join("code.py"), "").unwrap();
        let code = firmware_code_path(&t.0).unwrap();
        assert_eq!(code.file_name().unwrap(), "code.py");
        // No \?\ prefix, which tracebacks would print.
        assert!(!code.to_string_lossy().starts_with(r"\?\"), "{code:?}");
        assert!(code.is_file());
    }

    #[test]
    fn simulator_files_found_above_the_exe() {
        let t = Tmp::new("layout");
        let dist = t.0.join("simulator").join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::create_dir_all(t.0.join("simulator").join("assets")).unwrap();
        let paths = Paths::new(&dist, PathBuf::from("elsewhere/code.py"));
        assert_eq!(paths.simulator, t.0.join("simulator"));
        assert_eq!(paths.fw_dir(), Path::new("elsewhere"));
    }
}
