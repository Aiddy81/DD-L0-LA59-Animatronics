//! Reading WAV headers and the firmware's `play_wav("x.wav")` calls, for the
//! Audio panel: what each sound's format is, whether the mixer would accept
//! it, and which sounds code.py asks for that aren't on disk.
//!
//! Only std, so the tests run with plain `rustc --test`.

use std::ops::Range;

/// Length of one level-meter window.
const WINDOW_S: f32 = 0.05;

/// The fields of a WAV `fmt ` chunk that matter here, plus where the sample
/// data sits in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct WavFormat {
    /// 1 is PCM; anything else (float, ADPCM, extensible) isn't playable on the board.
    pub format: u16,
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
    /// Byte range of the sample data, cut short if the file is.
    pub data: Range<usize>,
}

impl WavFormat {
    fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * usize::from(self.bits) / 8
    }

    pub fn duration_s(&self) -> f32 {
        let frame = self.frame_bytes();
        if frame == 0 || self.rate == 0 {
            return 0.0;
        }
        (self.data.len() / frame) as f32 / self.rate as f32
    }

    /// What CircuitPython's `WaveFile` reports: 16-bit is signed, 8-bit unsigned.
    pub fn signed(&self) -> bool {
        self.bits == 16
    }
}

/// What `audiomixer.Mixer(...)` was created with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerFormat {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub signed: bool,
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Walks the RIFF chunks for `fmt ` and `data`. Works on a file's first few
/// KB too: a `data` chunk longer than what was given is cut to the bytes
/// present. Never panics, whatever the input.
pub fn wav_info(bytes: &[u8]) -> Result<WavFormat, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".to_owned());
    }
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut pos = 12usize;
    while let (Some(id), Some(size)) = (bytes.get(pos..pos + 4), le32(bytes, pos + 4)) {
        let body = pos + 8;
        let size = size as usize;
        if id == b"fmt " {
            let (Some(format), Some(channels), Some(rate), Some(bits)) = (
                le16(bytes, body),
                le16(bytes, body + 2),
                le32(bytes, body + 4),
                le16(bytes, body + 14),
            ) else {
                return Err("truncated fmt chunk".to_owned());
            };
            fmt = Some((format, channels, rate, bits));
        } else if id == b"data" {
            let Some((format, channels, rate, bits)) = fmt else {
                return Err("data chunk before fmt chunk".to_owned());
            };
            if channels == 0 || rate == 0 {
                return Err("fmt chunk has no channels or sample rate".to_owned());
            }
            let end = body.saturating_add(size).min(bytes.len());
            return Ok(WavFormat {
                format,
                channels,
                rate,
                bits,
                data: body.min(end)..end,
            });
        }
        // Chunks are padded to an even length.
        pos = match body.checked_add(size).and_then(|p| p.checked_add(size & 1)) {
            Some(p) => p,
            None => break,
        };
    }
    Err(if fmt.is_some() {
        "no data chunk".to_owned()
    } else {
        "no fmt chunk".to_owned()
    })
}

/// Why the board couldn't play this sound, or None if it can. With the mixer
/// known (code.py has created it), also checks it against the mixer, as
/// `MixerVoice.play` does.
pub fn format_problem(fmt: &WavFormat, mixer: Option<MixerFormat>) -> Option<String> {
    if fmt.format != 1 {
        return Some(format!("not PCM (format {})", fmt.format));
    }
    if fmt.bits != 8 && fmt.bits != 16 {
        return Some(format!("{}-bit; the mixer needs 8 or 16", fmt.bits));
    }
    let mixer = mixer?;
    let mut problems = Vec::new();
    if fmt.rate != mixer.rate {
        problems.push(format!("{} Hz, mixer is {} Hz", fmt.rate, mixer.rate));
    }
    if fmt.channels != mixer.channels {
        problems.push(format!(
            "{} ch, mixer is {} ch",
            fmt.channels, mixer.channels
        ));
    }
    if fmt.bits != mixer.bits {
        problems.push(format!("{}-bit, mixer is {}-bit", fmt.bits, mixer.bits));
    }
    if fmt.signed() != mixer.signed {
        let word = |signed| if signed { "signed" } else { "unsigned" };
        problems.push(format!(
            "{}, mixer is {}",
            word(fmt.signed()),
            word(mixer.signed)
        ));
    }
    (!problems.is_empty()).then(|| problems.join("; "))
}

/// RMS level (0..1) of each 50 ms window of an 8- or 16-bit PCM file, over all
/// channels. Empty for any other format.
pub fn envelope(bytes: &[u8], fmt: &WavFormat) -> Vec<f32> {
    let width = match (fmt.format, fmt.bits) {
        (1, 8) => 1,
        (1, 16) => 2,
        _ => return Vec::new(),
    };
    let Some(data) = bytes.get(fmt.data.clone()) else {
        return Vec::new();
    };
    let per_window = ((fmt.rate as f32 * WINDOW_S) as usize).max(1) * usize::from(fmt.channels);
    let sample = |chunk: &[u8]| -> f32 {
        if width == 1 {
            (f32::from(chunk[0]) - 128.0) / 128.0
        } else {
            f32::from(i16::from_le_bytes([chunk[0], chunk[1]])) / 32768.0
        }
    };
    data.chunks(per_window * width)
        .map(|window| {
            let samples = window.chunks_exact(width);
            let n = samples.len().max(1) as f32;
            let sum: f32 = samples.map(|c| sample(c).powi(2)).sum();
            (sum / n).sqrt().min(1.0)
        })
        .collect()
}

/// Seconds per `envelope` entry.
pub const fn envelope_step_s() -> f32 {
    WINDOW_S
}

/// Every string-literal argument of `play_wav("x.wav"` / `play_wav('x.wav'` in
/// a source file, with its 1-based line. Calls with a variable or expression
/// (`play_wav(random.choice(wavfiles))`) and comments are skipped.
pub fn play_wav_literals(source: &str) -> Vec<(String, usize)> {
    const CALL: &str = "play_wav(";
    let mut found = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let code = line.split('#').next().unwrap_or("");
        let mut rest = code;
        while let Some(at) = rest.find(CALL) {
            rest = &rest[at + CALL.len()..];
            let arg = rest.trim_start();
            let mut chars = arg.chars();
            let Some(quote @ ('"' | '\'')) = chars.next() else {
                continue;
            };
            let name: String = chars.take_while(|&c| c != quote).collect();
            // An unterminated literal has no closing quote to find.
            if arg.len() > name.len() + 1 && !name.is_empty() {
                found.push((name, i + 1));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 44-byte-header file: 2 ch, 22050 Hz, 16-bit, `frames` frames of `fill`.
    fn canonical(frames: usize, fill: i16) -> Vec<u8> {
        let data_len = frames * 4;
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend(((36 + data_len) as u32).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(22050u32.to_le_bytes());
        b.extend((22050u32 * 4).to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend((data_len as u32).to_le_bytes());
        for _ in 0..frames * 2 {
            b.extend(fill.to_le_bytes());
        }
        b
    }

    const MIXER: MixerFormat = MixerFormat {
        rate: 22050,
        channels: 2,
        bits: 16,
        signed: true,
    };

    #[test]
    fn reads_the_canonical_44_byte_header() {
        let info = wav_info(&canonical(22050, 0)).unwrap();
        assert_eq!(
            (info.format, info.channels, info.rate, info.bits),
            (1, 2, 22050, 16)
        );
        assert_eq!(info.data, 44..44 + 22050 * 4);
        assert!((info.duration_s() - 1.0).abs() < 1e-4);
        assert_eq!(format_problem(&info, Some(MIXER)), None);
    }

    #[test]
    fn skips_a_list_chunk_before_data_including_odd_padding() {
        let plain = canonical(100, 0);
        let mut b = plain[..36].to_vec();
        // An odd-sized chunk is followed by one pad byte.
        b.extend(b"LIST");
        b.extend(5u32.to_le_bytes());
        b.extend([1, 2, 3, 4, 5, 0]);
        b.extend(&plain[36..]);
        let info = wav_info(&b).unwrap();
        assert_eq!(info.data.start, 36 + 8 + 6 + 8);
        assert_eq!(info.data.len(), 400);
        assert_eq!((info.rate, info.channels), (22050, 2));
    }

    #[test]
    fn data_longer_than_the_bytes_given_is_cut_short() {
        let b = canonical(1000, 0);
        let info = wav_info(&b[..200]).unwrap();
        assert_eq!(info.data, 44..200);
    }

    #[test]
    fn garbage_and_truncation_are_errors_not_panics() {
        assert!(wav_info(&[]).is_err());
        assert!(wav_info(b"RIFF").is_err());
        assert!(wav_info(b"not a wav file at all").is_err());
        let b = canonical(10, 0);
        for len in 0..b.len() {
            let _ = wav_info(&b[..len]);
        }
        // fmt chunk cut off mid-way.
        assert_eq!(wav_info(&b[..30]), Err("truncated fmt chunk".to_owned()));
        // Chunk size that overflows the position.
        let mut huge = b[..12].to_vec();
        huge.extend(b"junk");
        huge.extend(u32::MAX.to_le_bytes());
        assert!(wav_info(&huge).is_err());
        // Every byte pattern a header could hold must be survivable.
        for fill in [0u8, 0xff, 0x7f] {
            let mut noise = b.clone();
            noise[12..].iter_mut().for_each(|x| *x = fill);
            let _ = wav_info(&noise);
        }
    }

    #[test]
    fn data_before_fmt_is_an_error() {
        let mut b = b"RIFF\0\0\0\0WAVEdata".to_vec();
        b.extend(4u32.to_le_bytes());
        b.extend([0; 4]);
        assert_eq!(wav_info(&b), Err("data chunk before fmt chunk".to_owned()));
    }

    #[test]
    fn mismatches_name_both_values() {
        let mut info = wav_info(&canonical(10, 0)).unwrap();
        info.rate = 44100;
        assert_eq!(
            format_problem(&info, Some(MIXER)).unwrap(),
            "44100 Hz, mixer is 22050 Hz"
        );
        info.channels = 1;
        info.bits = 8;
        let msg = format_problem(&info, Some(MIXER)).unwrap();
        assert!(msg.contains("1 ch, mixer is 2 ch"), "{msg}");
        assert!(msg.contains("8-bit, mixer is 16-bit"), "{msg}");
        assert!(msg.contains("unsigned, mixer is signed"), "{msg}");
        // No mixer yet: only what's wrong with the file itself.
        assert_eq!(format_problem(&info, None), None);
    }

    #[test]
    fn non_pcm_and_odd_bit_depths_are_problems() {
        let mut info = wav_info(&canonical(10, 0)).unwrap();
        info.format = 3;
        assert_eq!(format_problem(&info, None).unwrap(), "not PCM (format 3)");
        info.format = 1;
        info.bits = 24;
        assert!(format_problem(&info, None).unwrap().starts_with("24-bit"));
    }

    #[test]
    fn envelope_follows_the_signal() {
        let loud = canonical(2204, 16384);
        let info = wav_info(&loud).unwrap();
        let env = envelope(&loud, &info);
        // 2204 frames = two whole 1102-frame (50 ms) windows.
        assert_eq!(env.len(), 2);
        assert!((env[0] - 0.5).abs() < 1e-3);
        let silent = canonical(2204, 0);
        let info = wav_info(&silent).unwrap();
        assert!(envelope(&silent, &info).iter().all(|&l| l == 0.0));
    }

    #[test]
    fn envelope_is_empty_for_formats_it_cannot_read() {
        let b = canonical(100, 1000);
        let mut info = wav_info(&b).unwrap();
        info.bits = 24;
        assert!(envelope(&b, &info).is_empty());
        info.bits = 16;
        info.data = 0..10_000;
        assert!(envelope(&b, &info).is_empty());
    }

    #[test]
    fn finds_literal_play_wav_calls_only() {
        let src = "\
def play_wav(filename, loop=False):
    pass

play_wav(\"a.wav\")
    play_wav( 'b.wav', loop=True)
play_wav(random.choice(wavfiles))
# play_wav(\"commented.wav\")
x = 1  # play_wav(\"trailing.wav\")
play_wav(f\"{name}.wav\")
play_wav(\"unterminated.wav
foo(play_wav(\"c.wav\"), play_wav(\"d.wav\"))
";
        assert_eq!(
            play_wav_literals(src),
            vec![
                ("a.wav".to_owned(), 4),
                ("b.wav".to_owned(), 5),
                ("c.wav".to_owned(), 11),
                ("d.wav".to_owned(), 11),
            ]
        );
    }
}
