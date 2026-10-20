"""Measure the real PropMaker Feather so the simulator can match it.

Run once on the robot:
  1. Copy this file to the root of the CIRCUITPY drive (next to code.py).
  2. Open the serial console, press Ctrl-C to stop code.py, then Enter for
     the REPL prompt.
  3. Type:  import board_bench
  4. Copy the [board] block it prints into simulator/sim.toml, replacing the
     existing [board] section.

Only the main eye's red LED flickers at its dimmest; the wings don't move.
Delete board_bench.py from the drive afterwards.
"""
import gc
import time

gc.collect()
free_at_start = gc.mem_free()

import asyncio  # noqa: E402 - imported after the first heap reading on purpose
import board  # noqa: E402
import busio  # noqa: E402
import pwmio  # noqa: E402
from digitalio import DigitalInOut  # noqa: E402

import adafruit_lis3dh  # noqa: E402
from adafruit_servokit import ServoKit  # noqa: E402

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

N = 200


def per_call_ms(fn, n=N):
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
loop_ms = asyncio.run(yields(N))

print()
print("# Measured on the robot by board_bench.py")
print("# heap free at start: {} bytes".format(free_at_start))
print("[board]")
print("measured = true")
print("pca_write_ms = {:.3f}".format(pca_ms))
print("pwm_write_ms = {:.3f}".format(pwm_ms))
print("accel_read_ms = {:.3f}".format(accel_ms))
print("loop_ms = {:.3f}".format(loop_ms))
print("heap_budget_kb = {}".format(free_after_imports // 1024))
print("heap_scale = 0.45")
print()

white_led.deinit()
i2c.deinit()
