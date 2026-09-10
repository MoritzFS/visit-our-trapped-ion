# Audio Bench

A standalone desktop application for listening to ion trap sequences. Grey
chassis, square corners, hard bevels, serif type — it looks like an instrument
from the Motif era because that is what it is: a front panel for a signal you
cannot otherwise hear.

```
cargo run --release
```

That is the whole build. Rust 1.75+ and a working audio output are the only
requirements; on macOS and Windows nothing else is needed, on Linux you want
`libasound2-dev` and the usual X11/Wayland development packages.

## What it plays

Five runs from `icon_experiment_data_2026-09-01`, exported by the Python side
(`traart/export_demos.py`) into `demos.json`, which is compiled into the binary
and can be overridden at runtime:

```
cargo run --release -- /path/to/other-demos.json
```

| run | what you hear |
| --- | --- |
| Cool and Detect | 1 ms of 397 at −25 MHz, a 20 µs sigma pulse, two 200 µs detection windows at −10 MHz, repeating |
| 729 Carrier Sweep | a 500 µs probe at 0.8 % amplitude stepped in 300 Hz increments across the S(−1/2)→D(−5/2) line, 15 kHz end to end |
| GKP QEC, one mode | the QEC loop count scanned 1 → 81, so the correction cycle repeats more and more before readout |
| GKP QEC, two modes | the same on both radial modes (2.543 and 2.805 MHz); sideband cooling alternates between them |
| All sidebands, one ion | the probe sweeping across carrier and motional sidebands |

## The controls

**SLOWDOWN** is the point of the whole thing. It moves the playback cursor
through *experiment* time — 1× is real time, 1000× turns one millisecond into
one second. Because every note is synthesised from the score at the moment it
starts, and not resampled, changing it stretches the rhythm and leaves the
tuning exactly where it was.

Each beam gets a strip:

- **CARRIER** — the pitch at zero detuning. 729 is a bass, 854 and 866 sit
  under everything, 397 is high so that detection rings out above cooling.
- **sens** — semitones of pitch per MHz of detuning. This is what turns a
  frequency scan into a scale. 397 wants a fraction of a semitone per MHz; the
  729, whose structure lives in the kHz–MHz range, wants a few.
  The slider is logarithmic across five decades because the range really is
  that wide: the bare 729 scan needs 1600 st/MHz, the 397 wants a third of one.
- **resp** — amplitude response. 1 is linear; below 1 lifts weak pulses towards
  the loud ones. The 729 probe in the carrier sweep sits at 0.8 % of full scale
  against full-scale cooling light, which is inaudible until this is turned
  down.
- **level**, **ring**, **mute**, waveform — `ring` is how long a pulse keeps
  sounding after it ends. A 3 µs 729 probe is shorter than one cycle of a bass
  note, so without a tail it is silent; the attack still lands exactly on the
  pulse.
- **centre** — put the carrier at this run's median detuning.
- **fit** — centre, then set `sens` so this run's whole detuning span opens out
  to two octaves. The fastest way to make an unfamiliar run audible.

**Quantise to scale** snaps every beam onto one scale, so a sweep climbs it in
steps instead of gliding.

## Run parameters

Some runs expose a piece of their sequence as a live control, shown under the
run list:

- **QEC rounds** (both QEC runs) — `from`, `to` and `points` generate the scan
  on the fly. The run is stored as its one-round template; the player repeats
  the QEC block, sliding everything after it and holding anything that spans
  it. This is exact: rebuilt sequences match the real 11-, 41- and 81-round
  sequences pulse for pulse on every raw channel. (The composite 729 view
  drifts 12 ns per round, because the double pass's shaped edges sit on a 4 ns
  grid and genuinely advance 12 ns more per round than the other channels.)
  Defaults come from what was measured: 1 to 81 rounds, nine points.
- **probe** (729 Carrier Sweep) — the length of the `729 Pulse` block, 0.1 to
  8 ms, as run 0.512 ms. What lies inside the block is rescaled, what follows
  slides.

Both are the same operation, `edit_point` in `src/score.rs`, mirrored by
`traart/regions.py` on the Python side, which is where the pulse-for-pulse
check lives. To expose a block in another run, name it in its `DemoSpec`
(`loop="QEC"` or `stretch="729 Pulse"`); the exporter captures its time span.

## Saving a tuning

Nothing in a run says what it should sound like, and the settings that make one
audible are not the ones that make the next audible. **Tunings**, under the run
parameters, keeps them: type a name, press SAVE, and the slowdown, the master,
the scale and root, this run's parameters and every beam strip go in under it.
Click the entry later and they all come back, the run it was made on included,
if the bundle still has that run.

A tuning is the listening and not the data, so it is only numbers. Beams are
matched by key, which means one saved on a run with a 729 and a 397 lands on
any other run that has them, and a beam the tuning says nothing about keeps the
run's own setting; the readout says how many of this run's beams a recall
actually set. Saving under a name that exists replaces it, so a tuning can be
refined in place. The first knob moved afterwards clears the highlight, because
what is on the panel is then no longer what is in the file. DELETE forgets the
highlighted one.

They live in one JSON file:

```
~/Library/Application Support/traart-bench/tunings.json   macOS
$XDG_CONFIG_HOME/traart-bench/tunings.json                Linux
%APPDATA%\traart-bench\tunings.json                       Windows
```

which is readable and worth editing by hand. Its beam fields are named the way
the `voice` dict of a `DemoSpec` names them -- `carrier`, `refMHz`, `sens`,
`gain`, `resp`, `tail`, `pan` -- so a tuning worth keeping copies straight into
`export_demos.py` and becomes that run's default. `--tunings <file>` reads and
writes somewhere else, which is how a set is shared; `--tuning <name>` opens on
one, the way `--start` opens on a run:

```
cargo run --release -- --tuning "warm 729"
cargo run --release -- --tunings ./listening-session.json --start qec_2modes
```

Out of range values in a hand-edited file are clamped to what the control
offers rather than refused, and an unknown wave or scale name leaves that
setting alone.

## How it is put together

```
sequence JSON  ──►  Score IR  ──►  demos.json  ──►  Timeline  ──►  audio callback
 (.h5, Python)      (Python)       (compact)        (Rust)         (Rust, cpal)
```

The split that matters is the score: flat, integer-nanosecond, no audio
decisions in it. `src/score.rs` reads it and flattens shots and scan points into
one timeline; `src/engine.rs` walks that timeline in experiment time inside the
audio callback and spawns a voice per tone; `src/synth.rs` owns the only place
where a detuning becomes a frequency; `src/retro.rs` is the chassis;
`src/main.rs` is the panel; `src/serve.rs` is the door the headset comes in
through.

Playback state lives behind one mutex, locked once per audio buffer. The UI
edits a `Params` and bumps a generation counter; the audio thread notices on its
next buffer. Nothing is allocated per sample.

## Serving the headset

The bench can hand out the VR page itself, and hand out its playback cursor
with it, so the pulse you see arrive at the ion is the pulse you hear:

```
cargo run --release -- --serve ../traart/vr
```

It prints the URL to open in the Quest browser (`--port N` moves it off 8443),
and the title bar shows the link state — how many times the headset has polled
and how long ago. Sound stays here, on this machine's speakers; the headset
only draws.

The reason this is a server and not a broadcast is that **there is one cursor**.
The audio thread owns it -- `Shared::cursor_ns`, experiment nanoseconds over the
flattened timeline -- and the page does not run a clock of its own. `/clock`
returns the cursor, the rate and the server's wall time; the page fits the
offset between the two machines from the round trip, the way NTP does, and
evaluates the cursor afresh every frame rather than integrating its own. Four
polls a second, and the residual measured at the frame boundary is under a
millisecond regardless of frame rate. Streaming frames would be absurd anyway:
at a slowdown of 1000, one frame of wall clock is 16 microseconds of experiment.

| | |
| --- | --- |
| `GET /clock` | cursor, total, stretch, playing, which run, server time |
| `GET /score?demo=id` | that run's score as the bench is playing it — the run parameters (QEC round count, probe length) already applied, so a QEC run arrives as its generated scan points, not the one-round template in `demos.json`. `gen` in `/clock` changes whenever this does. |
| `GET /cmd?toggle=1` | and `next`, `demo=N`, `stretch=N`, `rewind` |
| `GET /ping` | five bytes, for when nothing works and you need to know why |
| anything else | a file from the served directory |

The headset's controllers post to `/cmd`, so the trigger pauses the audio here
and **A** steps to the next run on both at once. The masthead shows the URL and
lights when something is actually reading the clock.

HTTPS, with a self-signed certificate, because WebXR will not enter immersive
mode on anything but a secure origin and a LAN address over plain http is not
one. The certificate lives in `<served dir>/.certs/` and is shared with
`traart/vr/serve.py`, so the headset accepts it once whichever of the two is
serving; it is regenerated when this machine's addresses change. A browser that
assumed `http://` for a bare `address:port` -- which every URL bar does, and
which in a headset looks like an eternal spinner rather than an error -- gets a
redirect rather than a failed handshake.

```
cargo run --release -- --serve-only ../traart/vr      # server and cursor, no
                                                      # window, no audio device
```

is the same thing without eframe or cpal, which is how the sync gets tested on
a machine with neither.

### Two things it can do without a window

```
cargo run --release -- --render out.wav 20 qec_1mode --stretch 1000
cargo run --release -- --render out.wav 20 cool_det --solo 397_pi
cargo run --release -- --selftest
cargo run --release -- --stats qec_1mode 81        # notes and length at 81 rounds
cargo run --release -- --stats carriers_729 8.0    # ... with an 8 ms probe
cargo run --release -- --start qec_2modes          # open on a given run
```

`--render` bounces a run to a WAV through the same engine the speaker hears.
`--selftest` lays out every panel headlessly and reports the shape count, which
is how the UI gets checked on a machine with no display; it also saves a tuning
to a scratch file, moves every control, recalls it, and fails if anything comes
back different.

### The guess-the-experiment mode

The **Blind** switch in the title bar is the hook for it: it replaces every run
name, subtitle and scan label with `Run A`, `Run B`, and so on, so you can play
a run and try to place it before revealing. Scoring, a shuffle and a
multiple-choice panel are the missing pieces; the state they need is already
where they would look for it (`App::blind`, `Demo::tags`).

## Adding a run

On a machine that can reach the data:

```
python3 export_demos.py          # edit SPECS at the top first
cp web/demos.json ../traart-bench/demos.json
cargo run --release
```

`DemoSpec` takes the `.h5` path, how many scan points to keep (`point_stride`
thins a long scan), how many shots per point to loop through, and an optional
`voice` dict of per-beam overrides:

```python
voice={"729": {"carrier": 220.0, "refMHz": 25.339947, "sens": 1600.0,
               "gain": 1.6, "resp": 0.25, "tail": 0.10}}
```

Anything omitted keeps the global default. This is how a run whose entire
structure is 15 kHz wide and 0.8 % deep gets a mapping of its own instead of
being buried by settings that suit a 5 MHz sideband spectrum. If you do not
know what a new run needs, load it and press **fit** on the beam you care
about, then copy the numbers the strip shows back into the spec -- or save the
panel as a tuning and copy them out of `tunings.json`, where they already have
these names.
