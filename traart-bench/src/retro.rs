//! Motif / Qt-2 era chrome: grey chassis, square corners, hard bevels, serif type.
//!
//! egui draws flat by default, so the 3D edges are painted by hand: one light
//! line along the top and left, one dark line along the bottom and right,
//! swapped when a control is pressed or a panel is sunken.

use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

pub const FACE: Color32 = Color32::from_rgb(0xD4, 0xD0, 0xC8);
pub const FACE_DARK: Color32 = Color32::from_rgb(0xBD, 0xB9, 0xB1);
pub const LIGHT: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const MID: Color32 = Color32::from_rgb(0x9A, 0x96, 0x8F);
pub const SHADOW: Color32 = Color32::from_rgb(0x54, 0x51, 0x4C);
pub const INK: Color32 = Color32::from_rgb(0x0A, 0x0A, 0x0A);
pub const INK_DIM: Color32 = Color32::from_rgb(0x4A, 0x47, 0x42);
pub const SELECT: Color32 = Color32::from_rgb(0x00, 0x00, 0x80);
pub const SCREEN: Color32 = Color32::from_rgb(0x0C, 0x10, 0x14);
pub const SCREEN_GRID: Color32 = Color32::from_rgb(0x1E, 0x26, 0x2C);
pub const PHOSPHOR: Color32 = Color32::from_rgb(0xC8, 0xE6, 0xD0);

const SERIF_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
    "/Library/Fonts/Times New Roman.ttf",
    "/System/Library/Fonts/Supplemental/Georgia.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
    "/usr/share/fonts/TTF/DejaVuSerif.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
    "/usr/share/fonts/liberation-serif/LiberationSerif-Regular.ttf",
    "C:\\Windows\\Fonts\\times.ttf",
    "C:\\Windows\\Fonts\\georgia.ttf",
];

const MONO_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Courier New.ttf",
    "/Library/Fonts/Courier New.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "C:\\Windows\\Fonts\\cour.ttf",
];

fn first_readable(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// Returns the name of the serif face actually loaded, if any.
pub fn install(ctx: &egui::Context) -> Option<String> {
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded = None;

    if let Some(bytes) = first_readable(SERIF_CANDIDATES) {
        fonts.font_data.insert(
            "chassis-serif".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            fam.insert(0, "chassis-serif".to_owned());
        }
        loaded = Some("serif".to_string());
    }
    if let Some(bytes) = first_readable(MONO_CANDIDATES) {
        fonts.font_data.insert(
            "chassis-mono".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            fam.insert(0, "chassis-mono".to_owned());
        }
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    v.dark_mode = false;
    v.override_text_color = Some(INK);
    v.panel_fill = FACE;
    v.window_fill = FACE;
    v.extreme_bg_color = Color32::from_rgb(0xEC, 0xEA, 0xE5);
    v.faint_bg_color = FACE_DARK;
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_stroke = Stroke::new(1.0_f32, SHADOW);
    v.selection.bg_fill = SELECT;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::ZERO;
        w.bg_fill = FACE;
        w.weak_bg_fill = FACE;
        w.bg_stroke = Stroke::new(1.0_f32, MID);
        w.fg_stroke = Stroke::new(1.0_f32, INK);
        w.expansion = 0.0;
    }
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xDE, 0xDB, 0xD4);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xDE, 0xDB, 0xD4);
    v.widgets.active.bg_fill = FACE_DARK;
    v.widgets.active.weak_bg_fill = FACE_DARK;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, MID);
    // Slider rails read as sunken troughs; buttons and combos keep the face
    // colour because they are painted with their own bevels.
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.bg_fill = Color32::from_rgb(0x8C, 0x88, 0x82);
    }

    style.spacing.item_spacing = Vec2::new(6.0, 5.0);
    style.spacing.button_padding = Vec2::new(9.0, 4.0);
    style.spacing.slider_width = 74.0;
    style.spacing.interact_size = Vec2::new(40.0, 20.0);
    ctx.set_style(style);
    loaded
}

pub fn serif(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Hard two-pixel bevel. `raised` puts the light on the top-left.
pub fn bevel(painter: &egui::Painter, rect: Rect, raised: bool, depth: f32) {
    let (hi, lo) = if raised { (LIGHT, SHADOW) } else { (SHADOW, LIGHT) };
    let mid_out = if raised { MID } else { FACE };
    for i in 0..depth as i32 {
        let k = i as f32;
        let r = rect.shrink(k);
        let c_hi = if i == 0 { hi } else { mid_out };
        painter.line_segment([r.left_bottom(), r.left_top()], Stroke::new(1.0_f32, c_hi));
        painter.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, c_hi));
        let c_lo = if i == 0 { lo } else { MID };
        painter.line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0_f32, c_lo));
        painter.line_segment([r.right_bottom(), r.left_bottom()], Stroke::new(1.0_f32, c_lo));
    }
}

pub fn panel(ui: &mut Ui, rect: Rect, raised: bool) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, FACE);
    bevel(p, rect, raised, 2.0);
}

/// A sunken instrument screen: dark ground, faint graticule.
pub fn screen(ui: &Ui, rect: Rect, grid_x: usize, grid_y: usize) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, SCREEN);
    let stroke = Stroke::new(1.0_f32, SCREEN_GRID);
    for i in 1..grid_x {
        let x = rect.left() + rect.width() * i as f32 / grid_x as f32;
        p.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
    }
    for i in 1..grid_y {
        let y = rect.top() + rect.height() * i as f32 / grid_y as f32;
        p.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
    }
    bevel(p, rect, false, 2.0);
}

/// A classic push button. `latched` draws it held in.
pub fn button(ui: &mut Ui, text: &str, latched: bool, min_w: f32) -> Response {
    let font = serif(14.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, INK);
    let size = Vec2::new(
        (galley.size().x + 22.0).max(min_w),
        (galley.size().y + 11.0).max(24.0),
    );
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let down = latched || resp.is_pointer_button_down_on();
    let p = ui.painter();
    let face = if down {
        FACE_DARK
    } else if resp.hovered() {
        Color32::from_rgb(0xDE, 0xDB, 0xD4)
    } else {
        FACE
    };
    p.rect_filled(rect, CornerRadius::ZERO, face);
    bevel(p, rect, !down, 2.0);
    let off = if down { 1.0 } else { 0.0 };
    p.galley(
        Pos2::new(
            rect.center().x - galley.size().x / 2.0 + off,
            rect.center().y - galley.size().y / 2.0 + off,
        ),
        galley,
        INK,
    );
    resp
}

/// A list row, Motif style: sunken-blue when current.
pub fn list_row(ui: &mut Ui, title: &str, sub: &str, meta: &str, current: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 48.0), Sense::click());
    let p = ui.painter().with_clip_rect(rect.shrink2(Vec2::new(2.0, 0.0)));
    let bg = if current {
        SELECT
    } else if resp.hovered() {
        Color32::from_rgb(0xDE, 0xDB, 0xD4)
    } else {
        FACE
    };
    p.rect_filled(rect, CornerRadius::ZERO, bg);
    bevel(&p, rect, !current, 1.0);
    let (t_col, s_col) = if current {
        (Color32::WHITE, Color32::from_rgb(0xC0, 0xC8, 0xE8))
    } else {
        (INK, INK_DIM)
    };
    let meta_g = p.layout_no_wrap(meta.to_string(), mono(10.0), s_col);
    let meta_w = meta_g.size().x;
    // title on its own line, subtitle and meta share the second
    let title_g = p.layout_no_wrap(title.to_string(), serif(15.0), t_col);
    p.galley(rect.left_top() + Vec2::new(8.0, 5.0), title_g, t_col);
    let sub_g = p.layout_no_wrap(sub.to_string(), serif(12.0), s_col);
    p.galley(rect.left_top() + Vec2::new(8.0, 26.0), sub_g, s_col);
    // meta sits over the subtitle's tail on a face-coloured chip so it stays legible
    let chip = Rect::from_min_size(
        Pos2::new(rect.right() - meta_w - 12.0, rect.top() + 27.0),
        Vec2::new(meta_w + 8.0, 14.0),
    );
    p.rect_filled(chip, CornerRadius::ZERO, bg);
    p.galley(chip.left_top() + Vec2::new(4.0, 1.0), meta_g, s_col);
    resp
}

/// Sunken readout field, like an LCD strip on a rack unit.
pub fn readout(ui: &mut Ui, text: &str, width: f32, size: f32, col: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, size + 10.0), Sense::hover());
    let p = ui.painter().with_clip_rect(rect);
    p.rect_filled(rect, CornerRadius::ZERO, SCREEN);
    bevel(&p, rect, false, 1.0);
    p.text(
        rect.left_center() + Vec2::new(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        mono(size),
        col,
    );
}

pub struct KnobSpec<'a> {
    pub label: &'a str,
    pub min: f32,
    pub max: f32,
    pub log: bool,
    pub diameter: f32,
    pub unit: &'a str,
    pub decimals: usize,
}

/// Rotary control. Drag up/down or scroll; shift-drag for fine steps.
pub fn knob(ui: &mut Ui, spec: &KnobSpec, value: &mut f32) -> bool {
    let d = spec.diameter;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(d, d + 30.0), Sense::drag());
    let dial = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.top() + d / 2.0 + 11.0),
        Vec2::splat(d),
    );

    let to_norm = |v: f32| -> f32 {
        if spec.log {
            ((v.max(1e-6) / spec.min).ln() / (spec.max / spec.min).ln()).clamp(0.0, 1.0)
        } else {
            ((v - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0)
        }
    };
    let from_norm = |n: f32| -> f32 {
        let n = n.clamp(0.0, 1.0);
        if spec.log {
            spec.min * (spec.max / spec.min).powf(n)
        } else {
            spec.min + n * (spec.max - spec.min)
        }
    };

    let mut norm = to_norm(*value);
    let mut changed = false;
    let fine = ui.input(|i| i.modifiers.shift);
    let dy = resp.drag_delta().y;
    if dy != 0.0 {
        norm -= dy / if fine { 900.0 } else { 180.0 };
        changed = true;
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll != 0.0 {
            norm += scroll / if fine { 2400.0 } else { 480.0 };
            changed = true;
        }
    }
    if changed {
        *value = from_norm(norm);
    }
    let norm = to_norm(*value);

    let p = ui.painter();
    let c = dial.center();
    let r = d / 2.0;
    p.circle_filled(c, r, FACE);
    // bevel ring, drawn as two arcs' worth of short segments
    for i in 0..48 {
        let a0 = i as f32 / 48.0 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / 48.0 * std::f32::consts::TAU;
        let lit = a0.cos() + a0.sin() < 0.0;
        let col = if lit { LIGHT } else { SHADOW };
        p.line_segment(
            [
                c + Vec2::new(a0.cos(), a0.sin()) * r,
                c + Vec2::new(a1.cos(), a1.sin()) * r,
            ],
            Stroke::new(1.6_f32, col),
        );
    }
    p.circle_filled(c, r - 3.0, FACE_DARK);
    p.circle_stroke(c, r - 3.0, Stroke::new(1.0_f32, MID));

    // travel arc: 240 degrees, opening downward
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    for i in 0..30 {
        let t0 = i as f32 / 30.0;
        let t1 = (i + 1) as f32 / 30.0;
        let on = t1 <= norm;
        let a0 = start + sweep * t0;
        let a1 = start + sweep * t1;
        let rr = r + 3.5;
        p.line_segment(
            [
                c + Vec2::new(a0.cos(), a0.sin()) * rr,
                c + Vec2::new(a1.cos(), a1.sin()) * rr,
            ],
            Stroke::new(2.4_f32, if on { SELECT } else { MID }),
        );
    }
    let a = start + sweep * norm;
    p.line_segment(
        [
            c + Vec2::new(a.cos(), a.sin()) * (r * 0.28),
            c + Vec2::new(a.cos(), a.sin()) * (r - 5.0),
        ],
        Stroke::new(2.2_f32, INK),
    );

    p.text(
        Pos2::new(rect.center().x, rect.top()),
        egui::Align2::CENTER_TOP,
        spec.label,
        serif(11.0),
        INK_DIM,
    );
    let txt = format!("{:.*}{}", spec.decimals, *value, spec.unit);
    p.text(
        Pos2::new(rect.center().x, rect.bottom() - 13.0),
        egui::Align2::CENTER_TOP,
        &txt,
        mono(11.0),
        INK,
    );
    changed
}

/// Group box with a title cut into the top rule, as Motif did it.
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let title = title.to_string();
    egui::Frame::default()
        .fill(FACE)
        .inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 14,
            bottom: 9,
        })
        .show(ui, |ui| {
            let top = ui.min_rect().top() - 6.0;
            let out = add(ui);
            let r = ui.min_rect().expand2(Vec2::new(8.0, 0.0));
            let frame = Rect::from_min_max(Pos2::new(r.left(), top), r.right_bottom());
            let p = ui.painter();
            bevel(p, frame, false, 1.0);
            let g = p.layout_no_wrap(title.clone(), serif(11.5), INK_DIM);
            let tw = g.size().x + 8.0;
            p.rect_filled(
                Rect::from_min_size(
                    Pos2::new(frame.left() + 7.0, frame.top() - 7.0),
                    Vec2::new(tw, 14.0),
                ),
                CornerRadius::ZERO,
                FACE,
            );
            p.text(
                Pos2::new(frame.left() + 11.0, frame.top() - 7.0),
                egui::Align2::LEFT_TOP,
                &title,
                serif(11.5),
                INK_DIM,
            );
            out
        })
        .inner
}
