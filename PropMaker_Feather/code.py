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
import asyncio
import math
import random
from adafruit_servokit import ServoKit


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

mainEye_red.angle = 0  # Turn off Red
mainEye_green.angle = 0  # Turn off green
mainEye_blue.angle = 0   # Turn off blue
smallEye_red.angle = 0  # Turn off red
smallEye_green.angle = 0  # Turn off green
smallEye_blue.angle = 0   # Turn off blue
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



white_led= pwmio.PWMOut(board.EXTERNAL_NEOPIXELS, frequency=5000)
#white_led.duty_cycle = 100  # Set to half brightness (32768 out of 65535)
#prop_servo = servo.Servo(pwm, min_pulse=750, max_pulse=2250)
#prop_servo.actuation_range = 180
#angle_plus = True

pause_breathing = asyncio.Event()
pause_breathing.clear()  # Start with breathing enabled
reset_idle_event = asyncio.Event()



# Global Variables for functions
motion_count = 0  # This variable counts the number of motion events detected, 
                  # and is used to trigger angry animation. 
IDLE_MIN_SECONDS = 15 * 60    # 5 minutes # Minimum idle time before triggering idle animation
IDLE_MAX_SECONDS = 120 * 60   # 30 minutes # Maximum idle time before triggering idle animation
ANGRY_MIN_WINDOW = 1 * 60    # 1 minute (in seconds)
ANGRY_MAX_WINDOW = 3 * 60    # 3 minutes (in seconds)

wavfiles = [
    file
    for file in os.listdir("sounds/")
    if (file.endswith(".wav") and not file.startswith("._"))
]

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
    SERVO2_MIN = 40   # Adjust until both wings close equally, used to adjust the left wing as mounted inverted
    SERVO2_MAX = 60   # Adjust until both wings close equally, used to adjust the left wing as mounted inverted
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
            angle1 = SERVO2_MIN + (SERVO2_MAX - SERVO2_MIN) * (1 - norm)
            kit.servo[1].angle = angle1
            await asyncio.sleep(duration / steps)
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
    # WhiteLED
    white_led.duty_cycle = round(65535/2)  # Set white LED to full brightness
    await asyncio.sleep(0.5)  # Hold for a moment
    white_led.duty_cycle = 0  # Turn off white LED
    await asyncio.sleep(0.5)  # Hold for a moment
    white_led.duty_cycle = round(65535/2)  # Set white LED to full brightness
    await asyncio.sleep(0.5)  # Hold for a moment
    white_led.duty_cycle = 0  # Turn off white LED
    white_led.duty_cycle = 1000 # Set white LED to very low brightness
    print("Boot sequence complete!")

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

async def random_led_scheduler(min_interval=5, max_interval=60):
    """Continuously trigger the LED effect at random intervals."""
    while True:
        #await asyncio.sleep(random.uniform(min_interval, max_interval))
        #duration = random.uniform(2, 6)  # How long the flash effect lasts
        #await data_processing_led(duration)
        interval = random.uniform(min_interval, max_interval)
        print(f"Next LED activation in: {interval:.2f} seconds")
        await asyncio.sleep(interval)

        duration = random.uniform(2, 6)
        print(f"LED will flash for: {duration:.2f} seconds")
        await data_processing_led(duration)

async def data_processing_led(duration=10):
    """Flash an LED to simulate data processing with random brightness and timing. :param duration: How long to run the flashing effect in seconds"""
    start_time = time.monotonic()
    while time.monotonic() - start_time < duration:
        # Random brightness between 0 (off) and 65535 (full brightness)
        brightness = random.randint(0, 65535)
        white_led.duty_cycle = brightness

        # Random duration between flashes
        delay = random.uniform(0.02, 0.25)
        time.sleep(delay)

        # Occasionally turn fully off to simulate pauses in data
        if random.random() < 0.2:
            white_led.duty_cycle = 0
            time.sleep(random.uniform(0.05, 0.15))

    # Ensure LED is turned off at the end
    white_led.duty_cycle = 1000


def clamp(val, min_val, max_val):
    return max(min_val, min(max_val, val))

async def ambient_breathing(period=4, angle_min=50, angle_max=65):
    """Smoothly moves both wings up and down in a breathing pattern, with blue/red LED color and clamped servo angles."""
    t = 0
    while True:
        # Normalized sine wave: norm goes from 0 (down) to 1 (up)
        norm = (math.sin(2 * math.pi * t / period) + 1) / 2
        angle = angle_min + (angle_max - angle_min) * norm

        # Calibrated and inverted for servo 2
        SERVO2_MIN = 35  # Adjust as needed
        SERVO2_MAX = 50  # Adjust as needed
        angle0 = clamp(angle, 0, 180)
        angle1 = clamp(SERVO2_MIN + (SERVO2_MAX - SERVO2_MIN) * (1 - norm), 0, 180)

        kit.servo[0].angle = angle0
        kit.servo[1].angle = angle1

        # LED color: blue at bottom, red at top, smooth transition
        min_brightness = 60
        max_brightness = 360
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

async def nose_boops(boops=2, angle_min=20, angle_max=70, duration=1.2):
    await play_Wav("noseBoops.wav")
    """Smooth, gentle 'boop' animation using a sine wave."""
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

async def attention_animation(flaps=5, angle_min=10, angle_max=60, duration=0.2):
    await play_Wav("youWho.wav")
    SERVO2_MIN = 40   # Adjust until both wings close equally, used to adjust the left wing as mounted inverted
    SERVO2_MAX = 60   # Adjust until both wings close equally, used to adjust the left wing as mounted inverted

    """Smooth, attention-grabbing wing motion with full LED color cycling and servo calibration."""
    steps = 60
    for _ in range(flaps):
        for i in range(steps + 1):
            phase = 2 * math.pi * i / steps
            norm = (math.sin(phase - math.pi/2) + 1) / 2
            angle0 = angle_min + (angle_max - angle_min) * norm
            angle1 = SERVO2_MIN + (SERVO2_MAX - SERVO2_MIN) * (1 - norm)
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
    kit.servo[1].angle = SERVO2_MIN
    mainEye_red.angle = 0
    mainEye_green.angle = 0
    mainEye_blue.angle = 0
    smallEye_red.angle = 0
    smallEye_green.angle = 0
    smallEye_blue.angle = 0

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
    random_led_schedule_task = asyncio.create_task(random_led_scheduler())
    #move_s1_task = asyncio.create_task(move_s1())
    #move_s2_task = asyncio.create_task(move_s2())
    
    # This will run forever, because no tasks ever finish.
    await asyncio.gather(motion_sense_task, ambient_breathing_task, idle_attention_task, random_led_schedule_task)

asyncio.run(main())