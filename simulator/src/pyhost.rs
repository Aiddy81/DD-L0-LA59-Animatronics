//! Runs `code.py` on an embedded CPython interpreter, on its own thread.

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

use crate::hw::{hw, AudioCmd};

/// Hardware stand-ins, compiled into the exe so the dist folder stays simple.
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
    fn set_pwm_pin(pin: &str, duty: u16) {
        if pin == "EXTERNAL_NEOPIXELS" {
            hw().white_duty = duty;
        }
    }

    #[pyfunction]
    fn set_pin(pin: &str, value: bool) {
        let mut s = hw();
        match pin {
            "LED" => s.onboard_led = value,
            "EXTERNAL_POWER" => s.external_power = value,
            _ => {}
        }
    }

    #[pyfunction]
    fn accel() -> (f32, f32, f32) {
        let [x, y, z] = hw().next_accel();
        (x, y, z)
    }

    #[pyfunction]
    fn log(text: &str) {
        hw().write_log(text);
    }

    #[pyfunction]
    fn play_wav(path: String, level: f32, looped: bool) {
        hw().audio.push(AudioCmd::Play {
            path,
            level,
            looped,
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
    fn stopping() -> bool {
        hw().stop_requested
    }

    #[pyfunction]
    fn strict() -> bool {
        hw().strict
    }

    #[pyfunction]
    fn take_idle_trigger() -> bool {
        std::mem::take(&mut hw().idle_trigger)
    }
}

/// Must run before anything touches Python.
pub fn init_python() {
    pyo3::append_to_inittab!(simhw);
    pyo3::prepare_freethreaded_python();
}

pub struct PyHost {
    code_path: PathBuf,
    thread: Option<JoinHandle<()>>,
}

impl PyHost {
    pub fn new(code_path: PathBuf) -> Self {
        Self {
            code_path,
            thread: None,
        }
    }

    pub fn code_path(&self) -> &Path {
        &self.code_path
    }

    pub fn is_running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    pub fn start(&mut self) {
        self.stop();
        {
            let mut s = hw();
            s.reset_for_run();
            s.audio.push(AudioCmd::Stop);
            s.push_line(String::new());
        }
        let path = self.code_path.clone();
        self.thread = Some(std::thread::spawn(move || run_code(&path)));
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

fn run_code(path: &Path) {
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
        runtime
            .getattr("run")?
            .call1((path.to_string_lossy().into_owned(), shims))?;
        Ok(())
    });
    if let Err(err) = result {
        hw().push_line(format!("[sim] Python host error: {err}"));
    }
}
