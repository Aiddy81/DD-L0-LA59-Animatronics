"""Pure-Python stand-in for the simulator's Rust `simhw` module, so the shims
and code.py can run headless (in CI) without the desktop app.

It records what code.py does to the hardware; headless.py drives the
scenario (nudges, idle trigger) and checks the result.
"""
import math
import sys
import time

SPEED = 40.0
DURATION = 120.0
STRICT = True
BOARD = {"pca": 2.0, "pwm": 0.05, "accel": 1.5, "loop": 0.25}
HEAP_SCALE = 0.45
HEAP_BUDGET = 130 * 1024.0

pca = {}
pins = {}
white = [0]
audio = []
looped_audio = []  # (time, name) of sounds started on a loop
audio_stops = []   # times stop_audio() was called
mixer_formats = []  # (rate, channels, bits, signed) of each Mixer created
log_lines = []
heap_peak = [0.0]
unsimulated_libs = []
# (virtual time, (x, y, z) in g) readings queued for the accelerometer.
scheduled_accel = []
idle_trigger_at = []
# Behaviour buttons: what sim_runtime.py offered, and (time, name) clicks.
behaviours = []
behaviour_request_at = []
# Scenario inputs: one-shot readings, the held reading, a pending idle
# trigger; and the last set_scenario() report (name, state, results).
NUDGE_DELTA_G = 0.3
SHAKE_DELTA_G = 2.0
accel_pulses = []
held_accel = [(0.0, 0.0, 1.0)]
idle_now = [False]
scenario = [None]
# Virtual times of right-wing writes, white-LED writes and accelerometer
# reads, so headless.py can tell whether behaviours overlap.
wing_writes = []
white_writes = []
accel_reads = []
# (virtual time, events, tasks, angry, idle) for each State panel snapshot.
states = []

_t0 = [time.monotonic()]
_debt = [0.0]
_board_time = [0.0]
# (task name, virtual seconds) for each task step that held the loop 1 ms or more.
task_holds = []
_partial = [""]
_heap_now = [0.0]


# Generic parts (parts.toml in the app): name -> (kind, left to shims). Empty
# in headless runs; a test fills it in. part_values holds what the part calls
# set, as the app's HwState::parts does: ("level", x), ("rgb", (r, g, b)) and
# ("pixels", [...]) in 0-1, ("angle", deg), ("pressed", bool).
MAX_PART_NAME = 32
MAX_PIXELS = 1024
MAX_PART_WARNINGS = 128
parts = {}
part_values = {}
_part_warned = set()


def reset():
    _t0[0] = time.monotonic()
    _debt[0] = 0.0
    _board_time[0] = 0.0
    task_holds.clear()
    part_values.clear()
    _part_warned.clear()


def _part_warn(call, name, problem):
    if len(_part_warned) >= MAX_PART_WARNINGS or name in _part_warned:
        return
    _part_warned.add(name)
    log('[sim] simhw.{}("{}"): {}; ignored\n'.format(call, name, problem))


def _part_name(name, call):
    if not isinstance(name, str):
        _part_warn(call, "", "the name must be a string")
        return None
    return name[:MAX_PART_NAME + 1]


def _part_target(call, name, kinds, setting):
    if name not in parts:
        problem = "no part has this name in parts.toml"
    elif parts[name][0] not in kinds:
        problem = "it is a {} part, not {}".format(parts[name][0], " or ".join(kinds))
    elif setting and not parts[name][1]:
        problem = "it follows its pca or pin, so a shim can't set it"
    else:
        return parts[name][0]
    _part_warn(call, name, problem)
    return None


def _number(value):
    if isinstance(value, (str, bytes)):
        return None
    try:
        return float(value)
    except (TypeError, ValueError, OverflowError):
        return None


def _rgb(item):
    if isinstance(item, (str, bytes)):
        return None
    try:
        values = list(item) if len(item) == 3 else None
    except TypeError:
        return None
    if values is None:
        return None
    values = [_number(v) for v in values]
    return None if None in values else values


def _unit_colour(c):
    if not all(math.isfinite(v) for v in c):
        return None
    return tuple(min(max(v, 0.0), 255.0) / 255.0 for v in c)


def set_part(name, value):
    name = _part_name(name, "set_part")
    if name is None:
        return
    kind = _part_target("set_part", name, ("led", "pixels", "servo_part"), True)
    if kind is None:
        return
    problem = _set_part_value(name, kind, _shim_value(value))
    if problem:
        _part_warn("set_part", name, problem)


def _shim_value(value):
    """("on", bool), ("number", float), ("rgb", [r, g, b]) or None, checking
    bool first as True is also an int."""
    if isinstance(value, bool):
        return ("on", value)
    number = _number(value)
    if number is not None:
        return ("number", number)
    rgb = _rgb(value)
    return ("rgb", rgb) if rgb is not None else None


def _set_part_value(name, kind, value):
    """Stores the value for `kind`, or returns what's wrong with it."""
    if value is None:
        return "the value must be a number, True/False or (r, g, b)"
    what, v = value
    if kind == "servo_part":
        if what != "number" or not math.isfinite(v):
            return "a servo_part needs an angle in degrees"
        part_values[name] = ("angle", v)
    elif what == "on":
        part_values[name] = ("level", 1.0 if v else 0.0)
    elif what == "number":
        if not math.isfinite(v):
            return "brightness must be a number, not NaN or infinite"
        part_values[name] = ("level", min(max(v, 0.0), 1.0))
    elif kind == "pixels":
        return "a pixels part takes brightness here; set_pixels sets its colours"
    else:
        colour = _unit_colour(v)
        if colour is None:
            return "r, g and b must be numbers"
        part_values[name] = ("rgb", colour)
    return None


def _pixel_list(pixels):
    if isinstance(pixels, (str, bytes)):
        return None, "pixels must be a list of (r, g, b)"
    try:
        if len(pixels) > MAX_PIXELS:
            return None, "more than {} pixels".format(MAX_PIXELS)
    except TypeError:
        pass
    try:
        items = iter(pixels)
    except TypeError:
        return None, "pixels must be a list of (r, g, b)"
    out = []
    while True:
        try:
            item = next(items)
        except StopIteration:
            return out, None
        except Exception:  # noqa: BLE001 - the app also swallows a failing iterable
            return None, "pixels must be a list of (r, g, b)"
        if len(out) == MAX_PIXELS:
            return None, "more than {} pixels".format(MAX_PIXELS)
        rgb = _rgb(item)
        if rgb is None:
            return None, "each pixel must be (r, g, b)"
        out.append(rgb)


def set_pixels(name, pixels):
    name = _part_name(name, "set_pixels")
    if name is None:
        return
    found, problem = _pixel_list(pixels)
    if _part_target("set_pixels", name, ("pixels",), True) is None:
        return
    if problem is None:
        colours = [_unit_colour(c) for c in found]
        if None in colours:
            problem = "r, g and b must be numbers"
        else:
            part_values[name] = ("pixels", colours)
    if problem:
        _part_warn("set_pixels", name, problem)


def part_input(name):
    name = _part_name(name, "part_input")
    if name is None:
        return False
    if _part_target("part_input", name, ("button",), False) is None:
        return False
    return part_values.get(name) == ("pressed", True)


def now():
    return (time.monotonic() - _t0[0]) * SPEED


def speed():
    return SPEED


def paused():
    return False


def stopping():
    # A scenario run ends as soon as the scenario has its verdict.
    done = scenario[0] is not None and scenario[0][1] in ("pass", "fail", "error")
    return done or now() >= DURATION


def strict():
    return STRICT


def take_idle_trigger():
    if idle_now[0]:
        idle_now[0] = False
        return True
    if idle_trigger_at and now() >= idle_trigger_at[0]:
        idle_trigger_at.pop(0)
        return True
    return False


def set_behaviours(names):
    # Keep the last non-empty list; the run ends by clearing it.
    if names:
        behaviours[:] = names


def take_behaviour_request():
    if behaviour_request_at and now() >= behaviour_request_at[0][0]:
        return behaviour_request_at.pop(0)[1]
    return None


def set_behaviour_running(name):
    pass


def set_pca(channel, duty):
    pca[channel] = duty
    if channel == 0:
        wing_writes.append(now())


def set_pwm_pin(name, duty, frequency=500):
    if name == "EXTERNAL_NEOPIXELS":
        white[0] = duty
        white_writes.append(now())


def set_pin(name, value):
    pins[name] = value


def pin_input(name):
    # Headless runs have no button parts.
    return None


def accel():
    accel_reads.append(now())
    if scheduled_accel and now() >= scheduled_accel[0][0]:
        return scheduled_accel.pop(0)[1]
    if accel_pulses:
        return accel_pulses.pop(0)
    return held_accel[0]


def _bump(x, y, z, delta):
    held = held_accel[0]
    accel_pulses.append((held[0] + x * delta, held[1] + y * delta, held[2] + z * delta))


def nudge(x=0.0, y=0.0, z=1.0):
    _bump(x, y, z, NUDGE_DELTA_G)


def shake(x=0.0, y=0.0, z=1.0):
    _bump(x, y, z, SHAKE_DELTA_G)


def accel_range(g):
    pass


def set_accel(x, y, z):
    held_accel[0] = (x, y, z)


def trigger_idle():
    idle_now[0] = True


def set_state(events, tasks, angry, idle):
    states.append((now(), list(events), list(tasks), angry, idle))


def set_scenario(name, state, results):
    scenario[0] = (name, state, list(results))


def log(text):
    _partial[0] += text
    while "\n" in _partial[0]:
        line, _partial[0] = _partial[0].split("\n", 1)
        log_lines.append(line)
        sys.__stdout__.write(line + "\n")


def play_wav(path, level, looped):
    name = path.replace("\\", "/").rsplit("/", 1)[-1]
    audio.append((round(now(), 1), name))
    if looped:
        looped_audio.append((round(now(), 1), name))


def mixer_format(rate, channels, bits, signed):
    mixer_formats.append((rate, channels, bits, signed))


def stop_audio():
    audio_stops.append(round(now(), 1))


def set_volume(level):
    pass


def board_time():
    return _board_time[0]


def task_hold(name, secs):
    task_holds.append((name, secs))


def charge(kind):
    _board_time[0] += BOARD.get(kind, 0.0) / 1000.0
    _debt[0] += BOARD.get(kind, 0.0) / 1000.0
    real = _debt[0] / SPEED
    if real >= 0.001:
        _debt[0] = 0.0
        time.sleep(real)


def set_heap(desktop_bytes):
    _heap_now[0] = desktop_bytes * HEAP_SCALE
    heap_peak[0] = max(heap_peak[0], _heap_now[0])


def heap():
    return (_heap_now[0], HEAP_BUDGET)


def unsimulated(name):
    if name not in unsimulated_libs:
        unsimulated_libs.append(name)
