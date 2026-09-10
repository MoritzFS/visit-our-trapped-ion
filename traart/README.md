# tr(art) — hearing the ion

Turn an ICON run (`.h5` → generated ionpulse sequence JSON) into sound: one
voice per laser, pitch carrying the detuning, timing carrying the sequence.

```
sequence JSON ──parser──► Score (IR) ──render──► wav
   (.h5 or file)          ns-accurate,          voices + time map
                          serialisable
```

The **Score** is the whole point of the split: it is flat, integer-nanosecond
and JSON-serialisable, and it contains no audio decisions. Everything after it
(voice assignment, time stretching, mixing) is presentation. That is the seam a
Rust engine with a speed dial should plug into: dump the score once with
`--score out.json`, then let the engine re-render it live as the dial moves.

The desktop player lives next door in `../traart-bench` (Rust, `cargo run
--release`). It reads `web/demos.json`, which `export_demos.py` writes.
`web/template.html` is an abandoned browser version, kept only in case a
shareable web build is ever wanted.

## Quick start

```bash
python3 -m traart.cli index ../icon_experiment_data_2026-09-01   # what's in the folder
python3 -m traart.cli sonify RUN.h5 -c 397_pi --shots 12 --stretch 1000 -o out.wav
```

Useful flags: `-c/--channel` (repeatable), `--shots`, `--points`, `--stretch`
(1 = real time, 1000 = 1 ms → 1 s), `--timemap linear|fit|perceptual`,
`--markers` (click on PMT gates), `--score` (dump the IR), `--max-loops`
(cap huge `LoopSequence` blocks, e.g. QEC round scans).

## What the sequence JSON means

Reverse-engineered from `sequence_Cool and Detect.json` and cross-checked
against the runs in `icon_experiment_data_2026-09-01`.

**Channel indices.** Events address channels by *global* index into
`header.channel_idx_to_hw`. On this system that is `0 = Readout`, `1 = DIO`,
`2..17 = QuenchHardware 0..15`, so quench channel `q` is index `q + 2`. The
parser resolves this from the header rather than assuming it.

**Timing.** Every channel has its own cursor, all starting at 0. An event's
`time` is an *index* into the global `time` table, and the value there is a
delay in nanoseconds that elapses on each channel in `ch_mask` **before** the
event takes effect:

```
for ch in ch_mask:  cursor[ch] += time ; apply state at the new cursor
```

A channel holds the state written by one event until the next event on that
same channel — that is where pulse durations come from. `Wait` events only
advance cursors, which is how the `sync0` / `add_timeN` entries re-align
channels that ran for different lengths.

*Check:* in Cool and Detect the DIO channel spends 3 × 2 µs on its opening
edges while the RF channels take a single 6 µs `add_time`; both then sit at
6 µs, and after the 1.012 ms cooling block both sit at 1.018 ms again.

**Sub-sequences.** `sequence[i].sequences` holds `[ref, label]` pairs in a
single index space shared with the events: `ref < len(event)` is an event,
otherwise it is `sequence[ref - len(event)]`. `LoopSequence` repeats its body
`iterations` times. The root is the sequence named `main`.

**Frequency.** DDS words are a 32-bit fraction of a 1 GHz reference:
`f = word · 1e9 / 2^32`. Optical detuning follows the AOM geometry from
`hardware_description/hardware.py`:
`detuning = order · (2 if double-pass else 1) · (f_dds − f_central)`.

*Check:* Cool and Detect drives 397 π at 67.5 MHz through an 80 MHz double
pass → −25 MHz for Doppler cooling, and at 75 MHz → −10 MHz for detection.

**Amplitude.** 14-bit word, full scale 16383. Amplitude *splines* (pulse
shaping) share the index space of the flat `amp` table: index
`≥ len(amp)` addresses `amp_spline_parameter[i − len(amp)]`, which is why the
spline entries are named with their global index. Spline values are stored at
4× the flat table's resolution (full scale 65532) and `segment_length` counts
4 ns sequencer cycles. The parser keeps the `x0` knots as a piecewise-linear
envelope, which is plenty for audio; the cubic coefficients are ignored.

*Check:* a shaped 729 pulse's spline plateau ÷ 4 lands exactly on an entry of
the flat table, and its falling edge (500 × 4 ns) just fits the 2136 ns wait
that follows it.

## Sound design

`voices.py` gives each beam a carrier pitch and a detuning sensitivity in
semitones per MHz, so colour identifies the beam and pitch tells you where on
the transition you are. Defaults: 397 π 880 Hz, 397 σ 740 Hz, 729 1000 Hz,
866 294 Hz, 854 220 Hz, at 0.4 st/MHz for 397 and 4 st/MHz for 729 (whose
interesting structure lives in the kHz–MHz range). Tickle lines have no AOM,
so their voice is the drive frequency scaled straight down (1 MHz → 1 kHz).

Consequence worth knowing: in Cool and Detect the 15 MHz step from Doppler
cooling to detection is exactly 6 semitones, so the shot reads as a tritone —
low held tone, then two short bright ones.

**Gating.** The 729 single passes (`729_sp`, `729_sp_diag`) and the AODs
(`729_aod_p/m`) are gated by the double pass: they sit on for the whole shot
and put no light on the ion by themselves, so they are parsed but marked
`audible=False` and stay silent. `729_dp` is the 729 you hear. `--include-gates`
un-mutes them. The AOD tones still carry which ion is addressed, which is the
natural thing to map onto stereo position later.

## Time maps

`timemap.py` is where the dial lives. All maps are monotone, so ordering and
overlap survive.

- `linear(stretch)` — real time at 1, `1 ms → 1 s` at 1000.
- `fit(seconds)` — normalise any run to a fixed length.
- `perceptual(gamma, d_ref_ns, stretch)` — each gap of real length `d` gets
  audio length `d_ref · (d/d_ref)^gamma`, so µs gates and ms cooling blocks end
  up within earshot. `gamma = 1` reduces exactly to linear. Exported as a
  piecewise-linear breakpoint table via `breakpoints()` so an engine can
  re-implement it without the maths.

## Known gaps

- Frequency and phase splines are not evaluated (none appear in this day's
  data); the parser flags them in `score.meta["unhandled"]`.
- The inter-shot dead time is not recorded in the `.h5`. `--idle-us` inserts
  it, and the trailing state of `main` (usually Doppler cooling left switched
  on) is held across it, which is what the ion actually sees.
- Measurement outcomes are carried as markers only; nothing yet maps PMT
  counts or `result_channels` onto the sound.
