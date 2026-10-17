"""Runs code.py inside the simulator.

Installs the hardware shims, swaps in a virtual clock (so the speed slider can
fast-forward), routes print() to the simulator log, then executes code.py
exactly as CircuitPython would.
"""
import asyncio
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


def _virtual_time_module():
    module = types.ModuleType("time")
    for key in dir(_real_time):
        if not key.startswith("__"):
            setattr(module, key, getattr(_real_time, key))

    def monotonic():
        return simhw.now()

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
    sys.modules["time"] = _virtual_time_module()
    asyncio.set_event_loop_policy(_SimPolicy())
    asyncio.wait_for = _wait_for
    try:
        os.chdir(os.path.dirname(code_path))
        print("[sim] running", code_path)
        with open(code_path, encoding="utf-8") as f:
            source = f.read()
        exec(compile(source, code_path, "exec"), {"__name__": "__main__", "__file__": code_path})
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
