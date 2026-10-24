"""Serial output, optionally also saved to log.txt on CIRCUITPY.

The serial console can't be watched while the wings run, so with a file named
log_mode on the drive every log() line is also appended to log.txt, stamped
with seconds since power-on. boot.py makes the drive writable in that mode.
"""
import os
import time

LOG_FILE = "/log.txt"
OLD_LOG_FILE = "/log.old.txt"
# Roll over to log.old.txt at this size, so the log never crowds out the sounds.
MAX_BYTES = 256 * 1024


def _exists(path):
    try:
        os.stat(path)
        return True
    except OSError:
        return False


_enabled = _exists("/log_mode")
_size = None


def log(*args):
    global _enabled, _size
    print(*args)
    if not _enabled:
        return
    line = "{:.1f} {}\n".format(time.monotonic(), " ".join(str(a) for a in args))
    try:
        if _size is None:
            _size = os.stat(LOG_FILE)[6] if _exists(LOG_FILE) else 0
        if _size + len(line) > MAX_BYTES:
            if _exists(OLD_LOG_FILE):
                os.remove(OLD_LOG_FILE)
            os.rename(LOG_FILE, OLD_LOG_FILE)
            _size = 0
        # Opened per line so a log survives the battery being pulled.
        with open(LOG_FILE, "a") as f:
            f.write(line)
        _size += len(line)
    except OSError as e:
        # Read-only (log_mode added without a hard reset) or the drive is full.
        _enabled = False
        print("Log file off:", e)
