"""High level: a run on disk -> a wav you can listen to."""

from __future__ import annotations

from dataclasses import dataclass

from .h5io import Run, load_run
from .ir import Score
from .parser import parse_sequence
from .render import RenderConfig, render, write_wav
from .timemap import make_timemap


@dataclass
class SonifyConfig:
    channels: list[str] | None = None      # None = every active, audible channel
    include_gates: bool = False
    shots: int | None = 8                  # shots per scan point (None = as measured)
    points: int | None = None              # scan points to include
    idle_ns: int = 0                       # dead time between shots
    gap_ns: int = 0                        # silence between scan points
    max_loop_iterations: int | None = None  # cap huge LoopSequences
    time_map: str = "linear"
    time_map_kw: dict | None = None
    markers: bool = False
    stereo: bool = True
    sample_rate: int = 44100


def score_for_run(run: Run, cfg: SonifyConfig) -> Score:
    """Replay the run as the ion met it: shots within a point, then next point."""
    per_point: list[Score] = []
    seqs = run.sequences if cfg.points is None else run.sequences[:cfg.points]
    n_shots = run.n_shots if cfg.shots is None else min(cfg.shots, run.n_shots)
    for _idx, doc in seqs:
        s = parse_sequence(doc, max_loop_iterations=cfg.max_loop_iterations,
                           idle_ns=cfg.idle_ns)
        per_point.append(s.repeat(max(n_shots, 1)))
    score = Score.concat(per_point, gap_ns=cfg.gap_ns) if len(per_point) > 1 else per_point[0]
    score.meta.update({
        "experiment": run.short_name,
        "job_id": run.job_id,
        "file": run.path,
        "points_rendered": len(per_point),
        "shots_rendered": n_shots,
    })
    return score


def sonify_run(path: str, out_wav: str, cfg: SonifyConfig | None = None,
               out_score: str | None = None):
    cfg = cfg or SonifyConfig()
    run = load_run(path, max_points=cfg.points)
    score = score_for_run(run, cfg)
    tm = make_timemap(cfg.time_map, **(cfg.time_map_kw or {}))
    rcfg = RenderConfig(sample_rate=cfg.sample_rate, time_map=tm,
                        channels=cfg.channels, markers=cfg.markers,
                        stereo=cfg.stereo, include_gates=cfg.include_gates)
    audio, sr = render(score, rcfg)
    write_wav(out_wav, audio, sr)
    if out_score:
        score.save(out_score)
    return score, audio, sr
