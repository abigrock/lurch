use crate::core::account::AccountStore;
use crate::core::config::AppConfig;
use crate::core::curseforge_modpack;
use crate::core::instance::{self, Instance};
use crate::core::java::{self, JavaInstall};
use crate::core::launch::{prepare_and_launch, LaunchProgress, ProcessState};
use crate::core::modrinth_modpack;
use crate::core::version::VersionManifest;
use crate::core::ModpackModEntry;
use crate::theme::{bundled_themes, load_user_themes, seed_user_themes_dir, Theme};
use crate::ui::accounts::AccountsView;
use crate::ui::console::ConsoleView;
use crate::ui::instances::InstancesView;
use crate::ui::settings::SettingsView;
use crate::ui::sidebar::View;
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

type ModpackUpdateMap = std::collections::HashMap<String, crate::core::update::ModpackUpdateInfo>;
type PendingModpackUpdate = Arc<Mutex<Option<(String, crate::core::instance::ModpackOrigin, crate::core::update::UpdatedModpackMeta)>>>;

/// A background task (modpack install, update, etc.) — NOT a running game process.
/// Displayed in the sidebar task tray, not the Console.
pub struct BackgroundTask {
    pub id: String,
    pub label: String,
    pub progress: Arc<Mutex<LaunchProgress>>,
    /// Slot for a newly created instance (modpack install).
    pub instance_slot: Option<Arc<Mutex<Option<Instance>>>>,
    /// Slot for an in-place modpack update result.
    pub update_slot: Option<PendingModpackUpdate>,
    /// Slot for mods that were skipped due to distribution restrictions.
    pub skipped_slot: Option<Arc<Mutex<Vec<curseforge_modpack::SkippedMod>>>>,
}

impl BackgroundTask {
    pub fn is_done(&self) -> bool {
        let p = self.progress.lock().unwrap();
        p.done
    }

    pub fn error(&self) -> Option<String> {
        self.progress.lock().unwrap().error.clone()
    }
}

/// A transient notification shown as a floating overlay.
pub struct Toast {
    pub message: String,
    pub is_error: bool,
    pub created_at: Instant,
}

impl Toast {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: false,
            created_at: Instant::now(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: true,
            created_at: Instant::now(),
        }
    }
}

/// A mod the user must download manually because CurseForge blocks third-party distribution.
/// Lurch watches the user's Downloads folder for this file and auto-moves it.
pub struct PendingManualDownload {
    pub file_name: String,
    pub display_name: String,
    pub target_dir: std::path::PathBuf,
    pub download_url: String,
}

/// Tracks a single running (or recently-exited) instance process.
pub struct RunningProcess {
    pub instance_id: String,
    pub instance_name: String,
    pub progress: Arc<Mutex<LaunchProgress>>,
    #[allow(clippy::type_complexity)]
    pub pending_process: Arc<Mutex<Option<Arc<Mutex<ProcessState>>>>>,
    pub process: Option<Arc<Mutex<ProcessState>>>,
    pub auto_scroll: bool,
}

impl RunningProcess {
    /// Returns true while the instance is preparing or the child process is still running.
    pub fn is_alive(&self) -> bool {
        if let Some(proc) = &self.process
            && proc.lock().unwrap().running
        {
            return true;
        }
        let p = self.progress.lock().unwrap();
        !p.done || p.error.is_none() && self.process.is_none()
    }
}

pub struct App {
    pub config: AppConfig,
    pub themes: Vec<Theme>,
    pub builtin_theme_count: usize,
    pub current_theme_idx: usize,
    pub current_view: View,
    pub instances: Vec<Instance>,
    pub instances_view: InstancesView,
    pub java_installs: Vec<JavaInstall>,
    pub account_store: AccountStore,
    pub accounts_view: AccountsView,
    pub manifest: Arc<Mutex<ManifestState>>,
    pub console_view: ConsoleView,
    pub running_processes: Vec<RunningProcess>,
    pub background_tasks: Vec<BackgroundTask>,
    pub toasts: Vec<Toast>,
    pub java_download: Option<Arc<Mutex<JavaDownloadState>>>,
    pub java_prompt: Option<JavaPromptState>,
    pub launch_after_java_download: Option<String>,
    pub settings_view: SettingsView,
    pub http_client: reqwest::blocking::Client,
    pub modpack_updates: ModpackUpdateMap,
    pub modpack_update_check: Option<Arc<Mutex<Option<ModpackUpdateMap>>>>,
    /// Mods waiting for user to download manually from CurseForge.
    pub pending_manual_downloads: Vec<PendingManualDownload>,
    /// Show the manual-downloads dialog (set when blocked mods are detected).
    pub show_manual_downloads_dialog: bool,
    /// Pre-launch missing mods dialog state.
    pub missing_mods: Option<MissingModsState>,
    /// Set by "Launch Anyway" in the missing-mods dialog; bypasses the mod check.
    pub force_launch_requested: Option<String>,
    /// Completion signal from background mod re-download thread.
    pub mod_redownload_toast: Option<Arc<Mutex<Option<String>>>>,
    /// Throttle for checking the Downloads directory.
    last_download_check: Option<Instant>,
    /// Timestamp of the last rendered frame — used to cap frame rate during resize.
    last_frame_time: Option<Instant>,
}

#[allow(dead_code)]
pub struct JavaDownloadState {
    pub version: u32,
    pub message: String,
    pub done: bool,
    pub result: Option<Result<JavaInstall, String>>,
}

pub enum ManifestState {
    Loading,
    Loaded(VersionManifest),
    Failed(String),
}

pub struct JavaPromptState {
    pub instance_id: String,
    pub instance_name: String,
    pub required_java: u32,
    pub component: Option<String>,
}

/// Shown when a modpack instance is missing expected mod files before launch.
pub struct MissingModsState {
    pub instance_id: String,
    pub instance_name: String,
    pub missing_files: Vec<ModpackModEntry>,
}

impl App {
    pub fn new(ctx: egui::Context) -> Self {
        let config = AppConfig::load();
        let instances = instance::load_all_instances();
        let java_installs = java::detect_java_installations();
        let account_store = AccountStore::load();

        let http_client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(600))
            .user_agent(crate::core::USER_AGENT)
            .build()
            .expect("Failed to build HTTP client");

        let mut themes = bundled_themes();
        let builtin_theme_count = themes.len();
        seed_user_themes_dir();
        if let Ok(user) = load_user_themes() {
            themes.extend(user);
        }

        let current_theme_idx = themes
            .iter()
            .position(|t| t.name == config.current_theme)
            .unwrap_or(themes.len().saturating_sub(1));

        // Start background manifest fetch
        let manifest = Arc::new(Mutex::new(ManifestState::Loading));
        {
            let manifest = Arc::clone(&manifest);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                match crate::core::version::fetch_manifest() {
                    Ok(m) => *manifest.lock().unwrap() = ManifestState::Loaded(m),
                    Err(e) => *manifest.lock().unwrap() = ManifestState::Failed(e.to_string()),
                }
                ctx.request_repaint();
            });
        }

        // Start background fetch for available Adoptium Java versions is now handled by SettingsView::new()

        let origins: Vec<(String, String, crate::core::instance::ModpackOrigin)> = instances
            .iter()
            .filter_map(|inst| {
                inst.modpack_origin
                    .as_ref()
                    .map(|o| (inst.id.clone(), inst.mc_version.clone(), o.clone()))
            })
            .collect();

        let modpack_update_check = if origins.is_empty() {
            None
        } else {
            let slot: Arc<Mutex<Option<ModpackUpdateMap>>> = Arc::new(Mutex::new(None));
            let slot_clone = Arc::clone(&slot);
            let ctx_clone = ctx.clone();
            std::thread::spawn(move || {
                let results = crate::core::update::check_modpack_updates(&origins);
                *slot_clone.lock().unwrap() = Some(results);
                ctx_clone.request_repaint();
            });
            Some(slot)
        };

        Self {
            config,
            themes,
            builtin_theme_count,
            current_theme_idx,
            current_view: View::default(),
            instances,
            instances_view: InstancesView::default(),
            java_installs,
            account_store,
            accounts_view: AccountsView::default(),
            manifest,
            console_view: ConsoleView::default(),
            running_processes: Vec::new(),
            background_tasks: Vec::new(),
            toasts: Vec::new(),
            java_download: None,
            java_prompt: None,
            launch_after_java_download: None,
            settings_view: SettingsView::new(&ctx),
            http_client,
            modpack_updates: ModpackUpdateMap::new(),
            modpack_update_check,
            pending_manual_downloads: Vec::new(),
            show_manual_downloads_dialog: false,
            missing_mods: None,
            force_launch_requested: None,
            mod_redownload_toast: None,
            last_download_check: None,
            last_frame_time: None,
        }
    }

    fn launch_instance(&mut self, instance_id: &str, ctx: &egui::Context) {
        let Some(instance) = self.instances.iter().find(|i| i.id == instance_id) else {
            return;
        };

        // If instance has a custom java_path set, use it directly (trust user choice)
        if instance.java_path.is_some() {
            self.do_launch(instance_id, ctx);
            return;
        }

        // Determine required Java version
        let required = java::recommended_java_version(&instance.mc_version);

        // Check if we have a suitable Java installed (exact match or >= required)
        let has_suitable = self
            .java_installs
            .iter()
            .any(|j| j.major == required || j.major >= required);

        if has_suitable {
            self.do_launch(instance_id, ctx);
        } else {
            // Show Java prompt dialog
            // Try to infer component from major version
            let component = java::major_to_mojang_component(required).map(String::from);
            self.java_prompt = Some(JavaPromptState {
                instance_id: instance_id.to_string(),
                instance_name: instance.name.clone(),
                required_java: required,
                component,
            });
        }
    }

    fn do_launch(&mut self, instance_id: &str, ctx: &egui::Context) {
        // Pre-launch check: verify modpack mods are present
        if let Some(instance) = self.instances.iter().find(|i| i.id == instance_id) {
            if instance.modpack_origin.is_some() {
                if let Ok(mc_dir) = instance.minecraft_dir() {
                    let manifest_path = mc_dir.join(".modpack_mods.json");
                    if manifest_path.exists() {
                        if let Ok(data) = std::fs::read_to_string(&manifest_path) {
                            // Support both enriched (Vec<ModpackModEntry>) and legacy (Vec<String>) formats
                            let expected: Option<Vec<ModpackModEntry>> =
                                serde_json::from_str::<Vec<ModpackModEntry>>(&data)
                                    .ok()
                                    .or_else(|| {
                                        serde_json::from_str::<Vec<String>>(&data)
                                            .ok()
                                            .map(|names| {
                                                names
                                                    .into_iter()
                                                    .map(|name| ModpackModEntry {
                                                        name,
                                                        download_url: None,
                                                        display_name: None,
                                                        manual: false,
                                                        slug: None,
                                                        file_id: None,
                                                        website_url: None,
                                                    })
                                                    .collect()
                                            })
                                    });
                            if let Some(expected) = expected {
                                let mods_dir = mc_dir.join("mods");
                                let missing: Vec<ModpackModEntry> = expected
                                    .into_iter()
                                    .filter(|f| !mods_dir.join(&f.name).exists())
                                    .collect();
                                if !missing.is_empty() {
                                    self.missing_mods = Some(MissingModsState {
                                        instance_id: instance_id.to_string(),
                                        instance_name: instance.name.clone(),
                                        missing_files: missing,
                                    });
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.do_launch_inner(instance_id, ctx);
    }

    fn do_launch_inner(&mut self, instance_id: &str, ctx: &egui::Context) {
        let Some(instance) = self
            .instances
            .iter()
            .find(|i| i.id == instance_id)
            .cloned()
        else {
            return;
        };

        // If this instance already has a live process, just switch to its console tab
        if let Some(rp) = self.running_processes.iter().find(|rp| rp.instance_id == instance_id) {
            if rp.is_alive() {
                self.console_view.active_instance_id = Some(instance_id.to_string());
                self.current_view = View::Console;
                return;
            }
            // Exited — remove stale entry so we can reuse the slot
            let id = instance_id.to_string();
            self.running_processes.retain(|rp| rp.instance_id != id);
        }

        // Get active account
        let Some(account) = self.account_store.active_account().cloned() else {
            let progress = Arc::new(Mutex::new(LaunchProgress {
                message: String::new(),
                done: true,
                error: Some("No active account. Please add an account first.".to_string()),
            }));
            self.running_processes.push(RunningProcess {
                instance_id: instance_id.to_string(),
                instance_name: instance.name.clone(),
                progress,
                pending_process: Arc::new(Mutex::new(None)),
                process: None,
                auto_scroll: true,
            });
            self.console_view.active_instance_id = Some(instance_id.to_string());
            self.current_view = View::Console;
            return;
        };

        let java_installs = self.java_installs.clone();

        // Extract manifest versions
        let manifest_versions: Vec<(String, String)> = {
            let m = self.manifest.lock().unwrap();
            match &*m {
                ManifestState::Loaded(vm) => vm
                    .versions
                    .iter()
                    .map(|v| (v.id.clone(), v.url.clone()))
                    .collect(),
                _ => {
                    drop(m);
                    let progress = Arc::new(Mutex::new(LaunchProgress {
                        message: String::new(),
                        done: true,
                        error: Some("Version manifest not loaded yet.".to_string()),
                    }));
                    self.running_processes.push(RunningProcess {
                        instance_id: instance_id.to_string(),
                        instance_name: instance.name.clone(),
                        progress,
                        pending_process: Arc::new(Mutex::new(None)),
                        process: None,
                        auto_scroll: true,
                    });
                    self.console_view.active_instance_id = Some(instance_id.to_string());
                    self.current_view = View::Console;
                    return;
                }
            }
        };

        // Set up progress and create running process entry
        let progress = Arc::new(Mutex::new(LaunchProgress::new()));
        let console_process_slot: Arc<Mutex<Option<Arc<Mutex<ProcessState>>>>> =
            Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&console_process_slot);

        self.running_processes.push(RunningProcess {
            instance_id: instance_id.to_string(),
            instance_name: instance.name.clone(),
            progress: Arc::clone(&progress),
            pending_process: Arc::clone(&console_process_slot),
            process: None,
            auto_scroll: true,
        });
        self.console_view.active_instance_id = Some(instance_id.to_string());
        self.current_view = View::Console;

        // Spawn background thread
        let ctx_clone = ctx.clone();
        let progress_clone = Arc::clone(&progress);

        std::thread::spawn(move || {
            // Refresh auth token before launch (MC tokens expire in ~24h)
            let account = if !account.offline && !account.refresh_token.is_empty() {
                {
                    let mut p = progress_clone.lock().unwrap();
                    p.message = "Refreshing authentication...".to_string();
                }
                ctx_clone.request_repaint();
                match crate::core::account::refresh_account(&account) {
                    Ok(refreshed) => {
                        // Persist refreshed token to disk
                        let mut store = crate::core::account::AccountStore::load();
                        store.add_or_update(refreshed.clone());
                        let _ = store.save();
                        refreshed
                    }
                    Err(_) => account, // fall back to existing token
                }
            } else {
                account
            };

            match prepare_and_launch(
                &instance,
                &account,
                &java_installs,
                &manifest_versions,
                ctx_clone.clone(),
                progress_clone.clone(),
            ) {
                Ok(proc_state) => {
                    *slot_clone.lock().unwrap() = Some(proc_state);
                    let mut p = progress_clone.lock().unwrap();
                    p.done = true;
                }
                Err(e) => {
                    let mut p = progress_clone.lock().unwrap();
                    p.done = true;
                    p.error = Some(e.to_string());
                }
            }
            ctx_clone.request_repaint();
        });
    }

    fn run_modpack_install<F>(
        &mut self,
        console_label: String,
        initial_message: String,
        ctx: &egui::Context,
        install_fn: F,
    ) where
        F: FnOnce(
                Arc<Mutex<LaunchProgress>>,
                egui::Context,
                reqwest::blocking::Client,
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

        let install_id = format!("modpack-install-{}", uuid::Uuid::new_v4());
        let instance_slot: Arc<Mutex<Option<Instance>>> = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&instance_slot);

        self.background_tasks.push(BackgroundTask {
            id: install_id,
            label: console_label,
            progress: Arc::clone(&progress),
            instance_slot: Some(Arc::clone(&instance_slot)),
            update_slot: None,
            skipped_slot: None,
        });

        let ctx_clone = ctx.clone();
        let progress_clone = Arc::clone(&progress);
        let default_min_mem = self.config.default_min_memory_mb;
        let default_max_mem = self.config.default_max_memory_mb;
        let client = self.http_client.clone();

        std::thread::spawn(move || {
            let result = install_fn(
                Arc::clone(&progress_clone),
                ctx_clone.clone(),
                client,
                default_min_mem,
                default_max_mem,
            );

            match result {
                Ok(inst) => {
                    *slot_clone.lock().unwrap() = Some(inst);
                    let mut p = progress_clone.lock().unwrap();
                    p.message = "Modpack installed successfully!".to_string();
                    p.done = true;
                }
                Err(e) => {
                    let mut p = progress_clone.lock().unwrap();
                    p.done = true;
                    p.error = Some(e.to_string());
                }
            }
            ctx_clone.request_repaint();
        });

    }

    fn install_modpack(
        &mut self,
        project_id: String,
        title: String,
        icon_url: Option<String>,
        version_id: Option<String>,
        version_name: Option<String>,
        ctx: &egui::Context,
    ) {
        let display_title = title.clone();
        self.run_modpack_install(
            format!("Modpack: {}", title),
            format!("Installing modpack \"{}\"...", title),
            ctx,
            move |progress, ctx, client, min_mem, max_mem| {
                // Fetch version from Modrinth (specific or latest)
                {
                    let mut p = progress.lock().unwrap();
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

                // Download .mrpack to temp dir
                {
                    let mut p = progress.lock().unwrap();
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

                // Parse and install
                {
                    let mut p = progress.lock().unwrap();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let index = modrinth_modpack::parse_mrpack(&mrpack_path)?;

                {
                    let mut p = progress.lock().unwrap();
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
                    move |done, total, stage| {
                        let mut p = progress_for_files.lock().unwrap();
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
                instance.modpack_origin = Some(crate::core::instance::ModpackOrigin {
                    source: "modrinth".to_string(),
                    project_id: project_id.clone(),
                    version_id: version.id.clone(),
                    version_name: version_name.clone().unwrap_or_else(|| version.name.clone()),
                });
                instance.save_to_dir()?;

                Ok(instance)
            },
        );
    }

    fn install_cf_modpack(
        &mut self,
        mod_id: u64,
        title: String,
        icon_url: Option<String>,
        file_id: Option<u64>,
        file_name: Option<String>,
        ctx: &egui::Context,
    ) {
        let display_title = title.clone();
        let skipped: Arc<Mutex<Vec<curseforge_modpack::SkippedMod>>> =
            Arc::new(Mutex::new(Vec::new()));
        let skipped_clone = Arc::clone(&skipped);

        self.run_modpack_install(
            format!("Modpack: {}", title),
            format!("Installing modpack \"{}\"...", title),
            ctx,
            move |progress, ctx, client, min_mem, max_mem| {
                {
                    let mut p = progress.lock().unwrap();
                    p.message = format!("Fetching modpack info for \"{}\"...", display_title);
                }
                ctx.request_repaint();

                let files = crate::core::curseforge::get_cf_mod_files(mod_id, "", None)?;
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
                        let mut p = progress.lock().unwrap();
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
                    crate::core::curseforge_modpack::wait_for_cf_manual_download(
                        mod_id, file, &temp_dir, &progress, &ctx,
                    )?
                };

                // Parse and install
                {
                    let mut p = progress.lock().unwrap();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let manifest = curseforge_modpack::parse_cf_modpack(&zip_path)?;

                {
                    let mut p = progress.lock().unwrap();
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
                    move |done, total, stage| {
                        let mut p = progress_for_files.lock().unwrap();
                        p.message = if total > 0 {
                            format!("{stage} ({done}/{total})")
                        } else {
                            stage.to_string()
                        };
                        drop(p);
                        ctx_for_files.request_repaint();
                    },
                )?;

                // Store skipped mods for the main thread to handle
                *skipped_clone.lock().unwrap() = skipped_mods;

                let _ = std::fs::remove_dir_all(&temp_dir);

                let mut instance = instance;
                instance.min_memory_mb = min_mem;
                instance.max_memory_mb = max_mem;
                instance.icon = icon_url;
                instance.modpack_origin = Some(crate::core::instance::ModpackOrigin {
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

        // Attach skipped_slot to the just-created background task
        if let Some(task) = self.background_tasks.last_mut() {
            task.skipped_slot = Some(skipped);
        }
    }

    fn import_local_mrpack(&mut self, path: std::path::PathBuf, ctx: &egui::Context) {
use crate::core::modrinth_modpack;

        self.run_modpack_install(
            "Modpack Import".to_string(),
            "Importing Modrinth modpack...".to_string(),
            ctx,
            move |progress, ctx, client, min_mem, max_mem| {
                {
                    let mut p = progress.lock().unwrap();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let index = modrinth_modpack::parse_mrpack(&path)?;

                {
                    let mut p = progress.lock().unwrap();
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
                    move |done, total, stage| {
                        let mut p = progress_for_files.lock().unwrap();
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
    }

    fn poll_background_tasks(&mut self, ctx: &egui::Context) {
        // Poll for available Java versions from Adoptium
        self.settings_view.poll();

        // Promote pending processes in running_processes
        for rp in &mut self.running_processes {
            if rp.process.is_none() {
                let mut guard = rp.pending_process.lock().unwrap();
                if let Some(proc_state) = guard.take() {
                    rp.process = Some(proc_state);
                }
            }
        }

        // Expire old toasts (5s for success, 8s for errors)
        self.toasts.retain(|t| {
            let max_age = if t.is_error { 8.0 } else { 5.0 };
            t.created_at.elapsed().as_secs_f32() < max_age
        });

        // Poll background tasks (modpack installs, updates)
        let mut completed_indices = Vec::new();
        for (idx, task) in self.background_tasks.iter().enumerate() {
            if !task.is_done() {
                continue;
            }

            // Check for errors first
            if let Some(err) = task.error() {
                self.toasts
                    .push(Toast::error(format!("{}: {}", task.label, err)));
                completed_indices.push(idx);
                continue;
            }

            // Check for completed modpack install (instance slot)
            if let Some(slot) = &task.instance_slot {
                if let Some(inst) = slot.lock().unwrap().take() {
                    // Handle skipped (distribution-blocked) mods
                    if let Some(skipped_slot) = &task.skipped_slot {
                        let skipped = skipped_slot.lock().unwrap();
                        if !skipped.is_empty() {
                            let mods_dir = inst.minecraft_dir().ok().map(|d| d.join("mods"));
                            for sm in skipped.iter() {
                                let url = crate::core::curseforge::curseforge_file_download_url(
                                    &sm.slug,
                                    sm.file_id,
                                    sm.website_url.as_deref(),
                                );
                                if let Some(ref target) = mods_dir {
                                    self.pending_manual_downloads.push(PendingManualDownload {
                                        file_name: sm.file_name.clone(),
                                        display_name: sm.display_name.clone(),
                                        target_dir: target.clone(),
                                        download_url: url,
                                    });
                                }
                            }
                            self.toasts.push(Toast::error(format!(
                                "{} mod(s) need manual download — watching your Downloads folder",
                                skipped.len()
                            )));
                            self.show_manual_downloads_dialog = true;
                        }
                    }

                    self.instances.push(inst);
                    // Open edit dialog so user can review/change Java selection
                    if let Some(last) = self.instances.last() {
                        self.instances_view.editing = Some(last.id.clone());
                    }
                    // Navigate back to instances list (closes modpack browser / add-instance view)
                    self.instances_view.show_add_instance = false;
                    self.current_view = View::Instances;

                    // Trigger modpack update recheck so a newly-installed
                    // modpack gets an update badge immediately if applicable
                    self.instances_view.recheck_modpack_updates = true;
                }
            }

            // Check for completed modpack update (in-place)
            if let Some(slot) = &task.update_slot {
                if let Some((instance_id, new_origin, meta)) = slot.lock().unwrap().take() {
                    let mods_dir = self
                        .instances
                        .iter()
                        .find(|i| i.id == instance_id)
                        .and_then(|i| i.minecraft_dir().ok())
                        .map(|d| d.join("mods"));

                    if let Some(instance) =
                        self.instances.iter_mut().find(|i| i.id == instance_id)
                    {
                        instance.modpack_origin = Some(new_origin);
                        instance.mc_version = meta.mc_version;
                        instance.loader = meta.loader;
                        instance.loader_version = meta.loader_version;
                        if let Err(e) = instance.save_to_dir() {
                            log::warn!("Failed to save updated instance {instance_id}: {e}");
                        }
                    }
                    self.modpack_updates.remove(&instance_id);
                    self.instances_view.modpack_updates.remove(&instance_id);

                    // Handle skipped (distribution-blocked) mods from CF update
                    if let Some(skipped_slot) = &task.skipped_slot {
                        let skipped = skipped_slot.lock().unwrap();
                        if !skipped.is_empty() {
                            if let Some(ref target) = mods_dir {
                                for sm in skipped.iter() {
                                    let url =
                                        crate::core::curseforge::curseforge_file_download_url(
                                            &sm.slug,
                                            sm.file_id,
                                            sm.website_url.as_deref(),
                                        );
                                    self.pending_manual_downloads.push(PendingManualDownload {
                                        file_name: sm.file_name.clone(),
                                        display_name: sm.display_name.clone(),
                                        target_dir: target.clone(),
                                        download_url: url,
                                    });
                                }
                            }
                            self.toasts.push(Toast::error(format!(
                                "{} mod(s) need manual download — watching your Downloads folder",
                                skipped.len()
                            )));
                            self.show_manual_downloads_dialog = true;
                        }
                    }

                    self.toasts
                        .push(Toast::success(format!("{} complete", task.label)));
                    self.current_view = View::Instances;

                    // Re-check modpack updates — if the user used "Change Version"
                    // to install an older version, the badge should reappear.
                    self.instances_view.recheck_modpack_updates = true;
                }
            }

            completed_indices.push(idx);
        }

        // Remove completed tasks (reverse order to preserve indices)
        for idx in completed_indices.into_iter().rev() {
            self.background_tasks.remove(idx);
        }

        // Poll for completed Java downloads
        if let Some(ref state) = self.java_download {
            let mut s = state.lock().unwrap();
            if s.done {
                if let Some(Ok(install)) = s.result.take() {
                    self.java_installs.push(install);
                    self.java_installs.sort_by(|a, b| b.major.cmp(&a.major));
                }
                drop(s);
                self.java_download = None;

                // If we were downloading Java for a pending launch, trigger it now
                if let Some(inst_id) = self.launch_after_java_download.take() {
                    self.java_prompt = None;
                    self.do_launch(&inst_id, ctx);
                }
            }
        }

        if let Some(updates) = self
            .modpack_update_check
            .as_ref()
            .and_then(|slot| slot.lock().unwrap().take())
        {
            self.modpack_updates = updates.clone();
            self.instances_view.modpack_updates = updates;
            self.modpack_update_check = None;
        }

        // Check for background mod re-download completion
        let redownload_msg = self
            .mod_redownload_toast
            .as_ref()
            .and_then(|slot| slot.try_lock().ok().and_then(|mut g| g.take()));
        if let Some(msg) = redownload_msg {
            self.toasts.push(Toast::success(msg));
            self.mod_redownload_toast = None;
        }

        // Poll Downloads directory for pending manual downloads (blocked CF mods)
        if !self.pending_manual_downloads.is_empty() {
            let now = Instant::now();
            let should_check = self
                .last_download_check
                .map_or(true, |t| now.duration_since(t) >= Duration::from_secs(2));

            if should_check {
                self.last_download_check = Some(now);

                if let Some(downloads_dir) = directories::UserDirs::new()
                    .and_then(|u| u.download_dir().map(|d| d.to_path_buf()))
                {
                    let mut found_indices = Vec::new();
                    for (i, pending) in self.pending_manual_downloads.iter().enumerate() {
                        let src = downloads_dir.join(&pending.file_name);
                        if src.exists() {
                            if let Err(e) = std::fs::create_dir_all(&pending.target_dir) {
                                log::warn!("Failed to create target dir: {e}");
                                continue;
                            }
                            let dst = pending.target_dir.join(&pending.file_name);
                            // Try rename first, fall back to copy+delete for cross-device moves
                            let moved = std::fs::rename(&src, &dst).or_else(|_| {
                                std::fs::copy(&src, &dst)
                                    .and_then(|_| std::fs::remove_file(&src))
                            });
                            match moved {
                                Ok(_) => {
                                    // Cache the manually-downloaded file for future installs.
                                    crate::core::mod_cache::cache_file(
                                        &pending.file_name,
                                        &dst,
                                    );
                                    self.toasts.push(Toast::success(format!(
                                        "Auto-installed \"{}\"",
                                        pending.display_name
                                    )));
                                    found_indices.push(i);
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Failed to move {} to mods dir: {e}",
                                        pending.file_name
                                    );
                                }
                            }
                        }
                    }
                    let moved_any = !found_indices.is_empty();
                    for i in found_indices.into_iter().rev() {
                        self.pending_manual_downloads.remove(i);
                    }
                    if moved_any && self.pending_manual_downloads.is_empty() {
                        self.toasts
                            .push(Toast::success("All blocked mods installed!".to_string()));
                        self.last_download_check = None;
                    }
                }
            }

            // Keep polling every 2 seconds while downloads are pending
            ctx.request_repaint_after(Duration::from_secs(2));
        }
    }

    fn handle_view_requests(&mut self, ctx: &egui::Context) {
        // Drain pending toasts from instances view
        self.toasts
            .append(&mut self.instances_view.pending_toasts);

        // Handle launch requests from instances view
        if let Some(id) = self.instances_view.launch_requested.take() {
            if let Some(instance) = self.instances.iter_mut().find(|i| i.id == id) {
                instance.last_played = Some(crate::ui::helpers::format_human_timestamp(SystemTime::now()));
                if let Err(e) = instance.save_to_dir() {
                    log::warn!("Failed to save instance after updating last_played: {e}");
                }
            }
            self.launch_instance(&id, ctx);
        }

        // Handle "Launch Anyway" from missing-mods dialog (bypasses mod check)
        if let Some(id) = self.force_launch_requested.take() {
            self.do_launch_inner(&id, ctx);
        }

        // Handle modpack install requests from instances view
        if let Some(req) = self.instances_view.modpack_browser.install_requested.take() {
            self.install_modpack(
                req.project_id,
                req.title,
                req.icon_url,
                req.version_id,
                req.version_name,
                ctx,
            );
        }

        // Handle CurseForge modpack install requests from instances view
        if let Some(req) = self
            .instances_view
            .modpack_browser
            .cf_install_requested
            .take()
        {
            self.install_cf_modpack(
                req.mod_id,
                req.title,
                req.icon_url,
                req.file_id,
                req.file_name,
                ctx,
            );
        }

        // Handle local modpack imports from instances view
        if let Some(path) = self.instances_view.local_mrpack_import.take() {
            self.import_local_mrpack(path, ctx);
        }
        if let Some(path) = self.instances_view.local_cf_modpack_import.take() {
            self.import_local_cf_modpack(path, ctx);
        }

        if let Some(instance_id) = self.instances_view.update_modpack_requested.take() {
            if let Some(instance) = self.instances.iter().find(|i| i.id == instance_id) {
                if let Some(origin) = &instance.modpack_origin {
                    let name = instance.name.clone();
                    let source = origin.source.clone();
                    let project_id = origin.project_id.clone();
                    let version_id = origin.version_id.clone();
                    self.instances_view.open_modpack_version_picker(
                        &instance_id,
                        &name,
                        &source,
                        &project_id,
                        &version_id,
                        true,
                        ctx,
                    );
                }
            }
        }

        if let Some((instance_id, update_info)) = self.instances_view.change_modpack_version.take() {
            if let Some(instance) = self.instances.iter().find(|i| i.id == instance_id) {
                let title = instance.name.clone();
                match instance.minecraft_dir() {
                    Ok(minecraft_dir) => {
                        let inst_id = instance_id.clone();
                        self.run_modpack_update(title, inst_id, minecraft_dir, update_info, ctx);
                    }
                    Err(e) => {
                        log::warn!("Failed to get minecraft dir for {instance_id}: {e}");
                    }
                }
            }
        }

        if self.instances_view.recheck_modpack_updates {
            self.instances_view.recheck_modpack_updates = false;
            let origins: Vec<(String, String, crate::core::instance::ModpackOrigin)> = self
                .instances
                .iter()
                .filter_map(|inst| {
                    inst.modpack_origin
                        .as_ref()
                        .map(|o| (inst.id.clone(), inst.mc_version.clone(), o.clone()))
                })
                .collect();
            if !origins.is_empty() {
                let slot: Arc<Mutex<Option<ModpackUpdateMap>>> = Arc::new(Mutex::new(None));
                let slot_clone = Arc::clone(&slot);
                let ctx_clone = ctx.clone();
                std::thread::spawn(move || {
                    let results = crate::core::update::check_modpack_updates(&origins);
                    *slot_clone.lock().unwrap() = Some(results);
                    ctx_clone.request_repaint();
                });
                self.modpack_update_check = Some(slot);
            }
        }

        // Handle mod origin updates from detail view install threads
        if let Some(ref mut detail) = self.instances_view.detail_view {
            let mut needs_save = false;
            let inst_id = detail.instance_id().to_string();

            if !detail.mod_origin_updates.is_empty() {
                let origins: Vec<_> = detail.mod_origin_updates.drain(..).collect();
                if let Some(instance) = self.instances.iter_mut().find(|i| i.id == inst_id) {
                    for origin in origins {
                        instance.upsert_mod_origin(origin);
                    }
                    needs_save = true;
                }
            }

            if detail.reconcile_origins_requested {
                detail.reconcile_origins_requested = false;
                let filenames = detail.installed_filenames();
                if let Some(instance) = self.instances.iter_mut().find(|i| i.id == inst_id) {
                    let before = instance.mod_origins.len();
                    instance.reconcile_mod_origins(&filenames);
                    if instance.mod_origins.len() != before {
                        needs_save = true;
                    }
                }
            }

            if needs_save
                && let Some(instance) = self.instances.iter().find(|i| i.id == inst_id)
                && let Err(e) = instance.save_to_dir()
            {
                log::warn!("Failed to save mod origins for {inst_id}: {e}");
            }
        }
    }

    fn show_java_prompt(&mut self, ctx: &egui::Context, theme: &Option<Theme>) {
        if self.java_prompt.is_none() {
            return;
        }

        // Extract data from java_prompt to avoid borrow issues in the closure
        let prompt_instance_id = self.java_prompt.as_ref().unwrap().instance_id.clone();
        let prompt_instance_name = self.java_prompt.as_ref().unwrap().instance_name.clone();
        let prompt_required_java = self.java_prompt.as_ref().unwrap().required_java;
        let prompt_java_installs: Vec<(String, std::path::PathBuf, u32)> = self
            .java_installs
            .iter()
            .map(|j| {
                let label = if j.managed {
                    format!("Java {} — {} (Lurch)", j.major, j.version)
                } else {
                    format!("Java {} — {} (system)", j.major, j.version)
                };
                (label, j.path.clone(), j.major)
            })
            .collect();
        let is_downloading = self
            .java_download
            .as_ref()
            .is_some_and(|s| !s.lock().unwrap().done);
        let download_message = self
            .java_download
            .as_ref()
            .map(|s| s.lock().unwrap().message.clone());
        let _has_launch_pending = self.launch_after_java_download.is_some();

        let mut action: Option<JavaPromptAction> = None;

        egui::Window::new("Java Required")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(t) = theme.as_ref() {
                    ui.label(t.title(&format!(
                        "\"{}\" needs Java {}",
                        prompt_instance_name, prompt_required_java
                    )));
                    ui.add_space(4.0);
                    ui.label(t.subtext(
                        "No suitable Java installation was found. Choose an option below:",
                    ));
                } else {
                    ui.strong(format!(
                        "\"{}\" needs Java {}",
                        prompt_instance_name, prompt_required_java
                    ));
                    ui.add_space(4.0);
                    ui.label("No suitable Java installation was found. Choose an option below:");
                }

                ui.add_space(8.0);

                // Download option
                if is_downloading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        if let Some(msg) = &download_message {
                            ui.label(msg);
                        }
                    });
                } else {
                    let download_label =
                        format!("⬇ Download Java {} and Launch", prompt_required_java);
                    let download_clicked = if let Some(t) = theme.as_ref() {
                        ui.add(t.accent_button(&download_label)).clicked()
                    } else {
                        ui.button(&download_label).clicked()
                    };
                    if download_clicked {
                        action = Some(JavaPromptAction::Download(
                            prompt_required_java,
                            prompt_instance_id.clone(),
                        ));
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Existing installations
                if !prompt_java_installs.is_empty() {
                    if let Some(t) = theme.as_ref() {
                        ui.label(t.subtext("Or use an existing installation:"));
                    } else {
                        ui.weak("Or use an existing installation:");
                    }
                    ui.add_space(4.0);
                    for (label, path, _major) in &prompt_java_installs {
                        ui.horizontal(|ui| {
                            let use_clicked = if let Some(t) = theme.as_ref() {
                                ui.add(t.accent_button("Use")).clicked()
                            } else {
                                ui.button("Use").clicked()
                            };
                            ui.label(label);
                            if use_clicked {
                                action = Some(JavaPromptAction::UseExisting(
                                    path.clone(),
                                    prompt_instance_id.clone(),
                                ));
                            }
                        });
                    }
                }

                ui.add_space(8.0);
                if ui.button("Cancel").clicked() {
                    action = Some(JavaPromptAction::Cancel);
                }
            });

        match action {
            Some(JavaPromptAction::Download(version, inst_id)) => {
                let component = self.java_prompt.as_ref().and_then(|p| p.component.clone());

                let state = Arc::new(Mutex::new(JavaDownloadState {
                    version,
                    message: format!("Starting Java {} download...", version),
                    done: false,
                    result: None,
                }));
                self.java_download = Some(Arc::clone(&state));
                self.launch_after_java_download = Some(inst_id);

                let ctx2 = ctx.clone();
                let client = self.http_client.clone();
                std::thread::spawn(move || {
                    let state_for_cb = Arc::clone(&state);
                    let ctx_for_cb = ctx2.clone();
                    let progress_cb = move |msg: &str| {
                        state_for_cb.lock().unwrap().message = msg.to_string();
                        ctx_for_cb.request_repaint();
                    };

                    let mojang_result = component.as_deref().and_then(|comp| {
                        java::download_mojang_java(&client, comp, &progress_cb).ok()
                    });

                    let result = match mojang_result {
                        Some(inst) => Ok(inst),
                        None => java::download_java(&client, version, &progress_cb)
                            .map_err(|e| e.to_string()),
                    };
                    let mut s = state.lock().unwrap();
                    s.result = Some(result);
                    s.done = true;
                    drop(s);
                    ctx2.request_repaint();
                });
            }
            Some(JavaPromptAction::UseExisting(path, inst_id)) => {
                if let Some(inst) = self.instances.iter_mut().find(|i| i.id == inst_id) {
                    inst.java_path = Some(path);
                    let _ = inst.save_to_dir();
                }
                self.java_prompt = None;
                self.do_launch(&inst_id, ctx);
            }
            Some(JavaPromptAction::Cancel) => {
                self.java_prompt = None;
                self.launch_after_java_download = None;
            }
            None => {}
        }
    }

    fn show_missing_mods_dialog(&mut self, ctx: &egui::Context, theme: &Option<Theme>) {
        if self.missing_mods.is_none() {
            return;
        }

        let state = self.missing_mods.as_ref().unwrap();
        let instance_name = state.instance_name.clone();
        let instance_id = state.instance_id.clone();
        let missing_files = state.missing_files.clone();

        // Check if any missing mods are downloadable (auto or manual)
        let any_downloadable = missing_files
            .iter()
            .any(|m| m.download_url.is_some() || m.manual);

        let mut launch_anyway = false;
        let mut cancel = false;
        let mut download = false;

        egui::Window::new("Missing Mods")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(t) = theme.as_ref() {
                    ui.label(t.title(&format!(
                        "\"{}\" is missing {} mod file{}",
                        instance_name,
                        missing_files.len(),
                        if missing_files.len() == 1 { "" } else { "s" },
                    )));
                    ui.add_space(4.0);
                    ui.label(t.subtext(
                        "The following mods from the modpack were not found. They may have been \
                         accidentally deleted.",
                    ));
                } else {
                    ui.strong(format!(
                        "\"{}\" is missing {} mod file{}",
                        instance_name,
                        missing_files.len(),
                        if missing_files.len() == 1 { "" } else { "s" },
                    ));
                    ui.add_space(4.0);
                    ui.label(
                        "The following mods from the modpack were not found. They may have been \
                         accidentally deleted.",
                    );
                }

                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for entry in &missing_files {
                            let label =
                                entry.display_name.as_deref().unwrap_or(&entry.name);
                            if let Some(t) = theme.as_ref() {
                                ui.label(t.subtext(&format!("  • {label}")));
                            } else {
                                ui.label(format!("  • {label}"));
                            }
                        }
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if any_downloadable {
                        let dl_clicked = if let Some(t) = theme.as_ref() {
                            ui.add(t.accent_button("Download Missing")).clicked()
                        } else {
                            ui.button("Download Missing").clicked()
                        };
                        if dl_clicked {
                            download = true;
                        }
                    }

                    let launch_clicked = if let Some(t) = theme.as_ref() {
                        ui.add(t.danger_button("Launch Anyway")).clicked()
                    } else {
                        ui.button("Launch Anyway").clicked()
                    };
                    if launch_clicked {
                        launch_anyway = true;
                    }

                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if download {
            self.missing_mods = None;
            if let Some(inst) = self.instances.iter().find(|i| i.id == instance_id) {
                if let Ok(mc_dir) = inst.minecraft_dir() {
                    let mods_dir = mc_dir.join("mods");

                    // Separate auto-downloadable from manual (distribution-blocked)
                    let (auto_mods, manual_mods): (Vec<_>, Vec<_>) = missing_files
                        .into_iter()
                        .partition(|m| !m.manual && m.download_url.is_some());

                    // Handle manual/blocked mods — create PendingManualDownload entries
                    for m in manual_mods {
                        let url = if let (Some(slug), Some(fid)) =
                            (m.slug.as_deref(), m.file_id)
                        {
                            crate::core::curseforge::curseforge_file_download_url(
                                slug,
                                fid,
                                m.website_url.as_deref(),
                            )
                        } else if let Some(u) = m.download_url {
                            u
                        } else {
                            continue;
                        };
                        self.pending_manual_downloads.push(PendingManualDownload {
                            file_name: m.name.clone(),
                            display_name: m.display_name.unwrap_or_else(|| m.name),
                            target_dir: mods_dir.clone(),
                            download_url: url,
                        });
                    }
                    if !self.pending_manual_downloads.is_empty() {
                        self.show_manual_downloads_dialog = true;
                    }

                    // Handle auto-downloadable mods in a background thread
                    if !auto_mods.is_empty() {
                        let slot: Arc<Mutex<Option<String>>> =
                            Arc::new(Mutex::new(None));
                        self.mod_redownload_toast = Some(slot.clone());
                        let client = self.http_client.clone();
                        let ctx2 = ctx.clone();
                        std::thread::spawn(move || {
                            let mut success = 0usize;
                            let mut failed = 0usize;
                            for m in &auto_mods {
                                if let Some(url) = &m.download_url {
                                    match client
                                        .get(url)
                                        .send()
                                        .and_then(|r| r.error_for_status())
                                        .and_then(|r| r.bytes())
                                    {
                                        Ok(bytes) => {
                                            let dest = mods_dir.join(&m.name);
                                            if std::fs::write(&dest, &bytes).is_ok() {
                                                crate::core::mod_cache::cache_file(
                                                    &m.name, &dest,
                                                );
                                                success += 1;
                                            } else {
                                                failed += 1;
                                            }
                                        }
                                        Err(e) => {
                                            log::warn!(
                                                "Failed to download {}: {e}",
                                                m.name
                                            );
                                            failed += 1;
                                        }
                                    }
                                }
                            }
                            let msg = if failed == 0 {
                                format!(
                                    "Downloaded {success} missing mod{}",
                                    if success == 1 { "" } else { "s" }
                                )
                            } else {
                                format!(
                                    "Downloaded {success} mod{}, {failed} failed",
                                    if success == 1 { "" } else { "s" }
                                )
                            };
                            *slot.lock().unwrap() = Some(msg);
                            ctx2.request_repaint();
                        });
                    }
                }
            }
        } else if launch_anyway {
            self.missing_mods = None;
            self.force_launch_requested = Some(instance_id);
        } else if cancel {
            self.missing_mods = None;
        }
    }

    fn show_manual_downloads_dialog(&mut self, ctx: &egui::Context, theme: &Option<Theme>) {
        if !self.show_manual_downloads_dialog || self.pending_manual_downloads.is_empty() {
            self.show_manual_downloads_dialog = false;
            return;
        }

        let count = self.pending_manual_downloads.len();
        let mut dismiss = false;
        let mut open_all = false;
        let mut open_indices: Vec<usize> = Vec::new();

        egui::Window::new(format!("{} Manual Downloads Required", count))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(t) = theme.as_ref() {
                    ui.label(t.subtext(
                        "These mods block third-party distribution. Download them from CurseForge and Lurch will auto-install them from your Downloads folder.",
                    ));
                } else {
                    ui.label(
                        "These mods block third-party distribution. Download them from CurseForge and Lurch will auto-install them from your Downloads folder.",
                    );
                }

                ui.add_space(8.0);

                // "Open All Download Pages" accent button
                let open_all_clicked = if let Some(t) = theme.as_ref() {
                    ui.add(t.accent_button(&format!(
                        "{} Open All Download Pages",
                        egui_phosphor::regular::GLOBE
                    )))
                    .clicked()
                } else {
                    ui.button("Open All Download Pages").clicked()
                };
                if open_all_clicked {
                    open_all = true;
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // List each mod with individual "Open Page" button
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (i, pending) in self.pending_manual_downloads.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let clicked = if let Some(t) = theme.as_ref() {
                                    ui.add(t.ghost_button(egui_phosphor::regular::GLOBE))
                                        .on_hover_text("Open Download Page")
                                        .clicked()
                                } else {
                                    ui.button(egui_phosphor::regular::GLOBE).clicked()
                                };
                                if clicked {
                                    open_indices.push(i);
                                }

                                ui.vertical(|ui| {
                                    if let Some(t) = theme.as_ref() {
                                        ui.label(t.title(&pending.display_name));
                                        ui.label(t.subtext(&pending.file_name));
                                    } else {
                                        ui.strong(&pending.display_name);
                                        ui.weak(&pending.file_name);
                                    }
                                });
                            });
                            if i < count - 1 {
                                ui.separator();
                            }
                        }
                    });

                ui.add_space(8.0);

                // Dismiss button
                let dismiss_clicked = if let Some(t) = theme.as_ref() {
                    ui.add(t.ghost_button("Dismiss")).clicked()
                } else {
                    ui.button("Dismiss").clicked()
                };
                if dismiss_clicked {
                    dismiss = true;
                }
            });

        // Handle actions after the closure
        if open_all {
            for pending in &self.pending_manual_downloads {
                let _ = open::that(&pending.download_url);
            }
        }
        for i in open_indices {
            if let Some(pending) = self.pending_manual_downloads.get(i) {
                let _ = open::that(&pending.download_url);
            }
        }
        if dismiss {
            self.show_manual_downloads_dialog = false;
        }
    }

    fn import_local_cf_modpack(&mut self, path: std::path::PathBuf, ctx: &egui::Context) {
        use crate::core::curseforge_modpack;

        let skipped: Arc<Mutex<Vec<curseforge_modpack::SkippedMod>>> =
            Arc::new(Mutex::new(Vec::new()));
        let skipped_clone = Arc::clone(&skipped);

        self.run_modpack_install(
            "Modpack Import".to_string(),
            "Importing CurseForge modpack...".to_string(),
            ctx,
            move |progress, ctx, _client, min_mem, max_mem| {
                {
                    let mut p = progress.lock().unwrap();
                    p.message = "Parsing modpack...".to_string();
                }
                ctx.request_repaint();

                let manifest = curseforge_modpack::parse_cf_modpack(&path)?;

                {
                    let mut p = progress.lock().unwrap();
                    p.message = format!("Creating instance \"{}\"...", manifest.name);
                }
                ctx.request_repaint();

                let instance = curseforge_modpack::create_instance_from_cf_modpack(&manifest)?;
                let minecraft_dir = instance.minecraft_dir()?;

                let progress_for_files = Arc::clone(&progress);
                let ctx_for_files = ctx.clone();
                let skipped_mods = curseforge_modpack::install_cf_modpack_files(
                    &manifest,
                    &path,
                    &minecraft_dir,
                    move |done, total, stage| {
                        let mut p = progress_for_files.lock().unwrap();
                        p.message = if total > 0 {
                            format!("{stage} ({done}/{total})")
                        } else {
                            stage.to_string()
                        };
                        drop(p);
                        ctx_for_files.request_repaint();
                    },
                )?;

                *skipped_clone.lock().unwrap() = skipped_mods;

                let mut instance = instance;
                instance.min_memory_mb = min_mem;
                instance.max_memory_mb = max_mem;
                instance.save_to_dir()?;

                Ok(instance)
            },
        );

        // Attach skipped_slot to the just-created background task
        if let Some(task) = self.background_tasks.last_mut() {
            task.skipped_slot = Some(skipped);
        }
    }

    fn run_modpack_update(
        &mut self,
        title: String,
        instance_id: String,
        minecraft_dir: std::path::PathBuf,
        update_info: crate::core::update::ModpackUpdateInfo,
        ctx: &egui::Context,
    ) {
        let progress = Arc::new(Mutex::new(LaunchProgress {
            message: format!("Updating modpack \"{}\"...", title),
            done: false,
            error: None,
        }));

        let update_tab_id = format!("modpack-update-{}", instance_id);

        // Remove any existing task for this update
        self.background_tasks.retain(|t| t.id != update_tab_id);

        let update_slot: PendingModpackUpdate = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&update_slot);

        let skipped: Arc<Mutex<Vec<curseforge_modpack::SkippedMod>>> =
            Arc::new(Mutex::new(Vec::new()));
        let skipped_for_thread = Arc::clone(&skipped);

        self.background_tasks.push(BackgroundTask {
            id: update_tab_id,
            label: format!("Update: {}", title),
            progress: Arc::clone(&progress),
            instance_slot: None,
            update_slot: Some(Arc::clone(&update_slot)),
            skipped_slot: Some(Arc::clone(&skipped)),
        });

        let ctx_clone = ctx.clone();
        let progress_clone = Arc::clone(&progress);
        let client = self.http_client.clone();

        let source = update_info.source.clone();
        let project_id = update_info.project_id.clone();
        let version_id = update_info.latest_version_id.clone();
        let version_name = update_info.latest_version_name.clone();

        std::thread::spawn(move || {
            let result = match source.as_str() {
                "modrinth" => crate::core::modrinth_modpack::update_modrinth_modpack(
                    &client,
                    &project_id,
                    &version_id,
                    &minecraft_dir,
                    &progress_clone,
                    &ctx_clone,
                ),
                "curseforge" => crate::core::curseforge_modpack::update_curseforge_modpack(
                    &project_id,
                    &version_id,
                    &minecraft_dir,
                    &progress_clone,
                    &ctx_clone,
                    &skipped_for_thread,
                ),
                other => Err(anyhow::anyhow!("Unknown modpack source: {other}")),
            };

            match result {
                Ok(meta) => {
                    let new_origin = crate::core::instance::ModpackOrigin {
                        source: source.clone(),
                        project_id,
                        version_id,
                        version_name,
                    };
                    *slot_clone.lock().unwrap() = Some((instance_id, new_origin, meta));
                    let mut p = progress_clone.lock().unwrap();
                    p.message = "Modpack updated successfully!".to_string();
                    p.done = true;
                }
                Err(e) => {
                    let mut p = progress_clone.lock().unwrap();
                    p.done = true;
                    p.error = Some(e.to_string());
                }
            }
            ctx_clone.request_repaint();
        });
    }
}

enum JavaPromptAction {
    Download(u32, String),
    UseExisting(std::path::PathBuf, String),
    Cancel,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Frame rate cap: prevent rendering faster than ~120fps.
        // During normal operation egui only repaints on events (well below this).
        // During window resize, winit sends rapid events that can overwhelm the
        // compositor (especially KWin on Wayland), causing jank or crashes.
        {
            const MIN_FRAME_INTERVAL: std::time::Duration =
                std::time::Duration::from_micros(8333); // ~120 fps
            let now = Instant::now();
            if let Some(last) = self.last_frame_time {
                let elapsed = now.duration_since(last);
                if elapsed < MIN_FRAME_INTERVAL {
                    std::thread::sleep(MIN_FRAME_INTERVAL - elapsed);
                }
            }
            self.last_frame_time = Some(Instant::now());
        }

        let ctx = ui.ctx().clone();

        self.poll_background_tasks(&ctx);

        // Apply theme (visuals + spacing)
        if let Some(theme) = self.themes.get(self.current_theme_idx) {
            theme.apply(&ctx);
        }

        // Get theme for styling helpers (cheap clone — just a HashMap)
        let theme = self.themes.get(self.current_theme_idx).cloned();

        // Global keyboard shortcuts
        let input = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::N) && i.modifiers.command,
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Num1) && i.modifiers.command,
                i.key_pressed(egui::Key::Num2) && i.modifiers.command,
                i.key_pressed(egui::Key::Num3) && i.modifiers.command,
                i.key_pressed(egui::Key::Num4) && i.modifiers.command,
                i.key_pressed(egui::Key::Num5) && i.modifiers.command,
            )
        });
        let (ctrl_n, escape, ctrl_1, ctrl_2, ctrl_3, ctrl_4, _ctrl_5) = input;

        if ctrl_n {
            self.current_view = View::Instances;
            self.instances_view.show_add_instance = true;
        } else if escape {
            if self.instances_view.show_add_instance {
                self.instances_view.show_add_instance = false;
            } else if self.java_prompt.is_some() {
                self.java_prompt = None;
                self.launch_after_java_download = None;
            } else if self.missing_mods.is_some() {
                self.missing_mods = None;
            } else if self.instances_view.has_detail_view() {
                self.instances_view.close_detail_view();
            }
        } else if ctrl_1 {
            self.current_view = View::Instances;
        } else if ctrl_2 {
            self.current_view = View::Settings;
        } else if ctrl_3 {
            self.current_view = View::Accounts;
        } else if ctrl_4 {
            self.current_view = View::Console;
        }

        // Top bar
        egui::Panel::top("top_bar")
            .frame(
                theme
                    .as_ref()
                    .map_or(egui::Frame::NONE, |t| t.topbar_frame()),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let accent = theme
                        .as_ref()
                        .map_or(egui::Color32::WHITE, |t| t.color("accent"));
                    ui.label(
                        egui::RichText::new(egui_phosphor::regular::CUBE)
                            .size(20.0)
                            .color(accent),
                    );
                    ui.label(
                        egui::RichText::new("Lurch")
                            .size(18.0)
                            .color(accent)
                            .strong(),
                    );
                    // Show active account in top bar (clickable → Accounts view)
                    if let Some(acc) = self.account_store.active_account() {
                        let uuid = acc.uuid.clone();
                        let username = acc.username.clone();
                        let is_offline = acc.offline;
                        let mut clicked = false;
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let name_resp = ui.add(
                                egui::Label::new(egui::RichText::new(&username).weak())
                                    .sense(egui::Sense::click()),
                            );
                            if name_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if name_resp.clicked() {
                                clicked = true;
                            }
                            let identifier = if is_offline { &username } else { &uuid };
                            let avatar_url =
                                format!("https://mc-heads.net/avatar/{}/32", identifier);
                            let avatar_resp = ui.add(
                                egui::Image::new(&avatar_url)
                                    .fit_to_exact_size(egui::vec2(20.0, 20.0))
                                    .sense(egui::Sense::click()),
                            );
                            if avatar_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if avatar_resp.clicked() {
                                clicked = true;
                            }
                        });
                        if clicked {
                            self.current_view = crate::ui::sidebar::View::Accounts;
                        }
                    }
                });
            });

        // Sidebar
        egui::Panel::left("nav_panel")
            .resizable(false)
            .default_size(200.0)
            .frame(
                theme
                    .as_ref()
                    .map_or(egui::Frame::NONE, |t| t.sidebar_frame()),
            )
            .show_inside(ui, |ui| {
                crate::ui::sidebar::show(ui, &mut self.current_view, theme.as_ref());
            });

        // Content area with breathing room
        egui::CentralPanel::default()
            .frame(
                theme
                    .as_ref()
                    .map_or(egui::Frame::NONE, |t| t.content_frame()),
            )
            .show_inside(ui, |ui| match self.current_view {
                View::Instances => {
                    self.instances_view.theme = theme.clone();
                    self.instances_view.running_instance_ids = self
                        .running_processes
                        .iter()
                        .filter(|rp| rp.is_alive())
                        .map(|rp| rp.instance_id.clone())
                        .collect();
                    self.instances_view.show(
                        ui,
                        &mut self.instances,
                        &self.manifest,
                        &self.java_installs,
                        &self.config,
                    );
                }
                View::Settings => {
                    self.settings_view.show(
                        ui,
                        &mut self.config,
                        &self.themes,
                        self.builtin_theme_count,
                        &mut self.current_theme_idx,
                        &mut self.java_installs,
                        &mut self.java_download,
                        theme.as_ref(),
                    );
                }
                View::Accounts => {
                    self.accounts_view.show(
                        ui,
                        &mut self.account_store,
                        theme.as_ref(),
                    );
                }
                View::Console => {
                    self.console_view.show(
                        ui,
                        theme.as_ref(),
                        &mut self.running_processes,
                    );
                }
            });

        self.handle_view_requests(&ctx);
        self.show_java_prompt(&ctx, &theme);
        self.show_missing_mods_dialog(&ctx, &theme);
        self.show_manual_downloads_dialog(&ctx, &theme);

        // Toast overlay — floating notifications in bottom-right corner
        let has_toasts = !self.toasts.is_empty();
        let has_active_tasks = self.background_tasks.iter().any(|t| !t.is_done());
        if has_toasts || has_active_tasks {
            let screen = ctx.input(|i| i.content_rect());
            let margin = 16.0;
            let toast_width = 320.0;
            let mut y_offset = screen.max.y - margin;

            for (i, toast) in self.toasts.iter().enumerate().rev() {
                let age = toast.created_at.elapsed().as_secs_f32();
                let max_age = if toast.is_error { 8.0 } else { 5.0 };
                // Fade out in last 0.5s
                let alpha = if age > max_age - 0.5 {
                    ((max_age - age) / 0.5).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let a = |c: egui::Color32| -> egui::Color32 {
                    let [r, g, b, _] = c.to_array();
                    egui::Color32::from_rgba_unmultiplied(r, g, b, (alpha * (c.a() as f32)) as u8)
                };

                let (bg_color, accent_color, text_color, icon) = if let Some(t) = &theme {
                    let accent_c = if toast.is_error { t.color("error") } else { t.color("accent") };
                    (
                        a(t.color("bg_secondary")),
                        a(accent_c),
                        a(t.color("fg")),
                        if toast.is_error { egui_phosphor::regular::WARNING_CIRCLE } else { egui_phosphor::regular::CHECK_CIRCLE },
                    )
                } else {
                    if toast.is_error {
                        (
                            a(egui::Color32::from_gray(40)),
                            a(egui::Color32::from_rgb(220, 60, 60)),
                            a(egui::Color32::WHITE),
                            egui_phosphor::regular::WARNING_CIRCLE,
                        )
                    } else {
                        (
                            a(egui::Color32::from_gray(40)),
                            a(egui::Color32::from_rgb(50, 180, 70)),
                            a(egui::Color32::WHITE),
                            egui_phosphor::regular::CHECK_CIRCLE,
                        )
                    }
                };

                let toast_id = egui::Id::new("toast").with(i);
                let stroke_color = a(if let Some(t) = &theme { t.color("surface") } else { egui::Color32::from_gray(60) });

                let area_resp = egui::Area::new(toast_id)
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-margin, -(screen.max.y - y_offset)))
                    .order(egui::Order::Foreground)
                    .show(&ctx, |ui| {
                        let frame_resp = egui::Frame::NONE
                            .fill(bg_color)
                            .corner_radius(8.0)
                            .stroke(egui::Stroke::new(1.0, stroke_color))
                            .inner_margin(egui::Margin { left: 16, right: 12, top: 10, bottom: 10 })
                            .shadow(egui::epaint::Shadow {
                                spread: 0,
                                blur: 12,
                                color: egui::Color32::from_black_alpha((40.0 * alpha) as u8),
                                offset: [0, 4],
                            })
                            .show(ui, |ui| {
                                ui.set_max_width(toast_width - 28.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(icon)
                                            .color(accent_color)
                                            .size(16.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(&toast.message)
                                            .color(text_color)
                                            .size(13.0),
                                    );
                                });
                            });

                        // Paint accent stripe over the left edge of the frame
                        let frame_rect = frame_resp.response.rect;
                        let stripe_rect = egui::Rect::from_min_size(
                            frame_rect.left_top(),
                            egui::vec2(3.0, frame_rect.height()),
                        );
                        ui.painter().rect_filled(
                            stripe_rect,
                            egui::CornerRadius { nw: 8, sw: 8, ..Default::default() },
                            accent_color,
                        );
                    });

                let actual_h = area_resp.response.rect.height();
                y_offset -= actual_h + 8.0; // Stack toasts upward with gap
            }

            // Render active background tasks as persistent progress toasts
            for (i, task) in self.background_tasks.iter().enumerate() {
                if task.is_done() {
                    continue;
                }
                let progress_msg = {
                    let progress = task.progress.lock().unwrap();
                    progress.message.clone()
                };

                let (bg_color, accent_color, text_color) = if let Some(t) = &theme {
                    (t.color("bg_secondary"), t.color("accent"), t.color("fg"))
                } else {
                    (
                        egui::Color32::from_gray(40),
                        egui::Color32::from_rgb(50, 180, 70),
                        egui::Color32::WHITE,
                    )
                };
                let stroke_color = if let Some(t) = &theme { t.color("surface") } else { egui::Color32::from_gray(60) };
                let muted_color = if let Some(t) = &theme { t.color("fg_dim") } else { egui::Color32::GRAY };

                let task_id = egui::Id::new("bg_task_toast").with(i);

                let area_resp = egui::Area::new(task_id)
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-margin, -(screen.max.y - y_offset)))
                    .order(egui::Order::Foreground)
                    .show(&ctx, |ui| {
                        let frame_resp = egui::Frame::NONE
                            .fill(bg_color)
                            .corner_radius(8.0)
                            .stroke(egui::Stroke::new(1.0, stroke_color))
                            .inner_margin(egui::Margin { left: 16, right: 12, top: 10, bottom: 10 })
                            .shadow(egui::epaint::Shadow {
                                spread: 0,
                                blur: 12,
                                color: egui::Color32::from_black_alpha(40),
                                offset: [0, 4],
                            })
                            .show(ui, |ui| {
                                ui.set_max_width(toast_width - 28.0);
                                ui.horizontal(|ui| {
                                    ui.add(egui::Spinner::new().color(accent_color));
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&task.label)
                                                .color(text_color)
                                                .size(13.0),
                                        );
                                        if !progress_msg.is_empty() {
                                            ui.label(
                                                egui::RichText::new(&progress_msg)
                                                    .color(muted_color)
                                                    .size(11.0),
                                            );
                                        }
                                    });
                                });
                            });

                        // Paint accent stripe over the left edge of the frame
                        let frame_rect = frame_resp.response.rect;
                        let stripe_rect = egui::Rect::from_min_size(
                            frame_rect.left_top(),
                            egui::vec2(3.0, frame_rect.height()),
                        );
                        ui.painter().rect_filled(
                            stripe_rect,
                            egui::CornerRadius { nw: 8, sw: 8, ..Default::default() },
                            accent_color,
                        );
                    });

                let actual_h = area_resp.response.rect.height();
                y_offset -= actual_h + 8.0;
            }

            // Request repaint while toasts or active tasks are visible (for fade animation).
            // Use a short delay to avoid running the render loop at max FPS,
            // which can cause resize jank on Wayland.
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn on_exit(&mut self) {
        let _ = self.config.save();
        let _ = self.account_store.save();
        for inst in &self.instances {
            let _ = inst.save_to_dir();
        }
    }
}
