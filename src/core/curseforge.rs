use serde::Deserialize;

use crate::core::instance::ModLoader;

const CF_BASE: &str = "https://api.curseforge.com";
const CF_GAME_ID: u32 = 432; // Minecraft

// Placeholder — supply your own CurseForge API key in Settings, or replace this constant.
const DEFAULT_API_KEY: &str = "REPLACE_WITH_YOUR_CURSEFORGE_API_KEY";

pub const CLASS_MODS: u32 = 6;
pub const CLASS_RESOURCE_PACKS: u32 = 12;
pub const CLASS_SHADERS: u32 = 6552;
#[allow(dead_code)]
pub const CLASS_MODPACKS: u32 = 4471;

#[derive(Debug, Clone, Deserialize)]
pub struct CfSearchResponse {
    pub data: Vec<CfMod>,
    pub pagination: CfPagination,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub summary: String,
    pub download_count: u64,
    pub logo: Option<CfLogo>,
    pub categories: Vec<CfCategory>,
    pub allow_mod_distribution: Option<bool>,
    pub latest_files_indexes: Vec<CfLatestFileIndex>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfLogo {
    pub thumbnail_url: String,
    #[serde(default)]
    pub url: Option<String>,
}

impl CfLogo {
    /// Prefer `thumbnail_url`; fall back to `url` if thumbnail is empty.
    pub fn best_url(&self) -> &str {
        if !self.thumbnail_url.is_empty() {
            return &self.thumbnail_url;
        }
        if let Some(ref url) = self.url {
            if !url.is_empty() {
                return url;
            }
        }
        &self.thumbnail_url
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CfCategory {
    pub id: u64,
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfLatestFileIndex {
    pub game_version: String,
    pub file_id: u64,
    pub filename: String,
    pub mod_loader: Option<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfPagination {
    pub index: u32,
    pub page_size: u32,
    pub result_count: u32,
    pub total_count: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CfFilesResponse {
    pub data: Vec<CfFile>,
    pub pagination: CfPagination,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    pub id: u64,
    pub mod_id: u64,
    pub display_name: String,
    pub file_name: String,
    pub release_type: u32, // 1=Release, 2=Beta, 3=Alpha
    pub file_length: u64,
    pub download_url: Option<String>, // null when distribution disabled
    pub game_versions: Vec<String>,
    pub hashes: Vec<CfHash>,
    pub dependencies: Vec<CfDependency>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct CfHash {
    pub value: String,
    pub algo: u32, // 1=SHA1, 2=MD5
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfDependency {
    pub mod_id: u64,
    pub relation_type: u32, // 1=Embedded, 2=Optional, 3=Required, 4=Tool, 5=Incompatible, 6=Include
}

/// Map our ModLoader enum to CurseForge's modLoaderType integer.
/// Returns None for Vanilla (use no filter).
pub fn mod_loader_type(loader: &ModLoader) -> Option<u32> {
    match loader {
        ModLoader::Vanilla => None,
        ModLoader::Forge => Some(1),
        ModLoader::Fabric => Some(4),
        ModLoader::Quilt => Some(5),
        ModLoader::NeoForge => Some(6),
    }
}

/// CurseForge search sort fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CfSortField {
    #[default]
    Featured = 1,
    Popularity = 2,
    LastUpdated = 3,
    Name = 4,
    TotalDownloads = 6,
}

impl CfSortField {
    pub const ALL: &[CfSortField] = &[
        Self::Featured,
        Self::Popularity,
        Self::LastUpdated,
        Self::Name,
        Self::TotalDownloads,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Featured => "Featured",
            Self::Popularity => "Popularity",
            Self::LastUpdated => "Last Updated",
            Self::Name => "Name",
            Self::TotalDownloads => "Total Downloads",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Featured => "1",
            Self::Popularity => "2",
            Self::LastUpdated => "3",
            Self::Name => "4",
            Self::TotalDownloads => "6",
        }
    }
}

fn get_api_key() -> String {
    let config = crate::core::config::AppConfig::load();
    config
        .curseforge_api_key
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| DEFAULT_API_KEY.to_string())
}

pub fn search_cf_mods(
    query: &str,
    mc_version: &str,
    loader: Option<u32>,
    class_id: u32,
    offset: u32,
    sort_field: CfSortField,
    category_id: Option<u64>,
) -> anyhow::Result<CfSearchResponse> {
    let client = crate::core::http_client();
    let api_key = get_api_key();

    let sort_order = match sort_field {
        CfSortField::Name => "asc",
        _ => "desc",
    };

    let mut params: Vec<(&str, String)> = vec![
        ("gameId", CF_GAME_ID.to_string()),
        ("classId", class_id.to_string()),
        ("searchFilter", query.to_string()),
        ("sortField", sort_field.as_str().to_string()),
        ("sortOrder", sort_order.to_string()),
        ("pageSize", "20".to_string()),
        ("index", offset.to_string()),
    ];
    if !mc_version.is_empty() {
        params.push(("gameVersion", mc_version.to_string()));
    }
    if let Some(loader_type) = loader {
        params.push(("modLoaderType", loader_type.to_string()));
    }
    if let Some(cat_id) = category_id {
        params.push(("categoryId", cat_id.to_string()));
    }

    let resp = client
        .get(format!("{CF_BASE}/v1/mods/search"))
        .header("x-api-key", &api_key)
        .query(&params)
        .send()?;

    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("CurseForge search failed (HTTP {status}): {body}");
    }
    Ok(serde_json::from_str(&body)?)
}

// ── Category fetching ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct CfCategoriesResponse {
    data: Vec<CfCategory>,
}

/// Fetch CurseForge categories for a given class (e.g. CLASS_MODS, CLASS_MODPACKS).
pub fn fetch_cf_categories(class_id: u32) -> anyhow::Result<Vec<CfCategory>> {
    let client = crate::core::http_client();
    let api_key = get_api_key();

    let resp = client
        .get(format!("{CF_BASE}/v1/categories"))
        .header("x-api-key", &api_key)
        .query(&[
            ("gameId", CF_GAME_ID.to_string()),
            ("classId", class_id.to_string()),
        ])
        .send()?;

    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("CurseForge categories fetch failed (HTTP {status}): {body}");
    }
    let wrapper: CfCategoriesResponse = serde_json::from_str(&body)?;
    // Filter out the class-level entry itself (parent category with same id) and sort by name
    let mut cats: Vec<CfCategory> = wrapper
        .data
        .into_iter()
        .filter(|c| c.name != "Modpacks" && c.name != "Mods")
        .collect();
    cats.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(cats)
}

pub fn get_cf_mod_files(
    mod_id: u64,
    mc_version: &str,
    loader: Option<u32>,
) -> anyhow::Result<Vec<CfFile>> {
    let client = crate::core::http_client();
    let api_key = get_api_key();

    let mut params: Vec<(&str, String)> = Vec::new();
    if !mc_version.is_empty() {
        params.push(("gameVersion", mc_version.to_string()));
    }
    if let Some(loader_type) = loader {
        params.push(("modLoaderType", loader_type.to_string()));
    }

    let resp = client
        .get(format!("{CF_BASE}/v1/mods/{mod_id}/files"))
        .header("x-api-key", &api_key)
        .query(&params)
        .send()?;

    let status = resp.status();
    let body = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("CurseForge files failed (HTTP {status}): {body}");
    }
    let resp: CfFilesResponse = serde_json::from_str(&body)?;
    Ok(resp.data)
}

pub fn download_cf_file(file: &CfFile, mods_dir: &std::path::Path) -> anyhow::Result<String> {
    let url = file
        .download_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("This mod does not allow 3rd-party distribution"))?;

    let client = crate::core::http_client();
    let resp = client.get(url).send()?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("Download failed (HTTP {status})");
    }

    let bytes = resp.bytes()?;

    // Verify SHA1 if available
    if let Some(hash) = file.hashes.iter().find(|h| h.algo == 1) {
        let mut hasher = sha1_smol::Sha1::new();
        hasher.update(&bytes);
        let actual = hasher.hexdigest();
        if actual != hash.value {
            anyhow::bail!("SHA1 mismatch: expected {}, got {actual}", hash.value);
        }
    }

    std::fs::create_dir_all(mods_dir)?;
    let dest = mods_dir.join(&file.file_name);
    std::fs::write(&dest, &bytes)?;
    Ok(file.file_name.clone())
}

pub fn curseforge_mod_url(mod_id: u64, slug: &str) -> String {
    let _ = mod_id;
    format!("https://www.curseforge.com/minecraft/mc-mods/{slug}")
}

pub fn curseforge_modpack_url(mod_id: u64, slug: &str) -> String {
    let _ = mod_id;
    format!("https://www.curseforge.com/minecraft/modpacks/{slug}")
}

/// URL to download a specific file from CurseForge (user downloads manually).
/// Uses the project's websiteUrl if available, falling back to slug-based mc-mods URL.
pub fn curseforge_file_download_url(slug: &str, file_id: u64, website_url: Option<&str>) -> String {
    if let Some(base) = website_url {
        let base = base.trim_end_matches('/');
        format!("{base}/download/{file_id}")
    } else {
        format!("https://www.curseforge.com/minecraft/mc-mods/{slug}/download/{file_id}")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfBatchFilesResponse {
    pub data: Vec<CfFile>,
}

/// Resolve multiple files by ID in one request.
/// POST /v1/mods/files  with body { "fileIds": [...] }
pub fn batch_get_files(file_ids: &[u64]) -> anyhow::Result<Vec<CfFile>> {
    let client = crate::core::http_client();
    let api_key = get_api_key();

    let body = serde_json::json!({ "fileIds": file_ids });

    let resp = client
        .post(format!("{CF_BASE}/v1/mods/files"))
        .header("x-api-key", &api_key)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()?;

    let status = resp.status();
    let text = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("CurseForge batch files failed (HTTP {status}): {text}");
    }
    let resp: CfBatchFilesResponse = serde_json::from_str(&text)?;
    Ok(resp.data)
}

// ── Batch mod/project lookup ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfModInfoLinks {
    pub website_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfModInfo {
    pub id: u64,
    pub class_id: Option<u32>,
    pub allow_mod_distribution: Option<bool>,
    pub slug: String,
    pub links: Option<CfModInfoLinks>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CfBatchModsResponse {
    pub data: Vec<CfModInfo>,
}

/// Resolve multiple mods/projects by ID in one request to get their classId.
/// POST /v1/mods  with body { "modIds": [...] }
pub fn batch_get_mods(mod_ids: &[u64]) -> anyhow::Result<Vec<CfModInfo>> {
    let client = crate::core::http_client();
    let api_key = get_api_key();

    let body = serde_json::json!({ "modIds": mod_ids });

    let resp = client
        .post(format!("{CF_BASE}/v1/mods"))
        .header("x-api-key", &api_key)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()?;

    let status = resp.status();
    let text = resp.text()?;
    if !status.is_success() {
        anyhow::bail!("CurseForge batch mods failed (HTTP {status}): {text}");
    }
    let resp: CfBatchModsResponse = serde_json::from_str(&text)?;
    Ok(resp.data)
}
