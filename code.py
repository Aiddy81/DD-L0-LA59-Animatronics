# Using CircuitPython 9.x
# Notes
# Servo's 
#           - Servo[0] 
#                   - ngle_min=40 = heighst point
#                   - Servoangle_max = lowest point
#           - Servo[1]
#                   - Servo2_min = lowest point
#                   - Servo2_max = heighst point
import board
from digitalio import DigitalInOut, Direction, Pull 
import time
import audiocore
import audiobusio
import audiomixer
import busio
import pwmio
import os
import random
from adafruit_motor import servo 
import adafruit_lis3dh
import classDef as classDef
import asyncio
import math
import random
import adafruit_register
from adafruit_servokit import ServoKit
import colorsys

# onboard LIS3DH
#i2c = board.I2C()  # uses board.SCL and board.SDA
i2c = busio.I2C(board.SCL, board.SDA)  # Create an I2C bus instance
int1 = DigitalInOut(board.ACCELEROMETER_INTERRUPT)
lis3dh = adafruit_lis3dh.LIS3DH_I2C(i2c, int1=int1)
lis3dh.range = adafruit_lis3dh.RANGE_2_G

#kit = ServoKit(channels=8)
kit = ServoKit(channels=8, i2c=i2c)  # Reuse the same PCA9685 object
kit.servo[0].set_pulse_width_range(544, 2400)
kit.servo[1].set_pulse_width_range(544, 2400)

SERVO2_MIN = 40   # Adjust until both wings close equally, used to adjust the left wing as mounted inverted
SERVO2_MAX = 60   # Adjust until both wings close equally, used to adjust the left wing as mounted inverted

mainEye_red = kit.servo[2]
mainEye_green = kit.servo[3]
mainEye_blue = kit.servo[4]
mainEye_red.actuation_range = 360
mainEye_green.actuation_range = 360
mainEye_blue.actuation_range = 360
mainEye_red.set_pulse_width_range(0,6550)
mainEye_green.set_pulse_width_range(0,6550)
mainEye_blue.set_pulse_width_range(0,6550)

smallEye_red = kit.servo[5]
smallEye_green = kit.servo[6]
smallEye_blue = kit.servo[7]
smallEye_red.actuation_range = 360
smallEye_green.actuation_range = 360
smallEye_blue.actuation_range = 360
smallEye_red.set_pulse_width_range(0,6550)
smallEye_green.set_pulse_width_range(0,6550)
smallEye_blue.set_pulse_width_range(0,6550)


#mainEye_red.duty_cycle = 6550 # 10% duty cycle
#mainEye_red.angle = 0 #turns on and off + brightness, at 0 its off, then from 1 upto 360 increases in brightness
#mainEye_green.duty_cycle =  6550 # 10% duty cycle
#mainEye_green.angle = 0#turns on and off + brightness, at 0 its off, then from 1 upto 360 increases in brightness
#mainEye_blue.duty_cycle =  6550 # 10% duty cycle
#mainEye_blue.angle = 0 #turns on and off + brightness, at 0 its off, then from 1 upto 360 increases in brightness
smallEye_red.angle = 0  
smallEye_green.angle = 0
smallEye_blue.angle = 0

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

async def boot_sequence(angle_min=10, angle_max=60, cycles=2, duration=1):
    """Smooth boot sequence for both wings and eyes."""
    steps = 60  # Number of steps for smoothness

    # Smoothly move servo[0] and servo[1] up and down, cycles times
    for _ in range(cycles):
        for i in range(steps + 1):
            phase = 2 * math.pi * i / steps  # 0 to 2π
            # Sine wave from angle_min (down) to angle_max (up) and back
            norm = (math.sin(phase - math.pi/2) + 1) / 2  # 0 to 1 to 0
            angle0 = angle_min + (angle_max - angle_min) * norm
            angle1 = SERVO2_MIN + (SERVO2_MAX - SERVO2_MIN) * (1 - norm)
            kit.servo[0].angle = angle0
            kit.servo[1].angle = angle1
            await asyncio.sleep(duration / steps)
    kit.servo[0].angle = angle_max
    kit.servo[1].angle = SERVO2_MIN

    # Main eye red: angle 1, 180, 360
    mainEye_red.angle = 1
    await asyncio.sleep(0.2)
    mainEye_red.angle = 180
    await asyncio.sleep(0.2)
    mainEye_red.angle = 360
    await asyncio.sleep(0.2)
    mainEye_red.angle = 0

    # Main eye green: angle 1, 180, 360
    mainEye_green.angle = 1
    await asyncio.sleep(0.2)
    mainEye_green.angle = 180
    await asyncio.sleep(0.2)
    mainEye_green.angle = 360
    await asyncio.sleep(0.2)
    mainEye_green.angle = 0
    # Main eye blue: angle 1, 180, 360
    mainEye_blue.angle = 1
    await asyncio.sleep(0.2)
    mainEye_blue.angle = 180
    await asyncio.sleep(0.2)
    mainEye_blue.angle = 360
    await asyncio.sleep(0.2)
    mainEye_blue.angle = 0

    # Secondary eye (smallEye_red): angle 1, 180, 360
    smallEye_red.angle = 1
    await asyncio.sleep(0.2)
    smallEye_red.angle = 180
    await asyncio.sleep(0.2)
    smallEye_red.angle = 360
    await asyncio.sleep(0.2)
    smallEye_red.angle = 0
    # Secondary eye (smallEye_green): angle 1, 180, 360
    smallEye_green.angle = 1
    await asyncio.sleep(0.2)
    smallEye_green.angle = 180
    await asyncio.sleep(0.2)
    smallEye_green.angle = 360
    await asyncio.sleep(0.2)
    smallEye_green.angle = 0
    # Secondary eye (smallEye_blue): angle 1, 180, 360
    smallEye_blue.angle = 1
    await asyncio.sleep(0.2)
    smallEye_blue.angle = 180
    await asyncio.sleep(0.2)
    smallEye_blue.angle = 360
    await asyncio.sleep(0.2)
    smallEye_blue.angle = 0

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
                await nose_boops()  

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

async def ambient_breathing(period=10, angle_min=50, angle_max=70):
    """Smoothly moves both wings up and down in a breathing pattern, pausing when needed."""
    print("ambient_breathing started")
    SERVO2_MIN = 30  
    SERVO2_MAX = 68

    t = 0
    while True:
        if pause_breathing.is_set():
            await asyncio.sleep(0.05)
            continue
        angle = (angle_max - angle_min) / 2 * math.sin(2 * math.pi * t / period) + (angle_max + angle_min) / 2
        kit.servo[0].angle = angle
        #kit.servo[1].angle = angle_max + angle_min - angle  # Invert direction
        kit.servo[1].angle = SERVO2_MAX + SERVO2_MIN - angle
        
        # Update main eye colors based on the angle
        # Normalized value (0 = min angle, 1 = max angle)
        norm = (angle - angle_min) / (angle_max - angle_min)
        norm = max(0.0, min(1.0, norm))
        norm = norm ** 2

        min_brightness = 60
        max_brightness = 360

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
        
        await asyncio.sleep(0.1)
        t += 0.05

#async def angry_animation():
#    """Moves servos in an 'angry' rapid flapping pattern."""
#    print("ANGRY ANIMATION!")
#    for _ in range(6):  # 3 fast up/down cycles
#        kit.servo[0].angle = 45
#        kit.servo[1].angle = 0
#        await asyncio.sleep(0.1)
#        kit.servo[0].angle = 0
#        kit.servo[1].angle = 45
#        await asyncio.sleep(0.1)

async def angry_animation(flaps=3, angle_min=30, angle_max=90, duration=0.4):
    mainEye_red.angle = 360  # Set main eye to red
    mainEye_green.angle = 0  # Turn off green
    mainEye_blue.angle = 0   # Turn off blue
    smallEye_red.angle = 360  # Set small eye to red
    smallEye_green.angle = 0  # Turn off green
    smallEye_blue.angle = 0   # Turn off blue
    print("ANGRY ANIMATION!")
    await play_Wav("angry.wav")
    """Smooth, rapid angry flapping using a sine wave."""
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

async def nose_boops(boops=2, angle_min=20, angle_max=70, duration=0.8):
    await play_Wav("noseBoops.wav")
    """Smooth, gentle 'boop' animation using a sine wave."""
    steps = 30  # More steps for extra smoothness
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
    min_brightness = 60
    max_brightness = 360
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

async def attention_animation(flaps=2, angle_min=10, angle_max=100, duration=1.0):
    await play_Wav("youWho.wav")
    """Smooth, attention-grabbing wing motion using a sine wave."""
    steps = 40  # More steps for smoothness
    for _ in range(flaps):
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
    await boot_sequence()
    # Create tasks for each animation/function
    motion_sense_task = asyncio.create_task(motion_sense())
    play_Wav_task = asyncio.create_task(play_Wav(random.choice(wavfiles)))
    ambient_breathing_task = asyncio.create_task(ambient_breathing())
    idle_attention_task = asyncio.create_task(idle_attention())
    #move_s1_task = asyncio.create_task(move_s1())
    #move_s2_task = asyncio.create_task(move_s2())
    
    # This will run forever, because no tasks ever finish.
    await asyncio.gather(motion_sense_task, ambient_breathing_task, idle_attention_task)  # 

asyncio.run(main())