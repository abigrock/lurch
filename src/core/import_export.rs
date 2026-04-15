use crate::core::instance::Instance;
use anyhow::{Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// What kind of archive a zip file is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArchiveType {
    /// Lurch instance export (contains `instance.json`)
    LurchExport,
    /// Modrinth modpack (contains `modrinth.index.json`)
    ModrinthMrpack,
    /// CurseForge modpack (contains `manifest.json`)
    CurseForgeModpack,
    /// Unknown format
    Unknown,
}

/// Probe a zip file to determine its type by checking for known marker files.
pub fn detect_archive_type(path: &Path) -> Result<ArchiveType> {
    let file =
        fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("Not a valid zip: {}", path.display()))?;

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name();
        if name == "instance.json" || name.ends_with("/instance.json") {
            return Ok(ArchiveType::LurchExport);
        }
        if name == "modrinth.index.json" {
            return Ok(ArchiveType::ModrinthMrpack);
        }
        if name == "manifest.json" {
            return Ok(ArchiveType::CurseForgeModpack);
        }
    }

    Ok(ArchiveType::Unknown)
}

/// Export an instance directory to a .zip file.
pub fn export_instance(instance: &Instance, dest_path: &Path) -> Result<()> {
    let instance_dir = instance.instance_dir()?;
    if !instance_dir.exists() {
        anyhow::bail!(
            "Instance directory does not exist: {}",
            instance_dir.display()
        );
    }

    let file = fs::File::create(dest_path)
        .with_context(|| format!("Failed to create {}", dest_path.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, &instance_dir, &instance_dir, options)?;

    zip.finish()?;
    Ok(())
}

fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    base_dir: &Path,
    current_dir: &Path,
    options: SimpleFileOptions,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(current_dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(base_dir)?
            .to_string_lossy()
            .replace('\\', "/");

        if path.is_dir() {
            let dir_name = format!("{relative}/");
            zip.add_directory(&dir_name, options)?;
            add_dir_to_zip(zip, base_dir, &path, options)?;
        } else {
            zip.start_file(&relative, options)?;
            let data =
                fs::read(&path).with_context(|| format!("Failed to read {}", path.display()))?;
            zip.write_all(&data)?;
        }
    }
    Ok(())
}

/// Import an instance from a .zip file exported by Lurch.
/// Returns the newly created Instance with a fresh ID.
pub fn import_instance(zip_path: &Path) -> Result<Instance> {
    let file = fs::File::open(zip_path)
        .with_context(|| format!("Failed to open {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file)?;

    // Find instance.json in the archive
    let json_name = {
        let mut found = None;
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if name == "instance.json" || name.ends_with("/instance.json") {
                found = Some(name);
                break;
            }
        }
        found
    };

    let json_name = json_name.ok_or_else(|| {
        anyhow::anyhow!("No instance.json found in archive. Not a valid Lurch export.")
    })?;

    // Determine the prefix to strip (path before instance.json)
    let prefix = if json_name == "instance.json" {
        String::new()
    } else {
        json_name
            .strip_suffix("instance.json")
            .unwrap_or("")
            .to_string()
    };

    // Read instance.json
    let mut instance: Instance = {
        let mut entry = archive.by_name(&json_name)?;
        let mut json_str = String::new();
        entry.read_to_string(&mut json_str)?;
        serde_json::from_str(&json_str)
            .with_context(|| "Failed to parse instance.json from archive")?
    };

    // Assign fresh ID to avoid collisions
    instance.id = uuid::Uuid::new_v4().to_string();

    // Create destination directory
    let dest_dir = instance.instance_dir()?;
    fs::create_dir_all(&dest_dir)?;

    // Extract all files
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();

        // Strip prefix
        let relative = if prefix.is_empty() {
            entry_name.clone()
        } else if let Some(stripped) = entry_name.strip_prefix(&prefix) {
            stripped.to_string()
        } else {
            continue;
        };

        if relative.is_empty() {
            continue;
        }

        let dest_path = dest_dir.join(&relative);

        // Safety: reject path traversal
        if !dest_path.starts_with(&dest_dir) {
            log::warn!("Skipping path traversal entry: {entry_name}");
            continue;
        }

        if entry.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            fs::write(&dest_path, &data)?;
        }
    }

    // Save updated instance.json with new ID
    instance.save_to_dir()?;

    Ok(instance)
}
