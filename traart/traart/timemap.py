"""Experiment time (ns) -> audio time (s).

Kept separate from everything else because this is the knob you want on a
dial: the same score can be heard in real time, slowed by 1000, or warped so
that microsecond gates and millisecond cooling blocks are both audible.

Every map is monotone increasing, so pulse order and overlap are preserved.
A Rust engine only needs to re-implement ``__call__`` (and, for the
perceptual map, the same piecewise-linear breakpoint table, which
``breakpoints()`` exports).
"""

from __future__ import annotations

import bisect
from dataclasses import dataclass


class TimeMap:
    name = "base"

    def build(self, boundaries_ns: list[int]) -> None:
        """Optional hook: see the score's event grid before mapping."""

    def __call__(self, t_ns: float) -> float:
        raise NotImplementedError

    def breakpoints(self) -> tuple[list[float], list[float]] | None:
        """(experiment_ns, audio_s) knots, if this map is piecewise linear."""
        return None


@dataclass
class LinearTimeMap(TimeMap):
    """audio_seconds = experiment_seconds * stretch.

    stretch = 1     -> real time (a 1.4 ms shot is a 1.4 ms click)
    stretch = 1000  -> 1 ms of experiment becomes 1 s of audio
    """

    stretch: float = 1000.0
    name: str = "linear"

    def __call__(self, t_ns: float) -> float:
        return t_ns * 1e-9 * self.stretch


@dataclass
class FitDurationTimeMap(TimeMap):
    """Squeeze the whole score into a fixed number of seconds."""

    seconds: float = 4.0
    name: str = "fit"
    _total_ns: float = 1.0

    def build(self, boundaries_ns: list[int]) -> None:
        self._total_ns = max(boundaries_ns) if boundaries_ns else 1.0

    def __call__(self, t_ns: float) -> float:
        return self.seconds * t_ns / self._total_ns


@dataclass
class PerceptualTimeMap(TimeMap):
    """Power-law warp on the gaps between events.

    Each interval of real length ``d`` is given an audio length

        d_ref * (d / d_ref) ** gamma * stretch

    with ``gamma < 1`` compressing long stretches and expanding short ones, so
    a 200 ns gate and a 1 ms cooling block end up within earshot of each other.
    ``gamma = 1`` reduces exactly to :class:`LinearTimeMap`.
    """

    gamma: float = 0.5
    d_ref_ns: float = 100_000.0     # 100 us is "one unit" of felt duration
    stretch: float = 1000.0
    name: str = "perceptual"

    def __post_init__(self) -> None:
        self._xs: list[float] = [0.0]
        self._ys: list[float] = [0.0]

    def build(self, boundaries_ns: list[int]) -> None:
        xs = sorted(set(float(b) for b in boundaries_ns) | {0.0})
        ys = [0.0]
        for a, b in zip(xs, xs[1:]):
            d = b - a
            warped = self.d_ref_ns * (d / self.d_ref_ns) ** self.gamma if d > 0 else 0.0
            ys.append(ys[-1] + warped)
        self._xs, self._ys = xs, ys

    def __call__(self, t_ns: float) -> float:
        xs, ys = self._xs, self._ys
        if t_ns <= xs[0]:
            return ys[0] * 1e-9 * self.stretch
        if t_ns >= xs[-1]:
            return ys[-1] * 1e-9 * self.stretch
        i = bisect.bisect_right(xs, t_ns) - 1
        frac = (t_ns - xs[i]) / (xs[i + 1] - xs[i])
        y = ys[i] + frac * (ys[i + 1] - ys[i])
        return y * 1e-9 * self.stretch

    def breakpoints(self):
        return self._xs, [y * 1e-9 * self.stretch for y in self._ys]


def make_timemap(kind: str = "linear", **kw) -> TimeMap:
    kinds = {"linear": LinearTimeMap, "fit": FitDurationTimeMap,
             "perceptual": PerceptualTimeMap}
    if kind not in kinds:
        raise ValueError(f"unknown time map {kind!r}; have {sorted(kinds)}")
    return kinds[kind](**kw)
