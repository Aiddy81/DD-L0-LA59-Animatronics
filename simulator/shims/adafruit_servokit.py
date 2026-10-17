"""ServoKit on a simulated PCA9685. Every duty write is reported to the simulator."""
import simhw
from adafruit_motor import servo


class _Channel:
    def __init__(self, pca, index):
        self._pca = pca
        self._index = index
        self._duty = 0

    @property
    def frequency(self):
        return self._pca.frequency

    @property
    def duty_cycle(self):
        return self._duty

    @duty_cycle.setter
    def duty_cycle(self, value):
        if not 0 <= value <= 0xFFFF:
            raise ValueError("Out of range: value {} not 0 <= value <= 65,535".format(value))
        # The PCA9685 only has 12-bit resolution; 0xFFFF means fully on.
        self._duty = 0xFFFF if value == 0xFFFF else (value >> 4) << 4
        simhw.set_pca(self._index, self._duty)
        simhw.charge("pca")


class _PCA9685:
    def __init__(self, frequency):
        self.frequency = frequency
        self.channels = [_Channel(self, i) for i in range(16)]


class _Servo:
    def __init__(self, kit):
        self.kit = kit

    def __getitem__(self, index):
        num_channels = self.kit._channels
        if index >= num_channels or index < 0:
            raise ValueError("servo must be 0-{}!".format(num_channels - 1))
        item = self.kit._items[index]
        if item is None:
            item = servo.Servo(self.kit._pca.channels[index])
            self.kit._items[index] = item
            return item
        if isinstance(item, servo.Servo):
            return item
        raise ValueError("Channel {} is already in use.".format(index))

    def __len__(self):
        return len(self.kit._items)


class _ContinuousServo:
    def __init__(self, kit):
        self.kit = kit

    def __getitem__(self, index):
        num_channels = self.kit._channels
        if index >= num_channels or index < 0:
            raise ValueError("servo must be 0-{}!".format(num_channels - 1))
        item = self.kit._items[index]
        if item is None:
            item = servo.ContinuousServo(self.kit._pca.channels[index])
            self.kit._items[index] = item
            return item
        if isinstance(item, servo.ContinuousServo):
            return item
        raise ValueError("Channel {} is already in use.".format(index))

    def __len__(self):
        return len(self.kit._items)


class ServoKit:
    def __init__(self, *, channels, i2c=None, address=0x40,
                 reference_clock_speed=25000000, frequency=50):
        if channels not in [8, 16]:
            raise ValueError("servo_channels must be 8 or 16!")
        self._items = [None] * channels
        self._channels = channels
        self._pca = _PCA9685(frequency)
        self._servo = _Servo(self)
        self._continuous_servo = _ContinuousServo(self)

    @property
    def servo(self):
        return self._servo

    @property
    def continuous_servo(self):
        return self._continuous_servo
