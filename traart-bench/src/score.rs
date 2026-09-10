//! The score: what the parser exported, and the flat timeline we play from.
//!
//! Nothing here knows about audio. A `Timeline` is experiment nanoseconds and
//! optical detunings; turning that into pitch is the synth's job, which is why
//! slowing playback down never moves a note.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct Bundle {
    pub demos: Vec<Demo>,
}

#[derive(Deserialize)]
pub struct Demo {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub experiment: String,
    pub job: i64,
    pub file: String,
    #[serde(rename = "shotsMeasured", default)]
    pub shots_measured: u32,
    #[serde(default = "one")]
    pub shots: u32,
    #[serde(default)]
    pub scan: String,
    pub channels: Vec<Channel>,
    pub points: Vec<Point>,
    #[serde(default)]
    pub blocks: Vec<String>,
    /// Per-run voice overrides, keyed by beam. A run whose whole structure is
    /// 15 kHz wide and 0.8 % deep cannot be heard through the global defaults.
    #[serde(default)]
    pub voice: HashMap<String, VoiceOverride>,
    /// A LoopSequence whose round count is a live parameter. The exported
    /// point is the one-round template; `Timeline::build` makes the scan.
    #[serde(default, rename = "loop")]
    pub loop_block: Option<LoopBlock>,
    /// A block whose length is a live parameter (a probe pulse).
    #[serde(default)]
    pub stretch: Option<StretchBlock>,
}

#[derive(Deserialize, Clone)]
pub struct LoopBlock {
    pub name: String,
    pub t0: f64,
    pub t1: f64,
    /// How far a channel advances through one round.
    pub period: f64,
    /// Channels (by export index, as JSON keys) that advance by a different
    /// amount -- a shaped pulse on a 4 ns grid drifts a few ns per round.
    #[serde(rename = "channelPeriods", default)]
    pub channel_periods: HashMap<String, f64>,
    #[serde(default)]
    pub measured: Vec<u32>,
    #[serde(default = "loop_max")]
    pub max: u32,
}

fn loop_max() -> u32 {
    400
}

#[derive(Deserialize, Clone)]
pub struct StretchBlock {
    pub name: String,
    pub t0: f64,
    pub t1: f64,
    #[serde(rename = "minNs")]
    pub min_ns: f64,
    #[serde(rename = "maxNs")]
    pub max_ns: f64,
}

/// The live parameters a run exposes.
#[derive(Clone, PartialEq)]
pub struct Edit {
    pub loop_from: u32,
    pub loop_to: u32,
    pub loop_points: u32,
    pub stretch_ns: Option<f64>,
}

impl Edit {
    pub fn for_demo(d: &Demo) -> Edit {
        let (from, to, points) = match &d.loop_block {
            Some(l) if !l.measured.is_empty() => (
                *l.measured.iter().min().unwrap(),
                *l.measured.iter().max().unwrap(),
                l.measured.len().min(9) as u32,   // a listenable loop by default
            ),
            Some(_) => (1, 8, 8),
            None => (1, 1, 1),
        };
        Edit {
            loop_from: from,
            loop_to: to,
            loop_points: points,
            stretch_ns: d.stretch.as_ref().map(|s| s.t1 - s.t0),
        }
    }

    pub fn loop_counts(&self) -> Vec<u32> {
        let n = self.loop_points.max(1);
        let (a, b) = (self.loop_from.max(1) as f64, self.loop_to.max(1) as f64);
        let mut out: Vec<u32> = (0..n)
            .map(|i| {
                let f = if n == 1 { 0.0 } else { i as f64 / (n - 1) as f64 };
                (a + (b - a) * f).round() as u32
            })
            .collect();
        out.dedup();
        out
    }
}

#[derive(Deserialize, Clone, Default)]
pub struct VoiceOverride {
    pub carrier: Option<f32>,
    #[serde(rename = "refMHz")]
    pub ref_mhz: Option<f64>,
    pub sens: Option<f32>,
    pub gain: Option<f32>,
    pub resp: Option<f32>,
    pub tail: Option<f32>,
    pub pan: Option<f32>,
}

fn one() -> u32 {
    1
}

#[derive(Deserialize, Clone)]
pub struct Channel {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub wavelength: Option<f64>,
    #[serde(default)]
    pub role: String,
    #[serde(rename = "refMHz", default)]
    pub ref_mhz: f64,
    #[serde(rename = "spanMHz", default)]
    pub span_mhz: f64,
}

#[derive(Deserialize)]
pub struct Point {
    pub label: String,
    #[serde(rename = "durNs")]
    pub dur_ns: f64,
    pub pulses: Vec<RawPulse>,
    #[serde(default)]
    pub gates: Vec<f64>,
}

/// `[channel, t0_ns, t1_ns, [[detuning_hz, amp], ...], block]`
#[derive(Deserialize)]
pub struct RawPulse(
    pub usize,
    pub f64,
    pub f64,
    pub Vec<(f64, f32)>,
    #[serde(default)] pub usize,
);

/// One sounding pulse, placed on the flattened timeline.
pub struct Note {
    pub t0: f64,
    pub t1: f64,
    pub ch: usize,
    pub block: usize,
    pub point: usize,
    pub tones: Vec<(f64, f32)>,
}

/// One scan point before it is placed on the timeline.
#[derive(Clone)]
struct Local {
    pulses: Vec<(usize, f64, f64, Vec<(f64, f32)>, usize)>,
    gates: Vec<f64>,
    dur: f64,
    label: String,
}

enum Kind<'a> {
    Repeat { k: u32, period: f64, per_channel: &'a HashMap<String, f64> },
    Stretch { new_len: f64 },
}

/// Repeat or stretch the block `[a, b)` of one point.
///
/// Mirrors `traart/regions.py`, which is checked pulse-for-pulse against the
/// real multi-round sequences. Pulses are classified by their *start*: a
/// pulse is a state written at its start and held until the next write on
/// its channel, and channels reach the block boundary a few microseconds
/// apart, so a pulse's span is not a safe criterion.
fn edit_point(p: &Local, a: f64, b: f64, kind: &Kind) -> Local {
    let period_of = |ch: Option<usize>| -> f64 {
        match kind {
            Kind::Repeat { period, per_channel, .. } => ch
                .and_then(|c| per_channel.get(&c.to_string()).copied())
                .unwrap_or(*period),
            Kind::Stretch { .. } => b - a,
        }
    };
    let delta_of = |ch: Option<usize>| -> f64 {
        match kind {
            Kind::Repeat { k, .. } => (*k as f64 - 1.0) * period_of(ch),
            Kind::Stretch { new_len } => new_len - (b - a),
        }
    };
    let end = p.dur;
    let mut new_end = end + delta_of(None);
    for pl in &p.pulses {
        new_end = new_end.max(end + delta_of(Some(pl.0)));
    }

    let mut first_write: HashMap<usize, f64> = HashMap::new();
    for pl in &p.pulses {
        if pl.1 >= a && pl.1 < b {
            let e = first_write.entry(pl.0).or_insert(pl.1);
            if pl.1 < *e {
                *e = pl.1;
            }
        }
    }

    let copies = |t0: f64, t1: f64, ch: usize| -> Vec<(f64, f64)> {
        match kind {
            Kind::Repeat { k, .. } => {
                let per = period_of(Some(ch));
                (0..*k).map(|j| (t0 + j as f64 * per, t1 + j as f64 * per)).collect()
            }
            Kind::Stretch { new_len } => {
                let s = new_len / (b - a).max(1.0);
                let n0 = a + (t0 - a) * s;
                let n1 = if t1 > b { t1 + (new_len - (b - a)) } else { a + (t1 - a) * s };
                vec![(n0, n1.max(n0))]
            }
        }
    };

    let mut out = Local { pulses: Vec::new(), gates: Vec::new(), dur: new_end, label: p.label.clone() };
    for pl in &p.pulses {
        let (ch, t0, t1) = (pl.0, pl.1, pl.2);
        let d = delta_of(Some(ch));
        let trailing = t1 >= end;
        if t0 < a {
            if t1 <= b {
                out.pulses.push(pl.clone());
            } else {
                let e = if trailing { new_end } else { t1 + d };
                out.pulses.push((ch, t0, e, pl.3.clone(), pl.4));
            }
        } else if t0 >= b {
            let e = if trailing { new_end } else { t1 + d };
            out.pulses.push((ch, t0 + d, e, pl.3.clone(), pl.4));
        } else {
            let cs = copies(t0, t1, ch);
            let tail = t1 > b;
            let fw = first_write.get(&ch).copied();
            for (j, (c0, mut c1)) in cs.iter().copied().enumerate() {
                if tail && j + 1 < cs.len() {
                    if let Some(fw) = fw {
                        // not the last pass: the next pass rewrites this channel
                        c1 = cs[j + 1].0 - (t0 - fw);
                    }
                } else if trailing {
                    c1 = new_end;
                }
                out.pulses.push((ch, c0, c1.max(c0), pl.3.clone(), pl.4));
            }
        }
    }
    let dn = delta_of(None);
    for g in &p.gates {
        if *g < a {
            out.gates.push(*g);
        } else if *g >= b {
            out.gates.push(*g + dn);
        } else {
            for (c0, _) in copies(*g, *g, usize::MAX) {
                out.gates.push(c0);
            }
        }
    }
    out
}

/// Every shot of every scan point, laid end to end the way the run happened.
pub struct Timeline {
    pub notes: Vec<Note>,
    pub gates: Vec<f64>,
    pub total_ns: f64,
    /// (start_ns, end_ns, point index) for the scan-point readout.
    pub segments: Vec<(f64, f64, usize)>,
    /// One label per (possibly generated) scan point.
    pub labels: Vec<String>,
    /// The same points in the exported form, so a headset served by the
    /// bench flattens exactly what the bench is playing, live edits included.
    pub points_json: serde_json::Value,
}

impl Timeline {
    pub fn build(demo: &Demo, shots: u32, edit: &Edit) -> Arc<Timeline> {
        // 1. the exported points, as local data
        let mut locals: Vec<Local> = demo
            .points
            .iter()
            .map(|pt| Local {
                pulses: pt.pulses.iter().map(|p| (p.0, p.1, p.2, p.3.clone(), p.4)).collect(),
                gates: pt.gates.clone(),
                dur: pt.dur_ns,
                label: pt.label.clone(),
            })
            .collect();

        // 2. live edits: stretch a block, then make a scan out of a loop
        if let (Some(st), Some(len)) = (&demo.stretch, edit.stretch_ns) {
            let len = len.clamp(st.min_ns, st.max_ns);
            let kind = Kind::Stretch { new_len: len };
            locals = locals.iter().map(|l| edit_point(l, st.t0, st.t1, &kind)).collect();
        }
        if let Some(lp) = &demo.loop_block {
            if let Some(template) = locals.first().cloned() {
                let scan = if demo.scan.is_empty() { "rounds".to_string() } else { demo.scan.clone() };
                locals = edit
                    .loop_counts()
                    .into_iter()
                    .map(|k| {
                        let kind = Kind::Repeat {
                            k: k.clamp(1, lp.max),
                            period: lp.period,
                            per_channel: &lp.channel_periods,
                        };
                        let mut l = edit_point(&template, lp.t0, lp.t1, &kind);
                        l.label = format!("{scan} = {k}");
                        l
                    })
                    .collect();
            }
        }

        // 3. lay shots and points end to end
        let mut notes = Vec::new();
        let mut gates = Vec::new();
        let mut segments = Vec::new();
        let mut labels = Vec::new();
        let mut cursor = 0.0f64;
        for (pi, point) in locals.iter().enumerate() {
            labels.push(point.label.clone());
            for _ in 0..shots.max(1) {
                for p in &point.pulses {
                    notes.push(Note {
                        t0: cursor + p.1,
                        t1: cursor + p.2,
                        ch: p.0,
                        block: p.4,
                        point: pi,
                        tones: p.3.clone(),
                    });
                }
                for g in &point.gates {
                    gates.push(cursor + g);
                }
                segments.push((cursor, cursor + point.dur, pi));
                cursor += point.dur;
            }
        }
        notes.sort_by(|a, b| a.t0.partial_cmp(&b.t0).unwrap());
        let points_json = serde_json::Value::Array(
            locals
                .iter()
                .map(|l| {
                    serde_json::json!({
                        "label": l.label,
                        "durNs": l.dur,
                        "pulses": l.pulses.iter().map(|p| serde_json::json!([
                            p.0, p.1, p.2,
                            p.3.iter().map(|t| serde_json::json!([t.0, t.1])).collect::<Vec<_>>(),
                            p.4
                        ])).collect::<Vec<_>>(),
                        "gates": l.gates,
                    })
                })
                .collect(),
        );
        Arc::new(Timeline {
            notes,
            gates,
            total_ns: cursor.max(1.0),
            segments,
            labels,
            points_json,
        })
    }

    pub fn point_at(&self, t: f64) -> usize {
        match self
            .segments
            .binary_search_by(|s| s.0.partial_cmp(&t).unwrap())
        {
            Ok(i) => self.segments[i].2,
            Err(0) => 0,
            Err(i) => self.segments[i - 1].2,
        }
    }

    /// First note whose start is at or after `t` -- the scan window's left edge.
    pub fn first_from(&self, t: f64) -> usize {
        let mut lo = 0usize;
        let mut hi = self.notes.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.notes[mid].t0 < t {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
}
