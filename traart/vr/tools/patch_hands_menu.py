"""Bare-hands support for trap_vr.html: no transport from VR input, and a way out.

    python3 tools/patch_hands_menu.py      # patches body.html, runs build.py

What it changes in body.html:

  * VR input never touches the transport. A tracked hand shows up as an input
    source whose one-button gamepad is the pinch, mapped as the trigger, so
    every right-hand pinch used to pause the bench. Play, run and speed now
    live only on the bench (or the flat page's keys); controllers keep the
    stick navigation, hands do nothing to the scene.
  * A long pinch (or a long trigger) opens a menu: the hands are drawn, a ray
    comes out of each, and a panel in front of the viewer offers Leave VR and
    Back. Point, short-pinch to choose. The panel closes itself after ten idle
    seconds. `session.end()` is what actually leaves.
  * The bench's Exit VR button (an `exit_vr` counter in /clock) still ends
    the session from the laptop.

Idempotent: a body.html that already carries the menu is only rebuilt.
"""
import os, re, subprocess, sys

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
p = os.path.join(HERE, "body.html")
s = open(p).read()

if "xrMenuUpdate" in s:
    print("body.html already has the hands menu; rebuilding only")
else:
    open(p + ".bak-hands", "w").write(s)

    # ── 1. controllers: navigation only, and the menu ─────────────────────
    start = s.index("/* ── controllers ─")
    end = s.index("/* ═", start)
    controllers = r'''/* ── controllers and hands ────────────────────────────────────────────── */
/* Nothing here touches the transport. Play, run and speed are the bench's
   (or, off a disk, the keyboard's): a tracked hand arrives as an input source
   whose only gamepad button is the pinch, mapped as the trigger, and a page
   that pauses on "trigger" pauses every time someone gestures. Controllers
   keep the sticks for turning and magnifying; hands do nothing to the scene. */
function pollXR(dt){
  const s = renderer.xr.getSession(); if (!s) return;
  for (const src of s.inputSources){
    if (src.hand) continue;                       // bare hands: look, don't touch
    const gp = src.gamepad; if (!gp || gp.axes.length < 4) continue;
    const ax = [gp.axes[2], gp.axes[3]];
    const dead = v => Math.abs(v) < 0.14 ? 0 : v;
    if (src.handedness === 'right'){
      const v = dead(ax[1]); if (v) setScale(scaleX * Math.pow(1.9, -v*dt));
      const h = dead(ax[0]); if (h) rig.rotation.y -= h * dt * 1.1;
    }
  }
}

/* ── leaving: a long pinch opens a menu ───────────────────────────────── */
/* The system menu is a poor fit for a page (its pointer is not where people
   expect, and nothing in it is ours), so the page has its own way out. Hold a
   pinch -- or a trigger -- for a second and a panel appears a metre ahead with
   two targets; the hands are drawn so you can see what you are pointing with.
   A short pinch on a target chooses it. Ten idle seconds and it folds away. */
const PINCH_HOLD_S = 1.0, MENU_IDLE_S = 10;
const pinches = new Map();                      // inputSource -> start ms
const MENU = { open: false, hover: new Map(), lastAim: 0, holdFrac: 0 };

const handMat = new THREE.MeshBasicMaterial({ color: 0xD8DCE6 });
const handGeo = new THREE.SphereGeometry(0.006, 8, 6);
const hands = {};
const rays = {};
for (const side of ['left', 'right']){
  const m = new THREE.InstancedMesh(handGeo, handMat, 25);
  m.frustumCulled = false; m.visible = false; m.renderOrder = 998;
  rig.add(m); hands[side] = m;
  const r = new THREE.Line(
    new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(0,0,0), new THREE.Vector3(0,0,-1.2)]),
    new THREE.LineBasicMaterial({ color: 0x7C5CFF, transparent: true, opacity: 0.8, depthTest: false }));
  r.visible = false; r.renderOrder = 998;
  rig.add(r); rays[side] = r;
}

const menuCv = document.createElement('canvas');
menuCv.width = 1024; menuCv.height = 512;
const menuTex = new THREE.CanvasTexture(menuCv);
const menuMesh = new THREE.Mesh(
  new THREE.PlaneGeometry(0.6, 0.3),
  new THREE.MeshBasicMaterial({ map: menuTex, transparent: true, depthTest: false }));
menuMesh.visible = false; menuMesh.renderOrder = 999;
rig.add(menuMesh);

function drawMenu(){
  const g = menuCv.getContext('2d');
  g.clearRect(0, 0, 1024, 512);
  g.fillStyle = "rgba(9,12,20,.92)"; g.fillRect(0, 0, 1024, 512);
  g.fillStyle = "rgba(124,92,255,.35)"; g.fillRect(0, 0, 1024, 3);
  g.textAlign = "left"; g.textBaseline = "alphabetic";
  g.font = "600 34px 'IBM Plex Sans Condensed',sans-serif"; g.fillStyle = "#F2F5FF";
  g.fillText("Audio Bench", 40, 70);
  g.font = "400 22px 'IBM Plex Mono',monospace"; g.fillStyle = "#79839D";
  g.fillText("point with a hand, pinch to choose", 40, 108);
  const hovered = new Set(MENU.hover.values());
  const btn = (x, label, key, accent) => {
    const on = hovered.has(key);
    g.fillStyle = on ? (accent ? "#7C5CFF" : "#2A3244") : (accent ? "rgba(124,92,255,.22)" : "rgba(42,50,68,.6)");
    g.fillRect(x, 170, 432, 220);
    g.strokeStyle = on ? "#F2F5FF" : (accent ? "#7C5CFF" : "#4A5268"); g.lineWidth = on ? 5 : 2;
    g.strokeRect(x, 170, 432, 220);
    g.font = "600 46px 'IBM Plex Sans Condensed',sans-serif";
    g.fillStyle = "#F2F5FF"; g.textAlign = "center";
    g.fillText(label, x + 216, 295);
    g.textAlign = "left";
  };
  btn(40, "Back", "back", false);
  btn(552, "Leave VR", "leave", true);
  g.font = "400 20px 'IBM Plex Mono',monospace"; g.fillStyle = "#4A5268";
  g.fillText("closes itself when idle  ·  the bench can also end the session", 40, 460);
  menuTex.needsUpdate = true;
}

const _camPos = new THREE.Vector3(), _camDir = new THREE.Vector3();
function openMenu(){
  if (MENU.open) return;
  MENU.open = true; MENU.hover.clear(); MENU.lastAim = performance.now();
  // a metre ahead of the eyes, then left where it is: pointing at something
  // that follows your head is miserable
  camera.getWorldPosition(_camPos); camera.getWorldDirection(_camDir);
  const target = _camPos.clone().addScaledVector(_camDir, 1.0);
  menuMesh.position.copy(rig.worldToLocal(target.clone()));
  menuMesh.lookAt(_camPos);                       // lookAt wants world space
  menuMesh.updateMatrixWorld(true);               // so the first pointer frame can hit it
  menuMesh.visible = true;
  for (const side of ['left','right']){ rays[side].visible = true; }
  drawMenu();
}
function closeMenu(){
  MENU.open = false; MENU.hover.clear(); MENU.holdFrac = 0;
  menuMesh.visible = false;
  for (const side of ['left','right']){ hands[side].visible = false; rays[side].visible = false; }
}
function leaveVR(){
  const s = renderer.xr.getSession();
  closeMenu();
  if (s) s.end().catch(() => {});
}

function onSelectStart(e){ pinches.set(e.inputSource, performance.now()); }
function onSelectEnd(e){
  const t0 = pinches.get(e.inputSource); pinches.delete(e.inputSource);
  if (t0 === undefined || !MENU.open) return;
  if (performance.now() - t0 >= PINCH_HOLD_S * 1000) return;       // that was the opening hold
  const key = MENU.hover.get(e.inputSource);
  if (key === 'leave') leaveVR();
  else if (key === 'back') closeMenu();
}

const _ray = new THREE.Raycaster(), _o = new THREE.Vector3(), _d = new THREE.Vector3(), _q = new THREE.Quaternion();
const _m = new THREE.Matrix4(), _s = new THREE.Vector3(), _p = new THREE.Vector3();
function xrMenuUpdate(dt, frame){
  const s = renderer.xr.getSession(); if (!s) return;
  const now = performance.now();
  // a pinch held long enough opens the menu; that pinch is then spent
  MENU.holdFrac = 0;
  for (const [src, t0] of pinches){
    const f = (now - t0) / (PINCH_HOLD_S * 1000);
    if (!MENU.open) MENU.holdFrac = Math.max(MENU.holdFrac, Math.min(1, f));
    if (f >= 1 && !MENU.open){ openMenu(); pinches.delete(src); }
  }
  if (!MENU.open || !frame) return;
  const ref = renderer.xr.getReferenceSpace(); if (!ref) return;
  const seen = new Set();
  let aimed = false;
  for (const src of s.inputSources){
    const side = src.handedness === 'left' ? 'left' : 'right';
    seen.add(side);
    // the hand, as joints
    if (src.hand){
      const mesh = hands[side]; let i = 0;
      for (const joint of src.hand.values()){
        const jp = frame.getJointPose(joint, ref);
        if (!jp || i >= 25){ i++; continue; }
        const r = (jp.radius || 0.006) / 0.006;
        _m.compose(_p.copy(jp.transform.position), _q.copy(jp.transform.orientation), _s.set(r, r, r));
        mesh.setMatrixAt(i++, _m);
      }
      mesh.count = Math.min(i, 25); mesh.instanceMatrix.needsUpdate = true; mesh.visible = true;
    }
    // the ray, and what it points at
    const pose = src.targetRaySpace ? frame.getPose(src.targetRaySpace, ref) : null;
    const ray = rays[side];
    if (!pose){ ray.visible = false; MENU.hover.delete(src); continue; }
    ray.position.copy(pose.transform.position);
    ray.quaternion.copy(pose.transform.orientation);
    ray.visible = true;
    _o.copy(ray.position); _d.set(0, 0, -1).applyQuaternion(ray.quaternion);
    rig.localToWorld(_o); _d.transformDirection(rig.matrixWorld);
    _ray.set(_o, _d);
    const hit = _ray.intersectObject(menuMesh, false)[0];
    let key = null;
    if (hit && hit.uv){
      const u = hit.uv.x, v = 1 - hit.uv.y;            // canvas y runs down
      if (v > 170/512 && v < 390/512){
        if (u > 40/1024 && u < 472/1024) key = 'back';
        else if (u > 552/1024 && u < 984/1024) key = 'leave';
      }
      aimed = true;
    }
    const before = MENU.hover.get(src);
    if (key) MENU.hover.set(src, key); else MENU.hover.delete(src);
    if (before !== key) drawMenu();
  }
  for (const side of ['left','right']) if (!seen.has(side)){ hands[side].visible = false; rays[side].visible = false; }
  if (aimed) MENU.lastAim = now;
  if (now - MENU.lastAim > MENU_IDLE_S * 1000) closeMenu();
}

'''
    s = s[:start] + controllers + s[end:]

    # ── 2. session wiring ─────────────────────────────────────────────────
    s = s.replace('''renderer.xr.addEventListener('sessionstart', () => {
  ui.gate.style.display = 'none';
  panel.visible = true;''', '''renderer.xr.addEventListener('sessionstart', () => {
  ui.gate.style.display = 'none';
  panel.visible = true;
  pinches.clear(); closeMenu();
  const s = renderer.xr.getSession();
  s.addEventListener('selectstart', onSelectStart);
  s.addEventListener('selectend', onSelectEnd);''')
    s = s.replace("renderer.xr.addEventListener('sessionend', () => panel.visible = false);",
                  "renderer.xr.addEventListener('sessionend', () => { panel.visible = false; closeMenu(); pinches.clear(); });")

    # ── 3. the frame reaches the menu ─────────────────────────────────────
    s = s.replace("renderer.setAnimationLoop(() => {", "renderer.setAnimationLoop((_t, xrFrame) => {", 1)
    s = s.replace("  if (renderer.xr.isPresenting){ pollXR(dt); }",
                  "  if (renderer.xr.isPresenting){ pollXR(dt); xrMenuUpdate(dt, xrFrame); }", 1)

    # ── 4. the bench's exit counter ───────────────────────────────────────
    s = s.replace('''  fetching: false, nextTry: 0, title: "", index: 0, count: 0,
};''', '''  fetching: false, nextTry: 0, title: "", index: 0, count: 0,
  exitSeen: null,
};''', 1)
    s = s.replace('''    LINK.shots = Math.max(1, j.shots | 0);
    const newDemo = j.demo !== LINK.demo;''', '''    LINK.shots = Math.max(1, j.shots | 0);
    // The bench's "Exit VR" button bumps a counter; the first value we see is
    // just the baseline, every later change is a request.
    const ex = j.exit_vr | 0;
    if (LINK.exitSeen === null) LINK.exitSeen = ex;
    else if (ex !== LINK.exitSeen){ LINK.exitSeen = ex; if (renderer.xr.isPresenting) leaveVR(); }
    const newDemo = j.demo !== LINK.demo;''', 1)

    # ── 5. the readout panel says how to get out, and shows the hold ──────
    # (the fmtStretch line holds a thin space, so anchor on the unique line after it)
    s = s.replace('''  panelTex.needsUpdate = true;
}''', '''  g.textAlign = "right";
  g.fillStyle = MENU.holdFrac > 0 ? "#F2F5FF" : "#4A5268";
  g.fillText(MENU.holdFrac > 0 ? "keep holding…" : "hold a pinch for the menu", 994, 292);
  g.textAlign = "left";
  if (MENU.holdFrac > 0){
    g.fillStyle = "rgba(124,92,255,.25)"; g.fillRect(694, 302, 300, 6);
    g.fillStyle = "#7C5CFF"; g.fillRect(694, 302, 300 * MENU.holdFrac, 6);
  }
  panelTex.needsUpdate = true;
}''', 1)

    for must in ("xrMenuUpdate(dt, xrFrame)", "onSelectStart)", "exitSeen: null", "hold a pinch for the menu",
                 "(_t, xrFrame)", "closeMenu(); pinches.clear();"):
        if must not in s:
            sys.exit("patch incomplete, anchor missing for: " + must)
    open(p, "w").write(s)
    print("body.html patched (backup in body.html.bak-hands)")

subprocess.run([sys.executable, os.path.join(HERE, "build.py")], check=True)
print("trap_vr.html rebuilt")
