use serde::{Deserialize, Serialize};
use super::{CommandHideConsole, MutexExt};
use std::path::PathBuf;
use std::process::Command;

/// A detected or managed Java installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaInstall {
    pub path: PathBuf,
    pub version: String,
    pub major: u32,
    pub arch: String,
    pub vendor: String,         // e.g. "Temurin", "Microsoft", "Oracle", "GraalVM"
    pub managed: bool,          // true if downloaded by Lurch
}

impl std::fmt::Display for JavaInstall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Java {} ({})", self.version, self.path.display())
    }
}

/// Probe a java binary and extract version info
pub fn probe_java(java_bin: &std::path::Path) -> Option<JavaInstall> {
    let output = Command::new(java_bin).arg("-version").no_console_window().output().ok()?;

    // java -version outputs to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_java_version(&stderr, java_bin)
}

fn parse_java_version(version_output: &str, java_bin: &std::path::Path) -> Option<JavaInstall> {
    // First line: openjdk version "17.0.9" or java version "1.8.0_392"
    let first_line = version_output.lines().next()?;
    let version_str = first_line.split('"').nth(1)?.to_string();

    let major = parse_major_version(&version_str)?;

    // Try to detect architecture from output
    let arch = if version_output.contains("64-Bit") {
        "x64".to_string()
    } else if version_output.contains("aarch64") || version_output.contains("ARM 64") {
        "aarch64".to_string()
    } else {
        "x64".to_string() // default assumption
    };

    let vendor = detect_vendor(version_output);

    // Resolve to the java home directory (go up from bin/java)
    let path = java_bin
        .canonicalize()
        .unwrap_or_else(|_| java_bin.to_path_buf());

    Some(JavaInstall {
        path,
        version: version_str,
        major,
        arch,
        vendor,
        managed: false,
    })
}

fn parse_major_version(version: &str) -> Option<u32> {
    // Handle "1.8.0_xxx" (Java 8) or "17.0.9" (Java 9+)
    let parts: Vec<&str> = version.split('.').collect();
    match parts.first()?.parse::<u32>() {
        Ok(1) => parts.get(1)?.parse().ok(), // 1.8 -> 8
        Ok(n) => Some(n),                    // 17 -> 17
        Err(_) => None,
    }
}

/// Detect vendor/distribution from `java -version` output.
/// Parses the runtime name from the second line generically, e.g.:
///   "OpenJDK Runtime Environment Temurin-17.0.15+6 (build ...)" → "Temurin"
///   "OpenJDK Runtime Environment GraalVM CE 17.0.9+9.1 (build ...)" → "GraalVM CE"
///   "Java(TM) SE Runtime Environment (build ...)" → "Oracle"
fn detect_vendor(version_output: &str) -> String {
    let second_line = match version_output.lines().nth(1) {
        Some(line) => line.trim(),
        None => return "Unknown".to_string(),
    };

    // Oracle proprietary: "Java(TM) SE Runtime Environment"
    if second_line.contains("Java(TM)") {
        return "Oracle".to_string();
    }

    // OpenJDK-based: "OpenJDK Runtime Environment <Vendor><version> (build ...)"
    if let Some(rest) = second_line.strip_prefix("OpenJDK Runtime Environment") {
        let rest = rest.trim();
        if rest.starts_with("(build") || rest.is_empty() {
            return "OpenJDK".to_string();
        }
        // Take everything before "(build" and extract the name portion
        let before_build = rest.split("(build").next().unwrap_or(rest).trim();
        let vendor = strip_version_suffix(before_build);
        if vendor.is_empty() {
            return "OpenJDK".to_string();
        }
        return vendor;
    }

    "Unknown".to_string()
}

/// Strip trailing version/number suffixes from a vendor string.
/// "Temurin-17.0.15+6" → "Temurin"
/// "GraalVM CE 17.0.9+9.1" → "GraalVM CE"
/// "Zulu17.46+19-CA" → "Zulu"
/// "Microsoft-9889599" → "Microsoft"
fn strip_version_suffix(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_digit() {
            if i == 0 {
                return String::new();
            }
            let prev = chars[i - 1];
            if prev == '-' || prev == ' ' {
                // "Temurin-17..." or "GraalVM CE 17..."
                return chars[..i - 1].iter().collect::<String>().trim().to_string();
            }
            if prev.is_ascii_alphabetic() {
                // "Zulu17..." — digit directly after letters
                return chars[..i].iter().collect::<String>().trim().to_string();
            }
        }
    }
    s.trim().to_string()
}

/// Detect all Java installations on the system
pub fn detect_java_installations() -> Vec<JavaInstall> {
    let mut installs = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Check JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let bin = PathBuf::from(&java_home)
            .join("bin")
            .join(java_binary_name());
        if bin.exists()
            && let Some(inst) = probe_java(&bin) {
                seen_paths.insert(inst.path.clone());
                installs.push(inst);
            }
    }

    // 2. Search PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let bin = dir.join(java_binary_name());
            if bin.exists()
                && let Some(inst) = probe_java(&bin)
                    && seen_paths.insert(inst.path.clone()) {
                        installs.push(inst);
                    }
        }
    }

    // 3. Platform-specific common directories
    for dir in platform_java_dirs() {
        scan_java_dir(&dir, &mut installs, &mut seen_paths);
    }

    // 4. Lurch-managed Java directory
    if let Ok(managed_dir) = java_managed_dir()
        && managed_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&managed_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let bin = path.join("bin").join(java_binary_name());
                    if bin.exists()
                        && let Some(mut inst) = probe_java(&bin)
                            && seen_paths.insert(inst.path.clone()) {
                                inst.managed = true;
                                // Override vendor to match the download source
                                let dir_name = path.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                if dir_name.starts_with("mojang-") {
                                    inst.vendor = "Mojang".to_string();
                                } else if dir_name.starts_with("java-") {
                                    inst.vendor = "Adoptium".to_string();
                                }
                                installs.push(inst);
                            }
                }
            }

    // Sort by major version descending
    installs.sort_by(|a, b| b.major.cmp(&a.major));
    installs
}

/// Directory where Lurch stores downloaded Java installations
pub fn java_managed_dir() -> anyhow::Result<PathBuf> {
    let dir = crate::util::paths::data_dir()?.join("java");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn java_binary_name() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

fn platform_java_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/lib/jvm"));
        dirs.push(PathBuf::from("/usr/local/lib/jvm"));
        dirs.push(PathBuf::from("/usr/java"));
    }

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Java/JavaVirtualMachines"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            dirs.push(PathBuf::from(&pf).join("Java"));
            dirs.push(PathBuf::from(&pf).join("Eclipse Adoptium"));
            dirs.push(PathBuf::from(&pf).join("Eclipse Foundation"));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            dirs.push(PathBuf::from(&pf86).join("Java"));
        }
    }

    dirs
}

fn scan_java_dir(
    dir: &std::path::Path,
    installs: &mut Vec<JavaInstall>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Try bin/java directly
        let bin = path.join("bin").join(java_binary_name());
        if bin.exists() {
            if let Some(inst) = probe_java(&bin)
                && seen.insert(inst.path.clone()) {
                    installs.push(inst);
                }
            continue;
        }
        // macOS style: Contents/Home/bin/java
        let mac_bin = path.join("Contents/Home/bin").join(java_binary_name());
        if mac_bin.exists()
            && let Some(inst) = probe_java(&mac_bin)
                && seen.insert(inst.path.clone()) {
                    installs.push(inst);
                }
    }
}

/// Fetch available Java versions from the Adoptium API.
pub fn fetch_available_versions() -> anyhow::Result<Vec<u32>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(crate::core::USER_AGENT)
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp: serde_json::Value = client
        .get("https://api.adoptium.net/v3/info/available_releases")
        .send()?
        .json()?;
    let versions = resp["available_releases"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid Adoptium API response"))?
        .iter()
        .filter_map(|v| v.as_u64().map(|n| n as u32))
        .collect();
    Ok(versions)
}

// ── Adoptium download ────────────────────────────────────────────────────────

/// Returns the current platform's OS string for the Adoptium API
pub fn adoptium_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

/// Returns the current platform's arch string for the Adoptium API
pub fn adoptium_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    }
}

/// Build the Adoptium download URL for a given major Java version
pub fn adoptium_download_url(major_version: u32) -> String {
    format!(
        "https://api.adoptium.net/v3/binary/latest/{}/ga/{}/{}/jre/hotspot/normal/eclipse",
        major_version,
        adoptium_os(),
        adoptium_arch()
    )
}

/// Recommended Java major version for a given Minecraft version
pub fn recommended_java_version(mc_version: &str) -> u32 {
    // MC 1.21+ needs Java 21, 1.17-1.20.x needs Java 17, older needs Java 8
    let parts: Vec<&str> = mc_version.split('.').collect();
    let minor = parts
        .get(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    match parts.first().and_then(|s| s.parse::<u32>().ok()) {
        Some(1) if minor >= 21 => 21,
        Some(1) if minor >= 17 => 17,
        Some(1) => 8,
        _ => 21, // default to latest
    }
}

/// Find the best matching installed Java for a Minecraft version
#[allow(dead_code)]
pub fn find_best_java(mc_version: &str, installs: &[JavaInstall]) -> Option<usize> {
    let recommended = recommended_java_version(mc_version);
    // Exact major match first
    if let Some(idx) = installs.iter().position(|i| i.major == recommended) {
        return Some(idx);
    }
    // Fall back to any Java >= recommended
    installs.iter().position(|i| i.major >= recommended)
}

/// Download and install a Java JRE from Adoptium.
/// `progress_cb` is called with status messages during the process.
/// Returns the newly installed `JavaInstall` with `managed = true`.
pub fn download_java(
    client: &reqwest::blocking::Client,
    major_version: u32,
    progress_cb: impl Fn(&str),
) -> anyhow::Result<JavaInstall> {
    let managed_dir = java_managed_dir()?;
    let target_dir = managed_dir.join(format!("java-{}", major_version));

    // If already downloaded, just probe and return
    let bin = target_dir.join("bin").join(java_binary_name());
    if bin.exists()
        && let Some(mut inst) = probe_java(&bin) {
            inst.managed = true;
            inst.vendor = "Adoptium".to_string();
            return Ok(inst);
        }

    let url = adoptium_download_url(major_version);
    progress_cb(&format!("Downloading Java {}...", major_version));

    let resp = client
        .get(&url)
        .send()?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to download Java {}: HTTP {}",
            major_version,
            resp.status()
        );
    }

    let bytes = resp.bytes()?;
    progress_cb(&format!("Extracting Java {}...", major_version));

    // Clean up previous install / partial extract
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)?;
    }
    let temp_extract = managed_dir.join(format!(".extracting-java-{}", major_version));
    if temp_extract.exists() {
        std::fs::remove_dir_all(&temp_extract)?;
    }
    std::fs::create_dir_all(&temp_extract)?;

    // Extract the archive
    extract_java_archive(&bytes, &temp_extract)?;

    // Adoptium archives have a single top-level dir (e.g. "jdk-21.0.10+7-jre")
    // Move that inner dir to the target path
    let inner = find_single_subdir(&temp_extract)?;
    std::fs::rename(&inner, &target_dir)?;
    let _ = std::fs::remove_dir_all(&temp_extract);

    // Probe the newly installed Java
    let bin = target_dir.join("bin").join(java_binary_name());
    let mut inst = probe_java(&bin).ok_or_else(|| {
        anyhow::anyhow!(
            "Downloaded Java {} but could not probe the binary",
            major_version
        )
    })?;
    inst.managed = true;
    inst.vendor = "Adoptium".to_string();

    progress_cb(&format!(
        "Java {} installed ({})",
        major_version, inst.version
    ));

    Ok(inst)
}

/// Find the single subdirectory inside a directory (for archive extraction).
fn find_single_subdir(dir: &std::path::Path) -> anyhow::Result<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();

    if dirs.len() == 1 {
        Ok(dirs.remove(0))
    } else {
        // Fallback: treat the dir itself as Java home
        Ok(dir.to_path_buf())
    }
}

/// Extract a Java archive (tar.gz on Linux/macOS, zip on Windows).
fn extract_java_archive(data: &[u8], dest: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use flate2::read::GzDecoder;
        use tar::Archive;

        let gz = GzDecoder::new(std::io::Cursor::new(data));
        let mut archive = Archive::new(gz);
        archive.unpack(dest)?;
    }

    #[cfg(target_os = "windows")]
    {
        let cursor = std::io::Cursor::new(data);
        let mut archive = zip::ZipArchive::new(cursor)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(name) = entry.enclosed_name() else {
                continue;
            };
            let path = dest.join(name);

            if entry.is_dir() {
                std::fs::create_dir_all(&path)?;
            } else {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut file = std::fs::File::create(&path)?;
                std::io::copy(&mut entry, &mut file)?;
            }
        }
    }

    Ok(())
}

// ─── Mojang Java Runtime support ─────────────────────────────────────────────

const MOJANG_JAVA_MANIFEST_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// Top-level response from Mojang Java runtime manifest (per-component file manifest)
#[derive(Debug, Deserialize)]
pub struct MojangFileManifest {
    pub files: std::collections::HashMap<String, MojangFileEntry>,
}

/// A single entry in the Mojang file manifest
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MojangFileEntry {
    Directory,
    File {
        downloads: MojangFileDownloads,
        #[serde(default)]
        executable: bool,
    },
    Link {
        target: String,
    },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MojangFileDownloads {
    pub raw: MojangDownloadInfo,
    #[serde(default)]
    pub lzma: Option<MojangDownloadInfo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MojangDownloadInfo {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// Returns the Mojang platform key for the current OS + arch
pub fn mojang_platform_key() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            "windows-x64"
        } else if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x86"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else {
        // Linux
        if cfg!(target_arch = "x86") {
            "linux-i386"
        } else {
            "linux"
        }
    }
}

/// Fetch the Mojang file manifest for a specific Java runtime component.
/// Returns (file_manifest, version_name_string).
pub fn fetch_mojang_component_manifest(
    client: &reqwest::blocking::Client,
    component: &str,
) -> anyhow::Result<(MojangFileManifest, String)> {
    let platform = mojang_platform_key();

    // Fetch top-level manifest
    let resp: serde_json::Value = client
        .get(MOJANG_JAVA_MANIFEST_URL)
        .send()?
        .json()?;

    // Navigate: resp[platform][component][0]
    let entry = resp
        .get(platform)
        .and_then(|p| p.get(component))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Mojang Java runtime component '{}' not found for platform '{}'",
                component,
                platform
            )
        })?;

    let manifest_url = entry
        .get("manifest")
        .and_then(|m| m.get("url"))
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing manifest URL for component '{}'", component))?;

    let version_name = entry
        .get("version")
        .and_then(|v| v.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Fetch the per-component file manifest
    let manifest: MojangFileManifest = client
        .get(manifest_url)
        .send()?
        .json()?;

    Ok((manifest, version_name))
}

/// Download and install a Java runtime from Mojang's official distribution.
/// `component` is a Mojang runtime component name like "java-runtime-delta" or "jre-legacy".
/// Returns the newly installed `JavaInstall` with `managed = true`.
pub fn download_mojang_java(
    client: &reqwest::blocking::Client,
    component: &str,
    progress_cb: impl Fn(&str) + Send + Sync,
) -> anyhow::Result<JavaInstall> {
    let managed_dir = java_managed_dir()?;
    let target_dir = managed_dir.join(format!("mojang-{}", component));

    // If already downloaded, just probe and return
    let bin = target_dir.join("bin").join(java_binary_name());
    if bin.exists()
        && let Some(mut inst) = probe_java(&bin) {
            inst.managed = true;
            return Ok(inst);
        }

    progress_cb(&format!(
        "Fetching Mojang Java runtime manifest ({})...",
        component
    ));

    let (manifest, version_name) = fetch_mojang_component_manifest(client, component)?;

    progress_cb(&format!(
        "Downloading Mojang Java {} ({})...",
        version_name, component
    ));

    // Clean previous partial install
    if target_dir.exists() {
        std::fs::remove_dir_all(&target_dir)?;
    }
    std::fs::create_dir_all(&target_dir)?;

    // Separate files into categories
    let mut directories = Vec::new();
    let mut files_to_download: Vec<(String, &MojangDownloadInfo, bool)> = Vec::new();
    let mut links: Vec<(String, String)> = Vec::new();

    for (path, entry) in &manifest.files {
        match entry {
            MojangFileEntry::Directory => {
                directories.push(path.clone());
            }
            MojangFileEntry::File {
                downloads,
                executable,
            } => {
                files_to_download.push((path.clone(), &downloads.raw, *executable));
            }
            MojangFileEntry::Link { target } => {
                links.push((path.clone(), target.clone()));
            }
        }
    }

    // Create directories
    for dir in &directories {
        std::fs::create_dir_all(target_dir.join(dir))?;
    }

    // Download files in parallel
    let total = files_to_download.len();
    let completed = std::sync::atomic::AtomicUsize::new(0);

    progress_cb(&format!(
        "Downloading {} files for Java {}...",
        total, version_name
    ));

    let num_threads = 8.min(total);
    if num_threads > 0 {
        let chunk_size = total.div_ceil(num_threads);

        let thread_error: std::sync::Mutex<Option<anyhow::Error>> = std::sync::Mutex::new(None);

        std::thread::scope(|s| {
            let handles: Vec<_> = files_to_download
                .chunks(chunk_size)
                .map(|chunk| {
                    let target_dir = &target_dir;
                    let completed = &completed;
                    let progress_cb = &progress_cb;

                    let version_name = &version_name;
                    s.spawn(move || -> anyhow::Result<()> {
                        let thread_client = reqwest::blocking::Client::builder()
                            .connect_timeout(std::time::Duration::from_secs(10))
                            .timeout(std::time::Duration::from_secs(300))
                            .build()?;
                        for (path, info, _executable) in chunk {
                            let dest = target_dir.join(path);
                            if let Some(parent) = dest.parent() {
                                std::fs::create_dir_all(parent)?;
                            }

                            let bytes = thread_client.get(&info.url).send()?.bytes()?;

                            let actual = crate::core::sha1_hex(&bytes);
                            if actual != info.sha1 {
                                anyhow::bail!(
                                    "SHA1 mismatch for {}: expected {}, got {}",
                                    path,
                                    info.sha1,
                                    actual
                                );
                            }

                            std::fs::write(&dest, &bytes)?;

                            #[cfg(unix)]
                            if *_executable {
                                use std::os::unix::fs::PermissionsExt;
                                let perms = std::fs::Permissions::from_mode(0o755);
                                std::fs::set_permissions(&dest, perms)?;
                            }

                            let done =
                                completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                            if done.is_multiple_of(50) || done == total {
                                progress_cb(&format!(
                                    "Downloading Java {} ({}/{})...",
                                    version_name, done, total
                                ));
                            }
                        }
                        Ok(())
                    })
                })
                .collect();

            for handle in handles {
                if let Ok(Err(e)) = handle.join() {
                    let mut guard = thread_error.lock_or_recover();
                    if guard.is_none() {
                        *guard = Some(e);
                    }
                }
            }
        });

        if let Some(e) = thread_error.into_inner().unwrap() {
            return Err(e);
        }
    }

    // Create symlinks
    #[cfg(unix)]
    for (link_path, target) in &links {
        let full_link = target_dir.join(link_path);
        if let Some(parent) = full_link.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_file(&full_link);
        std::os::unix::fs::symlink(target, &full_link)?;
    }

    #[cfg(windows)]
    let _ = links;

    // Probe the installed binary
    let bin = target_dir.join("bin").join(java_binary_name());
    let mut inst = probe_java(&bin).ok_or_else(|| {
        anyhow::anyhow!(
            "Downloaded Mojang Java ({}) but could not probe the binary",
            component
        )
    })?;
    inst.managed = true;
    inst.vendor = "Mojang".to_string();

    progress_cb(&format!(
        "Mojang Java {} installed ({})",
        version_name, inst.version
    ));

    Ok(inst)
}

/// Map a required Java major version to a Mojang runtime component name.
/// Used as a fallback when the version manifest doesn't specify a component.
pub fn major_to_mojang_component(major: u32) -> Option<&'static str> {
    match major {
        8 => Some("jre-legacy"),
        16 => Some("java-runtime-alpha"),
        17 => Some("java-runtime-gamma"),
        21 => Some("java-runtime-delta"),
        25 => Some("java-runtime-epsilon"),
        _ => None,
    }
}

/// Returns the list of Java major versions available from Mojang's runtime distribution.
pub fn mojang_available_versions() -> Vec<u32> {
    vec![8, 16, 17, 21, 25]
}

/// Delete a managed Java installation by removing its directory.
/// Only works for managed installs (downloaded by Lurch).
pub fn delete_managed_java(install: &JavaInstall) -> anyhow::Result<()> {
    if !install.managed {
        anyhow::bail!("Cannot delete non-managed Java installation");
    }
    // Path is the java binary: {managed_dir}/{subdir}/bin/java
    // Go up two levels to get the install directory
    let install_dir = install
        .path
        .parent() // bin/
        .and_then(|p| p.parent()) // {subdir}/
        .ok_or_else(|| anyhow::anyhow!("Could not determine install directory"))?;

    // Safety check: ensure it's under our managed directory
    let managed = java_managed_dir()?;
    if !install_dir.starts_with(&managed) {
        anyhow::bail!("Install directory is not under managed Java directory");
    }

    std::fs::remove_dir_all(install_dir)?;
    Ok(())
}


