"""Export scores in the compact form the browser player eats.

One JSON holds every demo. Times are integer nanoseconds, detunings integer
hertz, amplitudes 3 decimals -- the player rebuilds the sound from these, so
slowing time down never touches pitch.
"""

from __future__ import annotations

import json
import re
import statistics
from dataclasses import dataclass, field

from .h5io import load_run
from .ir import Score
from .parser import parse_sequence


@dataclass
class DemoSpec:
    id: str
    title: str
    subtitle: str
    path: str
    point_stride: int = 1
    max_points: int | None = None
    shots: int = 1              # shots per point the player loops through
    note: str = ""
    tags: list[str] = field(default_factory=list)
    # Per-run voice overrides, keyed by beam: carrier, refMHz, sens, gain,
    # resp, tail, pan. A run whose interesting structure is 15 kHz wide and
    # 0.8 % deep needs a mapping the global defaults cannot supply.
    voice: dict[str, dict] = field(default_factory=dict)
    # A LoopSequence whose round count becomes a live parameter.  The run is
    # exported as its one-round template; the player repeats the block.
    loop: str | None = None
    loop_max: int = 400
    # A block whose length becomes a live parameter (a probe pulse).
    stretch: str | None = None
    stretch_range_ns: tuple[int, int] = (100_000, 8_000_000)


def _scan_axis(run) -> dict:
    for col, vals in run.scan_parameters.items():
        if col == "timestamp":
            continue
        m = (re.search(r"display_name='([^']*)'", col)
             or re.search(r"(?<!_)name='([^']*)'", col))
        name = m.group(1) if m else _humanise(col)
        return {"name": name, "values": vals}
    return {}


def _humanise(col: str) -> str:
    """Scan columns without a display name still carry their type tags."""
    bits = []
    if "FrequencyTypes.BARE" in col:
        bits.append("f729 bare")
    elif "ParameterTypes.FREQUENCY" in col:
        bits.append("frequency")
    elif "ParameterTypes.TIME" in col:
        bits.append("time")
    m = re.search(r"transition='CaTransitions\.([A-Z0-9_]+)'", col)
    if m:
        bits.append(m.group(1).replace("_TO_", "->").lower())
    m = re.search(r"pulse_type='PulseTypes\.([A-Z_]+)'", col)
    if m:
        bits.append(m.group(1).lower())
    return " ".join(bits) or "scan"


def _pack(score: Score, ch_index: dict[str, int], blocks: list[str],
          block_index: dict[str, int]) -> dict:
    pulses = []
    for p in score.pulses:
        if p.channel not in ch_index or not p.is_on or p.duration_ns <= 0:
            continue
        tones = [[int(round(t.detuning_hz or 0.0)), round(t.amp, 3)]
                 for t in p.tones if t.amp > 0]
        if not tones:
            continue
        if p.path not in block_index:
            block_index[p.path] = len(blocks)
            blocks.append(p.path)
        pulses.append([ch_index[p.channel], p.t0_ns, p.t1_ns, tones,
                       block_index[p.path]])
    gates = [m.t_ns for m in score.markers if m.kind == "pmt_gate_open"]
    return {"durNs": score.duration_ns, "pulses": pulses, "gates": gates}


def export_demo(spec: DemoSpec) -> dict:
    run = load_run(spec.path, max_points=spec.max_points)
    capture = tuple(n for n in (spec.loop, spec.stretch) if n)
    override = {spec.loop: 1} if spec.loop else {}
    if spec.loop:
        # one-round template; the player makes the scan
        idx = [0]
    else:
        idx = list(range(0, len(run.sequences), spec.point_stride))
    scores = [parse_sequence(run.sequences[i][1], capture=capture,
                             loop_override=override) for i in idx]

    keys: list[str] = []
    for s in scores:
        for k in s.channel_keys():
            if k not in keys:
                keys.append(k)
    ch_index = {k: i for i, k in enumerate(keys)}

    dets: dict[str, list[float]] = {k: [] for k in keys}
    for s in scores:
        for p in s.on_pulses():
            if p.channel in dets:
                dets[p.channel] += [(t.detuning_hz or 0.0) / 1e6
                                    for t in p.tones if t.amp > 0]

    info = {}
    for s in scores:
        info.update({k: v for k, v in s.channels.items() if k in ch_index})

    blocks: list[str] = []
    block_index: dict[str, int] = {}
    axis = _scan_axis(run)
    regions = scores[0].meta.get("regions", {})
    for sc in scores[1:]:
        for name, reg in sc.meta.get("regions", {}).items():
            base = regions.get(name)
            if base and (base["t0"], base["t1"]) != (reg["t0"], reg["t1"]):
                raise ValueError(f"{spec.id}: block {name!r} moves between scan points; "
                                 "cannot expose it as a live parameter")
    labels = []
    for j, i in enumerate(idx):
        if axis.get("values") and i < len(axis["values"]):
            v = axis["values"][i]
            labels.append(f"{axis['name']} = {v:g}" if isinstance(v, (int, float))
                          else str(v))
        else:
            labels.append(f"point {i + 1}")

    return {
        "id": spec.id,
        "title": spec.title,
        "subtitle": spec.subtitle,
        "note": spec.note,
        "tags": spec.tags,
        "experiment": run.short_name,
        "job": run.job_id,
        "file": spec.path.split("/")[-1],
        "shotsMeasured": run.n_shots,
        "shots": spec.shots,
        "scan": axis.get("name", ""),
        "voice": spec.voice,
        "loop": _loop_block(spec, regions, axis, ch_index),
        "stretch": _stretch_block(spec, regions),
        "channels": [{
            "key": k,
            "label": info[k].label if k in info else k,
            "wavelength": info[k].wavelength_nm if k in info else None,
            "role": info[k].role if k in info else "",
            "refMHz": round(statistics.median(dets[k]), 4) if dets[k] else 0.0,
            "spanMHz": round((max(dets[k]) - min(dets[k])), 4) if dets[k] else 0.0,
        } for k in keys],
        "points": [dict(_pack(s, ch_index, blocks, block_index), label=lbl)
                   for s, lbl in zip(scores, labels)],
        "blocks": blocks,
    }


def _loop_block(spec, regions, axis, ch_index):
    if not spec.loop:
        return None
    reg = regions[spec.loop]
    measured = sorted({int(round(v)) for v in axis.get("values", [])
                       if isinstance(v, (int, float))})
    return {
        "name": spec.loop,
        "t0": reg["t0"], "t1": reg["t1"],
        "period": reg["period"],
        "channelPeriods": {ch_index[k]: v for k, v in reg["channelPeriods"].items()
                           if k in ch_index},
        "measured": measured,
        "max": spec.loop_max,
    }


def _stretch_block(spec, regions):
    if not spec.stretch:
        return None
    reg = regions[spec.stretch]
    return {
        "name": spec.stretch,
        "t0": reg["t0"], "t1": reg["t1"],
        "minNs": spec.stretch_range_ns[0], "maxNs": spec.stretch_range_ns[1],
    }


def export_all(specs: list[DemoSpec], out_path: str) -> dict:
    data = {"version": 1, "demos": [export_demo(s) for s in specs]}
    with open(out_path, "w") as fh:
        json.dump(data, fh, separators=(",", ":"))
    return data
