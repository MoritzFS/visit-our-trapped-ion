"""Subset web/demos.json down to what the VR page inlines.

The page eats exactly the browser player's format, so this is a filter, not a
new exporter: pick the runs, cap the number of scan points, dump compact.
Run it from traart/ after export_demos.py.
"""
import json
import os

SRC = "web/demos.json"
DST = "vr/vrscore.json"

# (demo id, how many scan points to keep)
PICK = [
    ("cool_det", 1),        # 9 pulses -- the backbone, and the clearest to look at
    ("carriers_729", 41),   # the whole sweep: the 729 shaft walks across the carrier
    ("qec_1mode", 2),       # thousands of pulses; a storm of light
    ("qec_2modes", 2),
]

src = json.load(open(SRC))
by = {d["id"]: d for d in src["demos"]}
out = []
for key, n in PICK:
    d = dict(by[key])
    d["points"] = d["points"][:n]
    out.append(d)

os.makedirs(os.path.dirname(DST), exist_ok=True)
js = json.dumps({"version": 1, "demos": out}, separators=(",", ":"))
open(DST, "w").write(js)
for d in out:
    print("%-14s points=%-4d pulses=%d" % (
        d["id"], len(d["points"]), sum(len(p["pulses"]) for p in d["points"])))
print("bytes:", len(js))
