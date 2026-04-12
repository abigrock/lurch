//! Content-addressed mod cache with Downloads folder fallback.
//!
//! Files are stored flat by their original filename:
//! `<data_dir>/mod_cache/<original_filename>`
//!
//! SHA-1 is used for verification, not directory structure. On the rare
//! filename collision (different content, same name), the cache simply
//! overwrites — it's an optimization, not a source of truth.
//!
//! Lookup chain (when SHA-1 is known):
//! 1. Destination file already has correct content → skip
//! 2. `~/Downloads/<filename>` matches hash → cache + place
//! 3. Mod cache has the file with correct hash → copy to destination
//! 4. Download from internet → verify → cache + place

use std::path::{Path, PathBuf};

use anyhow::Context;

/// Mod cache directory: `<data_dir>/mod_cache/`
fn cache_dir() -> anyhow::Result<PathBuf> {
    Ok(crate::util::paths::data_dir()?.join("mod_cache"))
}

/// Path for a cached file: `<cache_dir>/<filename>`
fn cache_path(filename: &str) -> anyhow::Result<PathBuf> {
    Ok(cache_dir()?.join(filename))
}

/// User's Downloads directory (platform-specific).
fn downloads_dir() -> Option<PathBuf> {
    directories::UserDirs::new()?
        .download_dir()
        .map(Path::to_path_buf)
}

/// Store data in the mod cache. Best-effort — failures are silently ignored.
fn store(filename: &str, data: &[u8]) {
    if let Ok(dir) = cache_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(filename), data);
    }
}

/// Copy an existing file into the mod cache by filename.
/// Best-effort — failures are silently ignored.
pub fn cache_file(filename: &str, source: &Path) {
    if let Ok(dir) = cache_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::copy(source, dir.join(filename));
    }
}

/// Try to resolve a mod file from local sources only (no download).
///
/// Checks the same chain as `resolve_or_download` steps 1-3:
/// 1. Destination already has correct content
/// 2. `~/Downloads/<filename>` matches hash → cache + place
/// 3. Mod cache has the file with correct hash → copy to destination
///
/// Returns `true` if the file was placed at `dest`, `false` if not found locally.
/// When `sha1` is `None`, only checks if `dest` already exists.
pub fn resolve_from_cache(filename: &str, sha1: Option<&str>, dest: &Path) -> bool {
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let Some(sha1) = sha1 else {
        return dest.exists();
    };

    // 1. Already at destination with correct hash
    if dest.exists() {
        if let Ok(data) = std::fs::read(dest) {
            if crate::core::sha1_hex(&data) == sha1 {
                return true;
            }
        }
    }

    // 2. Check Downloads folder
    if let Some(downloads) = downloads_dir() {
        let candidate = downloads.join(filename);
        if candidate.exists() && candidate != dest {
            if let Ok(data) = std::fs::read(&candidate) {
                if crate::core::sha1_hex(&data) == sha1 {
                    store(filename, &data);
                    if std::fs::write(dest, &data).is_ok() {
                        return true;
                    }
                }
            }
        }
    }

    // 3. Check mod cache
    if let Ok(cached) = cache_path(filename) {
        if cached.exists() {
            if let Ok(data) = std::fs::read(&cached) {
                if crate::core::sha1_hex(&data) == sha1 {
                    if std::fs::write(dest, &data).is_ok() {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Resolve a mod file using the cache/Downloads lookup chain, falling back to download.
///
/// When `sha1` is provided:
/// 1. If `dest` already exists with the correct hash → return immediately
/// 2. Check `~/Downloads/<filename>` for a matching file → cache it, copy to `dest`
/// 3. Check the mod cache for `<filename>` with correct hash → copy to `dest`
/// 4. Call `download_fn` → verify hash → cache + write to `dest`
///
/// When `sha1` is `None`, downloads directly and caches by filename.
pub fn resolve_or_download(
    filename: &str,
    sha1: Option<&str>,
    dest: &Path,
    download_fn: impl FnOnce() -> anyhow::Result<Vec<u8>>,
) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if let Some(sha1) = sha1 {
        // 1. Already at destination with correct hash
        if dest.exists() {
            if let Ok(data) = std::fs::read(dest) {
                if crate::core::sha1_hex(&data) == sha1 {
                    return Ok(());
                }
            }
        }

        // 2. Check Downloads folder
        if let Some(downloads) = downloads_dir() {
            let candidate = downloads.join(filename);
            if candidate.exists() && candidate != dest {
                if let Ok(data) = std::fs::read(&candidate) {
                    if crate::core::sha1_hex(&data) == sha1 {
                        store(filename, &data);
                        std::fs::write(dest, &data).with_context(|| {
                            format!("Failed to copy from Downloads to {}", dest.display())
                        })?;
                        return Ok(());
                    }
                }
            }
        }

        // 3. Check mod cache
        if let Ok(cached) = cache_path(filename) {
            if cached.exists() {
                if let Ok(data) = std::fs::read(&cached) {
                    if crate::core::sha1_hex(&data) == sha1 {
                        std::fs::write(dest, &data).with_context(|| {
                            format!("Failed to copy from cache to {}", dest.display())
                        })?;
                        return Ok(());
                    }
                    // Wrong version in cache — will be overwritten if we download
                }
            }
        }

        // 4. Download
        let bytes = download_fn()?;
        let actual = crate::core::sha1_hex(&bytes);
        if actual != sha1 {
            anyhow::bail!("SHA1 mismatch: expected {sha1}, got {actual}");
        }
        store(filename, &bytes);
        std::fs::write(dest, &bytes)?;
    } else {
        // No SHA-1 available — download directly, cache by filename
        let bytes = download_fn()?;
        if dest.extension().is_some_and(|e| e == "jar") {
            crate::core::validate_jar(&bytes)
                .with_context(|| format!("Corrupt JAR download: {}", dest.display()))?;
        }
        store(filename, &bytes);
        std::fs::write(dest, &bytes)?;
    }

    Ok(())
}
