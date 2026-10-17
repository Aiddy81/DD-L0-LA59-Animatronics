"""Pin names for the RP2040 Prop-Maker Feather. In the simulator a pin is just a label."""


class Pin:
    def __init__(self, name):
        self.name = name

    def __repr__(self):
        return "board." + self.name


def __getattr__(name):
    # Any upper-case pin name code.py asks for exists, as on the real board.
    if name.isupper():
        pin = Pin(name)
        globals()[name] = pin
        return pin
    raise AttributeError(name)


def I2C():
    import busio
    return busio.I2C(SCL, SDA)  # noqa: F821 - resolved through __getattr__
