// SPDX-License-Identifier: MIT OR Apache-2.0
//! „Vom Handy holen“: QR-Code, Status und Hilfe. Der Server (kd-phone) läuft nur, solange der Dialog offen ist.

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Sense, vec2};
use kd_phone::{Event, Session};

use crate::app::App;
use crate::dialogs::{head, modal_frame};
use crate::i18n::{tr, trf};
use crate::settings::Settings;
use crate::theme;
use crate::widgets::{self, Button};

pub struct PhoneUi {
    session: Option<Session>,
    qr: egui::TextureHandle,
    qr_for: usize,
    connected: bool,
    received: usize,
    opened: Instant,
    help: bool,
    expired: bool,
    /// Rückgängig-Schritt der Sitzung schon angelegt
    snapshot_done: bool,
}

impl PhoneUi {
    pub fn start(ctx: &egui::Context, settings: &mut Settings) -> std::io::Result<Self> {
        let session = new_session(ctx, settings)?;
        log::info!("Handy-Empfang unter {}", session.url());
        let qr = qr_texture(ctx, &session);
        let qr_for = session.selected();
        Ok(PhoneUi {
            session: Some(session),
            qr,
            qr_for,
            connected: false,
            received: 0,
            opened: Instant::now(),
            help: false,
            expired: false,
            snapshot_done: false,
        })
    }

    /// Zeigt den Dialog; false = geschlossen (der Server endet mit dem Drop der Sitzung).
    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> bool {
        if let Some(s) = &self.session {
            for ev in s.events() {
                match ev {
                    Event::Connected => self.connected = true,
                    Event::Photo { name, data } => {
                        self.connected = true;
                        self.received += 1;
                        if !self.snapshot_done {
                            app.doc.snapshot();
                            self.snapshot_done = true;
                        }
                        app.add_one(name, data, None);
                    }
                    Event::Expired => self.expired = true,
                }
            }
        }
        if !self.connected && self.opened.elapsed() > Duration::from_secs(30) {
            self.help = true;
        }
        let mut keep = true;
        let mut renew = false;
        egui::Modal::new(egui::Id::new("handy"))
            .frame(modal_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(800.0);
                if head(
                    ui,
                    ctx,
                    tr("Fotos vom Handy holen"),
                    Some(tr("Handy und Computer müssen im selben WLAN sein.")),
                ) {
                    keep = false;
                }
                ui.add_space(14.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(480.0);
                        for (i, (bold, rest)) in [
                            (tr("Kamera am Handy öffnen"), tr("und auf den Code richten.")),
                            (tr("Auf den Link tippen,"), tr("der im Kamerabild erscheint.")),
                            (
                                tr("Fotos aufnehmen oder auswählen"),
                                tr("– sie erscheinen sofort hier."),
                            ),
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                                ui.painter().circle_filled(r.center(), 14.0, theme::accent(ctx));
                                widgets::centered_text(ui, r, &(i + 1).to_string(), 14.0, theme::accent_ink(ctx));
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(*bold)
                                        .family(crate::fonts::semibold())
                                        .color(theme::ink(ctx)),
                                );
                                ui.label(RichText::new(*rest).color(theme::ink(ctx)));
                            });
                            ui.add_space(4.0);
                        }
                        ui.add_space(14.0);
                        self.status(ui, ctx);
                        ui.add_space(12.0);
                        let toggle = if self.help {
                            tr("Klappt nicht? ▾")
                        } else {
                            tr("Klappt nicht? ▸")
                        };
                        if widgets::link(ui, toggle).clicked() {
                            self.help = !self.help;
                        }
                        if self.help {
                            ui.add_space(6.0);
                            self.help_ui(ui, ctx);
                        }
                    });
                    ui.add_space(24.0);
                    ui.vertical(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(272.0, 272.0), Sense::hover());
                        ui.painter()
                            .add(theme::shadow(ctx).as_shape(r, egui::CornerRadius::same(16)));
                        ui.painter().rect_filled(r, 16, Color32::WHITE);
                        if self.expired {
                            widgets::centered_text(
                                ui,
                                r.translate(vec2(0.0, -30.0)),
                                tr("Der Code ist abgelaufen."),
                                15.0,
                                Color32::from_rgb(0x1a, 0x1c, 0x22),
                            );
                            let br = egui::Rect::from_center_size(r.center() + vec2(0.0, 20.0), vec2(200.0, 40.0));
                            if ui
                                .put(br, |ui: &mut egui::Ui| {
                                    Button::new(tr("Neuen Code zeigen")).primary().show(ui)
                                })
                                .clicked()
                            {
                                renew = true;
                            }
                        } else {
                            let q = r.shrink(16.0);
                            ui.painter().image(
                                self.qr.id(),
                                q,
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                    });
                });
                ui.add_space(16.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if self.received > 0 {
                        let n = app.doc.pages.len();
                        ui.label(
                            RichText::new(trf("{} im Dokument", &[&crate::app::pages_text(n)]))
                                .color(theme::muted(ctx)),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if Button::new(tr("Fertig")).primary().show(ui).clicked() {
                            keep = false;
                        }
                    });
                });
            });
        if renew {
            match new_session(ctx, &mut app.settings) {
                Ok(s) => {
                    self.qr = qr_texture(ctx, &s);
                    self.qr_for = s.selected();
                    self.session = Some(s);
                    self.expired = false;
                    self.opened = Instant::now();
                }
                Err(e) => app.toast(trf("Der Empfang vom Handy ließ sich nicht starten: {}", &[&e])),
            }
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        keep
    }

    fn status(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            let t = ctx.input(|i| i.time) as f32;
            let (color, text) = if self.expired {
                (theme::muted(ctx), tr("Sitzung beendet").to_string())
            } else if self.received > 0 {
                let what = if self.received == 1 {
                    tr("1 Foto empfangen").to_string()
                } else {
                    trf("{} Fotos empfangen", &[&self.received])
                };
                (theme::ok(ctx), what)
            } else if self.connected {
                (theme::ok(ctx), tr("Handy verbunden").to_string())
            } else {
                // pulsierender Punkt, solange gewartet wird
                let pulse = (t * 2.5).sin() * 0.5 + 0.5;
                ui.painter().circle_filled(
                    r.center(),
                    6.0 + 4.0 * pulse,
                    theme::accent(ctx).gamma_multiply(0.25 * (1.0 - pulse)),
                );
                (theme::accent(ctx), tr("Warte auf das Handy …").to_string())
            };
            ui.painter().circle_filled(r.center(), 6.0, color);
            ui.label(
                RichText::new(text)
                    .family(crate::fonts::semibold())
                    .color(theme::ink(ctx)),
            );
            if let Some(s) = &self.session
                && !self.expired
            {
                let left = s.remaining().as_secs();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mmss = format!("{}:{:02}", left / 60, left % 60);
                    ui.label(RichText::new(trf("Code gilt noch {} Min.", &[&mmss])).color(theme::muted(ctx)));
                });
            }
        });
    }

    fn help_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let muted = theme::muted(ctx);
        for line in [
            tr("Ist das Handy im WLAN? Mit mobilen Daten klappt es nicht."),
            tr("Ein Gäste-WLAN trennt Geräte oft voneinander – dann das normale WLAN nehmen."),
            tr("Fragt der Computer nach der Firewall, bitte „Zulassen“ wählen."),
        ] {
            ui.label(RichText::new(format!("•  {line}")).color(muted));
        }
        let Some(session) = &mut self.session else { return };
        if session.addresses().len() > 1 {
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Andere Adresse versuchen:")).color(muted));
            ui.horizontal_wrapped(|ui| {
                let mut pick = None;
                for (i, a) in session.addresses().iter().enumerate() {
                    let label = format!("{} · {}", tr_label(&a.label), a.ip);
                    if widgets::choice(ui, session.selected() == i, &label).clicked() {
                        pick = Some(i);
                    }
                }
                if let Some(i) = pick {
                    session.select(i);
                }
            });
            if session.selected() != self.qr_for {
                self.qr = qr_texture(ctx, session);
                self.qr_for = session.selected();
            }
        }
        ui.add_space(4.0);
        ui.label(RichText::new(tr("Oder diese Adresse im Browser des Handys eingeben:")).color(muted));
        ui.label(RichText::new(session.url()).monospace().color(theme::ink(ctx)));
    }
}

fn new_session(ctx: &egui::Context, settings: &mut Settings) -> std::io::Result<Session> {
    let wake = {
        let ctx = ctx.clone();
        move || ctx.request_repaint()
    };
    let name = kd_phone::computer_name();
    let session = match settings.phone_port {
        Some(port) => Session::start_on(port, name, wake)?,
        None => Session::start(name, wake)?,
    };
    settings.phone_port = Some(session.port());
    Ok(session)
}

/// QR-Code als Textur mit Ruhezone (4 Module), scharf skaliert.
fn qr_texture(ctx: &egui::Context, s: &Session) -> egui::TextureHandle {
    let m = s.qr();
    let quiet = 4;
    let n = m.size + 2 * quiet;
    let mut pixels = vec![Color32::WHITE; n * n];
    for y in 0..m.size {
        for x in 0..m.size {
            if m.dark[y * m.size + x] {
                pixels[(y + quiet) * n + x + quiet] = Color32::BLACK;
            }
        }
    }
    let img = egui::ColorImage::new([n, n], pixels);
    ctx.load_texture("qr", img, egui::TextureOptions::NEAREST)
}

/// Bezeichnungen der Netzwerke aus kd-phone (deutsch) übersetzen.
fn tr_label(label: &str) -> String {
    match label {
        "WLAN" => tr("WLAN").into(),
        "LAN" => tr("LAN").into(),
        "Netzwerk" => tr("Netzwerk").into(),
        "VPN" => "VPN".into(),
        "Lokal" => tr("Lokal").into(),
        other => other.into(),
    }
}
