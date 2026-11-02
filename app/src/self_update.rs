// SPDX-License-Identifier: MIT OR Apache-2.0
//! Neue Version aus der App heraus installieren, damit niemand für Updates ein Terminal braucht.
//!
//! macOS: Die ZIP des Releases wird per `curl` geholt (ohne Quarantäne-Markierung, also ohne
//! Gatekeeper-Rückfrage), geprüft, ob sie mit demselben Zertifikat signiert ist wie diese App, und an
//! deren Stelle gelegt; eine kleine Shell wartet, bis dieser Prozess endet, und öffnet die neue App.
//! Ist der Ordner nicht beschreibbar, fragt macOS nach einem Administrator-Passwort.
//! Windows: Mit dem Installer eingerichtet (neben der exe liegt `unins000.exe`), wird der neue Installer
//! geladen und still gestartet (`/SILENT … /RELAUNCH`, siehe scripts/knipsdruck.iss); er schließt diese App
//! und startet die neue. Aus der ZIP ausgepackt, kommen die Dateien der neuen ZIP in den Ordner der laufenden
//! `Knipsdruck.exe`. Laufende Dateien lassen sich nicht überschreiben, aber umbenennen; die alten werden
//! beim nächsten Start gelöscht.
//! Linux: Mit dem `install.sh` des Archivs installiert (Programm in `~/.local/lib/knipsdruck`), wird
//! das neue Archiv entpackt und sein `install.sh` ausgeführt. Kein Passwort nötig.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

use eframe::egui;

/// Bekommt die neue Version beim Start nach einem Update mit.
pub const AFTER_UPDATE_FLAG: &str = "--after-update";

/// Endung, unter der Windows-Dateien beim Austausch beiseitegelegt werden.
const OLD_SUFFIX: &str = ".alt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Downloading,
    Installing,
    /// Fertig; die App beendet sich jetzt, die neue startet.
    Restart,
    Failed(String),
}

/// Ende des Dateinamens im Release für dieses System, wenn sich dieser Build selbst aktualisieren kann.
pub fn asset_suffix() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        bundle().map(|_| "-macOS.zip")
    } else if cfg!(windows) {
        portable_exe().map(|exe| {
            if installed_with_setup(&exe) {
                "-Windows-Setup.exe"
            } else {
                "-Windows.zip"
            }
        })
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        installed_for_user().map(|_| "-Linux-x86_64.tar.gz")
    } else {
        None
    }
}

pub fn start(url: String, ctx: egui::Context) -> Receiver<Progress> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("kd-self-update".into())
        .spawn(move || {
            let report = |p: Progress| {
                let _ = tx.send(p);
                ctx.request_repaint();
            };
            match run(&url, &report) {
                Ok(()) => report(Progress::Restart),
                Err(e) => {
                    log::warn!("Update fehlgeschlagen: {e}");
                    report(Progress::Failed(e));
                }
            }
        })
        .ok();
    rx
}

/// Räumt nach einem Update unter Windows die beim Austausch umbenannten alten Dateien weg.
pub fn clean_up() {
    let Some(dir) = portable_exe().and_then(|exe| exe.parent().map(Path::to_path_buf)) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.filter_map(Result::ok).map(|e| e.path()) {
        if path.to_string_lossy().ends_with(OLD_SUFFIX) {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        // kein Konsolenfenster aufblitzen lassen
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn checked(cmd: &mut Command, what: &str) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{what}: {}", err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    checked(
        command("curl")
            .args(["-fsSL", "--max-time", "900", "-A", "Knipsdruck-Update", "-o"])
            .arg(to)
            .arg(url),
        "Download",
    )
    .map(|_| ())
}

fn work_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("knipsdruck-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Der entpackte Ordner im Arbeitsordner, in dem `marker` liegt.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn unpacked_with(dir: &Path, marker: &str) -> Result<PathBuf, String> {
    std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.join(marker).is_file())
        .ok_or_else(|| format!("im Download ist kein {marker}"))
}

/// `…/Knipsdruck.app`, aus der dieses Programm läuft.
pub fn bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundle = exe.ancestors().nth(3)?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// Die laufende `Knipsdruck.exe` aus der Windows-ZIP (nicht die nackte `knipsdruck.exe` aus dem Build).
pub fn portable_exe() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    (exe.file_name()? == "Knipsdruck.exe").then_some(exe)
}

/// Mit dem Inno-Setup-Installer eingerichtet? Dann liegt dessen Deinstallationsprogramm neben der exe.
pub fn installed_with_setup(exe: &Path) -> bool {
    exe.parent().is_some_and(|dir| dir.join("unins000.exe").is_file())
}

/// Wohin `install.sh` das Programm gelegt hat (`~/.local/lib/knipsdruck`), wenn es von dort läuft.
pub fn installed_for_user() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let lib = PathBuf::from(std::env::var_os("HOME")?).join(".local/lib/knipsdruck");
    let exe = std::env::current_exe().ok()?;
    (exe.parent()? == lib && exe.file_name()? == "knipsdruck").then_some(exe)
}

/// Startet `program` (mit `AFTER_UPDATE_FLAG`), sobald dieser Prozess beendet ist.
#[cfg(unix)]
fn relaunch(open: &str, program: &Path) -> Result<(), String> {
    command("/bin/sh")
        .args([
            "-c",
            &format!("while kill -0 \"$1\" 2>/dev/null; do sleep 0.2; done; {open}"),
        ])
        .arg(program)
        .arg(std::process::id().to_string())
        .arg(AFTER_UPDATE_FLAG)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn run(url: &str, report: &impl Fn(Progress)) -> Result<(), String> {
    let bundle = bundle().ok_or("läuft nicht aus einem App-Bundle")?;
    install_app(url, &bundle, report)?;
    relaunch("open \"$0\" --args \"$2\"", &bundle)
}

/// Legt die App aus der ZIP unter `url` an die Stelle von `bundle`.
#[cfg(target_os = "macos")]
fn install_app(url: &str, bundle: &Path, report: &impl Fn(Progress)) -> Result<(), String> {
    let dir = work_dir()?;
    report(Progress::Downloading);
    let zip = dir.join("update.zip");
    download(url, &zip)?;
    report(Progress::Installing);
    let unpacked = dir.join("unpacked");
    checked(
        command("ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked),
        "Entpacken",
    )?;
    let new = unpacked.join("Knipsdruck.app");
    if !new.is_dir() {
        return Err("im Download ist keine Knipsdruck.app".into());
    }
    // Nur annehmen, was dasselbe Zertifikat signiert hat (bei ad-hoc signierten Builds nicht prüfbar)
    let mut verify = command("codesign");
    verify.args(["--verify", "--deep", "--strict"]);
    if let Some(requirement) = signer_requirement(bundle) {
        verify.arg(format!("-R={requirement}"));
    }
    checked(verify.arg(&new), "Signatur der neuen Version")?;
    replace(bundle, &new)?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Der Zertifikatsteil der Designated Requirement dieser App, falls sie einen hat.
#[cfg(target_os = "macos")]
fn signer_requirement(bundle: &Path) -> Option<String> {
    let out = command("codesign").args(["-d", "-r-"]).arg(bundle).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let line = text.lines().find_map(|l| l.strip_prefix("designated => "))?;
    line.contains("certificate root").then(|| line.trim().to_string())
}

#[cfg(target_os = "macos")]
fn replace(bundle: &Path, new: &Path) -> Result<(), String> {
    let parent = bundle.parent().ok_or("kein Ordner")?;
    let old = parent.join(".Knipsdruck-alt.app");
    let _ = std::fs::remove_dir_all(&old);
    if std::fs::rename(bundle, &old).is_ok() {
        let copied = checked(command("ditto").arg(new).arg(bundle), "Installieren");
        if let Err(e) = copied {
            let _ = std::fs::remove_dir_all(bundle);
            let _ = std::fs::rename(&old, bundle);
            return Err(e);
        }
        let _ = std::fs::remove_dir_all(&old);
        return Ok(());
    }
    // Für diesen Benutzer nicht beschreibbar: macOS fragt nach einem Administrator-Passwort
    let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
    let script = format!(
        "rm -rf {old} && mv {bundle} {old} && ditto {new} {bundle} && rm -rf {old}",
        old = quote(&old),
        bundle = quote(bundle),
        new = quote(new),
    );
    let apple = format!(
        "do shell script \"{}\" with administrator privileges",
        script.replace('\\', "\\\\").replace('"', "\\\"")
    );
    checked(command("osascript").args(["-e", &apple]), "Installieren")
        .map(|_| ())
        .map_err(|e| if e.contains("-128") { "abgebrochen".into() } else { e })
}

#[cfg(windows)]
fn run(url: &str, report: &impl Fn(Progress)) -> Result<(), String> {
    let exe = portable_exe().ok_or("läuft nicht als Knipsdruck.exe")?;
    if installed_with_setup(&exe) {
        return run_setup(url, report);
    }
    let target = exe.parent().ok_or("kein Ordner")?.to_path_buf();
    let dir = work_dir()?;
    report(Progress::Downloading);
    let zip = dir.join("update.zip");
    download(url, &zip)?;
    report(Progress::Installing);
    // tar.exe gehört seit Windows 10 (1803) zum System und entpackt auch ZIP
    checked(command("tar").arg("-xf").arg(&zip).arg("-C").arg(&dir), "Entpacken")?;
    let unpacked = unpacked_with(&dir, "Knipsdruck.exe")?;
    let files: Vec<PathBuf> = std::fs::read_dir(&unpacked)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    // Vorhandene Dateien beiseitelegen (laufende lassen sich nicht überschreiben, aber umbenennen),
    // dann die neuen hineinkopieren; bei einem Fehler alles zurück
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let restore = |moved: &[(PathBuf, PathBuf)]| {
        for (current, old) in moved {
            let _ = std::fs::remove_file(current);
            let _ = std::fs::rename(old, current);
        }
    };
    for file in &files {
        let current = target.join(file.file_name().ok_or("Dateiname")?);
        if current.exists() {
            let mut old = current.clone().into_os_string();
            old.push(OLD_SUFFIX);
            let old = PathBuf::from(old);
            let _ = std::fs::remove_file(&old);
            if let Err(e) = std::fs::rename(&current, &old) {
                restore(&moved);
                return Err(format!(
                    "Ordner nicht beschreibbar ({e}); bitte von der Download-Seite installieren"
                ));
            }
            moved.push((current.clone(), old));
        }
        if let Err(e) = std::fs::copy(file, &current) {
            restore(&moved);
            return Err(format!("Installieren: {e}"));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    // Neue Version starten, sobald diese beendet ist
    let pid = std::process::id();
    let script = format!(
        "while (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 200 }}; Start-Process -FilePath '{}' -ArgumentList '{AFTER_UPDATE_FLAG}'",
        exe.display().to_string().replace('\'', "''")
    );
    command("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Neuen Installer laden und still starten; er schließt diese App und startet danach die neue Version.
#[cfg(windows)]
fn run_setup(url: &str, report: &impl Fn(Progress)) -> Result<(), String> {
    let dir = work_dir()?;
    report(Progress::Downloading);
    let setup = dir.join("Knipsdruck-Setup.exe");
    download(url, &setup)?;
    report(Progress::Installing);
    command(&setup.to_string_lossy())
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/RELAUNCH"])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Installer: {e}"))
}

#[cfg(target_os = "linux")]
fn run(url: &str, report: &impl Fn(Progress)) -> Result<(), String> {
    let exe = installed_for_user().ok_or("nicht mit install.sh installiert")?;
    let dir = work_dir()?;
    report(Progress::Downloading);
    let archive = dir.join("update.tar.gz");
    download(url, &archive)?;
    report(Progress::Installing);
    checked(
        command("tar").arg("-xzf").arg(&archive).arg("-C").arg(&dir),
        "Entpacken",
    )?;
    let unpacked = unpacked_with(&dir, "install.sh")?;
    checked(command("bash").arg(unpacked.join("install.sh")), "Installieren")?;
    let _ = std::fs::remove_dir_all(&dir);
    relaunch("exec \"$0\" \"$2\" >/dev/null 2>&1", &exe)
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn run(_url: &str, _report: &impl Fn(Progress)) -> Result<(), String> {
    Err("auf diesem System nicht möglich".into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Ersetzt eine signierte App durch einen anderen Build; braucht einen in `KD_TEST_APP` (scripts/bundle-macos.sh).
    #[test]
    #[ignore = "braucht eine gebündelte App"]
    fn replaces_a_signed_app() {
        let source = PathBuf::from(std::env::var("KD_TEST_APP").expect("KD_TEST_APP"));
        let dir = std::env::temp_dir().join(format!("knipsdruck-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Applications")).unwrap();
        let installed = dir.join("Applications/Knipsdruck.app");
        checked(command("ditto").arg(&source).arg(&installed), "copy").unwrap();
        std::fs::write(installed.join("Contents/old-marker"), "old").unwrap();
        let zip = dir.join("release.zip");
        checked(
            command("ditto")
                .args(["-c", "-k", "--keepParent"])
                .arg(&source)
                .arg(&zip),
            "zip",
        )
        .unwrap();
        install_app(&format!("file://{}", zip.display()), &installed, &|_| {}).unwrap();
        assert!(installed.join("Contents/MacOS/Knipsdruck").exists());
        assert!(!installed.join("Contents/old-marker").exists());
        assert!(!dir.join("Applications/.Knipsdruck-alt.app").exists());

        // Von jemand anderem signiert (hier: ad-hoc): abgelehnt, die installierte App bleibt
        let foreign = dir.join("foreign/Knipsdruck.app");
        checked(command("ditto").arg(&source).arg(&foreign), "copy").unwrap();
        checked(
            command("codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&foreign),
            "sign",
        )
        .unwrap();
        let zip = dir.join("foreign.zip");
        checked(
            command("ditto")
                .args(["-c", "-k", "--keepParent"])
                .arg(&foreign)
                .arg(&zip),
            "zip",
        )
        .unwrap();
        let err = install_app(&format!("file://{}", zip.display()), &installed, &|_| {}).unwrap_err();
        assert!(err.contains("Signatur"), "{err}");
        assert!(installed.join("Contents/MacOS/Knipsdruck").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
