use crate::core::instance::{Instance, ModpackOrigin};
use crate::core::modrinth_modpack;
use crate::core::curseforge_modpack;
use crate::core::curseforge;
use crate::core::MutexExt;
use eframe::egui;
use std::sync::{Arc, Mutex};
use reqwest::blocking::Client;
use crate::core::launch::LaunchProgress;

pub struct ModpackManager {
    pub http_client: Client,
    pub default_min_mem: u32,
    pub default_max_mem: u32,
}

impl ModpackManager {
    pub fn new(http_client: Client, min_mem: u32, max_mem: u32) -> Self {
        Self {
            http_client,
            default_min_mem: min_mem,
            default_max_mem: max_mem,
        }
    }

    pub fn run_modpack_install<F>(
        &self,
        initial_message: String,
        ctx: &egui::Context,
        install_fn: F,
    ) -> (Arc<Mutex<LaunchProgress>>, Arc<Mutex<Option<Instance>>>, Option<Arc<Mutex<Vec<curseforge_modpack::SkippedMod>>>>) 
    where
        F: FnOnce(
                Arc<Mutex<LaunchProgress>>,
                egui::Context,
                Client,
                u32,
                u32,
            ) -> anyhow::Result<Instance>
            + Send
            + 'static,
    {
        let progress = Arc::new(Mutex::new(LaunchProgress {
            message: initial_message,
            done: false,
            error: None,
        }));

        let instance_slot = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&instance_slot);

        let ctx_clone = ctx.clone();
        let progress_clone = Arc::clone(&progress);
        let min_mem = self.default_min_mem;
        let max_mem = self.default_max_mem;
        let client = self.http_client.clone();

        std::thread::spawn(move || {
            let result = install_fn(
                Arc::clone(&progress_clone),
                ctx_clone.clone(),
                client,
                min_mem,
                max_mem,
            );

            match result {
                Ok(inst) => {
                    *slot_clone.lock_or_recover() = Some(inst);
                    let mut p = progress_clone.lock_or_recover();
                    p.message = "Modpack installed successfully!".to_string();
                    p.done = true;
                }
                Err(e) => {
                    let mut p = progress_clone.lock_or_recover();
                    p.done = true;
                    p.error = Some(e.to_string());
                }
            }
            ctx_clone.request_repaint();
        });

        (progress, instance_slot, None)
    }

    pub fn install_modpack(
        &self,
        project_id: String,
        title: String,
        icon_url: Option<String>,
        version_id: Option<String>,
        version_name: Option<String>,
        ctx: &egui::Context,
    ) -> (Arc<Mutex<LaunchProgress>>, Arc<Mutex<Option<Instance>>>) {
        let display_title = title.clone();
        let (progress, slot, _) = self.run_modpack_install(
            format!("Installing modpack \"{}\"...", title),
            ctx,
            move |progress: Arc<Mutex<LaunchProgress>>, ctx: egui::Context, client: Client, min_mem: u32, max_mem: u32| {
                {
                    let mut p = progress.lock_or_recover();
                    p.message = format!("Fetching modpack info for \"{}\"...", display_title);
                }
                ctx.request_repaint();

                let versions = crate::core::modrinth::get_project_versions(&project_id, None, None)?;
                let version = if let Some(ref vid) = version_id {
                    versions
                        .iter()
                        .find(|v| v.id == *vid)
                        .ok_or_else(|| anyhow::anyhow!("Selected version not found"))?
                } else {
                    versions
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("No versions found for modpack"))?
                };
                let file = version
                    .files
                    .iter()
                    .find(|f| f.primary)
                    .or(version.files.first())
                    .ok_or_else(|| anyhow::anyhow!("No files in modpack version"))?;

                {
                    let mut p = progress.lock_or_recover();
                    p.message = "Downloading modpack...".to_string();
                }
                ctx.request_repaint();

                let temp_dir = std::env::temp_dir().join("lurch_modpack_install");
                std::fs::create_dir_all(&temp_dir)?;
                let mrpack_path = temp_dir.join(&file.filename);

                let resp = client.get(&file.url).send()?;
                if !resp.status().is_success() {
                    anyhow::bail!("Failed to download mrpack: HTTP {}", resp.status());
                }
                let bytes = resp.bytes()?;
                std::fs::write(&mrpack_path, &bytes)?;

                {
                    let mut p = progress.lock_or_recover();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let index = modrinth_modpack::parse_mrpack(&mrpack_path)?;

                {
                    let mut p = progress.lock_or_recover();
                    p.message = format!("Creating instance \"{}\"...", index.name);
                }
                ctx.request_repaint();

                let instance = modrinth_modpack::create_instance_from_modpack(&index)?;
                let minecraft_dir = instance.minecraft_dir()?;

                let progress_for_files = Arc::clone(&progress);
                let ctx_for_files = ctx.clone();
                modrinth_modpack::install_modpack_files(
                    &index,
                    &mrpack_path,
                    &minecraft_dir,
                    &client,
                    move |done: usize, total: usize, stage: &str| {
                        let mut p = progress_for_files.lock_or_recover();
                        p.message = if total > 0 {
                            format!("{stage} ({done}/{total})")
                        } else {
                            stage.to_string()
                        };
                        drop(p);
                        ctx_for_files.request_repaint();
                    },
                )?;

                let _ = std::fs::remove_dir_all(&temp_dir);

                let mut instance = instance;
                instance.min_memory_mb = min_mem;
                instance.max_memory_mb = max_mem;
                instance.icon = icon_url;
                instance.modpack_origin = Some(ModpackOrigin {
                    source: "modrinth".to_string(),
                    project_id: project_id.clone(),
                    version_id: version.id.clone(),
                    version_name: version_name.clone().unwrap_or_else(|| version.name.clone()),
                });
                instance.save_to_dir()?;

                Ok(instance)
            },
        );

        (progress, slot)
    }

    pub fn install_cf_modpack(
        &self,
        mod_id: u64,
        title: String,
        icon_url: Option<String>,
        file_id: Option<u64>,
        file_name: Option<String>,
        ctx: &egui::Context,
    ) -> (Arc<Mutex<LaunchProgress>>, Arc<Mutex<Option<Instance>>>, Arc<Mutex<Vec<curseforge_modpack::SkippedMod>>>) {
        let display_title = title.clone();
        let skipped = Arc::new(Mutex::new(Vec::new()));
        let skipped_clone = Arc::clone(&skipped);

        let (progress, slot, _) = self.run_modpack_install(
            format!("Installing modpack \"{}\"...", title),
            ctx,
            move |progress: Arc<Mutex<LaunchProgress>>, ctx: egui::Context, client: Client, min_mem: u32, max_mem: u32| {
                {
                    let mut p = progress.lock_or_recover();
                    p.message = format!("Fetching modpack info for \"{}\"...", display_title);
                }
                ctx.request_repaint();

                let files = curseforge::get_cf_mod_files(mod_id, "", None)?;
                let file = if let Some(fid) = file_id {
                    files
                        .iter()
                        .find(|f| f.id == fid)
                        .ok_or_else(|| anyhow::anyhow!("Selected file not found"))?
                } else {
                    files
                        .first()
                        .ok_or_else(|| anyhow::anyhow!("No files found for modpack"))?
                };
                let temp_dir = std::env::temp_dir().join("lurch_cf_modpack_install");
                let zip_path = if let Some(url) = file.download_url.as_ref() {
                    {
                        let mut p = progress.lock_or_recover();
                        p.message = "Downloading modpack...".to_string();
                    }
                    ctx.request_repaint();

                    std::fs::create_dir_all(&temp_dir)?;
                    let path = temp_dir.join(&file.file_name);
                    let resp = client.get(url).send()?;
                    if !resp.status().is_success() {
                        anyhow::bail!("Failed to download modpack: HTTP {}", resp.status());
                    }
                    let bytes = resp.bytes()?;
                    std::fs::write(&path, &bytes)?;
                    path
                } else {
                    curseforge_modpack::wait_for_cf_manual_download(
                        mod_id, file, &temp_dir, &progress, &ctx,
                    )?
                };

                {
                    let mut p = progress.lock_or_recover();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let manifest = curseforge_modpack::parse_cf_modpack(&zip_path)?;

                {
                    let mut p = progress.lock_or_recover();
                    p.message = format!("Creating instance \"{}\"...", manifest.name);
                }
                ctx.request_repaint();

                let instance = curseforge_modpack::create_instance_from_cf_modpack(&manifest)?;
                let minecraft_dir = instance.minecraft_dir()?;

                let progress_for_files = Arc::clone(&progress);
                let ctx_for_files = ctx.clone();
                let skipped_mods = curseforge_modpack::install_cf_modpack_files(
                    &manifest,
                    &zip_path,
                    &minecraft_dir,
                    move |done: usize, total: usize, stage: &str| {
                        let mut p = progress_for_files.lock_or_recover();
                        p.message = if total > 0 {
                            format!("{stage} ({done}/{total})")
                        } else {
                            stage.to_string()
                        };
                        drop(p);
                        ctx_for_files.request_repaint();
                    },
                )?;

                *skipped_clone.lock_or_recover() = skipped_mods;

                let _ = std::fs::remove_dir_all(&temp_dir);

                let mut instance = instance;
                instance.min_memory_mb = min_mem;
                instance.max_memory_mb = max_mem;
                instance.icon = icon_url;
                instance.modpack_origin = Some(ModpackOrigin {
                    source: "curseforge".to_string(),
                    project_id: mod_id.to_string(),
                    version_id: file.id.to_string(),
                    version_name: file_name
                        .clone()
                        .unwrap_or_else(|| file.display_name.clone()),
                });
                instance.save_to_dir()?;

                Ok(instance)
            },
        );

        (progress, slot, skipped)
    }

    pub fn import_local_mrpack(&self, path: std::path::PathBuf, ctx: &egui::Context) -> (Arc<Mutex<LaunchProgress>>, Arc<Mutex<Option<Instance>>>) {
        let (progress, slot, _) = self.run_modpack_install(
            "Importing Modrinth modpack...".to_string(),
            ctx,
            move |progress: Arc<Mutex<LaunchProgress>>, ctx: egui::Context, client: Client, min_mem: u32, max_mem: u32| {
                {
                    let mut p = progress.lock_or_recover();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let index = modrinth_modpack::parse_mrpack(&path)?;

                {
                    let mut p = progress.lock_or_recover();
                    p.message = format!("Creating instance \"{}\"...", index.name);
                }
                ctx.request_repaint();

                let instance = modrinth_modpack::create_instance_from_modpack(&index)?;
                let minecraft_dir = instance.minecraft_dir()?;

                let progress_for_files = Arc::clone(&progress);
                let ctx_for_files = ctx.clone();
                modrinth_modpack::install_modpack_files(
                    &index,
                    &path,
                    &minecraft_dir,
                    &client,
                    move |done: usize, total: usize, stage: &str| {
                        let mut p = progress_for_files.lock_or_recover();
                        p.message = if total > 0 {
                            format!("{stage} ({done}/{total})")
                        } else {
                            stage.to_string()
                        };
                        drop(p);
                        ctx_for_files.request_repaint();
                    },
                )?;

                let mut instance = instance;
                instance.min_memory_mb = min_mem;
                instance.max_memory_mb = max_mem;
                instance.save_to_dir()?;

                Ok(instance)
            },
        );

        (progress, slot)
    }

    pub fn import_local_cf_modpack(&self, path: std::path::PathBuf, ctx: &egui::Context) -> (Arc<Mutex<LaunchProgress>>, Arc<Mutex<Option<Instance>>>) {
        let (progress, slot, _) = self.run_modpack_install(
            "Importing CurseForge modpack...".to_string(),
            ctx,
            move |progress: Arc<Mutex<LaunchProgress>>, ctx: egui::Context, _client: Client, min_mem: u32, max_mem: u32| {
                {
                    let mut p = progress.lock_or_recover();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let manifest = curseforge_modpack::parse_cf_modpack(&path)?;

                {
                    let mut p = progress.lock_or_recover();
                    p.message = format!("Creating instance \"{}\"...", manifest.name);
                }
                ctx.request_repaint();

                let instance = curseforge_modpack::create_instance_from_cf_modpack(&manifest)?;
                let minecraft_dir = instance.minecraft_dir()?;

                let progress_for_files = Arc::clone(&progress);
                let ctx_for_files = ctx.clone();
                curseforge_modpack::install_cf_modpack_files(
                    &manifest,
                    &path,
                    &minecraft_dir,
                    move |done: usize, total: usize, stage: &str| {
                        let mut p = progress_for_files.lock_or_recover();
                        p.message = if total > 0 {
                            format!("{stage} ({done}/{total})")
                        } else {
                            stage.to_string()
                        };
                        drop(p);
                        ctx_for_files.request_repaint();
                    },
                )?;

                let mut instance = instance;
                instance.min_memory_mb = min_mem;
                instance.max_memory_mb = max_mem;
                instance.save_to_dir()?;

                Ok(instance)
            },
        );

        (progress, slot)
    }
}
