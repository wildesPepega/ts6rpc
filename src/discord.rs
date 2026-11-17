//! Minimal Discord RPC client over the local IPC socket / named pipe.
//! Frame format: [i32 LE opcode][i32 LE length][JSON payload].

use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, watch};

use crate::log;

const OP_HANDSHAKE: i32 = 0;
const OP_FRAME: i32 = 1;
const OP_CLOSE: i32 = 2;
const OP_PING: i32 = 3;
const OP_PONG: i32 = 4;

type Reader = Box<dyn AsyncRead + Unpin + Send>;
type Writer = Box<dyn AsyncWrite + Unpin + Send>;

#[cfg(unix)]
async fn open_ipc() -> Option<(Reader, Writer)> {
    use std::path::PathBuf;
    let mut bases: Vec<PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .filter_map(|k| std::env::var_os(k).map(PathBuf::from))
        .collect();
    bases.push(PathBuf::from("/tmp"));
    // Native, Flatpak, Vesktop (Flatpak) and Snap installs.
    let subdirs = ["", "app/com.discordapp.Discord", "app/dev.vencord.Vesktop", "snap.discord"];
    for base in &bases {
        for sub in subdirs {
            for i in 0..10 {
                let path = base.join(sub).join(format!("discord-ipc-{i}"));
                if let Ok(stream) = tokio::net::UnixStream::connect(&path).await {
                    let (r, w) = stream.into_split();
                    return Some((Box::new(r), Box::new(w)));
                }
            }
        }
    }
    None
}

#[cfg(windows)]
async fn open_ipc() -> Option<(Reader, Writer)> {
    use tokio::net::windows::named_pipe::ClientOptions;
    for i in 0..10 {
        let path = format!(r"\\.\pipe\discord-ipc-{i}");
        if let Ok(pipe) = ClientOptions::new().open(&path) {
            let (r, w) = tokio::io::split(pipe);
            return Some((Box::new(r), Box::new(w)));
        }
    }
    None
}

fn encode(op: i32, payload: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(payload).unwrap_or_default();
    let mut buf = Vec::with_capacity(8 + body.len());
    buf.extend_from_slice(&op.to_le_bytes());
    buf.extend_from_slice(&(body.len() as i32).to_le_bytes());
    buf.extend_from_slice(&body);
    buf
}

/// A live connection to the Discord client. Dropping it closes the connection,
/// which makes Discord remove the activity.
pub struct Discord {
    tx: mpsc::UnboundedSender<Vec<u8>>,
    closed: watch::Receiver<bool>,
    nonce: u64,
    tasks: [tokio::task::AbortHandle; 2],
}

impl Drop for Discord {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl Discord {
    pub async fn connect(client_id: &str) -> Result<Self, String> {
        let (mut reader, mut writer) = open_ipc()
            .await
            .ok_or_else(|| "Discord IPC not found (is Discord running?)".to_string())?;

        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (closed_tx, closed_rx) = watch::channel(false);
        let closed_tx = std::sync::Arc::new(closed_tx);
        let (ready_tx, ready_rx) = oneshot::channel::<()>();

        // Writer task.
        let closed_w = closed_tx.clone();
        let writer_task = tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                if writer.write_all(&frame).await.is_err() || writer.flush().await.is_err() {
                    break;
                }
            }
            let _ = writer.shutdown().await;
            let _ = closed_w.send(true);
        });

        // Reader task.
        let pong_tx = tx.clone();
        let closed_r = closed_tx.clone();
        let reader_task = tokio::spawn(async move {
            let mut ready_tx = Some(ready_tx);
            let mut head = [0u8; 8];
            loop {
                if reader.read_exact(&mut head).await.is_err() {
                    break;
                }
                let op = i32::from_le_bytes(head[0..4].try_into().unwrap());
                let len = i32::from_le_bytes(head[4..8].try_into().unwrap()).max(0) as usize;
                let mut body = vec![0u8; len];
                if reader.read_exact(&mut body).await.is_err() {
                    break;
                }
                let msg: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                match op {
                    OP_PING => {
                        let _ = pong_tx.send(encode(OP_PONG, &msg));
                    }
                    OP_CLOSE => {
                        let reason = msg["message"].as_str().unwrap_or("closed by Discord");
                        log!("Discord: {reason}");
                        break;
                    }
                    OP_FRAME => {
                        if msg["cmd"] == "DISPATCH" && msg["evt"] == "READY" {
                            if let Some(t) = ready_tx.take() {
                                let _ = t.send(());
                            }
                        } else if msg["evt"] == "ERROR" {
                            log!("Discord error: {}", msg["data"]["message"].as_str().unwrap_or("unknown"));
                        }
                    }
                    _ => {}
                }
            }
            let _ = closed_r.send(true);
        });

        let tasks = [writer_task.abort_handle(), reader_task.abort_handle()];
        let abort = |tasks: &[tokio::task::AbortHandle; 2]| tasks.iter().for_each(|t| t.abort());
        let _ = tx.send(encode(OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id })));
        let result = tokio::time::timeout(Duration::from_secs(10), ready_rx).await;
        match result {
            Ok(Ok(())) => return Ok(Self { tx, closed: closed_rx, nonce: 0, tasks }),
            _ => abort(&tasks),
        }
        match result {
            Ok(_) => Err("Discord closed the connection during handshake (wrong application ID?)".into()),
            Err(_) => Err("Discord handshake timed out".into()),
        }
    }

    pub fn set_activity(&mut self, activity: Option<&Value>) {
        self.nonce += 1;
        let frame = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": std::process::id(), "activity": activity },
            "nonce": format!("ts6rpc-{}", self.nonce),
        });
        let _ = self.tx.send(encode(OP_FRAME, &frame));
    }

    /// Resolves once the connection is gone.
    pub async fn closed(&mut self) {
        let _ = self.closed.wait_for(|c| *c).await;
    }
}
