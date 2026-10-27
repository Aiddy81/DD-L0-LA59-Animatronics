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
import gc as _real_gc
import json
import linecache
import marshal
import struct
import importlib.abc
import importlib.util
import os
import selectors
import sys
import time as _real_time
import traceback
import tracemalloc
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
        filename = "<sim>/" + module.__name__ + ".py"
        # Lets tracebacks from a shim show its source lines.
        linecache.cache[filename] = (
            len(self._source), None, self._source.splitlines(True), filename)
        # What a stand-in allocates while importing is library code, which the
        # board's heap budget already excludes.
        before = _heap.stand_in_bytes()
        exec(compile(self._source, filename, "exec"), module.__dict__)
        _heap.import_bytes += _heap.stand_in_bytes() - before


def load_shim_dir(shim_dir, skipped=None):
    """Shims in a folder, as {module name: (source, is_package)}.

    foo.py -> "foo", pkg/__init__.py -> package "pkg", pkg/sub.py -> "pkg.sub".
    A missing folder gives {}. Files that can't be read are left out and, if
    `skipped` is a list, added to it as (module name, error)."""
    shims = {}
    if not shim_dir or not os.path.isdir(shim_dir):
        return shims
    for root, dirs, files in os.walk(shim_dir):
        dirs[:] = sorted(d for d in dirs if d != "__pycache__")
        for name in sorted(files):
            if not name.endswith(".py") or name == "sim_runtime.py":
                continue
            full = os.path.join(root, name)
            rel = os.path.relpath(full, shim_dir).replace(os.sep, "/")[:-3]
            is_pkg = rel == "__init__" or rel.endswith("/__init__")
            module = rel[: -len("/__init__")] if is_pkg else rel
            module = module.replace("/", ".")
            if not module or module == "__init__":
                continue
            try:
                with open(full, encoding="utf-8") as f:
                    shims[module] = (f.read(), is_pkg)
            except (OSError, UnicodeDecodeError) as e:
                if skipped is not None:
                    skipped.append((module, e))
    return shims


def _merge_disk_shims(shims, shim_dir):
    """Built-in shims with those in `shim_dir` on top (disk wins), logging each
    one taken from disk so an override is never silent. Never raises: on any
    problem it logs a traceback and keeps the built-ins."""
    if not shim_dir:
        return shims
    try:
        return _merge_disk_shims_unchecked(shims, shim_dir)
    except Exception:
        traceback.print_exc()
        print("[sim] could not read", shim_dir, "- using the built-in shims")
        return shims


def _merge_disk_shims_unchecked(shims, shim_dir):
    skipped = []
    disk = load_shim_dir(shim_dir, skipped)
    merged = dict(shims)
    where = os.path.join(os.path.basename(os.path.dirname(os.path.abspath(shim_dir))),
                         os.path.basename(os.path.abspath(shim_dir)), "")
    for module, source in disk.items():
        kind = "overrides built-in" if module in shims else "new"
        print("[sim] shim '{}' loaded from {} ({})".format(module, where, kind))
        merged[module] = source
    for module, error in skipped:
        print("[sim] shim '{}' in {} skipped: {}".format(module, where, error))
    return merged


class _ShimFinder(importlib.abc.MetaPathFinder):
    def __init__(self, shims):
        self._shims = shims

    def find_spec(self, name, path, target=None):
        entry = self._shims.get(name)
        if entry is None:
            return None
        source, is_pkg = entry
        return importlib.util.spec_from_loader(name, _ShimLoader(source), is_package=is_pkg)


class _FirmwareLoader(importlib.abc.Loader):
    """Runs a firmware module (a .py next to code.py) the way code.py runs:
    with CircuitPython's builtins, and counted on the heap gauge."""

    def __init__(self, path, builtins_):
        self._path = path
        self._builtins = builtins_

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        with open(self._path, encoding="utf-8") as f:
            code_obj = compile(f.read(), self._path, "exec")
        _heap.code_bytes += len(marshal.dumps(code_obj))
        module.__dict__["__builtins__"] = self._builtins
        exec(code_obj, module.__dict__)


class _FirmwareFinder(importlib.abc.MetaPathFinder):
    """Finds modules and packages in the firmware folder, as CircuitPython
    does from the drive root. Remembers what it loaded so each run starts fresh."""

    def __init__(self, fw_dir, builtins_):
        self._dir = fw_dir
        self._builtins = builtins_
        self.loaded = []

    def find_spec(self, name, path, target=None):
        base = os.path.join(self._dir, *name.split("."))
        init = os.path.join(base, "__init__.py")
        if os.path.isfile(init):
            file, locations = init, [base]
        elif os.path.isfile(base + ".py"):
            file, locations = base + ".py", None
        else:
            return None
        self.loaded.append(name)
        return importlib.util.spec_from_file_location(
            name, file, loader=_FirmwareLoader(file, self._builtins),
            submodule_search_locations=locations)


def _firmware_modules(fw_dir):
    """Top-level names importable from the firmware folder."""
    names = set()
    for entry in os.listdir(fw_dir):
        full = os.path.join(fw_dir, entry)
        if entry.endswith(".py") and entry != "code.py":
            names.add(entry[:-3])
        elif os.path.isfile(os.path.join(full, "__init__.py")) and entry != "lib":
            names.add(entry)
    return names


def _settings_value(text, default):
    """One settings.toml value: an integer or a "quoted string", like CircuitPython 9."""
    if text.startswith('"'):
        escaped = False
        for end in range(1, len(text)):
            if escaped:
                escaped = False
            elif text[end] == "\\":
                escaped = True
            elif text[end] == '"':
                try:
                    return json.loads(text[:end + 1])
                except ValueError:
                    return default
        return default
    try:
        return int(text.split("#", 1)[0].strip(), 0)
    except ValueError:
        return default


def _settings_getenv(fw_dir):
    """os.getenv as CircuitPython 9 has it: reads settings.toml next to code.py
    (top level only), never the desktop's environment."""
    path = os.path.join(fw_dir, "settings.toml")

    def getenv(key, default=None):
        try:
            with open(path, encoding="utf-8") as f:
                lines = f.readlines()
        except OSError:
            return default
        for line in lines:
            if line.lstrip().startswith("["):
                break
            name, sep, value = line.partition("=")
            if sep and name.strip() == key:
                return _settings_value(value.strip(), default)
        return default

    return getenv


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
        if simhw.paused():
            # Virtual time is frozen, so animations that yield with
            # asyncio.sleep(0) would otherwise spin a CPU core.
            _real_time.sleep(_POLL_S)
        simhw.charge("loop")
        _heap.sample()
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


class _HeapMeter:
    """Estimates code.py's heap: live objects allocated by code.py itself and by
    the hardware stand-ins it drives, plus code.py's compiled bytecode (which
    CircuitPython also keeps in RAM). Desktop-only overhead (imports, the event
    loop, text codecs) is left out, since the board's budget is measured after
    its libraries load. Rust scales the result to CircuitPython's smaller
    objects. Good for spotting "this won't fit", not for the last few KB."""

    def __init__(self):
        self.code_path = None
        self.code_bytes = 0
        self.import_bytes = 0
        self._last = 0.0
        self._fw_prefix = None
        self._lib_prefix = None

    def start(self, code_path, code_obj):
        self.code_path = code_path
        # Firmware modules add their own bytecode as they're imported.
        self.code_bytes = len(marshal.dumps(code_obj))
        self.import_bytes = 0
        self._last = 0.0
        fw_dir = os.path.dirname(code_path)
        self._fw_prefix = os.path.join(fw_dir, "")
        self._lib_prefix = os.path.join(fw_dir, "lib", "")
        tracemalloc.start(1)

    def _is_firmware(self, filename):
        return filename.startswith(self._fw_prefix) and not filename.startswith(self._lib_prefix)

    def _bytes(self):
        """(firmware bytes, stand-in bytes) currently live."""
        code = sim = 0
        for stat in tracemalloc.take_snapshot().statistics("filename"):
            name = stat.traceback[0].filename
            if self._is_firmware(name):
                code += stat.size
            elif name.startswith("<sim>/"):
                sim += stat.size
        return code, sim

    def stand_in_bytes(self):
        return self._bytes()[1] if self.code_path else 0

    def stop(self):
        self.code_path = None
        tracemalloc.stop()

    def sample(self, force=False):
        if self.code_path is None:
            return
        now = _real_time.monotonic()
        if not force and now - self._last < 2.0:
            return
        self._last = now
        code, sim = self._bytes()
        used = self.code_bytes + code + max(0, sim - self.import_bytes)
        simhw.set_heap(float(used))


_heap = _HeapMeter()


def _gc_module():
    """CircuitPython's gc, with mem_free()/mem_alloc() from the heap estimate."""
    module = types.ModuleType("gc")

    def collect():
        _real_gc.collect()
        _heap.sample(force=True)

    def mem_alloc():
        return int(simhw.heap()[0])

    def mem_free():
        used, budget = simhw.heap()
        return max(0, int(budget - used))

    module.collect = collect
    module.mem_alloc = mem_alloc
    module.mem_free = mem_free
    module.enable = _real_gc.enable
    module.disable = _real_gc.disable
    module.isenabled = _real_gc.isenabled
    module.threshold = lambda *args: None
    return module


class _NotSimulated:
    """Stands in for any object from a library the simulator doesn't model:
    accepts every call and attribute so the rest of code.py keeps running."""

    def __init__(self, path):
        object.__setattr__(self, "_path", path)

    def __getattr__(self, name):
        return _NotSimulated(self._path + "." + name)

    def __setattr__(self, name, value):
        pass

    def __call__(self, *args, **kwargs):
        return _NotSimulated(self._path + "()")

    def __getitem__(self, key):
        return _NotSimulated("{}[{!r}]".format(self._path, key))

    def __setitem__(self, key, value):
        pass

    def __iter__(self):
        return iter(())

    def __len__(self):
        return 0

    def __bool__(self):
        return False

    def __int__(self):
        return 0

    def __float__(self):
        return 0.0

    def __index__(self):
        return 0

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def __repr__(self):
        return "<not simulated: {}>".format(self._path)


def _not_simulated_module(name):
    module = types.ModuleType(name)
    module.__getattr__ = lambda attr: _NotSimulated(name + "." + attr)
    return module


def _os_module(strict, fw_dir):
    """os with CircuitPython's getenv (settings.toml); in strict mode only CircuitPython's API."""
    if strict:
        module = _cp_view(os, _CP_SURFACE["os"])
    else:
        module = types.ModuleType("os")
        for key in dir(os):
            if not key.startswith("__"):
                setattr(module, key, getattr(os, key))
    module.getenv = _settings_getenv(fw_dir)
    return module


def _code_builtins(code_path, strict, shims):
    """Builtins for code.py and the firmware modules beside it. Their imports
    get CircuitPython's gc and os.getenv, a stand-in for board libraries the
    simulator doesn't model, and, in strict mode, only CircuitPython 9's
    modules and APIs."""
    fw_dir = os.path.dirname(code_path)
    # Stand-ins model libraries the board has, so they're always importable.
    board_modules = set(_CP_MODULES) | {name.split(".")[0] for name in shims}
    board_modules |= _firmware_modules(fw_dir)
    lib = os.path.join(fw_dir, "lib")
    if os.path.isdir(lib):
        for entry in os.listdir(lib):
            board_modules.add(entry.split(".")[0])
    views = {"gc": _gc_module(), "os": _os_module(strict, fw_dir)}
    stubs = {}

    def cp_import(name, globals=None, locals=None, fromlist=(), level=0):
        top = name.split(".")[0]
        if level == 0 and name == "gc":
            return views["gc"]
        if level == 0 and name == "os":
            for wanted in fromlist or ():
                if strict and wanted != "*" and wanted not in _CP_SURFACE["os"]:
                    raise ImportError(
                        "cannot import name '{}' from 'os' (not in CircuitPython 9; "
                        "simulator strict mode)".format(wanted))
            return views["os"]
        if strict and level == 0 and top not in board_modules:
            raise ImportError(
                "no module named '{}' (not in CircuitPython 9 or lib/; "
                "simulator strict mode)".format(name))
        try:
            module = builtins.__import__(name, globals, locals, fromlist, level)
        except ModuleNotFoundError as e:
            missing = (e.name or "").split(".")[0]
            if level != 0 or missing != top or top not in board_modules:
                raise
            simhw.unsimulated(top)
            wanted = name if fromlist else top
            if wanted not in stubs:
                stubs[wanted] = _not_simulated_module(wanted)
            return stubs[wanted]
        if strict and name in _CP_SURFACE and level == 0:
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


def run(code_path, shims, shim_dir=None):
    """Execute code.py with the shims installed. Never raises.

    `shims` are the built-in stand-ins; any in `shim_dir` (read afresh each
    run) override them or add new ones."""
    saved = {
        "stdout": sys.stdout,
        "stderr": sys.stderr,
        "time": sys.modules.get("time"),
        "cwd": os.getcwd(),
        "policy": asyncio.get_event_loop_policy(),
    }
    firmware = None
    sys.stdout = sys.stderr = _LogStream()
    strict = simhw.strict()
    print("[sim] running", code_path, "(CircuitPython strict)" if strict else "")
    shims = _merge_disk_shims(shims, shim_dir)
    finder = _ShimFinder(shims)
    # Fresh copies of every shim each run, so no state leaks between runs.
    for name in shims:
        sys.modules.pop(name, None)
    sys.meta_path.insert(0, finder)
    sys.modules["time"] = _virtual_time_module(strict)
    asyncio.set_event_loop_policy(_SimPolicy())
    asyncio.wait_for = _wait_for
    try:
        os.chdir(os.path.dirname(code_path))
        with open(code_path, encoding="utf-8") as f:
            source = f.read()
        code_builtins = _code_builtins(code_path, strict, shims)
        firmware = _FirmwareFinder(os.path.dirname(code_path), code_builtins)
        sys.meta_path.insert(1, firmware)
        env = {"__name__": "__main__", "__file__": code_path, "__builtins__": code_builtins}
        code_obj = compile(source, code_path, "exec")
        _heap.start(code_path, code_obj)
        exec(code_obj, env)
        print("\nCode done running.")
    except (SimStop, asyncio.CancelledError, KeyboardInterrupt):
        print("[sim] stopped")
    except BaseException:
        traceback.print_exc()
        print("\nCode done running.")
    finally:
        _heap.stop()
        asyncio.wait_for = _original_wait_for
        asyncio.set_event_loop_policy(saved["policy"])
        if saved["time"] is not None:
            sys.modules["time"] = saved["time"]
        sys.meta_path.remove(finder)
        # A shim deleted from disk must not linger into the next run.
        for name in shims:
            sys.modules.pop(name, None)
        if firmware is not None:
            sys.meta_path.remove(firmware)
            # Fresh firmware modules each run, so edits and state don't carry over.
            for name in firmware.loaded:
                sys.modules.pop(name, None)
        os.chdir(saved["cwd"])
        sys.stdout = saved["stdout"]
        sys.stderr = saved["stderr"]
