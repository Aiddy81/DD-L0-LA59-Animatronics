//! Simulated hardware state shared between the Python thread (which writes it
//! through the `simhw` module) and the Bevy app (which renders it).

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Instant;

use crate::config::BoardCfg;

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Firmware,
    Sim,
    Error,
}

pub struct LogLine {
    pub kind: LogKind,
    pub text: String,
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
    pub log: VecDeque<LogLine>,
    partial_line: String,
    /// Between a "Traceback" header and its exception message, so the whole
    /// traceback can be coloured as an error.
    in_traceback: bool,
    /// Audio commands from `code.py`, drained by the audio system.
    pub audio: Vec<AudioCmd>,
    pub clock: SimClock,
    pub stop_requested: bool,
    pub idle_trigger: bool,
    /// Behaviours code.py can be asked to run once, found by sim_runtime.py.
    pub behaviours: Vec<String>,
    /// A behaviour button click, waiting for code.py's event loop.
    pub behaviour_request: Option<String>,
    /// The button-started behaviour currently running, if any.
    pub behaviour_running: Option<String>,
    /// Hold code.py to what CircuitPython 9 offers (imports, APIs, float precision).
    pub strict: bool,
    /// Make hardware calls take as long as on the robot.
    pub board_speed: bool,
    pub board: BoardCfg,
    /// Board time owed but not yet slept, in virtual seconds.
    debt_s: f64,
    /// Estimated CircuitPython heap used by code.py, in bytes.
    pub heap_bytes: f64,
    pub heap_peak: f64,
    heap_warned: u8,
    /// Libraries code.py imported that the simulator only stubs out.
    pub unsimulated: Vec<String>,
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
            in_traceback: false,
            audio: Vec::new(),
            clock: SimClock::new(),
            stop_requested: false,
            idle_trigger: false,
            behaviours: Vec::new(),
            behaviour_request: None,
            behaviour_running: None,
            strict: true,
            board_speed: true,
            board: BoardCfg::default(),
            debt_s: 0.0,
            heap_bytes: 0.0,
            heap_peak: 0.0,
            heap_warned: 0,
            unsimulated: Vec::new(),
        }
    }

    /// Clears everything a fresh run of `code.py` should not inherit. The held
    /// accelerometer value and clock speed are user settings, so they survive.
    pub fn reset_for_run(&mut self) {
        let accel = self.accel_g;
        let speed = self.clock.speed;
        let strict = self.strict;
        let board_speed = self.board_speed;
        let board = self.board.clone();
        let log = std::mem::take(&mut self.log);
        *self = Self::new();
        self.accel_g = accel;
        self.clock.speed = speed;
        self.strict = strict;
        self.board_speed = board_speed;
        self.board = board;
        self.log = log;
    }

    /// Books the board's cost for one hardware call. Returns real seconds to
    /// sleep now; small costs build up until they're worth a sleep.
    pub fn charge(&mut self, kind: &str) -> Option<f64> {
        if !self.board_speed {
            return None;
        }
        let ms = match kind {
            "pca" => self.board.pca_write_ms,
            "pwm" => self.board.pwm_write_ms,
            "accel" => self.board.accel_read_ms,
            "loop" => self.board.loop_ms,
            _ => return None,
        };
        self.debt_s += ms / 1000.0;
        let real = self.debt_s / self.clock.speed;
        if real < 0.001 {
            return None;
        }
        self.debt_s = 0.0;
        Some(real)
    }

    pub fn heap_budget(&self) -> f64 {
        self.board.heap_budget_kb * 1024.0
    }

    /// Records a new heap estimate and warns once each at 80% and 100%.
    pub fn set_heap(&mut self, desktop_bytes: f64) {
        self.heap_bytes = desktop_bytes * self.board.heap_scale;
        self.heap_peak = self.heap_peak.max(self.heap_bytes);
        let used = self.heap_bytes / self.heap_budget();
        let level = if used >= 1.0 {
            2
        } else if used >= 0.8 {
            1
        } else {
            0
        };
        if level > self.heap_warned {
            self.heap_warned = level;
            let kb = self.heap_bytes / 1024.0;
            let budget = self.board.heap_budget_kb;
            self.push_line(if level == 2 {
                format!("[sim] estimated heap {kb:.0} KB exceeds the board's {budget:.0} KB: likely MemoryError on the robot")
            } else {
                format!("[sim] estimated heap {kb:.0} KB is over 80% of the board's {budget:.0} KB")
            });
        }
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
        let kind = if line.starts_with("[sim]") {
            if line.to_lowercase().contains("error") {
                LogKind::Error
            } else {
                LogKind::Sim
            }
        } else if line.starts_with("Traceback (most recent call last)") {
            self.in_traceback = true;
            LogKind::Error
        } else if self.in_traceback {
            if line.is_empty() {
                self.in_traceback = false;
                LogKind::Firmware
            } else {
                // The first unindented line is the exception message, which ends it.
                self.in_traceback = line.starts_with(char::is_whitespace);
                LogKind::Error
            }
        } else {
            LogKind::Firmware
        };
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(LogLine { kind, text: line });
    }

    pub fn clear_log(&mut self) {
        self.log.clear();
        self.in_traceback = false;
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
    /// While set, `now()` stands still. `reset_for_run` builds a fresh clock
    /// (keeping only the speed), so restarting code.py also unpauses.
    paused: bool,
}

impl SimClock {
    fn new() -> Self {
        Self {
            anchor_real: Instant::now(),
            anchor_virtual: 0.0,
            speed: 1.0,
            paused: false,
        }
    }

    pub fn now(&self) -> f64 {
        if self.paused {
            return self.anchor_virtual;
        }
        self.anchor_virtual + self.anchor_real.elapsed().as_secs_f64() * self.speed
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        // Re-anchor before flipping so virtual time neither jumps nor counts
        // the paused interval.
        self.anchor_virtual = self.now();
        self.anchor_real = Instant::now();
        self.paused = paused;
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
