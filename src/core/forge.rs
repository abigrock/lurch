//! Forge & NeoForge installer support.
//!
//! Handles three responsibilities:
//! 1. Fetching available loader versions
//! 2. Downloading and extracting installer JARs (modern + legacy formats)
//! 3. Running post-install processors (modern format only)

use crate::core::instance::ModLoader;
use crate::core::version::{self, Arguments, Library};
use anyhow::Context;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Constants ────────────────────────────────────────────────────────────────

const FORGE_PROMOTIONS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const FORGE_MAVEN_BASE: &str = "https://maven.minecraftforge.net";

const NEOFORGE_MAVEN_VERSIONS: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const NEOFORGE_LEGACY_MAVEN_VERSIONS: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/forge";
const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases";

// ── Version listing types ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ForgePromotions {
    promos: HashMap<String, String>,
}

#[derive(Deserialize)]
struct NeoForgeVersionsList {
    versions: Vec<String>,
}

// ── Install profile types ────────────────────────────────────────────────────

/// Modern Forge/NeoForge install_profile.json (spec 0 or 1)
#[derive(Debug, Clone, Deserialize)]
pub struct ForgeInstallProfile {
    #[serde(default)]
    pub processors: Vec<ForgeProcessor>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub data: HashMap<String, ForgeDataEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgeProcessor {
    #[serde(default)]
    pub sides: Option<Vec<String>>,
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub outputs: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgeDataEntry {
    pub client: String,
    #[allow(dead_code)]
    pub server: String,
}

/// Legacy install_profile.json (pre-1.13 Forge)
#[derive(Deserialize)]
struct LegacyForgeInstallProfile {
    install: LegacyInstallInfo,
    #[serde(rename = "versionInfo")]
    version_info: serde_json::Value,
}

#[derive(Deserialize)]
struct LegacyInstallInfo {
    path: String,
    #[allow(dead_code)]
    minecraft: String,
}

/// Parsed version profile from inside the installer (version.json or legacy versionInfo).
/// Compatible with the vanilla VersionInfo merge pattern.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeVersionProfile {
    pub main_class: String,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    /// Legacy argument string (pre-1.13 Forge)
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

// ── Public API: Version listing ──────────────────────────────────────────────

/// Fetch available Forge versions for a MC version.
/// Returns `(build_number, is_stable)` pairs — e.g. `("47.2.0", true)`.
pub fn fetch_forge_versions(
    client: &reqwest::blocking::Client,
    mc_version: &str,
) -> anyhow::Result<Vec<(String, bool)>> {
    let resp = client.get(FORGE_PROMOTIONS_URL).send()?;
    if !resp.status().is_success() {
        anyhow::bail!("Failed to fetch Forge promotions: HTTP {}", resp.status());
    }
    let promos: ForgePromotions = resp.json()?;

    let recommended = promos.promos.get(&format!("{mc_version}-recommended"));
    let latest = promos.promos.get(&format!("{mc_version}-latest"));

    let mut versions = Vec::new();
    if let Some(rec) = recommended {
        versions.push((rec.clone(), true));
    }
    if let Some(lat) = latest {
        // Only add latest if it differs from recommended
        if recommended != Some(lat) {
            versions.push((lat.clone(), false));
        }
    }

    if versions.is_empty() {
        anyhow::bail!("No Forge versions found for Minecraft {mc_version}");
    }
    Ok(versions)
}

/// Fetch available NeoForge versions for a MC version.
/// Returns `(version, is_stable=true)` pairs sorted newest-first.
pub fn fetch_neoforge_versions(
    client: &reqwest::blocking::Client,
    mc_version: &str,
) -> anyhow::Result<Vec<(String, bool)>> {
    let (url, prefix) = if mc_version == "1.20.1" {
        // Legacy NeoForge for 1.20.1 uses net.neoforged:forge artifact
        (NEOFORGE_LEGACY_MAVEN_VERSIONS, format!("{mc_version}-"))
    } else {
        // Standard: MC 1.21.1 → prefix "21.1."
        let mc_minor = mc_version.strip_prefix("1.").unwrap_or(mc_version);
        (NEOFORGE_MAVEN_VERSIONS, format!("{mc_minor}."))
    };

    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!("Failed to fetch NeoForge versions: HTTP {}", resp.status());
    }
    let list: NeoForgeVersionsList = resp.json()?;

    let versions: Vec<(String, bool)> = list
        .versions
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .rev() // newest first
        .map(|v| (v, true))
        .collect();

    if versions.is_empty() {
        anyhow::bail!("No NeoForge versions found for Minecraft {mc_version}");
    }
    Ok(versions)
}

// ── Public API: Installer download + extraction ──────────────────────────────

/// Download the Forge/NeoForge installer JAR, extract the version profile
/// and embedded Maven libraries. Returns the profile for merging into VersionInfo.
pub fn download_and_extract_installer(
    client: &reqwest::blocking::Client,
    loader: &ModLoader,
    mc_version: &str,
    loader_version: &str,
) -> anyhow::Result<ForgeVersionProfile> {
    let cache_dir = forge_cache_dir(loader, mc_version, loader_version)?;
    let profile_path = cache_dir.join("version_profile.json");

    // Return cached profile if available
    if profile_path.exists() {
        let json = std::fs::read_to_string(&profile_path)?;
        return Ok(serde_json::from_str(&json)?);
    }

    // Download installer JAR (re-download if existing copy is corrupt)
    let installer_path = cache_dir.join("installer.jar");
    if !installer_path.exists() || !crate::core::is_jar_valid(&installer_path) {
        let url = installer_url(loader, mc_version, loader_version);
        log::info!("Downloading {loader} installer from {url}");
        let resp = client.get(&url).send()?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "Failed to download {loader} installer: HTTP {}",
                resp.status()
            );
        }
        let bytes = resp.bytes()?;
        crate::core::validate_jar(&bytes)
            .context("Downloaded Forge/NeoForge installer is not a valid JAR")?;
        std::fs::write(&installer_path, &bytes)?;
    }

    // Open installer as ZIP
    let file = std::fs::File::open(&installer_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let libs_dir = version::libraries_dir()?;

    // Detect format: modern has version.json at the root
    let is_modern = archive.file_names().any(|n| n == "version.json");

    let profile_json = if is_modern {
        // ── Modern format (1.13+) ────────────────────────────────────────
        let version_json = read_zip_entry_string(&mut archive, "version.json")?;
        let install_json = read_zip_entry_string(&mut archive, "install_profile.json")?;
        std::fs::write(cache_dir.join("install_profile.json"), &install_json)?;

        // Extract embedded Maven libraries
        extract_maven_libraries(&mut archive, &libs_dir)?;

        version_json
    } else {
        // ── Legacy format (1.5.2 – 1.12.2) ──────────────────────────────
        let install_json = read_zip_entry_string(&mut archive, "install_profile.json")?;
        let legacy: LegacyForgeInstallProfile = serde_json::from_str(&install_json)?;

        // Extract the forge library from the installer's maven/ dir
        if let Some(lib_path) = crate::core::maven_path(&legacy.install.path) {
            let jar_entry = format!("maven/{lib_path}");
            let dest = libs_dir.join(&lib_path);
            if (!dest.exists() || !crate::core::is_jar_valid(&dest))
                && let Ok(mut entry) = archive.by_name(&jar_entry) {
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut data = Vec::new();
                    std::io::Read::read_to_end(&mut entry, &mut data)?;
                    std::fs::write(&dest, &data)?;
                }
        }

        serde_json::to_string_pretty(&legacy.version_info)?
    };

    // Cache and parse
    std::fs::write(&profile_path, &profile_json)?;
    Ok(serde_json::from_str(&profile_json)?)
}

// ── Public API: Processor execution ──────────────────────────────────────────

/// Run Forge/NeoForge processors if needed (modern format only).
/// Must be called after the client JAR is downloaded and Java is selected.
pub fn run_processors_if_needed(
    client: &reqwest::blocking::Client,
    loader: &ModLoader,
    mc_version: &str,
    loader_version: &str,
    java_path: &Path,
    client_jar: &Path,
    progress: impl Fn(&str),
) -> anyhow::Result<()> {
    let cache_dir = forge_cache_dir(loader, mc_version, loader_version)?;
    let profile_path = cache_dir.join("install_profile.json");

    // No install profile → nothing to do (legacy format or already complete)
    if !profile_path.exists() {
        return Ok(());
    }

    // Check marker file — processors already ran successfully
    let marker = cache_dir.join("processors_done");
    if marker.exists() {
        return Ok(());
    }

    let profile_json = std::fs::read_to_string(&profile_path)?;
    let profile: ForgeInstallProfile = serde_json::from_str(&profile_json)?;

    if profile.processors.is_empty() {
        std::fs::write(&marker, "done")?;
        return Ok(());
    }

    let libs_dir = version::libraries_dir()?;
    let installer_path = cache_dir.join("installer.jar");

    // Download processor libraries (may partially overlap with maven/ extraction)
    progress("Downloading processor libraries...");
    download_processor_libraries(client, &profile, &libs_dir)?;

    // Build data map for variable substitution
    let data_map = build_data_map(&profile, &installer_path, &cache_dir, client_jar, &libs_dir)?;

    // Run each processor
    let total = profile.processors.len();
    for (i, proc) in profile.processors.iter().enumerate() {
        // Skip server-side processors
        if let Some(ref sides) = proc.sides
            && !sides.iter().any(|s| s == "client") {
                continue;
            }

        progress(&format!(
            "Running processor {}/{total}: {}",
            i + 1,
            proc.jar
        ));

        // Check if all outputs already exist with correct hashes
        if should_skip_processor(proc, &data_map, &libs_dir) {
            log::info!("Skipping processor {} (outputs up to date)", proc.jar);
            continue;
        }

        run_single_processor(proc, &data_map, &libs_dir, java_path)?;
    }

    std::fs::write(&marker, "done")?;
    log::info!("{loader} processors completed successfully");
    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn forge_cache_dir(
    loader: &ModLoader,
    mc_version: &str,
    loader_version: &str,
) -> anyhow::Result<PathBuf> {
    let loader_name = match loader {
        ModLoader::Forge => "forge",
        ModLoader::NeoForge => "neoforge",
        _ => anyhow::bail!("Not a Forge-family loader: {loader}"),
    };
    let dir = crate::util::paths::data_dir()?
        .join("forge_cache")
        .join(loader_name)
        .join(format!("{mc_version}-{loader_version}"));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn installer_url(loader: &ModLoader, mc_version: &str, loader_version: &str) -> String {
    match loader {
        ModLoader::NeoForge => {
            if mc_version == "1.20.1" {
                // Legacy NeoForge for 1.20.1 uses the forge artifact name
                format!(
                    "{NEOFORGE_MAVEN_BASE}/net/neoforged/forge/\
                     {loader_version}/forge-{loader_version}-installer.jar"
                )
            } else {
                format!(
                    "{NEOFORGE_MAVEN_BASE}/net/neoforged/neoforge/\
                     {loader_version}/neoforge-{loader_version}-installer.jar"
                )
            }
        }
        _ => {
            // Forge: full version is mc_version-loader_version
            let full = format!("{mc_version}-{loader_version}");
            format!(
                "{FORGE_MAVEN_BASE}/net/minecraftforge/forge/\
                 {full}/forge-{full}-installer.jar"
            )
        }
    }
}

fn read_zip_entry_string(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> anyhow::Result<String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| anyhow::anyhow!("Entry '{name}' not found in installer: {e}"))?;
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut entry, &mut buf)?;
    Ok(buf)
}

fn extract_maven_libraries(
    archive: &mut zip::ZipArchive<std::fs::File>,
    libs_dir: &Path,
) -> anyhow::Result<()> {
    let maven_entries: Vec<String> = archive
        .file_names()
        .filter(|n| n.starts_with("maven/") && !n.ends_with('/'))
        .map(String::from)
        .collect();

    for entry_name in &maven_entries {
        let relative = entry_name.strip_prefix("maven/").unwrap();
        let dest = libs_dir.join(relative);
        if dest.exists()
            && !(dest.extension().is_some_and(|e| e == "jar")
                && !crate::core::is_jar_valid(&dest))
        {
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut entry = archive.by_name(entry_name)?;
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut data)?;
        std::fs::write(&dest, &data)?;
    }

    Ok(())
}

fn read_jar_main_class(jar_path: &Path) -> anyhow::Result<String> {
    let file = std::fs::File::open(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut manifest = archive.by_name("META-INF/MANIFEST.MF")?;
    let mut content = String::new();
    std::io::Read::read_to_string(&mut manifest, &mut content)?;

    for line in content.lines() {
        if let Some(cls) = line.strip_prefix("Main-Class: ") {
            return Ok(cls.trim().to_string());
        }
    }
    anyhow::bail!("No Main-Class in MANIFEST.MF of {}", jar_path.display())
}

fn download_processor_libraries(
    client: &reqwest::blocking::Client,
    profile: &ForgeInstallProfile,
    libs_dir: &Path,
) -> anyhow::Result<()> {
    for lib in &profile.libraries {
        if let Some(ref downloads) = lib.downloads {
            if let Some(ref artifact) = downloads.artifact
                && !artifact.url.is_empty() {
                    let dest = libs_dir.join(&artifact.path);
                    version::download_file(client, &artifact.url, &dest, &artifact.sha1)?;
                }
        } else if let Some(ref base_url) = lib.url
            && let Some(path) = crate::core::maven_path(&lib.name) {
                let url = format!("{base_url}{path}");
                let dest = libs_dir.join(&path);
                let needs_download = !dest.exists()
                    || (dest.extension().is_some_and(|e| e == "jar")
                        && !crate::core::is_jar_valid(&dest));
                if needs_download {
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let resp = client.get(&url).send()?;
                    let bytes = resp.bytes()?;
                    if dest.extension().is_some_and(|e| e == "jar") {
                        crate::core::validate_jar(&bytes)
                            .with_context(|| format!("Bad download from {url}"))?;
                    }
                    std::fs::write(&dest, &bytes)?;
                }
            }
    }
    Ok(())
}

fn build_data_map(
    profile: &ForgeInstallProfile,
    installer_path: &Path,
    cache_dir: &Path,
    client_jar: &Path,
    libs_dir: &Path,
) -> anyhow::Result<HashMap<String, String>> {
    let mut map = HashMap::new();

    // Standard variables
    map.insert("SIDE".to_string(), "client".to_string());
    map.insert(
        "MINECRAFT_JAR".to_string(),
        client_jar.display().to_string(),
    );
    map.insert(
        "ROOT".to_string(),
        crate::util::paths::data_dir()?.display().to_string(),
    );
    map.insert(
        "INSTALLER".to_string(),
        installer_path.display().to_string(),
    );
    map.insert("LIBRARY_DIR".to_string(), libs_dir.display().to_string());

    // Profile data entries (client side)
    for (key, entry) in &profile.data {
        let resolved = resolve_data_value(&entry.client, installer_path, cache_dir, libs_dir)?;
        map.insert(key.clone(), resolved);
    }

    Ok(map)
}

fn resolve_data_value(
    value: &str,
    installer_path: &Path,
    cache_dir: &Path,
    libs_dir: &Path,
) -> anyhow::Result<String> {
    if value.starts_with('[') && value.ends_with(']') {
        // Maven coordinate → library file path
        let coord = &value[1..value.len() - 1];
        let path = crate::core::maven_path(coord)
            .ok_or_else(|| anyhow::anyhow!("Invalid maven coordinate in data: {coord}"))?;
        Ok(libs_dir.join(path).display().to_string())
    } else if let Some(entry_name) = value.strip_prefix('/') {
        // Path inside installer JAR → extract to cache
        // strip leading /
        let extract_path = cache_dir.join("extracted").join(entry_name);
        if !extract_path.exists()
            || (extract_path.extension().is_some_and(|e| e == "jar")
                && !crate::core::is_jar_valid(&extract_path))
        {
            let file = std::fs::File::open(installer_path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            let mut entry = archive
                .by_name(entry_name)
                .map_err(|e| anyhow::anyhow!("Entry '{entry_name}' not found in installer: {e}"))?;
            if let Some(parent) = extract_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut data)?;
            std::fs::write(&extract_path, &data)?;
        }
        Ok(extract_path.display().to_string())
    } else if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        // Literal string — strip quotes
        Ok(value[1..value.len() - 1].to_string())
    } else {
        Ok(value.to_string())
    }
}

fn substitute_arg(arg: &str, data_map: &HashMap<String, String>, libs_dir: &Path) -> String {
    // [maven:coord] → library file path
    if arg.starts_with('[') && arg.ends_with(']') {
        let coord = &arg[1..arg.len() - 1];
        if let Some(path) = crate::core::maven_path(coord) {
            return libs_dir.join(path).display().to_string();
        }
        return arg.to_string();
    }

    // Replace all {KEY} references from data map
    let mut result = arg.to_string();
    for (key, value) in data_map {
        let pattern = format!("{{{key}}}");
        result = result.replace(&pattern, value);
    }
    result
}

fn should_skip_processor(
    processor: &ForgeProcessor,
    data_map: &HashMap<String, String>,
    libs_dir: &Path,
) -> bool {
    let Some(ref outputs) = processor.outputs else {
        return false;
    };
    if outputs.is_empty() {
        return false;
    }

    outputs.iter().all(|(path_template, expected_sha1)| {
        let path_str = substitute_arg(path_template, data_map, libs_dir);
        let path = Path::new(&path_str);
        if !path.exists() {
            return false;
        }
        let expected = substitute_arg(expected_sha1, data_map, libs_dir);
        if expected.is_empty() {
            return true;
        }
        match std::fs::read(path) {
            Ok(data) => crate::core::sha1_hex(&data) == expected,
            Err(_) => false,
        }
    })
}

fn run_single_processor(
    processor: &ForgeProcessor,
    data_map: &HashMap<String, String>,
    libs_dir: &Path,
    java_path: &Path,
) -> anyhow::Result<()> {
    let proc_path = crate::core::maven_path(&processor.jar)
        .ok_or_else(|| anyhow::anyhow!("Invalid processor coordinate: {}", processor.jar))?;
    let proc_jar = libs_dir.join(&proc_path);

    let main_class = read_jar_main_class(&proc_jar)?;

    // Build classpath: processor JAR + classpath entries
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut cp_parts = vec![proc_jar.display().to_string()];
    for cp_entry in &processor.classpath {
        if let Some(path) = crate::core::maven_path(cp_entry) {
            cp_parts.push(libs_dir.join(path).display().to_string());
        }
    }
    let classpath = cp_parts.join(sep);

    // Build args with variable substitution
    let args: Vec<String> = processor
        .args
        .iter()
        .map(|a| substitute_arg(a, data_map, libs_dir))
        .collect();

    log::info!("Running processor: {}", processor.jar);
    log::debug!("  Main-Class: {main_class}");
    log::debug!("  Args: {args:?}");

    let output = std::process::Command::new(java_path)
        .arg("-cp")
        .arg(&classpath)
        .arg(&main_class)
        .args(&args)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "Processor {} failed (exit {}):\nstdout: {stdout}\nstderr: {stderr}",
            processor.jar,
            output.status
        );
    }

    Ok(())
}
