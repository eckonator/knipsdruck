// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ecken anpassen: Originalfoto, außerhalb des Blatts abgedunkelt, vier ziehbare Griffe mit Lupe,
//! daneben eine Miniatur des Ergebnisses.

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use kd_core::{PageSettings, Quad, Source};

use crate::app::{self, App};
use crate::dialogs::{head, modal_frame};
use crate::i18n::tr;
use crate::theme;
use crate::widgets::Button;

/// Lange Seite des Fotos im Editor (Textur).
const PHOTO_PX: u32 = 2000;
const MINI_PX: f32 = 300.0;

pub struct CornerEditor {
    id: u64,
    quad: Quad,
    source: Source,
    photo: egui::TextureHandle,
    /// Pixel der Textur je Pixel des Originalfotos
    tex_scale: f32,
    photo_size: (u32, u32),
    drag: Option<usize>,
    mini: Option<egui::TextureHandle>,
    mini_dirty: bool,
}

impl CornerEditor {
    pub fn new(app: &App, ctx: &egui::Context, id: u64) -> Self {
        let page = app.doc.pages.iter().find(|p| p.id() == id).cloned();
        let page = page.expect("Seite für den Ecken-Editor");
        let img = &page.photo.image;
        let (w, h) = img.dimensions();
        let scale = (PHOTO_PX as f32 / w.max(h) as f32).min(1.0);
        let small = image::imageops::resize(
            &**img,
            ((w as f32 * scale) as u32).max(1),
            ((h as f32 * scale) as u32).max(1),
            image::imageops::FilterType::Triangle,
        );
        CornerEditor {
            id,
            quad: page.settings.quad,
            source: page.settings.source,
            photo: ctx.load_texture("ecken-foto", app::color_image(&small), egui::TextureOptions::LINEAR),
            tex_scale: scale,
            photo_size: (w, h),
            drag: None,
            mini: None,
            mini_dirty: true,
        }
    }

    /// Zeigt den Editor; false = geschlossen.
    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> bool {
        let Some(page) = app.doc.pages.iter().find(|p| p.id() == self.id).cloned() else {
            return false;
        };
        if self.mini_dirty && self.drag.is_none() {
            let ps = PageSettings {
                quad: self.quad,
                source: self.source,
                rotation: page.settings.rotation,
            };
            let img = crate::worker::render_preview(&page.photo.image, &ps, &app.doc_settings(), MINI_PX);
            self.mini = Some(ctx.load_texture("ecken-mini", app::color_image(&img), egui::TextureOptions::LINEAR));
            self.mini_dirty = false;
        }
        let mut keep = true;
        let mut apply = false;
        let screen = ctx.content_rect();
        let size = vec2((screen.width() - 80.0).min(1240.0), (screen.height() - 80.0).min(900.0));
        egui::Modal::new(egui::Id::new("ecken"))
            .frame(modal_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(size.x);
                if head(
                    ui,
                    ctx,
                    tr("Ecken anpassen"),
                    Some(tr("Ziehe die vier Punkte auf die Ecken des Blatts.")),
                ) {
                    keep = false;
                }
                ui.add_space(10.0);
                ui.horizontal_top(|ui| {
                    let stage_size = vec2(size.x - 290.0, size.y - 170.0);
                    self.stage(ui, stage_size);
                    ui.add_space(18.0);
                    ui.vertical(|ui| {
                        ui.set_width(250.0);
                        ui.label(
                            RichText::new(tr("ERGEBNIS"))
                                .size(13.0)
                                .family(crate::fonts::semibold())
                                .color(theme::muted(ctx)),
                        );
                        ui.add_space(6.0);
                        let (r, _) = ui.allocate_exact_size(vec2(250.0, 330.0), Sense::hover());
                        if let Some(m) = &self.mini {
                            let s = m.size_vec2();
                            let scale = (r.width() / s.x).min(r.height() / s.y);
                            let mr = Rect::from_center_size(pos2(r.center().x, r.top() + s.y * scale / 2.0), s * scale);
                            ui.painter()
                                .add(theme::shadow(ctx).as_shape(mr, egui::CornerRadius::same(2)));
                            app::paint_texture(ui.painter(), m, mr, 1.0);
                        }
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(tr("Beim Ziehen zeigt eine Lupe die Ecke vergrößert."))
                                .color(theme::muted(ctx)),
                        );
                    });
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if Button::new(tr("Automatisch"))
                        .show(ui)
                        .on_hover_text(tr("Den Vorschlag der Erkennung wiederherstellen"))
                        .clicked()
                    {
                        self.quad = page.photo.auto.quad;
                        self.source = page.photo.auto.source;
                        self.mini_dirty = true;
                    }
                    if Button::new(tr("Ganzes Foto"))
                        .show(ui)
                        .on_hover_text(tr("Das ganze Foto als Seite verwenden"))
                        .clicked()
                    {
                        self.quad = Quad::whole(self.photo_size.0, self.photo_size.1);
                        self.source = Source::WholePhoto;
                        self.mini_dirty = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if Button::new(tr("Fertig")).primary().show(ui).clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            apply = true;
                        }
                        if Button::new(tr("Abbrechen")).quiet().show(ui).clicked() {
                            keep = false;
                        }
                    });
                });
            });
        if apply {
            if self.quad != page.settings.quad || self.source != page.settings.source {
                app.set_quad(self.id, self.quad, self.source);
            }
            keep = false;
        }
        keep
    }

    fn stage(&mut self, ui: &mut egui::Ui, size: egui::Vec2) {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(rect, 12, Color32::from_rgb(0x11, 0x11, 0x13));
        let tex_size = self.photo.size_vec2();
        let fit = ((rect.width() - 40.0) / tex_size.x).min((rect.height() - 40.0) / tex_size.y);
        let img_rect = Rect::from_center_size(rect.center(), tex_size * fit);
        app::paint_texture(ui.painter(), &self.photo, img_rect, 1.0);
        // Foto-Pixel → Bildschirm
        let k = self.tex_scale * fit;
        let to_screen = |p: [f32; 2]| img_rect.min + vec2(p[0], p[1]) * k;
        let to_photo = |p: Pos2| {
            let v = (p - img_rect.min) / k;
            [
                v.x.clamp(-20.0, self.photo_size.0 as f32 + 20.0),
                v.y.clamp(-20.0, self.photo_size.1 as f32 + 20.0),
            ]
        };
        let pts: Vec<Pos2> = self.quad.0.iter().map(|&p| to_screen(p)).collect();
        // außerhalb abdunkeln: vier Vierecke zwischen Bildrand und Blatt
        let outer = [
            img_rect.left_top(),
            img_rect.right_top(),
            img_rect.right_bottom(),
            img_rect.left_bottom(),
        ];
        let mut mesh = egui::Mesh::default();
        let shade = Color32::from_black_alpha(150);
        for i in 0..4 {
            let j = (i + 1) % 4;
            let base = mesh.vertices.len() as u32;
            for p in [outer[i], outer[j], pts[j], pts[i]] {
                mesh.colored_vertex(p, shade);
            }
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        ui.painter().add(mesh);
        let accent = Color32::from_rgb(0x5b, 0x8f, 0xf9);
        ui.painter()
            .add(egui::Shape::closed_line(pts.clone(), Stroke::new(3.0, accent)));
        // Griffe
        let pointer = ui.input(|i| i.pointer.interact_pos());
        for (i, &pt) in pts.iter().enumerate() {
            let hr = Rect::from_center_size(pt, vec2(44.0, 44.0));
            let resp = ui.interact(hr, ui.id().with(("ecke", i)), Sense::drag());
            if resp.drag_started() {
                self.drag = Some(i);
            }
            if resp.dragged()
                && let Some(p) = pointer
            {
                self.quad.0[i] = to_photo(p);
                self.source = Source::Manual;
                self.mini_dirty = true;
            }
            if resp.drag_stopped() {
                self.drag = None;
            }
            let active = self.drag == Some(i) || resp.hovered();
            let (fill, ring) = if active {
                (accent, Color32::WHITE)
            } else {
                (Color32::WHITE, Color32::from_rgb(0x1f, 0x5b, 0xd6))
            };
            ui.painter().circle(pt, 15.0, fill, Stroke::new(4.0, ring));
            resp.on_hover_cursor(egui::CursorIcon::Grab);
        }
        if let Some(i) = self.drag {
            self.loupe(ui, rect, img_rect, pts[i], i);
        }
    }

    /// Dreifach vergrößerter Ausschnitt um die gezogene Ecke, neben dem Mauszeiger.
    fn loupe(&self, ui: &egui::Ui, stage: Rect, img_rect: Rect, at: Pos2, corner: usize) {
        let size = 170.0;
        let zoom = 3.0;
        let side = if corner == 1 || corner == 2 { -1.0 } else { 1.0 };
        let mut center = at + vec2(side * (size / 2.0 + 40.0), -(size / 2.0 + 30.0));
        center.x = center
            .x
            .clamp(stage.left() + size / 2.0 + 8.0, stage.right() - size / 2.0 - 8.0);
        center.y = center
            .y
            .clamp(stage.top() + size / 2.0 + 8.0, stage.bottom() - size / 2.0 - 8.0);
        let r = Rect::from_center_size(center, vec2(size, size));
        // sichtbarer Ausschnitt in UV-Koordinaten
        let uv_c = (at - img_rect.min) / img_rect.size();
        let half = vec2(size / zoom / img_rect.width(), size / zoom / img_rect.height()) / 2.0;
        let uv = Rect::from_min_max(
            pos2(uv_c.x - half.x, uv_c.y - half.y),
            pos2(uv_c.x + half.x, uv_c.y + half.y),
        );
        let p = ui.painter();
        p.rect_filled(r, 12, Color32::BLACK);
        p.image(self.photo.id(), r, uv, Color32::WHITE);
        p.line_segment(
            [r.center() - vec2(12.0, 0.0), r.center() + vec2(12.0, 0.0)],
            Stroke::new(2.0, Color32::from_rgb(0x5b, 0x8f, 0xf9)),
        );
        p.line_segment(
            [r.center() - vec2(0.0, 12.0), r.center() + vec2(0.0, 12.0)],
            Stroke::new(2.0, Color32::from_rgb(0x5b, 0x8f, 0xf9)),
        );
        p.rect_stroke(r, 12, Stroke::new(4.0, Color32::WHITE), egui::StrokeKind::Outside);
    }
}
