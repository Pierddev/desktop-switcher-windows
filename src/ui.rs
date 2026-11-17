//! The settings window (egui). Opened from the tray icon; it runs in its own
//! short-lived process so the background helper stays tiny and starts fast.
//!
//! Look: a light, airy settings page in the spirit of Windows 11 — white cards
//! with one setting per row, toggles and segmented choices, one accent colour.

use crate::config::Config;
use crate::{helper_notify, i18n, icon, startup};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout, Margin, Rect,
    RichText, Sense, Stroke, Ui, Vec2,
};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Options {
    pub welcome: bool,
    pub screenshot: Option<PathBuf>,
}

// Palette
const BG: Color32 = Color32::from_rgb(0xf5, 0xf6, 0xf8);
const CARD: Color32 = Color32::WHITE;
const BORDER: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xec);
const DIVIDER: Color32 = Color32::from_rgb(0xef, 0xf0, 0xf3);
const TEXT: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x22);
const MUTED: Color32 = Color32::from_rgb(0x6b, 0x70, 0x7a);
const ACCENT: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xeb);
const ACCENT_SOFT: Color32 = Color32::from_rgb(0xe8, 0xf0, 0xfe);
const TRACK: Color32 = Color32::from_rgb(0xd5, 0xd8, 0xde);

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn run(opts: Options) -> eframe::Result<()> {
    let mut cfg = Config::load();
    cfg.auto_startup = startup::is_enabled();
    let title = i18n::texts(&cfg.language).settings_title;

    let size = 32u32;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size([560.0, 860.0])
        .with_min_inner_size([460.0, 520.0])
        .with_icon(egui::IconData { rgba: icon::rgba(size), width: size, height: size });
    if opts.screenshot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let native = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(
        title,
        native,
        Box::new(move |cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(SettingsApp {
                cfg,
                welcome: opts.welcome,
                screenshot: opts.screenshot,
                frame: 0,
                shot_requested: false,
            }))
        }),
    )
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    for (name, file) in [("segoe", "segoeui.ttf"), ("segoe-semibold", "seguisb.ttf"), ("segoe-bold", "segoeuib.ttf")] {
        if let Ok(bytes) = std::fs::read(PathBuf::from(&windir).join("Fonts").join(file)) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
        }
    }
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if fonts.font_data.contains_key("segoe") {
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    let mut strong = fallback;
    for name in ["segoe-bold", "segoe-semibold"] {
        if fonts.font_data.contains_key(name) {
            strong.insert(0, name.into());
        }
    }
    fonts.families.insert(FontFamily::Name("bold".into()), strong);
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = BG;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.slider_trailing_fill = true;
    for w in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.noninteractive,
    ] {
        w.corner_radius = CornerRadius::same(6);
    }
    style.visuals.widgets.inactive.bg_fill = TRACK;
    style.visuals.widgets.inactive.weak_bg_fill = CARD;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xf3, 0xf4, 0xf6);
    style.visuals.widgets.hovered.bg_fill = ACCENT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0xd4, 0xd7, 0xdd));
    style.visuals.widgets.active.bg_fill = ACCENT;
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(16.0, 6.0);
    style.spacing.slider_width = 150.0;
    style.spacing.interact_size.y = 22.0;
    style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
    ctx.set_style(style);
}

struct SettingsApp {
    cfg: Config,
    welcome: bool,
    screenshot: Option<PathBuf>,
    frame: u32,
    shot_requested: bool,
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = i18n::texts(&self.cfg.language);
        self.screenshot_step(ctx);

        egui::TopBottomPanel::bottom("buttons")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 24, right: 24, top: 12, bottom: 16 }))
            .show(ctx, |ui| {
                let line_y = ui.max_rect().top() - 12.0;
                ui.painter().hline(ui.max_rect().x_range(), line_y, Stroke::new(1.0_f32, BORDER));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let save = egui::Button::new(RichText::new(t.save).font(bold(14.0)).color(Color32::WHITE))
                        .fill(ACCENT)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(8))
                        .min_size(Vec2::new(112.0, 34.0));
                    if ui.add(save).clicked() {
                        self.save();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    let cancel = egui::Button::new(RichText::new(t.cancel).color(TEXT))
                        .fill(CARD)
                        .stroke(Stroke::new(1.0_f32, BORDER))
                        .corner_radius(CornerRadius::same(8))
                        .min_size(Vec2::new(100.0, 34.0));
                    if ui.add(cancel).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 24, right: 24, top: 22, bottom: 8 }))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.header(ui, t);
                    shortcuts(ui, t);

                    section(ui, t.sec_general);
                    card(ui, |ui| {
                        row(ui, t.startup, t.startup_desc, |ui| {
                            toggle(ui, &mut self.cfg.auto_startup);
                        });
                        divider(ui);
                        row(ui, t.language, t.language_desc, |ui| {
                            segmented(ui, "lang", &mut self.cfg.language, &[("en", "English"), ("fr", "Fran\u{e7}ais")]);
                        });
                    });

                    section(ui, t.sec_mouse);
                    card(ui, |ui| {
                        row(ui, t.sensitivity, t.sensitivity_desc, |ui| {
                            segmented(
                                ui,
                                "sens",
                                &mut self.cfg.sensitivity,
                                &[("low", t.low), ("medium", t.medium), ("high", t.high)],
                            );
                        });
                        divider(ui);
                        row(ui, t.zone, t.zone_desc, |ui| {
                            segmented(ui, "zone", &mut self.cfg.zone, &[("taskbar", t.zone_taskbar), ("edge", t.zone_edge)]);
                        });
                        divider(ui);
                        row(ui, t.middle_click, t.middle_desc, |ui| {
                            toggle(ui, &mut self.cfg.middle_click);
                        });
                    });

                    section(ui, t.sec_hint);
                    card(ui, |ui| {
                        row(ui, t.visual, t.visual_desc, |ui| {
                            toggle(ui, &mut self.cfg.visual_feedback);
                        });
                        divider(ui);
                        ui.add_enabled_ui(self.cfg.visual_feedback, |ui| {
                            row(ui, t.opacity, "", |ui| {
                                let mut pct = (self.cfg.indicator_opacity * 100.0).round() as i32;
                                ui.add_sized(
                                    [40.0, 20.0],
                                    egui::Label::new(RichText::new(format!("{pct}%")).color(MUTED)),
                                );
                                let slider = egui::Slider::new(&mut pct, 10..=80).show_value(false);
                                if ui.add(slider).changed() {
                                    self.cfg.indicator_opacity = pct as f64 / 100.0;
                                }
                            });
                            hint_preview(ui, &self.cfg, t.hint_title, t.hint_sub);
                        });
                    });
                    ui.add_space(8.0);
                });
            });
    }
}

impl SettingsApp {
    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frame += 1;
        ctx.request_repaint();
        if self.frame >= 120 && !self.shot_requested {
            self.shot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            save_png(&path, &img);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.frame > 600 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn header(&self, ui: &mut Ui, t: &i18n::Texts) {
        ui.horizontal(|ui| {
            app_mark(ui, 40.0);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(RichText::new(t.app_name).font(bold(20.0)).color(TEXT));
                ui.label(RichText::new(t.tagline).size(13.0).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                status_pill(ui, t);
            });
        });
        if self.welcome {
            ui.add_space(12.0);
            egui::Frame::new()
                .fill(ACCENT_SOFT)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(t.welcome_text).color(Color32::from_rgb(0x1e, 0x4f, 0xb8)));
                });
        }
        ui.add_space(14.0);
    }

    fn save(&mut self) {
        self.cfg.first_run = false;
        let _ = self.cfg.save();
        if self.cfg.auto_startup != startup::is_enabled() {
            startup::set(self.cfg.auto_startup);
        }
        helper_notify();
    }
}

// ---------------------------------------------------------------- building blocks

/// The app's mark: two overlapping desktops on a soft accent tile.
fn app_mark(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 10.0, ACCENT_SOFT);
    let inner = rect.shrink(size * 0.22);
    let back = Rect::from_min_max(inner.lerp_inside(Vec2::new(0.28, 0.0)), inner.lerp_inside(Vec2::new(1.0, 0.68)));
    let front = Rect::from_min_max(inner.lerp_inside(Vec2::new(0.0, 0.32)), inner.lerp_inside(Vec2::new(0.72, 1.0)));
    p.rect_filled(back, 3.0, Color32::from_rgb(0xa9, 0xc4, 0xf7));
    p.rect_stroke(front, 3.0, Stroke::new(2.0_f32, ACCENT_SOFT), egui::StrokeKind::Outside);
    p.rect_filled(front, 3.0, ACCENT);
}

fn status_pill(ui: &mut Ui, t: &i18n::Texts) {
    let running = helper_running();
    let (fg, bg, text) = if running {
        (Color32::from_rgb(0x15, 0x80, 0x3d), Color32::from_rgb(0xe3, 0xf5, 0xe9), t.running)
    } else {
        (MUTED, Color32::from_rgb(0xec, 0xee, 0xf1), t.stopped)
    };
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.5), fg);
    let size = Vec2::new(galley.size().x + 30.0, 24.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 12.0, bg);
    p.circle_filled(egui::pos2(rect.left() + 12.0, rect.center().y), 3.5, fg);
    p.galley(egui::pos2(rect.left() + 21.0, rect.center().y - galley.size().y / 2.0), galley, fg);
}

/// The three gestures, as small pills.
fn shortcuts(ui: &mut Ui, t: &i18n::Texts) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (key, what) in [(t.key_scroll, t.chip_scroll), (t.key_ctrl, t.chip_ctrl), (t.key_middle, t.chip_middle)] {
            egui::Frame::new()
                .fill(CARD)
                .stroke(Stroke::new(1.0_f32, BORDER))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(9, 4))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(key).font(bold(12.5)).color(ACCENT));
                        ui.label(RichText::new("\u{2192}").size(12.5).color(MUTED));
                        ui.label(RichText::new(what).size(12.5).color(TEXT));
                    });
                });
        }
    });
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).font(bold(13.0)).color(MUTED));
    ui.add_space(2.0);
}

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(16, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
}

fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0_f32, DIVIDER));
}

/// One setting: name and short description on the left, its control on the right.
fn row(ui: &mut Ui, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    let height = if desc.is_empty() { 44.0 } else { 54.0 };
    let width = ui.available_width();
    ui.allocate_ui_with_layout(Vec2::new(width, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(Vec2::new(width, height));
        if desc.is_empty() {
            ui.label(RichText::new(title).size(14.0).color(TEXT));
        } else {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(title).size(14.0).color(TEXT));
                ui.label(RichText::new(desc).size(12.0).color(MUTED));
            });
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            control(ui);
        });
    });
}

/// An on/off switch.
fn toggle(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let size = Vec2::new(40.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let enabled = ui.is_enabled();
    let p = ui.painter();
    let mut track = if *on { ACCENT } else { Color32::WHITE };
    let mut border = if *on { ACCENT } else { Color32::from_rgb(0x8a, 0x8f, 0x99) };
    let mut knob = if *on { Color32::WHITE } else { Color32::from_rgb(0x5f, 0x64, 0x6e) };
    if !enabled {
        track = track.gamma_multiply(0.4);
        border = border.gamma_multiply(0.4);
        knob = knob.gamma_multiply(0.5);
    }
    p.rect(rect, 11.0, track, Stroke::new(1.0_f32, border), egui::StrokeKind::Inside);
    let r = if response.hovered() { 7.0 } else { 6.0 };
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    p.circle_filled(egui::pos2(x, rect.center().y), r, knob);
    response
}

/// A row of mutually exclusive choices in one rounded pill.
fn segmented(ui: &mut Ui, id: &str, value: &mut String, options: &[(&str, &str)]) {
    let font = FontId::proportional(13.0);
    let galleys: Vec<_> = options
        .iter()
        .map(|(key, label)| {
            let colour = if value == key { ACCENT } else { MUTED };
            ui.painter().layout_no_wrap(label.to_string(), font.clone(), colour)
        })
        .collect();
    let pad = 14.0;
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad * 2.0).collect();
    let total = Vec2::new(widths.iter().sum::<f32>() + 6.0, 32.0);
    let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
    ui.painter().rect_filled(rect, 8.0, Color32::from_rgb(0xf0, 0xf1, 0xf4));
    let mut x = rect.left() + 3.0;
    for (i, ((key, _), galley)) in options.iter().zip(galleys).enumerate() {
        let seg = Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), Vec2::new(widths[i], rect.height() - 6.0));
        x += widths[i];
        let resp = ui.interact(seg, ui.id().with(id).with(i), Sense::click());
        if resp.clicked() {
            *value = key.to_string();
        }
        let selected = value == key;
        let p = ui.painter();
        if selected {
            p.rect_filled(seg.translate(Vec2::new(0.0, 1.0)), 6.0, Color32::from_black_alpha(14));
            p.rect_filled(seg, 6.0, CARD);
        } else if resp.hovered() {
            p.rect_filled(seg, 6.0, Color32::from_rgb(0xe6, 0xe8, 0xec));
        }
        let colour = if selected { ACCENT } else { MUTED };
        let pos = seg.center() - galley.size() / 2.0;
        p.galley(pos, galley, colour);
    }
}

/// A small preview of the hint bubble at the chosen opacity, above a taskbar.
fn hint_preview(ui: &mut Ui, cfg: &Config, title: &str, sub: &str) {
    ui.add_space(4.0);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 100.0), Sense::hover());
    let p = ui.painter();
    let area = rect.shrink2(Vec2::new(0.0, 6.0));
    p.rect_filled(area, 8.0, Color32::from_rgb(0xe9, 0xee, 0xf6));
    let bar = Rect::from_min_max(egui::pos2(area.left(), area.bottom() - 14.0), area.right_bottom());
    p.rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }, Color32::from_rgb(0x20, 0x22, 0x27));

    // Same look as the real bubble: dark card, blue badge, title and a second line.
    let a = cfg.alpha() as f32 / 255.0;
    let fade = |c: Color32| c.gamma_multiply(a);
    let title_g = p.layout_no_wrap(title.to_string(), bold(13.0), fade(Color32::WHITE));
    let sub_g = p.layout_no_wrap(sub.to_string(), FontId::proportional(11.5), fade(Color32::from_rgb(0xae, 0xb4, 0xc0)));
    let text_w = title_g.size().x.max(sub_g.size().x);
    let size = Vec2::new(12.0 * 2.0 + 30.0 + 10.0 + 4.0 + text_w, 20.0 + 30.0_f32.max(title_g.size().y + sub_g.size().y + 1.0));
    let bubble = Rect::from_center_size(egui::pos2(area.center().x, area.top() + 8.0 + size.y / 2.0), size);
    p.rect(bubble, 14.0, fade(Color32::from_rgb(0x1c, 0x1e, 0x24)), Stroke::new(1.0_f32, fade(Color32::from_rgb(0x3a, 0x3d, 0x46))), egui::StrokeKind::Inside);
    let badge = Rect::from_min_size(egui::pos2(bubble.left() + 12.0, bubble.center().y - 15.0), Vec2::splat(30.0));
    p.rect_filled(badge, 8.0, fade(ACCENT));
    let back = Rect::from_min_size(badge.min + Vec2::new(10.5, 8.0), Vec2::new(13.0, 10.0));
    let front = back.translate(Vec2::new(-4.0, 4.0));
    p.rect_filled(back, 3.0, fade(Color32::from_rgb(0xa9, 0xc4, 0xf7)));
    p.rect(front, 3.0, fade(Color32::WHITE), Stroke::new(1.0_f32, fade(ACCENT)), egui::StrokeKind::Outside);
    let tx = badge.right() + 10.0;
    let total = title_g.size().y + 1.0 + sub_g.size().y;
    let ty = bubble.center().y - total / 2.0;
    let th = title_g.size().y;
    p.galley(egui::pos2(tx, ty), title_g, Color32::WHITE);
    p.galley(egui::pos2(tx, ty + th + 1.0), sub_g, Color32::WHITE);
}

fn save_png(path: &PathBuf, img: &egui::ColorImage) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let [w, h] = img.size;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for c in &img.pixels {
        bytes.extend_from_slice(&c.to_srgba_unmultiplied());
    }
    if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
        let _ = buf.save(path);
    }
}

fn helper_running() -> bool {
    #[cfg(windows)]
    {
        crate::helper::is_running()
    }
    #[cfg(not(windows))]
    {
        false
    }
}
