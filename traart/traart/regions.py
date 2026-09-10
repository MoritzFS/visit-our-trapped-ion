"""Edit a score by repeating or stretching one captured block.

This is the reference for what the player does live: given a region [a, b)
that the parser reported for a named block, either play it k times (a loop
whose round count is a free parameter) or give it a new length (a probe pulse
whose duration is a free parameter).  Everything before the block is left
alone, everything after it slides, anything that spans it is extended, and
what lies inside is copied or rescaled.
"""

from __future__ import annotations

from .ir import Marker, Pulse, Score


def _edit(score: Score, a: int, b: int, inside, delta, per_channel=None) -> Score:
    """Classify every pulse by where its *start* falls.

    A pulse is a state written at its start and held until the next write on
    that channel, so the start is what the loop repeats.  Channels reach the
    block boundary a few microseconds apart (setup delays), which is why the
    span of a pulse is not a safe criterion: the pre-block hold on a channel
    legitimately ends a little way inside the block, and the block's last state
    on a channel legitimately runs a little way past it.
    """
    end = score.duration_ns
    chans = {p.channel for p in score.pulses}
    new_end = end + max([delta(None)] + [delta(c) for c in chans])
    out = Score(duration_ns=new_end, channels=dict(score.channels), meta=dict(score.meta))
    # first write on each channel inside the block: where the next pass will
    # cut off whatever state the previous pass left running
    first_write: dict[str, int] = {}
    for p in score.pulses:
        if a <= p.t0_ns < b and p.channel not in first_write:
            first_write[p.channel] = p.t0_ns
    for p in score.pulses:
        d = delta(p.channel)
        t1 = new_end if p.t1_ns >= end else None   # trailing holds run to the new end
        if p.t0_ns < a:
            if p.t1_ns <= b:
                out.pulses.append(p)                       # before, or the pre-block hold
            else:
                out.pulses.append(Pulse(p.channel, p.t0_ns, t1 or p.t1_ns + d,
                                        p.tones, p.path))  # spans the whole block
        elif p.t0_ns >= b:
            out.pulses.append(Pulse(p.channel, p.t0_ns + d, t1 or p.t1_ns + d,
                                    p.tones, p.path))
        else:
            copies = inside(p.t0_ns, p.t1_ns, p.channel)
            fw = first_write.get(p.channel)
            tail = p.t1_ns > b     # the block's last state on this channel
            for j, (t0, t1c) in enumerate(copies):
                if tail and j < len(copies) - 1 and fw is not None:
                    # Not the last pass: this state runs until the next pass
                    # rewrites the channel, not until whatever came after the
                    # block in the one-pass template.
                    t1c = copies[j + 1][0] - (p.t0_ns - fw)
                elif t1 is not None:
                    t1c = t1
                out.pulses.append(Pulse(p.channel, t0, max(t1c, t0), p.tones, p.path))
    for m in score.markers:
        if m.t_ns < a:
            out.markers.append(m)
        elif m.t_ns >= b:
            out.markers.append(Marker(m.t_ns + delta(None), m.kind, m.label, m.detail))
        else:
            for t0, _ in inside(m.t_ns, m.t_ns, None):
                out.markers.append(Marker(t0, m.kind, m.label, m.detail))
    out.pulses.sort(key=lambda p: (p.t0_ns, p.channel))
    out.markers.sort(key=lambda m: m.t_ns)
    return out


def repeat_region(score: Score, a: int, b: int, k: int, period: int | None = None,
                  channel_periods: dict[str, int] | None = None) -> Score:
    """Play the block ``k`` times instead of once.

    ``[a, b)`` classifies pulses by their start; ``period`` is how far one pass
    advances a channel, which is what the copies are spaced by.  A channel in
    ``channel_periods`` uses its own advance instead.
    """
    period = (b - a) if period is None else period
    cp = channel_periods or {}
    k = max(int(k), 1)

    def per(ch):
        return cp.get(ch, period)

    def inside(t0, t1, ch):
        P = per(ch)
        return [(t0 + j * P, t1 + j * P) for j in range(k)]

    return _edit(score, a, b, inside, lambda ch: (k - 1) * per(ch))


def stretch_region(score: Score, a: int, b: int, new_len: int) -> Score:
    """Give the block [a, b) a new length, rescaling what lies inside it."""
    period = max(b - a, 1)
    s = new_len / period

    def inside(t0, t1, ch):
        n0 = a + int(round((t0 - a) * s))
        # a state that runs past the block keeps its overhang; one that ends
        # inside is rescaled with it
        n1 = t1 + (new_len - period) if t1 > b else a + int(round((t1 - a) * s))
        return [(n0, max(n1, n0))]

    return _edit(score, a, b, inside, lambda ch: new_len - period)
