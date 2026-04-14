pub mod account;
pub mod config;
pub mod curseforge;
pub mod curseforge_modpack;
pub mod forge;
pub mod import_export;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loader_profiles;
pub mod local_mods;
pub mod mod_cache;
pub mod modrinth;
pub mod modrinth_modpack;
pub mod servers;
pub mod shaders;
pub mod version;
pub mod update;
pub mod worlds;

// ── Modpack mod manifest entry ──────────────────────────────────────────────

/// A single entry in `.modpack_mods.json`, the modpack mod manifest.
///
/// Stores enough information to re-download a missing mod:
/// - For directly downloadable mods: `download_url` is the CDN URL.
/// - For distribution-blocked CurseForge mods: `manual` is true, and
///   `slug` / `file_id` / `website_url` allow constructing the manual download page.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModpackModEntry {
    /// Filename inside the `mods/` directory (e.g. `"fabric-api-0.92.jar"`).
    pub name: String,
    /// Direct download URL (CDN).  `None` for distribution-blocked mods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// Human-readable display name (for UI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// `true` when the mod requires manual download (CurseForge distribution-blocked).
    #[serde(default, skip_serializing_if = "is_false")]
    pub manual: bool,
    /// `true` when the mod is disabled.
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    /// CurseForge project slug (for constructing manual download URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// CurseForge file ID (for constructing manual download URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<u64>,
    /// CurseForge project website URL (for constructing manual download URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
}

fn is_false(v: &bool) -> bool {
    !v
}

// ── Background task type alias ──────────────────────────────────────────────

/// Shared slot for a background task result, polled each frame.
///
/// Wrap in `Option<…>` for struct fields (None = no active task).
pub type BgTaskSlot<T> = std::sync::Arc<std::sync::Mutex<Option<Result<T, String>>>>;

// ── Shared constants & helpers ──────────────────────────────────────────────

/// User-Agent string sent with all outgoing HTTP requests.
pub const USER_AGENT: &str = "lurch/0.1.0 (kchristensen1@proton.me)";

/// Build a [`reqwest::blocking::Client`] with a consistent User-Agent and 30 s timeout.
pub fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client")
}

/// SHA-1 hex digest of the given bytes.
pub fn sha1_hex(data: &[u8]) -> String {
    sha1_smol::Sha1::from(data).hexdigest()
}

/// Verify that `data` is a readable ZIP/JAR archive.
///
/// Call after downloading a `.jar` to catch truncated or corrupt files
/// before they are persisted to disk.
pub fn validate_jar(data: &[u8]) -> anyhow::Result<()> {
    use std::io::Cursor;
    zip::ZipArchive::new(Cursor::new(data))
        .map_err(|e| anyhow::anyhow!("corrupt JAR: {e}"))?;
    Ok(())
}

/// Quick-check that a `.jar` file on disk is a structurally valid ZIP archive.
///
/// Reads only the central directory (seeks to end of file), so it is lightweight
/// even for large JARs.  Returns `false` for missing, empty, truncated, or corrupt files.
pub fn is_jar_valid(path: &std::path::Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    zip::ZipArchive::new(file).is_ok()
}

/// Strip ANSI escape sequences from a string (e.g. `\x1b[33m`).
///
/// Handles CSI sequences (`ESC [ ... final_byte`) which cover colors,
/// cursor movement, and other terminal control codes Minecraft emits.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume CSI sequence: ESC [ <params> <final byte>
            if let Some(next) = chars.next()
                && next == '[' {
                    // Skip until final byte (ASCII 0x40..0x7E)
                    for ch in chars.by_ref() {
                        if ch.is_ascii() && ('@'..='~').contains(&ch) {
                            break;
                        }
                    }
                }
                // else: non-CSI escape — drop ESC + next char
        } else {
            out.push(c);
        }
    }
    out
}

/// Convert a Maven coordinate to a relative filesystem path.
///
/// Handles all forms:
/// - `group:artifact:version` → `group/artifact/version/artifact-version.jar`
/// - `group:artifact:version:classifier` → `.../artifact-version-classifier.jar`
/// - `group:artifact:version:classifier@ext` → `.../artifact-version-classifier.ext`
pub fn maven_path(name: &str) -> Option<String> {
    let (name_part, ext) = match name.rfind('@') {
        Some(at) => (&name[..at], &name[at + 1..]),
        None => (name, "jar"),
    };

    let parts: Vec<&str> = name_part.split(':').collect();
    if parts.len() < 3 {
        return None;
    }

    let group_path = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();

    let filename = match classifier {
        Some(cls) => format!("{artifact}-{version}-{cls}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };

    Some(format!("{group_path}/{artifact}/{version}/{filename}"))
}

// ── Windows console suppression ────────────────────────────────────────────

/// Extension trait to suppress console window creation on Windows.
///
/// When a GUI application spawns a console program (e.g. `java.exe`),
/// Windows creates a visible console window by default.  Calling
/// `.no_console_window()` on a [`Command`] sets the `CREATE_NO_WINDOW`
/// creation flag to prevent this.  No-op on non-Windows platforms.
pub trait CommandHideConsole {
    fn no_console_window(&mut self) -> &mut Self;
}

impl CommandHideConsole for std::process::Command {
    fn no_console_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            self.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        self
    }
}

// ── Mutex poison recovery ──────────────────────────────────────────────────

use std::sync::Mutex;

/// Extension trait that adds poison-recovering lock to [`Mutex`].
///
/// All shared state in Lurch is simple (progress strings, result slots,
/// log buffers) — not transactional — so it is always safe to access after
/// a panic on the other side.  Recovering avoids cascade panics that would
/// crash the whole app when a background thread fails.
pub trait MutexExt<T> {
    /// Lock the mutex, recovering from poison rather than panicking.
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Extract files matching a given prefix from a zip archive into `dest_dir`.
///
/// Used by both Modrinth (`"overrides/"`, `"client-overrides/"`) and CurseForge
/// (`"overrides/"`) modpack installers.
pub fn extract_zip_overrides(
    zip_path: &std::path::Path,
    dest_dir: &std::path::Path,
    prefix: &str,
) -> anyhow::Result<()> {
    use std::io::Read;
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(name) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue; // skip unsafe paths
        };
        let name_str = name.to_string_lossy();

        if !name_str.starts_with(prefix) {
            continue;
        }

        let relative = match name_str.strip_prefix(prefix) {
            Some(r) if !r.is_empty() => r.to_string(),
            _ => continue,
        };

        let dest = dest_dir.join(&relative);

        // Path traversal protection
        if !dest.starts_with(dest_dir) {
            continue;
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            std::fs::write(&dest, &buf)?;
        }
    }

    Ok(())
}
