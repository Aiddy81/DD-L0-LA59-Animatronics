# Writing custom shims for the simulator

This guide shows how to add or change the Python stand-ins ("shims") that the simulator uses in place of CircuitPython hardware libraries. You only need Python and a text editor. No Rust, Docker or rebuild is involved: new LEDs, pixel strips, moving parts and buttons are listed in `simulator/parts.toml` and driven through `simhw`. Only a new *kind* of output, such as a display, needs Rust (see [step 8](#8-when-a-shim-needs-rust-changes)).

## 1. What a shim is, and when you need one

On the robot, `code.py` imports libraries such as `board`, `pwmio` or `adafruit_servokit`, and they talk to real chips. On a PC there are no chips, so the simulator swaps each of these libraries for a **shim**: a small Python module with the same names (classes, functions, constants) as the real library. Instead of driving hardware, a shim reports to the simulator, which moves the wings, lights the eyes, plays sounds and so on.

The simulator ships with built-in shims for everything the firmware uses today: `board`, `digitalio`, `busio`, `pwmio`, `audiocore`, `audiomixer`, `audiobusio`, `adafruit_motor`, `adafruit_servokit` and `adafruit_lis3dh`. Their source is in [`simulator/shims/`](../shims/).

If the firmware imports a board library that has **no** shim (one that is in `PropMaker_Feather/lib/` or built into CircuitPython, such as `neopixel` or `microcontroller`), the simulator doesn't crash. It hands `code.py` an automatic **"not simulated" placeholder** that accepts every call and does nothing, logs `[sim] neopixel is not simulated; calls to it do nothing`, and lists the library under *Not simulated* in the Board panel.

The placeholder keeps the rest of the firmware running, but it can't tell you anything: every value it returns is `0`, `False` or empty. Write a shim when you want to:

- see what the firmware does with a library (log the calls),
- feed the firmware realistic values (a button, a sensor),
- drive an output the simulator already shows (servos and LEDs on the PCA9685, the white LED), or a part you add in `simulator/parts.toml` (an LED, a NeoPixel strip or ring, a servo-turned shape, a button),
- change how a built-in shim behaves without touching the simulator's source.

## 2. Where shim files go

Put shims in a folder called `shims` next to the simulator exe:

```
simulator\dist\
    ddl0-sim.exe
    shims\                    <- create this folder
        adafruit_debouncer.py     a module:   import adafruit_debouncer
        adafruit_lis3dh.py        overrides the built-in adafruit_lis3dh
        mylib\                    a package:  import mylib
            __init__.py
            extra.py              a sub-module: import mylib.extra
```

The rules:

- The file name is the module name: `foo.py` is `import foo`. A folder with an `__init__.py` is a package, and other `.py` files in it are its sub-modules.
- A shim with the same name as a built-in one **replaces** it. Anything else **adds** a new one. Built-ins you don't override keep working, so with no `shims` folder the simulator behaves exactly as it always has.
- The folder is read at the start of **every run**, so you never need to restart the app. With *Restart when firmware is saved* ticked, adding, saving or deleting a shim restarts the run on its own; otherwise press **R** (or *Restart*).
- Rebuilding the simulator with `build.ps1` keeps the `shims` folder.
- Every shim taken from the folder is logged at the start of the run, so an override is never silent:

  ```
  [sim] shim 'adafruit_debouncer' loaded from dist\shims\ (new)
  [sim] shim 'adafruit_lis3dh' loaded from dist\shims\ (overrides built-in)
  ```

- Shims count as board libraries in CircuitPython strict mode, so `code.py` may import them. On the real robot the library must also be in `PropMaker_Feather/lib/`, or the import fails there.
- Files must be UTF-8. One that can't be read is skipped (the built-in, if any, is used) and the log says why.

## 3. Mirror only what the firmware uses

A shim copies the real library's **API surface**: the class names, constructor arguments, methods, properties and constants. It does **not** need the whole library. The project rule (see `CLAUDE.md`) is:

> Mirror the real CircuitPython/Adafruit API surface that `code.py` uses, nothing more.

So:

1. Find every use of the library in `PropMaker_Feather/` (search for its name).
2. Look each one up in the real library's documentation so names, argument names, defaults and value ranges match. The API references are on Read the Docs, for example:
   - CircuitPython core modules (`board`, `pwmio`, `digitalio`, ...): <https://docs.circuitpython.org/en/latest/shared-bindings/index.html>
   - Adafruit libraries: `https://docs.circuitpython.org/projects/<name>/en/latest/api.html`, e.g. [`adafruit_debouncer`](https://docs.circuitpython.org/projects/debouncer/en/latest/api.html), [`adafruit_pca9685`](https://docs.circuitpython.org/projects/pca9685/en/latest/api.html), [`adafruit_lis3dh`](https://docs.circuitpython.org/projects/lis3dh/en/latest/api.html)
   - The source of each Adafruit library is on GitHub (`github.com/adafruit/Adafruit_CircuitPython_<Name>`), handy for the exact maths.
3. Write just those names. Raise the same errors the real library does for bad values (for example `ValueError` for an out-of-range duty cycle), so firmware bugs show up in the simulator too.

If the firmware later uses more of the library, add it then.

## 4. Talking to the simulator: `simhw`

Shims reach the simulator only through the built-in `simhw` module (`import simhw`). These are the calls a shim can use:

| Call | What it does |
|---|---|
| `simhw.set_pca(channel, duty)` | Sets PCA9685 channel `channel` (0-15) to `duty` (0-65535). Channels 0-1 are the wing servos and 2-7 the eye RGB LEDs (mapping in `simulator/sim.toml`); they move the 3D model. Channels 8-15 aren't drawn unless a part in `simulator/parts.toml` follows them, and appear in the PCA9685 table once written. |
| `simhw.set_pwm_pin(pin_name, duty, frequency=500)` | A `pwmio` pin's duty (0-65535) at `frequency` Hz. `"EXTERNAL_NEOPIXELS"` is the white LED and lights the model; any pin can drive a part in `simulator/parts.toml` (see the README's [Parts file](../../README.md#parts-file)). |
| `simhw.set_pin(pin_name, value)` | A digital output. `"LED"` (onboard LED) and `"EXTERNAL_POWER"` show in the PCA9685 panel; any pin can drive a part in `simulator/parts.toml`. |
| `simhw.pin_input(pin_name)` | Whether the `button` part on that input pin is pressed, or `None` when no button is on it. The `digitalio` shim uses it so `DigitalInOut.value` reads the button, inverted with `Pull.UP`. |
| `simhw.set_part(name, value)` | Drives the part called `name` in `simulator/parts.toml`. What `value` means depends on the part's `kind`: brightness `0.0`-`1.0` (or `True`/`False` for on/off) for an `led` or `pixels` part, `(r, g, b)` with each `0`-`255` for an `led`, or the servo angle in degrees for a `servo_part` (turned through its `hinge` like the wings). The part must have no `pca` or `pin` (those follow their source instead). |
| `simhw.set_pixels(name, [(r, g, b), ...])` | Sets each pixel of a `pixels` part, `0`-`255` per channel: the values `neopixel` holds after its `brightness` is applied. Pixel 0 is the first in the row or ring. Missing pixels are dark, extra ones aren't drawn; at most 1024. |
| `simhw.part_input(name)` | Whether the `button` part called `name` is pressed (clicked on the model), `True` or `False`. Works for any button, with or without a `pin`. |
| `simhw.accel()` | The next accelerometer reading as `(x, y, z)` in **g** (not m/s²), from the Motion controls (*Nudge*, *Shake*, *Tipped over*, *held z*) or a click on the droid. |
| `simhw.accel_range(g)` | Tells the simulator the accelerometer's full-scale range in g (2, 4, 8 or 16), so the plot can draw its clip level. Call it when the range is set. The shim still clips the readings itself. |
| `simhw.play_wav(path, level, looped)` | Plays a WAV file on the PC's speakers at `level` (0.0-1.0), optionally looping. |
| `simhw.stop_audio()` | Stops all playback. |
| `simhw.set_volume(level)` | Changes the playing sound's level (0.0-1.0). |
| `simhw.charge(kind)` | Adds the real board's cost of one hardware operation when *Board speed* is on: `"pca"` (an I2C write to the PCA9685), `"pwm"` (a duty write), `"accel"` (a sensor read) or `"loop"`. Times come from `[board]` in `sim.toml`. Call it where the real library would talk to hardware. |
| `simhw.now()` | Virtual time in seconds, which follows the Speed slider and Pause. It's what `time.monotonic()` returns to the firmware. |
| `simhw.speed()` / `simhw.paused()` | The Speed slider value, and whether the run is paused. |
| `simhw.stopping()` | `True` once the user stops or restarts the run. Check it in any loop that waits, and raise `SystemExit` to unwind. |
| `simhw.strict()` | Whether CircuitPython strict mode is on. |
| `simhw.unsimulated(name)` | Marks `name` as not simulated: logs it once and lists it in the Board panel. Useful for parts of a library your shim doesn't model. |
| `simhw.log(text)` | Writes raw text to the log. `print()` in a shim does the same. Start a line with `[sim]` so it's filtered with the simulator's own messages rather than the firmware's. |

`set_heap`, `heap` and `take_idle_trigger` also exist, but are used by the runtime itself, not by shims.

The three part calls never raise. A name that isn't in `parts.toml`, a part of the wrong kind, a part that follows a `pca` or `pin`, or a bad value (wrong type, NaN, too many pixels) logs one warning per name per run and is otherwise ignored, for example:

```
[sim] simhw.set_pixels("ring"): no part has this name in parts.toml; ignored
```

`parts.toml` is read again on every restart, so a part added to it is there for the next run. A part placed with the mouse in the **Parts** panel (see the README's [Placing parts with the mouse](../../README.md#placing-parts-with-the-mouse)) is there at once, and a shim can drive it straight away.

For waits, use `time.sleep()` and `time.monotonic()` as the firmware does. Inside the simulator they are already the virtual clock, so they follow Speed and Pause.

## 5. Model the hardware maths faithfully

The point of the simulator is to show what the robot will **really** do, so a shim should do the same sums as the real library and chip, not take shortcuts. The servo path is a good example.

A hobby servo reads a pulse every 20 ms (50 Hz). Its angle depends on the pulse width: by default `adafruit_motor.servo` maps 0° to 750 µs and 180° to 2250 µs. The PCA9685 doesn't take microseconds; it takes a **duty cycle**, the fraction of the 20 ms period the pulse is high, scaled to 0-65535:

```
duty = pulse_us × frequency / 1,000,000 × 65535
```

Setting `servo.angle = 90` on a 50 Hz channel:

| Step | Sum | Result |
|---|---|---|
| Duty at 750 µs (0°) | `int(750 × 50 / 1e6 × 65535)` | 2457 |
| Duty at 2250 µs (180°) | `2250 × 50 / 1e6 × 65535` | 7372.7 |
| Duty range | `int(7372.7 − 2457)` | 4915 |
| 90° is half the range | `2457 + int(0.5 × 4915)` | 4914 |
| PCA9685 is 12-bit: low 4 bits dropped | `(4914 >> 4) << 4` | **4912** |
| Pulse the servo sees | `4912 / 65535 × 20,000 µs` | 1499.1 µs |

Doing the same steps means rounding, clipping and range errors appear in the simulator exactly as on the robot. The same applies elsewhere: the built-in LIS3DH shim clips readings at its configured range (about 4 g for `RANGE_4_G`), so a hard shake reads the same as on the real sensor.

## 6. Worked examples

Each example below has been run against a test `code.py` in strict mode. Copy a file into `simulator\dist\shims\` and restart the run.

### Example A: a shim that only logs calls (`adafruit_debouncer`)

Say you add a button with [`adafruit_debouncer`](https://docs.circuitpython.org/projects/debouncer/en/latest/api.html). Until it has a shim, the button reads as the placeholder's `False` and nothing explains why. This shim logs how the firmware uses it and reports a button that is never pressed:

```python
# simulator\dist\shims\adafruit_debouncer.py
"""Stand-in for adafruit_debouncer: logs how the firmware uses it, never
reports a press. Mirrors only Debouncer(io, interval), update(), value,
rose and fell."""


class Debouncer:
    def __init__(self, io, interval=0.010):
        self._io = io
        self.interval = interval
        self._updates = 0
        print("[sim] Debouncer created, interval", interval)

    def update(self, new_value=None):
        self._updates += 1
        if self._updates == 1:
            print("[sim] Debouncer.update() called; button presses are not simulated")

    @property
    def value(self):
        return True  # a pulled-up button that is never pressed

    @property
    def rose(self):
        return False

    @property
    def fell(self):
        return False
```

It logs once per run rather than on every `update()`, so the log stays readable when the firmware polls the button many times a second.

### Example B: driving an existing output (a servo on PCA9685 channel 8)

The firmware drives the PCA9685 through `adafruit_servokit`, which has a built-in shim. If you add a part that uses the lower-level [`adafruit_pca9685`](https://docs.circuitpython.org/projects/pca9685/en/latest/api.html) library directly (it's already in `PropMaker_Feather/lib/`), for example a tail servo on a spare channel:

```python
# in the firmware
import board, busio
import adafruit_pca9685
from adafruit_motor import servo

pca = adafruit_pca9685.PCA9685(busio.I2C(board.SCL, board.SDA))
pca.frequency = 50
tail = servo.Servo(pca.channels[8])
tail.angle = 90
```

this shim sends each channel's duty to the simulator, with the chip's 12-bit resolution and I2C cost:

```python
# simulator\dist\shims\adafruit_pca9685.py
"""Stand-in for adafruit_pca9685 on the simulator's PCA9685. Mirrors only
PCA9685(i2c), .frequency, .channels[n].duty_cycle and deinit()."""
import simhw


class PWMChannel:
    def __init__(self, pca, index):
        self._pca = pca
        self._index = index
        self._duty = 0

    @property
    def frequency(self):
        return self._pca.frequency

    @property
    def duty_cycle(self):
        return self._duty

    @duty_cycle.setter
    def duty_cycle(self, value):
        if not 0 <= value <= 0xFFFF:
            raise ValueError("Out of range")
        # 12-bit chip: the low 4 bits are lost, except 0xFFFF (fully on).
        self._duty = 0xFFFF if value == 0xFFFF else (value >> 4) << 4
        simhw.set_pca(self._index, self._duty)  # shows in the PCA9685 table
        simhw.charge("pca")                      # costs one I2C write of board time


class PCA9685:
    def __init__(self, i2c_bus, *, address=0x40, reference_clock_speed=25000000):
        self.frequency = 50
        self.channels = [PWMChannel(self, i) for i in range(16)]

    def deinit(self):
        pass
```

`adafruit_motor.servo` (built in) does the angle maths, so channel 8 shows as a *spare* row in the PCA9685 table with 7.5% duty and a 1499 µs pulse, as worked out in [step 5](#5-model-the-hardware-maths-faithfully). To see the tail on the 3D model too, add a `servo_part` with `pca = 8` to `simulator/parts.toml` (see the README's [Parts file](../../README.md#parts-file)).

The same pattern works for a `pwmio` pin: call `simhw.set_pwm_pin(pin.name, duty)`, as the built-in [`pwmio.py`](../shims/pwmio.py) does.

### Example C: overriding a built-in shim (`adafruit_lis3dh` with taps)

The built-in LIS3DH shim always reports `tapped` as `False`. To try tap detection:

1. Copy `simulator\shims\adafruit_lis3dh.py` to `simulator\dist\shims\adafruit_lis3dh.py`. Always start an override from the built-in, so everything else the firmware uses keeps working.
2. In `LIS3DH.__init__`, after `self._range = RANGE_4_G`, add:

   ```python
           self._tap = 0
           self._tap_threshold = 0
   ```

3. Replace `tapped` and `set_tap` with:

   ```python
       @property
       def tapped(self):
           # Simplified: a "tap" is any reading more than `threshold` counts away
           # from 1 g. The real chip latches clicks in hardware.
           if not self._tap:
               return False
           x, y, z = simhw.accel()
           counts = abs(math.sqrt(x * x + y * y + z * z) - 1) * 128 / _FULL_SCALE_G[self._range]
           return counts > self._tap_threshold

       def set_tap(self, tap, threshold, *, time_limit=10, time_latency=20,
                   time_window=255, click_cfg=None):
           self._tap = tap
           self._tap_threshold = threshold
   ```

The log shows `shim 'adafruit_lis3dh' loaded from dist\shims\ (overrides built-in)`. With `set_tap(1, 60)`, *Shake* (3 g) reads as a tap and *Nudge* doesn't. Delete the file to go back to the built-in.

### Example D: a NeoPixel ring and a touch pad, with no Rust (`neopixel`, `touchio`)

`neopixel` is "not simulated" out of the box. This example adds a ring of 12 NeoPixels around the droid's gear band and a touch pad on its front, and two shims that drive them. Say the firmware does this:

```python
# in the firmware
import time
import board
import neopixel
import touchio

ring = neopixel.NeoPixel(board.D10, 12, brightness=0.3, auto_write=False)
pad = touchio.TouchIn(board.A1)

while True:
    for i in range(ring.n):
        ring.fill(0)
        ring[i] = (255, 0, 0) if pad.value else 0x0050FF  # red while touched
        ring.show()
        time.sleep(0.1)
```

**1. Add the parts.** The quickest way is the **Parts** panel: drag `pixels` and `button` onto the droid, then in the settings rename them `band_pixels` and `touch_pad`, set the pixels' layout to *ring* and count to 12, and leave *Source* on *Shim only*. The panel writes `simulator/parts.toml` for you. (The entries below centre the ring inside the droid so it circles the gear band; type that into *Position* to match.) Or copy `simulator/parts.example.toml` to `simulator/parts.toml` (or start an empty one) and add the two entries by hand. Neither has a `pca` or `pin`, so they are left to shims:

```toml
# simulator/parts.toml
[[part]]
name = "band_pixels"
kind = "pixels"
layout = "ring"
count = 12
spacing = 0.6
pos = [0, 1.42, 0]
size = 0.04

[[part]]
name = "touch_pad"
kind = "button"
pos = [0, 1.38, 1.18]
rot_deg = [90, 0, 0]
size = 0.07
colour = [0.2, 0.6, 1.0]
```

**2. Add the `neopixel` shim.** It keeps the pixels the way the real library does (unscaled, so reading `ring[i]` back gives what was written) and applies `brightness` when it sends them with `simhw.set_pixels`:

```python
# simulator\dist\shims\neopixel.py
"""Stand-in for neopixel that draws on a `pixels` part in parts.toml. Mirrors
NeoPixel(pin, n, *, bpp, brightness, auto_write, pixel_order), pixels[i] and
slices, fill(), show(), brightness, n and deinit()."""
import simhw

# The pixels part each data pin drives; the names must match parts.toml.
PARTS = {"D10": "band_pixels"}

RGB = "RGB"
GRB = "GRB"
RGBW = "RGBW"
GRBW = "GRBW"


def _colour(value):
    """A pixel as (r, g, b): from a packed 0xRRGGBB int or a tuple."""
    if isinstance(value, int):
        return ((value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF)
    if len(value) not in (3, 4):
        raise ValueError("Expected tuple of length 3 or 4")
    return tuple(value[:3])  # an RGBW pixel's white channel isn't drawn


class NeoPixel:
    def __init__(self, pin, n, *, bpp=3, brightness=1.0, auto_write=True,
                 pixel_order=None):
        self.n = n
        self.auto_write = auto_write
        self._part = PARTS.get(pin.name, pin.name)
        self._pixels = [(0, 0, 0)] * n
        self._brightness = min(max(brightness, 0.0), 1.0)

    def __len__(self):
        return self.n

    def __getitem__(self, index):
        return self._pixels[index]

    def __setitem__(self, index, value):
        if isinstance(index, slice):
            for i, v in zip(range(*index.indices(self.n)), value):
                self._pixels[i] = _colour(v)
        else:
            self._pixels[index] = _colour(value)
        if self.auto_write:
            self.show()

    @property
    def brightness(self):
        return self._brightness

    @brightness.setter
    def brightness(self, value):
        self._brightness = min(max(value, 0.0), 1.0)
        if self.auto_write:
            self.show()

    def fill(self, color):
        self._pixels = [_colour(color)] * self.n
        if self.auto_write:
            self.show()

    def show(self):
        b = self._brightness
        simhw.set_pixels(self._part, [tuple(c * b for c in p) for p in self._pixels])

    def deinit(self):
        self._pixels = [(0, 0, 0)] * self.n
        self.show()
```

**3. Add the `touchio` shim.** A touch pad reads `True` while the `touch_pad` part is clicked on the model:

```python
# simulator\dist\shims\touchio.py
"""Stand-in for touchio that reads a `button` part in parts.toml. Mirrors
TouchIn(pin), value, threshold and deinit()."""
import simhw

# The button part each touch pin reads; the names must match parts.toml.
PARTS = {"A1": "touch_pad"}


class TouchIn:
    def __init__(self, pin):
        self._part = PARTS.get(pin.name, pin.name)
        self.threshold = 0

    @property
    def value(self):
        return simhw.part_input(self._part)

    def deinit(self):
        pass
```

**4. Restart the run (R).** The log shows both shims loaded from `dist\shims\` and `[sim] parts.toml: 2 parts loaded`. A blue dot chases round the ring and turns red while you hold the mouse down on the touch pad; the PCA9685 panel lists `band_pixels` (`1/12 lit`) and `touch_pad` (`pressed` / `released`). This works on both the Lola and Primitive models.

If a name doesn't match (say the firmware uses `board.D11`, which isn't in `PARTS`), the shim sends to a part called `D11` and the log says so once per run, `[sim] simhw.set_pixels("D11"): no part has this name in parts.toml; ignored`, while the rest of the firmware keeps running. Add the pin to `PARTS` or rename the part to fix it.

`simulator/tests/shim_dir.py` runs these two shims straight from this page, so CI catches it if they stop working.

## 7. Testing a shim

1. **Run it.** Save the shim; the run restarts if *Restart when firmware is saved* is ticked, otherwise press **R**.
2. **Check the log.** The first lines should include `[sim] shim '<name>' loaded from dist\shims\`. If they don't, check the file name and folder. Your own `[sim]` messages appear with the *[sim]* log toggle on.
3. **Check the Board panel.** If the library still appears under *Not simulated*, the firmware imported a name the shim doesn't have (a sub-module, for example) or the shim isn't being found.
4. **Errors.** A mistake in a shim, including a syntax error, shows as a red traceback in the log, pointing at `<sim>/<name>.py` and the line. The app keeps running: fix the file and save.

   ```
     File "<sim>/adafruit_debouncer.py", line 7
       def __init__(self, io, interval=0.010)
                                             ^
   SyntaxError: expected ':'
   ```

5. **Strict mode.** With *CircuitPython strict* on, `code.py` only sees CircuitPython 9's modules and APIs. Shims always count as board libraries, but the code **inside** a shim runs as normal desktop Python, so it can't hide firmware that uses something CircuitPython lacks. A strict-mode error names the module or attribute, e.g. `no module named 'foo' (not in CircuitPython 9 or lib/; simulator strict mode)`.
6. **Run the behaviour checks** if the shim touches anything the firmware already uses: boot, breathing, nudge → boop, 3 nudges or a hard shake → angry, *Trigger idle* → attention.

To make a drop-in shim permanent for everyone, move it into `simulator/shims/` and add it to the `SHIMS` table at the top of `simulator/src/pyhost.rs`, then rebuild. The headless test (`python simulator/tests/headless.py`) loads everything in `simulator/shims/`, so it's covered in CI from then on.

## 8. When a shim needs Rust changes

Most new hardware needs no Rust. The simulator has four general part kinds, listed in `simulator/parts.toml` (see the README's [Parts file](../../README.md#parts-file)):

| Kind | For | Driven by |
|---|---|---|
| `led` | a single LED, one colour or RGB | a PCA9685 channel, a pin, or `simhw.set_part` |
| `pixels` | a NeoPixel strip or ring | a PCA9685 channel or pin (all one brightness), or `simhw.set_pixels` / `set_part` |
| `servo_part` | a flap, tail or anything else a servo turns about a hinge | a PCA9685 channel, a pin, or `simhw.set_part` |
| `button` | a button, switch or touch pad (an input) | the mouse; read with `digitalio` (`pin = ...`) or `simhw.part_input` |

So a shim for new hardware is usually a parts entry (dragged on from the **Parts** panel, or written by hand) plus the calls in [step 4](#4-talking-to-the-simulator-simhw), as in [Example D](#example-d-a-neopixel-ring-and-a-touch-pad-with-no-rust-neopixel-touchio).

Only a new **kind** of output, one that none of these four can show (a display, for example), needs the simulator itself changing:

1. **`simulator/src/hw.rs`**: add a field to the shared hardware state (`HwState`) to hold the new output, and reset it in `reset_for_run`.
2. **`simulator/src/pyhost.rs`**: add a `#[pyfunction]` to the `simhw` module that writes that field, like `set_pca` or `set_part`. Check every argument Python passes (types, NaN, sizes) and log a `[sim]` warning rather than raising or panicking.
3. **`simulator/src/scene.rs`** (or `parts.rs` for something part-like): draw it on the model; `droid.rs` stays the primitive model. **`simulator/src/ui.rs`**: add a readout to the side panel if useful.
4. **`simulator/tests/simhw.py`**: add the same function to the pure-Python `simhw` that the headless test uses, so CI keeps passing.
5. Rebuild with `.\simulator\build.ps1`, and keep `cargo fmt` and `cargo clippy` clean.

The shim then calls the new `simhw` function. It still lives in `dist\shims\` (or `simulator/shims/`), so changing its behaviour later needs no rebuild.
