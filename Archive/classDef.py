import time

class servoMotion:
    # Emotion flags meanings
    # 0 = No Emotion
    # 1 = Angry
    # 2 = Excited
    # 3 = Ambient

    def __init__(servoMotion, theta_min, theta_max, T, num_points, acceleration, jerk, time_list, theta_list):
        # Define the parameters
        servoMotion.theta_min = theta_min  # Minimum angle (in degrees)
        servoMotion.theta_max = theta_max  # Maximum angle (in degrees)
        servoMotion.T = T  # Total duration of the motion (in seconds)
        servoMotion.num_points = num_points  # Number of points to calculate in the trajectory
        servoMotion.acceleration = acceleration  # Acceleration (degrees/second^2)
        servoMotion.jerk = jerk  # jerk (degrees/second^3)

        # Time step based on the number of points
        servoMotion.dt = servoMotion.T / servoMotion.num_points

        # Define global variables
        servoMotion.time_list = time_list
        servoMotion.theta_list = theta_list

    def calculate_s_curve_angles(self):
    
        for i in range(self.num_points + 1):
            t = i * self.dt  # Current time
            self.time_list.append(t)
    
            # Normalized time
            s = t / self.T
    
            # Quintic polynomial for smooth S-curve
            # P(s) = 10s^3 - 15s^4 + 6s^5
            P = 10 * s**3 - 15 * s**4 + 6 * s**5
    
            # Calculate the actual angle
            angle = self.theta_min + (self.theta_max - self.theta_min) * P
            self.theta_list.append(angle)
    
        # Print the time and angle lists
        print("Time (s):", self.time_list)
        print("Angles (degrees):", self.theta_list)
          
        #def calculate_s_curve_angles(self):
#
    #    # Total time for each phase (simplified assumption for symmetric S-curve)
    #    Tj = (self.acceleration / self.jerk)  # Time to reach full acceleration
    #    Ta = self.T - 2 * Tj  # Time at constant acceleration
#
    #    if Ta < 0:
    #        raise ValueError("Acceleration time is too short for the given jerk and total time.")
#
    #    # Velocity reached at end of each phase
    #    Vj = 0.5 * self.jerk * Tj**2
    #    Va = Vj + self.acceleration * Ta
#
    #    for i in range(self.num_points + 1):
    #        t = i * self.dt  # Current time
    #        self.time_list.append(t)
#
    #        if t < Tj:
    #            # Phase 1: Positive jerk
    #            angle = self.theta_min + 0.5 * self.jerk * t**3
    #        elif t < Tj + Ta:
    #            # Phase 2: Constant acceleration
    #            t2 = t - Tj
    #            angle = self.theta_min + (0.5 * self.jerk * Tj**3) + (0.5 * self.acceleration * t2**2) + (Vj * t2)
    #        elif t < 2 * Tj + Ta:
    #            # Phase 3: Negative jerk to zero acceleration
    #            t3 = t - (Tj + Ta)
    #            angle = self.theta_min + (0.5 * self.jerk * Tj**3) + (self.acceleration * Tj * Ta) + (0.5 * self.acceleration * Ta**2) + (Vj * Ta) + (Va * t3) - (0.5 * self.jerk * t3**3)
    #        elif t < 3 * Tj + Ta:
    #            # Phase 4: Constant velocity
    #            t4 = t - (2 * Tj + Ta)
    #            angle = self.theta_min + (0.5 * self.jerk * Tj**3) + (self.acceleration * Tj * Ta) + (0.5 * self.acceleration * Ta**2) + (Vj * Ta) + (Va * (Tj + Ta)) - (0.5 * self.jerk * Tj**3) + (Va * t4)
    #        elif t < 4 * Tj + Ta:
    #            # Phase 5: Negative jerk to deceleration
    #            t5 = t - (3 * Tj + Ta)
    #            angle = self.theta_max - 0.5 * self.jerk * t5**3
    #        else:
    #            # Phase 6: Deceleration
    #            t6 = t - (4 * Tj + Ta)
    #            angle = self.theta_max - 0.5 * self.acceleration * t6**2
#
    #        self.theta_list.append(angle)

        # Print the time and angle lists
        print("Time (s):", self.time_list)
        print("Angles (degrees):", self.theta_list)

    #def calculate_s_curve_angles(self):
    #    self.time_list 
    #    self.theta_list
#
    #    for i in range(self.num_points + 1):
    #        t = i * self.dt  # Current time
    #        self.time_list.append(t)
#
    #        # Normalized time
    #        s = t / self.T
#
    #        # Quintic polynomial for smooth S-curve
    #        # P(s) = 10s^3 - 15s^4 + 6s^5
    #        P = 10 * s**3 - 15 * s**4 + 6 * s**5
#
    #        # Calculate the actual angle
    #        angle = self.theta_min + (self.theta_max - self.theta_min) * P
    #        self.theta_list.append(angle)
#
    #    # Print the time and angle lists
    #    print("Time (s):", self.time_list)
    #    print("Angles (degrees):", self.theta_list)

    #def moveServo(self):
    #    self.theta_list
#
    #    # Move the servo according to the calculated angles
    #    for angle in self.theta_list:
    #        prop_servo.angle = angle
    #        time.sleep(self.dt)
#
    #    for angle in reversed(self.theta_list):
    #        prop_servo.angle = angle
    #        time.sleep(self.dt)    