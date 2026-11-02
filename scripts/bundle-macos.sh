#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Baut „Knipsdruck.app“ für macOS nach dist/.
# Aufruf: scripts/bundle-macos.sh   (KD_SKIP_BUILD=1 überspringt das Bauen, z. B. für das Universal-Binary der CI)
# KD_SIGN_IDENTITY / KD_SIGN_KEYCHAIN: Zertifikat zum Signieren (Standard: ad-hoc), siehe scripts/signing-keychain.sh
# Liegt ein Ordner licenses/ vor (CI), kommen die Lizenztexte nach Contents/Resources.
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP="dist/Knipsdruck.app"

if [ -z "${KD_SKIP_BUILD:-}" ]; then
  cargo build --release -p knipsdruck
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/knipsdruck "$APP/Contents/MacOS/Knipsdruck"
cp assets/Knipsdruck.icns "$APP/Contents/Resources/Knipsdruck.icns"

# NSLocalNetworkUsageDescription: Text der Rückfrage, wenn das Handy Fotos über das lokale Netz schickt
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Knipsdruck</string>
  <key>CFBundleDisplayName</key><string>Knipsdruck</string>
  <key>CFBundleIdentifier</key><string>de.eckonator.knipsdruck</string>
  <key>CFBundleExecutable</key><string>Knipsdruck</string>
  <key>CFBundleIconFile</key><string>Knipsdruck</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>NSHumanReadableCopyright</key><string>© 2026 Markus Eckert · MIT OR Apache-2.0</string>
  <key>CFBundleDevelopmentRegion</key><string>de</string>
  <key>CFBundleLocalizations</key><array><string>de</string><string>en</string></array>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key><string>Damit du Fotos vom Handy an Knipsdruck schicken kannst.</string>
</dict>
</plist>
PLIST
plutil -lint -s "$APP/Contents/Info.plist"

if [ -d licenses ]; then
  cp licenses/* "$APP/Contents/Resources/"
fi

# Mit dem eigenen Zertifikat des Projekts signiert, wenn es eins gibt (scripts/signing-keychain.sh):
# Dann nimmt das Update per Knopf nur Versionen mit demselben Zertifikat an. Sonst ad-hoc, damit
# Apple-Silicon-Macs die App überhaupt starten.
SIGN=(--force --sign "${KD_SIGN_IDENTITY:--}")
if [ -n "${KD_SIGN_KEYCHAIN:-}" ]; then
  SIGN+=(--keychain "$KD_SIGN_KEYCHAIN")
fi
codesign "${SIGN[@]}" --identifier de.eckonator.knipsdruck "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
echo "Fertig: $APP"
