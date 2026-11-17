//! Builds the Discord activity from a TeamSpeak snapshot.

use serde_json::{Map, Value, json};

use crate::bookmarks::Address;
use crate::config::{Config, TimestampMode};
use crate::teamspeak::Snapshot;

/// "[cspacer1] Friends" -> "Friends"
fn clean_name(s: &str) -> String {
    let t = s.trim();
    if let Some((tag, name)) = t.strip_prefix('[').and_then(|r| r.split_once(']')) {
        let tag = tag.to_ascii_lowercase();
        if tag.trim_start_matches(['l', 'r', 'c', '*']).starts_with("spacer") {
            return name.trim().to_owned();
        }
    }
    t.to_owned()
}

/// Discord requires 2..=128 characters for text fields.
fn fit(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut out: String = if s.chars().count() > 128 {
        s.chars().take(127).chain(std::iter::once('…')).collect()
    } else {
        s.to_owned()
    };
    if out.chars().count() < 2 {
        out.push(' ');
    }
    Some(out)
}

fn put(m: &mut Map<String, Value>, k: &str, v: Option<String>) {
    if let Some(v) = v {
        m.insert(k.into(), Value::String(v));
    }
}

pub fn build(snap: Option<&Snapshot>, cfg: &Config, addr: Option<&Address>) -> Option<Value> {
    let snap = snap?;
    let mut a = Map::new();
    let mut assets = Map::new();
    a.insert("instance".into(), Value::Bool(false));

    if !snap.connected {
        if !cfg.show_when_disconnected {
            return None;
        }
        put(&mut a, "details", fit("Not connected"));
        put(&mut a, "state", fit("Not on a server"));
        if !cfg.large_image.is_empty() {
            put(&mut assets, "large_image", Some(cfg.large_image.clone()));
            put(&mut assets, "large_text", fit("TeamSpeak 6"));
        }
    } else {
        let addr_text = addr.filter(|_| cfg.show_server_address).map(ToString::to_string);
        let details = match &addr_text {
            Some(at) => format!("{} ({at})", snap.server_name),
            None => snap.server_name.clone(),
        };
        put(&mut a, "details", fit(&details));

        let hidden = !cfg.show_channel_name || (cfg.hide_password_channels && snap.channel_has_password);
        let channel = match (&snap.channel_name, hidden) {
            (Some(name), false) => format!("🔊 {}", clean_name(name)),
            _ => "🔊 In a channel".to_owned(),
        };
        put(&mut a, "state", fit(&channel));

        if cfg.show_channel_user_count && snap.channel_client_count > 0 {
            let count = snap.channel_client_count;
            let max = snap.channel_max_clients.unwrap_or(snap.max_clients).max(count);
            let id: String = format!("ts6:{}:{}", snap.server_uid, snap.channel_id).chars().take(128).collect();
            a.insert("party".into(), json!({ "id": id, "size": [count, max] }));
        }

        let start = match cfg.timestamp {
            TimestampMode::Server => snap.connected_at,
            TimestampMode::Channel => snap.channel_joined_at,
            TimestampMode::None => None,
        };
        if let Some(start) = start {
            a.insert("timestamps".into(), json!({ "start": start }));
        }

        if !cfg.large_image.is_empty() {
            put(&mut assets, "large_image", Some(cfg.large_image.clone()));
            let lt = match &addr_text {
                Some(at) => format!("{} – {at}", snap.server_name),
                None => snap.server_name.clone(),
            };
            put(&mut assets, "large_text", fit(&lt));
        }

        let si = &cfg.small_images;
        let small = if snap.streaming && !si.screenshare.is_empty() {
            Some((&si.screenshare, "Sharing screen".to_owned()))
        } else if snap.away && !si.away.is_empty() {
            let t = if snap.away_message.is_empty() { "Away".to_owned() } else { format!("Away: {}", snap.away_message) };
            Some((&si.away, t))
        } else if snap.output_muted && !si.deafened.is_empty() {
            Some((&si.deafened, "Sound muted".to_owned()))
        } else if snap.input_muted && !si.muted.is_empty() {
            Some((&si.muted, "Microphone muted".to_owned()))
        } else {
            None
        };
        if let Some((img, text)) = small {
            put(&mut assets, "small_image", Some(img.clone()));
            put(&mut assets, "small_text", fit(&text));
        }
    }

    if !assets.is_empty() {
        a.insert("assets".into(), Value::Object(assets));
    }
    Some(Value::Object(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_spacer_tags() {
        assert_eq!(clean_name("[cspacer1] Friends"), "Friends");
        assert_eq!(clean_name("[*spacer0]---"), "---");
        assert_eq!(clean_name("[Lobby] Talk"), "[Lobby] Talk");
        assert_eq!(clean_name("〣Voice #1"), "〣Voice #1");
    }

    #[test]
    fn fits_discord_limits() {
        assert_eq!(fit("a").unwrap(), "a ");
        assert!(fit("").is_none());
        assert_eq!(fit(&"x".repeat(200)).unwrap().chars().count(), 128);
    }

    #[test]
    fn builds_connected_activity() {
        let cfg = Config::default();
        let snap = Snapshot {
            connected: true,
            server_uid: "uid".into(),
            server_name: "nein.live | TeamSpeak Server".into(),
            max_clients: 32,
            channel_id: "119".into(),
            channel_name: Some("〣Voice #1".into()),
            channel_client_count: 2,
            streaming: true,
            connected_at: Some(1000),
            ..Default::default()
        };
        let addr = Address { host: "ts.nein.live".into(), port: 9987 };
        let a = build(Some(&snap), &cfg, Some(&addr)).unwrap();
        assert_eq!(a["details"], "nein.live | TeamSpeak Server (ts.nein.live)");
        assert_eq!(a["state"], "🔊 〣Voice #1");
        assert_eq!(a["party"]["size"], json!([2, 32]));
        assert_eq!(a["assets"]["small_text"], "Sharing screen");
        assert_eq!(a["timestamps"]["start"], 1000);
        assert!(a.get("buttons").is_none());
    }

    #[test]
    fn hides_password_channel() {
        let cfg = Config::default();
        let snap = Snapshot {
            connected: true,
            server_name: "S".into(),
            channel_name: Some("Secret".into()),
            channel_has_password: true,
            ..Default::default()
        };
        let a = build(Some(&snap), &cfg, None).unwrap();
        assert_eq!(a["state"], "🔊 In a channel");
    }

    #[test]
    fn disconnected_and_unreachable() {
        let cfg = Config::default();
        assert!(build(None, &cfg, None).is_none());
        let a = build(Some(&Snapshot::default()), &cfg, None).unwrap();
        assert_eq!(a["details"], "Not connected");
    }
}
