# Focus

A quiet focus companion that lives in your tray (menu bar on macOS). No main window, ~2 MB.

Four gentle reminders, each with its own colour, stick figure and schedule:

- **Posture** (green) — *Head up.* Bad posture causes a forward-leaning neck.
- **Water** (blue) — *Drink some water.* Even mild dehydration can dull your focus.
- **Walk** (white) — *Time for a walk.* A few minutes on your feet resets your focus.
- **Caffeine** (violet) — *Caffeine time.* With a timing tip and **Took it / Not now**. The next
  countdown only starts when you tap *Took it*; *Not now* (or no answer in 20 s) asks again in 30 min.
  - **Interval**: e.g. every 4 hr, counted from your last dose.
  - **Daily**: 1–2 set times. A late first dose shifts the second by the same amount; a dose more
    than 4 hours late is skipped.

When one is due, a card appears in the middle of your main screen with every monitor softly blurred
behind it (a snapshot taken the moment it appears; macOS asks for Screen Recording permission once).
It stays at least 1 s; click or press any key to dismiss, or it fades after 7 s. Reminders due
together wait their turn.

## Using it

Click the tray icon:

- **Open Focus…** — the panel, opening out to the left of where you clicked. Click elsewhere or press
  Esc to close it.
  - Each column: countdown, **Skip to next** (show it now, restart its timer), **Reset** (restart its
    timer), a **Custom** box (`25`, `90`, `1.5h` + Enter) with up to 5 saved times (hover → × to
    remove), and **Off**.
  - Header: **Reset all**, **Pause all**. Footer: **Start at login**, **Check for updates**, **Quit**.
- **Skip to next posture / water / walk / caffeine**, **Reset all timers**, **Pause all**, **Quit**.

## Download

From the [latest release](https://github.com/973lean/posture/releases/latest). Unzip and keep both
files together in a folder you can write to (e.g. Documents, not Program Files):

| System | File |
|---|---|
| Windows | `Focus-windows.zip` |
| macOS | `Focus-mac.zip` |
| Linux | `Focus-linux.zip` |

macOS: the app is unsigned. First launch: right-click → Open, or run
`xattr -d com.apple.quarantine Focus-mac_universal && chmod +x Focus-mac_universal`.

## Updates

Every push to `main` builds the app, then **runs it on a GitHub Windows desktop** before publishing:
it opens the panel from a bottom-right tray click, shows the posture and caffeine reminders over a
blurred desktop, and updates itself to a newer build. If the update doesn't land, nothing is released.
Screenshots of the latest check are on the [`ci-preview`](https://github.com/973lean/posture/tree/ci-preview) branch.

Installed apps check 30 s after launch and every 6 hours (or via **Check for updates**), download the
new `resources.neu`, quit, swap it in and relaunch. A new Neutralino engine version needs a fresh download.

## Build

Built with [Neutralinojs](https://neutralino.js.org). UI in `resources/`, scheduling logic in
`resources/core.js` (tested with `node --test test/core.test.js`).

```bash
npx @neutralinojs/neu update
npx @neutralinojs/neu run
```
