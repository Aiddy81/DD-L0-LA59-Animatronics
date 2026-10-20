import asyncio
import math

import config
from hardware import kit, mainEye_red, mainEye_green, mainEye_blue, smallEye_blue


def clamp(val, min_val, max_val):
    return max(min_val, min(max_val, val))


async def ambient_breathing(period=config.BREATH_PERIOD_S, angle_min=config.BREATH_ANGLE_MIN,
                            angle_max=config.BREATH_ANGLE_MAX):
    """Smoothly moves both wings up and down in a breathing pattern, with blue/red LED color and clamped servo angles."""
    t = 0
    while True:
        # Normalized sine wave: norm goes from 0 (down) to 1 (up)
        norm = (math.sin(2 * math.pi * t / period) + 1) / 2
        angle = angle_min + (angle_max - angle_min) * norm

        # Calibrated and inverted for servo 2
        angle0 = clamp(angle, 0, 180)
        angle1 = clamp(config.BREATH_SERVO2_MIN
                       + (config.BREATH_SERVO2_MAX - config.BREATH_SERVO2_MIN) * (1 - norm), 0, 180)

        kit.servo[0].angle = angle0
        kit.servo[1].angle = angle1

        # LED color: blue at bottom, red at top, smooth transition
        min_brightness = config.EYE_MIN_BRIGHTNESS
        max_brightness = config.EYE_MAX_BRIGHTNESS
        # Use a nonlinear curve for more dramatic effect
        led_norm = norm ** 2

        blue_brightness = int(min_brightness + (max_brightness - min_brightness) * (1 - led_norm))
        red_brightness = int(min_brightness + (max_brightness - min_brightness) * led_norm)

        mainEye_red.angle = red_brightness
        mainEye_green.angle = 0
        mainEye_blue.angle = blue_brightness
        smallEye_blue.angle = blue_brightness - 50  # Small eye blue is always slightly dimmer

        #print(f"t: {t:.2f}, angle0: {angle0:.2f}, angle1: {angle1:.2f}, norm: {norm:.2f}, blue: {blue_brightness}, red: {red_brightness}")

        await asyncio.sleep(0.1)
        t += 0.05
