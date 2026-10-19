import asyncio
import math

import config
from hardware import (kit, white_led, mainEye_red, mainEye_green, mainEye_blue,
                      smallEye_red, smallEye_green, smallEye_blue)


async def boot_sequence(angle_min=10, angle_max=60, cycles=2, duration=1):
    """Smooth boot sequence for both wings and eyes."""
    steps = 60  # Number of steps for smoothness
    # Move servo[0] up and down, cycles times
    for _ in range(cycles):
        for i in range(steps + 1):
            phase = 2 * math.pi * i / steps  # 0 to 2π
            norm = (math.sin(phase - math.pi/2) + 1) / 2  # 0 to 1 to 0
            angle0 = angle_min + (angle_max - angle_min) * norm
            kit.servo[0].angle = angle0
            await asyncio.sleep(duration / steps)
    kit.servo[0].angle = angle_max

    # Then move servo[1] up and down, cycles times (inverted and calibrated)
    for _ in range(cycles):
        for i in range(steps + 1):
            phase = 2 * math.pi * i / steps
            norm = (math.sin(phase - math.pi/2) + 1) / 2
            angle1 = config.SERVO2_MIN + (config.SERVO2_MAX - config.SERVO2_MIN) * (1 - norm)
            kit.servo[1].angle = angle1
            await asyncio.sleep(duration / steps)
    kit.servo[1].angle = config.SERVO2_MIN

    # Each eye channel in turn: angle 1, 180, 360, then off.
    for channel in (mainEye_red, mainEye_green, mainEye_blue,
                    smallEye_red, smallEye_green, smallEye_blue):
        channel.angle = 1
        await asyncio.sleep(0.2)
        channel.angle = 180
        await asyncio.sleep(0.2)
        channel.angle = 360
        await asyncio.sleep(0.2)
        channel.angle = 0
    # WhiteLED
    white_led.duty_cycle = round(65535/2)  # Set white LED to full brightness
    await asyncio.sleep(0.5)  # Hold for a moment
    white_led.duty_cycle = 0  # Turn off white LED
    await asyncio.sleep(0.5)  # Hold for a moment
    white_led.duty_cycle = round(65535/2)  # Set white LED to full brightness
    await asyncio.sleep(0.5)  # Hold for a moment
    white_led.duty_cycle = 0  # Turn off white LED
    white_led.duty_cycle = config.WHITE_LED_IDLE_DUTY  # Set white LED to very low brightness
    print("Boot sequence complete!")
