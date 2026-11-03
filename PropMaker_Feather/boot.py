"""Runs once at power-on/reset, before code.py.

Logging mode: while a file named log_mode is on CIRCUITPY, let the firmware
write log.txt. Your PC then sees the drive read-only (you can still copy the
log off). To edit files again, turn it off from the serial console: Ctrl-C,
then type: import log_off
"""
import os

import storage

try:
    os.stat("/log_mode")
except OSError:
    pass
else:
    storage.remount("/", readonly=False)
