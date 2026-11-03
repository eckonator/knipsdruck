# Changelog

## 0.1.2

- **Settings redesigned:** sections with icons on the left (General, Paper, Print & PDF, About), the options in
  cards on the right. Clicking the paper size in the footer opens the *Paper* section directly.
- *Check for updates now* and *Clear history now* are proper buttons, the update check at startup is an
  on/off choice like the other options.

## 0.1.1

- **Searchable PDF is now on by default.** The text recognition model is loaded in the background when the
  app starts, so the first save no longer waits up to half a minute on macOS.
- Removing the last page switches back from *ID copy* to *One page per photo*, so the next photo is no longer
  shrunk to card size by mistake.
- Clearer settings icon (gear with eight distinct teeth).

## 0.1.0

First release.

- **Sheet detection and perspective correction** for phone photos of documents, also for sheets lying on a
  binder or on top of other sheets. Uncertain pages are marked *Please check*; the corners can be adjusted by
  hand with a magnifier, or the whole photo can be used.
- **Looks like a copy:** shadows and uneven lighting are evened out, paper becomes white, writing is sharpened.
  Color, grayscale or pure black and white.
- **Get photos from the phone** via QR code over the local network, without an app on the phone. Photos arrive
  in original quality.
- **Paper sizes** A3, A4, A5, A6, B5, US Letter, US Legal or custom (width × height in mm); portrait,
  landscape or automatic. Sheets are fitted, never distorted.
- **ID copy:** front and back of a card at actual size on one sheet.
- **Print** with the system dialog (macOS, Windows) or via CUPS (Linux), **save as PDF** in print or email
  quality, **send by email** with the PDF attached. Single pages from the page menu.
- **Searchable PDF** (optional) with the text recognition built into each system.
- **Recently used:** the last ten documents for 30 days (never ID copies), can be switched off.
- Drag and drop, paste, reorder, rotate, undo/redo; English and German; light and dark.
- Update check and **update from inside the app** (macOS, Windows installer, Linux with `install.sh`).
- Windows installer (per user, no admin rights needed).
