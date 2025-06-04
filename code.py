# Using CircuitPython 9.x

import board
from digitalio import DigitalInOut, Direction, Pull 
import time
import audiocore
import audiobusio
import audiomixer
import pwmio
import os
import random
from adafruit_motor import servo 
import adafruit_lis3dh
import classDef as classDef
import asyncio
import math
import random
from adafruit_servokit import ServoKit
kit = ServoKit(channels=8)
kit.servo[0].set_pulse_width_range(544, 2400)
kit.servo[1].set_pulse_width_range(544, 2400)

led = DigitalInOut(board.LED)
led.direction = Direction.OUTPUT

# enable external power pin
# provides power to the external components
external_power = DigitalInOut(board.EXTERNAL_POWER)
external_power.direction = Direction.OUTPUT
external_power.value = True

audio = audiobusio.I2SOut(board.I2S_BIT_CLOCK, board.I2S_WORD_SELECT, board.I2S_DATA)
mixer = audiomixer.Mixer(voice_count=1, sample_rate=22050, channel_count=2,
                         bits_per_sample=16, samples_signed=True)


# onboard LIS3DH
i2c = board.I2C()
int1 = DigitalInOut(board.ACCELEROMETER_INTERRUPT)
lis3dh = adafruit_lis3dh.LIS3DH_I2C(i2c, int1=int1)
lis3dh.range = adafruit_lis3dh.RANGE_2_G


#pwm = pwmio.PWMOut(board.EXTERNAL_SERVO, frequency=50)
#prop_servo = servo.Servo(pwm, min_pulse=750, max_pulse=2250)
#prop_servo.actuation_range = 180
#angle_plus = True

pause_breathing = asyncio.Event()
pause_breathing.clear()  # Start with breathing enabled
reset_idle_event = asyncio.Event()

# Global Variables for functions
motion_count = 0  # This variable counts the number of motion events detected, 
                  # and is used to trigger angry animation. 
IDLE_MIN_SECONDS = 5 * 60    # 5 minutes # Minimum idle time before triggering idle animation
IDLE_MAX_SECONDS = 30 * 60   # 30 minutes # Maximum idle time before triggering idle animation
ANGRY_MIN_WINDOW = 1 * 60    # 1 minute (in seconds)
ANGRY_MAX_WINDOW = 3 * 60    # 3 minutes (in seconds)

wavfiles = [
    file
    for file in os.listdir("sounds/")
    if (file.endswith(".wav") and not file.startswith("._"))
]


async def play_Wav(filename):
    """Plays a WAV file in its entirety (function blocks until done)."""
    print("Playing", filename)
    #with open(f"sounds/{filename}", "rb") as file:
    #audio.play(audiocore.WaveFile(file))
    wave_file = open(f"sounds/{filename}", "rb")
    wav = audiocore.WaveFile(wave_file)
    audio.play(mixer)
    mixer.voice[0].play(wav, loop=False)
    mixer.voice[0].level = 0.70

    # Randomly flicker the LED a bit while audio plays
    #while audio.playing:
        #led.duty_cycle = random.randint(5000, 30000)
        #time.sleep(0.1)
    #led.duty_cycle = 65535  # Back to full brightness
    return

async def motion_sense():
    global motion_count
    angry_window_start = time.monotonic()
    angry_window_length = random.uniform(ANGRY_MIN_WINDOW, ANGRY_MAX_WINDOW)
    print(f"Angry window starts at {angry_window_start} for {angry_window_length:.2f} seconds")
    while True:
        x, y, z = [
            value / adafruit_lis3dh.STANDARD_GRAVITY for value in lis3dh.acceleration
        ]
        hard_motion = abs(z) > 2.5

        now = time.monotonic()
        # Reset the angry window if time expired
        if now - angry_window_start > angry_window_length:
            motion_count = 0
            angry_window_start = now
            angry_window_length = random.uniform(ANGRY_MIN_WINDOW, ANGRY_MAX_WINDOW)
            print("Angry window expired, motion_count reset.")

        if z < 0 or z > 1.1 or hard_motion:
            reset_idle_event.set()  # <-- Reset idle timer!
            motion_count += 1
            print(f"Motion detected! Count: {motion_count}")
            pause_breathing.set()

            if motion_count >= 3 or hard_motion:
                await angry_animation()
                motion_count = 0
                # Reset the angry window after triggering
                angry_window_start = time.monotonic()
                angry_window_length = random.uniform(ANGRY_MIN_WINDOW, ANGRY_MAX_WINDOW)
                print("Angry animation triggered, motion_count reset and window restarted.")
            else:
                await loving_boops()  

            pause_breathing.clear()
        await asyncio.sleep(0.1)

# For Debugging and experimention
#async def move_s1(duration):
#    """Moves servo 1 in a breath motion."""
#    wingLeft = classDef.servoMotion(0, 90, 0.5, 1, 10, 10, [], [])
#    wingLeft.calculate_s_curve_angles()
#    while duration > 0:
#        for angle in wingLeft.theta_list:
#            kit.servo[0].angle = angle
#            await asyncio.sleep(wingLeft.dt)
#            duration -= 1
#        if duration <= 0:
#            break
#
#async def move_s2(duration):
#    """Moves servo 2 in a breath motion."""
#    wingRight = classDef.servoMotion(0, 90, 0.5, 1, 10, 10, [], [])
#    wingRight.calculate_s_curve_angles()
#    while duration > 0:
#        for angle in wingRight.theta_list:
#            kit.servo[1].angle = angle
#            await asyncio.sleep(wingRight.dt)
#            duration -= 1
#        if duration <= 0:
#            break

async def ambient_breathing(period=4, angle_min=20, angle_max=70):
    """Smoothly moves both wings up and down in a breathing pattern, pausing when needed."""
    t = 0
    while True:
        if pause_breathing.is_set():
            await asyncio.sleep(0.05)
            continue
        angle = (angle_max - angle_min) / 2 * math.sin(2 * math.pi * t / period) + (angle_max + angle_min) / 2
        kit.servo[0].angle = angle
        kit.servo[1].angle = angle
        await asyncio.sleep(0.1)
        t += 0.05

async def angry_animation():
    """Moves servos in an 'angry' rapid flapping pattern."""
    print("ANGRY ANIMATION!")
    for _ in range(6):  # 3 fast up/down cycles
        kit.servo[0].angle = 90
        kit.servo[1].angle = 90
        await asyncio.sleep(0.1)
        kit.servo[0].angle = 0
        kit.servo[1].angle = 0
        await asyncio.sleep(0.1)

async def loving_boops():
    """Moves servos in a gentle, cuddly 'boop' animation."""
    print("LOVING BOOP ANIMATION!")
    await play_Wav("006.wav")
    # Wings gently open, pause, then close
    for _ in range(2):  # Two gentle boops
        kit.servo[0].angle = 70
        kit.servo[1].angle = 70
        await asyncio.sleep(0.3)
        kit.servo[0].angle = 20
        kit.servo[1].angle = 20
        await asyncio.sleep(0.3)

async def attention_animation():
    """Moves servos and plays a sound to grab attention when idle."""
    print("ATTENTION ANIMATION!")
    await play_Wav("002.wav") 
    for _ in range(4):
        kit.servo[0].angle = 90
        kit.servo[1].angle = 0
        await asyncio.sleep(0.2)
        kit.servo[0].angle = 0
        kit.servo[1].angle = 90
        await asyncio.sleep(0.2)
    kit.servo[0].angle = 45
    kit.servo[1].angle = 45

async def idle_attention():
    """Waits for a random idle period, then grabs attention if no interaction."""
    while True:
        wait_time = random.uniform(IDLE_MIN_SECONDS, IDLE_MAX_SECONDS)
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

async def main():
    """Main function to run all tasks concurrently."""
    # Create tasks for each animation/function
    motion_sense_task = asyncio.create_task(motion_sense())
    play_Wav_task = asyncio.create_task(play_Wav(random.choice(wavfiles)))
    ambient_breathing_task = asyncio.create_task(ambient_breathing())
    idle_attention_task = asyncio.create_task(idle_attention())
    #move_s1_task = asyncio.create_task(move_s1())
    #move_s2_task = asyncio.create_task(move_s2())
    
    # This will run forever, because no tasks ever finish.
    await asyncio.gather(motion_sense_task, ambient_breathing_task, idle_attention_task)

asyncio.run(main())