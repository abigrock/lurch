use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub current_theme: String,
    pub default_java_path: Option<PathBuf>,
    pub default_min_memory_mb: u32,
    pub default_max_memory_mb: u32,
    pub default_jvm_args: Vec<String>,
    pub window_width: f32,
    pub window_height: f32,
    /// Optional CurseForge API key override (uses embedded default if None/empty)
    pub curseforge_api_key: Option<String>,
    pub global_env_vars: Vec<(String, String)>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            current_theme: "Catppuccin Mocha".to_string(),
            default_java_path: None,
            default_min_memory_mb: 512,
            default_max_memory_mb: 2048,
            default_jvm_args: Vec::new(),
            window_width: 1024.0,
            window_height: 768.0,
            curseforge_api_key: None,
            global_env_vars: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }

    fn config_path() -> PathBuf {
        crate::util::paths::config_dir()
            .map(|d| d.join("config.json"))
            .unwrap_or_else(|_| PathBuf::from("lurch_config.json"))
    }
}
