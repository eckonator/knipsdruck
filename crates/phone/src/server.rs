// SPDX-License-Identifier: MIT OR Apache-2.0
//! Der kleine HTTP-Server im eigenen Thread: beantwortet die Anfragen der Handyseite nacheinander.

use std::io::{Cursor, Read};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tiny_http::{Header, Method, Request, Response, Server};

use crate::Event;
use crate::address::is_allowed_peer;
use crate::magic;
use crate::text::{clean_file_name, percent_decode};
use crate::token::same;

/// Nach so langer Ruhe beendet sich die Sitzung.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Höchstgröße eines einzelnen Fotos.
pub const MAX_PHOTO_BYTES: usize = 50 * 1024 * 1024;
/// Höchstzahl an Fotos je Sitzung.
pub const MAX_PHOTOS: usize = 200;
/// So oft schaut die Schleife nach, ob sie aufhören soll.
const POLL: Duration = Duration::from_millis(200);

/// Zustand, den Server-Thread und [`crate::Session`] teilen.
pub struct Shared {
    pub received: AtomicUsize,
    pub stop: AtomicBool,
    pub finished: AtomicBool,
    last_activity: Mutex<Instant>,
}

impl Shared {
    pub fn new() -> Self {
        Shared {
            received: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            last_activity: Mutex::new(Instant::now()),
        }
    }

    fn touch(&self) {
        if let Ok(mut t) = self.last_activity.lock() {
            *t = Instant::now();
        }
    }

    /// Zeit seit der letzten Anfrage mit gültigem Schlüssel.
    pub fn idle(&self) -> Duration {
        self.last_activity.lock().map(|t| t.elapsed()).unwrap_or(IDLE_TIMEOUT)
    }
}

/// Was eine Anfrage will.
#[derive(Debug, PartialEq, Eq)]
enum Route {
    Page,
    Photo,
    Status,
    NotFound,
}

/// Ordnet Methode und Pfad zu; alles Unbekannte (auch ein falscher Schlüssel) ist [`Route::NotFound`].
fn route(method: &Method, url: &str, token: &str) -> Route {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let Some(rest) = path.strip_prefix("/u/") else {
        return Route::NotFound;
    };
    let (given, sub) = rest.split_once('/').unwrap_or((rest, ""));
    if !same(given, token) {
        return Route::NotFound;
    }
    match (method, sub) {
        (Method::Get, "") => Route::Page,
        (Method::Post, "foto") => Route::Photo,
        (Method::Get, "status") => Route::Status,
        _ => Route::NotFound,
    }
}

/// Grund für eine abgelehnte Übertragung: HTTP-Status und Kürzel, das die Handyseite übersetzt.
type Refusal = (u16, &'static str);

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("gültiger statischer Header")
}

fn with_common_headers(response: Response<Cursor<Vec<u8>>>, content_type: &str) -> Response<Cursor<Vec<u8>>> {
    response
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
        .with_header(header("Referrer-Policy", "no-referrer"))
}

fn json(status: u16, body: String) -> Response<Cursor<Vec<u8>>> {
    with_common_headers(
        Response::from_string(body).with_status_code(status),
        "application/json; charset=utf-8",
    )
}

/// Inhaltsrichtlinie der Handyseite: nur eigenes Inline-CSS/-JS, Verbindungen nur zu uns selbst.
const PAGE_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; \
    img-src data:; connect-src 'self'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'";

fn page(html: &str) -> Response<Cursor<Vec<u8>>> {
    with_common_headers(Response::from_string(html), "text/html; charset=utf-8")
        .with_header(header("Content-Security-Policy", PAGE_CSP))
}

fn respond(request: Request, response: Response<Cursor<Vec<u8>>>) {
    if let Err(e) = request.respond(response) {
        log::debug!("Antwort an das Handy nicht gesendet: {e}");
    }
}

fn respond_empty(request: Request, status: u16) {
    if let Err(e) = request.respond(Response::empty(status)) {
        log::debug!("Antwort an das Handy nicht gesendet: {e}");
    }
}

/// Weckt den Annahme-Thread von tiny_http, damit er den Port sicher freigibt (unter Windows gelingt
/// tiny_http das selbst nicht, weil es sich mit 0.0.0.0 verbinden will).
fn wake_accept_thread(port: u16) {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let _ = TcpStream::connect_timeout(&addr, Duration::from_millis(200));
}

/// Der Server-Thread mit allem, was er zum Antworten braucht.
pub struct Worker {
    pub server: Arc<Server>,
    pub shared: Arc<Shared>,
    pub events: Sender<Event>,
    pub wake: Box<dyn Fn() + Send + Sync>,
    /// Fertige Handyseite (Platzhalter bereits ersetzt)
    pub html: String,
    pub token: String,
    pub port: u16,
    pub connected: bool,
}

impl Worker {
    /// Hauptschleife bis zum Beenden durch [`crate::Session`] oder bis zum Ablauf.
    pub fn run(mut self) {
        while !self.shared.stop.load(Ordering::SeqCst) {
            if self.shared.idle() >= IDLE_TIMEOUT {
                log::info!(
                    "Handy-Sitzung nach {} min ohne Aktivität beendet",
                    IDLE_TIMEOUT.as_secs() / 60
                );
                self.emit(Event::Expired);
                break;
            }
            match self.server.recv_timeout(POLL) {
                Ok(Some(request)) => self.handle(request),
                Ok(None) => {}
                Err(e) => log::debug!("Verbindung nicht angenommen: {e}"),
            }
        }
        self.shared.finished.store(true, Ordering::SeqCst);
        let port = self.port;
        drop(self);
        wake_accept_thread(port);
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        (self.wake)();
    }

    fn handle(&mut self, request: Request) {
        let allowed = request.remote_addr().is_some_and(|a| is_allowed_peer(a.ip()));
        if !allowed {
            log::warn!("Anfrage von fremder Adresse abgelehnt: {:?}", request.remote_addr());
            return respond_empty(request, 403);
        }
        match route(request.method(), request.url(), &self.token) {
            Route::Page => self.serve_page(request),
            Route::Status => {
                self.shared.touch();
                respond(request, json(200, r#"{"aktiv":true}"#.into()));
            }
            Route::Photo => self.serve_upload(request),
            Route::NotFound => respond_empty(request, 404),
        }
    }

    fn serve_page(&mut self, request: Request) {
        self.shared.touch();
        if !self.connected {
            self.connected = true;
            self.emit(Event::Connected);
        }
        respond(request, page(&self.html));
    }

    fn serve_upload(&mut self, mut request: Request) {
        self.shared.touch();
        let result = self.receive_photo(&mut request);
        self.shared.touch();
        let response = match result {
            Ok(n) => json(200, format!(r#"{{"ok":true,"n":{n}}}"#)),
            Err((status, code)) => {
                log::info!("Foto vom Handy abgelehnt: {code}");
                json(status, format!(r#"{{"ok":false,"fehler":"{code}"}}"#))
            }
        };
        respond(request, response);
    }

    /// Liest, prüft und meldet ein Foto; liefert die neue Gesamtzahl.
    fn receive_photo(&mut self, request: &mut Request) -> Result<usize, Refusal> {
        if self.shared.received.load(Ordering::SeqCst) >= MAX_PHOTOS {
            return Err((413, "zu_viele"));
        }
        let raw_name = file_name_header(request);
        let data = read_body(request, &self.shared.stop)?;
        let kind = magic::detect(&data).ok_or((400, "kein_bild"))?;
        let n = self.shared.received.fetch_add(1, Ordering::SeqCst) + 1;
        let name = clean_file_name(&percent_decode(&raw_name), kind, n);
        log::info!("Foto vom Handy empfangen: {name} ({} KB)", data.len() / 1024);
        self.emit(Event::Photo { name, data });
        Ok(n)
    }
}

fn file_name_header(request: &Request) -> String {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Dateiname"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default()
}

/// Liest den Rumpf mit Größengrenze; prüft vorab die angekündigte Länge.
fn read_body(request: &mut Request, stop: &AtomicBool) -> Result<Vec<u8>, Refusal> {
    let announced = request.body_length();
    if announced.is_some_and(|n| n > MAX_PHOTO_BYTES) {
        return Err((413, "zu_gross"));
    }
    let mut data = Vec::with_capacity(announced.unwrap_or(0));
    let limit = MAX_PHOTO_BYTES as u64 + 1;
    let reader = Stoppable {
        inner: request.as_reader(),
        stop,
    };
    reader
        .take(limit)
        .read_to_end(&mut data)
        .map_err(|_| (400, "abgebrochen"))?;
    if data.len() > MAX_PHOTO_BYTES {
        return Err((413, "zu_gross"));
    }
    if data.is_empty() {
        return Err((400, "leer"));
    }
    Ok(data)
}

/// Bricht das Lesen ab, sobald die Sitzung beendet wird.
struct Stoppable<'a, R> {
    inner: R,
    stop: &'a AtomicBool,
}

impl<R: Read> Read for Stoppable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.stop.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("Sitzung beendet"));
        }
        self.inner.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "abcdefghijklmnopqrstuvwxyz";

    #[test]
    fn routes() {
        assert_eq!(route(&Method::Get, &format!("/u/{T}"), T), Route::Page);
        assert_eq!(route(&Method::Get, &format!("/u/{T}?x=1"), T), Route::Page);
        assert_eq!(route(&Method::Post, &format!("/u/{T}/foto"), T), Route::Photo);
        assert_eq!(route(&Method::Get, &format!("/u/{T}/status"), T), Route::Status);
        assert_eq!(route(&Method::Get, &format!("/u/{T}/foto"), T), Route::NotFound);
        assert_eq!(route(&Method::Post, &format!("/u/{T}"), T), Route::NotFound);
        assert_eq!(route(&Method::Get, "/u/falsch", T), Route::NotFound);
        assert_eq!(route(&Method::Get, "/u/falsch/status", T), Route::NotFound);
        assert_eq!(route(&Method::Get, "/favicon.ico", T), Route::NotFound);
        assert_eq!(route(&Method::Get, "/", T), Route::NotFound);
        assert_eq!(route(&Method::Get, &format!("/u/{T}/status/x"), T), Route::NotFound);
    }
}
