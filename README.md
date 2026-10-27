# DD-L0-LA59 Animatronics Firmware

## Overview
This project implements interactive animatronic behaviors for a custom device using Adafruit hardware and CircuitPython 9.x. The system features servo-driven wings, RGB LED eyes, sound playback, and motion sensing, all orchestrated with asynchronous code for lifelike, responsive actions.

## Features
- **Servo Animation**: Smooth, natural wing movements for idle, attention, trench-run and angry states.
- **LED Effects**: RGB LEDs (main and small "eyes") display color patterns synchronized with animations.
- **Audio Playback**: WAV sound effects triggered by events and behaviors.
- **Motion Sensing**: LIS3DH accelerometer detects movement to trigger responses.
- **Idle & Attention Logic**: Device performs attention-seeking animations after random idle periods.
- **Randomized Effects**: White LED flashes and sound effects at random intervals for realism.
- **Async Architecture**: Uses CircuitPython's `asyncio` for concurrent, non-blocking behaviors.

## Hardware Requirements
- Adafruit PropMaker Feather (with I2C, PWM, and I2S support, accelerometer, audio amplifier) - [https://www.adafruit.com/product/5768](https://www.adafruit.com/product/5768)
- 8-Channel PWM or Servo FeatherWing Add-on For All Feather Boards (for servos and LEDs) - [https://www.adafruit.com/product/2928](https://www.adafruit.com/product/2928)
- Servos (x2, for wing movement)
- RGB LEDs (main and small "eyes"))

## File Structure
- `PropMaker_Feather/code.py` — Entry point: runs the boot sequence, then starts every behaviour as an `asyncio` task
- `PropMaker_Feather/hardware.py` — Board setup: accelerometer, PCA9685 channel map, LEDs, audio, shared events
- `PropMaker_Feather/config.py` — Reads the tunables from `settings.toml`, with built-in defaults
- `PropMaker_Feather/settings.toml` — Tunables (see [Tuning](#tuning))
- `PropMaker_Feather/log.py`, `boot.py`, `log_off.py` — Serial output, optionally saved to `log.txt` (see [Logging on the robot](#logging-on-the-robot))
- `PropMaker_Feather/behaviours/` — One module per behaviour: `boot`, `breathing`, `motion` (boops and angry), `attention`, `trench_run`, `leds`
- `PropMaker_Feather/sounds/` — WAV files for sound effects
- `PropMaker_Feather/lib/` — CircuitPython libraries (Adafruit drivers, etc.)

## Main Behaviors
- **Boot Sequence**: Smooth startup animation for wings and eyes, with LED and audio effects.
- **Ambient Breathing**: Wings and LEDs move in a breathing pattern.
- **Motion Response**: Device reacts to motion with "boop" or "angry" animations and sounds.
- **Idle Attention**: After a random idle period, device performs an attention-seeking animation, or (equal chance) a trench run.
- **Trench Run**: A 2½-minute Top Gun: Maverick-style flight. Tight turns split the wings (one up, one down), steep climbs lift both, steep dives drop both, and the wings buffet under load. Eyes glow orange like an afterburner, brighter on climbs; the white LED flickers. A nudge or shake cuts it short.
- **Random LED Effects**: White LED flashes at random intervals to simulate data processing.

## Setup & Usage
1. Copy the contents of `PropMaker_Feather/` (`code.py`, `boot.py`, `config.py`, `hardware.py`, `log.py`, `log_off.py`, `settings.toml`, `behaviours/`, `lib/`, `sounds/`) to the root of your CIRCUITPY drive, preserving the folder structure.
2. Ensure all required libraries are present in `PropMaker_Feather/lib/`.
3. Place WAV files in `PropMaker_Feather/sounds/` and name them correctly.
4. Power the device. The firmware will run automatically.

### Logging on the robot
The wings can't run while the board is plugged into a computer, so the serial output can be saved to a file instead and read afterwards.

1. **Turn on:** with the board plugged in, create an empty file named `log_mode` (no extension) in the root of CIRCUITPY, then press the board's reset button. A soft reload (Ctrl-D) isn't enough, because `boot.py` only runs on a reset or power-on.
2. **Run** the robot on battery. Every serial line is appended to `log.txt`, starting with the seconds since power-on. Each run starts with `=== DD-L0-LA59 starting ===`. At 256 KB the log moves to `log.old.txt` and starts again, so only the latest ~512 KB is kept.
3. **Read:** plug back in and copy `log.txt` off. While logging mode is on, the drive is **read-only to the computer**, so you can copy files off but not edit or add any.
4. **Turn off:** open the serial console, press Ctrl-C, then type `import log_off` and press Enter. This deletes `log_mode` and restarts the board, and the drive is editable again.

Each line is written straight to flash, so a pulled battery loses at most the line being written. A write can take a few milliseconds, so the wings may pause very briefly when a line is logged.

## Customization
- Add or replace WAV files in the `sounds/` directory for new sound effects.
- Tune timings, thresholds and calibration in `settings.toml` (below). No code change needed.
- Change how a behaviour moves or lights up in its module under `behaviours/`.

### Tuning

`settings.toml` is read at boot. Edit it on the CIRCUITPY drive, save, and the board restarts with the new values. CircuitPython only reads whole numbers and quoted strings from it, so each name carries its unit: `_S` seconds, `_MG` milli-g (1000 = 1 g). A missing line falls back to the default in `config.py`. A value that isn't a whole number prints a warning on the serial console and uses the default. The file is committed to git, so never put Wi-Fi passwords or other secrets in it.

| Key | Default | What it does |
|---|---|---|
| `IDLE_MIN_S`, `IDLE_MAX_S` | 900, 7200 | Quiet spell (random, between these) before seeking attention |
| `TRENCH_RUN_S` | 150 | Length of the trench run |
| `ANGRY_MIN_WINDOW_S`, `ANGRY_MAX_WINDOW_S` | 60, 180 | Length of the random window in which nudges are counted |
| `ANGRY_NUDGES` | 3 | Nudges inside one window that make the droid angry |
| `NUDGE_Z_MG` | 250 | A change in z this big from where it rests, up or down, is a nudge. The resting z is measured at start-up and after each animation, so this works however the board is mounted or the droid sits |
| `ANIMATION_NUDGE_Z_MG` | 800 | The change in z needed to interrupt the attention animation or trench run, whose wing movement shakes the board. The serial log prints `Animation peak z change` after each one, to help tune it |
| `HARD_MOTION_MG` | 2500 | A z reading above this is a shake (straight to angry) |
| `BREATH_PERIOD_S` | 4 | Breathing period |
| `BREATH_ANGLE_MIN`, `BREATH_ANGLE_MAX` | 50, 65 | Right-wing angle range while breathing |
| `SERVO2_MIN`, `SERVO2_MAX` | 40, 60 | Left-wing calibration (mounted inverted) for boot and attention |
| `BREATH_SERVO2_MIN`, `BREATH_SERVO2_MAX` | 35, 50 | Left-wing calibration while breathing |
| `EYE_MIN_BRIGHTNESS`, `EYE_MAX_BRIGHTNESS` | 70, 360 | Eye brightness range for breathing and boops (0–360) |
| `WHITE_LED_IDLE_DUTY` | 500 | White LED glow between effects (0–65535) |

## Sound Files and Their Functions

| File Name        | Used In Function         | Purpose/Behavior Triggered                |
|------------------|-------------------------|-------------------------------------------|
| angry.wav        | angry_animation()        | Angry response to repeated/strong motion  |
| noseBoops.wav    | nose_boops()             | Gentle/loving response to motion          |
| youWho.wav       | attention_animation()    | Attention-seeking after idle period       |
| trenchRun.wav    | trench_run()             | Trench run after idle period (a short clip, looped for the whole run) |
| 001.wav          | play_Wav_task (random)   | General/random sound effect               |
| 003.wav          | play_Wav_task (random)   | General/random sound effect               |
| 005.wav          | play_Wav_task (random)   | General/random sound effect               |
| 007.wav          | play_Wav_task (random)   | General/random sound effect               |
| 008.wav          | play_Wav_task (random)   | General/random sound effect               |

Place these files in `PropMaker_Feather/sounds/` with the exact names above. You can add or replace WAV files for custom effects, but ensure the filenames match those referenced in the code or update the code accordingly.

## Simulator

`simulator/` is a Windows desktop app that runs `PropMaker_Feather/code.py` (with its modules and `settings.toml`) **unmodified** and shows the droid in 3D, so behaviours can be developed without the robot. A Rust (Bevy) app embeds Python and swaps the CircuitPython hardware modules (`board`, `adafruit_servokit`, `adafruit_lis3dh`, `audiomixer`, ...) for stand-ins that drive the 3D model, the lights and your speakers.

### Build and run

Needs Docker Desktop only; nothing is installed on the host.

```powershell
.\simulator\build.ps1 -Run
```

The first build takes a while. Output goes to `simulator\dist\`, a self-contained folder with the bundled Python. Run `simulator\dist\ddl0-sim.exe` directly after that. It finds `PropMaker_Feather\code.py` by walking up from the exe, or takes a path as its first argument.

### Using it

- **Motion**: *Nudge* gives one accelerometer reading 0.3 g past the held value, in the direction picked in *Direction* (+X, −X, +Y, −Y, +Z, −Z; default +Z). From rest that is 1.3 g on z, which is a boop. **Clicking the droid** also boops: the reading is pushed along the surface you clicked (top of the head = +Z, a side = ±X/±Y, using the full angle), and the longer you hold the mouse button the harder it is, from 0.3 g for a tap (0.1 s or less) up to 2 g at 1 s or more. From rest on top, a tap crosses the nudge threshold, about half a second crosses the animation threshold, and a full second counts as hard motion. The log shows e.g. `[sim] boop 0.9 g from +Z`. A drag to orbit the camera isn't a boop. Three nudges inside the angry window cause an angry flap. *Hard shake* gives one reading 2 g past the held value, which from rest is 3 g and goes straight to angry. A nudge is too gentle to interrupt the attention animation or trench run; use *Hard shake*. The three *held* sliders set the x, y and z reading the sensor returns between nudges; *Rest* puts them back to [0, 0, 1] g. *Tipped over* holds [0, 0, -1] g, which counts as one nudge. The firmware reads only z, so ±X and ±Y nudges are ignored, as on the robot. A −Z shake from rest reads -1 g, so it only counts as a nudge.
- **State** (side panel): why a behaviour did or didn't fire. Each `asyncio.Event` in `hardware.py` (green = set), the nudges counted so far against `ANGRY_NUDGES` with the time left in the current angry window, the time left until the idle trigger (or *not waiting* while its animation runs), and every running task with what it's awaiting, e.g. `motion_sense → nose_boops → sine_flaps → sleep`. Under each task are all the number variables in its firmware functions at that moment, e.g. `motion_count=1  rest_z=1.00  nose_boops.boops=2  sine_flaps.angle=38.91` (variables of a function the task called are prefixed with its name; a variable appears once it has been assigned). The values are read live (10 times a second) from the firmware's running coroutines, so the firmware needs no changes. The angry window only restarts when `motion_sense` next checks it, so its bar can sit at 0 s for a moment.
  - **Shows up on its own:** a new `asyncio.Event` in `hardware.py`; a new behaviour started as a task; a new function a running task calls (it appears in that task's chain); a new counter or timer variable (int or float) in a behaviour, as a raw value under its task.
  - **Needs a simulator change:** events declared outside `hardware.py`, and a countdown bar for a new timer. The two bars read the nudge count (`motion_count`, `angry_window_start`, `angry_window_length` in `motion_sense`) and the idle wait (`idle_attention`'s `wait_for`) by name. Renaming any of those blanks its bar instead of crashing (the raw values still show under the task); update `_State` in `simulator/shims/sim_runtime.py` to match.
- **Speed**: fast-forward up to 200x with the slider, or click *1x*, *10x* or *100x* to jump straight to that speed. *Skip to next event* runs the clock at 1000x until a behaviour starts, then drops back to 1x. A behaviour has started when a sound plays or an event in `hardware.py` changes (e.g. `pause_breathing`). Breathing and the white-LED flashes don't stop it, so one click gets you through the idle wait to the attention animation or trench run. The log names what stopped it. Click *Stop skip* to end it early. *Trigger idle now* ends the 15-120 min idle wait so the attention animation or the trench run plays (50/50).
- **Behaviours** (toolbar): one button per behaviour, to run it straight away without reproducing its trigger. A button runs the firmware's own function once, with breathing paused (`pause_breathing`) until it finishes. The other buttons are greyed out until then. The row is built from the firmware each run, so a new behaviour gets a button with no simulator change. To get one, it must be an `async def` in a `behaviours/` module that can be called with no arguments. Functions `code.py` already runs with `asyncio.create_task(...)` (breathing, motion sensing, idle attention, the LED scheduler) and names starting with `_` are left out. A trench run started this way doesn't hold off the white-LED scheduler the way one started from idle does.
- **Scenarios** (toolbar): pick one of `simulator/scenarios/*.toml` and click *Run scenario*. It restarts `code.py`, plays the scenario's inputs at set times and checks what the droid does. The badge shows *running*, then *PASS* or *FAIL* with a count; hover it for each check. Each result is also logged as a `[sim] scenario ...` line. See [Scenarios](#scenarios).
- **Edit and save any firmware file** (`code.py`, a module under `PropMaker_Feather/`, or `settings.toml`) and the simulator restarts with the change. Errors show as a traceback in the log.
- **Edit and save `simulator/sim.toml`** (wing mapping, eye channels, `[board]` timing) and it applies live, without restarting `code.py`; the log shows `[sim] sim.toml reloaded`. An invalid file logs a warning and keeps the previous config. `[model] kind` only applies at start-up; use **Model** in the toolbar to switch live.
- **`settings.toml`** is read the way CircuitPython 9 reads it: `os.getenv()` sees only the whole numbers and quoted strings in that file, never your PC's environment variables.
- **CircuitPython strict** (on by default) holds `code.py` and its modules to what the board has. Imports outside CircuitPython 9 and `lib/` fail, as do APIs CircuitPython lacks (`random.choices`, `os.path`, `asyncio.TaskGroup`...). `time.monotonic()` gets the board's reduced float precision, so long-uptime timer drift shows when you fast-forward. Untick it to run with full desktop Python; the change applies on the next restart.
- **Pause** (toolbar) freezes `code.py`'s clock; *Resume* carries on from the same moment. Restarting also unpauses. Sounds already playing finish. While paused, *+10 ms* and *+100 ms* run the clock for that long and stop again. Every animation frame inside the step plays, at the current speed, so you can walk through a fast angry flap or the trench-run buffeting.
- **Keyboard shortcuts**: Space pause/resume, . step 100 ms, F skip to next event, N nudge, S hard shake, R restart, I trigger idle. They're ignored while you type in the log's search box, and each button's tooltip shows its key.
- **Camera**: left/right drag to orbit, middle drag to pan, scroll to zoom. The *View* row jumps to Front, Side, Top, Eyes (a close-up of the eyes) or Reset (the start-up view).
- **Screenshot** saves a PNG of the window. **Record** saves a PNG every 0.1 s, for up to 30 s, to make into a GIF with any GIF tool. Both go into `simulator\dist\captures\`, next to the exe.
- The side panel shows every PCA9685 channel's duty and pulse width. The *view* column adds the servo angle the firmware asked for on the wing channels, and a swatch of each eye's colour, with the eye's peak duty (6550 µs) shown as full brightness. The **Plot** shows wing angles, LED levels (main eye solid, small eye dashed, each as separate R/G/B, plus the white LED) and, in its bottom band, the accelerometer reading the firmware actually got (z bright, x and y dim) with its thresholds: `NUDGE_Z_MG` (yellow), `ANIMATION_NUDGE_Z_MG` (orange) and `HARD_MOTION_MG` (red), read from `settings.toml`, plus the sensor's clip level (dashed grey). Pick a window from 5 s to 5 min; the last 5 min are always kept. *Freeze* holds the view still while recording carries on. Drag to pan; Ctrl+scroll or right-drag to zoom (the three bands share one time axis); *Reset* or a double-click goes back to following. Hover for every ticked series' value at that moment, and untick a series to hide it. Click a section heading (Motion, PCA9685, Board, Plot, Settings) to fold it away.
- **Settings** (side panel): every tunable `config.py` reads, with its value in `settings.toml`, its default and a *Reset* button. Hover a key for its help, taken from the comment above it in `config.py`, so a new `_int(...)` tunable shows up without changing the simulator. Changes stay pending (amber) until *Save* writes them into `settings.toml`. Only the edited values change: comments, order and every other line are kept, and a key not yet in the file is added at the end. Only whole numbers are written. With *Restart when firmware is saved* on, the run restarts with the new values; otherwise press *Restart*. *Revert* drops the pending changes. The left-wing calibration (`SERVO2_*`, `BREATH_SERVO2_*`) is shown but locked, because it matches the physical build. A key shown in bold differs from its default; one in red holds a value that isn't a whole number, so the firmware uses the default. Hand edits to `settings.toml` or `config.py` show up in the panel when there are no pending changes.
- **Log**: search box (case-insensitive), *Firmware* and *[sim]* toggles to show either source, tracebacks in red, and *Clear*, *Copy* (the lines shown) and *Save* (the whole log, to `simulator\dist\captures\`).
- **Remembered between runs**: window size, panel sizes, the Model choice, the toolbar checkboxes, which sections are folded, the log toggles, and the plot's window and ticked series. They're saved in `simulator\dist\ddl0-sim.prefs.toml`; delete it to go back to the defaults. Rebuilding keeps it and the `captures` folder. Once you've run the app, its saved Model choice takes priority over `[model] kind` in `sim.toml`. Speed always starts at 1x.

The simulator reproduces the hardware maths: angle to pulse width at 50 Hz, and the LIS3DH clipping at its configured range. So it shows what the robot will really do, including:
- The eye channels peak at 32.8% duty, because `set_pulse_width_range(0, 6550)` is about a third of the 20 ms period.
- The accelerometer saturates at its range. The firmware uses `RANGE_4_G`, so a 3 g shake reads as 3 g and passes `abs(z) > 2.5` (hard motion). At `RANGE_2_G` it would clip to about 2 g and only count as a nudge; the plot's trace flattens at the dashed clip line, below the red hard-motion line.
- A blocking `time.sleep()` in a behaviour freezes every other behaviour while it runs, just as on the board. (`data_processing_led()` used to do this; it now uses `await asyncio.sleep()`, so the wings keep breathing during the white-LED flicker.)

### Matching the real board

The RP2040 is much slower than a PC and has far less memory, so the simulator models both:

- **Board speed** (toolbar, on by default): every servo/LED write, accelerometer read and scheduler pass takes as long as it does on the robot. So an animation runs at the speed the robot will really manage, not just the speed the code asks for.
- **Heap gauge** (Board panel): an estimate of how much of the board's free memory the firmware (`code.py` and its modules, bytecode included) uses. It warns in the log at 80% and 100%. `gc.mem_free()` and `gc.mem_alloc()` return the estimate too. It's approximate: it catches "this clearly won't fit", not the last few KB.
- **Not simulated**: importing a library that's in `lib/` (or built into CircuitPython) but which the simulator doesn't model, such as `neopixel`, gives you a stand-in. The stand-in accepts every call and does nothing, so the rest of your code keeps running. It's listed in the Board panel. To make it do something, write a shim for it (see [Custom shims](#custom-shims)).

The timing and memory figures in `simulator/sim.toml` `[board]` start as estimates. To measure your robot:

The bench runs inside the robot with no PC attached. It saves its results in the board's non-volatile memory, and you read them back over USB afterwards.

1. On the CIRCUITPY drive, rename `code.py` to `code_main.py`, then copy `simulator/board_bench.py` in as `code.py`.
2. Unplug the Feather, fit it in the robot and power on. Only the main eye's red LED flickers dimly; the wings don't move. When it's done the main eye glows green. If the Feather's red LED blinks instead, it failed, and the error is saved in place of the results.
3. Plug the Feather into the PC and open the serial console. Press Ctrl-D to rerun if you missed the output. With USB connected it prints the saved `[board]` block instead of measuring.
4. Paste that block over the one in `simulator/sim.toml`. Then, on the drive, delete the bench `code.py` and rename `code_main.py` back to `code.py`.

If the Feather is plugged into USB and nothing is saved, the bench measures and prints straight away. That only works if the FeatherWing is attached.

### Scenarios

A scenario is a timeline of inputs plus what the droid should do, so the test checklist (boot, breathing, nudge → boop, 3 nudges / hard shake → angry, idle → attention) runs without clicking. The shipped ones cover that checklist: `boot`, `breathing`, `boop`, `angry-nudges`, `angry-shake`, `idle` and `sideways-nudge`. Run them from the toolbar or with `headless.py` (see [Tests and CI](#tests-and-ci)). To add one, drop a `.toml` file into `simulator/scenarios/`; the toolbar picks it up within a second.

```toml
description = "Three nudges in one window end in angry.wav"
start = "Boot sequence complete!"   # optional: times count from this log line

[[step]]
t = 5.0             # virtual seconds (from start, or from code.py starting)
action = "nudge"    # nudge | shake | tilt | idle | wait
# axis = "+x"       # nudge/shake direction: +x -x +y -y +z -z (default "+z")

[[expect]]
sound = "angry.wav" # must play between after and by
after = 15.0        # optional, default 0
by = 20.0
```

- **Steps**: `nudge` (one reading 0.3 g past the held value) and `shake` (2 g past it) match the Motion buttons. `axis = "+x"` (also `-x`, `+y`, `-y`, `+z`, `-z`; default `"+z"`) picks their direction, like *Direction*. `idle` is *Trigger idle now*. `tilt` holds the accelerometer at `g = [x, y, z]` (default `[0, 0, -1]`, tipped over). `wait` does nothing, but the scenario runs until it.
- **Expectations**, one kind each:
  - `sound`: a WAV file name, glob (`"angry*.wav"`) or list of them.
  - `wing = "right"` or `"left"` with `above` and/or `below`: the servo angle in degrees (from the pulse width, via `sim.toml` `[servo]`).
  - `led = "main_r"`, `"main_g"`, `"main_b"`, `"small_r"` .. `"small_b"` or `"white"` with `above` and/or `below`: brightness from 0 to 1 (eyes at their 6550 µs peak count as 1).
  - `log`: text that appears in a line `code.py` prints.
  - `never = true` turns any of them around: it fails if it does happen in the window.
  - `name` replaces the description shown for it.
- A traceback always fails the scenario. It finishes once every step has run and every expectation has a result; `code.py` stopping before then is a fail. A mistake in the file (an unknown key, a missing `by`) shows as *ERROR* with the reason in the log.
- Prefer `start = "Boot sequence complete!"` for anything after boot: Python's imports take a few virtual seconds at high speed, so times counted from the very start vary between machines.
- A gentle nudge is ignored while the wings are animating, so space nudges past the boop before them (see `angry-nudges.toml`).

### Custom shims

The hardware stand-ins ("shims") are compiled into the exe, so it runs on its own. To add one, or change a built-in one, without rebuilding, put a `.py` file in `simulator\dist\shims\` (create the folder). A file there replaces the built-in shim of the same name or adds a new one, and is read at the start of every run; the log names each shim taken from it. Rebuilding keeps the folder.

[`simulator/docs/custom-shims.md`](simulator/docs/custom-shims.md) is a step-by-step guide: when you need a shim, mirroring the real library's API, the `simhw` calls a shim uses to reach the simulator, worked examples, testing, and when Rust changes are needed.

### Tests and CI

`python simulator/tests/headless.py` runs the real firmware (`code.py`, its modules and `settings.toml`) headless, with CircuitPython strict mode and board speed on. It scripts a hard shake, a nudge, an idle trigger, repeated nudges and a *boot_sequence* behaviour button. It fails on any traceback, if the boop, attention or angry behaviour doesn't happen, if the State panel's events, tasks, nudge count or timers can't be read, if the behaviour buttons are wrong or don't run, or if the wings and motion sensing stall during a white-LED flicker. Then it plays every scenario in `simulator/scenarios/` against a fresh run each and fails if any of them doesn't pass. `--scenario NAME` (repeatable) runs only those scenarios. It needs Python 3.11+ (CI uses 3.13); without Python on the host, run it in the dev image: `docker run --rm -v "${PWD}:/workspace" -w /workspace ddl0-sim-dev python3 simulator/tests/headless.py`.

The Settings panel's `settings.toml` reader and writer (`simulator/src/settings.rs`) has unit tests that need only `rustc`: `docker run --rm -v "${PWD}:/workspace" -w /workspace/simulator ddl0-sim-dev bash -c "rustc --edition 2021 --test src/settings.rs -o /tmp/settings-test && /tmp/settings-test"`.

`simulator\build.ps1` runs `headless.py` in the dev image as its last step, so a firmware change that breaks a behaviour fails the build. The failing checks and scenarios are listed as `FAIL` lines. The simulator is still built in `simulator\dist\`, so you can open it to see what went wrong.

GitHub Actions (`.github/workflows/simulator.yml`) runs that test on every PR. It also builds the Windows simulator in the same Docker image, with rustfmt and clippy, and attaches the built `ddl0-sim-windows` folder to the run for download.

### The 3D model

There are two models, switchable live with **Model** in the toolbar (`[model] kind` in `simulator/sim.toml` picks the start-up one):

- **Lola**: `simulator/assets/droid.glb`, a detailed model of L0-LA59. It's generated by a Blender script, so it can be rebuilt and edited:

  ```powershell
  & "E:\Apps\Blender Foundation\Blender 5.1\blender.exe" --background --factory-startup --python simulator\assets\src\build_lola.py -- simulator\assets\droid.glb
  ```

  (adjust the path to your `blender.exe`). The simulator loads `droid.glb` at start-up, so no rebuild of the app is needed.

  `simulator/assets/src/lola.blend` is the same model as an editable Blender file, with modifiers still live and a camera and lights. It's generated too: add its path as a second argument after `droid.glb` to regenerate it. If you edit the `.blend` by hand, export it to `droid.glb` with *+Y Up* and *Apply Modifiers* ticked, keeping the node names above. The script's output would overwrite those hand edits.
- **Primitive**: the simple built-in version (`simulator/src/droid.rs`), always available.

Any model can drive the simulator if it has nodes named `wing_right` and `wing_left` (the rear white panels, each with its origin on its hinge line; on Lola that is the front edge of the panel), plus `eye_main`, `eye_small` and `led_white` (the lenses). The meshes inside them must have different names. `simulator/sim.toml` sets how servo angles map onto panel rotation: neutral angle, direction, linkage gain, travel limits and hinge axis per wing.

## License

Copyright (C) 2026 Adriano Cafarella.

This project (the firmware in `PropMaker_Feather/` outside `lib/`, and everything in `simulator/`) is licensed under the **GNU Affero General Public License v3.0 or later**. See [`LICENSE`](LICENSE).

Third-party components keep their own licenses:
- The Adafruit CircuitPython libraries in `PropMaker_Feather/lib/` are MIT-licensed by Adafruit Industries.
- The embeddable Python that `simulator/build.ps1` bundles into `simulator/dist/` is under the PSF License; its `LICENSE.txt` ships alongside it.
- Rust crates used by the simulator are under their own licenses (mostly MIT/Apache-2.0).