"""Measure the real PropMaker Feather so the simulator can match it.

It runs in the robot, away from the PC, and keeps its results in the board's
non-volatile memory (microcontroller.nvm) so they survive power-off:
  1. On the CIRCUITPY drive rename code.py to code_main.py, then copy this
     file in as code.py.
  2. Unplug the Feather, fit it in the robot and power on. When it's done the
     main eye glows green. If the onboard red LED blinks instead, it failed;
     the error is saved in place of the results.
  3. Plug it into the PC and open the serial console (press Ctrl-D to rerun
     if you missed the output). With USB connected it prints the saved
     [board] block instead of measuring. Copy it into simulator/sim.toml,
     replacing the existing [board] section.
  4. Delete this code.py and rename code_main.py back to code.py.

With USB connected and nothing saved, it measures and prints straight away.
To measure again, clear the saved result in the REPL:
  import microcontroller; microcontroller.nvm[0:4] = bytes(4)

Only the main eye's red LED flickers at its dimmest; the wings don't move.
"""
import gc
import time

gc.collect()
free_at_start = gc.mem_free()

import microcontroller  # noqa: E402 - imported after the first heap reading on purpose
import supervisor  # noqa: E402

MAGIC = b"DDL0"


def saved_result():
    nvm = microcontroller.nvm
    if nvm[0:4] != MAGIC:
        return None
    n = nvm[4] | nvm[5] << 8
    return bytes(nvm[6 : 6 + n]).decode()


def save(text):
    data = text.encode()
    record = MAGIC + bytes((len(data) & 0xFF, len(data) >> 8)) + data
    microcontroller.nvm[0 : len(record)] = record


def measure():
    import asyncio
    import board
    import busio
    import pwmio
    from digitalio import DigitalInOut

    import adafruit_lis3dh
    from adafruit_servokit import ServoKit

    gc.collect()
    # What is left for code.py once the same libraries code.py uses are loaded.
    free_after_imports = gc.mem_free()

    i2c = busio.I2C(board.SCL, board.SDA)
    kit = ServoKit(channels=8, i2c=i2c)
    lis3dh = adafruit_lis3dh.LIS3DH_I2C(i2c, int1=DigitalInOut(board.ACCELEROMETER_INTERRUPT))
    lis3dh.range = adafruit_lis3dh.RANGE_4_G
    eye = kit.servo[2]
    eye.actuation_range = 360
    eye.set_pulse_width_range(0, 6550)
    white_led = pwmio.PWMOut(board.EXTERNAL_NEOPIXELS, frequency=5000)

    def per_call_ms(fn, n=200):
        start = time.monotonic_ns()
        for i in range(n):
            fn(i)
        return (time.monotonic_ns() - start) / n / 1000000

    def pca_write(i):
        eye.angle = i & 1

    def pwm_write(i):
        white_led.duty_cycle = i & 1

    def accel_read(_):
        return lis3dh.acceleration

    async def yields(n):
        start = time.monotonic_ns()
        for _ in range(n):
            await asyncio.sleep(0)
        return (time.monotonic_ns() - start) / n / 1000000

    pca_ms = per_call_ms(pca_write)
    eye.angle = 0
    pwm_ms = per_call_ms(pwm_write)
    white_led.duty_cycle = 0
    accel_ms = per_call_ms(accel_read, 50)
    loop_ms = asyncio.run(yields(200))
    white_led.deinit()

    text = "\n".join(
        (
            "# Measured on the robot by board_bench.py",
            "# heap free at start: {} bytes".format(free_at_start),
            "[board]",
            "measured = true",
            "pca_write_ms = {:.3f}".format(pca_ms),
            "pwm_write_ms = {:.3f}".format(pwm_ms),
            "accel_read_ms = {:.3f}".format(accel_ms),
            "loop_ms = {:.3f}".format(loop_ms),
            "heap_budget_kb = {}".format(free_after_imports // 1024),
            "heap_scale = 0.45",
        )
    )
    return text, kit, i2c


def blink_forever():
    import board
    from digitalio import DigitalInOut, Direction

    led = DigitalInOut(board.LED)
    led.direction = Direction.OUTPUT
    while True:
        led.value = not led.value
        time.sleep(0.25)


on_usb = supervisor.runtime.usb_connected
saved = saved_result() if on_usb else None

if saved:
    print()
    print("# Saved result (run on robot power)")
    print(saved)
    print()
elif on_usb:
    text, _, i2c = measure()
    i2c.deinit()
    print()
    print(text)
    print()
else:
    try:
        text, kit, _ = measure()
    except Exception as e:  # noqa: BLE001 - any failure must be saved, not lost
        save("# board_bench failed: {!r}".format(e))
        blink_forever()
    save(text)
    done = kit.servo[3]  # main eye green
    done.actuation_range = 360
    done.set_pulse_width_range(0, 6550)
    done.angle = 90
    while True:
        time.sleep(1)
