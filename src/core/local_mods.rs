use crate::core::instance::ModOrigin;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Local mod tracking ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub filename: String,
    /// "modrinth", "curseforge", or "local"
    pub source: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub title: String,
    pub enabled: bool,
}

// ── Local mod management ────────────────────────────────────────────────────

/// Scan the mods directory and return a list of installed mods.
/// Treats .jar as enabled, .jar.disabled as disabled.
pub fn scan_installed_mods(mods_dir: &Path, mod_origins: &[ModOrigin]) -> Vec<InstalledMod> {
    let entries = match std::fs::read_dir(mods_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut mods = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
            continue;
        };

        let (is_jar, enabled) = if filename.ends_with(".jar") {
            (true, true)
        } else if filename.ends_with(".jar.disabled") {
            (true, false)
        } else {
            continue;
        };

        if is_jar {
            let title = filename
                .trim_end_matches(".jar.disabled")
                .trim_end_matches(".jar")
                .to_string();
            let base_name = filename.strip_suffix(".disabled").unwrap_or(filename);
            let origin = mod_origins.iter().find(|o| o.filename == base_name);
            mods.push(InstalledMod {
                filename: filename.to_string(),
                source: origin
                    .map(|o| o.source.clone())
                    .unwrap_or_else(|| "local".to_string()),
                project_id: origin.and_then(|o| o.project_id.clone()),
                version_id: origin.and_then(|o| o.version_id.clone()),
                title,
                enabled,
            });
        }
    }
    mods.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    mods
}

/// Enable a mod by renaming .jar.disabled → .jar
pub fn enable_mod(mods_dir: &Path, filename: &str) -> anyhow::Result<String> {
    if filename.ends_with(".jar.disabled") {
        let new_name = filename.trim_end_matches(".disabled");
        std::fs::rename(mods_dir.join(filename), mods_dir.join(new_name))?;
        Ok(new_name.to_string())
    } else {
        Ok(filename.to_string())
    }
}

/// Disable a mod by renaming .jar → .jar.disabled
pub fn disable_mod(mods_dir: &Path, filename: &str) -> anyhow::Result<String> {
    if filename.ends_with(".jar") && !filename.ends_with(".jar.disabled") {
        let new_name = format!("{filename}.disabled");
        std::fs::rename(mods_dir.join(filename), mods_dir.join(&new_name))?;
        Ok(new_name)
    } else {
        Ok(filename.to_string())
    }
}

/// Remove a mod file from disk
pub fn remove_mod(mods_dir: &Path, filename: &str) -> anyhow::Result<()> {
    let path = mods_dir.join(filename);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Build a web URL for a mod given its source and project_id.
/// Returns `None` for local mods or missing project_id.
pub fn mod_project_url(source: &str, project_id: &str) -> Option<String> {
    match source {
        "modrinth" => Some(format!("https://modrinth.com/project/{project_id}")),
        "curseforge" => Some(format!("https://www.curseforge.com/projects/{project_id}")),
        _ => None,
    }
}

/// Build a web URL for a modpack given its source and project_id.
pub fn modpack_project_url(source: &str, project_id: &str) -> Option<String> {
    mod_project_url(source, project_id)
}
