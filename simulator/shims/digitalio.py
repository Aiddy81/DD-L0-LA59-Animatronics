import simhw


class Direction:
    INPUT = "INPUT"
    OUTPUT = "OUTPUT"


class Pull:
    UP = "UP"
    DOWN = "DOWN"


class DriveMode:
    PUSH_PULL = "PUSH_PULL"
    OPEN_DRAIN = "OPEN_DRAIN"


class DigitalInOut:
    def __init__(self, pin):
        self._pin = pin
        self.direction = Direction.INPUT
        self.pull = None
        self.drive_mode = DriveMode.PUSH_PULL
        self._value = False

    @property
    def value(self):
        return self._value

    @value.setter
    def value(self, value):
        self._value = bool(value)
        simhw.set_pin(self._pin.name, self._value)

    def switch_to_output(self, value=False, drive_mode=DriveMode.PUSH_PULL):
        self.direction = Direction.OUTPUT
        self.drive_mode = drive_mode
        self.value = value

    def switch_to_input(self, pull=None):
        self.direction = Direction.INPUT
        self.pull = pull

    def deinit(self):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.deinit()
