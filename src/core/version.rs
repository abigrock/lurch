use anyhow::Context;
use serde::Deserialize;
use std::path::PathBuf;

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
#[allow(dead_code)]
const LIBRARIES_BASE: &str = "https://libraries.minecraft.net/";
const RESOURCES_BASE: &str = "https://resources.download.minecraft.net/";

// ── Version manifest (top-level) ─────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

impl std::fmt::Display for VersionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Release => "Release",
            Self::Snapshot => "Snapshot",
            Self::OldBeta => "Old Beta",
            Self::OldAlpha => "Old Alpha",
        })
    }
}

// ── Per-version JSON ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub main_class: String,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    /// Legacy argument string (pre-1.13)
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    pub libraries: Vec<Library>,
    pub downloads: Downloads,
    pub asset_index: AssetIndexRef,
    #[serde(default)]
    pub java_version: Option<JavaVersionReq>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgumentValue>,
    #[serde(default)]
    pub jvm: Vec<ArgumentValue>,
}

/// An argument is either a plain string or a conditional object with rules
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: StringOrVec,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
}

impl StringOrVec {
    pub fn as_vec(&self) -> Vec<String> {
        match self {
            Self::Single(s) => vec![s.clone()],
            Self::Multiple(v) => v.clone(),
        }
    }
}

// ── Libraries ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

// ── Downloads / Assets ───────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: DownloadEntry,
    #[serde(default)]
    pub server: Option<DownloadEntry>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadEntry {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub total_size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: std::collections::HashMap<String, AssetObject>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

// ── Rules ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<std::collections::HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JavaVersionReq {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
    #[serde(default)]
    pub component: Option<String>,
}

// ── Rule evaluation ──────────────────────────────────────────────────────────

pub fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

pub fn current_arch_name() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86"
    }
}

/// Evaluate whether a set of rules allows this library/argument on the current platform
pub fn rules_match(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true; // no rules = always include
    }
    let mut allowed = false;
    for rule in rules {
        let os_ok = match &rule.os {
            None => true,
            Some(os_rule) => {
                let name_ok = os_rule
                    .name
                    .as_deref()
                    .is_none_or(|n| n == current_os_name());
                let arch_ok = os_rule
                    .arch
                    .as_deref()
                    .is_none_or(|a| a == current_arch_name());
                name_ok && arch_ok
            }
        };
        // Rules with feature requirements only apply when those features are active.
        // We don't support any features (quick play, demo, etc.) so any rule
        // requiring a feature to be true won't match.
        let features_ok = match &rule.features {
            None => true,
            Some(features) => features.values().all(|&v| !v),
        };
        if os_ok && features_ok {
            match rule.action {
                RuleAction::Allow => allowed = true,
                RuleAction::Disallow => allowed = false,
            }
        }
    }
    allowed
}

// ── Network fetching ─────────────────────────────────────────────────────────

/// Fetch the version manifest (blocking)
pub fn fetch_manifest() -> anyhow::Result<VersionManifest> {
    let resp = reqwest::blocking::get(VERSION_MANIFEST_URL)?;
    let manifest: VersionManifest = resp.json()?;
    Ok(manifest)
}

/// Fetch a specific version's full info JSON (blocking)
pub fn fetch_version_info(
    client: &reqwest::blocking::Client,
    version_url: &str,
) -> anyhow::Result<VersionInfo> {
    let resp = client.get(version_url).send()?;
    let info: VersionInfo = resp.json()?;
    Ok(info)
}

/// Fetch the asset index (blocking)
#[allow(dead_code)]
pub fn fetch_asset_index(
    client: &reqwest::blocking::Client,
    url: &str,
) -> anyhow::Result<AssetIndex> {
    let resp = client.get(url).send()?;
    let index: AssetIndex = resp.json()?;
    Ok(index)
}

// ── File download helpers ────────────────────────────────────────────────────

/// Download a file to a path, creating parent dirs. Skips if SHA1 matches.
pub fn download_file(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &std::path::Path,
    expected_sha1: &str,
) -> anyhow::Result<()> {
    // Skip if already exists with correct hash
    if dest.exists()
        && let Ok(existing_sha1) = sha1_file(dest)
        && existing_sha1 == expected_sha1
    {
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let resp = client.get(url).send()?;
    let bytes = resp.bytes()?;
    std::fs::write(dest, &bytes)?;

    // Verify hash
    let actual_sha1 = crate::core::sha1_hex(&bytes);
    if actual_sha1 != expected_sha1 {
        anyhow::bail!(
            "SHA1 mismatch for {}: expected {expected_sha1}, got {actual_sha1}",
            dest.display()
        );
    }

    Ok(())
}

fn sha1_file(path: &std::path::Path) -> anyhow::Result<String> {
    let data = std::fs::read(path)?;
    Ok(crate::core::sha1_hex(&data))
}

// ── Install helpers ──────────────────────────────────────────────────────────

/// Base directory for storing versions
pub fn versions_dir() -> anyhow::Result<PathBuf> {
    let dir = crate::util::paths::data_dir()?.join("versions");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Base directory for storing libraries
pub fn libraries_dir() -> anyhow::Result<PathBuf> {
    let dir = crate::util::paths::data_dir()?.join("libraries");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Base directory for storing assets
pub fn assets_dir() -> anyhow::Result<PathBuf> {
    let dir = crate::util::paths::data_dir()?.join("assets");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Download the client JAR for a version
pub fn download_client_jar(
    client: &reqwest::blocking::Client,
    info: &VersionInfo,
) -> anyhow::Result<PathBuf> {
    let dir = versions_dir()?.join(&info.id);
    let jar_path = dir.join(format!("{}.jar", info.id));
    download_file(
        client,
        &info.downloads.client.url,
        &jar_path,
        &info.downloads.client.sha1,
    )?;
    Ok(jar_path)
}

/// Download all libraries for a version, returns list of local paths
pub fn download_libraries(
    client: &reqwest::blocking::Client,
    info: &VersionInfo,
) -> anyhow::Result<Vec<PathBuf>> {
    let libs_dir = libraries_dir()?;
    let mut paths = Vec::new();

    for lib in &info.libraries {
        if !rules_match(&lib.rules) {
            continue;
        }

        if let Some(ref downloads) = lib.downloads {
            if let Some(ref artifact) = downloads.artifact {
                let dest = libs_dir.join(&artifact.path);
                if artifact.url.is_empty() {
                    // Forge processor output — skip download, add path if file exists
                    if dest.exists() {
                        paths.push(dest);
                    }
                } else {
                    download_file(client, &artifact.url, &dest, &artifact.sha1)?;
                    paths.push(dest);
                }
            }
        } else if let Some(ref base_url) = lib.url {
            // Maven-style: construct path from name
            if let Some(path) = crate::core::maven_path(&lib.name) {
                let url = format!("{base_url}{path}");
                let dest = libs_dir.join(&path);
                // No SHA1 available — re-download if missing or corrupt
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
                paths.push(dest);
            }
        }
    }

    // Deduplicate: Forge/NeoForge profiles can overlap with vanilla libraries
    let mut seen = std::collections::HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));

    Ok(paths)
}

/// Download assets for a version
pub fn download_assets(
    info: &VersionInfo,
    client: &reqwest::blocking::Client,
    progress: impl Fn(usize, usize) + Send + Sync,
) -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let assets_base = assets_dir()?;

    // Download asset index
    let index_dir = assets_base.join("indexes");
    std::fs::create_dir_all(&index_dir)?;
    let index_path = index_dir.join(format!("{}.json", info.asset_index.id));
    download_file(
        client,
        &info.asset_index.url,
        &index_path,
        &info.asset_index.sha1,
    )?;

    // Parse and download objects
    let index: AssetIndex = serde_json::from_str(&std::fs::read_to_string(&index_path)?)?;
    let objects_dir = assets_base.join("objects");

    let objects: Vec<_> = index.objects.values().collect();
    let total = objects.len();
    let completed = AtomicUsize::new(0);

    progress(0, total);

    let num_threads = 8.min(total);
    if num_threads == 0 {
        return Ok(());
    }
    let chunk_size = total.div_ceil(num_threads);

    std::thread::scope(|s| {
        let errors: Vec<_> = objects
            .chunks(chunk_size)
            .map(|chunk| {
                let objects_dir = &objects_dir;
                let completed = &completed;
                let progress = &progress;
                s.spawn(move || -> anyhow::Result<()> {
                    for obj in chunk {
                        let prefix = &obj.hash[..2];
                        let dest = objects_dir.join(prefix).join(&obj.hash);
                        let url = format!("{RESOURCES_BASE}{prefix}/{}", obj.hash);
                        download_file(client, &url, &dest, &obj.hash)?;
                        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                        progress(done, total);
                    }
                    Ok(())
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .filter_map(|r| r.err())
            .collect();

        if let Some(e) = errors.into_iter().next() {
            Err(e)
        } else {
            Ok(())
        }
    })?;

    Ok(())
}

pub enum ManifestState {
    Loading,
    Loaded(crate::core::version::VersionManifest),
    Failed(String),
}
