import asyncio
import math
import random
import time

import adafruit_lis3dh

import config
from log import log
from hardware import (kit, lis3dh, play_wav, pause_breathing, reset_idle_event,
                      mainEye_red, mainEye_green, mainEye_blue,
                      smallEye_red, smallEye_green, smallEye_blue)

# How fast the resting (gravity) reading follows the board's orientation, per
# 0.1 s loop. Slow enough that a nudge barely moves it.
BASELINE_RATE = 0.1
# Animations end by snapping the wings back to rest, which shakes the board for
# a moment; on the robot that read as a fresh nudge and kept boops looping.
SETTLE_S = 1.0


def read_g():
    return [value / adafruit_lis3dh.STANDARD_GRAVITY for value in lis3dh.acceleration]


async def settle():
    """Let the wings stop shaking the board, then take the resting reading."""
    await asyncio.sleep(SETTLE_S)
    return read_g()


async def motion_sense():
    # Counts nudges in the current angry window; enough of them gets the droid angry.
    motion_count = 0
    angry_window_start = time.monotonic()
    angry_window_length = random.uniform(config.ANGRY_MIN_WINDOW_S, config.ANGRY_MAX_WINDOW_S)
    log(f"Angry window starts at {angry_window_start} for {angry_window_length:.2f} seconds")
    # The board is mounted sideways, so gravity isn't on any fixed axis. A nudge
    # is any change from the resting reading, in any direction.
    rest = await settle()  # the boot sequence has just moved the wings
    animation_peak = None
    while True:
        reading = read_g()
        delta = math.sqrt(sum((a - r) ** 2 for a, r in zip(reading, rest)))
        rest = [r + (a - r) * BASELINE_RATE for a, r in zip(reading, rest)]
        hard_motion = math.sqrt(sum(a * a for a in reading)) > config.HARD_MOTION_G

        # While attention or the trench run owns the wings, their movement
        # shakes the board, so it takes a firmer nudge to interrupt them.
        if pause_breathing.is_set():
            nudge_g = config.ANIMATION_NUDGE_G
            # Largest shake the animation caused itself, to tune ANIMATION_NUDGE_MG by.
            if delta <= nudge_g:
                animation_peak = max(animation_peak or 0.0, delta)
        elif animation_peak is not None:
            log(f"Animation peak shake: {animation_peak * 1000:.0f} mg")
            animation_peak = None
            rest = await settle()
            continue
        else:
            nudge_g = config.NUDGE_G

        now = time.monotonic()
        # Reset the angry window if time expired
        if now - angry_window_start > angry_window_length:
            motion_count = 0
            angry_window_start = now
            angry_window_length = random.uniform(config.ANGRY_MIN_WINDOW_S, config.ANGRY_MAX_WINDOW_S)
            log("Angry window expired, motion_count reset.")

        if delta > nudge_g or hard_motion:
            reset_idle_event.set()  # <-- Reset idle timer!
            motion_count += 1
            log(f"Motion detected! {delta * 1000:.0f} mg, count: {motion_count}")
            if animation_peak is not None:
                log(f"Animation peak shake: {animation_peak * 1000:.0f} mg")
            pause_breathing.set()

            if motion_count >= config.ANGRY_NUDGES or hard_motion:
                await angry_animation()
                motion_count = 0
                # Reset the angry window after triggering
                angry_window_start = time.monotonic()
                angry_window_length = random.uniform(config.ANGRY_MIN_WINDOW_S, config.ANGRY_MAX_WINDOW_S)
                log("Angry animation triggered, motion_count reset and window restarted.")
            else:
                await nose_boops()

            # Breathing stays paused while settling, so the wings are still.
            # Re-taking the rest also means a droid moved for good (picked up,
            # tipped over) counts once, not as a stream of nudges.
            rest = await settle()
            animation_peak = None
            pause_breathing.clear()
        await asyncio.sleep(0.1)


async def sine_flaps(flaps, angle_min, angle_max, duration):
    """Each flap is two sine strokes of `duration` seconds: min -> max -> min,
    then max -> min -> max. Returns the angle the last stroke ends on."""
    # Driven by the clock, not a step count: each frame's two PCA9685 writes
    # cost ~4 ms over I2C, so fixed per-step sleeps ran ~1.7x slower than asked.
    # A slower board just gets fewer, coarser frames per stroke.
    start = time.monotonic()
    while True:
        elapsed = time.monotonic() - start
        if elapsed >= flaps * 2 * duration:
            break
        strokes = elapsed / duration
        stroke = int(strokes)
        lift = (angle_max - angle_min) * math.sin(math.pi * (strokes - stroke))
        angle = angle_min + lift if stroke % 2 == 0 else angle_max - lift
        kit.servo[0].angle = angle
        kit.servo[1].angle = angle_max + angle_min - angle  # Inverted
        await asyncio.sleep(0)
    return angle_max


async def angry_animation(flaps=3, angle_min=30, angle_max=90, duration=0.4):
    """Smooth, rapid angry flapping using a sine wave."""
    mainEye_red.angle = 360  # Set main eye to red
    mainEye_green.angle = 0  # Turn off green
    mainEye_blue.angle = 0   # Turn off blue
    smallEye_red.angle = 360  # Set small eye to red
    smallEye_green.angle = 0  # Turn off green
    smallEye_blue.angle = 0   # Turn off blue
    log("ANGRY ANIMATION!")
    await play_wav("angry.wav")
    await sine_flaps(flaps, angle_min, angle_max, duration)
    # Return to neutral
    kit.servo[0].angle = angle_min
    kit.servo[1].angle = angle_max


async def nose_boops(boops=2, angle_min=20, angle_max=70, duration=1.2):
    """Smooth, gentle 'boop' animation using a sine wave."""
    await play_wav("noseBoops.wav")
    angle = await sine_flaps(boops, angle_min, angle_max, duration)
    # Return to neutral
    kit.servo[0].angle = angle_min
    kit.servo[1].angle = angle_max

    # Update main eye colors based on the angle
    # Normalized value (0 = min angle, 1 = max angle)
    norm = (angle - angle_min) / (angle_max - angle_min)
    norm = max(0.0, min(1.0, norm))
    norm = norm ** 2
    min_brightness = config.EYE_MIN_BRIGHTNESS
    max_brightness = config.EYE_MAX_BRIGHTNESS
    # Blue is max at norm=1, min at norm=0
    blue_brightness = int(min_brightness + (max_brightness - min_brightness) * norm)
    # Red is max at norm=0, min at norm=1 (inverse)
    red_brightness = int(min_brightness + (max_brightness - min_brightness) * (1 - norm))

    # Inverted: Blue is max at norm=0, Red is max at norm=1
    #blue_brightness = int(min_brightness + (max_brightness - min_brightness) * (1 - norm))
    #red_brightness = int(min_brightness + (max_brightness - min_brightness) * norm)
    mainEye_blue.angle = blue_brightness
    mainEye_red.angle = red_brightness
    smallEye_blue.angle = blue_brightness
