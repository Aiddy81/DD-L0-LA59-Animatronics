# Notes
# Servo's
#           - Servo[0]
#                   - ngle_min=40 = heighst point
#                   - Servoangle_max = lowest point
#           - Servo[1]
#                   - Servo2_min = lowest point
#                   - Servo2_max = heighst point
"""Board setup shared by every behaviour: sensors, PCA9685 channel map, LEDs,
audio, and the events the behaviours coordinate through."""
import board
from digitalio import DigitalInOut, Direction
import audiocore
import audiobusio
import audiomixer
import busio
import pwmio
import os
import adafruit_lis3dh
import asyncio
from adafruit_servokit import ServoKit


# onboard LIS3DH
#i2c = board.I2C()  # uses board.SCL and board.SDA
i2c = busio.I2C(board.SCL, board.SDA)  # Create an I2C bus instance
int1 = DigitalInOut(board.ACCELEROMETER_INTERRUPT)
lis3dh = adafruit_lis3dh.LIS3DH_I2C(i2c, int1=int1)
lis3dh.range = adafruit_lis3dh.RANGE_4_G  # 2 g would clip below HARD_MOTION_G

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
trench_active = asyncio.Event()  # the trench run owns the white LED while set

wavfiles = [
    file
    for file in os.listdir("sounds/")
    if (file.endswith(".wav") and not file.startswith("._"))
]


async def play_wav(filename):
    """Starts a WAV file on the mixer and returns straight away."""
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
