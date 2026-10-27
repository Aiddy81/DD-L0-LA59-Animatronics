"""Run PropMaker_Feather/code.py headless and check the droid still behaves.

Uses the simulator's real shims and runtime (CircuitPython strict mode, board
speed on) with a pure-Python simhw, scripts a few interactions, and fails on
any traceback or missing behaviour. Used by CI; also handy locally:

    python simulator/tests/headless.py [--code PATH] [--seconds 150] [--speed 40]
"""
import argparse
import os
import random
import re
import shutil
import sys
import tempfile
import tomllib
import wave

HERE = os.path.dirname(os.path.abspath(__file__))
SIM = os.path.dirname(HERE)
REPO = os.path.dirname(SIM)
SHIMS = os.path.join(SIM, "shims")

# Sounds the README lists; real files are git-ignored, so CI gets silent ones.
DEFAULT_SOUNDS = ["angry.wav", "noseBoops.wav", "youWho.wav", "trenchRun.wav",
                  "001.wav", "003.wav", "005.wav", "007.wav", "008.wav"]

# Virtual time of the scripted hard shake: after boot, before any nudge, so the
# nudge count is zero and only hard motion can make the droid angry.
SHAKE_AT = 20.0


def stage_firmware(code_path, workdir):
    """Copy code.py, its modules, settings.toml and lib/ into workdir, with
    sounds (silent ones if missing)."""
    src_dir = os.path.dirname(code_path)
    shutil.copy(code_path, os.path.join(workdir, "code.py"))
    sources = [code_path]
    for entry in os.listdir(src_dir):
        full = os.path.join(src_dir, entry)
        if entry == "lib":
            continue
        if entry.endswith(".py") and entry != "code.py":
            shutil.copy(full, os.path.join(workdir, entry))
            sources.append(full)
        elif entry == "settings.toml":
            shutil.copy(full, os.path.join(workdir, entry))
        elif os.path.isfile(os.path.join(full, "__init__.py")):
            shutil.copytree(full, os.path.join(workdir, entry),
                            ignore=shutil.ignore_patterns("__pycache__"))
            for root, _, files in os.walk(full):
                sources += [os.path.join(root, f) for f in files if f.endswith(".py")]
    lib = os.path.join(src_dir, "lib")
    if not os.path.isdir(lib):
        lib = os.path.join(REPO, "PropMaker_Feather", "lib")
    if os.path.isdir(lib):
        shutil.copytree(lib, os.path.join(workdir, "lib"))
    sounds = os.path.join(workdir, "sounds")
    real_sounds = os.path.join(src_dir, "sounds")
    if os.path.isdir(real_sounds):
        shutil.copytree(real_sounds, sounds)
    else:
        os.makedirs(sounds)
    referenced = set()
    for source in sources:
        with open(source, encoding="utf-8") as f:
            referenced |= set(re.findall(r'"([\w.-]+\.wav)"', f.read()))
    # Silent stand-ins for any sound not on disk (all of them in CI).
    for name in sorted(referenced | set(DEFAULT_SOUNDS)):
        if os.path.exists(os.path.join(sounds, name)):
            continue
        with wave.open(os.path.join(sounds, name), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(22050)
            w.writeframes(b"\x00\x00" * 2205)


def led_bursts(times, gap=0.5):
    """Group white-LED writes into (start, end) flicker bursts. Writes inside
    a burst are at most ~0.25 s apart; bursts are seconds apart."""
    bursts = []
    for t in times:
        if bursts and t - bursts[-1][1] <= gap:
            bursts[-1][1] = t
        else:
            bursts.append([t, t])
    return [(a, b) for a, b in bursts if b - a > 1.0]


def any_during(times, spans):
    return any(a < t < b for t in times for a, b in spans)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--code", default=os.path.join(REPO, "PropMaker_Feather", "code.py"))
    parser.add_argument("--seconds", type=float, default=150.0, help="virtual seconds to run")
    parser.add_argument("--speed", type=float, default=40.0, help="virtual seconds per real second")
    args = parser.parse_args()

    sys.path[:0] = [HERE, SHIMS]
    import simhw
    import sim_runtime

    with open(os.path.join(SIM, "sim.toml"), "rb") as f:
        board = tomllib.load(f).get("board", {})
    simhw.SPEED = args.speed
    simhw.DURATION = args.seconds
    simhw.BOARD = {
        "pca": board.get("pca_write_ms", 2.0), "pwm": board.get("pwm_write_ms", 0.05),
        "accel": board.get("accel_read_ms", 1.5), "loop": board.get("loop_ms", 0.25),
    }
    simhw.HEAP_SCALE = board.get("heap_scale", 0.45)
    simhw.HEAP_BUDGET = board.get("heap_budget_kb", 130) * 1024.0

    # Scenario: a hard shake after boot (the UI's 3 g "Hard shake"), which
    # should go straight to angry and restart the angry window; then a boop,
    # the idle timer cut short, then repeated nudges, which should end in a
    # second angry flap. Each idle trigger (45 s, 100 s) picks attention or a
    # trench run at random; the firmer 2 g nudge after each one aborts a trench
    # run (a gentle one is ignored while the wings are animating).
    simhw.scheduled_accel[:] = [(SHAKE_AT, (0.0, 0.0, 3.0))]
    simhw.scheduled_accel += [(30.0, (0.0, 0.0, 1.3))]
    simhw.scheduled_accel += [(60.0 + i * 0.5, (0.0, 0.0, 2.0 if i == 0 else 1.3))
                              for i in range(4)]
    simhw.scheduled_accel += [(110.0, (0.0, 0.0, 2.0))]
    simhw.idle_trigger_at[:] = [45.0, 100.0]
    # The "boot_sequence" behaviour button, after everything else has settled.
    simhw.behaviour_request_at[:] = [(130.0, "boot_sequence")]

    random.seed(1)
    workdir = tempfile.mkdtemp(prefix="ddl0-headless-")
    try:
        stage_firmware(os.path.abspath(args.code), workdir)
        simhw.reset()
        sim_runtime.run(os.path.join(workdir, "code.py"), sim_runtime.load_shim_dir(SHIMS))
    finally:
        shutil.rmtree(workdir, ignore_errors=True)

    log = "\n".join(simhw.log_lines)
    played = [name for _, name in simhw.audio]
    # The shake's response is whichever motion sound plays first after it.
    shake_response = next((name for t, name in simhw.audio if t >= SHAKE_AT
                           and name in ("angry.wav", "noseBoops.wav")), None)
    bursts = led_bursts(simhw.white_writes)
    checks = [
        ("no traceback", "Traceback" not in log),
        ("boot sequence completes", "Boot sequence complete!" in log),
        ("ran until stopped", "[sim] stopped" in log),
        ("both wing servos driven", 0 in simhw.pca and 1 in simhw.pca),
        ("all six eye channels driven", all(ch in simhw.pca for ch in range(2, 8))),
        ("nudge boops (noseBoops.wav)", "noseBoops.wav" in played),
        # Idle picks either animation at random, so accept both; a trench run
        # must be cut short by the nudge that follows its trigger.
        ("idle trigger seeks attention (youWho.wav or trenchRun.wav)",
         "youWho.wav" in played or "trenchRun.wav" in played),
        ("a nudge aborts the trench run",
         "trenchRun.wav" not in played or "Trench run aborted" in log),
        ("trench run sound loops, then stops with the run",
         all(any(n == "trenchRun.wav" and abs(lt - t) < 0.2 for lt, n in simhw.looped_audio)
             and any(s >= t for s in simhw.audio_stops)
             for t, name in simhw.audio if name == "trenchRun.wav")),
        ("hard shake goes straight to angry (angry.wav)", shake_response == "angry.wav"),
        ("repeated nudges get angry (angry.wav)", played.count("angry.wav") >= 2),
        ("white LED flickers", bool(bursts)),
        ("wings keep breathing during an LED flicker", any_during(simhw.wing_writes, bursts)),
        ("motion still sensed during an LED flicker", any_during(simhw.accel_reads, bursts)),
        ("behaviour buttons found",
         {"boot_sequence", "nose_boops", "angry_animation", "trench_run"} <= set(simhw.behaviours)),
        ("no button for code.py's background tasks",
         not {"motion_sense", "ambient_breathing", "idle_attention", "random_led_scheduler"}
         & set(simhw.behaviours)),
        ("boot_sequence button runs it again",
         "[sim] running boot_sequence" in log and log.count("Boot sequence complete!") == 2),
        ("heap estimate within board budget", simhw.heap_peak[0] < simhw.HEAP_BUDGET),
    ]
    print()
    print("Behaviour buttons: {}".format(", ".join(simhw.behaviours)))
    print("Audio played: {}".format(simhw.audio))
    print("LED bursts: {}".format(["{:.1f}-{:.1f}s".format(a, b) for a, b in bursts]))
    print("Heap peak ~{:.0f} KB of {:.0f} KB".format(simhw.heap_peak[0] / 1024, simhw.HEAP_BUDGET / 1024))
    if simhw.unsimulated_libs:
        print("Not simulated: {}".format(", ".join(simhw.unsimulated_libs)))
    failed = [name for name, ok in checks if not ok]
    for name, ok in checks:
        print("{} {}".format("PASS" if ok else "FAIL", name))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
