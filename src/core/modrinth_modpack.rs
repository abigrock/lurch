use crate::core::instance::{Instance, ModLoader};
use anyhow::Context;
use eframe::egui;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::launch::LaunchProgress;

// ── Modrinth index types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ModpackIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    pub files: Vec<ModpackFile>,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ModpackFile {
    pub path: String,
    pub hashes: HashMap<String, String>,
    pub downloads: Vec<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    pub file_size: u64,
}

// ── Dependency key constants ─────────────────────────────────────────────────

const DEP_MINECRAFT: &str = "minecraft";
const DEP_FABRIC: &str = "fabric-loader";
const DEP_QUILT: &str = "quilt-loader";
const DEP_FORGE: &str = "forge";
const DEP_NEOFORGE: &str = "neoforge";

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the Minecraft version from modpack dependencies
pub fn minecraft_version(deps: &HashMap<String, String>) -> Option<&str> {
    deps.get(DEP_MINECRAFT).map(|s| s.as_str())
}

/// Determine the mod loader and its version from modpack dependencies
pub fn determine_loader(deps: &HashMap<String, String>) -> (ModLoader, Option<String>) {
    if let Some(v) = deps.get(DEP_FABRIC) {
        (ModLoader::Fabric, Some(v.clone()))
    } else if let Some(v) = deps.get(DEP_QUILT) {
        (ModLoader::Quilt, Some(v.clone()))
    } else if let Some(v) = deps.get(DEP_NEOFORGE) {
        (ModLoader::NeoForge, Some(v.clone()))
    } else if let Some(v) = deps.get(DEP_FORGE) {
        (ModLoader::Forge, Some(v.clone()))
    } else {
        (ModLoader::Vanilla, None)
    }
}

// ── Parse mrpack ─────────────────────────────────────────────────────────────

/// Parse a .mrpack file and return the modpack index
pub fn parse_mrpack(path: &Path) -> anyhow::Result<ModpackIndex> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open mrpack: {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("Failed to read mrpack as ZIP")?;

    let mut manifest = archive
        .by_name("modrinth.index.json")
        .context("No modrinth.index.json found in mrpack")?;

    let mut content = String::new();
    manifest
        .read_to_string(&mut content)
        .context("Failed to read modrinth.index.json")?;
    drop(manifest);

    let index: ModpackIndex =
        serde_json::from_str(&content).context("Failed to parse modrinth.index.json")?;

    Ok(index)
}

// ── Install modpack files ────────────────────────────────────────────────────

/// Check if a modpack file should be installed on the client
fn is_client_file(file: &ModpackFile) -> bool {
    match &file.env {
        None => true, // no env restriction = install everywhere
        Some(env) => {
            match env.get("client").map(|s| s.as_str()) {
                Some("unsupported") => false,
                _ => true, // "required" or "optional" or missing = install
            }
        }
    }
}

/// Download a single modpack file, verify SHA1, skip if already cached
fn download_modpack_file(
    file: &ModpackFile,
    minecraft_dir: &Path,
    client: &reqwest::blocking::Client,
) -> anyhow::Result<()> {
    let dest = minecraft_dir.join(&file.path);

    // Skip if already exists with correct hash
    if dest.exists()
        && let Some(expected_sha1) = file.hashes.get("sha1")
            && let Ok(data) = std::fs::read(&dest) {
                let actual = crate::core::sha1_hex(&data);
                if actual == *expected_sha1 {
                    return Ok(());
                }
            }

    // Create parent dirs
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Try each download URL
    let url = file
        .downloads
        .first()
        .ok_or_else(|| anyhow::anyhow!("No download URLs for {}", file.path))?;

    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to download {}", file.path))?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {} downloading {}", resp.status(), file.path);
    }

    let bytes = resp.bytes()?;

    // Verify SHA1
    if let Some(expected_sha1) = file.hashes.get("sha1") {
        let actual = crate::core::sha1_hex(&bytes);
        if actual != *expected_sha1 {
            anyhow::bail!(
                "SHA1 mismatch for {}: expected {expected_sha1}, got {actual}",
                file.path
            );
        }
    }

    std::fs::write(&dest, &bytes)?;
    Ok(())
}

/// Install all modpack files (parallel, with progress reporting)
/// Then extract overrides from the mrpack zip.
/// `progress` receives (completed, total, stage_description).
pub fn install_modpack_files(
    index: &ModpackIndex,
    mrpack_path: &Path,
    minecraft_dir: &Path,
    client: &reqwest::blocking::Client,
    progress: impl Fn(usize, usize, &str) + Send + Sync,
) -> anyhow::Result<()> {
    // Filter to client-only files
    let client_files: Vec<&ModpackFile> =
        index.files.iter().filter(|f| is_client_file(f)).collect();

    let total = client_files.len();
    progress(0, total, "Downloading modpack files...");

    // Download files in parallel (8 threads)
    let completed = AtomicUsize::new(0);
    let num_threads = 8.min(total).max(1);
    let chunk_size = total.div_ceil(num_threads);

    if !client_files.is_empty() {
        std::thread::scope(|s| {
            let completed = &completed;
            let progress = &progress;
            let errors: Vec<anyhow::Error> = client_files
                .chunks(chunk_size)
                .map(|chunk| {
                    s.spawn(move || -> anyhow::Result<()> {
                        for file in chunk {
                            download_modpack_file(file, minecraft_dir, client)?;
                            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                            progress(done, total, "Downloading modpack files...");
                        }
                        Ok(())
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|h| h.join().ok())
                .filter_map(|r| r.err())
                .collect();

            if let Some(e) = errors.into_iter().next() {
                Err(e)
            } else {
                Ok(())
            }
        })?;
    }

    // Extract overrides (snapshot servers.dat first for merge)
    progress(0, 0, "Extracting overrides...");
    let servers_dat = minecraft_dir.join("servers.dat");
    let pre_existing_servers = crate::core::servers::snapshot_servers(&servers_dat);
    extract_overrides(mrpack_path, minecraft_dir, "overrides/")?;
    extract_overrides(mrpack_path, minecraft_dir, "client-overrides/")?;
    let _ = crate::core::servers::merge_modpack_servers(&servers_dat, &pre_existing_servers);

    Ok(())
}

/// Alias for the shared override extraction function.
fn extract_overrides(zip_path: &Path, dest: &Path, prefix: &str) -> anyhow::Result<()> {
    crate::core::extract_zip_overrides(zip_path, dest, prefix)
}

// ── High-level: create instance from modpack ─────────────────────────────────

/// Create a new Instance from a parsed modpack index.
/// Does NOT download files — call install_modpack_files separately.
pub fn create_instance_from_modpack(index: &ModpackIndex) -> anyhow::Result<Instance> {
    let mc_version = minecraft_version(&index.dependencies)
        .ok_or_else(|| anyhow::anyhow!("Modpack has no minecraft dependency"))?;

    let (loader, loader_version) = determine_loader(&index.dependencies);

    let mut inst = Instance::new(index.name.clone(), mc_version.to_string());
    inst.loader = loader;
    inst.loader_version = loader_version;
    inst.group = Some("Modpacks".to_string());
    inst.create_dirs()?;

    Ok(inst)
}

pub fn update_modrinth_modpack(
    client: &reqwest::blocking::Client,
    project_id: &str,
    version_id: &str,
    minecraft_dir: &std::path::Path,
    progress: &Arc<Mutex<LaunchProgress>>,
    ctx: &egui::Context,
) -> anyhow::Result<()> {
    {
        let mut p = progress.lock().unwrap();
        p.message = "Fetching new modpack version...".to_string();
    }
    ctx.request_repaint();

    let versions = super::modrinth::get_project_versions(project_id, None, None)?;
    let version = versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| anyhow::anyhow!("Version not found"))?;
    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or(version.files.first())
        .ok_or_else(|| anyhow::anyhow!("No files in modpack version"))?;

    {
        let mut p = progress.lock().unwrap();
        p.message = "Downloading modpack update...".to_string();
    }
    ctx.request_repaint();

    let temp_dir = std::env::temp_dir().join("lurch_modpack_update");
    std::fs::create_dir_all(&temp_dir)?;
    let mrpack_path = temp_dir.join(&file.filename);

    let resp = client.get(&file.url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!("Failed to download mrpack: HTTP {}", resp.status());
    }
    let bytes = resp.bytes()?;
    std::fs::write(&mrpack_path, &bytes)?;

    {
        let mut p = progress.lock().unwrap();
        p.message = "Parsing modpack...".to_string();
    }
    ctx.request_repaint();

    let index = parse_mrpack(&mrpack_path)?;

    let progress_for_files = Arc::clone(progress);
    let ctx_for_files = ctx.clone();
    install_modpack_files(
        &index,
        &mrpack_path,
        minecraft_dir,
        client,
        move |done, total, stage| {
            let mut p = progress_for_files.lock().unwrap();
            p.message = if total > 0 {
                format!("{stage} ({done}/{total})")
            } else {
                stage.to_string()
            };
            drop(p);
            ctx_for_files.request_repaint();
        },
    )?;

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}
