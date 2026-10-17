//! Plays the WAV files `code.py` sends to its single mixer voice.

use std::sync::Arc;

use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;

use crate::hw::{hw, AudioCmd};

#[derive(Component)]
struct CurrentSound;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, play_requests);
    }
}

fn play_requests(
    mut commands: Commands,
    mut sources: ResMut<Assets<AudioSource>>,
    mut current: Query<(Entity, Option<&mut AudioSink>), With<CurrentSound>>,
) {
    let cmds = std::mem::take(&mut hw().audio);
    if cmds.is_empty() {
        return;
    }
    // code.py sets the level just after starting a sound; fold that into the
    // new sound rather than racing its sink into existence.
    let mut start: Option<(String, f32, bool)> = None;
    let mut stop = false;
    for cmd in cmds {
        match cmd {
            AudioCmd::Play {
                path,
                level,
                looped,
            } => {
                stop = true;
                start = Some((path, level, looped));
            }
            AudioCmd::Stop => {
                stop = true;
                start = None;
            }
            AudioCmd::Volume(level) => match start.as_mut() {
                Some(s) => s.1 = level,
                None => {
                    for (_, sink) in &mut current {
                        if let Some(mut sink) = sink {
                            sink.set_volume(Volume::Linear(level));
                        }
                    }
                }
            },
        }
    }
    if stop {
        for (entity, _) in &current {
            commands.entity(entity).despawn();
        }
    }
    let Some((path, level, looped)) = start else {
        return;
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let handle = sources.add(AudioSource {
                bytes: Arc::from(bytes),
            });
            let mode = if looped {
                PlaybackMode::Loop
            } else {
                PlaybackMode::Despawn
            };
            commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings {
                    mode,
                    volume: Volume::Linear(level),
                    ..default()
                },
                CurrentSound,
            ));
        }
        Err(e) => hw().push_line(format!("[sim] cannot play {path}: {e}")),
    }
}
