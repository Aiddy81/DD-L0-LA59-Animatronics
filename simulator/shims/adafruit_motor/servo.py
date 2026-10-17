"""Same maths as Adafruit's adafruit_motor.servo, so angle -> duty matches the board."""


class _BaseServo:
    def __init__(self, pwm_out, *, min_pulse=750, max_pulse=2250):
        self._pwm_out = pwm_out
        self.set_pulse_width_range(min_pulse, max_pulse)

    def set_pulse_width_range(self, min_pulse=750, max_pulse=2250):
        self._min_duty = int((min_pulse * self._pwm_out.frequency) / 1000000 * 0xFFFF)
        max_duty = (max_pulse * self._pwm_out.frequency) / 1000000 * 0xFFFF
        self._duty_range = int(max_duty - self._min_duty)

    @property
    def fraction(self):
        if self._pwm_out.duty_cycle == 0:
            return None
        return (self._pwm_out.duty_cycle - self._min_duty) / self._duty_range

    @fraction.setter
    def fraction(self, value):
        if value is None:
            self._pwm_out.duty_cycle = 0
            return
        if not 0.0 <= value <= 1.0:
            raise ValueError("Must be 0.0 to 1.0")
        self._pwm_out.duty_cycle = self._min_duty + int(value * self._duty_range)


class Servo(_BaseServo):
    def __init__(self, pwm_out, *, actuation_range=180, min_pulse=750, max_pulse=2250):
        super().__init__(pwm_out, min_pulse=min_pulse, max_pulse=max_pulse)
        self.actuation_range = actuation_range
        self._pwm = pwm_out

    @property
    def angle(self):
        if self.fraction is None:
            return None
        return self.actuation_range * self.fraction

    @angle.setter
    def angle(self, new_angle):
        if new_angle is None:
            self.fraction = None
            return
        if new_angle < 0 or new_angle > self.actuation_range:
            raise ValueError("Angle out of range")
        self.fraction = new_angle / self.actuation_range


class ContinuousServo(_BaseServo):
    @property
    def throttle(self):
        return self.fraction * 2 - 1

    @throttle.setter
    def throttle(self, value):
        if value > 1.0 or value < -1.0:
            raise ValueError("Throttle must be between -1.0 and 1.0")
        if value is None:
            raise ValueError("Continuous servos cannot spin freely")
        self.fraction = (value + 1) / 2
