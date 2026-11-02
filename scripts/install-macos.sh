#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Installiert die neueste Knipsdruck-Version nach /Applications (macOS).
#
#   curl -fsSL https://raw.githubusercontent.com/eckonator/knipsdruck/main/scripts/install-macos.sh | bash
#
# Was curl herunterlädt, trägt keine Quarantäne-Markierung. Darum startet die App danach ohne
# Gatekeeper-Sperre, obwohl sie nicht von Apple notarisiert ist. Zur Sicherheit wird die Markierung
# trotzdem entfernt. Ausgaben auf Englisch, weil das Skript über GitHub verteilt wird.
set -euo pipefail

REPO="eckonator/knipsdruck"
APP_NAME="Knipsdruck.app"
# KD_INSTALL_DIR: anderes Ziel (zum Testen), KD_NO_OPEN=1: danach nicht starten
DEST="${KD_INSTALL_DIR:-/Applications}"

# Alles in einer Funktion, damit bei abgebrochenem Download (curl | bash) nichts halb ausgeführt wird
main() {
  say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
  fail() { printf '\033[31mError:\033[0m %s\n' "$*" >&2; exit 1; }

  [ "$(uname -s)" = "Darwin" ] || fail "This installer is for macOS. Other systems: https://github.com/$REPO/releases"

  if pgrep -f "$DEST/$APP_NAME/Contents/MacOS/" >/dev/null 2>&1; then
    fail "Knipsdruck is running. Please quit it first, then run this command again."
  fi

  say "Looking up the latest release"
  local api url version
  api=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest") \
    || fail "Could not reach GitHub. Download manually: https://github.com/$REPO/releases"
  url=$(printf '%s' "$api" | grep -o '"browser_download_url": *"[^"]*-macOS\.zip"' | head -n1 | sed 's/.*"\(https[^"]*\)"/\1/')
  [ -n "$url" ] || fail "No macOS download found in the latest release: https://github.com/$REPO/releases"
  version=$(printf '%s' "$api" | grep -o '"tag_name": *"[^"]*"' | head -n1 | sed 's/.*"\([^"]*\)"$/\1/')

  # Global, weil die Aufräum-Falle erst nach dem Ende von main läuft
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT

  say "Downloading Knipsdruck $version"
  curl -fL --progress-bar "$url" -o "$tmp/knipsdruck.zip" || fail "Download failed."
  ditto -x -k "$tmp/knipsdruck.zip" "$tmp/unpacked" || fail "Could not unpack the download."
  [ -d "$tmp/unpacked/$APP_NAME" ] || fail "The download does not contain $APP_NAME."

  # Ohne Schreibrecht auf /Applications (Standardkonto) mit Administrator-Passwort
  local sudo=""
  if [ ! -w "$DEST" ] || { [ -e "$DEST/$APP_NAME" ] && [ ! -w "$DEST/$APP_NAME" ]; }; then
    say "Administrator password needed to write to $DEST"
    sudo="sudo"
  fi

  say "Installing to $DEST/$APP_NAME"
  $sudo rm -rf "$DEST/$APP_NAME"
  $sudo ditto "$tmp/unpacked/$APP_NAME" "$DEST/$APP_NAME"
  $sudo xattr -dr com.apple.quarantine "$DEST/$APP_NAME" 2>/dev/null || true

  if [ -n "${KD_NO_OPEN:-}" ]; then
    say "Done."
  else
    say "Done. Starting Knipsdruck"
    open "$DEST/$APP_NAME"
  fi
}

main "$@"
