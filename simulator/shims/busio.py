class I2C:
    """Stand-in I2C bus. Devices on it (PCA9685, LIS3DH) are simulated directly."""

    def __init__(self, scl, sda, *, frequency=100000, timeout=255):
        self.scl = scl
        self.sda = sda
        self.frequency = frequency

    def try_lock(self):
        return True

    def unlock(self):
        pass

    def scan(self):
        return [0x18, 0x40]  # LIS3DH, PCA9685

    def deinit(self):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.deinit()
