# Posture

A quiet reminder to keep your head back. No window — it lives in your tray (menu bar on macOS). ~2 MB.

Click the tray icon and check an interval (10–60 min). When it's time, a small card slides in at the
top of your screen — *Head up. Chin back, ears over shoulders.* — and fades away after a few seconds
(click to dismiss). Looking up at it is the point.

Tray menu: interval, **Pause**, **Show reminder now**, **Quit**. Your choice is remembered.

## Download

Grab the binary for your system from `dist/Posture/` (or a release):

| System | File |
|---|---|
| Windows | `Posture-win_x64.exe` |
| macOS (Apple Silicon / Intel) | `Posture-mac_universal` |
| Linux | `Posture-linux_x64` |

macOS: the app is unsigned. First launch: right-click → Open, or run
`xattr -d com.apple.quarantine Posture-mac_universal && chmod +x Posture-mac_universal`.

## Build

Built with [Neutralinojs](https://neutralino.js.org) — the UI is `resources/index.html`.

```bash
npx @neutralinojs/neu update
npx @neutralinojs/neu build --release --embed-resources
```

