# Channel map

Which part is on which output. The firmware (`hardware.py`) and the simulator both expect exactly this, so the wiring must match.

## PCA9685 (Servo FeatherWing), 50 Hz

| Channel | Part | Notes |
|---|---|---|
| 0 | Right wing servo | Pulse 544–2400 µs. A lower angle lifts the wing higher. |
| 1 | Left wing servo | Pulse 544–2400 µs. Mounted inverted; calibrated with `SERVO2_*` and `BREATH_SERVO2_*`. |
| 2 | Main eye, red | Driven as a 360° "servo", pulse 0–6550 µs. |
| 3 | Main eye, green | As above. |
| 4 | Main eye, blue | As above. |
| 5 | Small eye, red | As above. |
| 6 | Small eye, green | As above. |
| 7 | Small eye, blue | As above. |
| 8–15 | Free | Not used. Available for [new hardware](Adding-New-Hardware). |

## Feather pins

| Pin | Part | Notes |
|---|---|---|
| `EXTERNAL_NEOPIXELS` | White LED | `pwmio.PWMOut` at 5 kHz; duty 0–65535. Idle glow is `WHITE_LED_IDLE_DUTY`. |
| `EXTERNAL_POWER` | Power for external parts | Turned on at start-up. |
| `SCL`, `SDA` | I²C bus | Shared by the PCA9685 and the built-in LIS3DH. |
| `ACCELEROMETER_INTERRUPT` | LIS3DH interrupt | Range set to ±4 g. |
| `I2S_BIT_CLOCK`, `I2S_WORD_SELECT`, `I2S_DATA` | Speaker amplifier | Mixer: 22050 Hz, stereo, 16-bit signed. |
| `LED` | Board's red LED | |

## Why the eyes never reach full brightness

A 0–6550 µs pulse in a 20 ms (50 Hz) period is at most about 32.8% duty. That's how the robot is built, and the simulator shows the same: its **PCA9685** panel treats 6550 µs as full brightness.
