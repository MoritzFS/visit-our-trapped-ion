"""python -m traart.cli --help"""

from __future__ import annotations

import argparse
import json
import os

from .h5io import list_runs
from .sonify import SonifyConfig, sonify_run


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="traart", description="Hear an ion trap experiment.")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_index = sub.add_parser("index", help="list the runs in a data folder")
    p_index.add_argument("folder")

    p = sub.add_parser("sonify", help="render a run to wav")
    p.add_argument("h5")
    p.add_argument("-o", "--out", default="out.wav")
    p.add_argument("-c", "--channel", action="append", dest="channels",
                   help="beam key, repeatable (397_pi, 729_dp, 866, ...). default: all")
    p.add_argument("--shots", type=int, default=8, help="shots per scan point")
    p.add_argument("--points", type=int, default=None, help="scan points to render")
    p.add_argument("--stretch", type=float, default=1000.0,
                   help="1 = real time, 1000 = 1 ms -> 1 s")
    p.add_argument("--timemap", default="linear",
                   choices=["linear", "fit", "perceptual"])
    p.add_argument("--gamma", type=float, default=0.5, help="perceptual map exponent")
    p.add_argument("--seconds", type=float, default=4.0, help="fit map target length")
    p.add_argument("--idle-us", type=float, default=0.0, help="dead time between shots")
    p.add_argument("--max-loops", type=int, default=None, help="cap LoopSequence reps")
    p.add_argument("--markers", action="store_true", help="click on PMT gates")
    p.add_argument("--mono", action="store_true")
    p.add_argument("--include-gates", action="store_true",
                   help="also sound the 729 single passes / AODs, which are "
                        "normally silent because the double pass gates them")
    p.add_argument("--score", default=None, help="also dump the score IR as json")

    a = ap.parse_args(argv)

    if a.cmd == "index":
        for r in list_runs(a.folder):
            print(json.dumps(r))
        return 0

    kw = {"linear": {"stretch": a.stretch},
          "fit": {"seconds": a.seconds},
          "perceptual": {"gamma": a.gamma, "stretch": a.stretch}}[a.timemap]
    cfg = SonifyConfig(channels=a.channels, shots=a.shots, points=a.points,
                       idle_ns=int(a.idle_us * 1000), time_map=a.timemap,
                       time_map_kw=kw, markers=a.markers, stereo=not a.mono,
                       max_loop_iterations=a.max_loops,
                       include_gates=a.include_gates)
    score, audio, sr = sonify_run(a.h5, a.out, cfg, out_score=a.score)
    print(f"{score.meta.get('experiment')}  job {score.meta.get('job_id')}")
    heard = a.channels or score.channel_keys(only_audible=not a.include_gates)
    print(f"  channels heard : {', '.join(heard)}")
    print(f"  points x shots : {score.meta['points_rendered']} x {score.meta['shots_rendered']}")
    print(f"  experiment time: {score.duration_ns/1e6:.3f} ms")
    print(f"  audio          : {len(audio)/sr:.2f} s -> {os.path.abspath(a.out)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
