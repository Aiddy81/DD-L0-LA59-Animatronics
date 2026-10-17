import simhw


class PWMOut:
    def __init__(self, pin, *, duty_cycle=0, frequency=500, variable_frequency=False):
        self._pin = pin
        self.frequency = frequency
        self._duty = 0
        self.duty_cycle = duty_cycle

    @property
    def duty_cycle(self):
        return self._duty

    @duty_cycle.setter
    def duty_cycle(self, value):
        value = int(value)
        if not 0 <= value <= 0xFFFF:
            raise ValueError("duty_cycle must be 0-65535")
        self._duty = value
        simhw.set_pwm_pin(self._pin.name, value)

    def deinit(self):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.deinit()
