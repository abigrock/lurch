use super::InstancesView;
use crate::app::ManifestState;
use crate::core::instance::{Instance, ModLoader};
use crate::core::java::JavaInstall;
use crate::core::version::{VersionEntry, VersionType};
use eframe::egui;
use std::sync::{Arc, Mutex};

impl InstancesView {
    // ── Edit / config dialog ─────────────────────────────────────────────────

    pub(super) fn edit_dialog(
        &mut self,
        ctx: &egui::Context,
        instances: &mut [Instance],
        edit_id: &str,
        java_installs: &[JavaInstall],
        manifest: &Arc<Mutex<ManifestState>>,
    ) {
        let Some(inst) = instances.iter_mut().find(|i| i.id == edit_id) else {
            self.editing = None;
            return;
        };

        // Initialize edit state when opening for a new instance
        if self.edit_initialized_for.as_deref() != Some(edit_id) {
            self.edit_mc_version = inst.mc_version.clone();
            self.edit_loader = inst.loader.clone();
            self.edit_loader_version = inst.loader_version.clone().unwrap_or_default();
            self.edit_show_snapshots = false;
            self.edit_loader_versions.clear();
            self.edit_loader_versions_loading = false;
            self.edit_loader_versions_error = None;
            self.edit_loader_versions_fetch = None;
            self.edit_initialized_for = Some(edit_id.to_string());
        }

        // Poll loader version fetch for edit dialog
        if let Some(fetch) = &self.edit_loader_versions_fetch {
            let finished = fetch.lock().ok().and_then(|mut g| g.take());
            if let Some(result) = finished {
                match result {
                    Ok(versions) => {
                        self.edit_loader_versions = versions;
                        self.edit_loader_versions_error = None;
                    }
                    Err(e) => {
                        self.edit_loader_versions = Vec::new();
                        self.edit_loader_versions_error = Some(e);
                    }
                }
                self.edit_loader_versions_loading = false;
                self.edit_loader_versions_fetch = None;
            }
        }

        let mut open = true;
        egui::Window::new(format!("Configure — {}", inst.name))
            .id(egui::Id::new(format!("edit_dialog_{}", inst.id)))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new(format!("edit_grid_{}", inst.id))
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        // Group
                        ui.label("Group:");
                        let mut group_text = inst.group.clone().unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut group_text)
                                    .margin(egui::Margin::symmetric(4, 9)),
                            )
                            .changed()
                        {
                            inst.group = if group_text.trim().is_empty() {
                                None
                            } else {
                                Some(group_text.trim().to_string())
                            };
                        }
                        ui.end_row();

                        // MC Version selector
                        let prev_edit_mc = self.edit_mc_version.clone();
                        ui.label("MC Version:");
                        let manifest_snapshot = manifest.lock().unwrap();
                        match &*manifest_snapshot {
                            ManifestState::Loading => {
                                drop(manifest_snapshot);
                                ui.horizontal(|ui| {
                                    ui.add(egui::Spinner::new().color(self.theme.color("accent")));
                                    ui.weak("Loading versions...");
                                });
                            }
                            ManifestState::Failed(_err) => {
                                drop(manifest_snapshot);
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.edit_mc_version)
                                        .margin(egui::Margin::symmetric(4, 9)),
                                );
                            }
                            ManifestState::Loaded(version_manifest) => {
                                let filtered: Vec<VersionEntry> = version_manifest
                                    .versions
                                    .iter()
                                    .filter(|v| {
                                        if !self.edit_show_snapshots
                                            && v.version_type != VersionType::Release
                                        {
                                            return false;
                                        }
                                        true
                                    })
                                    .cloned()
                                    .collect();
                                drop(manifest_snapshot);

                                let selected_label = if self.edit_mc_version.is_empty() {
                                    "Select a version...".to_string()
                                } else {
                                    self.edit_mc_version.clone()
                                };
                                egui::ComboBox::from_id_salt(format!(
                                    "edit_mc_version_{}",
                                    inst.id
                                ))
                                .selected_text(selected_label)
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    for entry in &filtered {
                                        let label = match entry.version_type {
                                            VersionType::Release => entry.id.clone(),
                                            VersionType::Snapshot => {
                                                format!("{} (snapshot)", entry.id)
                                            }
                                            VersionType::OldBeta => {
                                                format!("{} (old beta)", entry.id)
                                            }
                                            VersionType::OldAlpha => {
                                                format!("{} (old alpha)", entry.id)
                                            }
                                        };
                                        ui.selectable_value(
                                            &mut self.edit_mc_version,
                                            entry.id.clone(),
                                            label,
                                        );
                                    }
                                });
                            }
                        }
                        ui.end_row();

                        ui.label("");
                        ui.checkbox(&mut self.edit_show_snapshots, "Show snapshots");
                        ui.end_row();

                        // Loader selector
                        let prev_edit_loader = self.edit_loader.clone();
                        ui.label("Loader:");
                        egui::ComboBox::from_id_salt(format!("edit_loader_{}", inst.id))
                            .selected_text(self.edit_loader.to_string())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.edit_loader,
                                    ModLoader::Vanilla,
                                    "Vanilla",
                                );
                                ui.selectable_value(
                                    &mut self.edit_loader,
                                    ModLoader::Fabric,
                                    "Fabric",
                                );
                                ui.selectable_value(
                                    &mut self.edit_loader,
                                    ModLoader::Forge,
                                    "Forge",
                                );
                                ui.selectable_value(
                                    &mut self.edit_loader,
                                    ModLoader::NeoForge,
                                    "NeoForge",
                                );
                                ui.selectable_value(
                                    &mut self.edit_loader,
                                    ModLoader::Quilt,
                                    "Quilt",
                                );
                            });
                        ui.end_row();

                        // Reset loader versions when loader or MC version changes
                        if self.edit_loader != prev_edit_loader
                            || self.edit_mc_version != prev_edit_mc
                        {
                            self.edit_loader_versions.clear();
                            self.edit_loader_versions_loading = false;
                            self.edit_loader_versions_error = None;
                            self.edit_loader_versions_fetch = None;
                            if self.edit_loader != prev_edit_loader {
                                self.edit_loader_version.clear();
                            }
                        }

                        // Trigger loader version fetch
                        if self.edit_loader != ModLoader::Vanilla
                            && !self.edit_mc_version.is_empty()
                            && self.edit_loader_versions.is_empty()
                            && !self.edit_loader_versions_loading
                            && self.edit_loader_versions_error.is_none()
                        {
                            self.edit_loader_versions_loading = true;
                            let loader = self.edit_loader.clone();
                            let mc_version = self.edit_mc_version.clone();
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
                            self.edit_loader_versions_fetch = Some(result);
                        }

                        // Loader version picker
                        if self.edit_loader != ModLoader::Vanilla {
                            ui.label("Loader version:");
                            if self.edit_loader_versions_loading {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Spinner::new().color(self.theme.color("accent")));
                                    ui.weak("Loading versions...");
                                });
                            } else if self.edit_loader_versions_error.is_some() {
                                ui.colored_label(
                                    egui::Color32::from_rgb(255, 140, 0),
                                    format!(
                                        "{} is not available for Minecraft {}",
                                        self.edit_loader, self.edit_mc_version
                                    ),
                                );
                            } else if self.edit_loader_versions.is_empty() {
                                ui.weak("No versions available");
                            } else {
                                let display = if self.edit_loader_version.is_empty() {
                                    "Latest stable".to_string()
                                } else {
                                    self.edit_loader_version.clone()
                                };
                                egui::ComboBox::from_id_salt(format!(
                                    "edit_loader_ver_{}",
                                    inst.id
                                ))
                                .selected_text(&display)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            self.edit_loader_version.is_empty(),
                                            "Latest stable",
                                        )
                                        .clicked()
                                    {
                                        self.edit_loader_version = String::new();
                                    }
                                    for (ver, stable) in &self.edit_loader_versions {
                                        let label = if *stable {
                                            ver.clone()
                                        } else {
                                            format!("{ver} (unstable)")
                                        };
                                        if ui
                                            .selectable_label(
                                                self.edit_loader_version == *ver,
                                                &label,
                                            )
                                            .clicked()
                                        {
                                            self.edit_loader_version = ver.clone();
                                        }
                                    }
                                });
                            }
                            ui.end_row();
                        }

                        // Java path
                        ui.label("Java:");
                        let selected_text = match &inst.java_path {
                            None => "Auto-detect (recommended)".to_string(),
                            Some(path) => java_installs
                                .iter()
                                .find(|j| j.path == *path)
                                .map(|j| {
                                    if j.managed {
                                        format!(
                                            "Java {} — {} · {} (Lurch)",
                                            j.major, j.version, j.vendor
                                        )
                                    } else {
                                        format!("Java {} — {} · {}", j.major, j.version, j.vendor)
                                    }
                                })
                                .unwrap_or_else(|| {
                                    if let Some(provider) = &inst.java_provider {
                                        format!("Custom: {} ({})", path.display(), provider)
                                    } else {
                                        format!("Custom: {}", path.display())
                                    }
                                }),
                        };
                        egui::ComboBox::from_id_salt(format!("java_select_{}", inst.id))
                            .selected_text(&selected_text)
                            .width(300.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        inst.java_path.is_none(),
                                        "Auto-detect (recommended)",
                                    )
                                    .clicked()
                                {
                                    inst.java_path = None;
                                    inst.java_provider = None;
                                }
                                for j in java_installs {
                                    let label = if j.managed {
                                        format!(
                                            "Java {} — {} · {} (Lurch)",
                                            j.major, j.version, j.vendor
                                        )
                                    } else {
                                        format!("Java {} — {} · {}", j.major, j.version, j.vendor)
                                    };
                                    let provider = if j.managed {
                                        format!("{} (Lurch)", j.vendor)
                                    } else {
                                        j.vendor.clone()
                                    };
                                    let is_selected =
                                        inst.java_path.as_ref().is_some_and(|p| *p == j.path);
                                    if ui.selectable_label(is_selected, &label).clicked() {
                                        inst.java_path = Some(j.path.clone());
                                        inst.java_provider = Some(provider);
                                    }
                                }
                            });
                        ui.end_row();

                        // Min memory
                        ui.label("Min Memory (MB):");
                        ui.add(
                            egui::DragValue::new(&mut inst.min_memory_mb)
                                .range(256..=65536)
                                .speed(64.0),
                        );
                        ui.end_row();

                        // Max memory
                        ui.label("Max Memory (MB):");
                        ui.add(
                            egui::DragValue::new(&mut inst.max_memory_mb)
                                .range(256..=65536)
                                .speed(64.0),
                        );
                        ui.end_row();

                        // JVM args
                        ui.label("JVM Args:");
                        let mut jvm_text = inst.jvm_args.join(" ");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut jvm_text)
                                    .margin(egui::Margin::symmetric(4, 9)),
                            )
                            .changed()
                        {
                            inst.jvm_args =
                                jvm_text.split_whitespace().map(str::to_string).collect();
                        }
                        ui.end_row();

                        // Environment variables
                        ui.label("Env Vars:");
                        ui.add(
                            egui::TextEdit::multiline(&mut inst.env_vars)
                                .desired_rows(3)
                                .hint_text("KEY=VALUE (one per line)")
                                .margin(egui::Margin::symmetric(4, 9)),
                        );
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let save_clicked = ui.add(self.theme.accent_button("Save")).clicked();
                    if save_clicked {
                        // Apply MC version / loader changes
                        inst.mc_version = self.edit_mc_version.clone();
                        inst.loader = self.edit_loader.clone();
                        inst.loader_version = if self.edit_loader == ModLoader::Vanilla
                            || self.edit_loader_version.is_empty()
                        {
                            None
                        } else {
                            Some(self.edit_loader_version.clone())
                        };
                        let _ = inst.save_to_dir();
                        self.edit_initialized_for = None;
                        self.editing = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.edit_initialized_for = None;
                        self.editing = None;
                    }
                });
            });

        if !open {
            self.edit_initialized_for = None;
            self.editing = None;
        }
    }

    // ── Delete confirmation dialog ───────────────────────────────────────────

    pub(super) fn delete_confirm_dialog(
        &mut self,
        ctx: &egui::Context,
        instances: &mut Vec<Instance>,
        del_id: &str,
    ) {
        let inst_name = instances
            .iter()
            .find(|i| i.id == del_id)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| del_id.to_string());

        let mut open = true;
        egui::Window::new("Confirm Delete")
            .id(egui::Id::new(format!("confirm_delete_{del_id}")))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!("Delete instance \"{inst_name}\"?"));
                ui.label(
                    self.theme
                        .subtext("This will permanently remove all instance files."),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let delete_clicked = ui.add(self.theme.danger_button("Delete")).clicked();
                    if delete_clicked {
                        let del_id_owned = del_id.to_string();
                        if let Some(pos) = instances.iter().position(|i| i.id == del_id_owned) {
                            let _ = instances[pos].delete_dirs();
                            instances.remove(pos);
                        }
                        self.confirm_delete = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_delete = None;
                    }
                });
            });

        if !open {
            self.confirm_delete = None;
        }
    }
}
