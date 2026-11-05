# Posture

A quiet reminder to keep your head back. One screen, one setting, ~2 MB.

Set the interval, press **Start**. Every interval you get a system notification:
*Chin back. Head over shoulders.*

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

`resources/index.html` also runs on its own in any browser (e.g. GitHub Pages).
