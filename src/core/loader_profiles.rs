use serde::Deserialize;

use super::version::{ArgumentValue, Arguments, Library, VersionInfo};

// ── Mod loader support ───────────────────────────────────────────────────────

/// Fabric loader versions endpoint
pub const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
/// Quilt loader versions endpoint
pub const QUILT_META_URL: &str = "https://meta.quiltmc.org/v3/versions/loader";

/// An entry from the Fabric/Quilt loader versions list
#[derive(Debug, Clone, Deserialize)]
pub struct LoaderVersionEntry {
    pub loader: LoaderVersionInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoaderVersionInfo {
    pub version: String,
    #[serde(default)]
    pub stable: bool,
}

/// Deserialized profile JSON from Fabric/Quilt meta API
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderProfile {
    pub main_class: String,
    #[serde(default)]
    pub arguments: Option<LoaderArguments>,
    #[serde(default)]
    pub libraries: Vec<LoaderLibrary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoaderArguments {
    #[serde(default)]
    pub game: Vec<String>,
    #[serde(default)]
    pub jvm: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoaderLibrary {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

/// Fetch available loader versions for a Minecraft version.
/// Returns `(version_string, is_stable)` pairs.
pub fn fetch_loader_versions(
    client: &reqwest::blocking::Client,
    loader: &crate::core::instance::ModLoader,
    mc_version: &str,
) -> anyhow::Result<Vec<(String, bool)>> {
    use crate::core::instance::ModLoader;

    // Forge/NeoForge have their own version listing APIs
    match loader {
        ModLoader::Forge => {
            return crate::core::forge::fetch_forge_versions(client, mc_version);
        }
        ModLoader::NeoForge => {
            return crate::core::forge::fetch_neoforge_versions(client, mc_version);
        }
        _ => {}
    }

    let url = match loader {
        ModLoader::Fabric => format!("{FABRIC_META_URL}/{mc_version}"),
        ModLoader::Quilt => format!("{QUILT_META_URL}/{mc_version}"),
        other => anyhow::bail!("Loader version listing not supported for {other}"),
    };

    let resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to fetch {loader} versions for MC {mc_version}: HTTP {}",
            resp.status()
        );
    }

    let entries: Vec<LoaderVersionEntry> = resp.json()?;
    Ok(entries
        .iter()
        .map(|e| (e.loader.version.clone(), e.loader.stable))
        .collect())
}

/// Fetch a loader profile JSON and merge it into the vanilla VersionInfo.
/// Overrides main_class, prepends loader libraries, merges arguments.
pub fn fetch_and_merge_loader_profile(
    client: &reqwest::blocking::Client,
    loader: &crate::core::instance::ModLoader,
    mc_version: &str,
    loader_version: &str,
    info: &mut VersionInfo,
) -> anyhow::Result<()> {
    use crate::core::instance::ModLoader;

    // Forge/NeoForge use installer-based profiles
    match loader {
        ModLoader::Forge | ModLoader::NeoForge => {
            let profile = crate::core::forge::download_and_extract_installer(
                client,
                loader,
                mc_version,
                loader_version,
            )?;

            // Override main class with loader's
            info.main_class = profile.main_class;

            // Prepend loader libraries (must come before vanilla on classpath)
            let mut all_libs = profile.libraries;
            all_libs.append(&mut info.libraries);
            info.libraries = all_libs;

            // Merge arguments (modern Forge)
            if let Some(forge_args) = profile.arguments {
                if let Some(ref mut args) = info.arguments {
                    // Prepend Forge JVM args
                    let mut new_jvm = forge_args.jvm;
                    new_jvm.append(&mut args.jvm);
                    args.jvm = new_jvm;

                    // Append Forge game args
                    args.game.extend(forge_args.game);
                } else {
                    info.arguments = Some(forge_args);
                }
            }

            // Handle legacy minecraftArguments (replace entirely — Forge includes all args)
            if let Some(forge_mc_args) = profile.minecraft_arguments {
                info.minecraft_arguments = Some(forge_mc_args);
            }

            return Ok(());
        }
        _ => {}
    }

    let url = match loader {
        ModLoader::Fabric => {
            format!("{FABRIC_META_URL}/{mc_version}/{loader_version}/profile/json")
        }
        ModLoader::Quilt => {
            format!("{QUILT_META_URL}/{mc_version}/{loader_version}/profile/json")
        }
        ModLoader::Vanilla => return Ok(()),
        other => anyhow::bail!("{other} is not yet supported. Use Fabric or Quilt instead."),
    };

    let resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "Failed to fetch {loader} {loader_version} profile: HTTP {}",
            resp.status()
        );
    }

    let profile: LoaderProfile = resp.json()?;

    // Override main class with loader's
    info.main_class = profile.main_class;

    // Prepend loader libraries (they must come before vanilla libs on classpath)
    let loader_libs: Vec<Library> = profile
        .libraries
        .into_iter()
        .map(|l| Library {
            name: l.name,
            downloads: None,
            rules: Vec::new(),
            url: l.url,
        })
        .collect();
    let mut all_libs = loader_libs;
    all_libs.append(&mut info.libraries);
    info.libraries = all_libs;

    // Merge arguments
    if let Some(loader_args) = profile.arguments {
        if let Some(ref mut args) = info.arguments {
            // Prepend loader JVM args
            let mut new_jvm: Vec<ArgumentValue> = loader_args
                .jvm
                .into_iter()
                .map(ArgumentValue::Plain)
                .collect();
            new_jvm.append(&mut args.jvm);
            args.jvm = new_jvm;

            // Append loader game args
            for a in loader_args.game {
                args.game.push(ArgumentValue::Plain(a));
            }
        } else {
            // No existing arguments — create from loader args
            info.arguments = Some(Arguments {
                jvm: loader_args
                    .jvm
                    .into_iter()
                    .map(ArgumentValue::Plain)
                    .collect(),
                game: loader_args
                    .game
                    .into_iter()
                    .map(ArgumentValue::Plain)
                    .collect(),
            });
        }
    }

    Ok(())
}

/// Resolve the loader version to use. If `loader_version` is Some, returns it.
/// Otherwise, fetches available versions and picks the latest stable one.
pub fn resolve_loader_version(
    client: &reqwest::blocking::Client,
    loader: &crate::core::instance::ModLoader,
    mc_version: &str,
    loader_version: &Option<String>,
) -> anyhow::Result<String> {
    if let Some(v) = loader_version {
        return Ok(v.clone());
    }

    let versions = fetch_loader_versions(client, loader, mc_version)?;
    versions
        .iter()
        .find(|(_, stable)| *stable)
        .or(versions.first())
        .map(|(v, _)| v.clone())
        .ok_or_else(|| anyhow::anyhow!("No {loader} versions found for Minecraft {mc_version}"))
}
