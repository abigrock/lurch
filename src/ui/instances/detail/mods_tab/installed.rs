use eframe::egui;
use super::super::InstanceDetailView;
use crate::core::curseforge;
use crate::core::MutexExt;
use crate::core::instance::ModOrigin;
use crate::core::local_mods;
use crate::core::modrinth;
use crate::ui::helpers::{card_frame, row_hover_highlight};

struct ModInstallResult {
    #[allow(dead_code)]
    filename: String,
    origin: ModOrigin,
}

impl InstanceDetailView {
    pub(super) fn show_installed_tab(
        &mut self,
        ui: &mut egui::Ui,
        mods_dir: &std::path::Path,
        theme: &crate::theme::Theme,
    ) {
        ui.add_space(4.0);

        let update_count = self.mod_updates.len();

        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            if ui.add(theme.accent_button("Add Mod")).clicked()
                && let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Mod Files", &["jar"])
                    .set_title("Select mod file(s)")
                    .pick_files()
                {
                    for path in paths {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let dest = mods_dir.join(name);
                            match std::fs::copy(&path, &dest) {
                                Ok(_) => {
                                    self.needs_rescan = true;
                                    let origin = crate::core::instance::ModOrigin {
                                        filename: name.to_string(),
                                        source: "local".to_string(),
                                        project_id: None,
                                        version_id: None,
                                        version_name: None,
                                    };
                                    self.mod_origin_updates.push(origin);
                                }
                                Err(e) => {
                                    self.pending_toasts.push(crate::app::Toast::error(format!("Error copying mod: {e}")));
                                }
                            }
                        }
                    }
                }

            if ui.add(theme.accent_button("Open Folder")).clicked() {
                let _ = std::fs::create_dir_all(mods_dir);
                let _ = open::that(mods_dir);
            }
        });
        ui.add_space(4.0);

        if self.installed.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::PUZZLE_PIECE)
                        .size(48.0)
                        .color(theme.color("fg_muted")),
                );
                ui.add_space(8.0);
                ui.label(theme.subtext("No mods installed yet."));
                ui.add_space(4.0);
                ui.label(theme.subtext(
                    "Switch to Browse Modrinth or CurseForge to find and install mods.",
                ));
            });
            return;
        }

        ui.add(
            egui::TextEdit::singleline(&mut self.installed_filter)
                .hint_text("Filter mods...")
                .margin(egui::Margin::symmetric(4, 9)),
        );

        if self.mod_update_check.is_some() {
            ui.label(theme.subtext("Checking for updates..."));
        } else if update_count > 0 {
            let row_h2 = ui.spacing().interact_size.y + 4.0;
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), row_h2),
                egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                |ui| {
                let update_fill = theme.color("accent");
                theme.badge_frame(update_fill).show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {} update{} available",
                            egui_phosphor::regular::ARROW_CIRCLE_UP,
                            update_count,
                            if update_count == 1 { "" } else { "s" }
                        ))
                        .size(12.0)
                        .color(theme.button_fg()),
                    );
                });
            });
        }

        ui.add_space(4.0);

        let mut toggle_idx: Option<usize> = None;
        let mut remove_idx: Option<usize> = None;
        let mut update_filename: Option<String> = None;

        let filter_lower = self.installed_filter.to_lowercase();
        let filtered: Vec<(usize, &crate::core::local_mods::InstalledMod)> = self
            .installed
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                filter_lower.is_empty()
                    || m.title.to_lowercase().contains(&filter_lower)
                    || m.filename.to_lowercase().contains(&filter_lower)
            })
            .collect();

        let outer_frame = card_frame(ui, theme);

        outer_frame.show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("installed_mods_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (fi, &(orig_idx, m)) in filtered.iter().enumerate() {
                        if fi > 0 {
                            ui.separator();
                        }
                        let base_name = m
                            .filename
                            .strip_suffix(".disabled")
                            .unwrap_or(&m.filename);
                        let has_update = self.mod_updates.contains_key(base_name);
                        let row_h3 = ui.spacing().interact_size.y + 4.0;
                        let row_resp = ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_h3),
                            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                            |ui| {
                            let icon = if m.enabled {
                                egui_phosphor::regular::CHECK_CIRCLE
                            } else {
                                egui_phosphor::regular::CIRCLE
                            };
                            let toggle = ui.add(theme.ghost_button(icon));
                            if toggle
                                .on_hover_text(if m.enabled { "Disable" } else { "Enable" })
                                .clicked()
                            {
                                toggle_idx = Some(orig_idx);
                            }
                            let project_url = if (m.source == "modrinth" || m.source == "curseforge")
                                && let Some(pid) = &m.project_id
                            {
                                crate::core::local_mods::mod_project_url(&m.source, pid)
                            } else {
                                None
                            };
                            if let Some(url) = &project_url {
                                let tooltip = if m.source == "modrinth" {
                                    "Open on Modrinth"
                                } else {
                                    "Open on CurseForge"
                                };
                                let link_resp = ui.add(
                                    egui::Label::new(theme.title(&m.title))
                                        .truncate()
                                        .sense(egui::Sense::click()),
                                );
                                if link_resp.on_hover_text(tooltip).clicked() {
                                    let _ = open::that(url);
                                }
                            } else {
                                ui.add(egui::Label::new(theme.title(&m.title)).truncate());
                            }
                            if has_update {
                                let update_fill = theme.color("accent");
                                let badge_inner = theme.badge_frame(update_fill).show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(
                                            egui_phosphor::regular::ARROW_CIRCLE_UP,
                                        )
                                        .size(11.0)
                                        .color(theme.button_fg()),
                                    );
                                });
                                badge_inner
                                    .response
                                    .on_hover_text(format!(
                                        "Update → {}",
                                        self.mod_updates
                                            .get(base_name)
                                            .map(|u| u.latest_version_name.as_str())
                                            .unwrap_or("?")
                                    ));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(theme.danger_button(egui_phosphor::regular::TRASH))
                                        .on_hover_text("Remove")
                                        .clicked()
                                    {
                                        remove_idx = Some(orig_idx);
                                    }
                                    if has_update
                                        && ui
                                            .add(theme.accent_button(
                                                egui_phosphor::regular::ARROW_CIRCLE_UP,
                                            ))
                                            .on_hover_text("Update mod")
                                            .clicked()
                                        {
                                            update_filename = Some(base_name.to_string());
                                        }
                                    ui.add(egui::Label::new(theme.subtext(&m.filename)).truncate());
                                },
                            );
                        },
                        );
                        row_hover_highlight(ui, row_resp.response.rect, theme);
                    }
                });
        });

        if let Some(idx) = toggle_idx {
            let m = &self.installed[idx];
            let result = if m.enabled {
                local_mods::disable_mod(mods_dir, &m.filename)
            } else {
                local_mods::enable_mod(mods_dir, &m.filename)
            };
            match result {
                Ok(_) => self.needs_rescan = true,
                Err(e) => {
                    self.pending_toasts.push(crate::app::Toast::error(format!("Error: {e}")));
                }
            }
        }
        if let Some(idx) = remove_idx {
            self.confirm_mod_delete = Some(idx);
        }
        if let Some(filename) = update_filename
            && let Some(info) = self.mod_updates.remove(&filename)
        {
                let mods_path = mods_dir.to_path_buf();
                let old_filename = filename.clone();
                let pending = self.pending_origins.clone();
                let ctx = ui.ctx().clone();

                match info.source.as_str() {
                    "modrinth" => {
                        let project_id = info.project_id.clone();
                        let version_id = info.latest_version_id.clone();
                        let version_name = info.latest_version_name.clone();
                        std::thread::spawn(move || {
                            let result = (|| -> anyhow::Result<ModInstallResult> {
                                let versions = modrinth::get_project_versions(
                                    &project_id,
                                    None,
                                    None,
                                )?;
                                let version = versions
                                    .iter()
                                    .find(|v| v.id == version_id)
                                    .or(versions.first())
                                    .ok_or_else(|| anyhow::anyhow!("Version not found"))?;
                                let file = version
                                    .files
                                    .iter()
                                    .find(|f| f.primary)
                                    .or(version.files.first())
                                    .ok_or_else(|| anyhow::anyhow!("No files in version"))?;
                                let new_filename = modrinth::download_mod_file(file, &mods_path)?;
                                if new_filename != old_filename {
                                    let _ = std::fs::remove_file(mods_path.join(&old_filename));
                                }
                                Ok(ModInstallResult {
                                    filename: new_filename,
                                    origin: ModOrigin {
                                        filename: file.filename.clone(),
                                        source: "modrinth".to_string(),
                                        project_id: Some(project_id.clone()),
                                        version_id: Some(version.id.clone()),
                                        version_name: Some(version_name.clone()),
                                    },
                                })
                            })();
                            match result {
                                Ok(r) => {
                                    pending.lock_or_recover().push(r.origin);
                                }
                                Err(e) => {
                                    log::warn!("Mod update failed for {old_filename}: {e}");
                                }
                            }
                            ctx.request_repaint();
                        });
                    }
                    "curseforge" => {
                        let project_id = info.project_id.clone();
                        let version_id = info.latest_version_id.clone();
                        let version_name = info.latest_version_name.clone();
                        std::thread::spawn(move || {
                            let result = (|| -> anyhow::Result<ModInstallResult> {
                                let mod_id: u64 = project_id.parse()?;
                                let files = curseforge::get_cf_mod_files(mod_id, "", None)?;
                                let file = files
                                    .iter()
                                    .find(|f| f.id.to_string() == version_id)
                                    .or(files.first())
                                    .ok_or_else(|| anyhow::anyhow!("File not found"))?;
                                let new_filename =
                                    curseforge::download_cf_file(file, &mods_path)?;
                                if new_filename != old_filename {
                                    let _ = std::fs::remove_file(mods_path.join(&old_filename));
                                }
                                Ok(ModInstallResult {
                                    filename: new_filename,
                                    origin: ModOrigin {
                                        filename: file.file_name.clone(),
                                        source: "curseforge".to_string(),
                                        project_id: Some(project_id.clone()),
                                        version_id: Some(file.id.to_string()),
                                        version_name: Some(version_name.clone()),
                                    },
                                })
                            })();
                            match result {
                                Ok(r) => {
                                    pending.lock_or_recover().push(r.origin);
                                }
                                Err(e) => {
                                    log::warn!("Mod update failed for {old_filename}: {e}");
                                }
                            }
                            ctx.request_repaint();
                        });
                    }
                    _ => {}
                }
                self.needs_rescan = true;
        }

        // ── Mod delete confirmation dialog ──────────────────────────
        if let Some(del_idx) = self.confirm_mod_delete {
            if del_idx < self.installed.len() {
                let mod_name = self.installed[del_idx].title.clone();

                let mut open = true;
                egui::Window::new("Confirm Delete Mod")
                    .id(egui::Id::new(format!("confirm_mod_delete_{del_idx}")))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .open(&mut open)
                    .show(ui.ctx(), |ui| {
                        ui.label(format!("Remove mod \"{}\"?", mod_name));
                        ui.label(theme.subtext("This will permanently delete the mod file."));
                        ui.add_space(8.0);
                        let row_h4 = ui.spacing().interact_size.y + 4.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_h4),
                            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                            |ui| {
                            if ui.add(theme.danger_button("Delete")).clicked() {
                                let m = &self.installed[del_idx];
                                match local_mods::remove_mod(mods_dir, &m.filename) {
                                    Ok(()) => self.needs_rescan = true,
                                    Err(e) => {
                                        self.pending_toasts.push(crate::app::Toast::error(format!("Error: {e}")));
                                    }
                                }
                                self.confirm_mod_delete = None;
                            }
                            if ui.add(theme.ghost_button("Cancel")).clicked() {
                                self.confirm_mod_delete = None;
                            }
                        },
                        );
                    });

                if !open {
                    self.confirm_mod_delete = None;
                }
            } else {
                self.confirm_mod_delete = None;
            }
        }
    }
}
