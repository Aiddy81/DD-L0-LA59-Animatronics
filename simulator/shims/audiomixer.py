import time

import simhw


class MixerVoice:
    def __init__(self, mixer):
        self._mixer = mixer
        self._level = 1.0
        self._ends_at = 0.0

    def play(self, sample, *, loop=False):
        self._ends_at = float("inf") if loop else time.monotonic() + sample.duration
        # Silent unless the mixer itself is being played by an output, as on hardware.
        if self._mixer._output is not None and sample.path:
            simhw.play_wav(sample.path, self._level, loop)

    def stop(self):
        self._ends_at = 0.0
        simhw.stop_audio()

    @property
    def level(self):
        return self._level

    @level.setter
    def level(self, value):
        self._level = float(value)
        simhw.set_volume(self._level)

    @property
    def playing(self):
        return time.monotonic() < self._ends_at


class Mixer:
    def __init__(self, *, voice_count=2, buffer_size=1024, channel_count=2,
                 bits_per_sample=16, samples_signed=True, sample_rate=8000):
        self.sample_rate = sample_rate
        self.channel_count = channel_count
        self.voice = tuple(MixerVoice(self) for _ in range(voice_count))
        self._output = None

    @property
    def playing(self):
        return any(v.playing for v in self.voice)

    def play(self, sample, *, voice=0, loop=False):
        self.voice[voice].play(sample, loop=loop)

    def stop_voice(self, voice=0):
        self.voice[voice].stop()

    def deinit(self):
        pass
