# Dispatch: the Quest hangs on "loading"

*Paste this whole file into Claude Code, running in `traart/vr/` on the MacBook.
It has the shell access this needs; the Cowork VM does not.*

---

## The job

A WebXR page, `trap_vr.html`, is served off this MacBook by `serve.py` over
HTTPS on the LAN. A Meta Quest 3 on the same Wi-Fi opens the address and **sits
on a loading spinner forever** — no error, no certificate warning reported, no
page. Find out why, fix it, and confirm it fixed.

You have a real shell on the machine that is both the web server and the only
witness. Use it. Run commands, read the log, bisect. Do not theorise past the
first fact you can actually measure.

## Ground truth, so you do not re-derive it

- `trap_vr.html` is **one self-contained file, 1.14 MB**: three.js r160, the
  trap geometry as base64, the pulse score as inline JSON, and IBM Plex as
  `@font-face` data URIs. It fetches **nothing** at runtime. So a hang is not a
  blocked sub-resource — there are none.
- It is **verified working** in desktop Chromium (headless, software GL): loads
  clean, no console errors, renders the scene. So the page is not simply broken.
- WebXR needs a secure origin. That is the only reason HTTPS is involved.
- `serve.py` binds `0.0.0.0`, HTTPS on **8443**, and a plain-HTTP port on
  **8444** that exists purely as a reachability probe.
- Both ports answer `/ping` with `pong` — five bytes, no TLS-free excuses.
- `python3 build.py` regenerates `trap_vr.html` from `body.html` + `vendor/`.

## Try this first — a bug that explains the symptom exactly

`serve.py` was just rewritten. The previous version wrapped the **listening**
socket in TLS, which performs the handshake inside the accept loop. Any client
that opens a TCP connection and then says nothing blocks *every* other
connection, forever, silently. Chromium-based browsers — the Quest browser
included — open speculative preconnect sockets constantly. That produces
precisely this symptom: a spinner, no error, nothing in any log.

The handshake now happens in the worker thread with a 15 s timeout, and every
connection is logged.

So: **restart the server and just try again** before anything else.

```bash
cd "$(dirname "$0")" 2>/dev/null || true
caffeinate -i python3 serve.py
```

If it loads now, stop — that was it. Report which of the two it was.

## If it still hangs, bisect in this order

Each step answers one yes/no question. Do not skip ahead; the answer to step 3
decides whether the rest of the work is on the network side or the browser side.

### 1. Does the Mac serve itself correctly?

```bash
curl -sk https://127.0.0.1:8443/ping
curl -sk -o /dev/null -w 'page: %{http_code}  %{size_download} B  %{time_total}s\n' \
     https://127.0.0.1:8443/trap_vr.html
```

Expect `pong`, then `200 1144165 B` in well under a second. The server also runs
a self-test at startup and logs `self-test ... OK`. If any of that fails the
problem is local and nothing else matters yet.

### 2. What address should the headset even be using?

```bash
ipconfig getifaddr en0            # Wi-Fi; try en1 if empty
route -n get default | sed -n 's/.*interface: /iface: /p'
netstat -an | grep -E '\.844[34] ' | head
```

Confirm the server is listening on `*.8443`, not `127.0.0.1.8443`. Note the
subnet. Then check the headset's own IP: **Quest → Settings → Wi-Fi → your
network → Advanced**. If it is not on the same `/24`, the access point is
putting wireless clients on a separate VLAN and no amount of server work will
help — go to the cable route at the bottom.

### 3. Does anything from the headset reach this machine at all?

**This is the pivotal question.** Leave `serve.py` running in one pane, watch
its log, and load `https://<mac-ip>:8443/ping` in the headset browser. Five
bytes, so nothing about page size or JS can be blamed.

- **Log stays completely empty** → nothing arrives. The problem is the network
  path. Go to §4.
- **`connect` logged, then `TLS handshake failed`** → the path is fine, TLS is
  not. Go to §5.
- **`GET /ping ... 200` logged and the headset shows `pong`** → the path and TLS
  are both fine. The problem is the page itself. Go to §6.
- **`GET /trap_vr.html ... 200` logged but the headset still spins** → it was
  served; either the transfer stalled mid-body or the browser is stuck
  executing. Go to §6.

Also try `http://<mac-ip>:8444/ping` — plain HTTP, same machine. If that works
and 8443 does not, it is TLS and only TLS.

### 4. Nothing reaches the Mac

In order of likelihood:

```bash
/usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate
/usr/libexec/ApplicationFirewall/socketfilterfw --getblockall
/usr/libexec/ApplicationFirewall/socketfilterfw --listapps | grep -i -A2 python
```

If the firewall is on and Python is not explicitly allowed, incoming
connections are dropped with no trace at either end. Either allow it (System
Settings → Network → Firewall → Options) or turn the firewall off for the test.

Then rule the Mac out entirely: put a phone on the same Wi-Fi and open
`http://<mac-ip>:8444/ping` in its browser. A phone gives you a real error
message where the headset gives you a spinner.

- Phone works, headset does not → the headset is the odd one out: wrong
  network, wrong subnet, or a per-client rule.
- Neither works → **client isolation** on the access point. Nothing to fix on
  this machine. Use a phone hotspot or the cable.

### 5. It connects but TLS fails

Read the exact `SSLError` from the log; it names the cause. Then:

```bash
openssl x509 -in .certs/cert.pem -noout -text | grep -A1 'Alternative Name'
openssl x509 -in .certs/cert.pem -noout -dates
```

The certificate must list the **exact IP** you typed into the headset.
`serve.py` now regenerates it automatically when this machine's addresses
change, but force it if in doubt:

```bash
rm -rf .certs && caffeinate -i python3 serve.py
```

Then check what the headset actually negotiates:

```bash
openssl s_client -connect <mac-ip>:8443 -servername <mac-ip> </dev/null 2>&1 | head -25
```

### 6. It arrives but the page never finishes

Now you can see inside the headset's browser. This is the definitive tool and
worth the setup:

```bash
adb devices                                # Developer Mode must be on
adb forward tcp:9222 localabstract:chrome_devtools_remote
```

Open `chrome://inspect` in desktop Chrome, find the Quest's tab, click
*inspect*. You get its real console, network waterfall and timings. Look for:

- the `trap_vr.html` request — did it complete, or stall part-way through the
  1.14 MB body?
- a JS exception. Note the page's whole app is one `<script type="module">`; if
  it throws, the static gate screen still paints, so **a visible "Inside the
  Trap" screen with a dead button is a JS failure, and a blank white or grey
  screen is a transfer failure.** Establish which one you are looking at — ask
  the user if you cannot see the headset.
- memory pressure or a renderer crash.

Measure the transfer from a second machine on the same Wi-Fi:

```bash
curl -sk -o /dev/null -w 'time %{time_total}s  speed %{speed_download} B/s\n' \
     https://<mac-ip>:8443/trap_vr.html
```

If the download is genuinely slow or stalling, the fix is to stop shipping one
1.14 MB file. `build.py` inlines four things that do not have to be inlined —
`vendor/three.module.min.js` (670 KB), `vendor/fonts.css` (116 KB),
`trapmesh.js` (174 KB) and `vrscore.json` (144 KB). Add a mode that emits them
as separate files next to the page and references them with plain `<script>`,
`<link>` and `fetch`. The page then streams and caches instead of arriving as
one indivisible block, and the browser can show progress. Keep the single-file
build — it is what makes `file://` and USB-stick use work — and make the split
build a flag, not a replacement.

## The escape hatch, if the network turns out to be the problem

A USB-C cable removes the network from the picture entirely, and `localhost` is
a secure origin on its own merits, so the certificate stops mattering too:

```bash
adb reverse tcp:8443 tcp:8443
adb shell am start -a android.intent.action.VIEW -d https://localhost:8443
```

If this works while the Wi-Fi route does not, that is the diagnosis: the
network, not the page. Say so plainly rather than continuing to fix the server.

## What to report back

1. Which step the failure landed on, and the **exact log line or error** that
   established it.
2. Whether the cable route works — that single bit separates network from page.
3. What you changed, if anything.
4. If it turned out to be the preconnect bug, say so; that closes it.

Do not change `body.html`'s rendering, the beam geometry, or the trap mesh.
This is a delivery problem, not a graphics one.
