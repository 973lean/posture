# Posture

A quiet focus companion that lives in your tray (menu bar on macOS). No window, ~2 MB.

Three gentle reminders, each with its own interval (15 min – 2 hours, or Off):

- **Posture** — *Head up.* Bad posture causes a forward-leaning neck.
- **Water** — *Drink some water.* Even mild dehydration can dull your focus.
- **Walk** — *Time for a walk.* A few minutes on your feet resets your focus.

When one is due, a small card with a stick-figure illustration appears in the middle of your main
screen with every monitor softly blurred behind it (a snapshot taken the moment it appears; macOS
asks for Screen Recording permission once). It stays at least 1 s, then click or press any key to
dismiss; it fades on its own after 7 s. Reminders due together wait their turn.

Click the tray icon → **Open Focus…** for the settings panel: three columns (Posture, Water, Walk),
each with its interval list, time until the next reminder and a Preview. It stays open while you
change things; click elsewhere or press Esc to close it. Also there: **Pause all**, **Start at login**,
**Check for updates**, **Quit**. Your choices are remembered.

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

