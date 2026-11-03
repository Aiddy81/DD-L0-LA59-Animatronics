# Calibrate the wings

> **You'll need:** the robot with firmware on it, a text editor, the simulator (recommended) · **Time:** 20 minutes

Every servo is a tiny bit different, and the left one is mounted upside down. Calibrating means changing a few numbers so both wings open and close evenly.

## What you can change

All of these are in `settings.toml`. Angles are in degrees.

| Setting | Wing | Used when | Default |
|---|---|---|---|
| `BREATH_ANGLE_MIN`, `BREATH_ANGLE_MAX` | Right | Breathing | 50, 65 |
| `BREATH_SERVO2_MIN`, `BREATH_SERVO2_MAX` | Left | Breathing | 35, 50 |
| `SERVO2_MIN`, `SERVO2_MAX` | Left | Start-up and attention | 40, 60 |

**Right wing:** a *lower* number lifts it *higher*. **Left wing:** it's mounted inverted, so it's the other way round.

## What not to touch

- The servo **pulse range** (544–2400 µs) and the **channel numbers** in `hardware.py`. They match the servos and the wiring.
- `lib/` and `boot_out.txt`.

## Steps

1. Plug the robot in. Open **CIRCUITPY** and open `settings.toml` in a text editor.
2. Find the wing lines (search for `BREATH_ANGLE_MIN`).
3. Change one number by a small step, 2 or 3 degrees. For example change `BREATH_SERVO2_MAX = 50` to `BREATH_SERVO2_MAX = 48`.
4. Save the file. The board restarts with the new value.
5. Unplug, run on battery and watch her breathe. Do both wings close to the same place?
6. Repeat until they match. Write down the numbers that work.

> **Tip:** Try the change in the simulator first. Its **Settings (settings.toml)** panel shows every value with its default. The left-wing values are locked there on purpose, because they match the real robot, so edit `settings.toml` by hand for those.

### ✅ It should look like this

Both wings rise and fall together, close to the same height, and neither buzzes at the end of its travel.

## ⚠️ Something went wrong?

| Problem | Fix |
|---|---|
| A wing buzzes or strains at one end | It's pushing past its stop. Move that end's number back towards the middle a few degrees. |
| Nothing changed after saving | Check the line has no decimal point (`48`, not `48.5`) and is spelt exactly right. A bad value prints a warning on the serial console and uses the default. |
| The board didn't restart | Logging mode makes the drive read-only. [Turn it off](Test-and-Read-the-Log#turn-logging-off). |
| Both wings move but out of step | Breathing uses the `BREATH_*` values; start-up and attention use `SERVO2_*`. Calibrate both sets. |

## More detail (for tinkerers)

- `config.py` reads each value with `os.getenv()` and falls back to its built-in default if a line is missing or not a whole number.
- The left wing is driven as `angle_max + angle_min - angle`, so it mirrors the right wing.
- If you change the calibration on the robot, update the same values in the project's `PropMaker_Feather/settings.toml` so the simulator and the next deploy agree.
