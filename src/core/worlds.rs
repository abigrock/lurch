use std::path::Path;

/// A Minecraft world found in the `saves/` directory.
pub struct World {
    /// Directory name on disk
    pub dir_name: String,
    /// Display name (same as dir_name — NBT parsing not available)
    pub display_name: String,
    /// Total size in bytes (best-effort, 0 on error)
    pub size_bytes: u64,
    /// Last modified time as a formatted string, or empty
    pub last_modified: String,
}

/// Scan the `saves/` directory and return a sorted list of worlds.
/// A valid world is a subdirectory containing `level.dat`.
pub fn scan_worlds(dir: &Path) -> Vec<World> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut worlds = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Only include directories that contain level.dat
        if !path.join("level.dat").exists() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|f| f.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let size_bytes = dir_size(&path);
        let last_modified = path
            .join("level.dat")
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .map(format_system_time)
            .unwrap_or_default();

        worlds.push(World {
            display_name: dir_name.clone(),
            dir_name,
            size_bytes,
            last_modified,
        });
    }
    worlds.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    worlds
}

/// Delete a world directory
pub fn remove_world(dir: &Path, world_dir_name: &str) -> anyhow::Result<()> {
    let path = dir.join(world_dir_name);
    if path.is_dir() {
        std::fs::remove_dir_all(&path)?;
    }
    Ok(())
}

/// Recursively compute directory size (best-effort)
fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Format a SystemTime as a human-readable date string
fn format_system_time(time: std::time::SystemTime) -> String {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Simple date formatting without chrono dependency
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;

    // Approximate date from epoch days (good enough for display)
    let (year, month, day) = epoch_days_to_date(days);
    format!("{year}-{month:02}-{day:02} {hours:02}:{minutes:02}")
}

/// Convert days since Unix epoch to (year, month, day)
fn epoch_days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

/// Format bytes into a human-readable size string
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
