"""LIS3DH fed from the simulator's motion controls.

Readings are clipped to the configured range like the real sensor, so with
RANGE_2_G nothing above ~2 g is ever reported.
"""
import math

import simhw

STANDARD_GRAVITY = 9.806

RANGE_16_G = 0b11
RANGE_8_G = 0b10
RANGE_4_G = 0b01
RANGE_2_G = 0b00

DATARATE_1344_HZ = 0b1001
DATARATE_400_HZ = 0b0111
DATARATE_200_HZ = 0b0110
DATARATE_100_HZ = 0b0101
DATARATE_50_HZ = 0b0100
DATARATE_25_HZ = 0b0011
DATARATE_10_HZ = 0b0010
DATARATE_1_HZ = 0b0001
DATARATE_POWERDOWN = 0
DATARATE_LOWPOWER_1K6HZ = 0b1000
DATARATE_LOWPOWER_5KHZ = 0b1001

_FULL_SCALE_G = {RANGE_2_G: 2, RANGE_4_G: 4, RANGE_8_G: 8, RANGE_16_G: 16}


class LIS3DH:
    def __init__(self, int1=None, int2=None):
        self._range = RANGE_4_G
        self.data_rate = DATARATE_400_HZ

    @property
    def range(self):
        return self._range

    @range.setter
    def range(self, value):
        self._range = value

    @property
    def acceleration(self):
        # The sensor saturates one LSB below full scale.
        limit = _FULL_SCALE_G[self._range] * (1 - 1 / 32768)
        x, y, z = simhw.accel()
        return tuple(max(-limit, min(limit, v)) * STANDARD_GRAVITY for v in (x, y, z))

    def shake(self, shake_threshold=30, avg_count=10, total_delay=0.1):
        x, y, z = self.acceleration
        return math.sqrt(x * x + y * y + z * z) > shake_threshold

    def read_adc_raw(self, adc):
        return 0

    def read_adc_mV(self, adc):
        return 0

    @property
    def tapped(self):
        return False

    def set_tap(self, tap, threshold, *, time_limit=10, time_latency=20,
                time_window=255, click_cfg=None):
        pass


class LIS3DH_I2C(LIS3DH):
    def __init__(self, i2c, *, address=0x18, int1=None, int2=None):
        super().__init__(int1=int1, int2=int2)


class LIS3DH_SPI(LIS3DH):
    def __init__(self, spi, cs, *, baudrate=100000, int1=None, int2=None):
        super().__init__(int1=int1, int2=int2)
