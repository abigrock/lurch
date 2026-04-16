use super::MutexExt;
use crate::core::curseforge;
use crate::core::instance::{Instance, ModLoader};
use anyhow::Context;
use eframe::egui;
use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::launch::LaunchProgress;

// ── Skipped mod info (distribution-blocked mods) ─────────────────────────────

/// A mod that was skipped during install because it doesn't allow third-party distribution.
#[derive(Debug, Clone)]
pub struct SkippedMod {
    /// The expected filename from the CurseForge API.
    pub file_name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// CurseForge file ID.
    pub file_id: u64,
    /// CurseForge project slug (used for manual download URL).
    pub slug: String,
    /// CurseForge project website URL (used to generate correct download URL).
    pub website_url: Option<String>,
}

// ── CurseForge manifest types ────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct CfManifest {
    pub minecraft: CfManifestMinecraft,
    pub manifest_type: String,
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub author: String,
    pub files: Vec<CfManifestFile>,
    pub overrides: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfManifestMinecraft {
    pub version: String,
    pub mod_loaders: Vec<CfManifestModLoader>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfManifestModLoader {
    pub id: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfManifestFile {
    #[serde(alias = "projectID")]
    pub project_id: u64,
    #[serde(alias = "fileID")]
    pub file_id: u64,
    pub required: bool,
}

// ── Parse CurseForge modpack ─────────────────────────────────────────────────

/// Parse a CurseForge modpack ZIP and return its manifest.
pub fn parse_cf_modpack(path: &Path) -> anyhow::Result<CfManifest> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open CF modpack: {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("Failed to read modpack as ZIP")?;

    let mut manifest = archive
        .by_name("manifest.json")
        .context("No manifest.json found in CurseForge modpack")?;

    let mut content = String::new();
    manifest
        .read_to_string(&mut content)
        .context("Failed to read manifest.json")?;
    drop(manifest);

    let manifest: CfManifest =
        serde_json::from_str(&content).context("Failed to parse manifest.json")?;

    Ok(manifest)
}

// ── Loader parsing ───────────────────────────────────────────────────────────

/// Parse a CurseForge modLoader id like "forge-47.2.0" or "neoforge-21.1.77"
/// or "fabric-0.16.0" into (ModLoader, version_string).
pub fn parse_loader_id(id: &str) -> (ModLoader, Option<String>) {
    if let Some(version) = id.strip_prefix("forge-") {
        (ModLoader::Forge, Some(version.to_string()))
    } else if let Some(version) = id.strip_prefix("neoforge-") {
        (ModLoader::NeoForge, Some(version.to_string()))
    } else if let Some(version) = id.strip_prefix("fabric-") {
        (ModLoader::Fabric, Some(version.to_string()))
    } else if let Some(version) = id.strip_prefix("quilt-") {
        (ModLoader::Quilt, Some(version.to_string()))
    } else {
        (ModLoader::Vanilla, None)
    }
}

// ── Instance creation ────────────────────────────────────────────────────────

/// Create a new Instance from a parsed CurseForge manifest.
/// Does NOT download files — call `install_cf_modpack_files` separately.
pub fn create_instance_from_cf_modpack(manifest: &CfManifest) -> anyhow::Result<Instance> {
    let mc_version = &manifest.minecraft.version;

    let (loader, loader_version) = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|ml| ml.primary)
        .or(manifest.minecraft.mod_loaders.first())
        .map(|ml| parse_loader_id(&ml.id))
        .unwrap_or((ModLoader::Vanilla, None));

    let mut inst = Instance::new(manifest.name.clone(), mc_version.clone());
    inst.loader = loader;
    inst.loader_version = loader_version;
    inst.group = Some("Modpacks".to_string());
    inst.create_dirs()?;

    Ok(inst)
}

// ── File installation ────────────────────────────────────────────────────────

/// Install all CurseForge modpack files and extract overrides.
/// Returns a list of mods that were skipped because they block third-party distribution.
/// `progress` receives (completed, total, stage_description).
pub fn install_cf_modpack_files(
    manifest: &CfManifest,
    zip_path: &Path,
    minecraft_dir: &Path,
    progress: impl Fn(usize, usize, &str) -> bool + Send + Sync,
) -> anyhow::Result<Vec<SkippedMod>> {
    let required_files: Vec<&CfManifestFile> =
        manifest.files.iter().filter(|f| f.required).collect();
    let total = required_files.len();

    // Step 1: Batch-resolve file IDs via CurseForge API
    if !progress(0, total, "Resolving mod files...") {
        anyhow::bail!("Cancelled");
    }

    let file_ids: Vec<u64> = required_files.iter().map(|f| f.file_id).collect();
    let resolved = curseforge::batch_get_files(&file_ids)?;

    // Build a map from file_id -> CfFile for quick lookup
    let file_map: std::collections::HashMap<u64, &curseforge::CfFile> =
        resolved.iter().map(|f| (f.id, f)).collect();

    // Step 2: Batch-fetch project info to route files to correct directories
    //         (mods → mods/, shaders → shaderpacks/, resource packs → resourcepacks/)
    if !progress(0, total, "Resolving project types...") {
        anyhow::bail!("Cancelled");
    }
    let project_ids: Vec<u64> = required_files
        .iter()
        .map(|f| f.project_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mod_infos = curseforge::batch_get_mods(&project_ids)?;
    let class_map: std::collections::HashMap<u64, u32> = mod_infos
        .iter()
        .filter_map(|m| m.class_id.map(|c| (m.id, c)))
        .collect();
    let distribution_map: std::collections::HashMap<u64, bool> = mod_infos
        .iter()
        .filter_map(|m| m.allow_mod_distribution.map(|a| (m.id, a)))
        .collect();
    let slug_map: std::collections::HashMap<u64, String> =
        mod_infos.iter().map(|m| (m.id, m.slug.clone())).collect();
    let website_url_map: std::collections::HashMap<u64, Option<String>> = mod_infos
        .iter()
        .map(|m| (m.id, m.links.as_ref().and_then(|l| l.website_url.clone())))
        .collect();

    // Step 3: Download files in parallel (8 threads)
    // (progress(0, ...) will be called after building download_msg below)

    let completed = AtomicUsize::new(0);
    let mods_dir = minecraft_dir.join("mods");
    let resourcepacks_dir = minecraft_dir.join("resourcepacks");
    let shaderpacks_dir = minecraft_dir.join("shaderpacks");
    std::fs::create_dir_all(&mods_dir)?;
    std::fs::create_dir_all(&resourcepacks_dir)?;
    std::fs::create_dir_all(&shaderpacks_dir)?;

    // Separate blocked mods from downloadable files
    let mut downloadable: Vec<&curseforge::CfFile> = Vec::new();
    let mut skipped_mods: Vec<SkippedMod> = Vec::new();

    for mf in &required_files {
        let Some(cf_file) = file_map.get(&mf.file_id).copied() else {
            log::warn!(
                "CurseForge file {} not found in batch response, skipping",
                mf.file_id
            );
            continue;
        };
        // Check both file-level (download_url == null) and project-level
        // (allowModDistribution == false) distribution restrictions.
        // Some mods set the project flag but the API still returns a download URL.
        let distribution_blocked =
            cf_file.download_url.is_none() || distribution_map.get(&mf.project_id) == Some(&false);
        if distribution_blocked {
            // Try to resolve from local cache / Downloads before requiring manual download.
            let dest_dir = match class_map.get(&cf_file.mod_id).copied() {
                Some(curseforge::CLASS_RESOURCE_PACKS) => &resourcepacks_dir,
                Some(curseforge::CLASS_SHADERS) => &shaderpacks_dir,
                _ => &mods_dir,
            };
            let dest = dest_dir.join(&cf_file.file_name);
            let sha1 = cf_file
                .hashes
                .iter()
                .find(|h| h.algo == 1)
                .map(|h| h.value.as_str());
            if crate::core::mod_cache::resolve_from_cache(&cf_file.file_name, sha1, &dest) {
                log::info!(
                    "Mod \"{}\" resolved from cache (distribution-blocked but locally available)",
                    cf_file.display_name
                );
                downloadable.push(cf_file);
            } else {
                log::warn!(
                    "Mod \"{}\" does not allow 3rd-party distribution. User must download manually",
                    cf_file.display_name
                );
                skipped_mods.push(SkippedMod {
                    file_name: cf_file.file_name.clone(),
                    display_name: cf_file.display_name.clone(),
                    file_id: mf.file_id,
                    slug: slug_map.get(&mf.project_id).cloned().unwrap_or_default(),
                    website_url: website_url_map.get(&mf.project_id).cloned().flatten(),
                });
            }
        } else {
            downloadable.push(cf_file);
        }
    }

    let download_total = downloadable.len();
    let download_msg = if skipped_mods.is_empty() {
        "Downloading modpack files...".to_string()
    } else {
        format!(
            "Downloading files ({} skipped, blocked)...",
            skipped_mods.len()
        )
    };

    if !progress(0, download_total, &download_msg) {
        anyhow::bail!("Cancelled");
    }

    let num_threads = 8.min(download_total).max(1);
    let chunk_size = download_total.div_ceil(num_threads);

    if !downloadable.is_empty() {
        std::thread::scope(|s| {
            let completed = &completed;
            let progress = &progress;
            let class_map = &class_map;
            let mods_dir = &mods_dir;
            let resourcepacks_dir = &resourcepacks_dir;
            let shaderpacks_dir = &shaderpacks_dir;
            let download_msg = &download_msg;
            let errors: Vec<anyhow::Error> = downloadable
                .chunks(chunk_size)
                .map(|chunk| {
                    s.spawn(move || -> anyhow::Result<()> {
                        let client = reqwest::blocking::Client::builder()
                            .user_agent(crate::core::USER_AGENT)
                            .connect_timeout(std::time::Duration::from_secs(10))
                            .timeout(std::time::Duration::from_secs(300))
                            .build()?;

                        for cf_file in chunk {
                            // Route to correct directory based on project classId
                            let dest_dir = match class_map.get(&cf_file.mod_id).copied() {
                                Some(curseforge::CLASS_RESOURCE_PACKS) => resourcepacks_dir,
                                Some(curseforge::CLASS_SHADERS) => shaderpacks_dir,
                                _ => mods_dir,
                            };
                            let dest = dest_dir.join(&cf_file.file_name);

                            let sha1 = cf_file
                                .hashes
                                .iter()
                                .find(|h| h.algo == 1)
                                .map(|h| h.value.as_str());
                            crate::core::mod_cache::resolve_or_download(
                                &cf_file.file_name,
                                sha1,
                                &dest,
                                || {
                                    let url = cf_file.download_url.as_ref().ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "No download URL for {} (distribution-blocked)",
                                            cf_file.file_name
                                        )
                                    })?;
                                    let resp = client.get(url).send().with_context(|| {
                                        format!("Failed to download {}", cf_file.file_name)
                                    })?;
                                    if !resp.status().is_success() {
                                        anyhow::bail!(
                                            "HTTP {} downloading {}",
                                            resp.status(),
                                            cf_file.file_name
                                        );
                                    }
                                    Ok(resp.bytes()?.to_vec())
                                },
                            )?;

                            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                            if !progress(done, download_total, download_msg) {
                                anyhow::bail!("Cancelled");
                            }
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

    // Persist list of modpack mod filenames for pre-launch integrity check.
    // Include both successfully downloaded mods and skipped (distribution-blocked) mods.
    let mod_entries: Vec<crate::core::ModpackModEntry> = downloadable
        .iter()
        .filter(|f| {
            // Only include files routed to the mods directory (not resourcepacks/shaderpacks)
            !matches!(
                class_map.get(&f.mod_id).copied(),
                Some(curseforge::CLASS_RESOURCE_PACKS) | Some(curseforge::CLASS_SHADERS)
            )
        })
        .map(|f| crate::core::ModpackModEntry {
            name: f.file_name.clone(),
            download_url: f.download_url.clone(),
            display_name: Some(f.display_name.clone()),
            manual: false,
            disabled: false,
            slug: None,
            file_id: None,
            website_url: None,
        })
        .chain(skipped_mods.iter().map(|s| crate::core::ModpackModEntry {
            name: s.file_name.clone(),
            download_url: None,
            display_name: Some(s.display_name.clone()),
            manual: true,
            disabled: false,
            slug: Some(s.slug.clone()),
            file_id: Some(s.file_id),
            website_url: s.website_url.clone(),
        }))
        .collect();
    if !mod_entries.is_empty() {
        let manifest_path = minecraft_dir.join(".modpack_mods.json");
        if let Ok(json) = serde_json::to_string_pretty(&mod_entries) {
            let _ = std::fs::write(&manifest_path, json);
        }
    }

    // Step 4: Extract overrides (snapshot servers.dat first for merge)
    let overrides_prefix = if manifest.overrides.is_empty() {
        "overrides/".to_string()
    } else {
        let mut p = manifest.overrides.clone();
        if !p.ends_with('/') {
            p.push('/');
        }
        p
    };
    progress(0, 0, "Extracting overrides...");
    let servers_dat = minecraft_dir.join("servers.dat");
    let pre_existing_servers = crate::core::servers::snapshot_servers(&servers_dat);
    if !progress(0, 0, "Extracting overrides...") {
        anyhow::bail!("Cancelled");
    }
    extract_cf_overrides(zip_path, minecraft_dir, &overrides_prefix)?;
    let _ = crate::core::servers::merge_modpack_servers(&servers_dat, &pre_existing_servers);

    Ok(skipped_mods)
}

/// Alias for the shared override extraction function.
fn extract_cf_overrides(zip_path: &Path, dest: &Path, prefix: &str) -> anyhow::Result<()> {
    crate::core::extract_zip_overrides(zip_path, dest, prefix)
}

pub fn update_curseforge_modpack(
    project_id: &str,
    version_id: &str,
    minecraft_dir: &std::path::Path,
    progress: &Arc<Mutex<LaunchProgress>>,
    ctx: &egui::Context,
    skipped_slot: &Arc<Mutex<Vec<SkippedMod>>>,
) -> anyhow::Result<crate::core::update::UpdatedModpackMeta> {
    let mod_id: u64 = project_id
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid CurseForge mod_id"))?;
    let file_id: u64 = version_id
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid CurseForge file_id"))?;

    {
        let mut p = progress.lock_or_recover();
        p.message = "Fetching new modpack version...".to_string();
    }
    ctx.request_repaint();

    let files = curseforge::get_cf_mod_files(mod_id, "", None)?;
    let file = files
        .iter()
        .find(|f| f.id == file_id)
        .ok_or_else(|| anyhow::anyhow!("CurseForge file not found"))?;
    let temp_dir = std::env::temp_dir().join("lurch_cf_modpack_update");
    let zip_path = if let Some(url) = file.download_url.as_ref() {
        {
            let mut p = progress.lock_or_recover();
            p.message = "Downloading modpack update...".to_string();
        }
        ctx.request_repaint();

        std::fs::create_dir_all(&temp_dir)?;
        let path = temp_dir.join(&file.file_name);
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(600))
            .user_agent(super::USER_AGENT)
            .build()?;
        let resp = client.get(url).send()?;
        if !resp.status().is_success() {
            anyhow::bail!("Failed to download modpack: HTTP {}", resp.status());
        }
        let bytes = resp.bytes()?;
        std::fs::write(&path, &bytes)?;
        path
    } else {
        wait_for_cf_manual_download(mod_id, file, &temp_dir, progress, ctx)?
    };

    {
        let mut p = progress.lock_or_recover();
        p.message = "Parsing modpack...".to_string();
    }
    ctx.request_repaint();

    let manifest = parse_cf_modpack(&zip_path)?;

    let mc_ver = manifest.minecraft.version.clone();
    let (loader, loader_ver) = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|ml| ml.primary)
        .or(manifest.minecraft.mod_loaders.first())
        .map(|ml| parse_loader_id(&ml.id))
        .unwrap_or((ModLoader::Vanilla, None));

    // Snapshot old modpack mod list so we can remove stale mods after install.
    let old_mod_names: std::collections::HashSet<String> = {
        let manifest_path = minecraft_dir.join(".modpack_mods.json");
        if let Ok(data) = std::fs::read_to_string(&manifest_path) {
            // Try enriched format first, fall back to legacy Vec<String>
            if let Ok(entries) = serde_json::from_str::<Vec<crate::core::ModpackModEntry>>(&data) {
                entries.into_iter().map(|e| e.name).collect()
            } else if let Ok(names) = serde_json::from_str::<Vec<String>>(&data) {
                names.into_iter().collect()
            } else {
                std::collections::HashSet::new()
            }
        } else {
            std::collections::HashSet::new()
        }
    };

    let progress_for_files = Arc::clone(progress);
    let ctx_for_files = ctx.clone();
    let skipped_mods = install_cf_modpack_files(
        &manifest,
        &zip_path,
        minecraft_dir,
        move |done, total, stage| {
            let mut p = progress_for_files.lock_or_recover();
            if p.cancelled {
                return false;
            }
            p.message = if total > 0 {
                format!("{stage} ({done}/{total})")
            } else {
                stage.to_string()
            };
            drop(p);
            ctx_for_files.request_repaint();
            true
        },
    )?;

    // Remove stale mods that were in the old version but not in the new one.
    {
        let manifest_path = minecraft_dir.join(".modpack_mods.json");
        let new_mod_names: std::collections::HashSet<String> = if let Ok(data) =
            std::fs::read_to_string(&manifest_path)
        {
            if let Ok(entries) = serde_json::from_str::<Vec<crate::core::ModpackModEntry>>(&data) {
                entries.into_iter().map(|e| e.name).collect()
            } else {
                std::collections::HashSet::new()
            }
        } else {
            std::collections::HashSet::new()
        };
        let mods_dir = minecraft_dir.join("mods");
        for stale in old_mod_names.difference(&new_mod_names) {
            let path = mods_dir.join(stale);
            if path.exists() {
                log::info!("Removing stale mod: {}", stale);
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    *skipped_slot.lock_or_recover() = skipped_mods;

    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(crate::core::update::UpdatedModpackMeta {
        mc_version: mc_ver,
        loader,
        loader_version: loader_ver,
    })
}

/// When a CurseForge file blocks third-party distribution, open its page in
/// the user's browser and poll ~/Downloads until the file appears.
/// Returns the path to the downloaded file (moved into `dest_dir`).
pub fn wait_for_cf_manual_download(
    project_id: u64,
    file: &curseforge::CfFile,
    dest_dir: &std::path::Path,
    progress: &Arc<Mutex<LaunchProgress>>,
    ctx: &egui::Context,
) -> anyhow::Result<std::path::PathBuf> {
    let cf_url = format!("https://www.curseforge.com/projects/{}", project_id);
    let _ = open::that(&cf_url);

    let downloads_dir = directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
        .ok_or_else(|| anyhow::anyhow!("Could not find Downloads directory"))?;

    {
        let mut p = progress.lock_or_recover();
        p.message = format!(
            "Waiting for manual download of \"{}\"…\n\
             Download from CurseForge (opened in browser)",
            file.file_name
        );
    }
    ctx.request_repaint();

    let expected = downloads_dir.join(&file.file_name);
    loop {
        if progress.lock_or_recover().cancelled {
            anyhow::bail!("Manual download cancelled");
        }
        if expected.exists() {
            // Brief extra wait so the browser finishes writing.
            std::thread::sleep(Duration::from_secs(1));
            if expected.exists() {
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    std::fs::create_dir_all(dest_dir)?;
    let dest = dest_dir.join(&file.file_name);
    if std::fs::rename(&expected, &dest).is_err() {
        std::fs::copy(&expected, &dest)?;
        let _ = std::fs::remove_file(&expected);
    }

    // Cache the manually-downloaded file so future installs can resolve it locally.
    crate::core::mod_cache::cache_file(&file.file_name, &dest);

    Ok(dest)
}
