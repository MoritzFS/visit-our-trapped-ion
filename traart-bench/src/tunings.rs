//! Saving the panel, so a tuning can be found again.
//!
//! A tuning is everything the ear was set to and nothing the data decides:
//! the speed, the master, the scale, the run parameters, and every beam
//! strip. Runs, scores and timelines are not in here -- a tuning names the
//! run it was made on and is otherwise just numbers, which is what lets one
//! be recalled onto another run that has the same beams.
//!
//! The file is plain JSON, one array, in the usual place for a small app's
//! settings; `--tunings <file>` puts it somewhere else, which is how a set
//! gets shared and how the selftest keeps its hands off the real one.

use crate::engine::Params;
use crate::score::Edit;
use crate::synth::{Scale, Wave};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One beam strip, by key so it can be found again on another run.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct BeamState {
    pub key: String,
    pub carrier: f32,
    #[serde(rename = "refMHz")]
    pub ref_mhz: f64,
    pub sens: f32,
    pub gain: f32,
    pub resp: f32,
    pub tail: f32,
    pub pan: f32,
    pub mute: bool,
    pub wave: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Tuning {
    pub name: String,
    /// The run it was made on, by id, and its title for the list.
    pub demo: String,
    #[serde(default)]
    pub run: String,
    /// Unix seconds, so the list can say how old it is without a calendar.
    #[serde(default)]
    pub saved: u64,
    pub stretch: f32,
    pub master: f32,
    pub scale: String,
    pub root: i32,
    #[serde(default = "default_window")]
    pub window_s: f32,
    // the run parameters, when the run has any
    #[serde(default)]
    pub loop_from: u32,
    #[serde(default)]
    pub loop_to: u32,
    #[serde(default)]
    pub loop_points: u32,
    #[serde(default)]
    pub probe_ns: Option<f64>,
    pub beams: Vec<BeamState>,
}

fn default_window() -> f32 {
    6.0
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Tuning {
    /// Take the panel as it stands.
    pub fn capture(
        name: &str,
        demo_id: &str,
        run: &str,
        p: &Params,
        e: &Edit,
        window_s: f32,
    ) -> Tuning {
        Tuning {
            name: name.to_string(),
            demo: demo_id.to_string(),
            run: run.to_string(),
            saved: now_s(),
            stretch: p.stretch,
            master: p.master,
            scale: p.scale.name().to_string(),
            root: p.root,
            window_s,
            loop_from: e.loop_from,
            loop_to: e.loop_to,
            loop_points: e.loop_points,
            probe_ns: e.stretch_ns,
            beams: p
                .beams
                .iter()
                .map(|b| BeamState {
                    key: b.key.clone(),
                    carrier: b.carrier_hz,
                    ref_mhz: b.ref_mhz,
                    sens: b.sens,
                    gain: b.gain,
                    resp: b.resp,
                    tail: b.tail,
                    pan: b.pan,
                    mute: b.mute,
                    wave: b.wave.name().to_string(),
                })
                .collect(),
        }
    }

    /// Put it back. Beams are matched by key: one this run does not have is
    /// dropped, one the tuning does not mention keeps the run's own setting,
    /// so a tuning made on a run with a 729 still lands on another that has
    /// one. Everything is clamped to the range its control offers, because a
    /// hand-edited file should not be able to put a knob past its stop.
    pub fn apply_params(&self, p: &mut Params) -> usize {
        p.stretch = self.stretch.clamp(1.0, 5000.0);
        p.master = self.master.clamp(0.0, 1.4);
        p.scale = Scale::from_name(&self.scale).unwrap_or(p.scale);
        p.root = self.root.rem_euclid(12);
        let mut hit = 0usize;
        for b in p.beams.iter_mut() {
            let Some(s) = self.beams.iter().find(|s| s.key == b.key) else {
                continue;
            };
            hit += 1;
            b.carrier_hz = s.carrier.clamp(24.0, 4000.0);
            b.ref_mhz = s.ref_mhz;
            b.sens = s.sens.clamp(0.01, 5000.0);
            b.gain = s.gain.clamp(0.0, 4.0);
            b.resp = s.resp.clamp(0.1, 1.0);
            b.tail = s.tail.clamp(0.0, 1.2);
            b.pan = s.pan.clamp(-1.0, 1.0);
            b.mute = s.mute;
            b.wave = Wave::from_name(&s.wave).unwrap_or(b.wave);
        }
        hit
    }

    /// The run parameters, if this tuning carries any and the run wants them.
    /// A tuning saved on a run without a loop leaves the loop alone.
    pub fn apply_edit(&self, e: &mut Edit) {
        if self.loop_from > 0 && self.loop_to > 0 && self.loop_points > 0 {
            e.loop_from = self.loop_from.max(1);
            e.loop_to = self.loop_to.max(e.loop_from);
            e.loop_points = self.loop_points.clamp(1, 60);
        }
        if self.probe_ns.is_some() && e.stretch_ns.is_some() {
            e.stretch_ns = self.probe_ns;
        }
    }

    /// "just now", "14 m", "3 h", "6 d" -- old enough and it stops mattering.
    pub fn age(&self) -> String {
        if self.saved == 0 {
            return String::new();
        }
        let d = now_s().saturating_sub(self.saved);
        match d {
            0..=59 => "just now".into(),
            60..=3599 => format!("{} m", d / 60),
            3600..=86399 => format!("{} h", d / 3600),
            _ => format!("{} d", d / 86400),
        }
    }

    /// Same settings, whatever it is called and whenever it was saved.
    pub fn same_settings(&self, other: &Tuning) -> bool {
        let strip = |t: &Tuning| {
            let mut t = t.clone();
            t.name.clear();
            t.run.clear();
            t.saved = 0;
            t
        };
        strip(self) == strip(other)
    }
}

/// The file: an array of tunings, newest last.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    pub tunings: Vec<Tuning>,
}

impl Book {
    pub fn load(path: &Path) -> Book {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// Save under this name, replacing a tuning that already has it so that
    /// saving twice does not leave two rows called the same thing.
    pub fn put(&mut self, t: Tuning) -> (usize, bool) {
        match self
            .tunings
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(&t.name))
        {
            Some(i) => {
                self.tunings[i] = t;
                (i, true)
            }
            None => {
                self.tunings.push(t);
                (self.tunings.len() - 1, false)
            }
        }
    }

    /// A name that is not taken yet: "729 bass", "729 bass 2", ...
    pub fn free_name(&self, stem: &str) -> String {
        let taken = |n: &str| self.tunings.iter().any(|t| t.name.eq_ignore_ascii_case(n));
        if !taken(stem) {
            return stem.to_string();
        }
        (2..)
            .map(|i| format!("{stem} {i}"))
            .find(|n| !taken(n))
            .unwrap_or_else(|| stem.to_string())
    }
}

/// Where the tunings live when `--tunings` does not say.
pub fn default_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/traart-bench"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|a| a.join("traart-bench"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".config")))
            .map(|c| c.join("traart-bench"))
    };
    dir.unwrap_or_else(|| PathBuf::from(".")).join("tunings.json")
}

/// The path shown in the panel, with the home directory folded back to `~`.
pub fn pretty(path: &Path) -> String {
    let s = path.display().to_string();
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s,
    }
}
