use std::collections::HashMap;

use crate::core::instance::{ModLoader, ModOrigin};

#[derive(Debug, Clone)]
pub struct ModpackUpdateInfo {
    pub latest_version_id: String,
    pub latest_version_name: String,
    #[allow(dead_code)]
    pub current_version_id: String,
    pub current_version_name: String,
    pub source: String,
    pub project_id: String,
}

#[derive(Debug, Clone)]
pub struct ModUpdateInfo {
    pub filename: String,
    pub source: String,
    pub project_id: String,
    pub latest_version_id: String,
    pub latest_version_name: String,
    #[allow(dead_code)]
    pub current_version_id: String,
}

pub type ModUpdateMap = HashMap<String, ModUpdateInfo>;

pub fn check_mod_updates(
    origins: &[ModOrigin],
    mc_version: &str,
    loader: &ModLoader,
) -> ModUpdateMap {
    let mut results = ModUpdateMap::new();

    for origin in origins {
        let current_version_id = match origin.version_id.as_deref() {
            Some(v) => v,
            None => continue,
        };

        let update = match origin.source.as_str() {
            "modrinth" => check_mr_mod_update(origin, current_version_id, mc_version, loader),
            "curseforge" => check_cf_mod_update(origin, current_version_id, mc_version, loader),
            _ => continue,
        };

        if let Some(info) = update {
            results.insert(info.filename.clone(), info);
        }
    }

    results
}

fn check_mr_mod_update(
    origin: &ModOrigin,
    current_version_id: &str,
    mc_version: &str,
    loader: &ModLoader,
) -> Option<ModUpdateInfo> {
    let project_id = origin.project_id.as_deref()?;
    let loader_str = match loader {
        ModLoader::Vanilla => "",
        ModLoader::Forge => "forge",
        ModLoader::NeoForge => "neoforge",
        ModLoader::Fabric => "fabric",
        ModLoader::Quilt => "quilt",
    };
    let loader_arg = if loader_str.is_empty() {
        None
    } else {
        Some(loader_str)
    };
    let versions =
        crate::core::modrinth::get_project_versions(project_id, Some(mc_version), loader_arg)
            .ok()?;
    let latest = versions.first()?;

    if latest.id != current_version_id {
        Some(ModUpdateInfo {
            filename: origin.filename.clone(),
            source: "modrinth".to_string(),
            project_id: project_id.to_string(),
            latest_version_id: latest.id.clone(),
            latest_version_name: latest.name.clone(),
            current_version_id: current_version_id.to_string(),
        })
    } else {
        None
    }
}

fn check_cf_mod_update(
    origin: &ModOrigin,
    current_version_id: &str,
    mc_version: &str,
    loader: &ModLoader,
) -> Option<ModUpdateInfo> {
    let project_id = origin.project_id.as_deref()?;
    let mod_id: u64 = project_id.parse().ok()?;
    let loader_type = crate::core::curseforge::mod_loader_type(loader);
    let files = crate::core::curseforge::get_cf_mod_files(mod_id, mc_version, loader_type).ok()?;
    let latest = files.first()?;
    let latest_id_str = latest.id.to_string();

    if latest_id_str != current_version_id {
        Some(ModUpdateInfo {
            filename: origin.filename.clone(),
            source: "curseforge".to_string(),
            project_id: project_id.to_string(),
            latest_version_id: latest_id_str,
            latest_version_name: latest.display_name.clone(),
            current_version_id: current_version_id.to_string(),
        })
    } else {
        None
    }
}

pub fn check_modpack_updates(
    instances: &[(String, String, crate::core::instance::ModpackOrigin)],
) -> HashMap<String, ModpackUpdateInfo> {
    let mut results = HashMap::new();

    for (instance_id, mc_version, origin) in instances {
        let update = match origin.source.as_str() {
            "modrinth" => check_modrinth_update(origin, mc_version),
            "curseforge" => check_curseforge_update(origin, mc_version),
            _ => continue,
        };

        if let Some(info) = update {
            results.insert(instance_id.clone(), info);
        }
    }

    results
}

fn check_modrinth_update(
    origin: &crate::core::instance::ModpackOrigin,
    mc_version: &str,
) -> Option<ModpackUpdateInfo> {
    let versions =
        crate::core::modrinth::get_project_versions(&origin.project_id, Some(mc_version), None)
            .ok()?;
    let latest = versions.first()?;

    if latest.id != origin.version_id {
        Some(ModpackUpdateInfo {
            latest_version_id: latest.id.clone(),
            latest_version_name: latest.name.clone(),
            current_version_id: origin.version_id.clone(),
            current_version_name: origin.version_name.clone(),
            source: "modrinth".to_string(),
            project_id: origin.project_id.clone(),
        })
    } else {
        None
    }
}

fn check_curseforge_update(
    origin: &crate::core::instance::ModpackOrigin,
    mc_version: &str,
) -> Option<ModpackUpdateInfo> {
    let mod_id: u64 = origin.project_id.parse().ok()?;
    let files = crate::core::curseforge::get_cf_mod_files(mod_id, mc_version, None).ok()?;
    let latest = files.first()?;
    let latest_id_str = latest.id.to_string();

    if latest_id_str != origin.version_id {
        Some(ModpackUpdateInfo {
            latest_version_id: latest_id_str,
            latest_version_name: latest.display_name.clone(),
            current_version_id: origin.version_id.clone(),
            current_version_name: origin.version_name.clone(),
            source: "curseforge".to_string(),
            project_id: origin.project_id.clone(),
        })
    } else {
        None
    }
}
