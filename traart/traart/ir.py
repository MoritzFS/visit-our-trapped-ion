"""The intermediate representation: a *score* of pulses.

This is deliberately dumb, flat and JSON-serialisable.  Everything upstream
(sequence JSON, HDF5, hardware description) is resolved here; everything
downstream (Python renderer today, a Rust audio engine with a speed dial
tomorrow) consumes only this.  Times are integer nanoseconds of *experiment*
time -- no audio decisions are baked in.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from typing import Any, Literal

SCHEMA_VERSION = "traart-score/1"


@dataclass
class Tone:
    """One DDS tone inside a pulse."""

    dds_hz: float          # frequency the DDS emits
    amp: float             # 0..1 fraction of full scale (steady-state level)
    phase: float = 0.0     # radians
    detuning_hz: float | None = None   # optical detuning, None for RF-only lines
    # Shaped pulses (rising/falling edges) carry a piecewise-linear amplitude
    # curve as [[t_ns_from_pulse_start, fraction], ...]; None means a flat top.
    amp_curve: list[list[float]] | None = None


@dataclass
class Pulse:
    """A stretch of experiment time during which one channel holds a state."""

    channel: str           # beam key, e.g. "397_pi"
    t0_ns: int
    t1_ns: int
    tones: list[Tone] = field(default_factory=list)
    path: str = ""         # sequence path, e.g. "main/Cooling/Doppler cooling"

    @property
    def duration_ns(self) -> int:
        return self.t1_ns - self.t0_ns

    @property
    def is_on(self) -> bool:
        return any(t.amp > 0 for t in self.tones)

    @property
    def peak_amp(self) -> float:
        return max((t.amp for t in self.tones), default=0.0)


@dataclass
class Marker:
    """A zero-length event worth hearing or seeing: triggers, PMT gates."""

    t_ns: int
    kind: str              # "pmt_gate_open" | "pmt_gate_close" | "pmt_read" | "ttl" ...
    label: str = ""
    detail: dict[str, Any] = field(default_factory=dict)


@dataclass
class ChannelInfo:
    key: str
    label: str
    wavelength_nm: float | None
    role: str
    quench_channel: int
    central_frequency_mhz: float
    aom_type: str
    audible: bool = True


@dataclass
class Score:
    duration_ns: int = 0
    channels: dict[str, ChannelInfo] = field(default_factory=dict)
    pulses: list[Pulse] = field(default_factory=list)
    markers: list[Marker] = field(default_factory=list)
    meta: dict[str, Any] = field(default_factory=dict)

    # -- convenience --------------------------------------------------
    def on_pulses(self, channel: str | None = None) -> list[Pulse]:
        return [
            p for p in self.pulses
            if p.is_on and p.duration_ns > 0 and (channel is None or p.channel == channel)
        ]

    def channel_keys(self, only_active: bool = True,
                     only_audible: bool = True) -> list[str]:
        keys = sorted({p.channel for p in (self.on_pulses() if only_active else self.pulses)})
        if only_audible:
            keys = [k for k in keys
                    if getattr(self.channels.get(k), "audible", True)]
        return keys

    def repeat(self, n: int, gap_ns: int = 0) -> "Score":
        """Concatenate ``n`` copies -- one experimental shot after another."""
        if n <= 1:
            return self
        period = self.duration_ns + gap_ns
        out = Score(duration_ns=period * n - gap_ns, channels=dict(self.channels),
                    meta={**self.meta, "repeats": n, "gap_ns": gap_ns})
        for i in range(n):
            off = i * period
            for p in self.pulses:
                out.pulses.append(Pulse(p.channel, p.t0_ns + off, p.t1_ns + off,
                                        list(p.tones), p.path))
            for m in self.markers:
                out.markers.append(Marker(m.t_ns + off, m.kind, m.label, m.detail))
        return out

    @staticmethod
    def concat(scores: list["Score"], gap_ns: int = 0) -> "Score":
        """Lay scores end to end -- e.g. the successive points of a scan."""
        out = Score()
        cursor = 0
        for s in scores:
            out.channels.update(s.channels)
            for p in s.pulses:
                out.pulses.append(Pulse(p.channel, p.t0_ns + cursor, p.t1_ns + cursor,
                                        list(p.tones), p.path))
            for m in s.markers:
                out.markers.append(Marker(m.t_ns + cursor, m.kind, m.label, m.detail))
            cursor += s.duration_ns + gap_ns
        out.duration_ns = max(cursor - gap_ns, 0)
        out.meta = {"concat_of": len(scores)}
        return out

    # -- serialisation ------------------------------------------------
    def to_dict(self) -> dict[str, Any]:
        return {
            "schema": SCHEMA_VERSION,
            "duration_ns": self.duration_ns,
            "meta": self.meta,
            "channels": {k: asdict(v) for k, v in self.channels.items()},
            "pulses": [asdict(p) for p in self.pulses],
            "markers": [asdict(m) for m in self.markers],
        }

    def save(self, path: str) -> None:
        with open(path, "w") as fh:
            json.dump(self.to_dict(), fh, indent=1)
