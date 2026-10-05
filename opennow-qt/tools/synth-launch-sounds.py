#!/usr/bin/env python3
"""Synthesise the Game Mode launch cues in res/sounds/.

Original procedural sounds written for OpenNOW: filtered noise and two sine
plucks, no sampled or third-party audio. Standard library only, fixed seed,
so every run writes byte-identical files.

    python3 opennow-qt/tools/synth-launch-sounds.py [output-dir]

launch-air.wav   360 ms noise breath, low-pass sweeping 800 Hz -> 4 kHz, peak -24 dBFS
launch-open.wav  620 ms soft A4 + E5 pluck (E5 30 ms later), peak -16 dBFS
"""
import math
import random
import struct
import sys
import wave
from pathlib import Path

RATE = 48000
SEED = 0x0FE11D


def biquad_lowpass(cutoff, q=0.707):
    w = 2 * math.pi * cutoff / RATE
    alpha = math.sin(w) / (2 * q)
    cos_w = math.cos(w)
    a0 = 1 + alpha
    b0 = (1 - cos_w) / 2 / a0
    return b0, 2 * b0, b0, -2 * cos_w / a0, (1 - alpha) / a0


def normalise(samples, peak_dbfs):
    peak = max(abs(s) for s in samples) or 1.0
    gain = 10 ** (peak_dbfs / 20) / peak
    return [s * gain for s in samples]


def write(path, samples):
    with wave.open(str(path), "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(b"".join(struct.pack("<h", round(max(-1.0, min(1.0, s)) * 32767)) for s in samples))


def air():
    rng = random.Random(SEED)
    count = int(0.360 * RATE)
    x1 = x2 = y1 = y2 = 0.0
    hp_prev_in = hp_prev_out = 0.0
    hp = math.exp(-2 * math.pi * 180 / RATE)
    out = []
    for n in range(count):
        p = n / (count - 1)
        cutoff = 800 * (4000 / 800) ** p
        b0, b1, b2, a1, a2 = biquad_lowpass(cutoff)
        x0 = rng.uniform(-1, 1)
        y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        x2, x1, y2, y1 = x1, x0, y1, y0
        high = hp * (hp_prev_out + y0 - hp_prev_in)
        hp_prev_in, hp_prev_out = y0, high
        rise = 0.62
        env = math.sin(math.pi / 2 * p / rise) ** 2 if p < rise else math.cos(math.pi / 2 * (p - rise) / (1 - rise)) ** 2
        out.append(high * env)
    return normalise(out, -24)


def pluck(freq, start, length):
    attack = 0.040
    tau = 0.500 / 3
    out = []
    for n in range(length):
        t = n / RATE - start
        if t < 0:
            out.append(0.0)
            continue
        env = (0.5 - 0.5 * math.cos(math.pi * t / attack)) if t < attack else math.exp(-(t - attack) / tau)
        phase = 2 * math.pi * freq * t
        tone = math.sin(phase) + 0.18 * math.sin(2 * phase) * math.exp(-t / 0.08) + 0.05 * math.sin(3 * phase) * math.exp(-t / 0.05)
        out.append(tone * env)
    return out


def open_cue():
    length = int(0.620 * RATE)
    a4 = pluck(440.0, 0.0, length)
    e5 = pluck(659.255, 0.030, length)
    mixed = [a + 0.72 * e for a, e in zip(a4, e5)]
    tail = int(0.060 * RATE)
    for n in range(tail):
        mixed[length - tail + n] *= 0.5 + 0.5 * math.cos(math.pi * n / tail)
    return normalise(mixed, -16)


if __name__ == "__main__":
    target = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "res" / "sounds"
    target.mkdir(parents=True, exist_ok=True)
    write(target / "launch-air.wav", air())
    write(target / "launch-open.wav", open_cue())
    for name in ("launch-air.wav", "launch-open.wav"):
        print(target / name, (target / name).stat().st_size, "bytes")
