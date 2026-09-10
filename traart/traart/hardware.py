"""Static description of the GKP trap hardware, transcribed from
gkp-experiment-library/experiment_library/hardware_description/hardware.py.

The sequence JSON refers to channels by *global channel index*, which indexes
into header["channel_idx_to_hw"].  For this system that table is:

    idx 0        -> RFSoC / Readout        ch 0
    idx 1        -> RFSoC / DIOHardware    ch 0
    idx 2..17    -> RFSoC / QuenchHardware ch 0..15   (the DDS/RF channels)
    idx 18..33   -> RFSoC / QuenchADCHardware ch 0..15
    idx 34       -> FastinoRack / FastinoHardware ch 0

so quench channel q lives at global index q + 2.  We nevertheless resolve the
mapping from the header at parse time and use this table only for the physics
(which laser, which AOM geometry).
"""

from __future__ import annotations

from dataclasses import dataclass

SPEED_OF_LIGHT = 299_792_458.0


@dataclass(frozen=True)
class Beam:
    """One RF/DDS channel and the optical beam it drives."""

    key: str                  # short stable id, e.g. "397_pi"
    label: str                # human label
    quench_channel: int       # QuenchHardware channel number
    wavelength_nm: float | None   # None for pure-RF lines (tickles)
    aom_type: str             # "SP", "DP" or "RF"
    central_frequency_mhz: float
    order: int = 1
    role: str = ""            # free-form: cooling / repump / qubit / motion
    # Some channels do not put light on the ion by themselves -- the 729 single
    # passes and the AODs are gated by the double pass, which is the shutter
    # that decides whether there is any 729 at all.  They are still parsed
    # (the AOD tones say *which* ion is addressed) but they stay silent.
    audible: bool = True

    @property
    def pass_factor(self) -> int:
        """How many times the light passes the AOM (double pass doubles shift)."""
        return 2 if self.aom_type == "DP" else 1

    def detuning_hz(self, dds_hz: float) -> float:
        """Optical detuning produced by driving this AOM at ``dds_hz``.

        Relative to the AOM's central (nominal) drive frequency, so 0 means
        "parked at the nominal operating point".  Sign follows the diffraction
        order, so a red detuning comes out negative.
        """
        return self.order * self.pass_factor * (dds_hz - self.central_frequency_mhz * 1e6)

    @property
    def optical_frequency_hz(self) -> float | None:
        if self.wavelength_nm is None:
            return None
        return SPEED_OF_LIGHT / (self.wavelength_nm * 1e-9)


# --- the GKP system -------------------------------------------------------

BEAMS: dict[int, Beam] = {
    b.quench_channel: b
    for b in [
        Beam("729_dp",      "729 double pass",     0, 729.0, "DP", 160.0, -1, "qubit"),
        Beam("729_sp_diag", "729 SP diagonal",     1, 729.0, "SP", 150.0,  1, "gate", audible=False),
        Beam("854",         "854 repump",          2, 854.0, "SP", 350.0,  1, "repump"),
        Beam("397_sigma",   "397 sigma",           3, 397.0, "DP",  80.0,  1, "cooling"),
        Beam("729_sp",      "729 SP addressing",   4, 729.0, "SP", 150.0,  1, "gate", audible=False),
        Beam("729_aod_p",   "729 AOD order +1",    5, 729.0, "SP", 110.0,  1, "gate", audible=False),
        Beam("729_aod_m",   "729 AOD order -1",    6, 729.0, "SP", 110.0, -1, "gate", audible=False),
        Beam("tickle_rf",   "RF tickle",           7, None,  "RF",   0.0,  1, "motion"),
        Beam("tickle_hel",  "helical tickle",     11, None,  "RF",   0.0,  1, "motion"),
        Beam("397_pi",      "397 pi",             12, 397.0, "DP",  80.0,  1, "cooling"),
        Beam("866",         "866 repump",         13, 866.0, "SP", 300.0,  1, "repump"),
        Beam("tickle_dc",   "DC tickle",          14, None,  "RF",   0.0,  1, "motion"),
    ]
}

BEAMS_BY_KEY: dict[str, Beam] = {b.key: b for b in BEAMS.values()}

# DIO output pins (ttl_device.add_output in hardware.py)
DIO_PINS = {
    6: "Experiment Trigger",
    2: "PI",
    3: "3970th",
    9: "Fastino trigger",
    13: "TTL 14",
    14: "TTL 15",
    15: "TTL 16",
}

# DDS amplitude is a 14-bit unsigned word.
AMP_FULL_SCALE = 16383.0

# The DDS frequency word is a 32-bit fraction of a 1 GHz reference.
DDS_CLOCK_HZ = 1_000_000_000.0
DDS_FREQ_BITS = 32


# Amplitude splines are stored at 4x the resolution of the flat amplitude
# table (full scale 65532 rather than 16383), and their segment lengths are in
# 4 ns sequencer clock cycles.  Both were pinned down from shaped 729 pulses,
# where the spline plateau divides by 4 onto an exact entry of the flat table
# and the falling edge just fits the wait that follows it.
SPLINE_AMP_SCALE = 4.0
SPLINE_TIME_UNIT_NS = 4.0


def dds_word_to_hz(word: int) -> float:
    return word * DDS_CLOCK_HZ / (2 ** DDS_FREQ_BITS)


def dds_amp_to_fraction(word: int) -> float:
    return word / AMP_FULL_SCALE
