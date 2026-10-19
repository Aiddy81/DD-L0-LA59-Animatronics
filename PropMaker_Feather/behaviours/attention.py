import asyncio
import math
import random

import config
from hardware import (kit, play_wav, pause_breathing, reset_idle_event,
                      mainEye_red, mainEye_green, mainEye_blue,
                      smallEye_red, smallEye_green, smallEye_blue)


def hsv_to_rgb(h, s, v):
    """Convert HSV (0-1 floats) to RGB (0-1 floats)."""
    if s == 0.0:
        return v, v, v
    i = int(h * 6.0)
    f = (h * 6.0) - i
    p = v * (1.0 - s)
    q = v * (1.0 - s * f)
    t = v * (1.0 - s * (1.0 - f))
    i = i % 6
    if i == 0:
        return v, t, p
    if i == 1:
        return q, v, p
    if i == 2:
        return p, v, t
    if i == 3:
        return p, q, v
    if i == 4:
        return t, p, v
    if i == 5:
        return v, p, q

#async def attention_animation():
#    """Moves servos and plays a sound to grab attention when idle."""
#    print("ATTENTION ANIMATION!")
#    await play_Wav("002.wav")
#    for _ in range(4):
#        kit.servo[0].angle = 90
#        kit.servo[1].angle = 0
#        await asyncio.sleep(0.2)
#        kit.servo[0].angle = 0
#        kit.servo[1].angle = 90
#        await asyncio.sleep(0.2)
#    kit.servo[0].angle = 45
#    kit.servo[1].angle = 45

async def attention_animation(flaps=5, angle_min=10, angle_max=60, duration=0.2):
    """Smooth, attention-grabbing wing motion with full LED color cycling and servo calibration."""
    await play_wav("youWho.wav")
    steps = 60
    for _ in range(flaps):
        for i in range(steps + 1):
            phase = 2 * math.pi * i / steps
            norm = (math.sin(phase - math.pi/2) + 1) / 2
            angle0 = angle_min + (angle_max - angle_min) * norm
            angle1 = config.SERVO2_MIN + (config.SERVO2_MAX - config.SERVO2_MIN) * (1 - norm)
            kit.servo[0].angle = angle0
            kit.servo[1].angle = angle1

            # Use custom HSV to RGB
            hue = i / steps
            r, g, b = hsv_to_rgb(hue, 1.0, 1.0)
            mainEye_red.angle = int(r * 360)
            mainEye_green.angle = int(g * 360)
            mainEye_blue.angle = int(b * 360)
            smallEye_red.angle = int(r * 360)
            smallEye_green.angle = int(g * 360)
            smallEye_blue.angle = int(b * 360)

            await asyncio.sleep(duration / steps)
    kit.servo[0].angle = angle_min
    kit.servo[1].angle = config.SERVO2_MIN
    mainEye_red.angle = 0
    mainEye_green.angle = 0
    mainEye_blue.angle = 0
    smallEye_red.angle = 0
    smallEye_green.angle = 0
    smallEye_blue.angle = 0


async def idle_attention():
    """Waits for a random idle period, then grabs attention if no interaction."""
    while True:
        wait_time = random.uniform(config.IDLE_MIN_S, config.IDLE_MAX_S)
        print(f"Waiting for idle period: {wait_time:.2f} seconds")
        try:
            # Wait for either the timer or a reset event
            await asyncio.wait_for(reset_idle_event.wait(), timeout=wait_time)
            reset_idle_event.clear()  # Reset for next cycle
        except asyncio.TimeoutError:
            # Timer expired: do the attention animation
            pause_breathing.set()
            await attention_animation()
            pause_breathing.clear()
