use serde::Deserialize;
use std::path::Path;

const MODRINTH_BASE: &str = "https://api.modrinth.com/v2";

// ── API response types ──────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub categories: Vec<String>,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub project_type: String, // "mod", "resourcepack", "shader", "datapack"
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    pub dependencies: Vec<Dependency>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub hashes: FileHashes,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct FileHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub dependency_type: String, // "required", "optional", "incompatible", "embedded"
}

// ── Sort ─────────────────────────────────────────────────────────────────────

/// Modrinth search sort indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MrSortIndex {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl MrSortIndex {
    pub const ALL: &[MrSortIndex] = &[
        Self::Relevance,
        Self::Downloads,
        Self::Follows,
        Self::Newest,
        Self::Updated,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Relevance => "Relevance",
            Self::Downloads => "Downloads",
            Self::Follows => "Follows",
            Self::Newest => "Newest",
            Self::Updated => "Last Updated",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

// ── API functions (blocking) ────────────────────────────────────────────────

/// Search Modrinth for mods/resourcepacks/shaders compatible with given MC version and loader
pub fn search_mods(
    query: &str,
    mc_version: &str,
    loader: &str,
    project_type: &str, // "mod", "resourcepack", "shader"
    offset: u32,
    sort: MrSortIndex,
    category: Option<&str>,
) -> anyhow::Result<SearchResponse> {
    let client = crate::core::http_client();

    // Build facets: [[project_type], [versions:X], [categories:loader], [categories:cat]]
    // Only add loader facet if it's not empty/vanilla
    let mut facets = vec![format!("[\"project_type:{project_type}\"]")];
    if !mc_version.is_empty() {
        facets.push(format!("[\"versions:{mc_version}\"]"));
    }
    if !loader.is_empty() && loader != "vanilla" {
        facets.push(format!("[\"categories:{loader}\"]"));
    }
    if let Some(cat) = category {
        facets.push(format!("[\"categories:{cat}\"]"));
    }
    let facets_str = format!("[{}]", facets.join(","));

    let sort_str = sort.as_str();
    let resp = client
        .get(format!("{MODRINTH_BASE}/search"))
        .query(&[
            ("query", query),
            ("facets", &facets_str),
            ("limit", "20"),
            ("offset", &offset.to_string()),
            ("index", sort_str),
        ])
        .send()?;

    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Modrinth search failed (HTTP {status}): {body}");
    }
    Ok(serde_json::from_str(&body)?)
}

// ── Category fetching ───────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct MrCategory {
    pub name: String,
    pub project_type: String,
    pub header: String,
}

/// Fetch Modrinth categories filtered by project type (e.g. "mod", "modpack").
pub fn fetch_mr_categories(project_type: &str) -> anyhow::Result<Vec<MrCategory>> {
    let client = crate::core::http_client();

    let resp = client.get(format!("{MODRINTH_BASE}/tag/category")).send()?;

    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Modrinth categories fetch failed (HTTP {status}): {body}");
    }
    let all: Vec<MrCategory> = serde_json::from_str(&body)?;
    let mut cats: Vec<MrCategory> = all
        .into_iter()
        .filter(|c| c.project_type == project_type && c.header == "categories")
        .collect();
    cats.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(cats)
}

/// Get available versions for a project, optionally filtered by MC version and loader
pub fn get_project_versions(
    project_id: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> anyhow::Result<Vec<ProjectVersion>> {
    let client = crate::core::http_client();

    let mut url = format!("{MODRINTH_BASE}/project/{project_id}/version");
    let mut params = Vec::new();
    if let Some(v) = mc_version {
        params.push(format!("game_versions=[\"{v}\"]"));
    }
    if let Some(l) = loader
        && l != "vanilla"
    {
        params.push(format!("loaders=[\"{l}\"]"));
    }
    if !params.is_empty() {
        url = format!("{url}?{}", params.join("&"));
    }

    let resp = client.get(&url).send()?;

    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("Modrinth versions failed (HTTP {status}): {body}");
    }
    Ok(serde_json::from_str(&body)?)
}

/// Download a mod file to the instance mods directory. Returns the filename.
pub fn download_mod_file(file: &VersionFile, mods_dir: &Path) -> anyhow::Result<String> {
    let dest = mods_dir.join(&file.filename);
    let sha1 = file.hashes.sha1.as_deref();
    let client = crate::core::http_client();

    crate::core::mod_cache::resolve_or_download(&file.filename, sha1, &dest, || {
        let resp = client.get(&file.url).send()?;
        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("Download failed (HTTP {status})");
        }
        Ok(resp.bytes()?.to_vec())
    })?;

    Ok(file.filename.clone())
}

pub fn modrinth_project_url(slug: &str) -> String {
    format!("https://modrinth.com/project/{slug}")
}
