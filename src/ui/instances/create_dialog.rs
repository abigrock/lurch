use super::{AddInstanceTab, InstancesView, modpack_browser};
use crate::core::MutexExt;
use crate::core::instance::{Instance, ModLoader};
use crate::core::version::ManifestState;
use eframe::egui;
use std::sync::{Arc, Mutex};

impl InstancesView {
    // ── Add Instance modal ────────────────────────────────────────────────────

    pub(super) fn show_add_instance_view(
        &mut self,
        ui: &mut egui::Ui,
        instances: &mut Vec<Instance>,
        manifest: &Arc<Mutex<ManifestState>>,
        config: &crate::core::config::AppConfig,
    ) {
        // Poll loader version fetch
        if let Some(fetch) = &self.loader_versions_fetch {
            let finished = fetch.lock().ok().and_then(|mut g| g.take());
            if let Some(result) = finished {
                match result {
                    Ok(versions) => {
                        self.loader_versions = versions;
                        self.loader_versions_error = None;
                    }
                    Err(e) => {
                        self.loader_versions = Vec::new();
                        self.loader_versions_error = Some(e);
                    }
                }
                self.loader_versions_loading = false;
                self.loader_versions_fetch = None;
            }
        }

        // Top bar
        ui.horizontal(|ui| {
            let back_clicked = ui
                .add(self.theme.ghost_button(egui_phosphor::regular::ARROW_LEFT))
                .clicked();
            if back_clicked {
                self.show_add_instance = false;
            }
            ui.separator();
            ui.label(self.theme.section_header("Add Instance"));
        });
        ui.separator();

        // Tab bar
        ui.horizontal(|ui| {
            use crate::ui::helpers::tab_button;
            if tab_button(
                ui,
                &format!("{} Vanilla", egui_phosphor::regular::CUBE),
                self.add_instance_tab == AddInstanceTab::Vanilla,
                &self.theme,
            ) {
                self.add_instance_tab = AddInstanceTab::Vanilla;
            }
            if tab_button(
                ui,
                &format!("{} CurseForge", egui_phosphor::regular::FIRE),
                self.add_instance_tab == AddInstanceTab::CurseForge,
                &self.theme,
            ) {
                self.add_instance_tab = AddInstanceTab::CurseForge;
            }
            if tab_button(
                ui,
                &format!("{} Modrinth", egui_phosphor::regular::PACKAGE),
                self.add_instance_tab == AddInstanceTab::Modrinth,
                &self.theme,
            ) {
                self.add_instance_tab = AddInstanceTab::Modrinth;
            }
            if tab_button(
                ui,
                &format!("{} Import", egui_phosphor::regular::DOWNLOAD_SIMPLE),
                self.add_instance_tab == AddInstanceTab::Import,
                &self.theme,
            ) {
                self.add_instance_tab = AddInstanceTab::Import;
            }
        });
        ui.separator();

        // Tab content
        let ctx = ui.ctx().clone();
        match self.add_instance_tab {
            AddInstanceTab::Vanilla => {
                self.show_vanilla_tab(ui, instances, manifest, config, &ctx);
            }
            AddInstanceTab::Modrinth => {
                self.modpack_browser.show_for_source(
                    ui,
                    modpack_browser::ModpackSource::Modrinth,
                    &self.theme,
                    &mut self.pending_toasts,
                );
            }
            AddInstanceTab::CurseForge => {
                self.modpack_browser.show_for_source(
                    ui,
                    modpack_browser::ModpackSource::CurseForge,
                    &self.theme,
                    &mut self.pending_toasts,
                );
            }
            AddInstanceTab::Import => {
                self.show_import_tab(ui, instances);
            }
        }
    }

    fn show_vanilla_tab(
        &mut self,
        ui: &mut egui::Ui,
        instances: &mut Vec<Instance>,
        manifest: &Arc<Mutex<ManifestState>>,
        config: &crate::core::config::AppConfig,
        ctx: &egui::Context,
    ) {
        egui::Grid::new("create_instance_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Name:");
                let name_before = self.new_name.clone();
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_name)
                        .hint_text("Instance name")
                        .margin(egui::Margin::symmetric(4, 9)),
                );
                if self.new_name != name_before {
                    self.name_auto_generated = false;
                }
                ui.end_row();

                let prev_mc_version = self.new_mc_version.clone();
                ui.label("MC Version:");
                let manifest_snapshot = manifest.lock_or_recover();
                match &*manifest_snapshot {
                    ManifestState::Loading => {
                        drop(manifest_snapshot);
                        ui.add(egui::Spinner::new().color(self.theme.color("accent")));
                        ui.label("Loading versions...");
                    }
                    ManifestState::Failed(err) => {
                        let err_msg = err.clone();
                        drop(manifest_snapshot);
                        ui.vertical(|ui| {
                            ui.colored_label(
                                egui::Color32::RED,
                                format!("Failed to load versions: {err_msg}"),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_mc_version)
                                    .hint_text("Filter versions...")
                                    .margin(egui::Margin::symmetric(4, 9)),
                            );
                        });
                    }
                    ManifestState::Loaded(version_manifest) => {
                        let filtered: Vec<crate::core::version::VersionEntry> = version_manifest
                            .versions
                            .iter()
                            .filter(|v| {
                                if !self.show_snapshots
                                    && v.version_type != crate::core::version::VersionType::Release
                                {
                                    return false;
                                }
                                true
                            })
                            .cloned()
                            .collect();
                        drop(manifest_snapshot);

                        if self.new_mc_version.is_empty()
                            && let Some(latest) = filtered.iter().find(|v| {
                                v.version_type == crate::core::version::VersionType::Release
                            })
                        {
                            self.new_mc_version = latest.id.clone();
                            if self.new_loader == ModLoader::Vanilla {
                                self.new_name = format!("Minecraft {}", self.new_mc_version);
                            } else {
                                self.new_name =
                                    format!("{} {}", self.new_loader, self.new_mc_version);
                            }
                            self.name_auto_generated = true;
                        }

                        ui.horizontal(|ui| {
                            let selected_label = if self.new_mc_version.is_empty() {
                                "Select a version...".to_string()
                            } else {
                                self.new_mc_version.clone()
                            };
                            egui::ComboBox::from_id_salt("mc_version_select")
                                .selected_text(selected_label)
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    for entry in &filtered {
                                        let label = match entry.version_type {
                                            crate::core::version::VersionType::Release => {
                                                entry.id.clone()
                                            }
                                            crate::core::version::VersionType::Snapshot => {
                                                format!("{} (snapshot)", entry.id)
                                            }
                                            crate::core::version::VersionType::OldBeta => {
                                                format!("{} (old beta)", entry.id)
                                            }
                                            crate::core::version::VersionType::OldAlpha => {
                                                format!("{} (old alpha)", entry.id)
                                            }
                                        };
                                        ui.selectable_value(
                                            &mut self.new_mc_version,
                                            entry.id.clone(),
                                            label,
                                        );
                                    }
                                });
                            ui.checkbox(&mut self.show_snapshots, "Show snapshots");
                        });
                    }
                }
                ui.end_row();

                ui.label("Loader:");
                let prev_loader = self.new_loader.clone();
                egui::ComboBox::from_id_salt("loader_select")
                    .selected_text(self.new_loader.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.new_loader, ModLoader::Vanilla, "Vanilla");
                        ui.selectable_value(&mut self.new_loader, ModLoader::Fabric, "Fabric");
                        ui.selectable_value(&mut self.new_loader, ModLoader::Forge, "Forge");
                        ui.selectable_value(&mut self.new_loader, ModLoader::NeoForge, "NeoForge");
                        ui.selectable_value(&mut self.new_loader, ModLoader::Quilt, "Quilt");
                    });
                ui.end_row();

                if self.new_loader != prev_loader {
                    self.loader_versions.clear();
                    self.loader_versions_loading = false;
                    self.loader_versions_error = None;
                    self.loader_versions_fetch = None;
                    self.new_loader_version.clear();
                }

                let version_or_loader_changed =
                    self.new_mc_version != prev_mc_version || self.new_loader != prev_loader;
                if version_or_loader_changed
                    && !self.new_mc_version.is_empty()
                    && (self.name_auto_generated || self.new_name.is_empty())
                {
                    self.new_name = if self.new_loader == ModLoader::Vanilla {
                        format!("Minecraft {}", self.new_mc_version)
                    } else {
                        format!("{} {}", self.new_loader, self.new_mc_version)
                    };
                    self.name_auto_generated = true;
                }

                if self.new_loader != ModLoader::Vanilla
                    && !self.new_mc_version.is_empty()
                    && self.loader_versions.is_empty()
                    && !self.loader_versions_loading
                    && self.loader_versions_error.is_none()
                {
                    self.loader_versions_loading = true;
                    let loader = self.new_loader.clone();
                    let mc_version = self.new_mc_version.clone();
                    #[allow(clippy::type_complexity)]
                    let result: Arc<
                        Mutex<Option<Result<Vec<(String, bool)>, String>>>,
                    > = Arc::new(Mutex::new(None));
                    let result_clone = Arc::clone(&result);
                    let ctx_clone = ctx.clone();
                    std::thread::spawn(move || {
                        let client = crate::core::http_client();
                        let outcome = crate::core::loader_profiles::fetch_loader_versions(
                            &client,
                            &loader,
                            &mc_version,
                        )
                        .map_err(|e| e.to_string());
                        if let Ok(mut lock) = result_clone.lock() {
                            *lock = Some(outcome);
                        }
                        ctx_clone.request_repaint();
                    });
                    self.loader_versions_fetch = Some(result);
                }

                if self.new_loader != ModLoader::Vanilla {
                    ui.label("Loader version:");
                    if self.loader_versions_loading {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().color(self.theme.color("accent")));
                            ui.weak("Loading versions...");
                        });
                    } else if self.loader_versions_error.is_some() {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 140, 0),
                            format!(
                                "{} is not available for Minecraft {}",
                                self.new_loader, self.new_mc_version
                            ),
                        );
                    } else if self.loader_versions.is_empty() {
                        ui.weak("No versions available");
                    } else {
                        let display = if self.new_loader_version.is_empty() {
                            "Latest stable".to_string()
                        } else {
                            self.new_loader_version.clone()
                        };
                        egui::ComboBox::from_id_salt("loader_version_select")
                            .selected_text(&display)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        self.new_loader_version.is_empty(),
                                        "Latest stable",
                                    )
                                    .clicked()
                                {
                                    self.new_loader_version = String::new();
                                }
                                for (ver, stable) in &self.loader_versions {
                                    let label = if *stable {
                                        ver.clone()
                                    } else {
                                        format!("{ver} (unstable)")
                                    };
                                    if ui
                                        .selectable_label(self.new_loader_version == *ver, &label)
                                        .clicked()
                                    {
                                        self.new_loader_version = ver.clone();
                                    }
                                }
                            });
                    }
                    ui.end_row();
                }

                ui.label("Group:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_group)
                        .hint_text("Optional")
                        .margin(egui::Margin::symmetric(4, 9)),
                );
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_create =
                !self.new_name.trim().is_empty() && !self.new_mc_version.trim().is_empty();
            let create_clicked = ui
                .add_enabled(can_create, self.theme.accent_button("Create"))
                .clicked();
            if create_clicked {
                let mut inst = Instance::new(
                    self.new_name.trim().to_string(),
                    self.new_mc_version.trim().to_string(),
                );
                inst.min_memory_mb = config.default_min_memory_mb;
                inst.max_memory_mb = config.default_max_memory_mb;
                inst.loader = self.new_loader.clone();
                if !self.new_loader_version.is_empty() {
                    inst.loader_version = Some(self.new_loader_version.clone());
                }
                let group = self.new_group.trim().to_string();
                if !group.is_empty() {
                    inst.group = Some(group);
                }
                let _ = inst.create_dirs();
                instances.push(inst);
                self.show_add_instance = false;
            }
            if ui.add(self.theme.ghost_button("Cancel")).clicked() {
                self.show_add_instance = false;
            }
        });
    }

    fn show_import_tab(&mut self, ui: &mut egui::Ui, _instances: &mut Vec<Instance>) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);

            let icon = egui_phosphor::regular::DOWNLOAD_SIMPLE;
            ui.label(
                egui::RichText::new(icon)
                    .size(48.0)
                    .color(self.theme.color("fg_muted")),
            );

            ui.add_space(12.0);

            ui.label(self.theme.section_header("Import Instance"));
            ui.add_space(4.0);
            ui.label(self.theme.subtext("Import a Lurch export (.zip), Modrinth modpack (.mrpack), or CurseForge modpack."));

            ui.add_space(16.0);

            let browse_lbl = format!("{} Browse Files...", egui_phosphor::regular::FOLDER_OPEN);
            let browse_clicked = ui.add(self.theme.accent_button(&browse_lbl)).clicked();

            if browse_clicked
                && let Some(path) = rfd::FileDialog::new()
                    .set_title("Import Instance or Modpack")
                    .add_filter("Supported archives", &["zip", "mrpack"])
                    .add_filter("Zip Archive", &["zip"])
                    .add_filter("Modrinth Modpack", &["mrpack"])
                    .pick_file()
                {
                    match crate::core::import_export::detect_archive_type(&path) {
                        Ok(crate::core::import_export::ArchiveType::LurchExport) => {
                            let slot: Arc<Mutex<Option<Result<Instance, String>>>> =
                                Arc::new(Mutex::new(None));
                            let slot_clone = Arc::clone(&slot);
                            let ctx_clone = ui.ctx().clone();
                            std::thread::spawn(move || {
                                let result = crate::core::import_export::import_instance(&path)
                                    .map_err(|e| e.to_string());
                                *slot_clone.lock_or_recover() = Some(result);
                                ctx_clone.request_repaint();
                            });
                            self.import_task = Some(slot);
                            self.show_add_instance = false;
                            self.pending_toasts.push(crate::ui::notifications::Toast::success("Importing instance...".to_string()));
                        }
                        Ok(crate::core::import_export::ArchiveType::ModrinthMrpack) => {
                            self.local_mrpack_import = Some(path);
                            self.show_add_instance = false;
                        }
                        Ok(crate::core::import_export::ArchiveType::CurseForgeModpack) => {
                            self.local_cf_modpack_import = Some(path);
                            self.show_add_instance = false;
                        }
                        Ok(crate::core::import_export::ArchiveType::Unknown) => {
                            self.pending_toasts.push(crate::ui::notifications::Toast::error("Unrecognized archive format. Expected a Lurch export, Modrinth .mrpack, or CurseForge modpack."));
                        }
                        Err(e) => {
                            self.pending_toasts.push(crate::ui::notifications::Toast::error(format!("Import failed: {e}")));
                        }
                    }
                }
        });
    }
}
