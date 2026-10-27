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


def run(workdir, code, shim_dir):
    """Run `code` as code.py and return the log text."""
    import simhw
    import sim_runtime

    code_path = os.path.join(workdir, "fw", "code.py")
    write(code_path, code)
    simhw.log_lines.clear()
    simhw.reset()
    sim_runtime.run(code_path, sim_runtime.load_shim_dir(SHIMS), shim_dir)
    return "\n".join(simhw.log_lines)


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
    finally:
        shutil.rmtree(workdir, ignore_errors=True)

    if failures:
        print("\n{} check(s) failed".format(len(failures)))
        sys.exit(1)
    print("\nAll shim folder checks passed")


if __name__ == "__main__":
    main()
