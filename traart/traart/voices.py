"""How a laser pulse becomes a sound.

Each beam gets a *voice*: a carrier pitch that identifies the colour, plus a
rule turning the pulse's optical detuning into a pitch offset, so that within
one colour you can still hear where in the transition you are.  Scanning a
probe frequency then comes out as a scale / glissando, which is the point.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Voice:
    carrier_hz: float               # pitch at zero detuning
    semitones_per_mhz: float = 0.0  # detuning -> pitch offset
    gain: float = 1.0
    pan: float = 0.0                # -1 left .. +1 right
    harmonics: tuple[tuple[float, float], ...] = ((1.0, 1.0),)
    mode: str = "carrier"           # "carrier" | "scaled"
    scale: float = 1e-3             # for mode="scaled": audio_hz = dds_hz * scale
    ref_detuning_mhz: float = 0.0   # detuning that maps to the carrier
    min_hz: float = 40.0
    max_hz: float = 12000.0

    def pitch_hz(self, dds_hz: float, detuning_hz: float | None) -> float:
        if self.mode == "scaled":
            f = dds_hz * self.scale
        else:
            det_mhz = (detuning_hz or 0.0) / 1e6
            st = self.semitones_per_mhz * (det_mhz - self.ref_detuning_mhz)
            f = self.carrier_hz * 2.0 ** (st / 12.0)
        return min(max(f, self.min_hz), self.max_hz)


# Musically distinct defaults.  729 sits at 1 kHz as requested; the repumpers
# are low and soft so they read as a bed under the cooling/qubit light.
DEFAULT_VOICES: dict[str, Voice] = {
    "397_pi":      Voice(880.00, 0.40, gain=1.00, pan=-0.25,
                         harmonics=((1, 1.0), (2, 0.12))),
    "397_sigma":   Voice(739.99, 0.40, gain=1.00, pan=+0.25,
                         harmonics=((1, 1.0), (2, 0.12))),
    "729_dp":      Voice(1000.0, 4.00, gain=0.90, pan=0.0,
                         harmonics=((1, 1.0), (3, 0.18))),
    "729_sp":      Voice(1200.0, 4.00, gain=0.80, pan=-0.4,
                         harmonics=((1, 1.0), (3, 0.15))),
    "729_sp_diag": Voice(1318.5, 4.00, gain=0.80, pan=+0.4,
                         harmonics=((1, 1.0), (3, 0.15))),
    "729_aod_p":   Voice(1500.0, 4.00, gain=0.70, pan=+0.6),
    "729_aod_m":   Voice(1568.0, 4.00, gain=0.70, pan=-0.6),
    "854":         Voice(220.00, 0.50, gain=0.55, pan=+0.5,
                         harmonics=((1, 1.0), (2, 0.3), (3, 0.1))),
    "866":         Voice(293.66, 0.50, gain=0.55, pan=-0.5,
                         harmonics=((1, 1.0), (2, 0.3), (3, 0.1))),
    "tickle_rf":   Voice(0.0, mode="scaled", scale=1e-3, gain=0.5, pan=0.0),
    "tickle_dc":   Voice(0.0, mode="scaled", scale=1e-3, gain=0.5, pan=0.0),
    "tickle_hel":  Voice(0.0, mode="scaled", scale=1e-3, gain=0.5, pan=0.0),
}


def voices_for(keys, overrides: dict[str, Voice] | None = None) -> dict[str, Voice]:
    out = {k: DEFAULT_VOICES.get(k, Voice(440.0, 1.0)) for k in keys}
    if overrides:
        out.update({k: v for k, v in overrides.items() if k in out})
    return out
