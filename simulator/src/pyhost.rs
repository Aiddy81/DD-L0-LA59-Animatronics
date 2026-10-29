//! Runs `code.py` on an embedded CPython interpreter, on its own thread.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyModule, PyString};

use crate::config::PartCfg;
use crate::hw::{
    hw, AngryWindow, AudioCmd, FwState, IdleWait, MarkerKind, PinValue, ScenarioStatus, TaskState,
};
use crate::parts::ShimValue;
use crate::wav::MixerFormat;

/// Hardware stand-ins, compiled into the exe so it runs on its own. Shims in
/// `dist\shims\` override these or add new ones; sim_runtime.py reads that
/// folder at the start of every run, so no rebuild is needed.
/// (module name, source, is_package)
const SHIMS: &[(&str, &str, bool)] = &[
    ("board", include_str!("../shims/board.py"), false),
    ("digitalio", include_str!("../shims/digitalio.py"), false),
    ("busio", include_str!("../shims/busio.py"), false),
    ("pwmio", include_str!("../shims/pwmio.py"), false),
    ("audiocore", include_str!("../shims/audiocore.py"), false),
    ("audiomixer", include_str!("../shims/audiomixer.py"), false),
    ("audiobusio", include_str!("../shims/audiobusio.py"), false),
    (
        "adafruit_motor",
        include_str!("../shims/adafruit_motor/__init__.py"),
        true,
    ),
    (
        "adafruit_motor.servo",
        include_str!("../shims/adafruit_motor/servo.py"),
        false,
    ),
    (
        "adafruit_servokit",
        include_str!("../shims/adafruit_servokit.py"),
        false,
    ),
    (
        "adafruit_lis3dh",
        include_str!("../shims/adafruit_lis3dh.py"),
        false,
    ),
];
const RUNTIME: &str = include_str!("../shims/sim_runtime.py");

/// The `simhw` module: the only door from Python into the simulator.
#[pymodule]
mod simhw {
    use super::*;

    #[pyfunction]
    fn set_pca(channel: usize, duty: u16) {
        if channel < 16 {
            hw().pca_duty[channel] = Some(duty);
        }
    }

    #[pyfunction]
    #[pyo3(signature = (pin, duty, frequency = 500))]
    fn set_pwm_pin(pin: &str, duty: u16, frequency: u32) {
        let mut s = hw();
        if pin == "EXTERNAL_NEOPIXELS" {
            s.white_duty = duty;
        }
        s.pins.insert(
            pin.to_owned(),
            PinValue::Pwm {
                duty,
                freq: frequency,
            },
        );
    }

    #[pyfunction]
    fn set_pin(pin: &str, value: bool) {
        let mut s = hw();
        match pin {
            "LED" => s.onboard_led = value,
            "EXTERNAL_POWER" => s.external_power = value,
            _ => {}
        }
        s.pins.insert(pin.to_owned(), PinValue::Digital(value));
    }

    /// Whether the button part wired to this input pin is pressed; `None`
    /// when no button is on it.
    #[pyfunction]
    fn pin_input(pin: &str) -> Option<bool> {
        hw().button_pins.get(pin).copied()
    }

    /// Drives a part left to shims in parts.toml: brightness 0-1 or on/off
    /// for an `led` or `pixels` part, `(r, g, b)` 0-255 for an `led`, degrees
    /// for a `servo_part`. Anything else logs one warning per run and name.
    #[pyfunction]
    fn set_part(py: Python<'_>, name: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) {
        let Some(name) = part_name(name, "set_part") else {
            return;
        };
        wait_for_parts(py);
        crate::parts::shim_set_part(&mut hw(), &name, shim_value(value));
    }

    /// Sets a `pixels` part's colours, one `(r, g, b)` 0-255 per pixel, as
    /// `neopixel` holds them after its brightness is applied.
    #[pyfunction]
    fn set_pixels(py: Python<'_>, name: &Bound<'_, PyAny>, pixels: &Bound<'_, PyAny>) {
        let Some(name) = part_name(name, "set_pixels") else {
            return;
        };
        let pixels = pixel_list(pixels);
        wait_for_parts(py);
        crate::parts::shim_set_pixels(&mut hw(), &name, pixels);
    }

    /// Whether a `button` part is pressed; `False` for anything else.
    #[pyfunction]
    fn part_input(py: Python<'_>, name: &Bound<'_, PyAny>) -> bool {
        let Some(name) = part_name(name, "part_input") else {
            return false;
        };
        wait_for_parts(py);
        crate::parts::shim_part_input(&mut hw(), &name)
    }

    #[pyfunction]
    fn accel() -> (f32, f32, f32) {
        let [x, y, z] = hw().next_accel();
        (x, y, z)
    }

    /// The sensor's full-scale range in g, so the plot can draw its clip level.
    #[pyfunction]
    fn accel_range(g: f32) {
        hw().accel_range_g = g;
    }

    #[pyfunction]
    fn log(text: &str) {
        hw().write_log(text);
    }

    #[pyfunction]
    fn play_wav(path: String, level: f32, looped: bool) {
        let mut s = hw();
        // Every behaviour starts with a sound, so this is what ends a skip.
        let name = std::path::Path::new(&path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        s.end_skip(&format!("{name} started"));
        s.mark(MarkerKind::Sound, name);
        s.audio.push(AudioCmd::Play {
            path,
            level,
            looped,
        });
    }

    #[pyfunction]
    fn mixer_format(rate: u32, channels: u16, bits: u16, signed: bool) {
        hw().mixer = Some(MixerFormat {
            rate,
            channels,
            bits,
            signed,
        });
    }

    #[pyfunction]
    fn stop_audio() {
        hw().audio.push(AudioCmd::Stop);
    }

    #[pyfunction]
    fn set_volume(level: f32) {
        hw().audio.push(AudioCmd::Volume(level));
    }

    #[pyfunction]
    fn now() -> f64 {
        hw().clock.now()
    }

    #[pyfunction]
    fn speed() -> f64 {
        hw().clock.speed
    }

    #[pyfunction]
    fn paused() -> bool {
        hw().clock.is_paused()
    }

    #[pyfunction]
    fn stopping() -> bool {
        hw().stop_requested
    }

    #[pyfunction]
    fn charge(kind: &str) {
        let sleep = hw().charge(kind);
        if let Some(secs) = sleep {
            std::thread::sleep(std::time::Duration::from_secs_f64(secs));
        }
    }

    /// Board time charged so far (virtual seconds), so a task step can tell
    /// its board-speed sleeps apart from desktop CPU time.
    #[pyfunction]
    fn board_time() -> f64 {
        hw().board_time_s
    }

    #[pyfunction]
    fn task_hold(name: &str, secs: f64) {
        hw().task_hold(name, secs);
    }

    #[pyfunction]
    fn set_heap(desktop_bytes: f64) {
        hw().set_heap(desktop_bytes);
    }

    /// (estimated CircuitPython bytes used, board budget in bytes)
    #[pyfunction]
    fn heap() -> (f64, f64) {
        let s = hw();
        (s.heap_bytes, s.heap_budget())
    }

    #[pyfunction]
    fn unsimulated(name: String) {
        let mut s = hw();
        if !s.unsimulated.contains(&name) {
            s.push_line(format!(
                "[sim] {name} is not simulated; calls to it do nothing"
            ));
            s.unsimulated.push(name);
        }
    }

    #[pyfunction]
    fn strict() -> bool {
        hw().strict
    }

    #[pyfunction]
    fn take_idle_trigger() -> bool {
        std::mem::take(&mut hw().idle_trigger)
    }

    #[pyfunction]
    fn set_behaviours(names: Vec<String>) {
        hw().behaviours = names;
    }

    #[pyfunction]
    fn take_behaviour_request() -> Option<String> {
        hw().behaviour_request.take()
    }

    /// An empty name means nothing button-started is running.
    #[pyfunction]
    fn set_behaviour_running(name: &str) {
        hw().behaviour_running = (!name.is_empty()).then(|| name.to_owned());
    }

    /// A State panel snapshot: each task is (await chain, [(name, value)]),
    /// `angry` is (count, needed, deadline, length), `idle` is (deadline,
    /// length), each `None` when it can't be read.
    #[pyfunction]
    fn set_state(
        events: Vec<(String, bool)>,
        tasks: Vec<(String, Vec<(String, String)>)>,
        angry: Option<(u32, u32, f64, f64)>,
        idle: Option<(f64, f64)>,
    ) {
        let mut s = hw();
        // A behaviour also shows as a hardware.py event flipping (e.g.
        // pause_breathing). Snapshots are 0.1 s apart in real time, so a short
        // flip can be missed at skip speed; the sound in play_wav is the sure sign.
        let flipped = events.iter().find(|(name, set)| {
            s.fw_state
                .events
                .iter()
                .any(|(old, was)| old == name && was != set)
        });
        if let Some((name, set)) = flipped {
            let change = if *set { "set" } else { "cleared" };
            s.end_skip(&format!("{name} {change}"));
        }
        s.fw_state = FwState {
            events,
            tasks: tasks
                .into_iter()
                .map(|(chain, numbers)| TaskState { chain, numbers })
                .collect(),
            angry: angry.map(|(count, needed, deadline, length)| AngryWindow {
                count,
                needed,
                deadline,
                length,
            }),
            idle: idle.map(|(deadline, length)| IdleWait { deadline, length }),
        };
    }

    /// Scenario steps: the same inputs as the Motion panel and toolbar. The
    /// vector is the direction the reading is pushed in, from the held value.
    #[pyfunction]
    fn nudge(x: f32, y: f32, z: f32) {
        crate::ui::nudge([x, y, z]);
    }

    #[pyfunction]
    fn shake(x: f32, y: f32, z: f32) {
        crate::ui::shake([x, y, z]);
    }

    #[pyfunction]
    fn set_accel(x: f32, y: f32, z: f32) {
        hw().accel_g = [x, y, z];
    }

    #[pyfunction]
    fn trigger_idle() {
        hw().trigger_idle();
    }

    #[pyfunction]
    fn set_scenario(name: String, state: String, results: Vec<(String, String)>) {
        hw().scenario = Some(ScenarioStatus {
            name,
            state,
            results,
        });
    }
}

/// A part call's name argument. A name longer than any part's is cut, so it
/// can't grow the warning set; a non-string warns and gives `None`.
fn part_name(name: &Bound<'_, PyAny>, call: &str) -> Option<String> {
    match name.downcast::<PyString>() {
        Ok(text) => Some(
            text.to_string_lossy()
                .chars()
                .take(PartCfg::MAX_NAME + 1)
                .collect(),
        ),
        Err(_) => {
            crate::parts::shim_warn(&mut hw(), call, "", "the name must be a string");
            None
        }
    }
}

/// parts.rs fills in the parts for a run on the next frame after it starts,
/// which code.py can beat. Waits for that (once a run, about a frame), with
/// the GIL released.
fn wait_for_parts(py: Python<'_>) {
    py.allow_threads(|| {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            {
                let s = hw();
                if s.part_kinds.is_some() || s.stop_requested {
                    return;
                }
            }
            if Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
}

/// Three numbers, from any sequence of exactly three (not a string).
fn rgb(item: &Bound<'_, PyAny>) -> Option<[f64; 3]> {
    if item.is_instance_of::<PyString>() || item.len().ok()? != 3 {
        return None;
    }
    let values: Vec<f64> = item.extract().ok()?;
    values.try_into().ok()
}

/// A `set_part` value: a bool (checked first, as Python's `True` is also an
/// int), a number, or `(r, g, b)`.
fn shim_value(value: &Bound<'_, PyAny>) -> Result<ShimValue, &'static str> {
    if let Ok(on) = value.downcast::<PyBool>() {
        return Ok(ShimValue::On(on.is_true()));
    }
    if let Ok(x) = value.extract::<f64>() {
        return Ok(ShimValue::Number(x));
    }
    rgb(value)
        .map(ShimValue::Rgb)
        .ok_or("the value must be a number, True/False or (r, g, b)")
}

/// A `set_pixels` list, read no further than one past the limit, so a huge
/// or endless iterable costs nothing.
fn pixel_list(pixels: &Bound<'_, PyAny>) -> Result<Vec<[f64; 3]>, String> {
    let too_many = || format!("more than {} pixels", PartCfg::MAX_PIXELS);
    let not_a_list = || "pixels must be a list of (r, g, b)".to_owned();
    if pixels.is_instance_of::<PyString>() {
        return Err(not_a_list());
    }
    if pixels.len().is_ok_and(|n| n > PartCfg::MAX_PIXELS) {
        return Err(too_many());
    }
    let mut list = Vec::new();
    for item in pixels.try_iter().map_err(|_| not_a_list())? {
        if list.len() == PartCfg::MAX_PIXELS {
            return Err(too_many());
        }
        let item = item.map_err(|_| not_a_list())?;
        list.push(rgb(&item).ok_or_else(|| "each pixel must be (r, g, b)".to_owned())?);
    }
    Ok(list)
}

/// A scenario to play against a run: its name, TOML text, and the servo and
/// channel map from sim.toml that turns PCA9685 writes into wing angles and
/// LED levels.
pub struct ScenarioRun {
    pub name: String,
    pub text: String,
    pub servo: (f32, f32, f32),
    pub channels: Vec<(String, usize)>,
}

/// Must run before anything touches Python.
pub fn init_python() {
    pyo3::append_to_inittab!(simhw);
    pyo3::prepare_freethreaded_python();
}

pub struct PyHost {
    code_path: PathBuf,
    shim_dir: PathBuf,
    /// Writable layer of the simulated CIRCUITPY drive.
    fs_dir: PathBuf,
    thread: Option<JoinHandle<()>>,
}

impl PyHost {
    pub fn new(code_path: PathBuf, shim_dir: PathBuf, fs_dir: PathBuf) -> Self {
        Self {
            code_path,
            shim_dir,
            fs_dir,
            thread: None,
        }
    }

    pub fn code_path(&self) -> &Path {
        &self.code_path
    }

    pub fn shim_dir(&self) -> &Path {
        &self.shim_dir
    }

    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn start(&mut self) {
        self.start_with(None);
    }

    /// Restarts code.py with a scenario played against it.
    pub fn start_scenario(&mut self, scenario: ScenarioRun) {
        self.start_with(Some(scenario));
    }

    fn start_with(&mut self, scenario: Option<ScenarioRun>) {
        self.stop();
        {
            let mut s = hw();
            s.reset_for_run();
            let label = scenario
                .as_ref()
                .map_or_else(|| "restart".to_owned(), |r| format!("scenario {}", r.name));
            s.mark(MarkerKind::Restart, label);
            s.audio.push(AudioCmd::Stop);
            s.push_line(String::new());
        }
        let path = self.code_path.clone();
        let shim_dir = self.shim_dir.clone();
        let fs_dir = self.fs_dir.clone();
        self.thread = Some(std::thread::spawn(move || {
            run_code(&path, &shim_dir, &fs_dir, scenario.as_ref())
        }));
    }

    /// Asks code.py to unwind and waits briefly for it.
    pub fn stop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        hw().stop_requested = true;
        let deadline = Instant::now() + Duration::from_secs(3);
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if thread.is_finished() {
            let _ = thread.join();
        } else {
            hw().push_line("[sim] code.py did not stop within 3 s; leaving it running".into());
        }
    }
}

/// The scenario as the dict `_Scenario.start` takes.
fn scenario_dict<'py>(py: Python<'py>, run: &ScenarioRun) -> PyResult<Bound<'py, PyDict>> {
    let channels = PyDict::new(py);
    for (name, channel) in &run.channels {
        channels.set_item(name, channel)?;
    }
    channels.set_item("white", "white")?;
    let dict = PyDict::new(py);
    dict.set_item("name", &run.name)?;
    dict.set_item("text", &run.text)?;
    dict.set_item("servo", run.servo)?;
    dict.set_item("channels", channels)?;
    Ok(dict)
}

fn run_code(path: &Path, shim_dir: &Path, fs_dir: &Path, scenario: Option<&ScenarioRun>) {
    let result = Python::with_gil(|py| -> PyResult<()> {
        let runtime = PyModule::from_code(
            py,
            &std::ffi::CString::new(RUNTIME)?,
            c"sim_runtime.py",
            c"sim_runtime",
        )?;
        let shims = PyDict::new(py);
        for (name, source, is_pkg) in SHIMS {
            shims.set_item(*name, (*source, *is_pkg))?;
        }
        let scenario = scenario.map(|s| scenario_dict(py, s)).transpose()?;
        runtime.getattr("run")?.call1((
            path.to_string_lossy().into_owned(),
            shims,
            shim_dir.to_string_lossy().into_owned(),
            scenario,
            fs_dir.to_string_lossy().into_owned(),
        ))?;
        Ok(())
    });
    if let Err(err) = result {
        hw().push_line(format!("[sim] Python host error: {err}"));
    }
}
