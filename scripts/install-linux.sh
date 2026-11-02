#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Installiert Knipsdruck aus dem entpackten Linux-Archiv für den aktuellen Benutzer:
# Programme nach ~/.local/lib/knipsdruck, Verknüpfungen in ~/.local/bin,
# Startmenü-Eintrag und Icon nach ~/.local/share. Danach steht Knipsdruck mit Icon im Anwendungsmenü.
#   ./install.sh              installieren (erneut ausführen = aktualisieren)
#   ./install.sh --uninstall  wieder entfernen (Einstellungen bleiben erhalten)
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
LIB="$HOME/.local/lib/knipsdruck"
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
ICONS="$DATA/icons/hicolor"

refresh() {
  command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" || true
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -rf "$LIB"
  rm -f "$BIN/knipsdruck" "$BIN/knipsdruck-cli" "$APPS/knipsdruck.desktop" "$ICONS/256x256/apps/knipsdruck.png"
  refresh
  echo "Knipsdruck entfernt."
  exit 0
fi

mkdir -p "$LIB" "$BIN" "$APPS" "$ICONS/256x256/apps"
install -m755 "$HERE/knipsdruck" "$HERE/knipsdruck-cli" "$LIB/"
ln -sf "$LIB/knipsdruck" "$BIN/knipsdruck"
ln -sf "$LIB/knipsdruck-cli" "$BIN/knipsdruck-cli"
install -m644 "$HERE/knipsdruck.png" "$ICONS/256x256/apps/knipsdruck.png"
# Exec mit vollem Pfad: ~/.local/bin steht nicht bei jeder Desktop-Umgebung im Suchpfad
sed "s|^Exec=knipsdruck|Exec=\"$LIB/knipsdruck\"|" "$HERE/knipsdruck.desktop" > "$APPS/knipsdruck.desktop"
chmod 644 "$APPS/knipsdruck.desktop"
refresh

echo "Knipsdruck installiert: $LIB"
echo "Start über das Anwendungsmenü oder mit: knipsdruck"
