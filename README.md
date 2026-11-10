# Focus

A quiet focus companion that lives in your tray (menu bar on macOS). No main window.

Four gentle reminders, each with its own colour, stick figure and schedule:

- **Posture** (green) — *Head up.* Bad posture causes a forward-leaning neck.
- **Water** (blue) — *Drink some water.* Even mild dehydration can dull your focus.
- **Walk** (white) — *Time for a walk.* A few minutes on your feet resets your focus.
- **Caffeine** (violet) — *Caffeine time.* With a timing tip, and **Took it / Not now**. The next
  countdown only starts when you tap *Took it*; *Not now* (or no answer in 20 s) asks again in 30 min.
  - **Interval**: e.g. every 4 hr, counted from your last dose.
  - **Daily**: 1–2 set times. A late first dose shifts the second by the same amount; a dose more than
    4 hours late is skipped.

When one is due, a card appears in the middle of your main screen and every monitor is softly
blurred behind it (live, by Windows/macOS). It stays at least 1 s; click or press any key to dismiss,
or it fades after 7 s. Reminders due together wait their turn.

## Using it

- **Left-click the tray icon** to open the panel; click anywhere else or press Esc to close it.
  - Each column: countdown, **Skip to next** (show it now and restart its timer), **Reset**
    (restart its timer), a **Custom** box (`25`, `90`, `1.5h` + Enter) with up to 5 saved times
    (hover → × to remove), and **Off**.
  - Header: **Reset all**, **Pause all**. Footer: **Start at login**, **Check for updates**, **Quit**.
- **Right-click the tray icon** for a quick menu: skip to next posture / water / walk, reset all,
  pause all, quit (plus skip to next caffeine). (On Linux both clicks open this menu; use *Open Focus* for the panel.)

## Download

Get it from the [latest release](https://github.com/973lean/posture/releases/latest):

| System | File |
|---|---|
| Windows | `Focus_x.y.z_x64-setup.exe` (installs for your user, no admin) |
| macOS | `Focus_x.y.z_universal.dmg` |
| Linux | `Focus_x.y.z_amd64.AppImage` |

macOS: the app is unsigned. First launch: right-click → Open.

## Updates

Every push to `main` builds all three platforms and publishes a release
(`.github/workflows/release.yml`). The app checks 30 s after launch and every 6 hours, installs the
update and restarts. Updates are signed: the repo needs a `TAURI_SIGNING_PRIVATE_KEY` Actions secret
matching the public key in `src-tauri/tauri.conf.json`. Without it, releases still build but can't
auto-update.

## Build

Built with [Tauri 2](https://tauri.app). UI in `ui/`, tray/scheduler/windows in `src-tauri/src/main.rs`.

```bash
npm install
npm run dev
```
