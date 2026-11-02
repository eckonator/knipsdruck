// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ende-zu-Ende-Test des Handy-Servers über echte TCP-Verbindungen auf localhost.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use kd_phone::{Event, Session};

/// Kleinstes gültiges PNG (1×1 Pixel).
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\
\0\0\0\rIDATx\x9cc\xf8\x0f\0\0\x01\x01\0\x05\x18\xd8N\0\0\0\0IEND\xaeB`\x82";

struct Reply {
    status: u16,
    body: Vec<u8>,
}

impl Reply {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Schickt eine handgeschriebene HTTP/1.1-Anfrage und liest die Antwort bis zum Verbindungsende.
fn request(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    if method == "POST" && !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-length")) {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let _ = stream.shutdown(Shutdown::Write);
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    parse(&raw)
}

fn parse(raw: &[u8]) -> Reply {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("Antwortkopf");
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Reply {
        status,
        body: raw[split + 4..].to_vec(),
    }
}

fn start() -> (Session, String, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let session = Session::start("Büro-PC <Test>".into(), move || {
        counter.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    let url = session.url();
    let base = url[url.find("/u/").unwrap()..].to_string();
    (session, base, wakes)
}

#[test]
fn full_session() {
    let (session, base, wakes) = start();
    let port = session.port();
    assert!(port >= 49152);
    assert!(!session.addresses().is_empty());
    assert!(session.remaining() > Duration::from_secs(14 * 60));

    let page = request(port, "GET", &base, &[], b"");
    assert_eq!(page.status, 200);
    let html = page.text();
    assert!(html.contains("Knipsdruck"));
    assert!(html.contains("Büro-PC &lt;Test&gt;"));
    assert!(html.contains(&format!("content=\"{base}\"")));
    assert_eq!(session.events(), vec![Event::Connected]);
    request(port, "GET", &base, &[], b"");
    assert!(session.events().is_empty(), "Connected nur einmal");

    let wrong = request(port, "GET", "/u/aaaaaaaaaaaaaaaaaaaaaaaaaa", &[], b"");
    assert_eq!(wrong.status, 404);
    assert!(wrong.body.is_empty());
    assert_eq!(request(port, "GET", "/favicon.ico", &[], b"").status, 404);
    assert_eq!(request(port, "GET", &format!("{base}/foto"), &[], b"").status, 404);

    let status = request(port, "GET", &format!("{base}/status"), &[], b"");
    assert_eq!(status.status, 200);
    assert_eq!(status.text(), r#"{"aktiv":true}"#);

    for (n, (encoded, name)) in [("Seite%201.png", "Seite 1.png"), ("%C3%9Cbung.jpg", "Übung.jpg")]
        .iter()
        .enumerate()
    {
        let r = request(port, "POST", &format!("{base}/foto"), &[("X-Dateiname", encoded)], PNG);
        assert_eq!(r.status, 200);
        assert_eq!(r.text(), format!(r#"{{"ok":true,"n":{}}}"#, n + 1));
        let events = session.events();
        assert_eq!(
            events,
            vec![Event::Photo {
                name: name.to_string(),
                data: PNG.to_vec()
            }]
        );
    }
    assert_eq!(session.received(), 2);

    let garbage = request(
        port,
        "POST",
        &format!("{base}/foto"),
        &[("X-Dateiname", "a.jpg")],
        b"kein Bild",
    );
    assert_eq!(garbage.status, 400);
    assert_eq!(garbage.text(), r#"{"ok":false,"fehler":"kein_bild"}"#);
    let empty = request(port, "POST", &format!("{base}/foto"), &[], b"");
    assert_eq!(empty.status, 400);

    let big = request(
        port,
        "POST",
        &format!("{base}/foto"),
        &[("Content-Length", "60000000")],
        b"",
    );
    assert_eq!(big.status, 413);
    assert_eq!(big.text(), r#"{"ok":false,"fehler":"zu_gross"}"#);

    let nameless = request(port, "POST", &format!("{base}/foto"), &[], PNG);
    assert_eq!(nameless.status, 200);
    assert_eq!(
        session.events(),
        vec![Event::Photo {
            name: "Foto 3.png".into(),
            data: PNG.to_vec()
        }]
    );
    assert_eq!(session.received(), 3);
    assert!(wakes.load(Ordering::SeqCst) >= 4);

    drop(session);
    assert!(TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(500)).is_err());
}

#[test]
fn start_on_prefers_given_port() {
    let (first, _, _) = start();
    let taken = first.port();
    let second = Session::start_on(taken, "X".into(), || {}).unwrap();
    assert_ne!(second.port(), taken, "belegter Port → zufälliger");
    let port = second.port();
    drop(second);
    let third = Session::start_on(port, "X".into(), || {}).unwrap();
    assert_eq!(third.port(), port);
}

#[test]
fn qr_matches_url() {
    let (session, _, _) = start();
    let qr = session.qr();
    assert_eq!(qr.dark.len(), qr.size * qr.size);
    assert!(session.url().starts_with("http://"));
}
