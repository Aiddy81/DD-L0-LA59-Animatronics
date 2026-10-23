"""Tunables, read from settings.toml.

CircuitPython's settings.toml only holds integers and quoted strings, so every
value is an integer with its unit in the name (_S seconds, _MG milli-g). Each
default is the value the droid was tuned with, so a missing key or file
behaves exactly as before.
"""
import os


def _int(key, default):
    value = os.getenv(key)
    if value is None:
        return default
    if not isinstance(value, int):
        print("settings.toml: {} must be a whole number, using {}".format(key, default))
        return default
    return value


# Idle: seek attention after a random quiet spell in this range.
IDLE_MIN_S = _int("IDLE_MIN_S", 15 * 60)
IDLE_MAX_S = _int("IDLE_MAX_S", 120 * 60)
# How long the trench run (the other idle animation) lasts.
TRENCH_RUN_S = _int("TRENCH_RUN_S", 5 * 60)

# Angry: this many nudges inside one random window gets the droid angry.
ANGRY_MIN_WINDOW_S = _int("ANGRY_MIN_WINDOW_S", 1 * 60)
ANGRY_MAX_WINDOW_S = _int("ANGRY_MAX_WINDOW_S", 3 * 60)
ANGRY_NUDGES = _int("ANGRY_NUDGES", 3)

# Motion: a change of more than NUDGE from the resting reading, in any direction,
# is a nudge (TRENCH_NUDGE during the trench run, whose buffeting shakes the board).
# Total acceleration above HARD is a shake (straight to angry).
NUDGE_G = _int("NUDGE_MG", 60) / 1000
TRENCH_NUDGE_G = _int("TRENCH_NUDGE_MG", 200) / 1000
HARD_MOTION_G = _int("HARD_MOTION_MG", 2500) / 1000

# Breathing: period and right-wing (servo 0) angle range.
BREATH_PERIOD_S = _int("BREATH_PERIOD_S", 4)
BREATH_ANGLE_MIN = _int("BREATH_ANGLE_MIN", 50)
BREATH_ANGLE_MAX = _int("BREATH_ANGLE_MAX", 65)

# Left wing (servo 1) is mounted inverted; these line it up with the right wing.
# Breathing has its own pair because it runs in a narrower range.
SERVO2_MIN = _int("SERVO2_MIN", 40)
SERVO2_MAX = _int("SERVO2_MAX", 60)
BREATH_SERVO2_MIN = _int("BREATH_SERVO2_MIN", 35)
BREATH_SERVO2_MAX = _int("BREATH_SERVO2_MAX", 50)

# Eye brightness range for breathing and boops, in eye "angle" units (0-360).
EYE_MIN_BRIGHTNESS = _int("EYE_MIN_BRIGHTNESS", 60)
EYE_MAX_BRIGHTNESS = _int("EYE_MAX_BRIGHTNESS", 360)

# White LED glow between effects (duty cycle, 0-65535).
WHITE_LED_IDLE_DUTY = _int("WHITE_LED_IDLE_DUTY", 500)
