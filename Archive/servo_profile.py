#Explanation
#Parameters:
#
#total_time, num_points, max_velocity, max_acceleration, max_jerk are the same as before.
#Profile Generation:
#
#Using basic Python lists and loops instead of numpy for array operations.
#Normalization:
#
#Manually normalize the velocity and position lists.
#Output:
#
#Prints the data in CSV format for easy transfer to a computer for plotting.


import time

def s_curve_profile(total_time, num_points, max_velocity, max_acceleration, max_jerk):
    t = [i * (total_time / (num_points - 1)) for i in range(num_points)]
    dt = t[1] - t[0]

    jerk = [0] * num_points
    accel = [0] * num_points
    velocity = [0] * num_points
    position = [0] * num_points

    phase_time = total_time / 7

    for i in range(num_points):
        if t[i] < phase_time:  # Phase 1
            jerk[i] = max_jerk
        elif t[i] < 2 * phase_time:  # Phase 2
            jerk[i] = 0
        elif t[i] < 3 * phase_time:  # Phase 3
            jerk[i] = -max_jerk
        elif t[i] < 4 * phase_time:  # Phase 4
            jerk[i] = 0
        elif t[i] < 5 * phase_time:  # Phase 5
            jerk[i] = -max_jerk
        elif t[i] < 6 * phase_time:  # Phase 6
            jerk[i] = 0
        else:  # Phase 7
            jerk[i] = max_jerk

        if i > 0:
            accel[i] = accel[i-1] + jerk[i] * dt
            velocity[i] = velocity[i-1] + accel[i] * dt
            position[i] = position[i-1] + velocity[i] * dt

    # Normalize profiles
    max_vel = max(velocity)
    max_pos = max(position)
    velocity = [max_velocity * v / max_vel for v in velocity]
    position = [max_velocity * total_time * p / max_pos for p in position]
    
    return t, position, velocity, accel, jerk

# Parameters
total_time = 10.0  # Total time of the motion
num_points = 100  # Number of points in the profile
max_velocity = 1.0  # Maximum velocity
max_acceleration = 0.5  # Maximum acceleration
max_jerk = 0.1  # Maximum jerk

# Generate s-curve profile
t, position, velocity, accel, jerk = s_curve_profile(total_time, num_points, max_velocity, max_acceleration, max_jerk)

# Print the profiles
print("Time,Position,Velocity,Acceleration,Jerk")
for i in range(num_points):
    print(f"{t[i]},{position[i]},{velocity[i]},{accel[i]},{jerk[i]}")

# Note: In CircuitPython, use serial communication to output data to a computer for plotting.
