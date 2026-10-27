import time

import simhw


class MixerVoice:
    def __init__(self, mixer):
        self._mixer = mixer
        self._level = 1.0
        self._ends_at = 0.0

    def play(self, sample, *, loop=False):
        self._mixer._must_match(sample)
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
        self.bits_per_sample = bits_per_sample
        self._signed = samples_signed
        simhw.mixer_format(sample_rate, channel_count, bits_per_sample, samples_signed)
        self.voice = tuple(MixerVoice(self) for _ in range(voice_count))
        self._output = None

    def _must_match(self, sample):
        # Same checks, order and wording as audiosample_must_match in
        # CircuitPython 9.2 (shared-module/audiocore/__init__.c).
        if sample.sample_rate != self.sample_rate:
            raise ValueError("The sample's sample_rate does not match")
        if sample.channel_count != self.channel_count:
            raise ValueError("The sample's channel_count does not match")
        if sample.bits_per_sample != self.bits_per_sample:
            raise ValueError("The sample's bits_per_sample does not match")
        if getattr(sample, "_signed", self._signed) != self._signed:
            raise ValueError("The sample's signedness does not match")

    @property
    def playing(self):
        return any(v.playing for v in self.voice)

    def play(self, sample, *, voice=0, loop=False):
        self.voice[voice].play(sample, loop=loop)

    def stop_voice(self, voice=0):
        self.voice[voice].stop()

    def deinit(self):
        pass
