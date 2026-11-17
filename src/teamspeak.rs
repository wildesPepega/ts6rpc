//! Client for the TeamSpeak 5/6 "Remote Apps" WebSocket API (default ws://127.0.0.1:5899).
//! Keeps a model of servers, channels and clients and publishes a [`Snapshot`]
//! of what the presence should show.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

use crate::{log, now_ms};

/// 0 disconnected, 1 connecting, 2 connected, 3 establishing, 4 established
const STATUS_CONNECTED: i64 = 4;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    /// false = client running but not on a server
    pub connected: bool,
    pub server_uid: String,
    pub server_name: String,
    pub max_clients: i64,
    pub channel_id: String,
    pub channel_name: Option<String>,
    pub channel_has_password: bool,
    pub channel_max_clients: Option<i64>,
    pub channel_client_count: i64,
    pub input_muted: bool,
    pub output_muted: bool,
    pub away: bool,
    pub away_message: String,
    pub streaming: bool,
    pub connected_at: Option<u64>,
    pub channel_joined_at: Option<u64>,
}

#[derive(Default)]
struct Client {
    channel_id: String,
    props: Map<String, Value>,
    joined_at: Option<u64>,
}

#[derive(Default)]
struct Connection {
    status: i64,
    own_client_id: Option<i64>,
    server: Map<String, Value>,
    channels: HashMap<String, Map<String, Value>>,
    clients: HashMap<i64, Client>,
    connected_at: Option<u64>,
}

#[derive(Default)]
struct Model {
    connections: HashMap<i64, Connection>,
    current: Option<i64>,
}

fn as_i64(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str()?.parse().ok())
}
fn as_id_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
fn prop_bool(m: &Map<String, Value>, k: &str) -> bool {
    match m.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        Some(Value::String(s)) => s == "1" || s == "true",
        _ => false,
    }
}
fn merge(dst: &mut Map<String, Value>, src: &Value) {
    if let Some(o) = src.as_object() {
        for (k, v) in o {
            dst.insert(k.clone(), v.clone());
        }
    }
}

impl Model {
    fn conn(&mut self, id: &Value) -> &mut Connection {
        self.connections.entry(as_i64(id).unwrap_or(0)).or_default()
    }

    fn load_channels(c: &mut Connection, info: &Value) {
        let mut add = |ch: &Value| {
            let mut props = ch["properties"].as_object().cloned().unwrap_or_default();
            props.insert("parentId".into(), Value::String(as_id_str(&ch["parentId"])));
            c.channels.insert(as_id_str(&ch["id"]), props);
        };
        if let Some(list) = info["rootChannels"].as_array() {
            list.iter().for_each(&mut add);
        }
        if let Some(subs) = info["subChannels"].as_object() {
            subs.values().filter_map(Value::as_array).flatten().for_each(&mut add);
        }
    }

    fn load_connection(&mut self, raw: &Value) {
        let now = now_ms();
        let c = self.conn(&raw["id"]);
        c.status = as_i64(&raw["status"]).unwrap_or(c.status);
        c.own_client_id = as_i64(&raw["clientId"]).or(c.own_client_id);
        if let Some(p) = raw["properties"].as_object() {
            c.server = p.clone();
        }
        Self::load_channels(c, &raw["channelInfos"]);
        for cl in raw["clientInfos"].as_array().into_iter().flatten() {
            if let Some(id) = as_i64(&cl["id"]) {
                c.clients.insert(
                    id,
                    Client {
                        channel_id: as_id_str(&cl["channelId"]),
                        props: cl["properties"].as_object().cloned().unwrap_or_default(),
                        joined_at: None,
                    },
                );
            }
        }
        if c.status == STATUS_CONNECTED && c.connected_at.is_none() {
            c.connected_at = Some(now);
        }
        if let Some(me) = c.own_client_id.and_then(|id| c.clients.get_mut(&id)) {
            me.joined_at.get_or_insert(now);
        }
    }

    /// Applies one event. Returns an error message for a failed auth.
    fn handle(&mut self, ty: &str, p: &Value, status: &Value) -> Result<Option<String>, String> {
        match ty {
            "auth" => {
                if let Some(code) = as_i64(&status["code"]).filter(|c| *c != 0) {
                    return Err(format!("auth failed ({code}): {}", status["message"].as_str().unwrap_or("")));
                }
                self.connections.clear();
                for raw in p["connections"].as_array().into_iter().flatten() {
                    self.load_connection(raw);
                }
                self.current = as_i64(&p["currentConnectionId"]);
                return Ok(p["apiKey"].as_str().map(str::to_owned));
            }
            "connectStatusChanged" => {
                let id = as_i64(&p["connectionId"]).unwrap_or(0);
                let st = as_i64(&p["status"]).unwrap_or(0);
                if st == 0 {
                    self.connections.remove(&id);
                } else {
                    let c = self.conn(&p["connectionId"]);
                    c.status = st;
                    if let Some(cid) = as_i64(&p["info"]["clientId"]) {
                        c.own_client_id = Some(cid);
                    }
                    if let Some(sp) = p["info"]["serverProperties"].as_object() {
                        c.server = sp.clone();
                    }
                    if st == STATUS_CONNECTED && c.connected_at.is_none() {
                        c.connected_at = Some(now_ms());
                    }
                }
                self.current = Some(id);
            }
            "serverPropertiesUpdated" => merge(&mut self.conn(&p["connectionId"]).server, &p["properties"]),
            "channels" => Self::load_channels(self.conn(&p["connectionId"]), &p["info"]),
            "channelCreated" | "channelEdited" | "channelMoved" | "channelPropertiesUpdated" => {
                let c = self.conn(&p["connectionId"]);
                let id = as_id_str(if p["channelId"].is_null() { &p["id"] } else { &p["channelId"] });
                let ch = c.channels.entry(id).or_default();
                merge(ch, &p["properties"]);
                if !p["parentId"].is_null() {
                    ch.insert("parentId".into(), Value::String(as_id_str(&p["parentId"])));
                }
            }
            "channelDeleted" => {
                let id = as_id_str(if p["channelId"].is_null() { &p["id"] } else { &p["channelId"] });
                self.conn(&p["connectionId"]).channels.remove(&id);
            }
            "clientMoved" => {
                let c = self.conn(&p["connectionId"]);
                let Some(id) = as_i64(&p["clientId"]) else { return Ok(None) };
                let new_ch = as_id_str(&p["newChannelId"]);
                if new_ch.is_empty() || new_ch == "0" {
                    c.clients.remove(&id);
                } else {
                    let is_me = c.own_client_id == Some(id);
                    let cl = c.clients.entry(id).or_default();
                    merge(&mut cl.props, &p["properties"]);
                    if is_me && cl.channel_id != new_ch {
                        cl.joined_at = Some(now_ms());
                    }
                    cl.channel_id = new_ch;
                }
            }
            "clientPropertiesUpdated" => {
                let c = self.conn(&p["connectionId"]);
                if let Some(id) = as_i64(&p["clientId"]) {
                    merge(&mut c.clients.entry(id).or_default().props, &p["properties"]);
                }
            }
            "clientSelfPropertyUpdated" => {
                // { connectionId, flag, oldValue, newValue }; flags match client property names.
                let c = self.conn(&p["connectionId"]);
                if let (Some(me), Some(flag)) = (c.own_client_id.and_then(|id| c.clients.get_mut(&id)), p["flag"].as_str()) {
                    me.props.insert(flag.to_owned(), p["newValue"].clone());
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn snapshot(&self) -> Snapshot {
        let mut connected: Vec<(&i64, &Connection)> = self
            .connections
            .iter()
            .filter(|(_, c)| c.status == STATUS_CONNECTED && c.own_client_id.is_some())
            .collect();
        connected.sort_by_key(|(id, _)| **id);
        let Some((_, c)) = connected.iter().find(|(id, _)| Some(**id) == self.current).or(connected.first()).copied()
        else {
            return Snapshot::default();
        };
        let me = c.own_client_id.and_then(|id| c.clients.get(&id));
        let channel = me.and_then(|m| c.channels.get(&m.channel_id));
        let in_channel = me.map_or(0, |m| {
            c.clients
                .values()
                .filter(|x| x.channel_id == m.channel_id && as_i64(x.props.get("type").unwrap_or(&Value::Null)) != Some(1))
                .count() as i64
        });
        let mp = me.map(|m| &m.props);
        let mb = |k: &str| mp.is_some_and(|p| prop_bool(p, k));
        Snapshot {
            connected: true,
            server_uid: c.server.get("uniqueIdentifier").and_then(Value::as_str).unwrap_or("").into(),
            server_name: c
                .server
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("TeamSpeak")
                .into(),
            max_clients: c.server.get("maxClients").and_then(as_i64).unwrap_or(0),
            channel_id: me.map(|m| m.channel_id.clone()).unwrap_or_default(),
            channel_name: channel.and_then(|ch| ch.get("name")?.as_str().map(str::to_owned)),
            channel_has_password: channel.is_some_and(|ch| prop_bool(ch, "flagPassword")),
            channel_max_clients: channel
                .filter(|ch| !prop_bool(ch, "flagMaxclientsUnlimited"))
                .and_then(|ch| ch.get("maxclients").and_then(as_i64))
                .filter(|n| *n > 0),
            channel_client_count: in_channel,
            input_muted: mb("inputMuted") || mb("inputDeactivated") || mp.is_some_and(|p| p.get("inputHardware") == Some(&Value::Bool(false))),
            output_muted: mb("outputMuted"),
            away: mb("away"),
            away_message: mp.and_then(|p| p.get("awayMessage")?.as_str().map(str::to_owned)).unwrap_or_default(),
            // Flips with your own screenshare (stop -> false, start -> true).
            streaming: mb("isStreaming"),
            connected_at: c.connected_at,
            channel_joined_at: me.and_then(|m| m.joined_at).or(c.connected_at),
        }
    }
}

pub struct AppInfo {
    pub identifier: &'static str,
    pub version: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// Runs forever: connects to TeamSpeak, reconnects on failure, and publishes snapshots.
/// `None` = TeamSpeak not reachable / not authorized yet.
pub async fn run(
    url: String,
    mut api_key: String,
    app: AppInfo,
    debug: bool,
    on_api_key: impl Fn(&str),
    tx: watch::Sender<Option<Snapshot>>,
) {
    let mut announced_down = false;
    loop {
        match tokio_tungstenite::connect_async(url.as_str()).await {
            Ok((mut ws, _)) => {
                announced_down = false;
                log!("TeamSpeak: connected (on first run, accept \"{}\" in the TeamSpeak client)", app.name);
                let auth = json!({
                    "type": "auth",
                    "payload": {
                        "identifier": app.identifier,
                        "version": app.version,
                        "name": app.name,
                        "description": app.description,
                        "content": { "apiKey": api_key },
                    }
                });
                if ws.send(Message::text(auth.to_string())).await.is_ok() {
                    let mut model = Model::default();
                    let mut authed = false;
                    while let Some(Ok(msg)) = ws.next().await {
                        let Message::Text(text) = msg else { continue };
                        let Ok(v) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                        let ty = v["type"].as_str().unwrap_or("");
                        if matches!(ty, "log" | "screenshareSessionEvent") || ty.starts_with("proto.") || ty.starts_with("stream") {
                            continue;
                        }
                        if debug && ty != "auth" && ty != "talkStatusChanged" {
                            let s = v["payload"].to_string();
                            log!("TS event {ty} {}", s.chars().take(300).collect::<String>());
                        }
                        match model.handle(ty, &v["payload"], &v["status"]) {
                            Ok(new_key) => {
                                if ty == "auth" {
                                    authed = true;
                                    log!("TeamSpeak: authorized");
                                    if let Some(k) = new_key.filter(|k| *k != api_key) {
                                        api_key = k;
                                        on_api_key(&api_key);
                                    }
                                }
                            }
                            Err(e) => log!("TeamSpeak: {e}"),
                        }
                        if authed {
                            let snap = model.snapshot();
                            tx.send_if_modified(|cur| {
                                if cur.as_ref() == Some(&snap) {
                                    false
                                } else {
                                    *cur = Some(snap);
                                    true
                                }
                            });
                        }
                    }
                }
                log!("TeamSpeak: connection closed");
                tx.send_if_modified(|cur| cur.take().is_some());
            }
            Err(_) => {
                if !announced_down {
                    log!("TeamSpeak: not reachable at {url}, retrying…");
                    announced_down = true;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
