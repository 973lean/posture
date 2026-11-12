// Focus: a tray app with four reminders (posture, water, walk, caffeine).
// Rust owns the schedule, settings, tray and windows; the UI lives in ../ui.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use chrono::{Local, NaiveTime, TimeZone};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
    WindowEvent, Wry,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const KINDS: [&str; 4] = ["posture", "water", "walk", "caffeine"];
const MAX_SAVED: usize = 5;
const MAX_MINUTES: u32 = 720;
const CAFFEINE: &str = "caffeine";
/// After "Not now" (or no answer), ask again this much later.
const CAFFEINE_SNOOZE_MS: u64 = 30 * 60_000;
/// A daily dose not taken within this long of its time is skipped (late caffeine hurts sleep).
const CAFFEINE_MISS_MS: u64 = 4 * 3600_000;

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    every: BTreeMap<String, u32>, // minutes; 0 = off
    saved: BTreeMap<String, Vec<u32>>,
    paused: bool,
    #[serde(default)]
    caffeine: Caffeine,
}

/// Caffeine counts from when you actually take it, either on an interval or at 1–2 set times a day.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Caffeine {
    on: bool,
    mode: String,          // "interval" | "daily"
    times: Vec<u32>,       // daily: minutes after midnight, 1–2 doses
    last_taken: Option<u64>, // unix ms
    taken_date: String,    // local date of taken_today
    taken_today: usize,    // daily doses handled today (taken or skipped past)
}

impl Default for Caffeine {
    fn default() -> Self {
        Caffeine { on: true, mode: "interval".into(), times: vec![9 * 60], last_taken: None, taken_date: String::new(), taken_today: 0 }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let mut s = Settings { every: BTreeMap::new(), saved: BTreeMap::new(), paused: false, caffeine: Caffeine::default() };
        for (k, every, saved) in [
            ("posture", 20, vec![20, 30, 45]),
            ("water", 45, vec![30, 45, 60]),
            ("walk", 60, vec![60, 90]),
            ("caffeine", 240, vec![180, 240, 300]),
        ] {
            s.every.insert(k.into(), every);
            s.saved.insert(k.into(), saved);
        }
        s
    }
}

impl Settings {
    /// Repair anything odd in a loaded file instead of failing.
    fn sanitize(mut self) -> Self {
        let defaults = Settings::default();
        for k in KINDS {
            let mut saved: Vec<u32> = self.saved.remove(k).unwrap_or_else(|| defaults.saved[k].clone());
            saved.retain(|m| (1..=MAX_MINUTES).contains(m));
            saved.sort_unstable();
            saved.dedup();
            saved.truncate(MAX_SAVED);
            let every = self.every.get(k).copied().unwrap_or(defaults.every[k]);
            let every = if every == 0 || saved.contains(&every) { every } else { 0 };
            self.saved.insert(k.into(), saved);
            self.every.insert(k.into(), every);
        }
        let c = &mut self.caffeine;
        if c.mode != "daily" {
            c.mode = "interval".into();
        }
        c.times = clean_times(std::mem::take(&mut c.times));
        if c.times.is_empty() {
            c.times = vec![9 * 60];
        }
        self
    }
}

fn clean_times(mut t: Vec<u32>) -> Vec<u32> {
    t.retain(|m| *m < 24 * 60);
    t.sort_unstable();
    t.dedup();
    t.truncate(2);
    t
}

// ---------- caffeine timing ----------

fn local_ms(date: chrono::NaiveDate, minutes: u32) -> u64 {
    let t = NaiveTime::from_hms_opt(minutes / 60, minutes % 60, 0).unwrap_or_default();
    Local.from_local_datetime(&date.and_time(t)).earliest().map(|d| d.timestamp_millis() as u64).unwrap_or(0)
}

fn today() -> chrono::NaiveDate {
    Local::now().date_naive()
}

/// Daily mode: which dose is up now (skipping ones missed by more than CAFFEINE_MISS_MS) and when it's due.
/// A late first dose pushes the second back by the same amount, keeping their spacing.
fn daily_next(c: &Caffeine, now: u64) -> (Option<usize>, u64) {
    let date = today();
    let done = if c.taken_date == date.to_string() { c.taken_today } else { 0 };
    let last_today = c.last_taken.filter(|t| *t >= local_ms(date, 0));
    for i in done..c.times.len() {
        let mut due = local_ms(date, c.times[i]);
        if i > 0 {
            if let Some(last) = last_today {
                due = due.max(last + (c.times[i] - c.times[i - 1]) as u64 * 60_000);
            }
        }
        if now <= due + CAFFEINE_MISS_MS {
            return (Some(i), due);
        }
    }
    (None, local_ms(date.succ_opt().unwrap_or(date), c.times[0]))
}

struct Inner {
    s: Settings,
    next_at: BTreeMap<String, u64>, // unix ms
    queue: VecDeque<String>,
    showing: bool,
    overlays: Vec<String>,
    panel_hidden_at: Option<Instant>,
    panel_shown_at: Option<Instant>,
    pause_item: Option<CheckMenuItem<Wry>>,
    started_at: u64,
    caffeine_snooze: Option<u64>,
    tray_rect: Option<(i32, i32, i32, i32)>, // last known tray icon bounds, physical px
}

impl Inner {
    fn reset(&mut self, kind: &str) {
        if kind == CAFFEINE {
            // Caffeine's schedule comes from when you take it; just drop any pending re-ask.
            self.caffeine_snooze = None;
            self.refresh_caffeine();
            return;
        }
        let mins = self.s.every.get(kind).copied().unwrap_or(0) as u64;
        self.next_at.insert(kind.into(), now_ms() + mins * 60_000);
    }
    fn caffeine_due(&self, now: u64) -> Option<u64> {
        let c = &self.s.caffeine;
        if !c.on {
            return None;
        }
        let due = if c.mode == "daily" {
            daily_next(c, now).1
        } else {
            let every = self.s.every.get(CAFFEINE).copied().unwrap_or(0) as u64;
            if every == 0 {
                return None;
            }
            c.last_taken.map(|t| t + every * 60_000).unwrap_or(self.started_at + 60_000)
        };
        Some(due.max(self.caffeine_snooze.unwrap_or(0)))
    }
    fn refresh_caffeine(&mut self) {
        match self.caffeine_due(now_ms()) {
            Some(d) => self.next_at.insert(CAFFEINE.into(), d),
            None => self.next_at.remove(CAFFEINE),
        };
    }
    fn took_caffeine(&mut self) {
        let now = now_ms();
        let c = &mut self.s.caffeine;
        if c.mode == "daily" {
            let (dose, _) = daily_next(c, now);
            c.taken_today = dose.map(|i| i + 1).unwrap_or(c.times.len());
            c.taken_date = today().to_string();
        }
        c.last_taken = Some(now);
        self.caffeine_snooze = None;
        self.refresh_caffeine();
    }
    fn reset_all(&mut self) {
        for k in KINDS {
            self.reset(k);
        }
    }
    fn view(&self) -> View {
        let mut every = self.s.every.clone();
        if !self.s.caffeine.on {
            every.insert(CAFFEINE.into(), 0);
        }
        let mut next_at = self.next_at.clone();
        match self.caffeine_due(now_ms()) {
            Some(d) => next_at.insert(CAFFEINE.into(), d),
            None => next_at.remove(CAFFEINE),
        };
        let c = &self.s.caffeine;
        View {
            every,
            saved: self.s.saved.clone(),
            paused: self.s.paused,
            next_at,
            caffeine: CaffeineView { on: c.on, mode: c.mode.clone(), times: c.times.clone(), last_taken: c.last_taken },
        }
    }
}

struct AppState(Mutex<Inner>);

#[derive(Serialize)]
struct View {
    every: BTreeMap<String, u32>,
    saved: BTreeMap<String, Vec<u32>>,
    paused: bool,
    next_at: BTreeMap<String, u64>,
    caffeine: CaffeineView,
}

#[derive(Serialize)]
struct CaffeineView {
    on: bool,
    mode: String,
    times: Vec<u32>,
    last_taken: Option<u64>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn valid_kind(kind: &str) -> Result<(), String> {
    if KINDS.contains(&kind) { Ok(()) } else { Err(format!("unknown reminder: {kind}")) }
}

// ---------- settings file ----------

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .unwrap_or_default()
        .sanitize()
}

fn persist(app: &AppHandle, inner: &Inner) {
    if let Some(p) = settings_path(app) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&inner.s) {
            let _ = std::fs::write(p, json);
        }
    }
    if let Some(item) = &inner.pause_item {
        let _ = item.set_checked(inner.s.paused);
    }
}

/// Apply a change, save it and hand the panel the new state.
fn update(app: &AppHandle, f: impl FnOnce(&mut Inner)) -> View {
    let state = app.state::<AppState>();
    let mut g = state.0.lock().unwrap();
    f(&mut g);
    persist(app, &g);
    g.view()
}

// ---------- panel ----------

fn panel_visible(app: &AppHandle) -> bool {
    app.get_webview_window("panel").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

/// Where the panel goes: its right edge lines up with the tray icon so it opens out to the left,
/// above the icon when the taskbar is at the bottom (below it for a top menu bar), and always
/// fully inside the screen's usable area. All values are physical pixels.
fn panel_origin(tray: Option<(i32, i32, i32, i32)>, area: (i32, i32, i32, i32), panel: (i32, i32)) -> (i32, i32) {
    const GAP: i32 = 8;
    let (ax, ay, aw, ah) = area;
    let (pw, ph) = panel;
    let (x, y) = match tray {
        Some((tx, ty, tw, th)) => {
            let x = tx + tw - pw;
            let y = if ty + th / 2 > ay + ah / 2 { ty - ph - GAP } else { ty + th + GAP };
            (x, y)
        }
        // Tray position unknown (e.g. opened from a second launch): bottom-right on Windows/Linux, top-right on macOS.
        None => (ax + aw - pw - GAP, if cfg!(target_os = "macos") { ay + GAP } else { ay + ah - ph - GAP }),
    };
    let clamp = |v: i32, lo: i32, hi: i32| if hi < lo { lo } else { v.clamp(lo, hi) };
    (clamp(x, ax + GAP, ax + aw - pw - GAP), clamp(y, ay + GAP, ay + ah - ph - GAP))
}

fn place_panel(app: &AppHandle, w: &tauri::WebviewWindow) {
    let tray = app.state::<AppState>().0.lock().unwrap().tray_rect;
    let monitor = match tray {
        Some((x, y, _, _)) => app.monitor_from_point(x as f64, y as f64).ok().flatten(),
        None => None,
    }
    .or_else(|| w.current_monitor().ok().flatten())
    .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let wa = m.work_area();
    let area = (wa.position.x, wa.position.y, wa.size.width as i32, wa.size.height as i32);
    // Hidden windows can report a zero size on some platforms; fall back to the configured size.
    let size = w.outer_size().ok().filter(|s| s.width > 0 && s.height > 0)
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or(((800.0 * m.scale_factor()) as i32, (490.0 * m.scale_factor()) as i32));
    let (x, y) = panel_origin(tray, area, size);
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

fn show_panel(app: &AppHandle) {
    let Some(w) = app.get_webview_window("panel") else { return };
    if app.state::<AppState>().0.lock().unwrap().showing {
        return;
    }
    app.state::<AppState>().0.lock().unwrap().panel_shown_at = Some(Instant::now());
    place_panel(app, &w);
    let _ = w.show();
    place_panel(app, &w); // again once shown: some platforms report a zero size while hidden
    let _ = w.set_focus();
    let _ = app.emit_to("panel", "panel-shown", ());
}

fn toggle_panel(app: &AppHandle) {
    if panel_visible(app) {
        if let Some(w) = app.get_webview_window("panel") {
            let _ = w.hide();
        }
        return;
    }
    // Clicking the tray icon blurs (and hides) the panel just before this click arrives.
    if let Some(t) = app.state::<AppState>().0.lock().unwrap().panel_hidden_at {
        if t.elapsed() < Duration::from_millis(300) {
            return;
        }
    }
    show_panel(app);
}

fn hide_panel_now(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
    app.state::<AppState>().0.lock().unwrap().panel_hidden_at = Some(Instant::now());
}

// ---------- reminders ----------

#[cfg(target_os = "windows")]
fn blur() -> Option<tauri::utils::config::WindowEffectsConfig> {
    use tauri::window::{Effect, EffectState, EffectsBuilder};
    // Acrylic works on Windows 10 and 11. (The plain Blur effect only works up to Windows 11 22H1
    // and renders black on newer builds.) A light neutral tint keeps it a soft blur.
    Some(EffectsBuilder::new().effect(Effect::Acrylic).state(EffectState::Active).color(tauri::window::Color(24, 24, 24, 40)).build())
}
#[cfg(target_os = "macos")]
fn blur() -> Option<tauri::utils::config::WindowEffectsConfig> {
    use tauri::window::{Effect, EffectState, EffectsBuilder};
    Some(EffectsBuilder::new().effect(Effect::FullScreenUI).state(EffectState::Active).build())
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn blur() -> Option<tauri::utils::config::WindowEffectsConfig> {
    None
}

/// One borderless window per monitor; the OS blurs what's behind each. The card is on the primary.
fn show_overlays(app: &AppHandle, kind: &str) -> tauri::Result<Vec<String>> {
    let monitors = app.available_monitors()?;
    let primary = app.primary_monitor()?.map(|m| *m.position());
    let mut labels = Vec::new();
    for (i, m) in monitors.iter().enumerate() {
        let is_primary = primary.map(|p| p == *m.position()).unwrap_or(i == 0);
        let label = format!("overlay{i}");
        if let Some(old) = app.get_webview_window(&label) {
            let _ = old.destroy();
        }
        let url = format!("popup.html?kind={kind}&primary={}", if is_primary { 1 } else { 0 });
        let mut b = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
            .title("Focus")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .shadow(false)
            .visible(false)
            .focused(is_primary);
        if let Some(fx) = blur() {
            b = b.effects(fx);
        }
        let w = b.build()?;
        w.set_position(PhysicalPosition::new(m.position().x, m.position().y))?;
        w.set_size(PhysicalSize::new(m.size().width, m.size().height))?;
        w.show()?;
        if is_primary {
            let _ = w.set_focus();
        }
        labels.push(label);
    }
    Ok(labels)
}

/// Show the next queued reminder, unless one is up or the panel is open.
fn pump(app: &AppHandle) {
    let kind = {
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        if g.showing || g.queue.is_empty() || panel_visible(app) {
            return;
        }
        g.showing = true;
        g.queue.pop_front().unwrap()
    };
    // Windows are built off the main thread to avoid blocking the event loop.
    let app = app.clone();
    thread::spawn(move || {
        let result = show_overlays(&app, &kind);
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        match result {
            Ok(labels) if !labels.is_empty() => g.overlays = labels,
            _ => g.showing = false,
        }
    });
}

fn queue_now(app: &AppHandle, kind: &str) {
    {
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        g.queue.retain(|k| k != kind);
        g.queue.push_front(kind.into());
        if kind == CAFFEINE {
            g.caffeine_snooze = Some(now_ms() + CAFFEINE_SNOOZE_MS); // re-ask later unless taken
        } else {
            g.reset(kind);
        }
    }
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(250));
        pump(&app);
    });
}

/// Every few seconds: queue anything that's due. Clock-based, so sleep or throttling never drifts it.
fn tick(app: &AppHandle) {
    {
        let state = app.state::<AppState>();
        let mut g = state.0.lock().unwrap();
        if !g.s.paused {
            let now = now_ms();
            if let Some(due) = g.caffeine_due(now) {
                if now >= due && !g.queue.iter().any(|q| q == CAFFEINE) {
                    g.caffeine_snooze = Some(now + CAFFEINE_SNOOZE_MS); // re-ask later unless taken
                    g.queue.push_back(CAFFEINE.into());
                }
            }
            g.refresh_caffeine();
            for k in KINDS {
                if k == CAFFEINE {
                    continue;
                }
                let every = g.s.every.get(k).copied().unwrap_or(0);
                if every == 0 || now < g.next_at.get(k).copied().unwrap_or(0) {
                    continue;
                }
                g.reset(k);
                if !g.queue.iter().any(|q| q == k) {
                    g.queue.push_back(k.into());
                }
            }
        }
    }
    pump(app);
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_panel(app),
        "quit" => app.exit(0),
        "pause" => {
            update(app, |g| {
                g.s.paused = !g.s.paused;
                g.reset_all();
            });
        }
        "reset" => {
            update(app, |g| g.reset_all());
        }
        _ => {
            if let Some(kind) = id.strip_prefix("skip:") {
                if valid_kind(kind).is_ok() {
                    queue_now(app, kind);
                }
            }
        }
    }
    let _ = app.emit_to("panel", "state-changed", ());
}

// ---------- updates ----------

async fn run_update(app: AppHandle) -> &'static str {
    use tauri_plugin_updater::UpdaterExt;
    let Ok(updater) = app.updater() else { return "failed" };
    match updater.check().await {
        Ok(Some(update)) => match update.download_and_install(|_, _| {}, || {}).await {
            Ok(()) => {
                let app = app.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(1500));
                    app.restart();
                });
                "updating"
            }
            Err(_) => "failed",
        },
        Ok(None) => "current",
        Err(_) => "failed",
    }
}

// ---------- commands (called from the UI) ----------

#[tauri::command]
fn get_state(app: AppHandle) -> View {
    app.state::<AppState>().0.lock().unwrap().view()
}

#[tauri::command]
fn set_interval(app: AppHandle, kind: String, minutes: u32) -> Result<View, String> {
    valid_kind(&kind)?;
    Ok(update(&app, |g| {
        if kind == CAFFEINE {
            g.s.caffeine.on = minutes != 0;
            if minutes != 0 && g.s.saved[&kind].contains(&minutes) {
                g.s.every.insert(kind.clone(), minutes);
                g.s.caffeine.mode = "interval".into();
            }
            g.reset(&kind);
            return;
        }
        if minutes == 0 || g.s.saved[&kind].contains(&minutes) {
            g.s.every.insert(kind.clone(), minutes);
            g.reset(&kind);
        }
    }))
}

#[tauri::command]
fn save_time(app: AppHandle, kind: String, minutes: u32) -> Result<View, String> {
    valid_kind(&kind)?;
    if !(1..=MAX_MINUTES).contains(&minutes) {
        return Err("out of range".into());
    }
    Ok(update(&app, |g| {
        let saved = g.s.saved.get_mut(&kind).unwrap();
        if !saved.contains(&minutes) {
            if saved.len() >= MAX_SAVED {
                return;
            }
            saved.push(minutes);
            saved.sort_unstable();
        }
        g.s.every.insert(kind.clone(), minutes);
        if kind == CAFFEINE {
            g.s.caffeine.on = true;
            g.s.caffeine.mode = "interval".into();
        }
        g.reset(&kind);
    }))
}

#[tauri::command]
fn remove_time(app: AppHandle, kind: String, minutes: u32) -> Result<View, String> {
    valid_kind(&kind)?;
    Ok(update(&app, |g| {
        g.s.saved.get_mut(&kind).unwrap().retain(|m| *m != minutes);
        if g.s.every[&kind] == minutes {
            g.s.every.insert(kind.clone(), 0);
            if kind == CAFFEINE && g.s.caffeine.mode == "interval" {
                g.s.caffeine.on = false;
            }
        }
        g.reset(&kind);
    }))
}

#[tauri::command]
fn set_paused(app: AppHandle, paused: bool) -> View {
    update(&app, |g| {
        g.s.paused = paused;
        g.reset_all();
    })
}

#[tauri::command]
fn reset(app: AppHandle, kind: Option<String>) -> Result<View, String> {
    if let Some(k) = &kind {
        valid_kind(k)?;
    }
    Ok(update(&app, |g| {
        match &kind {
            Some(k) => g.reset(k),
            None => g.reset_all(),
        }
        // Resetting caffeine in interval mode restarts its countdown from now.
        if kind.as_deref().map_or(true, |k| k == CAFFEINE) && g.s.caffeine.mode == "interval" {
            g.s.caffeine.last_taken = Some(now_ms());
            g.refresh_caffeine();
        }
    }))
}

#[tauri::command]
fn skip(app: AppHandle, kind: String) -> Result<(), String> {
    valid_kind(&kind)?;
    hide_panel_now(&app);
    queue_now(&app, &kind);
    Ok(())
}

#[tauri::command]
fn dismiss_popup(app: AppHandle) {
    let labels = std::mem::take(&mut app.state::<AppState>().0.lock().unwrap().overlays);
    for l in &labels {
        let _ = app.emit_to(l.as_str(), "popup-out", ());
    }
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(420)); // let the fade-out play
        for l in &labels {
            if let Some(w) = app.get_webview_window(l) {
                let _ = w.destroy();
            }
        }
        app.state::<AppState>().0.lock().unwrap().showing = false;
        thread::sleep(Duration::from_millis(1500)); // breathing room before the next one
        pump(&app);
    });
}

/// The caffeine popup's answer. Only "Took it" starts the next countdown.
#[tauri::command]
fn caffeine_answer(app: AppHandle, took: bool) {
    update(&app, |g| {
        if took {
            g.took_caffeine();
        } else {
            g.caffeine_snooze = Some(now_ms() + CAFFEINE_SNOOZE_MS);
            g.refresh_caffeine();
        }
    });
    let _ = app.emit_to("panel", "state-changed", ());
}

#[tauri::command]
fn set_caffeine(app: AppHandle, mode: Option<String>, times: Option<Vec<u32>>) -> View {
    update(&app, |g| {
        let c = &mut g.s.caffeine;
        if let Some(m) = mode {
            c.mode = if m == "daily" { "daily".into() } else { "interval".into() };
        }
        if let Some(t) = times {
            let t = clean_times(t);
            if !t.is_empty() {
                c.times = t;
            }
        }
        c.on = true;
        if c.mode == "interval" && g.s.every[CAFFEINE] == 0 {
            let first = g.s.saved[CAFFEINE].first().copied().unwrap_or(240);
            g.s.every.insert(CAFFEINE.into(), first);
        }
        g.reset(CAFFEINE);
    })
}

#[tauri::command]
fn hide_panel(app: AppHandle) {
    hide_panel_now(&app);
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> bool {
    let l = app.autolaunch();
    let _ = if enabled { l.enable() } else { l.disable() };
    l.is_enabled().unwrap_or(false)
}

#[tauri::command]
async fn check_update(app: AppHandle) -> String {
    run_update(app).await.to_string()
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        // Launching it again just opens the panel.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_panel(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state, set_interval, save_time, remove_time, set_paused, reset, skip,
            dismiss_popup, caffeine_answer, set_caffeine, hide_panel, get_autostart, set_autostart, check_update, quit
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let settings = load_settings(&handle);
            let mut inner = Inner {
                s: settings,
                next_at: BTreeMap::new(),
                queue: VecDeque::new(),
                showing: false,
                overlays: Vec::new(),
                panel_hidden_at: None,
                panel_shown_at: None,
                pause_item: None,
                started_at: now_ms(),
                caffeine_snooze: None,
                tray_rect: None,
            };
            inner.reset_all();

            let pause = CheckMenuItem::with_id(app, "pause", "Pause all", true, inner.s.paused, None::<&str>)?;
            let menu = Menu::with_items(app, &[
                &MenuItem::with_id(app, "open", "Open Focus", true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "skip:posture", "Skip to next posture", true, None::<&str>)?,
                &MenuItem::with_id(app, "skip:water", "Skip to next water", true, None::<&str>)?,
                &MenuItem::with_id(app, "skip:walk", "Skip to next walk", true, None::<&str>)?,
                &MenuItem::with_id(app, "skip:caffeine", "Skip to next caffeine", true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "reset", "Reset all timers", true, None::<&str>)?,
                &pause,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "quit", "Quit Focus", true, None::<&str>)?,
            ])?;
            inner.pause_item = Some(pause);
            app.manage(AppState(Mutex::new(inner)));

            #[cfg(target_os = "macos")]
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
            #[cfg(not(target_os = "macos"))]
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

            // Left click opens the panel; right click shows the quick menu.
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("Focus")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| handle_menu(app, e.id.as_ref()))
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { rect, .. } | TrayIconEvent::Enter { rect, .. } | TrayIconEvent::Move { rect, .. } = &e {
                        let scale = tray.app_handle().primary_monitor().ok().flatten().map(|m| m.scale_factor()).unwrap_or(1.0);
                        let p = rect.position.to_physical::<i32>(scale);
                        let sz = rect.size.to_physical::<u32>(scale);
                        tray.app_handle().state::<AppState>().0.lock().unwrap().tray_rect =
                            Some((p.x, p.y, sz.width as i32, sz.height as i32));
                    }
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        toggle_panel(tray.app_handle());
                    }
                })
                .build(app)?;

            let h = handle.clone();
            thread::spawn(move || loop {
                thread::sleep(Duration::from_secs(5));
                tick(&h);
            });

            let h = handle.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(30));
                loop {
                    tauri::async_runtime::block_on(run_update(h.clone()));
                    thread::sleep(Duration::from_secs(6 * 3600));
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // The panel closes when you click anywhere else; queued reminders then get their turn.
            if window.label() == "panel" {
                if let WindowEvent::Focused(false) = event {
                    let app = window.app_handle().clone();
                    // Some platforms report a focus change while the window is still appearing.
                    if let Some(t) = app.state::<AppState>().0.lock().unwrap().panel_shown_at {
                        if t.elapsed() < Duration::from_millis(400) {
                            return;
                        }
                    }
                    hide_panel_now(&app);
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(800));
                        pump(&app);
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Focus");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daily(times: Vec<u32>) -> Caffeine {
        Caffeine { on: true, mode: "daily".into(), times, ..Caffeine::default() }
    }
    const H: u64 = 3600_000;

    #[test]
    fn first_dose_is_due_at_its_time() {
        let d = today();
        let c = daily(vec![9 * 60, 13 * 60]);
        assert_eq!(daily_next(&c, local_ms(d, 8 * 60)), (Some(0), local_ms(d, 9 * 60)));
    }

    #[test]
    fn late_first_dose_pushes_the_second_back() {
        let d = today();
        let mut c = daily(vec![9 * 60, 13 * 60]);
        // Took dose 1 at 11:00 (2 h late) → dose 2 keeps the 4 h gap: 15:00, not 13:00.
        c.last_taken = Some(local_ms(d, 11 * 60));
        c.taken_today = 1;
        c.taken_date = d.to_string();
        assert_eq!(daily_next(&c, local_ms(d, 12 * 60)), (Some(1), local_ms(d, 15 * 60)));
    }

    #[test]
    fn missed_first_dose_is_skipped_for_the_second() {
        let d = today();
        let c = daily(vec![8 * 60, 14 * 60]);
        // Never took the 08:00 dose; at 13:00 it's over 4 h late, so the 14:00 dose is up.
        assert_eq!(daily_next(&c, local_ms(d, 13 * 60)), (Some(1), local_ms(d, 14 * 60)));
    }

    #[test]
    fn all_done_means_tomorrow() {
        let d = today();
        let mut c = daily(vec![9 * 60]);
        c.taken_today = 1;
        c.taken_date = d.to_string();
        assert_eq!(daily_next(&c, local_ms(d, 10 * 60)).0, None);
        assert_eq!(daily_next(&c, local_ms(d, 10 * 60)).1, local_ms(d.succ_opt().unwrap(), 9 * 60));
    }

    #[test]
    fn yesterdays_count_resets() {
        let d = today();
        let mut c = daily(vec![9 * 60]);
        c.taken_today = 1;
        c.taken_date = "2000-01-01".into();
        assert_eq!(daily_next(&c, local_ms(d, 8 * 60)).0, Some(0));
    }

    #[test]
    fn panel_opens_left_of_a_bottom_right_tray() {
        // 1920x1040 work area, taskbar at the bottom, icon near the right edge, 800x470 panel.
        let (x, y) = panel_origin(Some((1800, 1045, 24, 24)), (0, 0, 1920, 1040), (800, 470));
        assert_eq!(x, 1800 + 24 - 800); // right edge on the icon, extends left
        assert_eq!(y, 1040 - 470 - 8);  // above the taskbar, inside the screen
    }

    #[test]
    fn panel_never_leaves_the_screen() {
        // Icon at the far right of a small screen: clamped to the right edge.
        let (x, _) = panel_origin(Some((1350, 740, 24, 24)), (0, 0, 1366, 728), (1000, 588));
        assert_eq!(x, 1366 - 1000 - 8);
        // Top menu bar (macOS): below the icon.
        let (_, y) = panel_origin(Some((1700, 0, 22, 24)), (0, 25, 1920, 1055), (800, 470));
        assert_eq!(y, 25 + 8); // clamped into the work area below the menu bar
        // Second monitor to the left (negative coordinates).
        let (x, _) = panel_origin(Some((-100, 1045, 24, 24)), (-1920, 0, 1920, 1040), (800, 470));
        assert_eq!(x, -100 + 24 - 800);
    }

    #[test]
    fn settings_sanitize_caffeine() {
        let mut s = Settings::default();
        s.caffeine.mode = "weird".into();
        s.caffeine.times = vec![2000, 600, 600, 300, 900];
        let s = s.sanitize();
        assert_eq!(s.caffeine.mode, "interval");
        assert_eq!(s.caffeine.times, vec![300, 600]);
        let _ = H;
    }
}
