"""Check that shims dropped into a folder (dist\\shims\\ in the app) override
or add to the built-in ones, and that a broken one can't crash the run.

Uses the simulator's real runtime with the pure-Python simhw, like headless.py:

    python simulator/tests/shim_dir.py
"""
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SIM = os.path.dirname(HERE)
SHIMS = os.path.join(SIM, "shims")

failures = []


def check(ok, what):
    print(("PASS " if ok else "FAIL ") + what)
    if not ok:
        failures.append(what)


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def run(workdir, code, shim_dir, setup=None):
    """Run `code` as code.py and return the log text. `setup` runs after the
    stand-in simhw is reset, as the app's parts are set up at each start."""
    import simhw
    import sim_runtime

    code_path = os.path.join(workdir, "fw", "code.py")
    write(code_path, code)
    simhw.log_lines.clear()
    simhw.reset()
    if setup:
        setup()
    sim_runtime.run(code_path, sim_runtime.load_shim_dir(SHIMS), shim_dir)
    return "\n".join(simhw.log_lines)


def guide_shims():
    """The example shims in docs/custom-shims.md, by module name: each is a
    ```python block whose first line is `# simulator\\dist\\shims\\NAME.py`."""
    with open(os.path.join(SIM, "docs", "custom-shims.md"), encoding="utf-8") as f:
        blocks = f.read().split("```python\n")[1:]
    found = {}
    for block in blocks:
        code = block.split("```", 1)[0]
        first = code.split("\n", 1)[0]
        prefix = "# simulator\\dist\\shims\\"
        if first.startswith(prefix) and first.endswith(".py"):
            found[first[len(prefix):-3]] = code
    return found


def close(a, b):
    return all(abs(x - y) < 1e-6 for x, y in zip(a, b))


def check_part_calls(workdir, shim_dir):
    """The simhw part calls, through the guide's neopixel and touchio shims,
    and straight on the stand-in with bad input."""
    import simhw

    examples = guide_shims()
    check({"neopixel", "touchio"} <= set(examples), "guide has the neopixel and touchio shims")
    for name in ("neopixel", "touchio"):
        write(os.path.join(shim_dir, name + ".py"), examples.get(name, ""))
    simhw.parts.clear()
    simhw.parts.update({
        "band_pixels": ("pixels", True),
        "touch_pad": ("button", True),
        "lamp": ("led", True),
        "flap": ("servo_part", True),
        "tail": ("servo_part", False),
    })

    def touched():
        simhw.part_values["touch_pad"] = ("pressed", True)

    log = run(workdir, (
        "import board, neopixel, touchio\n"
        "ring = neopixel.NeoPixel(board.D10, 12, brightness=0.5, auto_write=False)\n"
        "pad = touchio.TouchIn(board.A1)\n"
        "ring.fill(0x0050FF)\n"
        "ring[1] = (255, 0, 0) if pad.value else 0\n"
        "ring[2:4] = [(10, 20, 30), (40, 50, 60)]\n"
        "ring.show()\n"
        "print('pad', pad.value, ring[1], len(ring))\n"
        "other = neopixel.NeoPixel(board.D11, 3)\n"
        "other.fill((1, 2, 3))\n"
        "other.fill((4, 5, 6))\n"
    ), shim_dir, touched)
    check("Traceback" not in log, "neopixel example: runs in strict mode")
    kind, pixels = simhw.part_values.get("band_pixels", (None, []))
    check(kind == "pixels" and len(pixels) == 12, "neopixel example: sets all 12 pixels")
    check(len(pixels) == 12 and close(pixels[0], (0, 40 / 255, 0.5))
          and close(pixels[1], (0.5, 0, 0)) and close(pixels[3], (20 / 255, 25 / 255, 30 / 255)),
          "neopixel example: packed ints, tuples, slices and brightness")
    check("pad True (255, 0, 0) 12" in log, "touchio example: a pressed button reaches code.py")
    check(log.count('simhw.set_pixels("D11")') == 1 and "no part has this name" in log,
          "an unknown part name warns once per run")

    simhw.log_lines.clear()
    simhw.reset()
    simhw.set_part("lamp", 0.25)
    check(simhw.part_values.get("lamp") == ("level", 0.25), "set_part: led brightness")
    simhw.set_part("lamp", (255, 51, 0))
    kind, rgb = simhw.part_values.get("lamp", (None, ()))
    check(kind == "rgb" and close(rgb, (1.0, 0.2, 0.0)), "set_part: led (r, g, b)")
    simhw.set_part("band_pixels", True)
    check(simhw.part_values.get("band_pixels") == ("level", 1.0), "set_part: pixels on")
    simhw.set_part("flap", 120)
    check(simhw.part_values.get("flap") == ("angle", 120.0), "set_part: servo angle")
    check(simhw.part_input("touch_pad") is False, "part_input: released button")
    before = dict(simhw.part_values)
    raised = None
    try:
        simhw.set_part("lamp", float("nan"))
        simhw.set_part("lamp", "bright")
        simhw.set_part("flap", True)
        simhw.set_part("touch_pad", 1)
        simhw.set_part("tail", 90)
        simhw.set_part(42, 1)
        simhw.set_pixels("band_pixels", [(0, 0, 0)] * 2000)
        simhw.set_pixels("band_pixels", iter(lambda: (1, 2, 3), None))
        simhw.set_pixels("band_pixels", "red")
        simhw.set_pixels("band_pixels", [(0, float("inf"), 0)])
        simhw.set_pixels("lamp", [(1, 2, 3)])
        check(simhw.part_input("flap") is False and simhw.part_input(None) is False,
              "part_input: not a button reads False")
    except Exception as e:  # noqa: BLE001 - any escape is a failure here
        raised = e
    check(raised is None, "bad part calls never raise ({})".format(raised))
    check(simhw.part_values == before, "bad part calls change nothing")
    warned = [l for l in simhw.log_lines if l.startswith("[sim] simhw.")]
    check(len(warned) == 6, "one warning per name ({} lines)".format(len(warned)))
    simhw.parts.clear()


def main():
    sys.path[:0] = [HERE, SHIMS]
    import simhw
    import sim_runtime

    simhw.STRICT = True
    simhw.DURATION = 60.0
    workdir = tempfile.mkdtemp(prefix="ddl0-shims-")
    shim_dir = os.path.join(workdir, "dist", "shims")
    try:
        built_in = sim_runtime.load_shim_dir(SHIMS)
        check(built_in.get("adafruit_motor", (None, False))[1], "built-in package found")
        check("adafruit_motor.servo" in built_in, "built-in sub-module found")
        check("sim_runtime" not in built_in, "sim_runtime is not a shim")

        for label, folder in (("no shims folder", shim_dir), ("no folder given", None)):
            log = run(workdir, "import board\nprint('led', board.LED)\n", folder)
            check("led" in log and "Traceback" not in log, label + ": runs as before")
            check("loaded from" not in log, label + ": nothing logged as from disk")

        write(os.path.join(shim_dir, "mytoy.py"), "VALUE = 42\n")
        write(os.path.join(shim_dir, "adafruit_lis3dh.py"), "MARK = 'disk'\n")
        write(os.path.join(shim_dir, "toypkg", "__init__.py"), "")
        write(os.path.join(shim_dir, "toypkg", "sub.py"), "NAME = 'sub'\n")
        write(os.path.join(shim_dir, "__pycache__", "junk.py"), "raise RuntimeError\n")
        log = run(workdir, (
            "import mytoy, adafruit_lis3dh, toypkg.sub\n"
            "print('values', mytoy.VALUE, adafruit_lis3dh.MARK, toypkg.sub.NAME)\n"
        ), shim_dir)
        check("values 42 disk sub" in log, "new, overriding and package shims are used")
        check("Traceback" not in log, "strict mode accepts new shims as board modules")
        check("shim 'mytoy' loaded from dist" in log and "(new)" in log, "new shim is logged")
        check("shim 'adafruit_lis3dh' loaded from dist" in log
              and "(overrides built-in)" in log, "override is logged")
        check("junk" not in log, "__pycache__ is ignored")

        os.remove(os.path.join(shim_dir, "mytoy.py"))
        log = run(workdir, "import mytoy\n", shim_dir)
        check("ImportError" in log or "ModuleNotFoundError" in log,
              "a shim deleted from disk is gone on the next run")

        write(os.path.join(shim_dir, "broken.py"), "def oops(:\n    pass\n")
        try:
            log = run(workdir, "import broken\n", shim_dir)
            raised = False
        except BaseException:  # noqa: BLE001 - any escape is a failure here
            raised = True
        check(not raised, "a syntax error in a shim doesn't escape run()")
        check("SyntaxError" in log and "broken.py" in log, "the syntax error is in the log")
        check("Code done running." in log, "the run ends normally after the error")

        with open(os.path.join(shim_dir, "latin.py"), "wb") as f:
            f.write(b"NAME = '\xe9'\n")
        log = run(workdir, "import board\n", shim_dir)
        check("shim 'latin'" in log and "skipped" in log, "an unreadable shim is skipped and logged")
        os.remove(os.path.join(shim_dir, "latin.py"))
        os.remove(os.path.join(shim_dir, "broken.py"))

        check_part_calls(workdir, shim_dir)
    finally:
        shutil.rmtree(workdir, ignore_errors=True)

    if failures:
        print("\n{} check(s) failed".format(len(failures)))
        sys.exit(1)
    print("\nAll shim folder checks passed")


if __name__ == "__main__":
    main()
