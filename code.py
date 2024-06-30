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

# servo control - FeeTech FS90MR
#Angle Range: 
#120 degrees @ 900-2100 µsec
#180 degrees @ 500 - 2500 µsec
#pwm = pwmio.PWMOut(board.EXTERNAL_SERVO, duty_cycle=2 ** 15, frequency=50)
pwm = pwmio.PWMOut(board.EXTERNAL_SERVO, frequency=50)
prop_servo = servo.Servo(pwm, min_pulse=750, max_pulse=2250)
prop_servo.actuation_range = 180
#angle_plus = True

# servo control - continous
# uses prop_servo.throttle to control 0 is stop values either side are direction and speed
#pwm = pwmio.PWMOut(board.EXTERNAL_SERVO, frequency=50)
#prop_servo = servo.ContinousServo(pwm, min_pulse = 700, max_pulse = 2300)
#prop_servo = servo.ContinuousServo(pwm, min_pulse = 700, max_pulse = 2300)


wavfiles = [
    file
    for file in os.listdir("sounds/")
    if (file.endswith(".wav") and not file.startswith("._"))
]


def play_Wav(filename):
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

play_once = 1

while True:
    led.value = True
    time.sleep(0.1)
    led.value = False
    time.sleep(0.5)
    # read and print LIS3DH values
    x, y, z = [
        value / adafruit_lis3dh.STANDARD_GRAVITY for value in lis3dh.acceleration
    ]
    #print(f"x = {x:.3f} G, y = {y:.3f} G, z = {z:.3f} G")

    #if x < 0 and y < 0 and z < 0:
    if z < 0 or z > 1.1:
        play_once = 1

    if play_once == 1:
        
        #print("Hello World!")
        #print("Audio files found:", wavfiles) 
        play_once = 0
        play_Wav(random.choice(wavfiles))
        #play_Wav(wavfiles[0])
        #time.sleep(10)
        
        

    print("forward")
    #prop_servo.throttle = 0.1
    time.sleep(2)
    print("stop")
    #prop_servo.throttle = 0.0
    time.sleep(2)
    print("reverse")
   # prop_servo.throttle = -0.1
    time.sleep(2)
    print("stop")
    #prop_servo.throttle = 0.0
    time.sleep(2)
    
    

    
   
    
     
   
    #look for taps, depending on how often or hard will determine what 
    #animation, sounds and light colours are played
    #Motor controls will be a separate thread to wav files and led
    #play_file(random.choice(wavefiles))