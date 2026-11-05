# Posture

A quiet reminder to keep your head back. No window — it lives in your tray (menu bar on macOS). ~2 MB.

Click the tray icon and check an interval (10–60 min). When it's time, a small card slides in at the
top of your screen — *Head up. Chin back, ears over shoulders.* — and fades away after a few seconds
(click to dismiss). Looking up at it is the point.

Tray menu: interval, **Pause**, **Show reminder now**, **Quit**. Your choice is remembered.

## Download

Get the zip for your system from the [latest release](https://github.com/973lean/posture/releases/latest)
and keep both files in it together, in a folder you can write to:

| System | File |
|---|---|
| Windows | `Posture-windows.zip` |
| macOS | `Posture-mac.zip` |
| Linux | `Posture-linux.zip` |

macOS: the app is unsigned. First launch: right-click → Open, or run
`xattr -d com.apple.quarantine Posture-mac_universal && chmod +x Posture-mac_universal`.

## Updates

Every push to `main` builds and publishes a new release (`.github/workflows/release.yml`).
The app checks for one 30 s after launch and every 6 hours, swaps it in, and restarts quietly.
The current version is shown at the bottom of the tray menu.

Updates cover the app itself (`resources/`, config). A new Neutralino engine version needs a fresh download.
Auto-update needs the repository (or at least its releases) to be public.

## Build

Built with [Neutralinojs](https://neutralino.js.org) — the UI is `resources/index.html`.

```bash
npx @neutralinojs/neu update
npx @neutralinojs/neu build --release
```

