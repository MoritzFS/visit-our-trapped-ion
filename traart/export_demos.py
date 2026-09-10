"""Build the demo bundle for the browser player."""
import os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from traart.webexport import DemoSpec, export_all

D = "../icon_experiment_data_2026-09-01/"
SPECS = [
    DemoSpec("cool_det", "Cool and Detect",
             "Doppler cool, spin-polarise, look twice",
             D + "2026-09-01 00:14:58.830625.h5", shots=6,
             note="The simplest thing the trap does, and the backbone of every "
                  "other sequence: 1 ms of 397 at -25 MHz, a 20 us sigma pulse, "
                  "then two 200 us detection windows at -10 MHz.",
             tags=["cooling", "detection"]),
    DemoSpec("carriers_729", "729 Carrier Sweep",
             "Bare frequency, 15 kHz across the line",
             D + "2026-09-01 07:52:52.702371.h5", shots=1,
             note="A 500 us probe at 0.8 percent amplitude, stepped in 300 Hz "
                  "increments across the S(-1/2) to D(-5/2) line: 15 kHz end "
                  "to end, and nothing else on the 729 all run. Far too narrow "
                  "and too weak for the standard mapping, so this run carries "
                  "its own -- 1600 semitones per MHz opens the scan out over "
                  "two octaves, and a steep amplitude response lifts the probe "
                  "up next to the cooling light.",
             tags=["729", "scan", "spectroscopy"],
             voice={"729": {"carrier": 220.0, "refMHz": 25.339947,
                            "sens": 1600.0, "gain": 1.6, "resp": 0.25,
                            "tail": 0.10}},
             stretch="729 Pulse", stretch_range_ns=(100_000, 8_000_000)),
    DemoSpec("qec_1mode", "GKP QEC, one mode",
             "Error-correction rounds, 1 to 81",
             D + "2026-09-01 00:54:58.363730.h5", shots=1,
             note="The QEC block is a LoopSequence whose iteration count is the "
                  "scanned parameter, so the correction cycle repeats more and "
                  "more times before readout. That is the lifetime measurement. "
                  "The run was measured at 1 to 81 rounds; the round count is a "
                  "live parameter here, built from the one-round template.",
             tags=["qec", "loops"], loop="QEC"),
    DemoSpec("qec_2modes", "GKP QEC, two modes",
             "Super Gate, both radial modes",
             D + "2026-09-01 09:35:54.243548.h5", shots=1,
             note="Same idea on both radial modes at once (2.543 and 2.805 MHz). "
                  "Sideband cooling alternates between them, and the SDF pulses "
                  "carry a two-tone structure around the carrier. Measured at 1 "
                  "to 8 rounds; the round count is live.",
             tags=["qec", "two-mode"], loop="QEC"),
    DemoSpec("sidebands", "All sidebands, one ion",
             "81-point walk across the motional spectrum",
             D + "2026-09-01 07:59:04.992648.h5", point_stride=2, shots=1,
             note="Standing in for motional coherence until that data is here: "
                  "the 729 probe sweeps across carrier and motional sidebands, "
                  "so the spectrum arrives as a melody.",
             tags=["729", "spectroscopy"],
             voice={"729": {"sens": 8.6, "refMHz": 22.80}}),
]
if __name__ == "__main__":
    data = export_all(SPECS, "web/demos.json")
    for d in data["demos"]:
        n = sum(len(p["pulses"]) for p in d["points"])
        print("%-14s %-26s points=%-4d pulses=%-6d channels=%s"
              % (d["id"], d["title"], len(d["points"]), n,
                 ",".join(c["key"] for c in d["channels"])))
    print("bytes:", os.path.getsize("web/demos.json"))
