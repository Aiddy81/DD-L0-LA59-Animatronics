"""Runs code.py inside the simulator.

Installs the hardware shims, swaps in a virtual clock (so the speed slider can
fast-forward), routes print() to the simulator log, then executes code.py
exactly as CircuitPython would.

In CircuitPython strict mode, code.py also only sees what CircuitPython 9 has:
other imports fail, a few stdlib modules shrink to their CircuitPython API, and
time.monotonic() has CircuitPython's reduced float precision.
"""
import asyncio
import builtins
import struct
import importlib.abc
import importlib.util
import os
import selectors
import sys
import time as _real_time
import traceback
import types

import simhw


class SimStop(SystemExit):
    """Raised to unwind code.py when the simulator stops or restarts it."""


# Real-time cap on any single wait, so speed changes and Stop are noticed promptly.
_POLL_S = 0.05


class _ShimLoader(importlib.abc.Loader):
    def __init__(self, source):
        self._source = source

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        exec(compile(self._source, "<sim>/" + module.__name__ + ".py", "exec"), module.__dict__)


class _ShimFinder(importlib.abc.MetaPathFinder):
    def __init__(self, shims):
        self._shims = shims

    def find_spec(self, name, path, target=None):
        entry = self._shims.get(name)
        if entry is None:
            return None
        source, is_pkg = entry
        return importlib.util.spec_from_loader(name, _ShimLoader(source), is_package=is_pkg)


def _cp_float(value):
    """Round to CircuitPython's float: float32 with the low 2 mantissa bits
    dropped (30-bit objects), so long-uptime timers lose precision like the board."""
    bits = struct.unpack("<I", struct.pack("<f", value))[0] & ~0x3
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def _virtual_time_module(strict):
    module = types.ModuleType("time")
    for key in dir(_real_time):
        if not key.startswith("__"):
            setattr(module, key, getattr(_real_time, key))

    def monotonic():
        now = simhw.now()
        return _cp_float(now) if strict else now

    def monotonic_ns():
        return int(simhw.now() * 1_000_000_000)

    def sleep(seconds):
        # Blocking, like CircuitPython's time.sleep: nothing else runs meanwhile.
        end = simhw.now() + seconds
        while True:
            if simhw.stopping():
                raise SimStop()
            remaining = end - simhw.now()
            if remaining <= 0:
                return
            _real_time.sleep(min(remaining / simhw.speed(), _POLL_S))

    module.monotonic = monotonic
    module.monotonic_ns = monotonic_ns
    module.sleep = sleep
    return module


class _ScaledSelector(selectors.SelectSelector):
    """Shrinks the event loop's waits by the speed factor and watches for Stop."""

    loop = None
    _cancelled = False

    def select(self, timeout=None):
        if simhw.stopping() and not self._cancelled and self.loop is not None:
            self._cancelled = True
            for task in asyncio.all_tasks(self.loop):
                task.cancel()
            timeout = 0
        if timeout is None:
            timeout = _POLL_S
        else:
            timeout = min(timeout / simhw.speed(), _POLL_S)
        # Windows select() only wakes every ~15 ms, which would distort 1/60 s
        # animation steps; time.sleep uses a high-resolution timer. Nothing in
        # code.py does I/O through the loop, so sleeping first loses nothing.
        if timeout > 0:
            _real_time.sleep(timeout)
        return super().select(0)


class _SimLoop(asyncio.SelectorEventLoop):
    def time(self):
        return simhw.now()


class _SimPolicy(asyncio.DefaultEventLoopPolicy):
    def new_event_loop(self):
        selector = _ScaledSelector()
        loop = _SimLoop(selector)
        selector.loop = loop
        return loop


_original_wait_for = asyncio.wait_for


async def _wait_for(aw, timeout):
    """asyncio.wait_for that the "Trigger idle now" button can expire early."""
    if timeout is None:
        return await _original_wait_for(aw, None)
    loop = asyncio.get_running_loop()
    deadline = loop.time() + timeout
    task = asyncio.ensure_future(aw)
    try:
        while not task.done():
            remaining = deadline - loop.time()
            if remaining <= 0 or simhw.take_idle_trigger():
                task.cancel()
                raise asyncio.TimeoutError()
            await asyncio.wait({task}, timeout=min(remaining, 0.25))
    except BaseException:
        task.cancel()
        raise
    return task.result()


# Modules CircuitPython 9 provides on the RP2040 Prop-Maker Feather (core and
# frozen), plus the hardware modules the simulator provides. Libraries in the
# board's lib/ folder are added at run time.
_CP_MODULES = {
    "adafruit_bus_device", "adafruit_pixelbuf", "alarm", "analogbufio", "analogio",
    "array", "asyncio", "atexit", "audiobusio", "audiocore", "audiomixer", "audiomp3",
    "audiopwmio", "binascii", "bitbangio", "bitmaptools", "bitops", "board", "builtins",
    "busdisplay", "busio", "collections", "countio", "digitalio", "displayio", "epaperdisplay",
    "errno", "floppyio", "fontio", "fourwire", "framebufferio", "gc", "getpass", "gifio",
    "i2cdisplaybus", "i2ctarget", "imagecapture", "io", "jpegio", "json", "keypad", "locale",
    "math", "microcontroller", "micropython", "msgpack", "neopixel_write", "nvm",
    "onewireio", "os", "paralleldisplaybus", "pulseio", "pwmio", "qrio", "rainbowio",
    "random", "re", "rgbmatrix", "rotaryio", "rp2pio", "rtc", "sdcardio", "select",
    "sharpdisplay", "storage", "struct", "supervisor", "synthio", "sys", "terminalio",
    "time", "touchio", "traceback", "ulab", "usb", "usb_cdc", "usb_hid", "usb_midi",
    "usb_video", "vectorio", "warnings", "watchdog", "zlib",
}

# The CircuitPython API of stdlib modules whose CPython versions offer much more.
_CP_SURFACE = {
    "random": {"getrandbits", "seed", "randrange", "randint", "choice", "random", "uniform"},
    "time": {"monotonic", "monotonic_ns", "sleep", "struct_time", "localtime", "mktime",
             "time", "time_ns"},
    "os": {"chdir", "getcwd", "getenv", "listdir", "mkdir", "remove", "rename", "rmdir",
           "sep", "stat", "statvfs", "sync", "uname", "urandom", "utime"},
    "asyncio": {"run", "sleep", "sleep_ms", "create_task", "gather", "wait_for",
                "wait_for_ms", "Event", "Lock", "Task", "TimeoutError", "CancelledError",
                "current_task", "get_event_loop", "new_event_loop", "ThreadSafeFlag",
                "StreamReader", "StreamWriter", "open_connection", "start_server"},
}


def _cp_view(module, allowed):
    """A copy of `module` exposing only CircuitPython's names."""
    view = types.ModuleType(module.__name__)
    for name in allowed:
        if hasattr(module, name):
            setattr(view, name, getattr(module, name))

    def missing(name):
        raise AttributeError(
            "module '{}' has no attribute '{}' "
            "(not in CircuitPython 9; simulator strict mode)".format(module.__name__, name))

    view.__getattr__ = missing
    return view


def _strict_builtins(code_path):
    """Builtins for code.py whose import only allows CircuitPython modules."""
    allowed = set(_CP_MODULES)
    lib = os.path.join(os.path.dirname(code_path), "lib")
    if os.path.isdir(lib):
        for entry in os.listdir(lib):
            allowed.add(entry.split(".")[0])
    views = {}

    def cp_import(name, globals=None, locals=None, fromlist=(), level=0):
        top = name.split(".")[0]
        if level == 0 and top not in allowed:
            raise ImportError(
                "no module named '{}' (not in CircuitPython 9 or lib/; "
                "simulator strict mode)".format(name))
        module = builtins.__import__(name, globals, locals, fromlist, level)
        if name in _CP_SURFACE and level == 0:
            if name not in views:
                views[name] = _cp_view(module, _CP_SURFACE[name])
            for wanted in fromlist or ():
                if wanted != "*" and wanted not in _CP_SURFACE[name]:
                    raise ImportError(
                        "cannot import name '{}' from '{}' (not in CircuitPython 9; "
                        "simulator strict mode)".format(wanted, name))
            return views[name]
        return module

    gated = dict(builtins.__dict__)
    gated["__import__"] = cp_import
    return gated


class _LogStream:
    def write(self, text):
        simhw.log(text)
        return len(text)

    def flush(self):
        pass

    def isatty(self):
        return False


def run(code_path, shims):
    """Execute code.py with the shims installed. Never raises."""
    saved = {
        "stdout": sys.stdout,
        "stderr": sys.stderr,
        "time": sys.modules.get("time"),
        "cwd": os.getcwd(),
        "policy": asyncio.get_event_loop_policy(),
    }
    finder = _ShimFinder(shims)
    sys.stdout = sys.stderr = _LogStream()
    # Fresh copies of every shim each run, so no state leaks between runs.
    for name in shims:
        sys.modules.pop(name, None)
    sys.meta_path.insert(0, finder)
    strict = simhw.strict()
    sys.modules["time"] = _virtual_time_module(strict)
    asyncio.set_event_loop_policy(_SimPolicy())
    asyncio.wait_for = _wait_for
    try:
        os.chdir(os.path.dirname(code_path))
        print("[sim] running", code_path, "(CircuitPython strict)" if strict else "")
        with open(code_path, encoding="utf-8") as f:
            source = f.read()
        env = {"__name__": "__main__", "__file__": code_path}
        if strict:
            env["__builtins__"] = _strict_builtins(code_path)
        exec(compile(source, code_path, "exec"), env)
        print("\nCode done running.")
    except (SimStop, asyncio.CancelledError, KeyboardInterrupt):
        print("[sim] stopped")
    except BaseException:
        traceback.print_exc()
        print("\nCode done running.")
    finally:
        asyncio.wait_for = _original_wait_for
        asyncio.set_event_loop_policy(saved["policy"])
        if saved["time"] is not None:
            sys.modules["time"] = saved["time"]
        sys.meta_path.remove(finder)
        os.chdir(saved["cwd"])
        sys.stdout = saved["stdout"]
        sys.stderr = saved["stderr"]
