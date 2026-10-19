import asyncio
import math
import random
import time

import adafruit_lis3dh

import config
from hardware import (kit, lis3dh, play_wav, pause_breathing, reset_idle_event,
                      mainEye_red, mainEye_green, mainEye_blue,
                      smallEye_red, smallEye_green, smallEye_blue)


async def motion_sense():
    # Counts nudges in the current angry window; enough of them gets the droid angry.
    motion_count = 0
    angry_window_start = time.monotonic()
    angry_window_length = random.uniform(config.ANGRY_MIN_WINDOW_S, config.ANGRY_MAX_WINDOW_S)
    print(f"Angry window starts at {angry_window_start} for {angry_window_length:.2f} seconds")
    while True:
        x, y, z = [
            value / adafruit_lis3dh.STANDARD_GRAVITY for value in lis3dh.acceleration
        ]
        hard_motion = abs(z) > config.HARD_MOTION_G

        now = time.monotonic()
        # Reset the angry window if time expired
        if now - angry_window_start > angry_window_length:
            motion_count = 0
            angry_window_start = now
            angry_window_length = random.uniform(config.ANGRY_MIN_WINDOW_S, config.ANGRY_MAX_WINDOW_S)
            print("Angry window expired, motion_count reset.")

        if z < config.MOTION_Z_MIN_G or z > config.MOTION_Z_MAX_G or hard_motion:
            reset_idle_event.set()  # <-- Reset idle timer!
            motion_count += 1
            print(f"Motion detected! Count: {motion_count}")
            pause_breathing.set()

            if motion_count >= config.ANGRY_NUDGES or hard_motion:
                await angry_animation()
                motion_count = 0
                # Reset the angry window after triggering
                angry_window_start = time.monotonic()
                angry_window_length = random.uniform(config.ANGRY_MIN_WINDOW_S, config.ANGRY_MAX_WINDOW_S)
                print("Angry animation triggered, motion_count reset and window restarted.")
            else:
                await nose_boops()

            pause_breathing.clear()
        await asyncio.sleep(0.1)


async def angry_animation(flaps=3, angle_min=30, angle_max=90, duration=0.4):
    """Smooth, rapid angry flapping using a sine wave."""
    mainEye_red.angle = 360  # Set main eye to red
    mainEye_green.angle = 0  # Turn off green
    mainEye_blue.angle = 0   # Turn off blue
    smallEye_red.angle = 360  # Set small eye to red
    smallEye_green.angle = 0  # Turn off green
    smallEye_blue.angle = 0   # Turn off blue
    print("ANGRY ANIMATION!")
    await play_wav("angry.wav")
    steps = 20  # Number of steps per flap
    for _ in range(flaps):
        for i in range(steps + 1):
            # Sine wave for smooth up/down
            phase = math.pi * i / steps  # 0 to pi
            angle = angle_min + (angle_max - angle_min) * math.sin(phase)
            kit.servo[0].angle = angle
            kit.servo[1].angle = angle_max + angle_min - angle  # Inverted
            await asyncio.sleep(duration / steps)
        for i in range(steps + 1):
            phase = math.pi * i / steps  # 0 to pi
            angle = angle_max - (angle_max - angle_min) * math.sin(phase)
            kit.servo[0].angle = angle
            kit.servo[1].angle = angle_max + angle_min - angle  # Inverted
            await asyncio.sleep(duration / steps)
    # Return to neutral
    kit.servo[0].angle = angle_min
    kit.servo[1].angle = angle_max

#async def nose_boops():
#    """Moves servos in a gentle, cuddly 'boop' animation."""
#    print("LOVING BOOP ANIMATION!")
#    await play_Wav("006.wav")
#    # Wings gently open, pause, then close
#    for _ in range(2):  # Two gentle boops
#        kit.servo[0].angle = 70
#        kit.servo[1].angle = 20
#        await asyncio.sleep(0.3)
#        kit.servo[0].angle = 20
#        kit.servo[1].angle = 70
#        await asyncio.sleep(0.3)

async def nose_boops(boops=2, angle_min=20, angle_max=70, duration=1.2):
    """Smooth, gentle 'boop' animation using a sine wave."""
    await play_wav("noseBoops.wav")
    steps = 60  # More steps for extra smoothness
    for _ in range(boops):
        for i in range(steps + 1):
            phase = math.pi * i / steps  # 0 to pi
            angle = angle_min + (angle_max - angle_min) * math.sin(phase)
            kit.servo[0].angle = angle
            kit.servo[1].angle = angle_max + angle_min - angle  # Inverted
            await asyncio.sleep(duration / steps)
        for i in range(steps + 1):
            phase = math.pi * i / steps  # 0 to pi
            angle = angle_max - (angle_max - angle_min) * math.sin(phase)
            kit.servo[0].angle = angle
            kit.servo[1].angle = angle_max + angle_min - angle  # Inverted
            await asyncio.sleep(duration / steps)
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
