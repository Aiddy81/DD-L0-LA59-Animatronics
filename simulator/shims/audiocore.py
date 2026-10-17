import os
import wave


class WaveFile:
    def __init__(self, file, buffer=None):
        self.path = os.path.abspath(file.name)
        with wave.open(self.path, "rb") as w:
            self.sample_rate = w.getframerate()
            self.channel_count = w.getnchannels()
            self.bits_per_sample = w.getsampwidth() * 8
            self.duration = w.getnframes() / float(self.sample_rate)

    def deinit(self):
        pass


class RawSample:
    def __init__(self, buffer, *, channel_count=1, sample_rate=8000, single_buffer=True):
        self.path = None
        self.duration = len(buffer) / float(sample_rate * channel_count)
