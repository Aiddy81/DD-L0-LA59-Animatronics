//! Camera presets, screenshots and frame-sequence recording, driven from the
//! toolbar through the `ViewControls` resource.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::hw::hw;
use crate::scene::CameraView;
use crate::toast::{ToastKind, Toasts};
use crate::Paths;

/// Seconds between recorded frames: 10 fps is plenty for a GIF.
const RECORD_INTERVAL_S: f32 = 0.1;
/// Caps disk use: ~300 PNGs.
const RECORD_MAX_S: f32 = 30.0;

/// Requests set by the toolbar and consumed by `capture` / `orbit_camera`.
#[derive(Resource, Default)]
pub struct ViewControls {
    pub camera: Option<CameraView>,
    pub screenshot: bool,
    pub toggle_record: bool,
    recording: Option<Recording>,
}

impl ViewControls {
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }
}

struct Recording {
    dir: PathBuf,
    frame: u32,
    /// Seconds since the start at which the next frame is due.
    next: f32,
    started: Instant,
}

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewControls>()
            .add_systems(Update, capture);
    }
}

/// Seconds since the Unix epoch, for unique capture file names.
pub fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn save_png(commands: &mut Commands, path: PathBuf) {
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

fn stop_recording(rec: Recording) {
    hw().push_line(format!(
        "[sim] recorded {} frames to {}",
        rec.frame,
        rec.dir.display()
    ));
}

fn start_recording(captures: &Path) -> Result<Recording, std::io::Error> {
    let dir = captures.join(format!("rec-{}", unix_secs()));
    std::fs::create_dir_all(&dir)?;
    Ok(Recording {
        dir,
        frame: 0,
        next: 0.0,
        started: Instant::now(),
    })
}

fn capture(
    mut commands: Commands,
    mut controls: ResMut<ViewControls>,
    paths: Res<Paths>,
    mut toasts: ResMut<Toasts>,
) {
    if std::mem::take(&mut controls.screenshot) {
        let path = paths.captures.join(format!("shot-{}.png", unix_secs()));
        let note = match std::fs::create_dir_all(&paths.captures) {
            Ok(()) => {
                save_png(&mut commands, path.clone());
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                toasts.push(ToastKind::Ok, format!("Screenshot saved: {name}"), None);
                format!("[sim] screenshot saved to {}", path.display())
            }
            Err(e) => {
                toasts.push(ToastKind::Error, "Could not save the screenshot", None);
                format!("[sim] could not save the screenshot: {e}")
            }
        };
        hw().push_line(note);
    }

    if std::mem::take(&mut controls.toggle_record) {
        if let Some(rec) = controls.recording.take() {
            stop_recording(rec);
        } else {
            match start_recording(&paths.captures) {
                Ok(rec) => {
                    hw().push_line(format!("[sim] recording frames to {}", rec.dir.display()));
                    controls.recording = Some(rec);
                }
                Err(e) => hw().push_line(format!("[sim] could not start recording: {e}")),
            }
        }
    }

    let Some(rec) = controls.recording.as_mut() else {
        return;
    };
    let elapsed = rec.started.elapsed().as_secs_f32();
    if elapsed >= rec.next {
        save_png(
            &mut commands,
            rec.dir.join(format!("frame-{:05}.png", rec.frame)),
        );
        rec.frame += 1;
        rec.next += RECORD_INTERVAL_S;
    }
    if elapsed >= RECORD_MAX_S {
        if let Some(rec) = controls.recording.take() {
            stop_recording(rec);
        }
    }
}
