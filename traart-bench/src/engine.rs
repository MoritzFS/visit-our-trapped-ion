//! The audio engine.
//!
//! The callback walks the timeline in *experiment* time and spawns a voice per
//! tone. Playback speed only changes how fast that cursor moves, so the speed
//! dial never touches pitch -- there is no resampling anywhere.

use crate::score::Timeline;
use crate::synth::{pitch_of, BeamCfg, Scale};
#[cfg(not(target_arch = "wasm32"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub const SCOPE_LEN: usize = 1400;
const MAX_VOICES: usize = 96;
const ATTACK_S: f32 = 0.004;
const RELEASE_S: f32 = 0.010;

#[derive(Clone)]
pub struct Params {
    pub playing: bool,
    /// 1 = real time, 1000 = one millisecond of experiment per second of audio.
    pub stretch: f32,
    pub master: f32,
    pub scale: Scale,
    pub root: i32,
    pub beams: Vec<BeamCfg>,
}

pub struct Shared {
    pub params: Params,
    pub params_gen: u64,
    pub timeline: Arc<Timeline>,
    pub timeline_gen: u64,
    // audio thread -> UI
    pub cursor_ns: f64,
    pub scope: Vec<f32>,
    pub scope_ch: Vec<u8>,
    pub scope_head: usize,
    pub levels: Vec<f32>,
    pub voices_used: usize,
}

impl Shared {
    pub fn new(params: Params, timeline: Arc<Timeline>, nch: usize) -> Self {
        Shared {
            params,
            params_gen: 1,
            timeline,
            timeline_gen: 1,
            cursor_ns: 0.0,
            scope: vec![0.0; SCOPE_LEN],
            scope_ch: vec![255; SCOPE_LEN],
            scope_head: 0,
            levels: vec![0.0; nch.max(1)],
            voices_used: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    phase: f32,
    step: f32,
    amp: f32,
    env: f32,
    att: f32,
    rel: f32,
    releasing: bool,
    end_ns: f64,
    ch: usize,
    wave: crate::synth::Wave,
    lg: f32,
    rg: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            active: false,
            phase: 0.0,
            step: 0.0,
            amp: 0.0,
            env: 0.0,
            att: 1.0,
            rel: 1.0,
            releasing: false,
            end_ns: 0.0,
            ch: 0,
            wave: crate::synth::Wave::Sine,
            lg: 0.7,
            rg: 0.7,
        }
    }
}

struct Local {
    sr: f32,
    cursor: f64,
    idx: usize,
    voices: [Voice; MAX_VOICES],
    params: Params,
    params_gen: u64,
    timeline: Arc<Timeline>,
    timeline_gen: u64,
    hop: usize,
    hop_n: usize,
    hop_peak: f32,
    hop_ch: u8,
    hop_ch_amp: f32,
    levels: Vec<f32>,
}

impl Local {
    fn new(sr: f32, shared: &Arc<Mutex<Shared>>) -> Local {
        let (params, timeline, nch) = {
            let s = shared.lock().unwrap();
            (s.params.clone(), s.timeline.clone(), s.levels.len())
        };
        Local {
            sr,
            cursor: 0.0,
            idx: 0,
            voices: [Voice::default(); MAX_VOICES],
            params,
            params_gen: 0,
            timeline,
            timeline_gen: 0,
            hop: ((sr / 240.0) as usize).max(8),
            hop_n: 0,
            hop_peak: 0.0,
            hop_ch: 255,
            hop_ch_amp: 0.0,
            levels: vec![0.0; nch],
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Engine {
    pub shared: Arc<Mutex<Shared>>,
    _stream: cpal::Stream,
    pub sample_rate: f32,
    pub device_name: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl Engine {
    pub fn start(shared: Arc<Mutex<Shared>>) -> Result<Engine, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no output device".to_string())?;
        let device_name = device.name().unwrap_or_else(|_| "output".into());
        let supported = device
            .default_output_config()
            .map_err(|e| format!("no output config: {e}"))?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let sr = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        let mut local = Local::new(sr, &shared);

        let sh = shared.clone();
        let err_fn = |e| eprintln!("audio stream error: {e}");

        macro_rules! build {
            ($t:ty) => {
                device.build_output_stream(
                    &config,
                    move |data: &mut [$t], _: &cpal::OutputCallbackInfo| {
                        let mut buf = vec![0.0f32; data.len()];
                        render(&mut local, &sh, &mut buf, channels);
                        for (o, v) in data.iter_mut().zip(buf.iter()) {
                            *o = cpal::Sample::from_sample(*v);
                        }
                    },
                    err_fn,
                    None,
                )
            };
        }

        let stream = match sample_format {
            cpal::SampleFormat::I16 => build!(i16),
            cpal::SampleFormat::U16 => build!(u16),
            _ => build!(f32),
        }
        .map_err(|e| format!("could not open the audio stream: {e}"))?;
        stream
            .play()
            .map_err(|e| format!("could not start playback: {e}"))?;

        Ok(Engine {
            shared,
            _stream: stream,
            sample_rate: sr,
            device_name,
        })
    }
}

/// The same engine in a browser tab. Web Audio calls `render` from a script
/// processor on the page's own thread, which is all the mutex ever sees there.
/// A page may not make sound before it has been touched, so the context
/// starts suspended and the first click or key press anywhere wakes it.
#[cfg(target_arch = "wasm32")]
pub struct Engine {
    pub shared: Arc<Mutex<Shared>>,
    pub sample_rate: f32,
    pub device_name: String,
    _ctx: web_sys::AudioContext,
    _node: web_sys::ScriptProcessorNode,
    _on_audio: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::AudioProcessingEvent)>,
    _on_touch: wasm_bindgen::closure::Closure<dyn FnMut()>,
}

#[cfg(target_arch = "wasm32")]
impl Engine {
    pub fn start(shared: Arc<Mutex<Shared>>) -> Result<Engine, String> {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        let err = |what: &str| {
            let what = what.to_string();
            move |e: wasm_bindgen::JsValue| format!("{what}: {e:?}")
        };
        let ctx = web_sys::AudioContext::new().map_err(err("no Web Audio"))?;
        let sr = ctx.sample_rate();
        const BLOCK: usize = 2048;
        let node = ctx
            .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
                BLOCK as u32, 1, 2,
            )
            .map_err(err("no script processor"))?;

        let mut local = Local::new(sr, &shared);
        let sh = shared.clone();
        let mut inter = vec![0.0f32; BLOCK * 2];
        let mut left = vec![0.0f32; BLOCK];
        let mut right = vec![0.0f32; BLOCK];
        let on_audio = Closure::wrap(Box::new(move |ev: web_sys::AudioProcessingEvent| {
            let Ok(out) = ev.output_buffer() else { return };
            let n = (out.length() as usize).min(BLOCK);
            render(&mut local, &sh, &mut inter[..n * 2], 2);
            for i in 0..n {
                left[i] = inter[2 * i];
                right[i] = inter[2 * i + 1];
            }
            let _ = out.copy_to_channel(&left[..n], 0);
            let _ = out.copy_to_channel(&right[..n], 1);
        }) as Box<dyn FnMut(web_sys::AudioProcessingEvent)>);
        node.set_onaudioprocess(Some(on_audio.as_ref().unchecked_ref()));
        node.connect_with_audio_node(&ctx.destination())
            .map_err(err("could not connect the output"))?;

        let wake = ctx.clone();
        let on_touch = Closure::wrap(Box::new(move || {
            if wake.state() != web_sys::AudioContextState::Running {
                let _ = wake.resume();
            }
        }) as Box<dyn FnMut()>);
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let opts = web_sys::AddEventListenerOptions::new();
            opts.set_capture(true);
            for name in ["pointerdown", "keydown", "touchend"] {
                let _ = doc.add_event_listener_with_callback_and_add_event_listener_options(
                    name,
                    on_touch.as_ref().unchecked_ref(),
                    &opts,
                );
            }
        }

        Ok(Engine {
            shared,
            sample_rate: sr,
            device_name: "Web Audio".into(),
            _ctx: ctx,
            _node: node,
            _on_audio: on_audio,
            _on_touch: on_touch,
        })
    }
}

fn render(l: &mut Local, shared: &Arc<Mutex<Shared>>, out: &mut [f32], channels: usize) {
    // One short lock per buffer: pick up edits, hand back what the scope draws.
    let mut scope_writes: Vec<(f32, u8)> = Vec::new();
    {
        let mut s = match shared.try_lock() {
            Ok(s) => s,
            Err(_) => {
                for v in out.iter_mut() {
                    *v = 0.0;
                }
                return;
            }
        };
        if s.params_gen != l.params_gen {
            l.params = s.params.clone();
            l.params_gen = s.params_gen;
        }
        if s.timeline_gen != l.timeline_gen {
            l.timeline = s.timeline.clone();
            l.timeline_gen = s.timeline_gen;
            l.cursor = 0.0;
            l.idx = 0;
            l.voices = [Voice::default(); MAX_VOICES];
            if l.levels.len() != s.levels.len() {
                l.levels = vec![0.0; s.levels.len()];
            }
        }
        s.cursor_ns = l.cursor;
    }

    let frames = out.len() / channels.max(1);
    if !l.params.playing {
        for v in out.iter_mut() {
            *v = 0.0;
        }
        for lv in l.levels.iter_mut() {
            *lv *= 0.8;
        }
        push_state(l, shared, &scope_writes);
        return;
    }

    let tl = l.timeline.clone();
    let ns_per_sample = 1.0e9 / (l.sr as f64 * l.params.stretch.max(0.01) as f64);
    let total = tl.total_ns;
    let master = l.params.master;

    for f in 0..frames {
        // advance experiment time
        l.cursor += ns_per_sample;
        if l.cursor >= total {
            l.cursor -= total;
            l.idx = 0;
        }

        // spawn everything that starts at or before the cursor
        while l.idx < tl.notes.len() && tl.notes[l.idx].t0 <= l.cursor {
            let n = &tl.notes[l.idx];
            let cfg = l.params.beams.get(n.ch).cloned();
            if let Some(cfg) = cfg {
                if !cfg.mute {
                    let dur_audio =
                        ((n.t1 - n.t0) * 1e-9 * l.params.stretch as f64).max(0.0005) as f32;
                    for (det, amp) in &n.tones {
                        let freq = pitch_of(&cfg, *det, l.params.scale, l.params.root);
                        let level = amp.powf(cfg.resp.max(0.05)) * cfg.gain;
                        spawn(l, n.ch, freq, level, n.t1, dur_audio, &cfg);
                    }
                }
            }
            l.idx += 1;
        }

        let mut left = 0.0f32;
        let mut right = 0.0f32;
        let mut used = 0usize;
        let mut loudest = (0.0f32, 255u8);
        for v in l.voices.iter_mut() {
            if !v.active {
                continue;
            }
            used += 1;
            if !v.releasing && l.cursor >= v.end_ns {
                v.releasing = true;
            }
            if v.releasing {
                v.env -= v.rel;
                if v.env <= 0.0 {
                    v.active = false;
                    v.env = 0.0;
                    continue;
                }
            } else if v.env < 1.0 {
                v.env = (v.env + v.att).min(1.0);
            }
            let s = v.wave.sample(v.phase) * v.env * v.amp;
            v.phase += v.step;
            if v.phase >= 1.0 {
                v.phase -= 1.0;
            }
            left += s * v.lg;
            right += s * v.rg;
            let a = s.abs();
            if a > loudest.0 {
                loudest = (a, v.ch as u8);
            }
            if let Some(lv) = l.levels.get_mut(v.ch) {
                *lv = lv.max(a);
            }
        }

        let l_out = (left * master * 0.34).clamp(-1.0, 1.0);
        let r_out = (right * master * 0.34).clamp(-1.0, 1.0);
        let base = f * channels;
        for c in 0..channels {
            out[base + c] = if c % 2 == 0 { l_out } else { r_out };
        }

        // scope: one peak column every `hop` frames
        let mono = 0.5 * (l_out + r_out);
        l.hop_peak = l.hop_peak.max(mono.abs());
        if loudest.0 > l.hop_ch_amp {
            l.hop_ch_amp = loudest.0;
            l.hop_ch = loudest.1;
        }
        l.hop_n += 1;
        if l.hop_n >= l.hop {
            scope_writes.push((l.hop_peak, l.hop_ch));
            l.hop_n = 0;
            l.hop_peak = 0.0;
            l.hop_ch_amp = 0.0;
            l.hop_ch = 255;
        }
        for lv in l.levels.iter_mut() {
            *lv *= 0.9995;
        }
        l.voices_used_hint(used);
    }

    push_state(l, shared, &scope_writes);
}

impl Local {
    fn voices_used_hint(&mut self, _n: usize) {}
}

fn spawn(
    l: &mut Local,
    ch: usize,
    freq: f32,
    amp: f32,
    end_ns: f64,
    dur_audio: f32,
    cfg: &BeamCfg,
) {
    let att_t = ATTACK_S.min(dur_audio * 0.4).max(0.0008);
    // The tail rings on past the end of the pulse; without one a 3 us probe at
    // 110 Hz would not complete a single cycle.
    let rel_t = RELEASE_S.min(dur_audio * 0.4).max(cfg.tail);
    let wave = if freq > crate::synth::Wave::PURE_ABOVE {
        crate::synth::Wave::Sine
    } else {
        cfg.wave
    };
    let theta = (cfg.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    let v = Voice {
        active: true,
        phase: 0.0,
        step: freq / l.sr,
        amp,
        env: 0.0,
        att: 1.0 / (att_t * l.sr).max(1.0),
        rel: 1.0 / (rel_t * l.sr).max(1.0),
        releasing: false,
        end_ns,
        ch,
        wave,
        lg: theta.cos(),
        rg: theta.sin(),
    };
    // Take a free slot, else steal the quietest voice.
    let mut steal = 0usize;
    let mut quietest = f32::MAX;
    for (i, slot) in l.voices.iter().enumerate() {
        if !slot.active {
            l.voices[i] = v;
            return;
        }
        let e = slot.env * slot.amp;
        if e < quietest {
            quietest = e;
            steal = i;
        }
    }
    l.voices[steal] = v;
}

fn push_state(l: &mut Local, shared: &Arc<Mutex<Shared>>, writes: &[(f32, u8)]) {
    if let Ok(mut s) = shared.try_lock() {
        s.cursor_ns = l.cursor;
        for (peak, ch) in writes {
            let h = s.scope_head;
            s.scope[h] = *peak;
            s.scope_ch[h] = *ch;
            s.scope_head = (h + 1) % SCOPE_LEN;
        }
        let n = s.levels.len().min(l.levels.len());
        for i in 0..n {
            s.levels[i] = l.levels[i];
        }
        s.voices_used = l.voices.iter().filter(|v| v.active).count();
    }
}

/// Render straight to a buffer with no audio device -- used to check the engine
/// headlessly and to bounce a run to disk (`--render out.wav 20`).
pub fn render_offline(shared: &Arc<Mutex<Shared>>, sr: f32, seconds: f32) -> Vec<f32> {
    let mut local = Local::new(sr, shared);
    let total = (sr * seconds) as usize;
    let block = 512usize;
    let mut out = Vec::with_capacity(total * 2);
    let mut buf = vec![0.0f32; block * 2];
    while out.len() < total * 2 {
        render(&mut local, shared, &mut buf, 2);
        out.extend_from_slice(&buf);
    }
    out.truncate(total * 2);
    out
}

/// Interleaved stereo as a PCM WAV file, 16 or 24 bit. `info` goes into a
/// LIST/INFO chunk (INAM title, IART artist, ICOP copyright, ICMT comment,
/// ISFT software), so the credit travels inside the file and not next to it.
pub fn wav_bytes(samples: &[f32], sr: u32, bits: u16, info: &[(&str, String)]) -> Vec<u8> {
    let bytes = if bits == 24 { 3u32 } else { 2u32 };
    let bits = (bytes * 8) as u16;
    let data_bytes = samples.len() as u32 * bytes;

    let mut list = Vec::new();
    for (id, text) in info {
        if id.len() != 4 || text.is_empty() {
            continue;
        }
        let mut t = text.as_bytes().to_vec();
        t.push(0);
        list.extend_from_slice(id.as_bytes());
        list.extend_from_slice(&(t.len() as u32).to_le_bytes());
        if t.len() % 2 == 1 {
            t.push(0);
        }
        list.extend_from_slice(&t);
    }
    let list_bytes = if list.is_empty() { 0 } else { 12 + list.len() as u32 };

    let mut f = Vec::with_capacity(44 + (list_bytes + data_bytes) as usize);
    f.extend_from_slice(b"RIFF");
    f.extend_from_slice(&(36 + list_bytes + data_bytes).to_le_bytes());
    f.extend_from_slice(b"WAVEfmt ");
    f.extend_from_slice(&16u32.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&2u16.to_le_bytes());
    f.extend_from_slice(&sr.to_le_bytes());
    f.extend_from_slice(&(sr * 2 * bytes).to_le_bytes());
    f.extend_from_slice(&((2 * bytes) as u16).to_le_bytes());
    f.extend_from_slice(&bits.to_le_bytes());
    if !list.is_empty() {
        f.extend_from_slice(b"LIST");
        f.extend_from_slice(&(4 + list.len() as u32).to_le_bytes());
        f.extend_from_slice(b"INFO");
        f.extend_from_slice(&list);
    }
    f.extend_from_slice(b"data");
    f.extend_from_slice(&data_bytes.to_le_bytes());
    for s in samples {
        let x = s.clamp(-1.0, 1.0);
        if bytes == 3 {
            let v = (x * 8_388_607.0) as i32;
            f.extend_from_slice(&v.to_le_bytes()[..3]);
        } else {
            let v = (x * 32767.0) as i16;
            f.extend_from_slice(&v.to_le_bytes());
        }
    }
    f
}
