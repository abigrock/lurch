use std::path::PathBuf;

use anyhow::Context;
use directories::ProjectDirs;

pub fn app_dirs() -> anyhow::Result<ProjectDirs> {
    ProjectDirs::from("org", "abigrock", "Lurch")
        .context("Failed to determine platform application directories")
}

pub fn data_dir() -> anyhow::Result<PathBuf> {
    let dir = app_dirs()?.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn config_dir() -> anyhow::Result<PathBuf> {
    let dir = app_dirs()?.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn themes_dir() -> anyhow::Result<PathBuf> {
    let dir = data_dir()?.join("themes");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn instances_dir() -> anyhow::Result<PathBuf> {
    let dir = data_dir()?.join("instances");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
