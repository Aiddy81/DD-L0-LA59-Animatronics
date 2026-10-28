//! DD-L0-LA59 simulator: runs PropMaker_Feather/code.py unmodified against
//! simulated hardware and shows the droid in 3D.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod capture;
mod config;
mod deploy;
mod dock;
mod droid;
mod editor;
mod hw;
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
}

impl Paths {
    /// `ddl0-sim.exe [path\to\code.py]`; otherwise walk up from the exe to
    /// the repo that contains PropMaker_Feather/code.py.
    fn discover() -> Result<Self, String> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let code = match std::env::args_os().nth(1) {
            Some(arg) => PathBuf::from(arg),
            None => exe_dir
                .ancestors()
                .map(|dir| dir.join("PropMaker_Feather").join("code.py"))
                .find(|p| p.is_file())
                .ok_or_else(|| {
                    format!(
                        "could not find PropMaker_Feather/code.py above {}",
                        exe_dir.display()
                    )
                })?,
        };
        let code = std::fs::canonicalize(&code).map_err(|e| format!("{}: {e}", code.display()))?;
        let code = strip_verbatim(code);
        let repo = code
            .parent()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let simulator = repo.join("simulator");
        Ok(Self {
            code,
            assets: simulator.join("assets"),
            captures: exe_dir.join("captures"),
            prefs: exe_dir.join("ddl0-sim.prefs.toml"),
            shims: exe_dir.join("shims"),
            fs: exe_dir.join("fs"),
            simulator,
        })
    }
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
    let paths = match Paths::discover() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ddl0-sim: {e}");
            std::process::exit(2);
        }
    };
    let sim_toml = paths.simulator.join("sim.toml");
    let (cfg, cfg_warning) = SimConfig::load(&sim_toml);
    let sim_toml_mtime = ui::file_mtime(&sim_toml);
    if let Some(w) = cfg_warning {
        hw().push_line(w);
    }
    hw().board = cfg.board.clone();
    let (prefs, prefs_warning) = prefs::load(&paths.prefs);
    if let Some(w) = prefs_warning {
        hw().push_line(w);
    }
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

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "DD-L0-LA59 Simulator".into(),
                        resolution: (prefs.window_width, prefs.window_height).into(),
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
            EguiPlugin {
                enable_multipass_for_primary_context: false,
            },
        ))
        .insert_resource(cfg)
        .insert_resource(paths)
        .insert_resource(ui::Host(host))
        .insert_resource(ui::UiState::new(
            code_mtime,
            shims_mtime,
            sim_toml_mtime,
            &prefs,
        ))
        .insert_resource(audio::AudioMix::new(prefs.volume, prefs.muted))
        .insert_resource(theme::ActiveTheme::new(prefs.theme, prefs.ui_scale))
        .insert_resource(prefs)
        .add_plugins((
            theme::ThemePlugin,
            scene::ScenePlugin,
            audio::AudioPlugin,
            capture::CapturePlugin,
            toast::ToastPlugin,
            ui::UiPlugin,
            dock::DockPlugin,
        ))
        .run();
}
