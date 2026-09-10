"""Read sequence JSONs (and scan context) out of an ICON .h5 run file.

An .h5 holds one generated sequence per scan point in ``sequence_json``, plus
the shot/point counts in the root attributes -- everything needed to replay a
run the way the ion experienced it: N shots of a point, then the next point.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any

import h5py


@dataclass
class Run:
    path: str
    experiment_id: str
    job_id: int
    n_points: int
    n_shots: int
    repetitions: int
    scan_mode: str
    sequences: list[tuple[int, dict]] = field(default_factory=list)
    scan_parameters: dict[str, list] = field(default_factory=dict)
    result_channels: dict[str, list] = field(default_factory=dict)

    @property
    def short_name(self) -> str:
        eid = self.experiment_id
        return eid[eid.find("(") + 1:eid.rfind(")")] if "(" in eid else eid


def load_run(path: str, max_points: int | None = None) -> Run:
    with h5py.File(path, "r") as f:
        a = f.attrs
        run = Run(
            path=path,
            experiment_id=str(a.get("experiment_id", "?")),
            job_id=int(a.get("job_id", -1)),
            n_points=int(a.get("number_of_data_points", 1)),
            n_shots=int(a.get("number_of_shots", 1)),
            repetitions=int(a.get("repetitions", 1)),
            scan_mode=str(a.get("scan_mode", "")),
        )
        if "sequence_json" not in f:
            raise ValueError(
                f"{path}: no 'sequence_json' dataset -- this run stored no "
                f"generated sequences (realtime_scan="
                f"{a.get('realtime_scan')}, points={run.n_points}). "
                "Nothing to sonify.")
        ds = f["sequence_json"]
        rows = ds if max_points is None else ds[:max_points]
        for row in rows:
            raw = row["Sequence"]
            if isinstance(raw, bytes):
                raw = raw.decode()
            run.sequences.append((int(row["index"]), json.loads(raw)))

        for name, dset in (("scan_parameters", "scan_parameters"),
                           ("result_channels", "result_channels")):
            if dset in f:
                d = f[dset]
                if d.dtype.names:
                    target = getattr(run, name)
                    for col in d.dtype.names:
                        vals = d[col].tolist()
                        target[col] = [v.decode() if isinstance(v, bytes) else v
                                       for v in _flatten(vals)]
    return run


def _flatten(seq):
    for v in seq:
        if isinstance(v, (list, tuple)):
            yield from _flatten(v)
        else:
            yield v


def list_runs(folder: str) -> list[dict[str, Any]]:
    """Cheap index of a data folder: file -> experiment, points, shots."""
    import glob
    import os

    out = []
    for p in sorted(glob.glob(os.path.join(folder, "*.h5"))):
        try:
            with h5py.File(p, "r") as f:
                a = f.attrs
                out.append({
                    "path": p,
                    "file": os.path.basename(p),
                    "experiment_id": str(a.get("experiment_id", "?")),
                    "points": int(a.get("number_of_data_points", 0)),
                    "shots": int(a.get("number_of_shots", 0)),
                })
        except Exception as e:  # noqa: BLE001
            out.append({"path": p, "file": os.path.basename(p), "error": str(e)})
    return out
