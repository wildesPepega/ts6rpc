//! The Remote Apps API does not expose the address you connected with, so it is
//! looked up from the client's own bookmarks (settings.db, protobuf blobs) by
//! matching the server's unique identifier.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Address {
    pub host: String,
    pub port: u16,
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.port == 9987 {
            write!(f, "{}", self.host)
        } else if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// Parses "host", "host:port" or "[v6]:port".
pub fn parse_address(s: &str) -> Option<Address> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        let port = tail.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(9987);
        return Some(Address { host: host.into(), port });
    }
    match s.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => Some(Address { host: h.into(), port: p.parse().ok()? }),
        _ => Some(Address { host: s.into(), port: 9987 }),
    }
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = std::env::var_os("TS6RPC_SETTINGS_DB") {
        v.push(PathBuf::from(p));
    }
    // Linux: ~/.config, Windows: %APPDATA% (Roaming), macOS: ~/Library/Application Support
    if let Some(d) = dirs::config_dir() {
        v.push(d.join("TeamSpeak").join("Default").join("settings.db"));
    }
    // Windows: also check %LOCALAPPDATA%
    if let Some(d) = dirs::data_local_dir() {
        v.push(d.join("TeamSpeak").join("Default").join("settings.db"));
    }
    v
}

enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut r = 0u64;
    let mut shift = 0;
    loop {
        let x = *b.get(*i)?;
        *i += 1;
        if shift < 64 {
            r |= u64::from(x & 0x7f) << shift;
        }
        shift += 7;
        if x < 0x80 {
            return Some(r);
        }
    }
}

/// Decodes the top level of a protobuf message, keeping the first value of each field.
fn fields(b: &[u8]) -> Option<HashMap<u64, Field<'_>>> {
    let mut out = HashMap::new();
    let mut i = 0;
    while i < b.len() {
        let key = varint(b, &mut i)?;
        let (f, wire) = (key >> 3, key & 7);
        let v = match wire {
            0 => Field::Varint(varint(b, &mut i)?),
            2 => {
                let len = varint(b, &mut i)? as usize;
                let end = i.checked_add(len)?;
                let s = b.get(i..end)?;
                i = end;
                Field::Bytes(s)
            }
            1 => {
                i += 8;
                continue;
            }
            5 => {
                i += 4;
                continue;
            }
            _ => return None,
        };
        out.entry(f).or_insert(v);
    }
    Some(out)
}

fn parse_bookmark(blob: &[u8]) -> Option<(String, Address)> {
    let outer = fields(blob)?;
    let Field::Bytes(inner) = outer.get(&16)? else { return None };
    let f = fields(inner)?;
    let text = |n: u64| match f.get(&n) {
        Some(Field::Bytes(b)) => std::str::from_utf8(b).ok().map(str::to_owned),
        _ => None,
    };
    let host = text(2).filter(|s| !s.is_empty())?;
    let uid = text(15).filter(|s| !s.is_empty())?;
    let port = match f.get(&3) {
        Some(Field::Varint(p)) => u16::try_from(*p).unwrap_or(9987),
        _ => 9987,
    };
    Some((uid, Address { host, port }))
}

/// Returns server unique identifier -> address. Blocking; call from spawn_blocking.
pub fn read_bookmarks() -> HashMap<String, Address> {
    use rusqlite::{Connection, OpenFlags};
    let mut map = HashMap::new();
    let Some(path) = candidate_paths().into_iter().find(|p| p.exists()) else { return map };
    let Ok(db) = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
    else {
        return map;
    };
    let Ok(mut stmt) = db.prepare("SELECT CAST(value AS BLOB) FROM ProtobufItems") else { return map };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, Option<Vec<u8>>>(0)) else { return map };
    for blob in rows.flatten().flatten() {
        if let Some((uid, addr)) = parse_bookmark(&blob) {
            map.insert(uid, addr);
        }
    }
    map
}
