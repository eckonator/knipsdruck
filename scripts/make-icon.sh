#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Erzeugt aus assets/icon.svg die Icon-Dateien:
#   assets/icon-1024.png, assets/icon-256.png (Fenster-Icon, Linux), assets/Knipsdruck.icns (macOS),
#   assets/knipsdruck.ico (Windows, in die .exe eingebettet)
# Braucht Google Chrome zum Rendern des SVG. Nur nach Änderungen am SVG nötig.
set -euo pipefail

cd "$(dirname "$0")/../assets"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

"$CHROME" --headless=new --disable-gpu --hide-scrollbars --window-size=1024,1024 \
  --force-device-scale-factor=1 --default-background-color=00000000 \
  --screenshot="$PWD/icon-1024.png" "file://$PWD/icon.svg" 2>/dev/null

sips -z 256 256 icon-1024.png --out icon-256.png >/dev/null

SET=$(mktemp -d)/Knipsdruck.iconset
mkdir -p "$SET"
for s in 16 32 128 256 512; do
  sips -z $s $s icon-1024.png --out "$SET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) icon-1024.png --out "$SET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$SET" -o "Knipsdruck.icns"

# Windows-ICO: PNG-Bilder in einem ICO-Container (seit Windows Vista erlaubt)
for s in 16 32 48 256; do
  sips -z $s $s icon-1024.png --out "$SET/ico_$s.png" >/dev/null
done
python3 - "$SET" <<'PY'
import struct, sys, pathlib
d = pathlib.Path(sys.argv[1])
sizes = [16, 32, 48, 256]
images = [(d / f"ico_{s}.png").read_bytes() for s in sizes]
header = struct.pack("<HHH", 0, 1, len(images))
offset = 6 + 16 * len(images)
entries = b""
for s, img in zip(sizes, images):
    entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(img), offset)
    offset += len(img)
pathlib.Path("knipsdruck.ico").write_bytes(header + entries + b"".join(images))
PY
echo "Fertig: assets/Knipsdruck.icns, assets/knipsdruck.ico, assets/icon-*.png"
