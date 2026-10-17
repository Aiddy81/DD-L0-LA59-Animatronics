//! Simulated hardware state shared between the Python thread (which writes it
//! through the `simhw` module) and the Bevy app (which renders it).

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Instant;

/// PCA9685 runs the servo FeatherWing at 50 Hz, so one period is 20 ms.
pub const PWM_PERIOD_US: f32 = 20_000.0;
const LOG_CAP: usize = 4000;

pub enum AudioCmd {
    Play {
        path: String,
        level: f32,
        looped: bool,
    },
    Stop,
    Volume(f32),
}

pub struct HwState {
    /// 16-bit duty cycle per PCA9685 channel, `None` until first written.
    pub pca_duty: [Option<u16>; 16],
    /// White LED (`pwmio.PWMOut` on EXTERNAL_NEOPIXELS), 16-bit duty.
    pub white_duty: u16,
    pub onboard_led: bool,
    pub external_power: bool,
    /// Held acceleration in g (x, y, z). 1 g on z = sitting still.
    pub accel_g: [f32; 3],
    /// One-shot readings returned before falling back to `accel_g`.
    pub accel_pulses: VecDeque<[f32; 3]>,
    pub log: VecDeque<String>,
    partial_line: String,
    /// Audio commands from `code.py`, drained by the audio system.
    pub audio: Vec<AudioCmd>,
    pub clock: SimClock,
    pub stop_requested: bool,
    pub idle_trigger: bool,
}

impl HwState {
    fn new() -> Self {
        Self {
            pca_duty: [None; 16],
            white_duty: 0,
            onboard_led: false,
            external_power: false,
            accel_g: [0.0, 0.0, 1.0],
            accel_pulses: VecDeque::new(),
            log: VecDeque::new(),
            partial_line: String::new(),
            audio: Vec::new(),
            clock: SimClock::new(),
            stop_requested: false,
            idle_trigger: false,
        }
    }

    /// Clears everything a fresh run of `code.py` should not inherit. The held
    /// accelerometer value and clock speed are user settings, so they survive.
    pub fn reset_for_run(&mut self) {
        let accel = self.accel_g;
        let speed = self.clock.speed;
        let log = std::mem::take(&mut self.log);
        *self = Self::new();
        self.accel_g = accel;
        self.clock.speed = speed;
        self.log = log;
    }

    pub fn duty_fraction(&self, channel: usize) -> Option<f32> {
        self.pca_duty[channel].map(|d| d as f32 / 65535.0)
    }

    pub fn pulse_us(&self, channel: usize) -> Option<f32> {
        self.duty_fraction(channel).map(|f| f * PWM_PERIOD_US)
    }

    pub fn write_log(&mut self, text: &str) {
        self.partial_line.push_str(text);
        while let Some(pos) = self.partial_line.find('\n') {
            let line: String = self.partial_line.drain(..=pos).collect();
            self.push_line(line.trim_end_matches(['\r', '\n']).to_string());
        }
    }

    pub fn push_line(&mut self, line: String) {
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    pub fn next_accel(&mut self) -> [f32; 3] {
        self.accel_pulses.pop_front().unwrap_or(self.accel_g)
    }
}

/// Virtual time for `code.py`. Runs at `speed` x real time so the 15-120
/// minute idle timer can be watched without waiting for it.
pub struct SimClock {
    anchor_real: Instant,
    anchor_virtual: f64,
    pub speed: f64,
}

impl SimClock {
    fn new() -> Self {
        Self {
            anchor_real: Instant::now(),
            anchor_virtual: 0.0,
            speed: 1.0,
        }
    }

    pub fn now(&self) -> f64 {
        self.anchor_virtual + self.anchor_real.elapsed().as_secs_f64() * self.speed
    }

    pub fn set_speed(&mut self, speed: f64) {
        // Re-anchor so changing speed never makes virtual time jump.
        self.anchor_virtual = self.now();
        self.anchor_real = Instant::now();
        self.speed = speed.max(0.01);
    }
}

static HW: LazyLock<Mutex<HwState>> = LazyLock::new(|| Mutex::new(HwState::new()));

/// Locks the shared state. A panic while holding the lock must not take the
/// whole simulator down, so poisoning is ignored.
pub fn hw() -> MutexGuard<'static, HwState> {
    HW.lock().unwrap_or_else(|e| e.into_inner())
}
