//! Plays the WAV files `code.py` sends to its single mixer voice, and keeps
//! what the Audio panel shows: what is playing, the output volume, and which
//! sounds the board could or couldn't play.

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::audio::{PlaybackMode, Volume};
use bevy::prelude::*;

use crate::hw::{hw, AudioCmd};
use crate::wav::{self, MixerFormat};

/// A WAV header sits in the first bytes; this is enough to get past a LIST
/// chunk of tags without reading the whole sound.
const HEADER_BYTES: u64 = 64 * 1024;
const RESCAN_EVERY: Duration = Duration::from_secs(1);

#[derive(Component)]
struct CurrentSound;

/// The sound on the speaker now, and its level over time for the meter.
pub struct Playing {
    pub name: String,
    pub started: Instant,
    pub looped: bool,
    pub duration_s: f32,
    /// RMS level (0..1) per 50 ms window; empty if the format isn't readable.
    pub envelope: Vec<f32>,
}

#[derive(Resource, Default)]
pub struct NowPlaying(pub Option<Playing>);

impl NowPlaying {
    /// The envelope at the current wall time (audio isn't on the sim clock),
    /// wrapping while looped; 0 when idle.
    pub fn level(&self) -> f32 {
        let Some(p) = &self.0 else { return 0.0 };
        let mut t = p.started.elapsed().as_secs_f32();
        if p.looped && p.duration_s > 0.0 {
            t %= p.duration_s;
        }
        p.envelope
            .get((t / wav::envelope_step_s()) as usize)
            .copied()
            .unwrap_or(0.0)
    }
}

/// The simulator's own output volume and mute, on top of the firmware's
/// mixer level; the speaker plays `level * volume`, or nothing when muted.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct AudioMix {
    pub volume: f32,
    pub muted: bool,
    /// The firmware's `mixer.voice[0].level` for the current sound.
    pub level: f32,
}

impl Default for AudioMix {
    fn default() -> Self {
        Self::new(1.0, false)
    }
}

impl AudioMix {
    pub fn new(volume: f32, muted: bool) -> Self {
        Self {
            volume: volume.clamp(0.0, 1.0),
            muted,
            level: 1.0,
        }
    }

    pub fn effective(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.level * self.volume
        }
    }
}

/// One `sounds/*.wav` as the Audio panel lists it.
pub struct SoundFile {
    pub name: String,
    /// (rate, channels, bits) when the header could be read.
    pub format: Option<(u32, u16, u16)>,
    pub problem: Option<String>,
}

#[derive(Default)]
pub struct SoundScan {
    pub sounds: Vec<SoundFile>,
    /// (file, "behaviours/motion.py:136") for each `play_wav("x.wav")` whose
    /// file isn't in sounds/.
    pub missing: Vec<(String, String)>,
}

/// Lists `fw_dir/sounds` with each file's problems, and the sounds the firmware
/// names that aren't there. Reads headers only.
pub fn scan_sounds(fw_dir: &Path, mixer: Option<MixerFormat>) -> SoundScan {
    let sounds_dir = fw_dir.join("sounds");
    let mut names: Vec<String> = std::fs::read_dir(&sounds_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        // `._x.wav` are macOS resource forks, not sounds.
        .filter(|n| !n.starts_with("._") && n.to_ascii_lowercase().ends_with(".wav"))
        .collect();
    names.sort();
    let sounds = names
        .iter()
        .map(|name| {
            let mut head = Vec::new();
            let read = std::fs::File::open(sounds_dir.join(name))
                .and_then(|f| f.take(HEADER_BYTES).read_to_end(&mut head));
            let info = match read {
                Ok(_) => wav::wav_info(&head),
                Err(e) => Err(e.to_string()),
            };
            match info {
                Ok(fmt) => SoundFile {
                    name: name.clone(),
                    format: Some((fmt.rate, fmt.channels, fmt.bits)),
                    problem: wav::format_problem(&fmt, mixer),
                },
                Err(e) => SoundFile {
                    name: name.clone(),
                    format: None,
                    problem: Some(e),
                },
            }
        })
        .collect();

    let mut missing = Vec::new();
    let mut sources = Vec::new();
    firmware_sources(fw_dir, fw_dir, &mut sources);
    sources.sort();
    for rel in sources {
        let Ok(text) = std::fs::read_to_string(fw_dir.join(&rel)) else {
            continue;
        };
        for (file, line) in wav::play_wav_literals(&text) {
            let entry = (file, format!("{rel}:{line}"));
            if !sounds_dir.join(&entry.0).is_file() && !missing.contains(&entry) {
                missing.push(entry);
            }
        }
    }
    SoundScan { sounds, missing }
}

/// Firmware `.py` files under `dir`, as `/`-separated paths relative to `root`.
fn firmware_sources(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !crate::ui::NOT_FIRMWARE.contains(&name.as_str()) {
                firmware_sources(root, &path, out);
            }
        } else if name.ends_with(".py") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// The latest scan, refreshed only while the Audio panel is open.
#[derive(Resource, Default)]
pub struct Sounds {
    pub scan: SoundScan,
    /// Set by the panel each frame it is open.
    pub wanted: bool,
    scanned_at: Option<Instant>,
    scanned_mixer: Option<MixerFormat>,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NowPlaying>()
            .init_resource::<AudioMix>()
            .init_resource::<Sounds>()
            .add_systems(
                Update,
                (play_requests, apply_mix, expire_playing, rescan_sounds).chain(),
            );
    }
}

fn play_requests(
    mut commands: Commands,
    mut sources: ResMut<Assets<AudioSource>>,
    current: Query<Entity, With<CurrentSound>>,
    mut mix: ResMut<AudioMix>,
    mut now: ResMut<NowPlaying>,
) {
    let cmds = std::mem::take(&mut hw().audio);
    if cmds.is_empty() {
        return;
    }
    // code.py sets the level just after starting a sound; fold that into the
    // new sound rather than racing its sink into existence.
    let mut start: Option<(String, bool)> = None;
    let mut level = None;
    let mut stop = false;
    for cmd in cmds {
        match cmd {
            AudioCmd::Play {
                path,
                level: l,
                looped,
            } => {
                stop = true;
                start = Some((path, looped));
                level = Some(l);
            }
            AudioCmd::Stop => {
                stop = true;
                start = None;
            }
            AudioCmd::Volume(l) => level = Some(l),
        }
    }
    // Compared first so an unchanged level doesn't mark the resource changed.
    if let Some(l) = level.filter(|&l| l != mix.level) {
        mix.level = l;
    }
    if stop {
        for entity in &current {
            commands.entity(entity).despawn();
        }
        now.0 = None;
    }
    let Some((path, looped)) = start else {
        return;
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let (duration_s, envelope) = match wav::wav_info(&bytes) {
                Ok(fmt) => (fmt.duration_s(), wav::envelope(&bytes, &fmt)),
                Err(_) => (0.0, Vec::new()),
            };
            now.0 = Some(Playing {
                name: Path::new(&path)
                    .file_name()
                    .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned()),
                started: Instant::now(),
                looped,
                duration_s,
                envelope,
            });
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
                    volume: Volume::Linear(mix.effective()),
                    ..default()
                },
                CurrentSound,
            ));
        }
        Err(e) => hw().push_line(format!("[sim] cannot play {path}: {e}")),
    }
}

/// Re-applies the volume to the playing sound when the firmware's level or the
/// panel's volume/mute changes.
fn apply_mix(mix: Res<AudioMix>, mut sinks: Query<&mut AudioSink, With<CurrentSound>>) {
    if !mix.is_changed() {
        return;
    }
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(mix.effective()));
    }
}

/// A one-shot sound ends on its own; the meter should notice.
fn expire_playing(mut now: ResMut<NowPlaying>) {
    let done = now
        .0
        .as_ref()
        .is_some_and(|p| !p.looped && p.started.elapsed().as_secs_f32() > p.duration_s);
    if done {
        now.0 = None;
    }
}

fn rescan_sounds(mut sounds: ResMut<Sounds>, paths: Res<crate::Paths>) {
    if !sounds.wanted {
        return;
    }
    let mixer = hw().mixer;
    let stale = sounds
        .scanned_at
        .is_none_or(|t| t.elapsed() >= RESCAN_EVERY)
        || mixer != sounds.scanned_mixer;
    if !stale {
        return;
    }
    let dir = paths.code.parent().unwrap_or(Path::new("."));
    sounds.scan = scan_sounds(dir, mixer);
    sounds.scanned_at = Some(Instant::now());
    sounds.scanned_mixer = mixer;
}
