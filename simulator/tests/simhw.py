"""Pure-Python stand-in for the simulator's Rust `simhw` module, so the shims
and code.py can run headless (in CI) without the desktop app.

It records what code.py does to the hardware; headless.py drives the
scenario (nudges, idle trigger) and checks the result.
"""
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
_partial = [""]
_heap_now = [0.0]


def reset():
    _t0[0] = time.monotonic()
    _debt[0] = 0.0


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


def set_pwm_pin(name, duty):
    if name == "EXTERNAL_NEOPIXELS":
        white[0] = duty
        white_writes.append(now())


def set_pin(name, value):
    pins[name] = value


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


def stop_audio():
    audio_stops.append(round(now(), 1))


def set_volume(level):
    pass


def charge(kind):
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
