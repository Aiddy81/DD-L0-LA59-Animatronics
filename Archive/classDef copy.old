class emotion:
    # Emotion flags meanings
    # 0 = No Emotion
    # 1 = Angry
    # 2 = Excited
    # 3 = Ambient

    def __init__(emotionObject, totTime, numPoints, maxVelocity, maxAcceleration, maxJerk, position, emotionType, max_pos, max_vel):
        emotionObject.position = position
        emotionObject.maxVelocity = maxVelocity
        emotionObject.maxAcceleration = maxAcceleration
        emotionObject.maxJerk = maxJerk
        emotionObject.totTime = totTime
        emotionObject.numPoints = numPoints
        emotionObject.emotionType = emotionType
        emotionObject.max_pos = max_pos
        emotionObject.max_vel = max_vel

    def sCurveProfile(self):
        t = [i * (self.totTime / (self.numPoints - 1)) for i in range(self.numPoints)]
        dt = t[1] - t[0]

        jerk = [0] * self.numPoints
        accel = [0] * self.numPoints
        velocity = [0] * self.numPoints
        position = [0] * self.numPoints

        phase_time = self.totTime / 7

        for i in range(self.numPoints):
            if t[i] < phase_time:  # Phase 1
                jerk[i] = self.maxJerk
            elif t[i] < 2 * phase_time:  # Phase 2
                jerk[i] = 0
            elif t[i] < 3 * phase_time:  # Phase 3
                jerk[i] = -self.maxJerk
            elif t[i] < 4 * phase_time:  # Phase 4
                jerk[i] = 0
            elif t[i] < 5 * phase_time:  # Phase 5
                jerk[i] = -self.maxJerk
            elif t[i] < 6 * phase_time:  # Phase 6
                jerk[i] = 0
            else:  # Phase 7
                jerk[i] = self.maxJerk

            if i > 0:
                # Calculate acceleration using maxAcceleration
                accel[i] = min(self.maxAcceleration, accel[i-1] + jerk[i] * dt)
                velocity[i] = velocity[i-1] + accel[i] * dt
                position[i] = position[i-1] + velocity[i] * dt

        # Normalize profiles
        max_vel = max(velocity)
        max_pos = max(position)
        velocity = [self.maxVelocity * v / max_vel for v in velocity]
        position = [self.maxVelocity * self.totTime * p / max_pos for p in position]
        
        self.max_pos = max_pos
        self.max_vel = max_vel
        
        return position