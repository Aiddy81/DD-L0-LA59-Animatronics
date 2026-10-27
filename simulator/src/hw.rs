//! Simulated hardware state shared between the Python thread (which writes it
//! through the `simhw` module) and the Bevy app (which renders it).

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Instant;

use crate::config::BoardCfg;

/// PCA9685 runs the servo FeatherWing at 50 Hz, so one period is 20 ms.
pub const PWM_PERIOD_US: f32 = 20_000.0;
const LOG_CAP: usize = 4000;
/// Clock speed while skipping to the next behaviour: the longest idle wait
/// (120 min) passes in about 7 s.
const SKIP_SPEED: f64 = 1000.0;
/// Virtual seconds between two loop-hold warnings for the same task, so a
/// task that blocks every pass doesn't flood the log.
const HOLD_WARN_EVERY_S: f64 = 5.0;

pub enum AudioCmd {
    Play {
        path: String,
        level: f32,
        looped: bool,
    },
    Stop,
    Volume(f32),
}

/// What a plot marker shows: an input or event that can explain what follows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerKind {
    Motion,
    Sound,
    Idle,
    Restart,
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

/// Progress of a scripted scenario, as sim_runtime.py reports it.
#[derive(Clone)]
pub struct ScenarioStatus {
    pub name: String,
    /// "running", "pass", "fail" or "error".
    pub state: String,
    /// Each expectation's description and "pending", "pass" or "fail".
    pub results: Vec<(String, String)>,
}

/// What code.py's behaviours are doing, read by sim_runtime.py from
/// hardware.py and the running coroutines. Deadlines are in virtual seconds.
#[derive(Clone, Default)]
pub struct FwState {
    /// Each `asyncio.Event` in hardware.py and whether it is set.
    pub events: Vec<(String, bool)>,
    pub tasks: Vec<TaskState>,
    pub angry: Option<AngryWindow>,
    /// `None` while idle_attention isn't waiting (its animation is running).
    pub idle: Option<IdleWait>,
}

#[derive(Clone)]
pub struct TaskState {
    /// The await chain, e.g. "motion_sense → nose_boops → sleep".
    pub chain: String,
    /// Every number variable in the chain's firmware functions, as (name, value).
    pub numbers: Vec<(String, String)>,
}

#[derive(Clone, Copy)]
pub struct AngryWindow {
    pub count: u32,
    pub needed: u32,
    pub deadline: f64,
    pub length: f64,
}

#[derive(Clone, Copy)]
pub struct IdleWait {
    pub deadline: f64,
    pub length: f64,
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
    /// Unit direction that Nudge, Hard shake and a head click push in, added to
    /// `accel_g`.
    pub nudge_dir: [f32; 3],
    /// One-shot readings returned before falling back to `accel_g`.
    pub accel_pulses: VecDeque<[f32; 3]>,
    /// The latest reading the firmware got, after the sensor's range clipping.
    pub last_accel: [f32; 3],
    /// Full-scale range of the LIS3DH in g, reported by the shim.
    pub accel_range_g: f32,
    pub log: VecDeque<LogLine>,
    partial_line: String,
    /// Between a "Traceback" header and its exception message, so the whole
    /// traceback can be coloured as an error.
    in_traceback: bool,
    /// Audio commands from `code.py`, drained by the audio system.
    pub audio: Vec<AudioCmd>,
    pub clock: SimClock,
    /// Running at `SKIP_SPEED` until a behaviour starts.
    pub skipping: bool,
    pub stop_requested: bool,
    pub idle_trigger: bool,
    /// Plot markers not yet picked up by the plot, which timestamps them.
    pub markers: Vec<(MarkerKind, String)>,
    /// Behaviours code.py can be asked to run once, found by sim_runtime.py.
    pub behaviours: Vec<String>,
    /// A behaviour button click, waiting for code.py's event loop.
    pub behaviour_request: Option<String>,
    /// The button-started behaviour currently running, if any.
    pub behaviour_running: Option<String>,
    /// The scenario this run is playing, if any.
    pub scenario: Option<ScenarioStatus>,
    pub fw_state: FwState,
    /// Hold code.py to what CircuitPython 9 offers (imports, APIs, float precision).
    pub strict: bool,
    /// Make hardware calls take as long as on the robot.
    pub board_speed: bool,
    /// Turn the 3D wings no faster than the real servo (`[servo]` in sim.toml).
    pub servo_speed: bool,
    pub board: BoardCfg,
    /// Board time owed but not yet slept, in virtual seconds.
    debt_s: f64,
    /// All board time charged this run, in virtual seconds.
    pub board_time_s: f64,
    /// The longest a task step held the event loop this run: (task, ms).
    pub longest_hold: Option<(String, f64)>,
    /// Virtual time of each task's last loop-hold warning.
    hold_warned: HashMap<String, f64>,
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
            nudge_dir: [0.0, 0.0, 1.0],
            accel_pulses: VecDeque::new(),
            last_accel: [0.0, 0.0, 1.0],
            accel_range_g: 4.0,
            log: VecDeque::new(),
            partial_line: String::new(),
            in_traceback: false,
            audio: Vec::new(),
            clock: SimClock::new(),
            skipping: false,
            stop_requested: false,
            idle_trigger: false,
            markers: Vec::new(),
            behaviours: Vec::new(),
            behaviour_request: None,
            behaviour_running: None,
            scenario: None,
            fw_state: FwState::default(),
            strict: true,
            board_speed: true,
            servo_speed: true,
            board: BoardCfg::default(),
            debt_s: 0.0,
            board_time_s: 0.0,
            longest_hold: None,
            hold_warned: HashMap::new(),
            heap_bytes: 0.0,
            heap_peak: 0.0,
            heap_warned: 0,
            unsimulated: Vec::new(),
        }
    }

    /// Clears everything a fresh run of `code.py` should not inherit. The held
    /// accelerometer value, its nudge direction and clock speed are user settings, so they survive.
    /// A skip is not: the new run starts at 1x.
    pub fn reset_for_run(&mut self) {
        let accel = self.accel_g;
        let nudge_dir = self.nudge_dir;
        let speed = if self.skipping { 1.0 } else { self.clock.speed };
        let strict = self.strict;
        let board_speed = self.board_speed;
        let servo_speed = self.servo_speed;
        let board = self.board.clone();
        let log = std::mem::take(&mut self.log);
        *self = Self::new();
        self.accel_g = accel;
        self.nudge_dir = nudge_dir;
        self.clock.speed = speed;
        self.strict = strict;
        self.board_speed = board_speed;
        self.servo_speed = servo_speed;
        self.board = board;
        self.log = log;
    }

    /// Fast-forwards until a behaviour starts (see `end_skip`).
    pub fn start_skip(&mut self) {
        self.skipping = true;
        self.clock.set_paused(false);
        self.clock.set_speed(SKIP_SPEED);
        self.push_line("[sim] skipping to the next behaviour".to_owned());
    }

    /// Drops back to 1x if a skip is running, logging what ended it.
    pub fn end_skip(&mut self, reason: &str) {
        if !self.skipping {
            return;
        }
        self.skipping = false;
        self.clock.set_speed(1.0);
        self.push_line(format!("[sim] skip stopped: {reason}"));
    }

    pub fn mark(&mut self, kind: MarkerKind, label: impl Into<String>) {
        self.markers.push((kind, label.into()));
    }

    /// Ends idle_attention's wait (from the toolbar, the I key or a scenario).
    pub fn trigger_idle(&mut self) {
        self.idle_trigger = true;
        self.mark(MarkerKind::Idle, "idle");
    }

    /// Sets the clock speed, ending any skip.
    pub fn set_speed(&mut self, speed: f64) {
        self.skipping = false;
        self.clock.set_speed(speed);
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
        self.board_time_s += ms / 1000.0;
        let real = self.debt_s / self.clock.speed;
        if real < 0.001 {
            return None;
        }
        self.debt_s = 0.0;
        Some(real)
    }

    /// Records how long one task step (the code between two awaits) held the
    /// event loop. Over the board's limit it logs a warning, at most once per
    /// task every `HOLD_WARN_EVERY_S`.
    pub fn task_hold(&mut self, name: &str, secs: f64) {
        let ms = secs * 1000.0;
        if self.longest_hold.as_ref().is_none_or(|(_, m)| ms > *m) {
            self.longest_hold = Some((name.to_owned(), ms));
        }
        let limit = self.board.loop_hold_warn_ms;
        if limit <= 0.0 || ms <= limit {
            return;
        }
        let now = self.clock.now();
        let due = self
            .hold_warned
            .get(name)
            .is_none_or(|&t| now - t >= HOLD_WARN_EVERY_S);
        if due {
            self.hold_warned.insert(name.to_owned(), now);
            self.push_line(format!(
                "[sim] {name} held the loop for {ms:.0} ms without an await (limit {limit:.0} ms): every other behaviour stalled meanwhile"
            ));
        }
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
        let reading = self.accel_pulses.pop_front().unwrap_or(self.accel_g);
        // Same saturation as the shim (one LSB below full scale), so the plot
        // shows what the firmware actually read.
        let limit = self.accel_range_g * (1.0 - 1.0 / 32768.0);
        self.last_accel = reading.map(|v| v.clamp(-limit, limit));
        reading
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
    /// End of a single step. The clock runs up to it and then reads as paused.
    stop_at: Option<f64>,
}

impl SimClock {
    fn new() -> Self {
        Self {
            anchor_real: Instant::now(),
            anchor_virtual: 0.0,
            speed: 1.0,
            paused: false,
            stop_at: None,
        }
    }

    fn running_time(&self) -> f64 {
        self.anchor_virtual + self.anchor_real.elapsed().as_secs_f64() * self.speed
    }

    pub fn now(&self) -> f64 {
        if self.paused {
            return self.anchor_virtual;
        }
        let t = self.running_time();
        self.stop_at.map_or(t, |stop| t.min(stop))
    }

    pub fn is_paused(&self) -> bool {
        self.paused || self.stop_at.is_some_and(|stop| self.running_time() >= stop)
    }

    pub fn set_paused(&mut self, paused: bool) {
        // Re-anchor before flipping so virtual time neither jumps nor counts
        // the paused interval.
        self.anchor_virtual = self.now();
        self.anchor_real = Instant::now();
        self.paused = paused;
        self.stop_at = None;
    }

    /// Runs the clock for `dt` virtual seconds, then stops it again. Running
    /// rather than jumping lets every animation frame inside the step happen.
    pub fn step(&mut self, dt: f64) {
        let start = self.now();
        self.anchor_virtual = start;
        self.anchor_real = Instant::now();
        self.paused = false;
        self.stop_at = Some(start + dt);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Waits (briefly, in real time) until the clock reads as paused.
    fn wait_until_paused(clock: &SimClock) {
        let start = Instant::now();
        while !clock.is_paused() {
            assert!(start.elapsed().as_secs() < 5, "step never finished");
            std::thread::yield_now();
        }
    }

    #[test]
    fn step_runs_exactly_dt_then_pauses() {
        let mut clock = SimClock::new();
        clock.set_speed(1000.0);
        clock.set_paused(true);
        let start = clock.now();
        clock.step(0.1);
        wait_until_paused(&clock);
        assert_eq!(clock.now(), start + 0.1);
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_eq!(clock.now(), start + 0.1, "time moved after the step");
    }

    #[test]
    fn resume_after_step_runs_freely() {
        let mut clock = SimClock::new();
        clock.set_speed(1000.0);
        clock.set_paused(true);
        clock.step(0.01);
        wait_until_paused(&clock);
        let stepped = clock.now();
        clock.set_paused(false);
        assert!(!clock.is_paused());
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(
            clock.now() > stepped + 0.5,
            "clock still held at the step's end"
        );
    }

    #[test]
    fn speed_change_keeps_time_continuous() {
        let mut clock = SimClock::new();
        clock.set_paused(true);
        let before = clock.now();
        clock.set_speed(100.0);
        assert_eq!(clock.now(), before);
    }

    #[test]
    fn skip_ends_on_first_reason_only() {
        let mut s = HwState::new();
        s.start_skip();
        assert_eq!(s.clock.speed, SKIP_SPEED);
        s.end_skip("youWho.wav started");
        s.end_skip("pause_breathing set");
        assert!(!s.skipping);
        assert_eq!(s.clock.speed, 1.0);
        let stops = s
            .log
            .iter()
            .filter(|l| l.text.starts_with("[sim] skip stopped"))
            .count();
        assert_eq!(stops, 1);
    }

    #[test]
    fn restart_during_skip_comes_back_at_1x() {
        let mut s = HwState::new();
        s.set_speed(10.0);
        s.reset_for_run();
        assert_eq!(s.clock.speed, 10.0);
        s.start_skip();
        s.reset_for_run();
        assert_eq!(s.clock.speed, 1.0);
        assert!(!s.skipping);
    }
}
