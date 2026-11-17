//! Config (config.json) and persisted state (state.json) in the user's config dir:
//! Linux ~/.config/ts6rpc, Windows %APPDATA%\ts6rpc, macOS ~/Library/Application Support/ts6rpc.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimestampMode {
    /// Elapsed time since connecting to the server
    Server,
    /// Elapsed time since joining the current channel
    Channel,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SmallImages {
    pub screenshare: String,
    pub muted: String,
    pub deafened: String,
    pub away: String,
}

impl Default for SmallImages {
    fn default() -> Self {
        Self {
            screenshare: "https://panel.nein.live/assets/Other/DiscordRPC-Assets/TS6RPC/screenshare.png".into(),
            muted: String::new(),
            deafened: String::new(),
            away: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub discord_client_id: String,
    /// Show an icon in the system tray (Windows, and Linux desktops with StatusNotifierItem support).
    pub tray: bool,
    pub teamspeak_url: String,
    /// Show a presence while TeamSpeak is open but not connected to a server.
    pub show_when_disconnected: bool,
    pub show_server_address: bool,
    pub show_channel_name: bool,
    pub hide_password_channels: bool,
    pub show_channel_user_count: bool,
    pub timestamp: TimestampMode,
    /// Asset key from the Discord application, or an https:// image URL.
    pub large_image: String,
    pub small_images: SmallImages,
    /// Manual address overrides by server unique identifier, e.g. { "VoNG...=": "ts.example.com:9987" }.
    /// Normally the address is read automatically from your TeamSpeak bookmarks.
    pub server_addresses: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            discord_client_id: "1572206068568952862".into(),
            tray: true,
            teamspeak_url: "ws://127.0.0.1:5899".into(),
            show_when_disconnected: true,
            show_server_address: true,
            show_channel_name: true,
            hide_password_channels: true,
            show_channel_user_count: true,
            timestamp: TimestampMode::Server,
            large_image: "https://panel.nein.live/assets/Other/DiscordRPC-Assets/TS6RPC/ts6.jpeg".into(),
            small_images: SmallImages::default(),
            server_addresses: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub api_key: String,
    /// Rich Presence switched on (toggled from the tray menu).
    pub enabled: bool,
}

impl Default for State {
    fn default() -> Self {
        Self { api_key: String::new(), enabled: true }
    }
}

pub fn dir() -> PathBuf {
    if let Some(d) = std::env::var_os("TS6RPC_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("ts6rpc")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(value)? + "\n";
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    opts.open(path)?.write_all(data.as_bytes())
}

pub fn load_config() -> Result<(Config, PathBuf), String> {
    let path = dir().join("config.json");
    let cfg = if path.exists() {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("invalid {}: {e}", path.display()))?
    } else {
        let cfg = Config::default();
        write_json(&path, &cfg).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        cfg
    };
    Ok((cfg, path))
}

pub fn load_state() -> State {
    std::fs::read_to_string(dir().join("state.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &State) -> std::io::Result<()> {
    write_json(&dir().join("state.json"), state)
}
