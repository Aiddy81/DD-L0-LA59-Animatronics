import asyncio
import math
import random
import time

import config
from hardware import (kit, play_wav, stop_wav, white_led, reset_idle_event,
                      mainEye_red, mainEye_green, mainEye_blue,
                      smallEye_red, smallEye_green, smallEye_blue)

# Manoeuvres as (pitch, roll, shortest hold s, longest hold s).
# pitch: -1 steep dive .. +1 steep climb. roll: -1 hard bank left .. +1 hard bank right.
MANOEUVRES = (
    (0.0, 0.0, 1.0, 2.5),    # level, low through the trench
    (0.2, 1.0, 0.6, 1.2),    # tight turn right
    (0.2, -1.0, 0.6, 1.2),   # tight turn left
    (1.0, 0.0, 1.0, 2.0),    # steep climb
    (-1.0, 0.0, 0.8, 1.5),   # steep dive
    (0.7, 0.7, 0.8, 1.5),    # climbing turn right
    (0.7, -0.7, 0.8, 1.5),   # climbing turn left
)

# Buffeting: a fast flutter on top of each pose, stronger the harder the move,
# like air loading up an aileron.
BUFFET_HZ = 7
BUFFET_MIN = 0.03   # fraction of the wing's travel
BUFFET_MAX = 0.08
JITTER = 0.02


def clamp01(val):
    return max(0.0, min(1.0, val))


def smoothstep(x):
    x = clamp01(x)
    return x * x * (3 - 2 * x)


def wing_heights(pitch, roll):
    """Wing heights 0 (down) to 1 (up). Banking right drops the right wing."""
    right = clamp01(0.5 + 0.5 * pitch - 0.5 * roll)
    left = clamp01(0.5 + 0.5 * pitch + 0.5 * roll)
    return right, left


def set_wings(right, left):
    # Same ranges as attention_animation: servo 0 lower angle = higher wing,
    # servo 1 is mounted inverted and calibrated by SERVO2_MIN/MAX.
    kit.servo[0].angle = 60 - 50 * right
    kit.servo[1].angle = config.SERVO2_MIN + (config.SERVO2_MAX - config.SERVO2_MIN) * left


def set_eyes(red, green):
    mainEye_red.angle = red
    mainEye_green.angle = green
    mainEye_blue.angle = 0
    smallEye_red.angle = red
    smallEye_green.angle = green
    smallEye_blue.angle = 0


async def trench_run(duration=config.TRENCH_RUN_S):
    """Top Gun: Maverick trench run: tight turns, steep climbs and dives with
    buffeting wings and an afterburner glow. Returns True if a nudge cut it short."""
    print("TRENCH RUN!")
    # A short clip looped for the whole run: a full-length track won't fit on CIRCUITPY.
    await play_wav("trenchRun.wav", loop=True)
    pitch, roll = 0.0, 0.0
    start = time.monotonic()
    while True:
        # Next manoeuvre: ease from the current pose, then hold it.
        from_pitch, from_roll = pitch, roll
        to_pitch, to_roll, hold_min, hold_max = random.choice(MANOEUVRES)
        ease = random.uniform(0.3, 0.6)
        move_len = ease + random.uniform(hold_min, hold_max)
        move_start = time.monotonic()

        # Clock-driven frames, as in attention_animation.
        while True:
            now = time.monotonic()
            aborted = reset_idle_event.is_set()
            if aborted or now - start >= duration:
                # Boops only reset some eye channels, so clear the afterburner
                # either way. On a nudge, motion_sense owns the wings and
                # pause_breathing from here.
                stop_wav()
                if not aborted:
                    set_wings(0.5, 0.5)
                set_eyes(0, 0)
                white_led.duty_cycle = config.WHITE_LED_IDLE_DUTY
                print("Trench run aborted" if aborted else "Trench run complete")
                return aborted
            in_move = now - move_start
            if in_move >= move_len:
                break

            k = smoothstep(in_move / ease)
            pitch = from_pitch + (to_pitch - from_pitch) * k
            roll = from_roll + (to_roll - from_roll) * k
            right, left = wing_heights(pitch, roll)

            load = min(1.0, abs(pitch) + abs(roll))
            amp = BUFFET_MIN + (BUFFET_MAX - BUFFET_MIN) * load
            phase = 2 * math.pi * BUFFET_HZ * (now - start)
            # Wings flutter slightly out of step, like two separate control surfaces.
            right = clamp01(right + amp * math.sin(phase) + random.uniform(-JITTER, JITTER))
            left = clamp01(left + amp * math.sin(phase + 1.3) + random.uniform(-JITTER, JITTER))
            set_wings(right, left)

            # Afterburner: orange, burning brighter on climbs, dimmer on dives.
            level = (0.5 + 0.25 * (right + left)) * random.uniform(0.85, 1.0)
            red = int(360 * level)
            set_eyes(red, int(red * 0.35))
            white_led.duty_cycle = random.randint(25000, 65535)

            await asyncio.sleep(0)
