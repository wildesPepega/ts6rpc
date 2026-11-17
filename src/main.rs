// No console window on Windows; logs go to ts6rpc.log in the config dir.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! ts6rpc — Discord Rich Presence for the TeamSpeak 6 client.
//!
//! Talks to TeamSpeak through its official "Remote Apps" WebSocket API and to
//! Discord through the local RPC socket / named pipe.

mod app;
mod autostart;
mod bookmarks;
mod config;
mod discord;
mod instance;
mod logging;
mod presence;
mod teamspeak;
mod tray;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::watch;
use tokio::time::Instant;

use app::App;
use bookmarks::Address;
use config::Config;
use teamspeak::Snapshot;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Discord rate-limits SET_ACTIVITY (about 5 per 20 s).
const MIN_UPDATE_INTERVAL: Duration = Duration::from_secs(4);

/// Server address lookup: manual overrides first, then TeamSpeak bookmarks.
struct AddressBook {
    overrides: HashMap<String, String>,
    bookmarks: HashMap<String, Address>,
    last_miss: Option<String>,
}

impl AddressBook {
    async fn lookup(&mut self, uid: &str) -> Option<Address> {
        if uid.is_empty() {
            return None;
        }
        if let Some(a) = self.overrides.get(uid).and_then(|s| bookmarks::parse_address(s)) {
            return Some(a);
        }
        // Re-read the bookmarks once per unknown server (e.g. after bookmarking it).
        if !self.bookmarks.contains_key(uid) && self.last_miss.as_deref() != Some(uid) {
            self.last_miss = Some(uid.to_owned());
            if let Ok(map) = tokio::task::spawn_blocking(bookmarks::read_bookmarks).await {
                self.bookmarks = map;
            }
        }
        self.bookmarks.get(uid).cloned()
    }
}

fn presence_line(a: &serde_json::Value) -> String {
    let mut line = format!("{} | {}", a["details"].as_str().unwrap_or(""), a["state"].as_str().unwrap_or(""));
    if let Some(s) = a["assets"]["small_text"].as_str() {
        line += &format!(" | {s}");
    }
    line
}

/// Sleeps for `d`, returning early (with `false`) if the presence gets switched off.
async fn sleep_while_enabled(enabled: &mut watch::Receiver<bool>, d: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(d) => true,
        _ = enabled.wait_for(|e| !*e) => false,
    }
}

async fn presence_loop(cfg: Config, mut rx: watch::Receiver<Option<Snapshot>>, app: Arc<App>, debug: bool) {
    let mut book = AddressBook { overrides: cfg.server_addresses.clone(), bookmarks: HashMap::new(), last_miss: None };
    let mut enabled = app.enabled.subscribe();
    let mut announced_down = false;
    loop {
        // While switched off we don't keep a Discord connection at all.
        if enabled.wait_for(|e| *e).await.is_err() {
            return;
        }
        let mut discord = match discord::Discord::connect(&cfg.discord_client_id).await {
            Ok(d) => d,
            Err(e) => {
                if !announced_down {
                    log!("Discord: {e}, retrying…");
                    announced_down = true;
                }
                sleep_while_enabled(&mut enabled, Duration::from_secs(10)).await;
                continue;
            }
        };
        announced_down = false;
        log!("Discord: connected");
        app.status.send_modify(|s| s.discord = true);

        let mut last_sent: Option<Option<serde_json::Value>> = None;
        let mut last_sent_at: Option<Instant> = None;
        let switched_off = loop {
            // Throttle, then always use the newest snapshot.
            if let Some(t) = last_sent_at {
                tokio::select! {
                    _ = tokio::time::sleep_until(t + MIN_UPDATE_INTERVAL) => {}
                    _ = discord.closed() => break false,
                    _ = enabled.wait_for(|e| !*e) => break true,
                }
            }
            let snap = rx.borrow_and_update().clone();
            let addr = match snap.as_ref().filter(|s| s.connected) {
                Some(s) => book.lookup(&s.server_uid).await,
                None => None,
            };
            let activity = presence::build(snap.as_ref(), &cfg, addr.as_ref());
            if last_sent.as_ref() != Some(&activity) {
                discord.set_activity(activity.as_ref());
                let line = activity.as_ref().map(presence_line);
                match &line {
                    Some(l) => log!("Presence: {l}"),
                    None => log!("Presence cleared"),
                }
                if debug && let Some(a) = &activity {
                    log!("{a}");
                }
                app.status.send_modify(|s| s.presence = line);
                last_sent = Some(activity);
                last_sent_at = Some(Instant::now());
            }
            tokio::select! {
                changed = rx.changed() => if changed.is_err() { return },
                _ = discord.closed() => break false,
                _ = enabled.wait_for(|e| !*e) => break true,
            }
        };
        // Dropping the connection makes Discord remove the activity.
        drop(discord);
        app.status.send_modify(|s| {
            s.discord = false;
            s.presence = None;
        });
        if switched_off {
            log!("Discord: disconnected (Rich Presence switched off)");
        } else {
            log!("Discord: connection closed");
            sleep_while_enabled(&mut enabled, Duration::from_secs(5)).await;
        }
    }
}

/// Mirrors "is TeamSpeak connected" into the shared status.
async fn track_teamspeak(mut rx: watch::Receiver<Option<Snapshot>>, app: Arc<App>) {
    loop {
        let up = rx.borrow_and_update().is_some();
        app.status.send_if_modified(|s| std::mem::replace(&mut s.teamspeak, up) != up);
        if rx.changed().await.is_err() {
            return;
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("signal handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Everything except the Windows tray: TeamSpeak, Discord and (on Linux) the tray.
async fn run(cfg: Config, app: Arc<App>, debug: bool) {
    let (tx, rx) = watch::channel::<Option<Snapshot>>(None);
    let info = teamspeak::AppInfo {
        identifier: "de.ts6rpc.discord",
        version: env!("CARGO_PKG_VERSION"),
        name: "Discord Rich Presence",
        description: "Shows your TeamSpeak status in Discord",
    };
    let app_ts = app.clone();
    let ts = teamspeak::run(cfg.teamspeak_url.clone(), app.api_key(), info, debug, move |k| app_ts.save_api_key(k), tx);

    #[cfg(target_os = "linux")]
    let tray = async {
        if cfg.tray {
            tray::linux::run(app.clone()).await
        } else {
            std::future::pending().await
        }
    };
    #[cfg(not(target_os = "linux"))]
    let tray = std::future::pending::<()>();

    let mut quit = app.quit.subscribe();
    // Discord removes the activity on its own once the IPC connection closes.
    tokio::select! {
        _ = ts => {}
        _ = presence_loop(cfg.clone(), rx.clone(), app.clone(), debug) => {}
        _ = track_teamspeak(rx, app.clone()) => {}
        _ = tray => {}
        _ = shutdown_signal() => log!("Shutting down"),
        _ = quit.wait_for(|q| *q) => log!("Quit"),
    }
}

/// Shows an error. On Windows there is no console, so use a message box.
fn fatal(msg: &str) -> ! {
    log!("Error: {msg}");
    #[cfg(windows)]
    message_box(msg);
    std::process::exit(1);
}

#[cfg(windows)]
fn message_box(msg: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MessageBoxW};
    let text: Vec<u16> = msg.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "ts6rpc".encode_utf16().chain(Some(0)).collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_ICONINFORMATION) };
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("ts6rpc {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "ts6rpc {} — Discord Rich Presence for TeamSpeak 6\n\n\
             Usage: ts6rpc [--no-tray]\n\n  \
             --no-tray   don't show a tray icon\n\n\
             Config: {}\nSet TS6RPC_DEBUG=1 for verbose logs.",
            env!("CARGO_PKG_VERSION"),
            config::dir().join("config.json").display()
        );
        return;
    }
    if !instance::acquire() {
        #[cfg(windows)]
        message_box("ts6rpc is already running. Look for its icon in the notification area.");
        eprintln!("ts6rpc is already running.");
        return;
    }
    logging::init();

    let (mut cfg, cfg_path) = config::load_config().unwrap_or_else(|e| fatal(&e));
    if args.iter().any(|a| a == "--no-tray") {
        cfg.tray = false;
    }
    if !(15..=22).contains(&cfg.discord_client_id.len()) || !cfg.discord_client_id.bytes().all(|b| b.is_ascii_digit()) {
        fatal(&format!("Please set a valid \"discordClientId\" in {}", cfg_path.display()));
    }
    let debug = std::env::var_os("TS6RPC_DEBUG").is_some();
    log!("ts6rpc {} — config: {}", env!("CARGO_PKG_VERSION"), cfg_path.display());

    let app = Arc::new(App::new(config::load_state()));
    if !app.is_enabled() {
        log!("Rich Presence is switched off (turn it on from the tray menu)");
    }
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");

    #[cfg(windows)]
    if cfg.tray {
        // The tray needs the main thread; everything else runs on a worker thread.
        let worker = {
            let app = app.clone();
            std::thread::spawn(move || rt.block_on(run(cfg, app, debug)))
        };
        tray::windows::run(app, || worker.is_finished());
        return;
    }

    rt.block_on(run(cfg, app, debug));
}
