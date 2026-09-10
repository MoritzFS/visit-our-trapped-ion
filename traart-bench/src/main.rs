//! Audio Bench -- listen to an ion trap sequence.
//!
//! Reads the score exported by the Python side (`traart`), plays it as one
//! voice per beam, and draws what is coming. The speed control moves the
//! playback cursor through experiment time; pitch is derived from optical
//! detuning, so slowing down never transposes anything.

mod engine;
mod retro;
mod score;
mod serve;
mod synth;
mod tunings;

use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use engine::{Engine, Params, Shared, SCOPE_LEN};
use score::{Bundle, Edit, Timeline};
use std::sync::{Arc, Mutex};
use synth::{default_cfg, BeamCfg, Scale, Wave, NOTE_NAMES};

const EMBEDDED: &str = include_str!("../demos.json");

fn main() -> eframe::Result<()> {
    let argv: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        argv.iter().position(|a| a == name).and_then(|i| argv.get(i + 1).cloned())
    };
    // A bundle path is the first positional argument; flags never count as one.
    let positional = argv.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
    let text = flag("--data")
        .or(positional)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .or_else(|| std::fs::read_to_string("demos.json").ok())
        .unwrap_or_else(|| EMBEDDED.to_string());
    let bundle: Bundle = match serde_json::from_str(&text) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("could not read the score bundle: {e}");
            std::process::exit(1);
        }
    };
    let raw: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    // --serve <dir>: also hand out the VR page from <dir> and the playback clock
    let serve_dir = flag("--serve").map(std::path::PathBuf::from);
    let serve_port: u16 = flag("--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(serve::DEFAULT_PORT);
    // --tunings <file>: where saved tunings live; --tuning <name>: open on one
    let tunings_path = flag("--tunings")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(tunings::default_path);
    let open_tuning = flag("--tuning");

    // Headless bounce: traart-bench --render out.wav 20 [demo-id] [--stretch N]
    if let Some(i) = argv.iter().position(|a| a == "--render") {
        let out = argv.get(i + 1).cloned().unwrap_or_else(|| "out.wav".into());
        let secs: f32 = argv.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(20.0);
        let want = argv.get(i + 3).cloned().filter(|w| !w.starts_with("--"));
        let stretch: f32 = argv
            .iter()
            .position(|a| a == "--stretch")
            .and_then(|k| argv.get(k + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000.0);
        let d = want
            .and_then(|w| bundle.demos.iter().position(|dd| dd.id == w))
            .unwrap_or(0);
        let mut beams = beams_for(&bundle, d);
        if let Some(k) = argv.iter().position(|a| a == "--solo") {
            if let Some(key) = argv.get(k + 1) {
                for b in beams.iter_mut() {
                    b.mute = &b.key != key;
                }
            }
        }
        let edit = Edit::for_demo(&bundle.demos[d]);
        let tl = Timeline::build(&bundle.demos[d], bundle.demos[d].shots, &edit);
        let params = Params {
            playing: true,
            stretch,
            master: 0.9,
            scale: Scale::Off,
            root: 3,
            beams,
        };
        let n = params.beams.len();
        let shared = Arc::new(Mutex::new(Shared::new(params, tl, n)));
        let pcm = engine::render_offline(&shared, 44100.0, secs);
        engine::write_wav(&out, &pcm, 44100).expect("write wav");
        println!("{out}: {secs:.1} s of {} at x{stretch:.0}", bundle.demos[d].title);
        return Ok(());
    }

    // Headless check of the live edits: --stats qec_1mode 11  /  --stats carriers_729 2.0
    if let Some(i) = argv.iter().position(|a| a == "--stats") {
        let id = argv.get(i + 1).cloned().unwrap_or_default();
        let d = bundle.demos.iter().position(|dd| dd.id == id).unwrap_or(0);
        let demo = &bundle.demos[d];
        let mut edit = Edit::for_demo(demo);
        if let Some(v) = argv.get(i + 2).and_then(|s| s.parse::<f64>().ok()) {
            if demo.loop_block.is_some() {
                edit.loop_from = v as u32;
                edit.loop_to = v as u32;
                edit.loop_points = 1;
            } else if demo.stretch.is_some() {
                edit.stretch_ns = Some(v * 1e6);
            }
        }
        let tl = Timeline::build(demo, 1, &edit);
        let mut per_ch: Vec<(String, usize)> = demo
            .channels
            .iter()
            .enumerate()
            .map(|(k, c)| (c.key.clone(), tl.notes.iter().filter(|n| n.ch == k).count()))
            .collect();
        per_ch.sort();
        println!(
            "{}: {} points, {} notes, {:.0} ns, gates {}, per channel {:?}, labels {:?}",
            demo.title,
            tl.labels.len(),
            tl.notes.len(),
            tl.total_ns,
            tl.gates.len(),
            per_ch,
            &tl.labels[..tl.labels.len().min(4)]
        );
        return Ok(());
    }

    if std::env::args().any(|a| a == "--selftest") {
        let ctx = egui::Context::default();
        let scratch = std::env::temp_dir().join("traart-bench-selftest-tunings.json");
        let _ = std::fs::remove_file(&scratch);
        let mut app = App::build(&ctx, bundle, raw, None, serve_port, scratch.clone(), None);
        // save, recall, delete -- the panel's three verbs, off-screen
        app.tuning_name = "selftest".into();
        app.tuning_save();
        let n = app.book.tunings.len();
        app.params.stretch = 123.0;
        app.tuning_recall(0);
        let back = app.params.stretch;
        app.tuning_delete();
        println!("tunings: saved {n}, recalled stretch {back}, {} left", app.book.tunings.len());
        let _ = std::fs::remove_file(&scratch);
        app.params.playing = true;
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1420.0, 900.0),
        ));
        for frame in 0..12 {
            if frame == 4 {
                app.load_demo(2.min(app.bundle.demos.len().saturating_sub(1)));
            }
            if frame == 7 {
                app.blind = true;
            }
            let out = ctx.run(input.clone(), |ctx| app.ui_all(ctx));
            let shapes = out.shapes.len();
            if frame == 11 {
                println!("selftest ok: {shapes} shapes on the last frame");
            }
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1420.0, 900.0])
            .with_min_inner_size([1040.0, 700.0])
            .with_title("Audio Bench"),
        ..Default::default()
    };
    eframe::run_native(
        "Audio Bench",
        options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(cc, bundle, raw, serve_dir, serve_port, tunings_path, open_tuning)))
        }),
    )
}

struct App {
    bundle: Bundle,
    demo: usize,
    shared: Arc<Mutex<Shared>>,
    engine: Option<Engine>,
    audio_err: Option<String>,
    params: Params,
    edit: Edit,
    /// What a headset served by the bench sees, and what it asks back.
    link: Arc<Mutex<serve::Link>>,
    serving: Option<serve::Serving>,
    serve_err: Option<String>,
    /// Saved tunings: the file, the list, the name being typed, which entry
    /// the panel currently matches, and the one-line readout under the list.
    book: tunings::Book,
    book_path: std::path::PathBuf,
    tuning_name: String,
    tuning_current: Option<usize>,
    tuning_note: String,
    applying_tuning: bool,
    blind: bool,
    window_s: f32,
    cursor_ns: f64,
    levels: Vec<f32>,
    scope: Vec<f32>,
    scope_ch: Vec<u8>,
    scope_head: usize,
    font_note: bool,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        bundle: Bundle,
        raw: serde_json::Value,
        serve_dir: Option<std::path::PathBuf>,
        serve_port: u16,
        tunings_path: std::path::PathBuf,
        open_tuning: Option<String>,
    ) -> Self {
        App::build(&cc.egui_ctx, bundle, raw, serve_dir, serve_port, tunings_path, open_tuning)
    }

    fn build(
        ctx: &egui::Context,
        bundle: Bundle,
        raw: serde_json::Value,
        serve_dir: Option<std::path::PathBuf>,
        serve_port: u16,
        tunings_path: std::path::PathBuf,
        open_tuning: Option<String>,
    ) -> Self {
        let font_note = retro::install(ctx).is_some();

        let demo = 0usize;
        let beams = beams_for(&bundle, demo);
        let edit = Edit::for_demo(&bundle.demos[demo]);
        let timeline = Timeline::build(&bundle.demos[demo], bundle.demos[demo].shots, &edit);
        let params = Params {
            playing: false,
            stretch: 1000.0,
            master: 0.85,
            scale: Scale::Off,
            root: 3, // C
            beams,
        };
        let shared = Arc::new(Mutex::new(Shared::new(
            params.clone(),
            timeline,
            params.beams.len(),
        )));
        let (engine, audio_err) = match Engine::start(shared.clone()) {
            Ok(e) => (Some(e), None),
            Err(e) => (None, Some(e)),
        };
        let link = Arc::new(Mutex::new(serve::Link::new(Arc::new(raw))));
        let (serving, serve_err) = match serve_dir {
            Some(dir) => match serve::spawn(shared.clone(), link.clone(), dir, serve_port) {
                Ok(sv) => {
                    println!("serving the VR page and the clock:");
                    for u in &sv.urls {
                        println!("  {u}/trap_vr.html");
                    }
                    (Some(sv), None)
                }
                Err(e) => {
                    eprintln!("server: {e}");
                    (None, Some(e))
                }
            },
            None => (None, None),
        };
        let start = std::env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|w| w[0] == "--start")
            .and_then(|w| bundle.demos.iter().position(|d| d.id == w[1]));
        let mut app = App {
            bundle,
            demo,
            shared,
            engine,
            audio_err,
            params,
            edit,
            link,
            serving,
            serve_err,
            book: tunings::Book::load(&tunings_path),
            book_path: tunings_path,
            tuning_name: String::new(),
            tuning_current: None,
            tuning_note: String::new(),
            applying_tuning: false,
            blind: false,
            window_s: 6.0,
            cursor_ns: 0.0,
            levels: Vec::new(),
            scope: vec![0.0; SCOPE_LEN],
            scope_ch: vec![255; SCOPE_LEN],
            scope_head: 0,
            font_note,
        };
        if let Some(i) = start {
            app.load_demo(i);
        }
        if let Some(name) = open_tuning {
            match app.book.tunings.iter().position(|t| t.name.eq_ignore_ascii_case(&name)) {
                Some(i) => app.tuning_recall(i),
                None => app.tuning_note = format!("no tuning called {name:?}"),
            }
        }
        app.sync_link();
        app
    }

    // ── tunings: save, recall, delete ────────────────────────────────────
    fn tuning_save(&mut self) {
        let d = &self.bundle.demos[self.demo];
        let name = self.tuning_name.trim().to_string();
        let name = if name.is_empty() { self.book.free_name(&d.title) } else { name };
        let t = tunings::Tuning::capture(&name, &d.id, &d.title, &self.params, &self.edit, self.window_s);
        let (i, replaced) = self.book.put(t);
        match self.book.save(&self.book_path) {
            Ok(()) => {
                self.tuning_current = Some(i);
                self.tuning_name = name.clone();
                self.tuning_note = if replaced { format!("replaced {name:?}") } else { format!("saved {name:?}") };
            }
            Err(e) => self.tuning_note = format!("could not save: {e}"),
        }
    }

    fn tuning_recall(&mut self, i: usize) {
        let Some(t) = self.book.tunings.get(i).cloned() else { return };
        self.applying_tuning = true;
        if let Some(di) = self.bundle.demos.iter().position(|d| d.id == t.demo) {
            if di != self.demo {
                self.load_demo(di);
            }
        }
        let hit = t.apply_params(&mut self.params);
        t.apply_edit(&mut self.edit);
        self.window_s = t.window_s.clamp(1.0, 20.0);
        self.rebuild_timeline();
        self.push_params();
        self.applying_tuning = false;
        self.tuning_current = Some(i);
        self.tuning_name = t.name.clone();
        let total = self.params.beams.len();
        let run_note = if self.bundle.demos[self.demo].id == t.demo {
            String::new()
        } else {
            format!(", made on {}", if t.run.is_empty() { t.demo.clone() } else { t.run.clone() })
        };
        self.tuning_note = format!("recalled {:?}: {hit} of {total} beams set{run_note}", t.name);
    }

    fn tuning_delete(&mut self) {
        let Some(i) = self.tuning_current else { return };
        if i >= self.book.tunings.len() {
            self.tuning_current = None;
            return;
        }
        let gone = self.book.tunings.remove(i);
        self.tuning_current = None;
        self.tuning_note = match self.book.save(&self.book_path) {
            Ok(()) => format!("forgot {:?}", gone.name),
            Err(e) => format!("could not save: {e}"),
        };
    }

    /// A knob moved by hand: the panel no longer matches the file.
    fn tuning_touched(&mut self) {
        if !self.applying_tuning {
            self.tuning_current = None;
        }
    }

    /// Keep the served description in step: which run, and its points as the
    /// bench is actually playing them (round count and probe length applied).
    fn sync_link(&self) {
        let d = &self.bundle.demos[self.demo];
        let tl = match self.shared.lock() {
            Ok(s) => s.timeline.clone(),
            Err(_) => return,
        };
        if let Ok(mut l) = self.link.lock() {
            l.demo_id = d.id.clone();
            l.demo_title = d.title.clone();
            l.demo_index = self.demo;
            l.demo_count = self.bundle.demos.len();
            l.shots = d.shots;
            let mut raw = (*l.raw).clone();
            if let Some(demo) = raw
                .get_mut("demos")
                .and_then(|v| v.as_array_mut())
                .and_then(|a| a.get_mut(self.demo))
            {
                demo["points"] = tl.points_json.clone();
            }
            l.raw = Arc::new(raw);
        }
    }

    /// Whatever the headset asked for since the last frame.
    fn drain_commands(&mut self) {
        let cmds: Vec<serve::Cmd> = match self.link.lock() {
            Ok(mut l) => l.cmds.drain(..).collect(),
            Err(_) => return,
        };
        for c in cmds {
            match c {
                serve::Cmd::TogglePlay => {
                    self.params.playing = !self.params.playing;
                    self.push_params();
                }
                serve::Cmd::SetPlaying(p) => {
                    self.params.playing = p;
                    self.push_params();
                }
                serve::Cmd::NextDemo => {
                    let n = self.bundle.demos.len();
                    self.load_demo((self.demo + 1) % n.max(1));
                }
                serve::Cmd::SetDemo(i) => {
                    if i < self.bundle.demos.len() {
                        self.load_demo(i);
                    }
                }
                serve::Cmd::Stretch(v) => {
                    self.params.stretch = v.clamp(1.0, 5000.0);
                    self.push_params();
                }
                serve::Cmd::Rewind => {
                    if let Ok(mut s) = self.shared.lock() {
                        s.timeline_gen = s.timeline_gen.wrapping_add(1);
                    }
                }
            }
        }
    }

    fn push_params(&mut self) {
        if let Ok(mut s) = self.shared.lock() {
            s.params = self.params.clone();
            s.params_gen = s.params_gen.wrapping_add(1);
        }
        self.tuning_touched();
    }

    fn load_demo(&mut self, idx: usize) {
        self.demo = idx;
        self.params.beams = beams_for(&self.bundle, idx);
        self.edit = Edit::for_demo(&self.bundle.demos[idx]);
        self.rebuild_timeline();
    }

    /// Re-lay the current run with the live edits (round count, probe time).
    fn rebuild_timeline(&mut self) {
        let d = &self.bundle.demos[self.demo];
        let tl = Timeline::build(d, d.shots, &self.edit);
        if let Ok(mut s) = self.shared.lock() {
            s.params = self.params.clone();
            s.params_gen = s.params_gen.wrapping_add(1);
            s.timeline = tl;
            s.timeline_gen = s.timeline_gen.wrapping_add(1);
            s.levels = vec![0.0; self.params.beams.len()];
            s.cursor_ns = 0.0;
        }
        self.sync_link();
        self.tuning_touched();
    }

    fn pull_state(&mut self) {
        if let Ok(s) = self.shared.lock() {
            self.cursor_ns = s.cursor_ns;
            self.levels = s.levels.clone();
            self.scope.copy_from_slice(&s.scope);
            self.scope_ch.copy_from_slice(&s.scope_ch);
            self.scope_head = s.scope_head;
        }
    }

    fn demo_name(&self, i: usize) -> String {
        if self.blind {
            format!("Run {}", (b'A' + i as u8) as char)
        } else {
            self.bundle.demos[i].title.clone()
        }
    }
}

fn beams_for(bundle: &Bundle, demo: usize) -> Vec<BeamCfg> {
    let d = &bundle.demos[demo];
    d.channels
        .iter()
        .map(|c| {
            let mut b = default_cfg(&c.key, &c.label, c.ref_mhz);
            if let Some(o) = d.voice.get(&c.key) {
                if let Some(v) = o.carrier {
                    b.carrier_hz = v;
                }
                if let Some(v) = o.ref_mhz {
                    b.ref_mhz = v;
                }
                if let Some(v) = o.sens {
                    b.sens = v;
                }
                if let Some(v) = o.gain {
                    b.gain = v;
                }
                if let Some(v) = o.resp {
                    b.resp = v;
                }
                if let Some(v) = o.tail {
                    b.tail = v;
                }
                if let Some(v) = o.pan {
                    b.pan = v;
                }
            }
            b
        })
        .collect()
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui_all(ctx);
    }
}

impl App {
    fn ui_all(&mut self, ctx: &egui::Context) {
        self.drain_commands();
        self.pull_state();
        if self.params.playing {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        self.masthead(ctx);
        self.left_rail(ctx);
        self.deck(ctx);
        self.mixer(ctx);
        self.stage(ctx);
    }
}

impl App {
    fn masthead(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("masthead")
            .frame(
                egui::Frame::default()
                    .fill(retro::FACE)
                    .inner_margin(egui::Margin::symmetric(10, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Audio Bench")
                            .font(retro::serif(19.0))
                            .color(retro::INK),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("ion trap sonification")
                            .font(retro::serif(12.0))
                            .color(retro::INK_DIM),
                    );
                    ui.add_space(14.0);
                    let dev = match (&self.engine, &self.audio_err) {
                        (Some(e), _) => format!("{} @ {:.0} Hz", e.device_name, e.sample_rate),
                        (None, Some(err)) => {
                            let mut e = err.replace('\n', " ");
                            e.truncate(46);
                            format!("no audio: {e}")
                        }
                        _ => "no audio".into(),
                    };
                    retro::readout(ui, &dev, 330.0, 11.0, retro::PHOSPHOR);
                    if let Some(sv) = &self.serving {
                        let (polls, age) = match self.link.lock() {
                            Ok(l) => {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                (l.polls, now.saturating_sub(l.last_poll_ms))
                            }
                            Err(_) => (0, u64::MAX),
                        };
                        let url = sv.urls.first().cloned().unwrap_or_default();
                        let state = if polls == 0 {
                            "no headset yet".to_string()
                        } else if age < 2000 {
                            format!("headset linked, {polls} polls")
                        } else {
                            format!("headset quiet {}s", age / 1000)
                        };
                        ui.add_space(8.0);
                        retro::readout(ui, &format!("{url}  |  {state}"), 300.0, 11.0,
                                       retro::PHOSPHOR);
                        ui.add_space(6.0);
                        if retro::button(ui, "Exit VR", false, 64.0)
                            .on_hover_text("take the headset out of immersive mode")
                            .clicked()
                        {
                            if let Ok(mut l) = self.link.lock() {
                                l.exit_vr = l.exit_vr.wrapping_add(1);
                            }
                        }
                    } else if let Some(e) = &self.serve_err {
                        ui.add_space(8.0);
                        retro::readout(ui, &format!("server: {e}"), 340.0, 11.0,
                                       retro::PHOSPHOR);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Hook for the guess-the-experiment mode: hides every name.
                        if ui.checkbox(&mut self.blind, "Blind").changed() {}
                        if !self.font_note {
                            ui.label(
                                egui::RichText::new("no serif face found")
                                    .font(retro::serif(11.0))
                                    .color(retro::INK_DIM),
                            );
                        }
                    });
                });
            });
    }

    fn left_rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("runs")
            .exact_width(292.0)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(retro::FACE)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut pick = None;
                    retro::group(ui, "Runs", |ui| {
                        for i in 0..self.bundle.demos.len() {
                            let d = &self.bundle.demos[i];
                            let (sub, meta) = if self.blind {
                                ("hidden".to_string(), String::new())
                            } else {
                                (
                                    d.subtitle.clone(),
                                    match &d.loop_block {
                                        Some(l) => format!("{} rounds live", l.measured.len()),
                                        None => format!("{} pts", d.points.len()),
                                    },
                                )
                            };
                            if retro::list_row(ui, &self.demo_name(i), &sub, &meta, i == self.demo)
                                .clicked()
                            {
                                pick = Some(i);
                            }
                        }
                    });
                    if let Some(i) = pick {
                        self.load_demo(i);
                    }

                    if self.run_parameters(ui) {
                        self.rebuild_timeline();
                    }
                    self.tunings_panel(ui);

                    ui.add_space(12.0);
                    let d = &self.bundle.demos[self.demo];
                    if !self.blind {
                        retro::group(ui, "About this run", |ui| {
                            ui.label(
                                egui::RichText::new(&d.note)
                                    .font(retro::serif(12.5))
                                    .color(retro::INK),
                            );
                        });
                        ui.add_space(12.0);
                        retro::group(ui, "Source", |ui| {
                            let rows = [
                                ("experiment", d.experiment.clone()),
                                ("job", d.job.to_string()),
                                ("file", d.file.clone()),
                                (
                                    "scan",
                                    if d.scan.is_empty() {
                                        "-".into()
                                    } else {
                                        d.scan.clone()
                                    },
                                ),
                                ("points", d.points.len().to_string()),
                                (
                                    "shots",
                                    format!("{} of {} measured", d.shots, d.shots_measured),
                                ),
                            ];
                            for (k, v) in rows {
                                ui.horizontal_top(|ui| {
                                    ui.allocate_ui(Vec2::new(66.0, 15.0), |ui| {
                                        ui.label(
                                            egui::RichText::new(k)
                                                .font(retro::serif(11.5))
                                                .color(retro::INK_DIM),
                                        );
                                    });
                                    ui.label(
                                        egui::RichText::new(v)
                                            .font(retro::mono(11.0))
                                            .color(retro::INK),
                                    );
                                });
                            }
                        });
                    }
                });
            });
    }

    fn deck(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("deck")
            .frame(
                egui::Frame::default()
                    .fill(retro::FACE)
                    .inner_margin(egui::Margin::symmetric(14, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut changed = false;
                    let spec = retro::KnobSpec {
                        label: "SLOWDOWN",
                        min: 1.0,
                        max: 5000.0,
                        log: true,
                        diameter: 62.0,
                        unit: "x",
                        decimals: 0,
                    };
                    changed |= retro::knob(ui, &spec, &mut self.params.stretch);
                    ui.add_space(6.0);
                    let mut master = self.params.master;
                    let mspec = retro::KnobSpec {
                        label: "MASTER",
                        min: 0.0,
                        max: 1.4,
                        log: false,
                        diameter: 48.0,
                        unit: "",
                        decimals: 2,
                    };
                    if retro::knob(ui, &mspec, &mut master) {
                        self.params.master = master;
                        changed = true;
                    }
                    ui.add_space(14.0);

                    ui.vertical(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("Quantise to scale")
                                .font(retro::serif(11.5))
                                .color(retro::INK_DIM),
                        );
                        egui::ComboBox::from_id_salt("scale")
                            .width(150.0)
                            .selected_text(self.params.scale.name())
                            .show_ui(ui, |ui| {
                                for s in Scale::ALL {
                                    if ui
                                        .selectable_label(self.params.scale == s, s.name())
                                        .clicked()
                                    {
                                        self.params.scale = s;
                                        changed = true;
                                    }
                                }
                            });
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("root")
                                    .font(retro::serif(11.5))
                                    .color(retro::INK_DIM),
                            );
                            egui::ComboBox::from_id_salt("root")
                                .width(58.0)
                                .selected_text(NOTE_NAMES[self.params.root as usize % 12])
                                .show_ui(ui, |ui| {
                                    for (i, n) in NOTE_NAMES.iter().enumerate() {
                                        if ui
                                            .selectable_label(self.params.root == i as i32, *n)
                                            .clicked()
                                        {
                                            self.params.root = i as i32;
                                            changed = true;
                                        }
                                    }
                                });
                        });
                    });

                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("View window")
                                .font(retro::serif(11.5))
                                .color(retro::INK_DIM),
                        );
                        ui.add(
                            egui::Slider::new(&mut self.window_s, 1.0..=20.0)
                                .suffix(" s")
                                .max_decimals(1),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Detuning sets pitch. Slowing down stretches the rhythm \
                                 and leaves the tuning alone.",
                            )
                            .font(retro::serif(11.5))
                            .color(retro::INK_DIM),
                        );
                    });

                    if changed {
                        self.push_params();
                    }
                });
            });
    }

    fn mixer(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("mixer")
            .exact_width(352.0)
            .resizable(false)
            .frame(
                egui::Frame::default()
                    .fill(retro::FACE)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut changed = false;
                    let stats: Vec<(f64, f64)> = self.bundle.demos[self.demo]
                        .channels
                        .iter()
                        .map(|c| (c.ref_mhz, c.span_mhz))
                        .collect();
                    for i in 0..self.params.beams.len() {
                        let level = self.levels.get(i).copied().unwrap_or(0.0);
                        let (median, span) = stats.get(i).copied().unwrap_or((0.0, 0.0));
                        changed |= beam_strip(ui, &mut self.params.beams[i], level, median, span);
                        ui.add_space(9.0);
                    }
                    if changed {
                        self.push_params();
                    }
                });
            });
    }

    fn stage(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(retro::FACE)
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show(ctx, |ui| {
                self.transport(ui);
                ui.add_space(8.0);
                let h = ui.available_height();
                let lane_h = (h * 0.62).max(150.0);
                let rect = ui.allocate_space(Vec2::new(ui.available_width(), lane_h)).1;
                self.draw_lanes(ui, rect);
                ui.add_space(8.0);
                let wave_h = (ui.available_height() - 6.0).max(80.0);
                let rect2 = ui.allocate_space(Vec2::new(ui.available_width(), wave_h)).1;
                self.draw_wave(ui, rect2);
            });
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let playing = self.params.playing;
            if retro::button(ui, if playing { "Stop" } else { "Play" }, false, 78.0).clicked() {
                self.params.playing = !playing;
                self.push_params();
            }
            if retro::button(ui, "Rewind", false, 78.0).clicked() {
                if let Ok(mut s) = self.shared.lock() {
                    s.timeline_gen = s.timeline_gen.wrapping_add(1);
                }
            }
            ui.add_space(10.0);

            let (block, path) = self.current_block();
            ui.vertical(|ui| {
                retro::readout(ui, &block, 330.0, 15.0, retro::PHOSPHOR);
                retro::readout(ui, &path, 330.0, 10.0, retro::SCREEN_GRID.gamma_multiply(3.0));
            });
            ui.add_space(10.0);
            ui.vertical(|ui| {
                let (label, pos) = {
                    let tl = self.shared.lock().ok().map(|s| s.timeline.clone());
                    match tl {
                        Some(t) => {
                            let pi = t.point_at(self.cursor_ns);
                            (
                                t.labels.get(pi).cloned().unwrap_or_default(),
                                format!("{}/{}", pi + 1, t.labels.len().max(1)),
                            )
                        }
                        None => (String::new(), String::new()),
                    }
                };
                let shown = if self.blind {
                    format!("point {pos}")
                } else {
                    format!("{label}   [{pos}]")
                };
                retro::readout(ui, &shown, 260.0, 13.0, retro::PHOSPHOR);
                retro::readout(
                    ui,
                    &format!("t = {:.3} ms", self.cursor_ns / 1e6),
                    260.0,
                    10.0,
                    retro::SCREEN_GRID.gamma_multiply(3.0),
                );
            });
        });
    }

    fn current_block(&self) -> (String, String) {
        let tl = match self.shared.lock() {
            Ok(s) => s.timeline.clone(),
            Err(_) => return ("--".into(), String::new()),
        };
        let d = &self.bundle.demos[self.demo];
        let t = self.cursor_ns;
        let start = tl.first_from(t - 2.0e6);
        let mut best: Option<(f32, usize)> = None;
        for n in tl.notes[start..].iter() {
            if n.t0 > t {
                break;
            }
            if n.t1 <= t {
                continue;
            }
            let amp = n.tones.iter().map(|x| x.1).fold(0.0f32, f32::max);
            if best.map_or(true, |b| amp > b.0) {
                best = Some((amp, n.block));
            }
        }
        match best {
            Some((_, b)) => {
                let full = d.blocks.get(b).cloned().unwrap_or_default();
                let leaf = full.rsplit('/').next().unwrap_or("").to_string();
                (leaf, full)
            }
            None => ("(silence)".into(), String::new()),
        }
    }

    fn draw_lanes(&self, ui: &mut egui::Ui, rect: Rect) {
        let d = &self.bundle.demos[self.demo];
        let nch = self.params.beams.len().max(1);
        retro::screen(ui, rect, 8, nch);
        let tl = match self.shared.lock() {
            Ok(s) => s.timeline.clone(),
            Err(_) => return,
        };
        let p = ui.painter_at(rect);

        let win_ns = (self.window_s as f64) * 1e9 / self.params.stretch.max(0.01) as f64;
        let head = 0.30f64;
        let left = self.cursor_ns - head * win_ns;
        let right = left + win_ns;
        let x_of = |t: f64| rect.left() + ((t - left) / win_ns) as f32 * rect.width();
        let lane_h = rect.height() / nch as f32;

        for (pass_from, pass_to, off) in [
            (left, right, 0.0),
            (left + tl.total_ns, right + tl.total_ns, -tl.total_ns),
            (left - tl.total_ns, right - tl.total_ns, tl.total_ns),
        ] {
            if pass_to < 0.0 || pass_from > tl.total_ns {
                continue;
            }
            let lookback = (win_ns * 1.2).max(2.0e6);
            let start = tl.first_from(pass_from - lookback);
            let mut drawn = 0usize;
            for n in tl.notes[start..].iter() {
                if n.t0 > pass_to {
                    break;
                }
                if n.t1 < pass_from {
                    continue;
                }
                drawn += 1;
                if drawn > 6000 {
                    break;
                }
                let cfg = match self.params.beams.get(n.ch) {
                    Some(c) => c,
                    None => continue,
                };
                let amp = n.tones.iter().map(|x| x.1).fold(0.0f32, f32::max);
                let y0 = rect.top() + n.ch as f32 * lane_h + 3.0;
                let hh = (lane_h - 6.0).max(3.0);
                let x0 = x_of(n.t0 + off);
                let x1 = x_of(n.t1 + off).max(x0 + 1.2);
                let a = if cfg.mute { 55 } else { 255 };
                let col = Color32::from_rgba_unmultiplied(
                    cfg.color[0].saturating_add(40),
                    cfg.color[1].saturating_add(40),
                    cfg.color[2].saturating_add(40),
                    a,
                );
                let bar = Rect::from_min_max(
                    Pos2::new(x0, y0 + hh * (1.0 - amp.clamp(0.08, 1.0)) * 0.75),
                    Pos2::new(x1, y0 + hh),
                );
                p.rect_filled(bar, CornerRadius::ZERO, col);
                // tone spread: one tick per tone, so an SDF pulse shows as two
                if n.tones.len() > 1 {
                    for (k, _) in n.tones.iter().enumerate() {
                        let y = y0 + hh * (k as f32 + 0.5) / n.tones.len() as f32;
                        p.line_segment(
                            [Pos2::new(x0, y), Pos2::new(x1, y)],
                            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 150)),
                        );
                    }
                }
            }
            for g in tl.gates.iter() {
                if *g < pass_from || *g > pass_to {
                    continue;
                }
                let x = x_of(*g + off);
                p.line_segment(
                    [
                        Pos2::new(x, rect.bottom() - 7.0),
                        Pos2::new(x, rect.bottom()),
                    ],
                    Stroke::new(1.0_f32, retro::PHOSPHOR),
                );
            }
        }

        // lane labels
        for (i, cfg) in self.params.beams.iter().enumerate() {
            let y = rect.top() + i as f32 * lane_h + 2.0;
            p.text(
                Pos2::new(rect.left() + 6.0, y),
                egui::Align2::LEFT_TOP,
                &cfg.key,
                retro::mono(10.0),
                Color32::from_rgb(
                    cfg.color[0].saturating_add(80),
                    cfg.color[1].saturating_add(80),
                    cfg.color[2].saturating_add(80),
                ),
            );
        }

        // playhead
        let xh = rect.left() + head as f32 * rect.width();
        p.line_segment(
            [Pos2::new(xh, rect.top()), Pos2::new(xh, rect.bottom())],
            Stroke::new(1.5_f32, Color32::WHITE),
        );
        p.text(
            Pos2::new(rect.right() - 6.0, rect.top() + 4.0),
            egui::Align2::RIGHT_TOP,
            format!("{} pulses  |  window {:.1} s", tl.notes.len(), self.window_s),
            retro::mono(10.0),
            retro::SCREEN_GRID.gamma_multiply(3.5),
        );
        let _ = d;
    }

    fn draw_wave(&self, ui: &mut egui::Ui, rect: Rect) {
        retro::screen(ui, rect, 8, 2);
        let p = ui.painter_at(rect);
        let mid = rect.center().y;
        let half = rect.height() * 0.46;
        let n = SCOPE_LEN;
        let w = rect.width();
        for i in 0..n {
            let idx = (self.scope_head + i) % n;
            let v = self.scope[idx].clamp(0.0, 1.0);
            if v <= 0.0005 {
                continue;
            }
            let x = rect.left() + w * i as f32 / n as f32;
            let ch = self.scope_ch[idx];
            let col = match self.params.beams.get(ch as usize) {
                Some(c) => Color32::from_rgb(
                    c.color[0].saturating_add(70),
                    c.color[1].saturating_add(70),
                    c.color[2].saturating_add(70),
                ),
                None => retro::PHOSPHOR,
            };
            p.line_segment(
                [
                    Pos2::new(x, mid - v * half),
                    Pos2::new(x, mid + v * half),
                ],
                Stroke::new(1.0_f32, col),
            );
        }
        p.line_segment(
            [
                Pos2::new(rect.left(), mid),
                Pos2::new(rect.right(), mid),
            ],
            Stroke::new(1.0_f32, retro::SCREEN_GRID),
        );
        p.text(
            Pos2::new(rect.left() + 6.0, rect.top() + 4.0),
            egui::Align2::LEFT_TOP,
            "output waveform",
            retro::mono(10.0),
            retro::SCREEN_GRID.gamma_multiply(3.5),
        );
    }
}

impl App {
    /// Saved tunings: a name, SAVE, the list, DELETE, and a readout.
    fn tunings_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        let mut recall: Option<usize> = None;
        let mut save = false;
        let mut delete = false;
        retro::group(ui, "Tunings", |ui| {
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.tuning_name)
                        .hint_text(self.bundle.demos[self.demo].title.clone())
                        .desired_width(150.0)
                        .font(retro::serif(13.0)),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    save = true;
                }
                if retro::button(ui, "SAVE", false, 56.0).clicked() {
                    save = true;
                }
            });
            ui.add_space(4.0);
            if self.book.tunings.is_empty() {
                ui.label(
                    egui::RichText::new("nothing saved yet")
                        .font(retro::serif(11.5))
                        .color(retro::INK_DIM),
                );
            }
            let n = self.book.tunings.len();
            for i in 0..n {
                let t = &self.book.tunings[i];
                let sub = if t.run.is_empty() { t.demo.clone() } else { t.run.clone() };
                let cur = self.tuning_current == Some(i);
                if retro::list_row(ui, &t.name, &sub, &t.age(), cur).clicked() {
                    recall = Some(i);
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let has = self.tuning_current.is_some();
                if has && retro::button(ui, "DELETE", false, 64.0).clicked() {
                    delete = true;
                }
                ui.label(
                    egui::RichText::new(&self.tuning_note)
                        .font(retro::serif(11.5))
                        .color(retro::INK_DIM),
                );
            });
            ui.label(
                egui::RichText::new(tunings::pretty(&self.book_path))
                    .font(retro::mono(9.5))
                    .color(retro::INK_DIM),
            );
        });
        if save {
            self.tuning_save();
        }
        if let Some(i) = recall {
            self.tuning_recall(i);
        }
        if delete {
            self.tuning_delete();
        }
    }

    /// Sliders for whatever this run exposes. Returns true when the timeline
    /// must be rebuilt (on release, so dragging stays smooth).
    fn run_parameters(&mut self, ui: &mut egui::Ui) -> bool {
        let d = &self.bundle.demos[self.demo];
        let has_loop = d.loop_block.is_some();
        let has_stretch = d.stretch.is_some();
        if !has_loop && !has_stretch {
            return false;
        }
        let loop_block = d.loop_block.clone();
        let stretch = d.stretch.clone();
        let mut rebuild = false;
        let mut done = |r: &egui::Response| {
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                rebuild = true;
            }
        };
        ui.add_space(10.0);
        retro::group(ui, "Run parameters", |ui| {
            if let Some(l) = &loop_block {
                ui.label(
                    egui::RichText::new(format!(
                        "{} rounds: one round is {:.3} us, measured at {}..{}",
                        l.name,
                        l.period / 1e3,
                        l.measured.iter().min().copied().unwrap_or(1),
                        l.measured.iter().max().copied().unwrap_or(1),
                    ))
                    .font(retro::serif(11.5))
                    .color(retro::INK_DIM),
                );
                let max = l.max.max(2);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("from").font(retro::serif(11.0)));
                    let r = ui.add(egui::Slider::new(&mut self.edit.loop_from, 1..=max));
                    done(&r);
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("to").font(retro::serif(11.0)));
                    let r = ui.add(egui::Slider::new(&mut self.edit.loop_to, 1..=max));
                    done(&r);
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("points").font(retro::serif(11.0)));
                    let r = ui.add(egui::Slider::new(&mut self.edit.loop_points, 1..=60));
                    done(&r);
                });
                if self.edit.loop_to < self.edit.loop_from {
                    self.edit.loop_to = self.edit.loop_from;
                }
            }
            if let Some(st) = &stretch {
                let mut ms = (self.edit.stretch_ns.unwrap_or(st.t1 - st.t0) / 1e6) as f32;
                ui.label(
                    egui::RichText::new(format!(
                        "{} length, as run: {:.3} ms",
                        st.name,
                        (st.t1 - st.t0) / 1e6
                    ))
                    .font(retro::serif(11.5))
                    .color(retro::INK_DIM),
                );
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("probe").font(retro::serif(11.0)));
                    let r = ui.add(
                        egui::Slider::new(&mut ms, (st.min_ns / 1e6) as f32..=(st.max_ns / 1e6) as f32)
                            .logarithmic(true)
                            .suffix(" ms")
                            .max_decimals(3),
                    );
                    if r.changed() {
                        self.edit.stretch_ns = Some((ms as f64) * 1e6);
                    }
                    done(&r);
                });
            }
        });
        rebuild
    }
}

fn beam_strip(
    ui: &mut egui::Ui,
    cfg: &mut BeamCfg,
    level: f32,
    median_ref: f64,
    span_mhz: f64,
) -> bool {
    let mut changed = false;
    let title = cfg.label.clone();
    retro::group(ui, &title, |ui| {
        ui.horizontal(|ui| {
            let (sw, _) = ui.allocate_exact_size(Vec2::new(10.0, 30.0), egui::Sense::hover());
            ui.painter().rect_filled(
                sw,
                CornerRadius::ZERO,
                Color32::from_rgb(cfg.color[0], cfg.color[1], cfg.color[2]),
            );
            retro::bevel(ui.painter(), sw, false, 1.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(&cfg.key)
                        .font(retro::mono(11.0))
                        .color(retro::INK),
                );
                // level meter
                let (m, _) = ui.allocate_exact_size(Vec2::new(120.0, 8.0), egui::Sense::hover());
                ui.painter().rect_filled(m, CornerRadius::ZERO, retro::SCREEN);
                let fill = Rect::from_min_size(
                    m.left_top(),
                    Vec2::new(m.width() * level.clamp(0.0, 1.0), m.height()),
                );
                ui.painter().rect_filled(
                    fill,
                    CornerRadius::ZERO,
                    Color32::from_rgb(
                        cfg.color[0].saturating_add(60),
                        cfg.color[1].saturating_add(60),
                        cfg.color[2].saturating_add(60),
                    ),
                );
                retro::bevel(ui.painter(), m, false, 1.0);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if retro::button(ui, if cfg.mute { "MUTE" } else { "mute" }, cfg.mute, 52.0)
                    .clicked()
                {
                    cfg.mute = !cfg.mute;
                    changed = true;
                }
            });
        });

        ui.horizontal(|ui| {
            let spec = retro::KnobSpec {
                label: "CARRIER",
                min: 24.0,
                max: 4000.0,
                log: true,
                diameter: 50.0,
                unit: " Hz",
                decimals: 0,
            };
            changed |= retro::knob(ui, &spec, &mut cfg.carrier_hz);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("sens")
                            .font(retro::serif(11.0))
                            .color(retro::INK_DIM),
                    );
                    // Five decades: a 729 bare scan needs ~1600 st/MHz, the
                    // 397 wants a third of one.
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut cfg.sens, 0.01..=5000.0)
                                .logarithmic(true)
                                .max_decimals(2),
                        )
                        .on_hover_text("semitones of pitch per MHz of detuning")
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("level")
                            .font(retro::serif(11.0))
                            .color(retro::INK_DIM),
                    );
                    changed |= ui
                        .add(egui::Slider::new(&mut cfg.gain, 0.0..=4.0).max_decimals(2))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("resp")
                            .font(retro::serif(11.0))
                            .color(retro::INK_DIM),
                    );
                    changed |= ui
                        .add(egui::Slider::new(&mut cfg.resp, 0.1..=1.0).max_decimals(2))
                        .on_hover_text(
                            "amplitude response: 1 is linear, lower lifts weak pulses",
                        )
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("ring")
                            .font(retro::serif(11.0))
                            .color(retro::INK_DIM),
                    );
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut cfg.tail, 0.0..=1.2)
                                .suffix(" s")
                                .max_decimals(2),
                        )
                        .on_hover_text("how long a pulse keeps sounding after it ends")
                        .changed();
                });
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(format!("wave{}", cfg.key))
                        .width(78.0)
                        .selected_text(cfg.wave.name())
                        .show_ui(ui, |ui| {
                            for w in Wave::ALL {
                                if ui.selectable_label(cfg.wave == w, w.name()).clicked() {
                                    cfg.wave = w;
                                    changed = true;
                                }
                            }
                        });
                    if retro::button(ui, "centre", false, 50.0)
                        .on_hover_text("put the carrier at this run's median detuning")
                        .clicked()
                    {
                        cfg.ref_mhz = median_ref;
                        changed = true;
                    }
                    if retro::button(ui, "fit", false, 34.0)
                        .on_hover_text("open this run's detuning span out to two octaves")
                        .clicked()
                    {
                        cfg.ref_mhz = median_ref;
                        if span_mhz > 1e-9 {
                            cfg.sens = (crate::synth::FIT_SEMITONES / span_mhz as f32)
                                .clamp(0.01, 5000.0);
                        }
                        changed = true;
                    }
                });
                ui.label(
                    egui::RichText::new(format!(
                        "ref {:+.4} MHz   span {:.4} MHz",
                        cfg.ref_mhz, span_mhz
                    ))
                    .font(retro::mono(9.5))
                    .color(retro::INK_DIM),
                );
            });
        });
    });
    changed
}
