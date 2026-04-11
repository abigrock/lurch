use std::io::{Cursor, Read, Write};
use std::path::Path;

/// A Minecraft server entry (from `servers.dat`).
pub struct Server {
    pub name: String,
    pub ip: String,
    pub accept_textures: Option<bool>,
    pub hidden: bool,
}

// ── NBT tag IDs (only the subset we need) ───────────────────────────────────

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;

// ── Public API ──────────────────────────────────────────────────────────────

/// Read servers from a `servers.dat` file. Returns empty vec on any error.
pub fn read_servers(path: &Path) -> Vec<Server> {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    parse_servers_dat(&data).unwrap_or_default()
}

/// Write servers to a `servers.dat` file.
pub fn write_servers(path: &Path, servers: &[Server]) -> anyhow::Result<()> {
    let data = encode_servers_dat(servers);
    std::fs::write(path, data)?;
    Ok(())
}

/// Capture the current server list before modpack overrides extraction.
pub fn snapshot_servers(path: &Path) -> Vec<Server> {
    read_servers(path)
}

/// Merge modpack-injected servers with pre-existing ones (captured via `snapshot_servers`
/// before extraction). Deduplicates by IP (case-insensitive), keeping existing entries on
/// conflict. Returns the list of newly-added server names (empty if none).
pub fn merge_modpack_servers(path: &Path, pre_existing: &[Server]) -> Vec<String> {
    let post_extract = read_servers(path);
    if post_extract.is_empty() {
        return Vec::new();
    }

    let existing_ips: std::collections::HashSet<String> =
        pre_existing.iter().map(|s| s.ip.to_lowercase()).collect();

    let new_servers: Vec<&Server> = post_extract
        .iter()
        .filter(|s| !existing_ips.contains(&s.ip.to_lowercase()))
        .collect();

    if new_servers.is_empty() {
        if !pre_existing.is_empty() {
            let _ = write_servers(path, pre_existing);
        }
        return Vec::new();
    }

    let new_names: Vec<String> = new_servers.iter().map(|s| s.name.clone()).collect();

    let mut merged: Vec<Server> = pre_existing
        .iter()
        .map(|s| Server {
            name: s.name.clone(),
            ip: s.ip.clone(),
            accept_textures: s.accept_textures,
            hidden: s.hidden,
        })
        .collect();

    for s in new_servers {
        merged.push(Server {
            name: s.name.clone(),
            ip: s.ip.clone(),
            accept_textures: s.accept_textures,
            hidden: s.hidden,
        });
    }

    let _ = write_servers(path, &merged);
    new_names
}

// ── NBT Reader (minimal, servers.dat-specific) ─────────────────────────────

fn parse_servers_dat(data: &[u8]) -> anyhow::Result<Vec<Server>> {
    let mut cur = Cursor::new(data);

    // Root: TAG_Compound
    let tag_type = read_u8(&mut cur)?;
    if tag_type != TAG_COMPOUND {
        anyhow::bail!("Expected root TAG_Compound, got {tag_type}");
    }
    let _root_name = read_nbt_string(&mut cur)?;

    // Read tags inside root compound until TAG_End
    let mut servers = Vec::new();
    loop {
        let tag = read_u8(&mut cur)?;
        if tag == TAG_END {
            break;
        }
        let name = read_nbt_string(&mut cur)?;
        if tag == TAG_LIST && name == "servers" {
            servers = read_server_list(&mut cur)?;
        } else {
            skip_tag_payload(&mut cur, tag)?;
        }
    }
    Ok(servers)
}

fn read_server_list(cur: &mut Cursor<&[u8]>) -> anyhow::Result<Vec<Server>> {
    let list_type = read_u8(cur)?;
    let count = read_i32(cur)?;

    if count <= 0 {
        return Ok(Vec::new());
    }
    if list_type != TAG_COMPOUND {
        // Skip unknown list entries
        for _ in 0..count {
            skip_tag_payload(cur, list_type)?;
        }
        return Ok(Vec::new());
    }

    let mut servers = Vec::new();
    for _ in 0..count {
        servers.push(read_server_compound(cur)?);
    }
    Ok(servers)
}

fn read_server_compound(cur: &mut Cursor<&[u8]>) -> anyhow::Result<Server> {
    let mut name = String::new();
    let mut ip = String::new();
    let mut accept_textures: Option<bool> = None;
    let mut hidden = false;

    loop {
        let tag = read_u8(cur)?;
        if tag == TAG_END {
            break;
        }
        let key = read_nbt_string(cur)?;
        match (tag, key.as_str()) {
            (TAG_STRING, "name") => name = read_nbt_string(cur)?,
            (TAG_STRING, "ip") => ip = read_nbt_string(cur)?,
            (TAG_BYTE, "acceptTextures") => {
                accept_textures = Some(read_u8(cur)? != 0);
            }
            (TAG_BYTE, "hidden") => {
                hidden = read_u8(cur)? != 0;
            }
            _ => skip_tag_payload(cur, tag)?,
        }
    }

    Ok(Server {
        name,
        ip,
        accept_textures,
        hidden,
    })
}

// ── NBT Writer (minimal, servers.dat-specific) ─────────────────────────────

fn encode_servers_dat(servers: &[Server]) -> Vec<u8> {
    let mut buf = Vec::new();

    // Root compound
    buf.push(TAG_COMPOUND);
    write_nbt_string(&mut buf, "");

    // "servers" list
    buf.push(TAG_LIST);
    write_nbt_string(&mut buf, "servers");
    buf.push(TAG_COMPOUND); // list element type
    write_i32(&mut buf, servers.len() as i32);

    for server in servers {
        // name
        buf.push(TAG_STRING);
        write_nbt_string(&mut buf, "name");
        write_nbt_string(&mut buf, &server.name);

        // ip
        buf.push(TAG_STRING);
        write_nbt_string(&mut buf, "ip");
        write_nbt_string(&mut buf, &server.ip);

        // acceptTextures (if set)
        if let Some(accept) = server.accept_textures {
            buf.push(TAG_BYTE);
            write_nbt_string(&mut buf, "acceptTextures");
            buf.push(if accept { 1 } else { 0 });
        }

        // hidden
        if server.hidden {
            buf.push(TAG_BYTE);
            write_nbt_string(&mut buf, "hidden");
            buf.push(1);
        }

        buf.push(TAG_END); // end of this server compound
    }

    buf.push(TAG_END); // end of root compound
    buf
}

// ── Primitive I/O helpers ───────────────────────────────────────────────────

fn read_u8(cur: &mut Cursor<&[u8]>) -> anyhow::Result<u8> {
    let mut b = [0u8; 1];
    cur.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_i16(cur: &mut Cursor<&[u8]>) -> anyhow::Result<i16> {
    let mut b = [0u8; 2];
    cur.read_exact(&mut b)?;
    Ok(i16::from_be_bytes(b))
}

fn read_i32(cur: &mut Cursor<&[u8]>) -> anyhow::Result<i32> {
    let mut b = [0u8; 4];
    cur.read_exact(&mut b)?;
    Ok(i32::from_be_bytes(b))
}

fn read_nbt_string(cur: &mut Cursor<&[u8]>) -> anyhow::Result<String> {
    let len = read_i16(cur)? as usize;
    let mut buf = vec![0u8; len];
    cur.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn write_nbt_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let _ = buf.write_all(&(bytes.len() as i16).to_be_bytes());
    let _ = buf.write_all(bytes);
}

fn write_i32(buf: &mut Vec<u8>, v: i32) {
    let _ = buf.write_all(&v.to_be_bytes());
}

/// Skip over the payload of an NBT tag (for tags we don't care about).
fn skip_tag_payload(cur: &mut Cursor<&[u8]>, tag_type: u8) -> anyhow::Result<()> {
    match tag_type {
        TAG_BYTE => {
            read_u8(cur)?;
        }
        2 => {
            // TAG_Short
            read_i16(cur)?;
        }
        3 => {
            // TAG_Int
            read_i32(cur)?;
        }
        4 => {
            // TAG_Long
            let mut b = [0u8; 8];
            cur.read_exact(&mut b)?;
        }
        5 => {
            // TAG_Float
            let mut b = [0u8; 4];
            cur.read_exact(&mut b)?;
        }
        6 => {
            // TAG_Double
            let mut b = [0u8; 8];
            cur.read_exact(&mut b)?;
        }
        7 => {
            // TAG_Byte_Array
            let len = read_i32(cur)? as usize;
            let mut buf = vec![0u8; len];
            cur.read_exact(&mut buf)?;
        }
        TAG_STRING => {
            read_nbt_string(cur)?;
        }
        TAG_LIST => {
            let elem_type = read_u8(cur)?;
            let count = read_i32(cur)?;
            for _ in 0..count.max(0) {
                skip_tag_payload(cur, elem_type)?;
            }
        }
        TAG_COMPOUND => loop {
            let inner = read_u8(cur)?;
            if inner == TAG_END {
                break;
            }
            let _name = read_nbt_string(cur)?;
            skip_tag_payload(cur, inner)?;
        },
        11 => {
            // TAG_Int_Array
            let count = read_i32(cur)? as usize;
            let mut buf = vec![0u8; count * 4];
            cur.read_exact(&mut buf)?;
        }
        12 => {
            // TAG_Long_Array
            let count = read_i32(cur)? as usize;
            let mut buf = vec![0u8; count * 8];
            cur.read_exact(&mut buf)?;
        }
        _ => anyhow::bail!("Unknown NBT tag type {tag_type}"),
    }
    Ok(())
}
