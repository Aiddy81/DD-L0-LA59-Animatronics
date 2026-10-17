//! DD-L0-LA59 simulator: runs PropMaker_Feather/code.py unmodified against
//! simulated hardware and shows the droid in 3D.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod droid;
mod hw;
mod pyhost;
mod scene;
mod ui;

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
    let (cfg, cfg_warning) = SimConfig::load(&paths.simulator.join("sim.toml"));
    if let Some(w) = cfg_warning {
        hw().push_line(w);
    }

    pyhost::init_python();
    let mut host = PyHost::new(paths.code.clone());
    host.start();
    let code_mtime = std::fs::metadata(&paths.code)
        .and_then(|m| m.modified())
        .ok();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "DD-L0-LA59 Simulator".into(),
                        resolution: (1500.0, 920.0).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: paths.assets.to_string_lossy().into_owned(),
                    ..default()
                }),
        )
        .add_plugins((
            MeshPickingPlugin,
            EguiPlugin {
                enable_multipass_for_primary_context: false,
            },
        ))
        .insert_resource(cfg)
        .insert_resource(paths)
        .insert_resource(ui::Host(host))
        .insert_resource(ui::UiState::new(code_mtime))
        .add_plugins((scene::ScenePlugin, audio::AudioPlugin, ui::UiPlugin))
        .run();
}
