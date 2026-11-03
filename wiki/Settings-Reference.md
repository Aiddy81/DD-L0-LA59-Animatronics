# Settings reference

Every number you can change in `settings.toml`, without touching the code.

## How to change a setting

1. Open `settings.toml` in a text editor (on **CIRCUITPY** for the robot, or `PropMaker_Feather/settings.toml` for the simulator). Or use the simulator's **Settings (settings.toml)** panel.
2. Change the number after the `=`. Use **whole numbers only**: `90`, not `1.5` or `90.5`.
3. Save. The robot restarts by itself; in the simulator press **🔄 Restart**, or tick **Restart when firmware is saved**.

A line that's missing, or isn't a whole number, uses the default instead. On the robot that prints a warning on the serial console; in the simulator the key turns red in the **Settings (settings.toml)** panel.

> ⚠️ `settings.toml` is shared on GitHub. **Never** put Wi-Fi passwords or other secrets in it.

## Units

The end of each name tells you its unit:

- `_S`: seconds. 60 = 1 minute, 900 = 15 minutes, 3600 = 1 hour.
- `_MG`: milli-g. 1000 = 1 g, the pull of gravity.
- No ending: degrees for wing angles, or a brightness number.

## Every setting

| Setting | Default | What it does | Safe range and tips |
|---|---|---|---|
| `IDLE_MIN_S` | 900 (15 min) | Shortest wait, when left alone, before she wants attention. | 30 or more. Must be ≤ `IDLE_MAX_S`. Use 60 for testing. |
| `IDLE_MAX_S` | 7200 (2 h) | Longest wait. A random wait between the two is picked each time; a boop restarts it. | ≥ `IDLE_MIN_S`. Use 120 for testing. |
| `TRENCH_RUN_S` | 150 (2½ min) | How long the trench run lasts, unless a nudge stops it. | 20–600. |
| `ANGRY_MIN_WINDOW_S` | 60 | Shortest "window" in which nudges are counted towards angry. | 10 or more. Must be ≤ `ANGRY_MAX_WINDOW_S`. |
| `ANGRY_MAX_WINDOW_S` | 180 | Longest window. When a window runs out, the count goes back to zero. | ≥ `ANGRY_MIN_WINDOW_S`. Shorter makes her harder to annoy. |
| `ANGRY_NUDGES` | 3 | How many nudges in one window make her angry. A count, not seconds. | 2–10. 1 makes every boop angry. |
| `NUDGE_Z_MG` | 250 | How big a change in the up-down reading counts as a nudge (0.25 g). | 150–600. Lower is more sensitive; too low and her own wings set it off. |
| `ANIMATION_NUDGE_Z_MG` | 800 | The bigger change that counts as a nudge while attention or the trench run plays, because they shake the board themselves. It's what stops a trench run early. | Above the `Animation peak z change` the log prints after each one. |
| `HARD_MOTION_MG` | 2500 | An up-down reading above this is a shake: straight to angry. | 1500–3900. The sensor is set to ±4 g, so 4000 or more can never happen. |
| `BREATH_PERIOD_S` | 4 | Seconds for one full breath. Bigger is slower and calmer. | 2–10. |
| `BREATH_ANGLE_MIN` | 50 | Right wing's highest point while breathing (lower number = higher wing). | Calibration: see [Calibrate the wings](Calibrate-the-Wings). |
| `BREATH_ANGLE_MAX` | 65 | Right wing's lowest point while breathing. | Calibration. |
| `SERVO2_MIN` | 40 | Left wing (mounted inverted) for start-up and attention. | 🔒 Matches the physical build. Only change when recalibrating. |
| `SERVO2_MAX` | 60 | Left wing for start-up and attention. | 🔒 As above. |
| `BREATH_SERVO2_MIN` | 35 | Left wing while breathing. | 🔒 As above. |
| `BREATH_SERVO2_MAX` | 50 | Left wing while breathing. | 🔒 As above. |
| `EYE_MIN_BRIGHTNESS` | 70 | Dimmest the eyes get while breathing and booping. | 0–360, and less than `EYE_MAX_BRIGHTNESS`. |
| `EYE_MAX_BRIGHTNESS` | 360 | Brightest the eyes get. | 0–360. |
| `WHITE_LED_IDLE_DUTY` | 500 | How brightly the white light glows between flickers. | 0 (off) to 65535 (full). |

The simulator's **Settings (settings.toml)** panel shows the 🔒 left-wing values locked, because they match the real robot.

## More detail (for tinkerers)

- `config.py` reads each key with `_int(key, default)`, which calls `os.getenv()`. CircuitPython 9 only reads whole numbers and quoted strings from `settings.toml`, which is why everything is an integer with its unit in the name. `_MG` values become g (`NUDGE_Z_G` etc.) by dividing by 1000.
- The simulator reads `settings.toml` the same way: `os.getenv()` never sees your PC's environment variables.
- A new tunable read with `_int(...)` in `config.py` shows up in the simulator's **Settings (settings.toml)** panel by itself, with the comment above it as help. If you add one, add a row here too.
- The **Plot** draws `NUDGE_Z_MG`, `ANIMATION_NUDGE_Z_MG` and `HARD_MOTION_MG` as lines over the accelerometer reading, to help you pick values.
