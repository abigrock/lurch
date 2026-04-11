mod installed;
mod browse_mr;
mod browse_cf;

use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::{InstanceDetailView, ModsSubTab};
use crate::core::curseforge::{self, CfFile};
use crate::core::instance::ModOrigin;
use crate::core::modrinth::{self, ProjectVersion};

// ── Mod version picker types ────────────────────────────────────────────────

pub(super) struct ModVersionPickerState {
    pub(super) title: String,
    pub(super) source: String,
    pub(super) mr_project_id: Option<String>,
    pub(super) mr_versions: Vec<ProjectVersion>,
    pub(super) cf_mod_id: Option<u64>,
    pub(super) cf_files: Vec<CfFile>,
    pub(super) fetch_handle: Option<Arc<Mutex<Option<ModVersionFetchResult>>>>,
    pub(super) selected_index: usize,
    pub(super) mods_dir: PathBuf,
}

pub(super) enum ModVersionFetchResult {
    MrVersions(Result<Vec<ProjectVersion>, String>),
    CfFiles(Result<Vec<CfFile>, String>),
}

enum ModVersionPickerAction {
    Install,
    Cancel,
}

impl InstanceDetailView {
    pub(super) fn show_mods_tab(
        &mut self,
        ui: &mut egui::Ui,
        instance: &crate::core::instance::Instance,
        mods_dir: &std::path::Path,
        theme: Option<&crate::theme::Theme>,
    ) {
        ui.horizontal(|ui| {
            for (tab, label) in [
                (ModsSubTab::Installed, "Installed"),
                (ModsSubTab::BrowseCurseForge, "Browse CurseForge"),
                (ModsSubTab::BrowseModrinth, "Browse Modrinth"),
            ] {
                if crate::ui::helpers::tab_button(ui, label, self.mods_sub_tab == tab, theme) {
                    self.mods_sub_tab = tab;
                }
            }
        });

        match self.mods_sub_tab {
            ModsSubTab::Installed => self.show_installed_tab(ui, mods_dir, theme),
            ModsSubTab::BrowseCurseForge => {
                self.show_browse_curseforge_tab(ui, instance, mods_dir, theme)
            }
            ModsSubTab::BrowseModrinth => {
                self.show_browse_tab(ui, instance, mods_dir, theme)
            }
        }

        self.poll_mod_version_picker();
        self.show_mod_version_picker(ui, theme);
    }

    fn open_mr_mod_version_picker(
        &mut self,
        project_id: String,
        title: String,
        mc_version: &str,
        loader: &str,
        mods_dir: &std::path::Path,
        ctx: &egui::Context,
    ) {
        let slot: Arc<Mutex<Option<ModVersionFetchResult>>> = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&slot);
        let pid = project_id.clone();
        let mc_ver = mc_version.to_string();
        let loader_str = loader.to_string();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let result =
                modrinth::get_project_versions(&pid, Some(&mc_ver), Some(&loader_str))
                    .map_err(|e| e.to_string());
            *slot_clone.lock().unwrap() = Some(ModVersionFetchResult::MrVersions(result));
            ctx_clone.request_repaint();
        });

        self.mod_version_picker = Some(ModVersionPickerState {
            title,
            source: "modrinth".to_string(),
            mr_project_id: Some(project_id),
            mr_versions: Vec::new(),
            cf_mod_id: None,
            cf_files: Vec::new(),
            fetch_handle: Some(slot),
            selected_index: 0,
            mods_dir: mods_dir.to_path_buf(),
        });
    }

    fn open_cf_mod_version_picker(
        &mut self,
        mod_id: u64,
        title: String,
        mc_version: &str,
        loader_type: Option<u32>,
        mods_dir: &std::path::Path,
        ctx: &egui::Context,
    ) {
        let slot: Arc<Mutex<Option<ModVersionFetchResult>>> = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&slot);
        let mc_ver = mc_version.to_string();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let result =
                curseforge::get_cf_mod_files(mod_id, &mc_ver, loader_type)
                    .map_err(|e| e.to_string());
            *slot_clone.lock().unwrap() = Some(ModVersionFetchResult::CfFiles(result));
            ctx_clone.request_repaint();
        });

        self.mod_version_picker = Some(ModVersionPickerState {
            title,
            source: "curseforge".to_string(),
            mr_project_id: None,
            mr_versions: Vec::new(),
            cf_mod_id: Some(mod_id),
            cf_files: Vec::new(),
            fetch_handle: Some(slot),
            selected_index: 0,
            mods_dir: mods_dir.to_path_buf(),
        });
    }

    fn poll_mod_version_picker(&mut self) {
        let Some(vp) = &mut self.mod_version_picker else {
            return;
        };
        let Some(handle) = &vp.fetch_handle else {
            return;
        };
        let taken = handle.lock().unwrap().take();
        if let Some(result) = taken {
            vp.fetch_handle = None;
            match result {
                ModVersionFetchResult::MrVersions(Ok(versions)) => {
                    vp.mr_versions = versions;
                }
                ModVersionFetchResult::CfFiles(Ok(files)) => {
                    vp.cf_files = files;
                }
                ModVersionFetchResult::MrVersions(Err(e))
                | ModVersionFetchResult::CfFiles(Err(e)) => {
                    log::warn!("Failed to fetch mod versions: {e}");
                    self.mod_version_picker = None;
                }
            }
        }
    }

    fn show_mod_version_picker(
        &mut self,
        ui: &mut egui::Ui,
        theme: Option<&crate::theme::Theme>,
    ) {
        if self.mod_version_picker.is_none() {
            return;
        }

        let mut action: Option<ModVersionPickerAction> = None;

        let vp = self.mod_version_picker.as_mut().unwrap();
        let is_loading = vp.fetch_handle.is_some();
        let title = vp.title.clone();

        egui::Window::new(format!("Install \"{}\"", title))
            .collapsible(false)
            .resizable(true)
            .default_size([450.0, 400.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                if is_loading {
                    ui.horizontal(|ui| {
                        if let Some(t) = theme {
                            ui.add(egui::Spinner::new().color(t.color("accent")));
                        } else {
                            ui.spinner();
                        }
                        ui.label("Fetching versions...");
                    });
                    return;
                }

                if let Some(t) = theme {
                    ui.label(t.subtext("Select a version to install:"));
                } else {
                    ui.weak("Select a version to install:");
                }
                ui.add_space(4.0);

                match vp.source.as_str() {
                    "modrinth" => {
                        if vp.mr_versions.is_empty() {
                            ui.label("No versions available.");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for (i, version) in vp.mr_versions.iter().enumerate() {
                                        let selected = vp.selected_index == i;
                                        let game_vers = version.game_versions.join(", ");
                                        let loaders = version.loaders.join(", ");
                                        let label = format!(
                                            "{} (MC {}) [{}]",
                                            version.name, game_vers, loaders
                                        );
                                        if ui.selectable_label(selected, &label).clicked() {
                                            vp.selected_index = i;
                                        }
                                    }
                                });
                        }
                    }
                    "curseforge" => {
                        if vp.cf_files.is_empty() {
                            ui.label("No versions available.");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for (i, file) in vp.cf_files.iter().enumerate() {
                                        let selected = vp.selected_index == i;
                                        let release_tag = match file.release_type {
                                            1 => "Release",
                                            2 => "Beta",
                                            3 => "Alpha",
                                            _ => "Unknown",
                                        };
                                        let game_vers = file.game_versions.join(", ");
                                        let label = format!(
                                            "{} [{}] ({})",
                                            file.display_name, release_tag, game_vers
                                        );
                                        if ui.selectable_label(selected, &label).clicked() {
                                            vp.selected_index = i;
                                        }
                                    }
                                });
                        }
                    }
                    _ => {}
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let has_versions = match vp.source.as_str() {
                        "modrinth" => !vp.mr_versions.is_empty(),
                        "curseforge" => !vp.cf_files.is_empty(),
                        _ => false,
                    };
                    let install_clicked = if let Some(t) = theme {
                        ui.add_enabled(has_versions, t.accent_button("Install"))
                            .clicked()
                    } else {
                        ui.add_enabled(has_versions, egui::Button::new("Install"))
                            .clicked()
                    };
                    if install_clicked {
                        action = Some(ModVersionPickerAction::Install);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(ModVersionPickerAction::Cancel);
                    }
                });
            });

        match action {
            Some(ModVersionPickerAction::Install) => {
                let vp = self.mod_version_picker.take().unwrap();
                match vp.source.as_str() {
                    "modrinth" => {
                        if let Some(version) = vp.mr_versions.get(vp.selected_index) {
                            let version = version.clone();
                            let project_id = vp.mr_project_id.unwrap_or_default();
                            let mods_dir = vp.mods_dir.clone();
                            let origins = Arc::clone(&self.pending_origins);
                            let ctx = ui.ctx().clone();
                            let status: Arc<Mutex<Option<String>>> =
                                Arc::new(Mutex::new(None));
                            let status_clone = Arc::clone(&status);

                            std::thread::spawn(move || {
                                let file = version
                                    .files
                                    .iter()
                                    .find(|f| f.primary)
                                    .or(version.files.first());
                                let msg = match file {
                                    Some(f) => match modrinth::download_mod_file(f, &mods_dir) {
                                        Ok(filename) => {
                                            origins.lock().unwrap().push(ModOrigin {
                                                filename: f.filename.clone(),
                                                source: "modrinth".to_string(),
                                                project_id: Some(project_id),
                                                version_id: Some(version.id.clone()),
                                                version_name: Some(version.name.clone()),
                                            });
                                            format!("Installed: {filename}")
                                        }
                                        Err(e) => format!("Install failed: {e}"),
                                    },
                                    None => "Install failed: no files in version".to_string(),
                                };
                                *status_clone.lock().unwrap() = Some(msg);
                                ctx.request_repaint();
                            });

                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::new("install_status"), status);
                            });
                            self.needs_rescan = true;
                        }
                    }
                    "curseforge" => {
                        if let Some(file) = vp.cf_files.get(vp.selected_index) {
                            let file = file.clone();
                            let mod_id = vp.cf_mod_id.unwrap_or(0);
                            let mods_dir = vp.mods_dir.clone();
                            let origins = Arc::clone(&self.pending_origins);
                            let ctx = ui.ctx().clone();
                            let status: Arc<Mutex<Option<String>>> =
                                Arc::new(Mutex::new(None));
                            let status_clone = Arc::clone(&status);

                            std::thread::spawn(move || {
                                let msg = match curseforge::download_cf_file(&file, &mods_dir)
                                {
                                    Ok(filename) => {
                                        origins.lock().unwrap().push(ModOrigin {
                                            filename: file.file_name.clone(),
                                            source: "curseforge".to_string(),
                                            project_id: Some(mod_id.to_string()),
                                            version_id: Some(file.id.to_string()),
                                            version_name: Some(file.display_name.clone()),
                                        });
                                        format!("Installed: {filename}")
                                    }
                                    Err(e) => format!("Install failed: {e}"),
                                };
                                *status_clone.lock().unwrap() = Some(msg);
                                ctx.request_repaint();
                            });

                            ui.ctx().data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::new("cf_install_status"),
                                    status,
                                );
                            });
                            self.needs_rescan = true;
                        }
                    }
                    _ => {}
                }
            }
            Some(ModVersionPickerAction::Cancel) => {
                self.mod_version_picker = None;
            }
            None => {}
        }
    }
}
