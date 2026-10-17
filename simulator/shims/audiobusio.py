import time

import simhw
from audiomixer import Mixer


class I2SOut:
    def __init__(self, bit_clock, word_select, data, *, main_clock=None, left_justified=False):
        self._source = None
        self._ends_at = 0.0

    def play(self, sample, *, loop=False):
        self.stop()
        self._source = sample
        if isinstance(sample, Mixer):
            sample._output = self
        elif getattr(sample, "path", None):
            self._ends_at = float("inf") if loop else time.monotonic() + sample.duration
            simhw.play_wav(sample.path, 1.0, loop)

    def stop(self):
        if isinstance(self._source, Mixer):
            self._source._output = None
        if self._source is not None:
            simhw.stop_audio()
        self._source = None
        self._ends_at = 0.0

    @property
    def playing(self):
        if isinstance(self._source, Mixer):
            return self._source.playing
        return time.monotonic() < self._ends_at

    def pause(self):
        pass

    def resume(self):
        pass

    def deinit(self):
        self.stop()
