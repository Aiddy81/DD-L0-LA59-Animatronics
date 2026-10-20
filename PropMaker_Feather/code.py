# Using CircuitPython 9.x
"""DD-L0-LA59 entry point: boots the droid, then runs every behaviour as a task.

Board setup lives in hardware.py, tunables in settings.toml (read by config.py)
and each behaviour in behaviours/.
"""
import asyncio
import random

from hardware import play_wav, wavfiles
from behaviours.attention import idle_attention
from behaviours.boot import boot_sequence
from behaviours.breathing import ambient_breathing
from behaviours.leds import random_led_scheduler
from behaviours.motion import motion_sense


async def main():
    """Main function to run all tasks concurrently."""
    await boot_sequence()
    # Create tasks for each animation/function
    motion_sense_task = asyncio.create_task(motion_sense())
    asyncio.create_task(play_wav(random.choice(wavfiles)))
    ambient_breathing_task = asyncio.create_task(ambient_breathing())
    idle_attention_task = asyncio.create_task(idle_attention())
    random_led_schedule_task = asyncio.create_task(random_led_scheduler())

    # This will run forever, because no tasks ever finish.
    await asyncio.gather(motion_sense_task, ambient_breathing_task, idle_attention_task, random_led_schedule_task)

asyncio.run(main())
