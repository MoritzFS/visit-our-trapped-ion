"""Eyes on the ears: pulse timeline next to the spectrogram of the render."""

from __future__ import annotations

import wave

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt   # noqa: E402
import numpy as np                # noqa: E402

from .ir import Score             # noqa: E402
from .voices import voices_for    # noqa: E402

ROLE_COLOR = {"cooling": "#3b82f6", "repump": "#f59e0b",
              "qubit": "#ef4444", "motion": "#10b981", "": "#64748b"}


def read_wav(path: str):
    with wave.open(path, "rb") as w:
        sr = w.getframerate()
        a = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
        a = a.reshape(-1, w.getnchannels()).astype(float) / 32768.0
    return a.mean(axis=1), sr


def plot_run(score: Score, wav_path: str, out_png: str,
             seconds: float | None = None, channels: list[str] | None = None,
             title: str = "") -> str:
    x, sr = read_wav(wav_path)
    span = seconds or len(x) / sr
    keys = channels or score.channel_keys()
    voices = voices_for(keys)

    # experiment time -> audio time is linear per rendered file; recover it
    scale = (len(x) / sr) / (score.duration_ns * 1e-9) if score.duration_ns else 1.0

    fig, (ax0, ax1) = plt.subplots(
        2, 1, figsize=(13, 3 + 0.42 * len(keys)), sharex=True,
        gridspec_kw={"height_ratios": [max(len(keys), 3), 6]})

    for i, k in enumerate(keys):
        info = score.channels.get(k)
        color = ROLE_COLOR.get(getattr(info, "role", ""), "#64748b")
        for p in score.on_pulses(k):
            t0 = p.t0_ns * 1e-9 * scale
            t1 = p.t1_ns * 1e-9 * scale
            if t0 > span:
                continue
            ax0.barh(i, t1 - t0, left=t0, height=0.6,
                     color=color, alpha=0.35 + 0.55 * min(p.peak_amp, 1.0))
    for m in score.markers:
        if m.kind == "pmt_gate_open":
            t = m.t_ns * 1e-9 * scale
            if t <= span:
                ax0.axvline(t, color="#111827", lw=0.8, ls=":", alpha=0.7)
    ax0.set_yticks(range(len(keys)))
    ax0.set_yticklabels([f"{k}  ({voices[k].carrier_hz:.0f} Hz)" for k in keys], fontsize=8)
    ax0.set_ylim(-0.6, len(keys) - 0.4)
    ax0.set_title(title or score.meta.get("experiment", ""), fontsize=11)
    ax0.grid(axis="x", alpha=0.25)

    n = int(span * sr)
    ax1.specgram(x[:n], NFFT=2048, Fs=sr, noverlap=1536, cmap="magma",
                 vmin=-110, vmax=-25)
    ax1.set_ylim(0, 2000)
    ax1.set_ylabel("audio frequency [Hz]")
    ax1.set_xlabel(f"audio time [s]   (experiment time x {scale:.0f})")
    ax1.set_xlim(0, span)
    fig.tight_layout()
    fig.savefig(out_png, dpi=140)
    plt.close(fig)
    return out_png
