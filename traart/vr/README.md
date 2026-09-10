# tr(art) VR — seeing the ion

Standing inside the trap while the sequence runs. Same score, other sense: the
audio bench turns a run into pitch, this turns it into light arriving from a
direction.

```
GKP_trap.stp ──step_to_mesh──► trapmesh.js ─┐
                                            ├──build──► trap_vr.html
web/demos.json ──export_vrscore──► vrscore.json ─┘        (one file, no network)
```

`trap_vr.html` is self-contained: three.js, the trap geometry and the pulse
score and even the typefaces are all inlined. It runs from a `file://` URL on a
laptop and needs no internet, ever. WebXR only grants immersive mode on
`https://` or `localhost`, so for the Quest it has to be served — `serve.py`
does that in one command; see **Getting it into the headset, on site**.

## Where you are standing

The STEP file is one 18.90 × 10.00 × 2.25 mm plate. The through-slot lined
with segmented electrodes is the trap, and its centre is the ion:

| | CAD | app |
|---|---|---|
| slot gap | x ∈ [3.0464, 3.6079] mm | **vertical**, 0.5615 mm |
| slot length | z ∈ [−2.2575, 2.2575] mm | along **world X**, 4.515 mm |
| slot depth | full 2.25 mm thickness | along **world Z** |
| ion | (3.32715, −5.44959, 0) mm | origin |

Those numbers are measured, not eyeballed: rays cast through the plate along
CAD *y* over a grid in x–z leave exactly two open regions at that end, and the
segmented one is the slot above. The plate's long CAD axis (x) becomes world
**+Y**, so it stands vertically as asked, with the ion 1.90 mm from the top and
17.00 mm of plate hanging below — at the default 10 000× that is 19 m of
ceiling and 170 m of wall going down into the dark.

The mapping, which is also the frame the beam directions are written in:

```
world +X = CAD +z    along the slot
world +Y = CAD +x    up
world +Z = CAD +y    through the plate  (the "big trap plane" normal)
```

## The beams — the block you will want to edit

Near the top of the module in `body.html`, under **OPTICAL GEOMETRY**:

```js
const BEAMS = {
  "729":       { dir:[ 0, 0, 1], ... },              // ⟂ to the plate
  "397_pi":    { dir:[ 0.70711, 0, 0.70711], ... },  // 45°, y+z
  "866":       { dir:[ 0.70711, 0, 0.70711], ... },  // with 397 π
  "397_sigma": { dir:[ 1, 0, 0], ... },              // down the slot
  "854":       { dir:[ 0.70711, 0,-0.70711], ... },  // ← a guess, mirrored 45°
};
```

`dir` is the propagation direction and the ion is at the origin, so the pulse
flies in along `+dir` and leaves the far side. Change a vector, run
`python3 build.py`, reload.

| beam | world `dir` | in CAD | ∠ slot axis (z) | ∠ plate normal (y) | azimuth |
|---|---|---|---|---|---|
| 397 σ | `[ 1, 0, 0]` | +z | 0° | 90° | 0° |
| 854 | `[ 1, 0, 0]` | +z | 0° | 90° | 0° |
| 397 π | `[0.70711, 0, 0.70711]` | (z+y)/√2 | 45° | 45° | 45° |
| 866 | `[0.70711, 0, 0.70711]` | (z+y)/√2 | 45° | 45° | 45° |
| 729 | `[0, 0, 1]` | +y | 90° | 0° | 90° |

All five lie in the horizontal plane, so all are perpendicular to the plate's
long (vertical) axis. Pairwise: σ–π 45°, σ–729 90°, π–729 45°; σ and 854 are the same ray.

**The axial pair is the trap's own geometry, not a guess.** Two conical bores
are cut into the slot walls from either side along CAD z, tapering 0.880 mm to
0.500 mm over 2.15 mm — a 5.0° half angle — and they are centred on
(x 3.3256, y −5.4495). That is the ion position, derived independently from the
slot's centre, to within **2 µm in x and 0.14 µm in y**. A ray along that line
meets no material anywhere in the plate: it is the one clear sight-line through
the trap, which is what makes it the axial access. 397 σ and 854 share it,
co-propagating: in through one cone, out through the other.

866 sits exactly on the 397 π ray and 854 exactly on σ's, so each repumper is
drawn with a wider waist (0.052 and 0.046 mm against 0.026) — otherwise the pair
would be one line and you would never see the repumper come on.

`G` toggles a faint ghost that draws the whole beam line through material,
which is the quick way to check where a direction actually points.

Beams that are gated rather than lit — the 729 single passes, the AODs — are
already `audible:false` in the score, so they never appear, same as in the
audio. Tickles have no beam at all: they shake the electrodes, so they pulse
the slot walls teal instead.

## What you see, and what is honest about it

A laser in a vacuum has nothing to scatter off and would be invisible from the
side. The shaft is a deliberate lie; the parts that are not:

- **timing** — every edge is the recorded sequence, at integer nanoseconds,
  through the same parser as the audio.
- **detuning** — the number on each beam in the legend is the real optical
  detuning, and the readout is live.
- **geometry** — the plate, the slot, the electrode segmentation, the ion's
  position in it.
- **colour** — 397 is violet-ish because 397 nm is; 729, 854 and 866 are
  infrared and you would see none of them, so they carry the audio bench's
  palette instead of a wavelength.

The wavefront's flight time is a constant ≈0.3 s of wall clock, not a scaled
speed of light: at these stretch factors real propagation is either
instantaneous or geological.

## Controls

| | desktop | Quest |
|---|---|---|
| look / move | click to capture, mouse + `WASD` `QE`, `shift` faster | head + right stick to turn |
| magnification | wheel, or `-` `=` | right stick ↕ |
| time stretch | `[` `]` | left stick ↕ |
| hold | `space` | right trigger |
| run | `1`–`4` | A |
| scan point | `←` `→` | Y |
| vantage / ion | `R` / `F` | — |
| ghost beams | `G` | — |
| hide the key list | `H` | — |

The default vantage is 4 m from the ion, at beam height, on the azimuth with the
widest clearance from any beam (22.5°); `F` puts you on the ion itself, which is
the intended seat but a poor camera. `VIEW_AZ`, `VIEW_R` and `VIEW_H` at the top
of the module set it.

Every beam passes through the ion and nowhere else, but at 4 m the nearest beam
axis still runs 1.5 m past your head, and the eye reads that line as aimed at
itself. So nothing nearer to you than the ion is drawn: what survives of each
beam starts at the ion and recedes, and the cull is relative to your own
position, so walking around changes which half you see rather than putting a
beam in your face.

## Hearing it as well: serving from the bench

`serve.py` serves the page. The Rust bench serves the page **and its playback
cursor**, which is the version you want once sound is involved:

```
cd ../../traart-bench
cargo run --release -- --serve ../traart/vr
```

Open the URL it prints. The page notices it is being served by the bench and
stops running a clock of its own: `/clock` gives it the cursor in experiment
nanoseconds plus the rate, it fits the offset between the two machines from the
round trip the way NTP does, and it evaluates that prediction fresh every frame
rather than integrating its own. Measured at the frame boundary the residual is
under a millisecond and does not depend on frame rate, so the visual lands
within one headset frame of the sound.

It also fetches the run's score from the bench rather than using the copy baked
into the file, so both sides flatten the same numbers -- points repeated
`shots` times, laid end to end, exactly as `score.rs` does it. A subset baked in
months ago would put the picture a whole scan point out of step.

The controllers become the bench's transport: the trigger pauses the audio,
**A** steps to the next run on both at once, the left stick re-times both. The
`time` line in the corner reads `· bench` when the link is up.

With nothing serving the clock -- opened off a disk, or off `serve.py` -- the
page runs its own cursor over the score inside it, and everything else is the
same. `__traart()` in the browser console reports the link state and the error
in milliseconds of wall clock; with the Quest on a cable, `chrome://inspect`
gets you that console.

## Getting it into the headset, on site, with no internet

Nothing in `trap_vr.html` fetches anything — three.js, the typefaces, the trap
geometry and the score are all inside the file. The laptop and the headset
never need to reach the internet. They only need to reach *each other*, and the
page needs to arrive over a **secure context**, because WebXR will not offer
*Enter VR* on `file://` or on a plain `http://192.168.x.x`. It just quietly
doesn't appear.

So: serve it over https, from the Mac.

```bash
cd traart/vr
python3 serve.py            # https on :8443
```

It makes a self-signed certificate on first run (into `.certs/`, covering
every address the Mac answers on) and prints the exact URLs to type. macOS will
ask whether to allow incoming connections for Python — say yes. In the headset
browser the first load warns about the certificate: **Advanced → Proceed**.
That is the route Meta's own WebXR docs point at, not a workaround.

### Getting the two on the same network

In rough order of how likely they are to just work:

1. **Any Wi-Fi both can join.** It does not need internet — a lab SSID with a
   dead uplink is fine. The one thing that kills it is *client isolation*,
   which most guest/eduroam-style networks turn on: the headset then cannot see
   the Mac at all. If the URL times out, that is almost always why.
2. **A phone hotspot.** Turn on Personal Hotspot, join both devices. No mobile
   data is spent — the traffic never leaves the hotspot. This is the least
   fussy option when the venue Wi-Fi is locked down.
3. **Internet Sharing on the Mac** — System Settings → General → Sharing →
   Internet Sharing, share *from* one interface *to* Wi-Fi. The catch is that
   it needs a second interface to share from: Ethernet (a USB-C dongle),
   Thunderbolt Bridge, or iPhone USB. You cannot share Wi-Fi to Wi-Fi.
4. **The old "Create Network…"** ad-hoc item in the Wi-Fi menu — the thing you
   are remembering. Apple's how-to for it stops at macOS Monterey and it is
   gone from the Wi-Fi menu on current macOS. Have a look if you like, but do
   not build the demo around it.
5. **A USB-C cable and no network at all.** The bulletproof one, worth setting
   up before you travel:

   ```bash
   brew install --cask android-platform-tools     # once
   # Quest: Settings -> System -> Developer -> USB debugging, then plug in
   # and accept the "Allow USB debugging" prompt inside the headset
   adb devices                                    # should list the Quest
   adb reverse tcp:8443 tcp:8443
   ```

   Now `https://localhost:8443/trap_vr.html` inside the headset is the Mac's
   server, over the cable. `localhost` is a secure context on its own, so the
   certificate matters even less. Developer mode needs a Meta developer
   account, which is free but takes a few minutes — do it at your desk.

A dry run at your desk on the same setup you will use on site is worth the
twenty minutes. The failure modes are all network ones, and they all look the
same from inside the headset: a page that loads but has no *Enter VR* button
(not secure), or one that never loads at all (client isolation, firewall).

## Rebuilding

```bash
python3 tools/step_to_mesh.py ../../GKP_trap.stp trapmesh.js   # geometry
cd .. && python3 tools/export_vrscore.py                       # score, from web/demos.json
python3 build.py                                               # -> trap_vr.html
```

`build.py` also writes `trap_vr.artifact.html`, the same page without the
`<!doctype>`/`<head>` shell, for publishing as a Claude Artifact.

`serve.py` needs nothing but the standard library and `openssl`, both of which
macOS already has.

## Next

Sound is the point of the pairing and it is not here yet. The score the page
already holds is the same object the renderer voices, so the join is small:
port `voices.py`'s pitch rule into a Web Audio graph, one oscillator per
channel, and pan each voice to its beam's `dir` through a `PannerNode` — then
the 397 σ you hear is coming from down the slot and the 729 from behind the
plate, and the tritone in Cool and Detect arrives from two different places.
