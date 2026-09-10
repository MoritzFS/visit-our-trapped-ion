"""Flatten an ionpulse sequence JSON into a :class:`~traart.ir.Score`.

Timing model (verified against ``sequence_Cool and Detect.json``)
----------------------------------------------------------------
Every hardware channel owns an independent cursor, all starting at 0.  An
event carries a ``time`` index into the global ``time`` table; the value is a
*delay in nanoseconds* that elapses on each channel in the event's ``ch_mask``
*before* the event takes effect.  So::

    cursor[ch] += time ; apply state at the new cursor

A channel therefore holds the state written by one event until the next event
on that same channel.  ``Wait`` events only advance cursors -- which is how
``sync0``/``add_timeN`` re-align channels that ran for different durations.

Cross-check: in Cool and Detect the DIO channel spends 3 x 2 us on its opening
edges while the RF channels take a single 6 us ``add_time``; both then sit at
6 us, and after the 1.012 ms cooling block both sit at 1.018 ms again.
"""

from __future__ import annotations

import json
from typing import Any

from .hardware import (BEAMS, SPLINE_AMP_SCALE, SPLINE_TIME_UNIT_NS,
                       dds_amp_to_fraction, dds_word_to_hz)
from .ir import ChannelInfo, Marker, Pulse, Score, Tone


class SequenceParser:
    def __init__(self, doc: dict[str, Any], max_loop_iterations: int | None = None,
                 idle_ns: int = 0, capture: tuple[str, ...] = (),
                 loop_override: dict[str, int] | None = None):
        self.doc = doc
        self.events: list[dict] = doc["event"]
        self.sequences: list[dict] = doc["sequence"]
        self.n_events = len(self.events)
        self.time_tbl = [t["value"] for t in doc.get("time", [])]
        self.freq_tbl = [t["value"] for t in doc.get("freq", [])]
        self.phase_tbl = [t["value"] for t in doc.get("phase", [])]
        self.amp_tbl = [t["value"] for t in doc.get("amp", [])]
        self.amp_splines = doc.get("amp_spline_parameter", [])
        self.freq_splines = doc.get("freq_spline_parameter", [])
        self.max_loop_iterations = max_loop_iterations
        # Dead time between shots.  The last block of `main` typically switches
        # Doppler cooling back on without ever switching it off, so the ion sits
        # in that state until the next shot -- give it somewhere to live.
        self.idle_ns = idle_ns
        # Named blocks whose time span should be reported, so a player can
        # repeat or stretch them live (a QEC loop, a probe pulse).  Loop
        # iteration counts can be pinned so the template holds one round.
        self.capture = tuple(capture)
        self.loop_override = dict(loop_override or {})
        self.regions: dict[str, dict[str, Any]] = {}

        # global channel index -> quench channel number (via the header table)
        self.idx_to_quench: dict[int, int] = {}
        self.idx_to_hw: dict[int, tuple[str, int]] = {}
        for i, hw in enumerate(doc["header"].get("channel_idx_to_hw", [])):
            self.idx_to_hw[i] = (hw.get("hardware", "?"), hw.get("channel", -1))
            if hw.get("hardware") == "QuenchHardware":
                self.idx_to_quench[i] = hw["channel"]

        self.cursor: dict[int, int] = {}
        self.loop_stack: list[int] = []   # current iteration of each open loop
        self.pending: dict[str, Pulse] = {}   # channel key -> open pulse
        self.score = Score()
        self.unhandled: set[str] = set()

    # -- helpers -------------------------------------------------------
    def _beam(self, idx: int):
        q = self.idx_to_quench.get(idx)
        return BEAMS.get(q) if q is not None else None

    def _delay(self, ev: dict) -> int:
        """Delay in ns before this event fires.

        A ``time`` table entry is usually a scalar, but inside a
        ``LoopSequence`` it may be a list with one value per iteration -- that
        is how a ramped sideband-cooling pulse lengthens as the loop runs.
        Index it with the innermost open loop's counter.
        """
        ti = ev.get("time")
        if ti is None:
            return 0
        val = self.time_tbl[ti]
        if isinstance(val, list):
            k = self.loop_stack[-1] if self.loop_stack else 0
            val = val[min(k, len(val) - 1)] if val else 0.0
        return int(round(val))

    def _amp(self, ai: int) -> tuple[float, list[list[float]] | None]:
        """Resolve an amplitude index to (steady level, optional shape curve).

        Indices past the end of the flat ``amp`` table address the amplitude
        splines: the two tables share one index space, which is why the spline
        entries are named with their global index.
        """
        if ai < len(self.amp_tbl):
            return dds_amp_to_fraction(self.amp_tbl[ai]), None
        sp = self.amp_splines[ai - len(self.amp_tbl)]
        lengths = sp["segment_length"][0]
        values = sp["x0"][0]
        curve, t = [], 0.0
        for n, v in zip(lengths, values):
            curve.append([t, v / SPLINE_AMP_SCALE / 16383.0])
            t += n * SPLINE_TIME_UNIT_NS
        return curve[-1][1], curve

    def _resolve(self, ref: int) -> tuple[str, dict]:
        if ref < self.n_events:
            return "event", self.events[ref]
        return "sequence", self.sequences[ref - self.n_events]

    # -- pulse bookkeeping ---------------------------------------------
    def _close(self, key: str, t: int) -> None:
        p = self.pending.pop(key, None)
        if p is not None:
            p.t1_ns = t
            if p.duration_ns > 0:
                self.score.pulses.append(p)

    def _open(self, key: str, t: int, tones: list[Tone], path: str) -> None:
        self.pending[key] = Pulse(channel=key, t0_ns=t, t1_ns=t, tones=tones, path=path)

    # -- event handling -------------------------------------------------
    def _apply(self, ev: dict, path: str) -> None:
        etype = ev["type"]
        delay = self._delay(ev)
        chans = ev.get("ch_mask") or []

        # advance every channel this event touches, then act at the new time
        for idx in chans:
            self.cursor[idx] = self.cursor.get(idx, 0) + delay

        if etype == "Wait":
            return

        if etype == "MultiToneRFEvent":
            for idx in chans:
                beam = self._beam(idx)
                if beam is None:
                    self.unhandled.add(f"RF on non-beam channel idx {idx}")
                    continue
                t = self.cursor[idx]
                tones = []
                for fi, pi, ai in zip(ev["freq"], ev["phase"], ev["amp"]):
                    if fi >= len(self.freq_tbl):
                        self.unhandled.add("freq_spline")
                        continue
                    dds = dds_word_to_hz(self.freq_tbl[fi])
                    amp, curve = self._amp(ai)
                    ph = self.phase_tbl[pi] if pi < len(self.phase_tbl) else 0.0
                    tones.append(Tone(dds_hz=dds, amp=amp, phase=float(ph),
                                      detuning_hz=beam.detuning_hz(dds),
                                      amp_curve=curve))
                self._close(beam.key, t)
                self.score.channels.setdefault(beam.key, ChannelInfo(
                    beam.key, beam.label, beam.wavelength_nm, beam.role,
                    beam.quench_channel, beam.central_frequency_mhz, beam.aom_type,
                    beam.audible))
                self._open(beam.key, t, tones, path)
            return

        if etype == "DIOEvent":
            for idx in chans:
                t = self.cursor[idx]
                if ev.get("output_mask"):
                    self.score.markers.append(Marker(
                        t, "ttl", ev.get("name", ""),
                        {"state": ev["output_state"], "mask": ev["output_mask"],
                         "path": path}))
                if ev.get("input_gate_mask"):
                    kind = "pmt_gate_open" if ev.get("input_gate_state") else "pmt_gate_close"
                    self.score.markers.append(Marker(t, kind, ev.get("name", ""),
                                                     {"path": path}))
            return

        if etype in ("PopPMTFIFO", "Discriminator"):
            t = max(self.cursor.values(), default=0)
            self.score.markers.append(Marker(
                t, "readout", ev.get("name", ""),
                {"event": etype, "path": path}))
            return

        self.unhandled.add(etype)

    # -- traversal --------------------------------------------------------
    def _walk(self, node: dict, path: str, depth: int = 0) -> None:
        if depth > 64:
            raise RecursionError("sequence nesting too deep")
        name = (node.get("name") or "").split("] ", 1)[-1].strip() or node.get("type", "")
        here = f"{path}/{name}" if path else name
        iterations = 1
        if node["type"] == "LoopSequence":
            iterations = int(node.get("iterations", 1))
            if self.max_loop_iterations is not None:
                iterations = min(iterations, self.max_loop_iterations)
            if name in self.loop_override:
                iterations = int(self.loop_override[name])
        capture = name in self.capture
        if capture:
            chans = node.get("ch_mask") or []
            t_in = {c: self.cursor.get(c, 0) for c in chans}
        is_loop = node["type"] == "LoopSequence"
        for it in range(iterations):
            if is_loop:
                self.loop_stack.append(it)
            for ref, _label in node["sequences"]:
                kind, child = self._resolve(int(ref))
                if kind == "event":
                    self._apply(child, here)
                else:
                    self._walk(child, here, depth + 1)
            if is_loop:
                self.loop_stack.pop()
        if capture:
            t_out = {c: self.cursor.get(c, 0) for c in chans}
            advances = [t_out[c] - t_in[c] for c in chans]
            per_channel = {}
            for c in chans:
                beam = self._beam(c)
                key = beam.key if beam else f"idx{c}"
                per_channel[key] = (t_out[c] - t_in[c]) // max(iterations, 1)
            common = (max(set(per_channel.values()), key=list(per_channel.values()).count)
                      if per_channel else 0)
            region = {
                "t0": min(t_in.values(), default=0),
                "t1": max(t_out.values(), default=0),
                # How far a channel moves through one pass of the block.  With
                # unsynced boundaries this is *not* t1 - t0: channels enter and
                # leave a few microseconds apart, and that skew must not be
                # counted once per repetition.
                "period": common,
                # Channels that do not advance by the common period (a shaped
                # pulse on a 4 ns grid drifts a few ns per round); the copies
                # on those channels are spaced by their own advance.
                "channelPeriods": {k: v for k, v in per_channel.items() if v != common},
                "synced": len(set(t_in.values())) <= 1 and len(set(t_out.values())) <= 1,
                "kind": "loop" if node["type"] == "LoopSequence" else "block",
                "iterations": iterations,
                "occurrences": self.regions.get(name, {}).get("occurrences", 0) + 1,
            }
            if name not in self.regions:
                self.regions[name] = region
            else:
                self.regions[name]["occurrences"] += 1

    def parse(self, root: int | None = None) -> Score:
        root_seq = self.sequences[root] if root is not None else self._find_main()
        self._walk(root_seq, "")
        end = max(self.cursor.values(), default=0)
        for key in list(self.pending):
            self._close(key, end + self.idle_ns)
        self.score.duration_ns = end + self.idle_ns
        self.score.pulses.sort(key=lambda p: (p.t0_ns, p.channel))
        self.score.markers.sort(key=lambda m: m.t_ns)
        self.score.meta.update({
            "shots_header": self.doc["header"].get("shots"),
            "n_events": self.n_events,
            "n_sequences": len(self.sequences),
            "unhandled": sorted(self.unhandled),
            "regions": self.regions,
        })
        return self.score

    def _find_main(self) -> dict:
        for s in reversed(self.sequences):
            if s.get("name", "").split("] ", 1)[-1].strip().lower() == "main":
                return s
        return self.sequences[-1]


def parse_sequence(doc: dict | str, combine: bool = True, **kw) -> Score:
    """Parse one sequence JSON.

    ``combine`` folds series AOMs (the 729 double pass + single pass + AOD)
    into one composite channel per beam path -- see :mod:`traart.optics`.
    """
    if isinstance(doc, (str, bytes)):
        doc = json.loads(doc)
    score = SequenceParser(doc, **kw).parse()
    if combine:
        from .optics import combine_paths
        score = combine_paths(score)
    return score


def load_sequence_file(path: str, **kw) -> Score:
    with open(path) as fh:
        return parse_sequence(json.load(fh), **kw)
