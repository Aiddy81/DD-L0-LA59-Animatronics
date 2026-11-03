"""Turns logging mode off. In the serial console press Ctrl-C, then type:
import log_off
The board restarts with CIRCUITPY editable from your PC again."""
import os

import microcontroller

try:
    os.remove("/log_mode")
except OSError:
    print("Logging mode is already off (no log_mode file).")
else:
    print("Logging mode off. Restarting...")
    microcontroller.reset()
