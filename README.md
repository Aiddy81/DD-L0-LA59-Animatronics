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
- **Camera**: left/right drag to orbit, middle drag to pan, scroll to zoom.
- The side panel shows every PCA9685 channel's duty and pulse width, plus a 20 s plot of wing angles and LED levels.

The simulator reproduces the hardware maths: angle to pulse width at 50 Hz, and the LIS3DH clipping at its configured range. So it shows what the robot will really do, including:
- The eye channels peak at 32.8% duty, because `set_pulse_width_range(0, 6550)` is about a third of the 20 ms period.
- With `RANGE_2_G` the accelerometer never reports more than about 2 g, so `abs(z) > 2.5` (hard motion) can't fire.
- `data_processing_led()` uses blocking `time.sleep()`, which freezes every other behaviour while it runs.

### Your own 3D model

The built-in model is a primitive L0-LA59: red saucer head, the two white top panels as the servo-driven wings hinged on the centre spine, main and small eyes, gear ring, stem and base. Put a `simulator/assets/droid.glb` in place and it replaces that model. Name the nodes `wing_right`, `wing_left`, `eye_main`, `eye_small` and `led_white`. `simulator/sim.toml` sets how servo angles map onto wing rotation: neutral angle, direction, linkage gain and hinge axis per wing.

## License
See `LICENSE` file if present, or contact the author for usage terms.