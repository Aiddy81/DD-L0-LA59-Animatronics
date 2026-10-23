import asyncio
import random
import time

import config
from hardware import white_led, trench_active


async def random_led_scheduler(min_interval=5, max_interval=60):
    """Continuously trigger the LED effect at random intervals."""
    while True:
        #await asyncio.sleep(random.uniform(min_interval, max_interval))
        #duration = random.uniform(2, 6)  # How long the flash effect lasts
        #await data_processing_led(duration)
        interval = random.uniform(min_interval, max_interval)
        print(f"Next LED activation in: {interval:.2f} seconds")
        await asyncio.sleep(interval)
        if trench_active.is_set():
            continue  # the afterburner flicker has the LED

        duration = random.uniform(2, 6)
        print(f"LED will flash for: {duration:.2f} seconds")
        await data_processing_led(duration)


async def data_processing_led(duration=10):
    """Flash an LED to simulate data processing with random brightness and timing. :param duration: How long to run the flashing effect in seconds"""
    start_time = time.monotonic()
    while time.monotonic() - start_time < duration:
        if trench_active.is_set():
            return  # trench run took the LED over mid-burst; it sets the LED itself
        # Random brightness between 0 (off) and 65535 (full brightness)
        brightness = random.randint(0, 65535)
        white_led.duty_cycle = brightness

        # Random duration between flashes
        delay = random.uniform(0.02, 0.25)
        await asyncio.sleep(delay)

        # Occasionally turn fully off to simulate pauses in data
        if random.random() < 0.2:
            white_led.duty_cycle = 0
            await asyncio.sleep(random.uniform(0.05, 0.15))

    # Ensure LED is turned off at the end
    white_led.duty_cycle = config.WHITE_LED_IDLE_DUTY
