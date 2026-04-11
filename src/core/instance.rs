use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Tracks which source a single mod was installed from, enabling update checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModOrigin {
    /// The on-disk filename this metadata belongs to (soft key)
    pub filename: String,
    /// "modrinth", "curseforge", or "local"
    pub source: String,
    /// Modrinth project_id or CurseForge mod_id (as string)
    pub project_id: Option<String>,
    /// Modrinth version_id or CurseForge file_id (as string)
    pub version_id: Option<String>,
    /// Human-readable version name for display
    pub version_name: Option<String>,
}

/// Tracks which modpack (and version) an instance was created from,
/// enabling update checks later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackOrigin {
    /// "modrinth" or "curseforge"
    pub source: String,
    /// Modrinth project_id or CurseForge mod_id (as string)
    pub project_id: String,
    /// Modrinth version_id or CurseForge file_id (as string)
    pub version_id: String,
    /// Human-readable version name for display
    pub version_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub group: Option<String>,
    pub mc_version: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub java_path: Option<PathBuf>,
    /// Human-readable label for the Java provider, e.g. "Adoptium", "Lurch", "system"
    #[serde(default)]
    pub java_provider: Option<String>,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub jvm_args: Vec<String>,
    pub last_played: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub modpack_origin: Option<ModpackOrigin>,
    #[serde(default)]
    pub mod_origins: Vec<ModOrigin>,
    #[serde(default)]
    pub created: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum ModLoader {
    #[default]
    Vanilla,
    Forge,
    NeoForge,
    Fabric,
    Quilt,
}

impl Instance {
    pub fn new(name: String, mc_version: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            group: None,
            mc_version,
            loader: ModLoader::default(),
            loader_version: None,
            java_path: None,
            java_provider: None,
            min_memory_mb: 512,
            max_memory_mb: 2048,
            jvm_args: Vec::new(),
            last_played: None,
            icon: None,
            modpack_origin: None,
            mod_origins: Vec::new(),
            created: None,
        }
    }

    /// Create a duplicate with a new id and modified name
    pub fn duplicate(&self) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{} (copy)", self.name),
            group: self.group.clone(),
            mc_version: self.mc_version.clone(),
            loader: self.loader.clone(),
            loader_version: self.loader_version.clone(),
            java_path: self.java_path.clone(),
            java_provider: self.java_provider.clone(),
            min_memory_mb: self.min_memory_mb,
            max_memory_mb: self.max_memory_mb,
            jvm_args: self.jvm_args.clone(),
            last_played: None,
            icon: self.icon.clone(),
            modpack_origin: self.modpack_origin.clone(),
            mod_origins: self.mod_origins.clone(),
            created: None,
        }
    }

    /// Directory on disk for this instance
    pub fn instance_dir(&self) -> anyhow::Result<PathBuf> {
        Ok(crate::util::paths::instances_dir()?.join(&self.id))
    }

    /// The .minecraft directory inside this instance
    pub fn minecraft_dir(&self) -> anyhow::Result<PathBuf> {
        Ok(self.instance_dir()?.join(".minecraft"))
    }

    /// Create the directory structure for a new instance
    pub fn create_dirs(&self) -> anyhow::Result<()> {
        let mc = self.minecraft_dir()?;
        std::fs::create_dir_all(&mc)?;
        for sub in ["mods", "resourcepacks", "shaderpacks", "saves", "config"] {
            std::fs::create_dir_all(mc.join(sub))?;
        }
        self.save_to_dir()?;
        Ok(())
    }

    /// Persist instance metadata to its directory
    pub fn save_to_dir(&self) -> anyhow::Result<()> {
        let dir = self.instance_dir()?;
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(dir.join("instance.json"), json)?;
        Ok(())
    }

    /// Load a single instance from a directory containing instance.json
    pub fn load_from_dir(dir: &std::path::Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(dir.join("instance.json"))?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Remove the instance directory from disk
    pub fn delete_dirs(&self) -> anyhow::Result<()> {
        let dir = self.instance_dir()?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn upsert_mod_origin(&mut self, origin: ModOrigin) {
        if let Some(existing) = self
            .mod_origins
            .iter_mut()
            .find(|o| o.filename == origin.filename)
        {
            *existing = origin;
        } else {
            self.mod_origins.push(origin);
        }
    }

    #[allow(dead_code)]
    pub fn remove_mod_origin(&mut self, filename: &str) {
        self.mod_origins.retain(|o| o.filename != filename);
    }

    pub fn reconcile_mod_origins(&mut self, installed_filenames: &[String]) {
        let base_names: Vec<&str> = installed_filenames
            .iter()
            .map(|f| f.strip_suffix(".disabled").unwrap_or(f.as_str()))
            .collect();
        self.mod_origins
            .retain(|o| base_names.contains(&o.filename.as_str()));
    }
}

/// Load all instances from the instances directory
pub fn load_all_instances() -> Vec<Instance> {
    let dir = match crate::util::paths::instances_dir() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut instances = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("instance.json").exists() {
            match Instance::load_from_dir(&path) {
                Ok(inst) => instances.push(inst),
                Err(e) => log::warn!("Failed to load instance at {}: {e}", path.display()),
            }
        }
    }
    // Sort by name
    instances.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    instances
}

impl std::fmt::Display for ModLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Vanilla => "Vanilla",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
        })
    }
}
