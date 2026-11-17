//! State shared between the presence loop and the tray icon.

use std::sync::Mutex;

use tokio::sync::watch;

use crate::config::{self, State};
use crate::log;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// Connected to (and authorized by) the TeamSpeak client.
    pub teamspeak: bool,
    /// Connected to the Discord client.
    pub discord: bool,
    /// What Discord currently shows, as one line.
    pub presence: Option<String>,
}

pub struct App {
    pub enabled: watch::Sender<bool>,
    pub status: watch::Sender<Status>,
    pub quit: watch::Sender<bool>,
    state: Mutex<State>,
}

impl App {
    pub fn new(state: State) -> Self {
        Self {
            enabled: watch::Sender::new(state.enabled),
            status: watch::Sender::new(Status::default()),
            quit: watch::Sender::new(false),
            state: Mutex::new(state),
        }
    }

    pub fn api_key(&self) -> String {
        self.state.lock().unwrap().api_key.clone()
    }

    fn save(&self, f: impl FnOnce(&mut State)) -> std::io::Result<()> {
        let mut st = self.state.lock().unwrap();
        f(&mut st);
        config::save_state(&st)
    }

    pub fn save_api_key(&self, key: &str) {
        match self.save(|s| s.api_key = key.to_owned()) {
            Ok(()) => log!("TeamSpeak: API key saved"),
            Err(e) => log!("TeamSpeak: could not save API key: {e}"),
        }
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.borrow()
    }

    pub fn set_enabled(&self, on: bool) {
        if self.enabled.send_replace(on) != on {
            log!("Rich Presence {}", if on { "enabled" } else { "disabled" });
            if let Err(e) = self.save(|s| s.enabled = on) {
                log!("Could not save state: {e}");
            }
        }
    }

    pub fn request_quit(&self) {
        self.quit.send_replace(true);
    }

    /// One-line status for the tray tooltip and menu.
    pub fn status_line(&self) -> String {
        let st = self.status.borrow();
        if !self.is_enabled() {
            "Rich Presence is off".into()
        } else if !st.teamspeak {
            "Waiting for TeamSpeak…".into()
        } else if !st.discord {
            "Waiting for Discord…".into()
        } else {
            st.presence.clone().unwrap_or_else(|| "Nothing shown".into())
        }
    }
}
