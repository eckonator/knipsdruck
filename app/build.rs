// SPDX-License-Identifier: MIT OR Apache-2.0
//! Commit-Kurzkennung für die Seite „Über“; unter Windows zusätzlich Programm-Icon und Versionsangaben in die .exe.

fn main() {
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=KD_GIT_HASH={hash}");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads");

    // Nur beim Bauen auf Windows für Windows (braucht dort rc.exe); der Gegencheck vom Mac aus lässt es weg
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../assets/knipsdruck.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/knipsdruck.ico")
            .set("ProductName", "Knipsdruck")
            .set("FileDescription", "Knipsdruck")
            .set("CompanyName", "Markus Eckert")
            .set("LegalCopyright", "© 2026 Markus Eckert · MIT OR Apache-2.0")
            .set("OriginalFilename", "Knipsdruck.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Icon nicht eingebettet: {e}");
        }
    }
}
