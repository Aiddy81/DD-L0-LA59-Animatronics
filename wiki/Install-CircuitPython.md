# Install CircuitPython

> **You'll need:** the Prop-Maker Feather, a USB-C **data** cable, a computer · **Time:** 10 minutes

CircuitPython is the small programming language that runs Lola's code. You only do this once per board.

## Steps

1. Go to the [CircuitPython page for the Prop-Maker Feather](https://circuitpython.org/board/adafruit_feather_rp2040_prop_maker/).
2. Download the newest **9.x** version (a `.uf2` file). **Not 10.x** — Lola's code is written for version 9.
3. Plug the Feather into your computer with the USB cable.
4. Hold down the **BOOT** button on the Feather.
5. While holding it, press and let go of the **RESET** button.
6. Let go of **BOOT**. A drive called **RPI-RP2** appears on your computer.
7. Drag the `.uf2` file onto **RPI-RP2**.
8. Wait. The drive disappears and a new one called **CIRCUITPY** appears.

### ✅ It should look like this

A drive called **CIRCUITPY** with a file called `boot_out.txt` in it. Open that file: the first line says `Adafruit CircuitPython 9.…`.

## ⚠️ Something went wrong?

| Problem | Fix |
|---|---|
| No drive appears at all | Try another cable: many only charge. Try another USB port. |
| **RPI-RP2** doesn't appear | Hold **BOOT** *before* pressing **RESET**, and keep holding until you've let go of **RESET**. |
| **CIRCUITPY** never appears after copying | Wait 30 seconds. Unplug and plug back in. Check you downloaded the file for the *Prop-Maker* Feather, not another board. |

## More detail (for tinkerers)

- The robot was last tested on **CircuitPython 9.2.8**. Upgrading to 10.x isn't supported yet: libraries and `settings.toml` handling may differ.
- The `.mpy` libraries in `PropMaker_Feather/lib/` are compiled for CircuitPython 9. If you ever update them, get the 9.x bundle from [circuitpython.org/libraries](https://circuitpython.org/libraries).

Next: [Copy the firmware](Copy-the-Firmware).
