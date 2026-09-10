"""Compose AOMs that sit in series in one beam path.

The 729 reaches the ion through a double pass *and* a single pass (and, for the
addressed beam, an AOD). The light is only there when the whole chain is on,
and its detuning is the sum of the shifts along it -- which is why the SDF's
two-tone structure lives on the single pass while the double pass sets the
coarse detuning. Sonifying the channels separately would either lose that or
turn one beam into three droning voices, so we fold each chain into a single
composite channel.
"""

from __future__ import annotations

from dataclasses import dataclass

from .hardware import BEAMS_BY_KEY
from .ir import ChannelInfo, Pulse, Score, Tone


@dataclass(frozen=True)
class BeamPath:
    key: str
    label: str
    members: tuple[str, ...]      # channels whose shifts add up
    steering: tuple[str, ...] = ()  # on/off matters, frequency does not
    role: str = "qubit"


DEFAULT_PATHS: tuple[BeamPath, ...] = (
    BeamPath("729", "729 addressed (DP+SP)", ("729_dp", "729_sp"),
             ("729_aod_p", "729_aod_m")),
    BeamPath("729_global", "729 diagonal (DP+SP diag)", ("729_dp", "729_sp_diag")),
)


def _state_at(pulses: list[Pulse], t: int) -> Pulse | None:
    for p in pulses:
        if p.t0_ns <= t < p.t1_ns:
            return p
    return None


def combine_paths(score: Score, paths=DEFAULT_PATHS, keep_members: bool = True) -> Score:
    """Add one composite channel per beam path present in the score."""
    for path in paths:
        by_member = {m: [p for p in score.pulses if p.channel == m] for m in path.members}
        if not all(by_member.values()):
            continue

        edges = sorted({e for m in path.members for p in by_member[m]
                        for e in (p.t0_ns, p.t1_ns)})
        combined: list[Pulse] = []
        for t0, t1 in zip(edges, edges[1:]):
            states = [_state_at(by_member[m], t0) for m in path.members]
            if any(s is None or not s.is_on for s in states):
                continue
            tones = [Tone(dds_hz=0.0, amp=1.0, detuning_hz=0.0)]
            for s in states:
                nxt = []
                for acc in tones:
                    for tone in s.tones:
                        if tone.amp <= 0:
                            continue
                        nxt.append(Tone(
                            dds_hz=tone.dds_hz,
                            amp=acc.amp * tone.amp,
                            phase=tone.phase,
                            detuning_hz=(acc.detuning_hz or 0.0) + (tone.detuning_hz or 0.0),
                            amp_curve=tone.amp_curve or acc.amp_curve))
                tones = nxt
            if not tones:
                continue
            label = next((s.path for s in states if s.path), "")
            if combined and combined[-1].t1_ns == t0 and _same(combined[-1].tones, tones):
                combined[-1].t1_ns = t1
            else:
                combined.append(Pulse(path.key, t0, t1, tones, label))

        if not combined:
            continue
        first = BEAMS_BY_KEY[path.members[0]]
        score.channels[path.key] = ChannelInfo(
            path.key, path.label, first.wavelength_nm, path.role,
            first.quench_channel, first.central_frequency_mhz, "composite", True)
        score.pulses.extend(combined)
        for m in (*path.members, *path.steering):
            if m in score.channels:
                score.channels[m].audible = False
                if not keep_members:
                    score.pulses = [p for p in score.pulses if p.channel != m]
    score.pulses.sort(key=lambda p: (p.t0_ns, p.channel))
    return score


def _same(a: list[Tone], b: list[Tone]) -> bool:
    if len(a) != len(b):
        return False
    ka = sorted((round(t.detuning_hz or 0, 3), round(t.amp, 6)) for t in a)
    kb = sorted((round(t.detuning_hz or 0, 3), round(t.amp, 6)) for t in b)
    return ka == kb
