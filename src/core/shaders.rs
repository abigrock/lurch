use std::path::Path;

/// An installed shader pack found in the `shaderpacks/` directory.
pub struct ShaderPack {
    /// File or folder name on disk
    pub filename: String,
    /// Display title (filename without extension / `.disabled` suffix)
    pub title: String,
    /// Whether the pack is enabled (no `.disabled` suffix)
    pub enabled: bool,
    /// Whether this entry is a folder (vs a zip file)
    pub is_folder: bool,
}

/// Scan the `shaderpacks/` directory and return a sorted list of installed shader packs.
/// Treats `.zip` as enabled, `.zip.disabled` as disabled; directories are always enabled.
pub fn scan_shaderpacks(dir: &Path) -> Vec<ShaderPack> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut packs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
            continue;
        };

        if path.is_dir() {
            packs.push(ShaderPack {
                filename: filename.to_string(),
                title: filename.to_string(),
                enabled: true,
                is_folder: true,
            });
        } else if filename.ends_with(".zip") {
            let title = filename.trim_end_matches(".zip").to_string();
            packs.push(ShaderPack {
                filename: filename.to_string(),
                title,
                enabled: true,
                is_folder: false,
            });
        } else if filename.ends_with(".zip.disabled") {
            let title = filename.trim_end_matches(".zip.disabled").to_string();
            packs.push(ShaderPack {
                filename: filename.to_string(),
                title,
                enabled: false,
                is_folder: false,
            });
        }
    }
    packs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    packs
}

/// Enable a shader pack by renaming `.zip.disabled` → `.zip`
pub fn enable_shaderpack(dir: &Path, filename: &str) -> anyhow::Result<String> {
    if filename.ends_with(".zip.disabled") {
        let new_name = filename.trim_end_matches(".disabled");
        std::fs::rename(dir.join(filename), dir.join(new_name))?;
        Ok(new_name.to_string())
    } else {
        Ok(filename.to_string())
    }
}

/// Disable a shader pack by renaming `.zip` → `.zip.disabled`
pub fn disable_shaderpack(dir: &Path, filename: &str) -> anyhow::Result<String> {
    if filename.ends_with(".zip") && !filename.ends_with(".zip.disabled") {
        let new_name = format!("{filename}.disabled");
        std::fs::rename(dir.join(filename), dir.join(&new_name))?;
        Ok(new_name)
    } else {
        Ok(filename.to_string())
    }
}

/// Remove a shader pack file or folder from disk
pub fn remove_shaderpack(dir: &Path, filename: &str) -> anyhow::Result<()> {
    let path = dir.join(filename);
    if path.is_dir() {
        std::fs::remove_dir_all(&path)?;
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
