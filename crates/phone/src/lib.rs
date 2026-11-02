// SPDX-License-Identifier: MIT OR Apache-2.0
//! „Vom Handy holen“: Fotos vom Handy im selben WLAN direkt in die App übertragen.
//!
//! Die App zeigt einen QR-Code mit der Adresse einer kleinen Upload-Seite, die dieser Server ausliefert.
//! Das Handy braucht weder Internet noch eine eigene App. Schutz: zufälliger 128-Bit-Schlüssel im Pfad,
//! nur Gegenstellen aus privaten Netzen, Größen- und Mengengrenzen, nur echte Bilddateien, Ablauf nach
//! 15 Minuten ohne Aktivität. Auf der Festplatte wird nichts gespeichert.

mod address;
mod hostname;
mod magic;
mod server;
mod text;
mod token;

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use address::Address;
pub use hostname::computer_name;
pub use server::{IDLE_TIMEOUT, MAX_PHOTO_BYTES, MAX_PHOTOS};

use server::{Shared, Worker};

/// Vorlage der Handyseite; Platzhalter `{{BASIS}}` und `{{RECHNER}}`.
const PAGE_TEMPLATE: &str = include_str!("../assets/phone.html");
/// So oft wird ein zufälliger Port versucht, bevor aufgegeben wird.
const PORT_ATTEMPTS: usize = 32;

/// Meldung des Servers an die App.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Die Handyseite wurde zum ersten Mal geladen.
    Connected,
    /// Ein empfangenes Bild (Originaldatei, unverändert) mit dem bereinigten Dateinamen vom Handy.
    Photo { name: String, data: Vec<u8> },
    /// 15 Minuten ohne Aktivität: Der Server hat sich beendet.
    Expired,
}

/// QR-Code als Raster, zeilenweise, ohne Ruhezone (`true` = dunkles Modul).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrMatrix {
    pub size: usize,
    pub dark: Vec<bool>,
}

/// Eine laufende Übertragungssitzung; beim Verwerfen wird der Server beendet.
pub struct Session {
    addresses: Vec<Address>,
    selected: usize,
    port: u16,
    token: String,
    events: Receiver<Event>,
    shared: Arc<Shared>,
    server: Weak<tiny_http::Server>,
    thread: Option<JoinHandle<()>>,
}

impl Session {
    /// Startet den Server auf einem zufälligen Port (49152–65535) in eigenem Thread. `wake` wird nach jedem
    /// Event aufgerufen (die App ruft darin `ctx.request_repaint()`). `computer_name` erscheint auf der
    /// Handyseite („Verbunden mit „Büro-PC““).
    pub fn start(computer_name: String, wake: impl Fn() + Send + Sync + 'static) -> io::Result<Session> {
        Self::launch(bind_random()?, computer_name, Box::new(wake))
    }

    /// Wie [`Session::start`], aber bevorzugt auf `port` (z. B. dem zuletzt benutzten, damit eine
    /// Firewall-Freigabe weiter passt). Ist er belegt, wird ein zufälliger Port gewählt; siehe [`Session::port`].
    pub fn start_on(port: u16, computer_name: String, wake: impl Fn() + Send + Sync + 'static) -> io::Result<Session> {
        let listener = match bind(port) {
            Ok(l) => l,
            Err(e) => {
                log::info!("Port {port} nicht verfügbar ({e}), nehme einen zufälligen");
                bind_random()?
            }
        };
        Self::launch(listener, computer_name, Box::new(wake))
    }

    fn launch(listener: TcpListener, computer_name: String, wake: Box<dyn Fn() + Send + Sync>) -> io::Result<Session> {
        let port = listener.local_addr()?.port();
        let token = token::new_token()?;
        let server = tiny_http::Server::from_listener(listener, None).map_err(io::Error::other)?;
        let server = Arc::new(server);
        let shared = Arc::new(Shared::new());
        let (tx, events) = mpsc::channel();
        let worker = Worker {
            server: Arc::clone(&server),
            shared: Arc::clone(&shared),
            events: tx,
            wake,
            html: render_page(&computer_name, &token),
            token: token.clone(),
            port,
            connected: false,
        };
        let thread = std::thread::Builder::new()
            .name("kd-phone".into())
            .spawn(move || worker.run())?;
        let addresses = address::local_addresses();
        let selected = addresses.iter().position(|a| a.preferred).unwrap_or(0);
        log::info!("Handy-Server läuft auf Port {port}");
        Ok(Session {
            addresses,
            selected,
            port,
            token,
            events,
            shared,
            server: Arc::downgrade(&server),
            thread: Some(thread),
        })
    }

    /// Mögliche Adressen des Computers, die wahrscheinlich richtige zuerst; nie leer.
    pub fn addresses(&self) -> &[Address] {
        &self.addresses
    }

    /// Index der gewählten Adresse in [`Session::addresses`].
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Wählt eine andere Adresse (z. B. wenn das Handy im Gäste-WLAN ist); ungültige Indizes werden ignoriert.
    pub fn select(&mut self, i: usize) {
        if i < self.addresses.len() {
            self.selected = i;
        }
    }

    /// Der tatsächlich benutzte Port (zum Merken für [`Session::start_on`]).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Adresse der Handyseite: `http://<ip>:<port>/u/<schlüssel>`.
    pub fn url(&self) -> String {
        let ip = self.addresses.get(self.selected).map_or(Ipv4Addr::LOCALHOST, |a| a.ip);
        format!("http://{ip}:{}/u/{}", self.port, self.token)
    }

    /// QR-Code für [`Session::url`].
    pub fn qr(&self) -> QrMatrix {
        qr_matrix(&self.url())
    }

    /// Alle Meldungen seit dem letzten Aufruf; blockiert nicht.
    pub fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Anzahl der bisher empfangenen Fotos.
    pub fn received(&self) -> usize {
        self.shared.received.load(Ordering::SeqCst)
    }

    /// Zeit bis zum Ablauf wegen Inaktivität; null, wenn der Server schon beendet ist.
    pub fn remaining(&self) -> Duration {
        if self.shared.finished.load(Ordering::SeqCst) {
            return Duration::ZERO;
        }
        IDLE_TIMEOUT.saturating_sub(self.shared.idle())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(server) = self.server.upgrade() {
            server.unblock();
        }
        let Some(thread) = self.thread.take() else { return };
        // Hängt ein Handy mitten in einer Übertragung, soll die Oberfläche nicht einfrieren.
        let deadline = Instant::now() + Duration::from_secs(1);
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if thread.is_finished() {
            let _ = thread.join();
        } else {
            log::warn!("Handy-Server beendet sich verzögert (Übertragung läuft noch)");
        }
    }
}

fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))
}

/// Bindet einen zufälligen Port; bei Belegung wird ein anderer versucht.
fn bind_random() -> io::Result<TcpListener> {
    let mut last = io::Error::other("kein freier Port gefunden");
    for _ in 0..PORT_ATTEMPTS {
        match bind(token::random_port()?) {
            Ok(listener) => return Ok(listener),
            Err(e) if matches!(e.kind(), io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied) => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// Setzt Upload-Pfad und (maskierten) Computernamen in die Seite ein. Der Name kommt zuletzt, damit ein
/// Name wie „{{BASIS}}“ nicht selbst ersetzt wird.
fn render_page(computer_name: &str, token: &str) -> String {
    PAGE_TEMPLATE
        .replace("{{BASIS}}", &format!("/u/{token}"))
        .replace("{{RECHNER}}", &text::html_escape(computer_name))
}

fn qr_matrix(data: &str) -> QrMatrix {
    match qrcode::QrCode::with_error_correction_level(data, qrcode::EcLevel::M) {
        Ok(code) => QrMatrix {
            size: code.width(),
            dark: code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect(),
        },
        Err(e) => {
            log::error!("QR-Code nicht erzeugbar: {e}");
            QrMatrix {
                size: 0,
                dark: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_placeholders() {
        let html = render_page("<Büro> & {{BASIS}}", "abc");
        assert!(html.contains("/u/abc"));
        assert!(html.contains("&lt;Büro&gt; &amp; {{BASIS}}"));
        assert!(!html.contains("{{RECHNER}}"));
        assert!(!html.contains("<Büro>"));
    }

    #[test]
    fn qr_is_square() {
        let qr = qr_matrix("http://192.168.178.23:51234/u/abcdefghijklmnopqrstuvwxyz");
        assert!(qr.size >= 21);
        assert_eq!(qr.dark.len(), qr.size * qr.size);
        assert!(qr.dark.iter().any(|d| *d));
    }
}
