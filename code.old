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
# use prop_servo.throttle = 0.0 to stop


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

#def s_curve_profile(total_time, num_points, max_velocity, max_acceleration, max_jerk):
#    t = [i * (total_time / (num_points - 1)) for i in range(num_points)]
#    dt = t[1] - t[0]
#
#    global max_vel
#    global max_pos
#    global velocity
#    global position
#
#    jerk = [0] * num_points
#    accel = [0] * num_points
#    velocity = [0] * num_points
#    position = [0] * num_points
#
#    phase_time = total_time / 7
#
#    for i in range(num_points):
#        if t[i] < phase_time:  # Phase 1
#            jerk[i] = max_jerk
#        elif t[i] < 2 * phase_time:  # Phase 2
#            jerk[i] = 0
#        elif t[i] < 3 * phase_time:  # Phase 3
#            jerk[i] = -max_jerk
#        elif t[i] < 4 * phase_time:  # Phase 4
#            jerk[i] = 0
#        elif t[i] < 5 * phase_time:  # Phase 5
#            jerk[i] = -max_jerk
#        elif t[i] < 6 * phase_time:  # Phase 6
#            jerk[i] = 0
#        else:  # Phase 7
#            jerk[i] = max_jerk
#
#        if i > 0:
#            accel[i] = accel[i-1] + jerk[i] * dt
#            velocity[i] = velocity[i-1] + accel[i] * dt
#            position[i] = position[i-1] + velocity[i] * dt
#
#    # Normalize profiles
#    max_vel = max(velocity)
#    max_pos = max(position)
#    velocity = [max_velocity * v / max_vel for v in velocity]
#    position = [max_velocity * total_time * p / max_pos for p in position]

#def calculate_s_curve_angles_orig():
#    global time_list, theta_list
#
#    for i in range(num_points + 1):
#        t = i * dt  # Current time
#        time_list.append(t)
#
#        # Normalized time
#        s = t / T
#
#        # Quintic polynomial for smooth S-curve
#        # P(s) = 10s^3 - 15s^4 + 6s^5
#        P = 10 * s**3 - 15 * s**4 + 6 * s**5
#
#        # Calculate the actual angle
#        angle = theta_min + (theta_max - theta_min) * P
#        theta_list.append(angle)
#
#    # Print the time and angle lists
#    print("Time (s):", time_list)
#    print("Angles (degrees):", theta_list)

def move_servo(servo):

    # Move the servo according to the calculated angles
    for angle in servo.theta_list:
        prop_servo.angle = angle
        time.sleep(servo.dt)

    for angle in reversed(servo.theta_list):
        prop_servo.angle = angle
        time.sleep(servo.dt)
    

    
set_once = 1

if set_once == 1:
    #here
    print("")

    
# s_curve_profile(total_time, num_points, max_velocity, max_acceleration, max_jerk):
wingLeft =  classDef.servoMotion(0, 90, 10, 100, 10, 10, [], [])
# Define the parameters
wingLeft.theta_min = 0  # Minimum angle (in degrees)
wingLeft.theta_max = 90  # Maximum angle (in degrees)
wingLeft.T = 20  # Total duration of the motion (in seconds)
wingLeft.num_points = 100  # Number of points to calculate in the trajectory
wingLeft.acceleration = 10  # Acceleration (degrees/second^2)
wingLeft.jerk = 10  # Jerk (degrees/second^3)

wingLeft.calculate_s_curve_angles()

#scurve(wingLeft.min_angle,wingLeft.max_angle, wingLeft.totTime, wingLeft.maxAcceleration) 

#print(angryEmo.position)



# Time step based on the number of points
#dt = T / num_points

# Define global variables
#time_list = []
#theta_list = []


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

    #s_curve_profile(total_time, num_points, max_velocity, max_acceleration, max_jerk): 
    #print("S curve")   
    # Assuming you've calculated position[] from your S-curve profile
    #print(max_vel)
    #print("max position ", max_pos) 
    #print(velocity)
    #print(position)
    if set_once == 1:
        #wingLeft.s_curve_profile(10, 2000, 0.5, 0.2, 0.1)
        set_once = 0
    #print(max_vel)
    #print(max_pos) 
    #print(velocity)
    #print(position)
    #for p in wingLeft.position:
    #    # Map position to servo angle (0° to 180°)
    #    #servo_angle = int((p / max_pos) * 90)
    #    lServoAngle = wingLeft.min_angle + (wingLeft.max_angle - wingLeft.min_angle) * p
    #    print(lServoAngle)

    # First calculate the S-curve angles
    #calculate_s_curve_angles()

    # Then move the servo using the calculated angles
    #move_servo(wingLeft)
    #wingLeft.time_list = []
    #wingLeft.theta_list = []

    prop_servo.angle = 0
    prop_servo.angle = 90
    time.sleep(1)
    prop_servo.angle = 70
    time.sleep(1)
    prop_servo.angle = 0



    #wingLeft.position =[]
    #prop_servo.angle = lServoAngle
    #wingLeft.totTime = 2
    #wingLeft.numPoints = 100
    #wingLeft.maxVelocity = 1
    #wingLeft.maxAcceleration = 1
    #wingLeft.maxJerk = 1
    #wingLeft.emotionType = 1
    #wingLeft.min_angle = 0
    #wingLeft.max_angle = 50 #FIXME 180 scales to 90 degrees
    #wingLeft.position = wingLeft.sCurveProfile()
    #for p in wingLeft.position:
    #    # Map position to servo angle (0° to 180°)
    #    #servo_angle = int((p / max_pos) * 90)
    #    lServoAngle = wingLeft.min_angle + (wingLeft.max_angle - wingLeft.min_angle) * p
    #    print("M2 " + str(lServoAngle))
        #print("max position ", max_pos) 
        #print("servo angle ", servo_angle)
        #if servo_angle > 90:
            #print("here")
            #time.sleep(10)
        #else:
            #prop_servo.angle = servo_angle
            #time.sleep(10)
    #print(wingLeft.position)
    #for a in angles:
        #prop_servo.angle = a
        #time.sleep(1)
   # for p in wingLeft.position:
        # Map position to servo angle (0° to 180°)
        #max_angle = 30
        #servo_angle = int((p / wingLeft.max_pos) * max_angle)
   #     servo_angle = int(p / wingLeft.max_angle)
        #print(servo_angle)
   #     prop_servo.angle =  servo_angle
        #time.sleep(10)
    #    print("max position ", wingLeft.max_pos) 
    #    print("servo angle ", servo_angle)
        #if servo_angle > max_angle:
        #    print("here")
        #    #time.sleep(2)
        #elif servo_angle < min_angle:
        #    print("here2")
        #else:
        #    prop_servo.angle = servo_angle
        #    #time.sleep(2)

    #print("0")
    #prop_servo.angle = 0
    #time.sleep(10)
    #print("90")
    #prop_servo.angle = 90
    #time.sleep(10)

    #continous servo
    #print("forward")
    ##prop_servo.throttle = 0.1
    #time.sleep(2)
    #print("stop")
    ##prop_servo.throttle = 0.0
    #time.sleep(2)
    #print("reverse")
   ## prop_servo.throttle = -0.1
    #time.sleep(2)
    #print("stop")
    ##prop_servo.throttle = 0.0
    #time.sleep(2)
    
    

    
   
    
     
   
    #look for taps, depending on how often or hard will determine what 
    #animation, sounds and light colours are played
    #Motor controls will be a separate thread to wav files and led
    #play_file(random.choice(wavefiles))