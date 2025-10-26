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

## License
See `LICENSE` file if present, or contact the author for usage terms.