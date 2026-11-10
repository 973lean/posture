// Focus: a tray app with three reminders (posture, water, walk).
// Rust owns the schedule, settings, tray and windows; the UI lives in ../ui.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
    WindowEvent, Wry,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_positioner::{Position, WindowExt};

const KINDS: [&str; 3] = ["posture", "water", "walk"];
const MAX_SAVED: usize = 5;
const MAX_MINUTES: u32 = 720;

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    every: BTreeMap<String, u32>, // minutes; 0 = off
    saved: BTreeMap<String, Vec<u32>>,
    paused: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let mut s = Settings { every: BTreeMap::new(), saved: BTreeMap::new(), paused: false };
        for (k, every, saved) in [("posture", 20, vec![20, 30, 45]), ("water", 45, vec![30, 45, 60]), ("walk", 60, vec![60, 90])] {
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
        self
    }
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
}

impl Inner {
    fn reset(&mut self, kind: &str) {
        let mins = self.s.every.get(kind).copied().unwrap_or(0) as u64;
        self.next_at.insert(kind.into(), now_ms() + mins * 60_000);
    }
    fn reset_all(&mut self) {
        for k in KINDS {
            self.reset(k);
        }
    }
    fn view(&self) -> View {
        View { every: self.s.every.clone(), saved: self.s.saved.clone(), paused: self.s.paused, next_at: self.next_at.clone() }
    }
}

struct AppState(Mutex<Inner>);

#[derive(Serialize)]
struct View {
    every: BTreeMap<String, u32>,
    saved: BTreeMap<String, Vec<u32>>,
    paused: bool,
    next_at: BTreeMap<String, u64>,
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

/// Next to the tray icon: below the menu bar on macOS, above the taskbar elsewhere.
fn place_panel(w: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    let (near_tray, fallback) = (Position::TrayBottomCenter, Position::TopRight);
    #[cfg(not(target_os = "macos"))]
    let (near_tray, fallback) = (Position::TrayCenter, Position::BottomRight);
    // The tray position is unknown until the icon has been clicked once (e.g. on a second launch).
    let near = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.move_window(near_tray).is_ok())).unwrap_or(false);
    if !near {
        let _ = w.move_window(fallback);
    }
}

fn show_panel(app: &AppHandle) {
    let Some(w) = app.get_webview_window("panel") else { return };
    if app.state::<AppState>().0.lock().unwrap().showing {
        return;
    }
    app.state::<AppState>().0.lock().unwrap().panel_shown_at = Some(Instant::now());
    place_panel(&w);
    let _ = w.show();
    place_panel(&w); // again once shown: some platforms report a zero size while hidden
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
    Some(EffectsBuilder::new().effect(Effect::Blur).state(EffectState::Active).build())
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
        g.reset(kind);
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
            for k in KINDS {
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
    Ok(update(&app, |g| match &kind {
        Some(k) => g.reset(k),
        None => g.reset_all(),
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
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state, set_interval, save_time, remove_time, set_paused, reset, skip,
            dismiss_popup, hide_panel, get_autostart, set_autostart, check_update, quit
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
            };
            inner.reset_all();

            let pause = CheckMenuItem::with_id(app, "pause", "Pause all", true, inner.s.paused, None::<&str>)?;
            let menu = Menu::with_items(app, &[
                &MenuItem::with_id(app, "open", "Open Focus", true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "skip:posture", "Skip to next posture", true, None::<&str>)?,
                &MenuItem::with_id(app, "skip:water", "Skip to next water", true, None::<&str>)?,
                &MenuItem::with_id(app, "skip:walk", "Skip to next walk", true, None::<&str>)?,
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
                    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &e);
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
