# DD-L0-LA59 Animatronics Firmware

## Overview
This project implements interactive animatronic behaviors for a custom device using Adafruit hardware and CircuitPython 9.x. The system features servo-driven wings, RGB LED eyes, sound playback, and motion sensing, all orchestrated with asynchronous code for lifelike, responsive actions.

## Features
- **Servo Animation**: Smooth, natural wing movements for idle, attention, and angry states.
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
- `PropMaker_Feather/code.py` — Main firmware controlling all hardware and behaviors
- `PropMaker_Feather/sounds/` — WAV files for sound effects
- `PropMaker_Feather/lib/` — CircuitPython libraries (Adafruit drivers, etc.)

## Main Behaviors
- **Boot Sequence**: Smooth startup animation for wings and eyes, with LED and audio effects.
- **Ambient Breathing**: Wings and LEDs move in a breathing pattern.
- **Motion Response**: Device reacts to motion with "boop" or "angry" animations and sounds.
- **Idle Attention**: After a random idle period, device performs an attention-seeking animation.
- **Random LED Effects**: White LED flashes at random intervals to simulate data processing.

## Setup & Usage
1. Copy all files to your CIRCUITPY drive, preserving the folder structure.
2. Ensure all required libraries are present in `PropMaker_Feather/lib/`.
3. Place WAV files in `PropMaker_Feather/sounds/` and name them correctly.
4. Power the device. The firmware will run automatically.

## Customization
- Add or replace WAV files in the `sounds/` directory for new sound effects.
- Adjust servo and LED parameters in `code.py` for different movement or color effects.

## Sound Files and Their Functions

| File Name        | Used In Function         | Purpose/Behavior Triggered                |
|------------------|-------------------------|-------------------------------------------|
| angry.wav        | angry_animation()        | Angry response to repeated/strong motion  |
| noseBoops.wav    | nose_boops()             | Gentle/loving response to motion          |
| youWho.wav       | attention_animation()    | Attention-seeking after idle period       |
| 001.wav          | play_Wav_task (random)   | General/random sound effect               |
| 003.wav          | play_Wav_task (random)   | General/random sound effect               |
| 005.wav          | play_Wav_task (random)   | General/random sound effect               |
| 007.wav          | play_Wav_task (random)   | General/random sound effect               |
| 008.wav          | play_Wav_task (random)   | General/random sound effect               |

Place these files in `PropMaker_Feather/sounds/` with the exact names above. You can add or replace WAV files for custom effects, but ensure the filenames match those referenced in the code or update the code accordingly.

## Simulator

`simulator/` is a Windows desktop app that runs `PropMaker_Feather/code.py` **unmodified** and shows the droid in 3D, so behaviours can be developed without the robot. A Rust (Bevy) app embeds Python and swaps the CircuitPython hardware modules (`board`, `adafruit_servokit`, `adafruit_lis3dh`, `audiomixer`, ...) for stand-ins that drive the 3D model, the lights and your speakers.

### Build and run

Needs Docker Desktop only; nothing is installed on the host.

```powershell
.\simulator\build.ps1 -Run
```

The first build takes a while. Output goes to `simulator\dist\`, a self-contained folder with the bundled Python. Run `simulator\dist\ddl0-sim.exe` directly after that. It finds `PropMaker_Feather\code.py` by walking up from the exe, or takes a path as its first argument.

### Using it

- **Motion**: *Nudge* (or click the head) gives one accelerometer reading of 1.3 g, which is a boop. Three nudges inside the angry window cause an angry flap. *Tipped over* holds z at -1 g.
- **Speed**: fast-forward up to 200x. *Trigger idle now* ends the 15-120 min idle wait so the attention animation plays.
- **Edit and save `code.py`** and the simulator restarts with the change. Errors show as a traceback in the log.
- **CircuitPython strict** (on by default) holds `code.py` to what the board has. Imports outside CircuitPython 9 and `lib/` fail, as do APIs CircuitPython lacks (`random.choices`, `os.path`, `asyncio.TaskGroup`...). `time.monotonic()` gets the board's reduced float precision, so long-uptime timer drift shows when you fast-forward. Untick it to run with full desktop Python; the change applies on the next restart.
- **Camera**: left/right drag to orbit, middle drag to pan, scroll to zoom.
- The side panel shows every PCA9685 channel's duty and pulse width, plus a 20 s plot of wing angles and LED levels.

The simulator reproduces the hardware maths: angle to pulse width at 50 Hz, and the LIS3DH clipping at its configured range. So it shows what the robot will really do, including:
- The eye channels peak at 32.8% duty, because `set_pulse_width_range(0, 6550)` is about a third of the 20 ms period.
- With `RANGE_2_G` the accelerometer never reports more than about 2 g, so `abs(z) > 2.5` (hard motion) can't fire.
- `data_processing_led()` uses blocking `time.sleep()`, which freezes every other behaviour while it runs.

### Matching the real board

The RP2040 is much slower than a PC and has far less memory, so the simulator models both:

- **Board speed** (toolbar, on by default): every servo/LED write, accelerometer read and scheduler pass takes as long as it does on the robot. So an animation runs at the speed the robot will really manage, not just the speed the code asks for.
- **Heap gauge** (Board panel): an estimate of how much of the board's free memory `code.py` uses. It warns in the log at 80% and 100%. `gc.mem_free()` and `gc.mem_alloc()` return the estimate too. It's approximate: it catches "this clearly won't fit", not the last few KB.
- **Not simulated**: importing a library that's in `lib/` (or built into CircuitPython) but which the simulator doesn't model, such as `neopixel`, gives you a stand-in. The stand-in accepts every call and does nothing, so the rest of your code keeps running. It's listed in the Board panel.

The timing and memory figures in `simulator/sim.toml` `[board]` start as estimates. To measure your robot:

1. Copy `simulator/board_bench.py` to the root of the CIRCUITPY drive.
2. In the serial console press Ctrl-C, then Enter, and type `import board_bench`. Only the main eye's red LED flickers dimly; the wings don't move.
3. Paste the `[board]` block it prints over the one in `simulator/sim.toml`, then delete `board_bench.py` from the drive.

### Tests and CI

`python simulator/tests/headless.py` runs the real `code.py` headless, with CircuitPython strict mode and board speed on. It scripts a nudge, an idle trigger and repeated nudges, and fails on any traceback or if the boop, attention or angry behaviour doesn't happen. It needs Python 3.11+ (CI uses 3.13).

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

Any model can drive the simulator if it has nodes named `wing_right` and `wing_left` (the white top panels, with their origin on the hinge along the spine), plus `eye_main`, `eye_small` and `led_white` (the lenses). The meshes inside them must have different names. `simulator/sim.toml` sets how servo angles map onto panel rotation: neutral angle, direction, linkage gain, travel limits and hinge axis per wing.

## License

Copyright (C) 2026 Adriano Cafarella.

This project (the firmware in `PropMaker_Feather/code.py` and everything in `simulator/`) is licensed under the **GNU Affero General Public License v3.0 or later**. See [`LICENSE`](LICENSE).

Third-party components keep their own licenses:
- The Adafruit CircuitPython libraries in `PropMaker_Feather/lib/` are MIT-licensed by Adafruit Industries.
- The embeddable Python that `simulator/build.ps1` bundles into `simulator/dist/` is under the PSF License; its `LICENSE.txt` ships alongside it.
- Rust crates used by the simulator are under their own licenses (mostly MIT/Apache-2.0).