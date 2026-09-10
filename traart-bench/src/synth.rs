//! Detuning -> pitch, and the parameters the mixer strips edit.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Triangle,
    Square,
    Saw,
}

impl Wave {
    pub const ALL: [Wave; 4] = [Wave::Sine, Wave::Triangle, Wave::Square, Wave::Saw];
    pub fn name(self) -> &'static str {
        match self {
            Wave::Sine => "sine",
            Wave::Triangle => "triangle",
            Wave::Square => "square",
            Wave::Saw => "saw",
        }
    }
    /// The inverse of `name`, for a tuning read back off disk.
    pub fn from_name(name: &str) -> Option<Wave> {
        Wave::ALL.into_iter().find(|w| w.name() == name)
    }
    /// Above this the richer shapes fold back over Nyquist, so they fall back
    /// to a sine -- cheaper than band-limiting and inaudible as a change.
    pub const PURE_ABOVE: f32 = 1500.0;

    #[inline]
    pub fn sample(self, phase: f32) -> f32 {
        match self {
            Wave::Sine => (phase * std::f32::consts::TAU).sin(),
            Wave::Triangle => 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0,
            // Slightly softened square: less harsh than a hard step at low pitch.
            Wave::Square => {
                let s = if phase < 0.5 { 1.0 } else { -1.0 };
                0.75 * s + 0.25 * (phase * std::f32::consts::TAU).sin()
            }
            Wave::Saw => 2.0 * (phase - 0.5),
        }
    }
}

/// One beam's voice: where its pitch sits and how hard detuning moves it.
#[derive(Clone)]
pub struct BeamCfg {
    pub key: String,
    pub label: String,
    pub carrier_hz: f32,
    /// Detuning that maps exactly onto the carrier.
    pub ref_mhz: f64,
    /// Semitones of pitch per MHz of detuning away from the reference.
    pub sens: f32,
    pub gain: f32,
    /// Exponent on the pulse's DDS amplitude. 1 is linear; below 1 lifts weak
    /// pulses towards the loud ones, which is the only way a 0.8 % probe is
    /// heard next to full-scale cooling light.
    pub resp: f32,
    pub mute: bool,
    pub wave: Wave,
    pub pan: f32,
    /// Seconds the note keeps ringing after the pulse ends. A microsecond 729
    /// probe is far shorter than one cycle of a bass note, so without a tail it
    /// is inaudible; the attack still lands exactly on the pulse.
    pub tail: f32,
    pub color: [u8; 3],
}

/// Defaults chosen so the physics reads: detection sits high, Doppler cooling
/// well below it, the repumpers sit under everything, and the 729 is a bass.
pub fn default_cfg(key: &str, label: &str, fallback_ref: f64) -> BeamCfg {
    let (carrier, ref_mhz, sens, gain, wave, pan, tail, color) = match key {
        "397_pi" => (1396.9, -10.0, 0.32, 0.95, Wave::Sine, -0.22, 0.02, [75, 92, 191]),
        "397_sigma" => (1108.7, 25.0, 0.32, 0.75, Wave::Triangle, 0.28, 0.02, [122, 63, 168]),
        "729" => (110.0, 24.0, 2.4, 1.10, Wave::Triangle, 0.0, 0.22, [176, 34, 26]),
        "729_global" => (146.8, 24.0, 2.4, 0.9, Wave::Triangle, 0.35, 0.22, [150, 60, 30]),
        "854" => (49.0, -10.0, 0.08, 0.75, Wave::Sine, 0.45, 0.05, [122, 34, 48]),
        "866" => (61.7, -10.0, 0.08, 0.75, Wave::Sine, -0.45, 0.05, [94, 32, 80]),
        _ if key.starts_with("tickle") => {
            (82.4, fallback_ref, 1.2, 0.6, Wave::Saw, 0.0, 0.16, [20, 112, 106])
        }
        _ => (440.0, fallback_ref, 1.0, 0.7, Wave::Sine, 0.0, 0.05, [70, 70, 70]),
    };
    BeamCfg {
        key: key.to_string(),
        label: label.to_string(),
        carrier_hz: carrier,
        ref_mhz,
        sens,
        gain,
        resp: 1.0,
        mute: false,
        wave,
        pan,
        tail,
        color,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Off,
    Major,
    Minor,
    Pentatonic,
    WholeTone,
    Octatonic,
    HarmonicMinor,
}

impl Scale {
    pub const ALL: [Scale; 7] = [
        Scale::Off,
        Scale::Major,
        Scale::Minor,
        Scale::Pentatonic,
        Scale::WholeTone,
        Scale::Octatonic,
        Scale::HarmonicMinor,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Scale::Off => "continuous",
            Scale::Major => "major",
            Scale::Minor => "natural minor",
            Scale::Pentatonic => "minor pentatonic",
            Scale::WholeTone => "whole tone",
            Scale::Octatonic => "octatonic",
            Scale::HarmonicMinor => "harmonic minor",
        }
    }
    /// The inverse of `name`, for a tuning read back off disk.
    pub fn from_name(name: &str) -> Option<Scale> {
        Scale::ALL.into_iter().find(|s| s.name() == name)
    }
    fn degrees(self) -> &'static [i32] {
        match self {
            Scale::Off => &[],
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Pentatonic => &[0, 3, 5, 7, 10],
            Scale::WholeTone => &[0, 2, 4, 6, 8, 10],
            Scale::Octatonic => &[0, 2, 3, 5, 6, 8, 9, 11],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
        }
    }
}

pub const NOTE_NAMES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

/// A0 = 27.5 Hz is the anchor; `root` is a semitone offset from it.
fn anchor(root: i32) -> f32 {
    27.5 * 2f32.powf(root as f32 / 12.0)
}

pub fn quantise(freq: f32, scale: Scale, root: i32) -> f32 {
    let degs = scale.degrees();
    if degs.is_empty() {
        return freq;
    }
    let a = anchor(root);
    let n = 12.0 * (freq / a).log2();
    let oct = (n / 12.0).floor();
    let pc = n - oct * 12.0;
    let mut best = degs[0] as f32;
    let mut bestd = f32::MAX;
    for d in degs.iter().copied().chain(std::iter::once(12)) {
        let dd = (pc - d as f32).abs();
        if dd < bestd {
            bestd = dd;
            best = d as f32;
        }
    }
    a * 2f32.powf((oct * 12.0 + best) / 12.0)
}

/// Semitones a run's whole detuning span is opened out to by the `fit` button.
pub const FIT_SEMITONES: f32 = 24.0;

pub const F_MIN: f32 = 24.0;
pub const F_MAX: f32 = 5000.0;

pub fn pitch_of(cfg: &BeamCfg, detuning_hz: f64, scale: Scale, root: i32) -> f32 {
    let st = cfg.sens * ((detuning_hz / 1e6 - cfg.ref_mhz) as f32);
    let f = cfg.carrier_hz * 2f32.powf(st / 12.0);
    quantise(f, scale, root).clamp(F_MIN, F_MAX)
}
