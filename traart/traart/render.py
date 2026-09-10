"""Score -> audio.

Deliberately thin: pick the pulses you want, ask the time map where they land,
ask the voice what pitch they are, add a click-free envelope, sum.
"""

from __future__ import annotations

import wave
from dataclasses import dataclass, field

import numpy as np

from .ir import Score
from .timemap import LinearTimeMap, TimeMap
from .voices import Voice, voices_for


@dataclass
class RenderConfig:
    sample_rate: int = 44100
    time_map: TimeMap = field(default_factory=LinearTimeMap)
    channels: list[str] | None = None      # None = every active, audible channel
    include_gates: bool = False           # also sound the 729 SP / AOD gates
    voices: dict[str, Voice] | None = None
    ramp_ms: float = 3.0                   # anti-click attack/release
    stereo: bool = True
    markers: bool = False                  # click on PMT gate open
    marker_gain: float = 0.15
    peak: float = 0.89                     # normalisation target
    amplitude_gamma: float = 1.0           # <1 lifts quiet pulses


def render(score: Score, cfg: RenderConfig | None = None) -> tuple[np.ndarray, int]:
    cfg = cfg or RenderConfig()
    sr = cfg.sample_rate

    boundaries = sorted({p.t0_ns for p in score.pulses} |
                        {p.t1_ns for p in score.pulses} |
                        {m.t_ns for m in score.markers} | {0, score.duration_ns})
    cfg.time_map.build(boundaries)

    total_s = cfg.time_map(score.duration_ns)
    n = int(np.ceil(total_s * sr)) + 1
    ncols = 2 if cfg.stereo else 1
    buf = np.zeros((n, ncols), dtype=np.float64)

    keys = cfg.channels or score.channel_keys(only_audible=not cfg.include_gates)
    voices = cfg.voices or voices_for(keys)

    for p in score.pulses:
        if p.channel not in keys or not p.is_on or p.duration_ns <= 0:
            continue
        voice = voices[p.channel]
        a = cfg.time_map(p.t0_ns)
        b = cfg.time_map(p.t1_ns)
        i0, i1 = int(a * sr), int(b * sr)
        if i1 <= i0:
            continue
        m = i1 - i0
        t = np.arange(m) / sr

        seg = np.zeros(m)
        for tone in p.tones:
            if tone.amp <= 0:
                continue
            f = voice.pitch_hz(tone.dds_hz, tone.detuning_hz)
            amp = _amp_envelope(tone, t, p.duration_ns) ** cfg.amplitude_gamma
            wave_ = np.zeros(m)
            for mult, h in voice.harmonics:
                wave_ += h * np.sin(2 * np.pi * f * mult * t + tone.phase)
            seg += amp * wave_
        seg *= voice.gain
        seg *= _envelope(m, sr, cfg.ramp_ms)
        _add(buf, i0, seg, voice.pan, cfg.stereo)

    if cfg.markers:
        for mk in score.markers:
            if mk.kind not in ("pmt_gate_open", "readout"):
                continue
            i0 = int(cfg.time_map(mk.t_ns) * sr)
            m = max(int(0.004 * sr), 8)
            t = np.arange(m) / sr
            click = (np.sin(2 * np.pi * 3200 * t) * np.exp(-t / 0.0012)
                     * cfg.marker_gain)
            _add(buf, i0, click, 0.0, cfg.stereo)

    peak = np.abs(buf).max()
    if peak > 0:
        buf *= cfg.peak / peak
    return buf, sr


def _amp_envelope(tone, t: np.ndarray, duration_ns: int) -> np.ndarray:
    """Flat top, or the pulse-shaping spline stretched onto audio time."""
    if not tone.amp_curve:
        return np.full(len(t), tone.amp)
    span = t[-1] if len(t) else 0.0
    ts = np.array([c[0] for c in tone.amp_curve]) / max(duration_ns, 1) * (span or 1.0)
    vs = np.array([c[1] for c in tone.amp_curve])
    return np.interp(t, ts, vs, left=vs[0], right=vs[-1])


def _envelope(m: int, sr: int, ramp_ms: float) -> np.ndarray:
    env = np.ones(m)
    r = min(int(sr * ramp_ms / 1000.0), m // 2)
    if r > 1:
        ramp = 0.5 * (1 - np.cos(np.linspace(0, np.pi, r)))
        env[:r] = ramp
        env[-r:] = ramp[::-1]
    return env


def _add(buf: np.ndarray, i0: int, seg: np.ndarray, pan: float, stereo: bool) -> None:
    i0 = max(i0, 0)
    m = min(len(seg), len(buf) - i0)
    if m <= 0:
        return
    if stereo:
        theta = (np.clip(pan, -1, 1) + 1) * np.pi / 4
        buf[i0:i0 + m, 0] += seg[:m] * np.cos(theta)
        buf[i0:i0 + m, 1] += seg[:m] * np.sin(theta)
    else:
        buf[i0:i0 + m, 0] += seg[:m]


def write_wav(path: str, audio: np.ndarray, sr: int) -> str:
    data = np.clip(audio, -1.0, 1.0)
    pcm = (data * 32767).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(pcm.shape[1])
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(pcm.tobytes())
    return path
