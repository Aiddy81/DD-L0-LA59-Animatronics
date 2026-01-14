# Copilot Instructions for DD-L0-LA59 Animatronics Firmware

This document provides essential guidelines for AI coding agents working on the `DD-L0-LA59 Animatronics Firmware` project. The firmware orchestrates interactive animatronic behaviors using CircuitPython and Adafruit hardware.

## 1. Architecture Overview

The project is built on CircuitPython's `asyncio` for concurrent execution of animatronic behaviors.
- **Event-Driven Interactions**: Behaviors are primarily triggered by motion sensing (LIS3DH accelerometer) and idle timeouts, managed through `asyncio.Event` objects.
- **Hardware Abstraction**: Adafruit libraries (`adafruit_servokit`, `adafruit_lis3dh`, `audiomixer`) are used to interface with servos, RGB LEDs, accelerometer, and audio hardware.
- **Modular Behaviors**: Each distinct animatronic action (e.g., `boot_sequence`, `ambient_breathing`, `angry_animation`, `nose_boops`, `attention_animation`) is implemented as an independent asynchronous function within `code.py`.
- **Central Orchestration**: The `main` asynchronous function in `code.py` is responsible for initializing hardware and launching all concurrent behavior tasks.

**Key Files:**
- `PropMaker_Feather/code.py`: Contains all core logic, hardware initialization, and behavior functions.
- `PropMaker_Feather/sounds/`: Stores WAV files for audio playback, referenced by filename in `code.py`.
- `PropMaker_Feather/lib/`: Contains necessary CircuitPython library modules (`.mpy` files).

## 2. Developer Workflows

- **Deployment**: The entire `PropMaker_Feather` directory, including `code.py`, `lib/`, and `sounds/`, is copied directly to the root of the `CIRCUITPY` drive on the Adafruit device.
- **Dependency Management**: Python dependencies are managed by manually copying pre-compiled `.mpy` files into `PropMaker_Feather/lib/`. The `requirements.txt` file is currently empty and not used for dependency installation.
- **Configuration**: All hardware parameters, animation timings, thresholds, and other settings are hardcoded within `PropMaker_Feather/code.py`. The `settings.toml` file is not used.
- **Debugging**: Primarily involves monitoring `print()` statements via the CircuitPython serial console.

## 3. Project-Specific Conventions and Patterns

- **Asynchronous Programming**: Extensive use of `asyncio` for non-blocking operations, cooperative multitasking, and state management via `asyncio.Event` (e.g., `pause_breathing`, `reset_idle_event`).
- **Servo-based LED Control**: RGB LEDs (main and small eyes) are controlled using the `adafruit_servokit` by setting their `angle` property after configuring their `actuation_range` and `set_pulse_width_range` for a 360-degree range. This effectively uses the servo channels as PWM outputs for LED brightness.
  - **Example**:
    ```python
    mainEye_red = kit.servo[2]
    mainEye_red.actuation_range = 360
    mainEye_red.set_pulse_width_range(0,6550) # Non-standard range for PWM control
    mainEye_red.angle = 180 # Sets brightness/color component
    ```
- **Motion Detection**: The LIS3DH accelerometer detects motion based on hardcoded `z` axis thresholds in `motion_sense()`. Repeated or strong motion triggers "angry" behavior, while lighter motion triggers "boop".
- **Randomization**: Many aspects of behavior, such as idle animation timings (`idle_attention`), LED flash intervals (`random_led_scheduler`), and sound effect selection, incorporate `random.uniform` or `random.randint` for more natural and unpredictable interactions.

## 4. Integration Points and External Dependencies

- **Hardware Peripherals**:
    - **Servos**: `kit.servo[0]` and `kit.servo[1]` control the wings.
    - **RGB Eyes**: `kit.servo[2]`-`kit.servo[7]` control the main and small RGB eyes.
    - **White LED**: `white_led` (PWMOut on `board.EXTERNAL_NEOPIXELS`) for data processing flashes.
    - **LIS3DH Accelerometer**: Used for motion sensing.
    - **I2S Audio**: For `.wav` sound playback.
- **CircuitPython Libraries**: The `PropMaker_Feather/lib/` directory must contain all `.mpy` files imported in `code.py`, including `adafruit_lis3dh`, `adafruit_pca9685` (via `adafruit_servokit`), `adafruit_servokit`, `adafruit_bus_device`, `adafruit_motor`, `adafruit_register`, and `asyncio` components.
- **Audio Assets**: WAV files located in `PropMaker_Feather/sounds/`. Ensure correct filenames as referenced in `code.py` (e.g., `angry.wav`, `noseBoops.wav`, `youWho.wav`).

## 5. Future Enhancements

- **Configuration Management**: Externalize configurable parameters from `code.py` into `settings.toml` for easier modification without firmware changes.
- **Error Handling**: Implement more robust error handling for hardware failures or missing sound files.
- **Testing**: Develop a strategy for testing individual behaviors and hardware interactions.

Please provide feedback on any unclear or incomplete sections.
