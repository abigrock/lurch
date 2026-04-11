use eframe::egui;

use super::InstanceDetailView;
use crate::core::shaders::{self, ShaderPack};
use crate::ui::helpers::row_hover_highlight;

impl InstanceDetailView {
    pub(super) fn show_shaders_tab(
        &mut self,
        ui: &mut egui::Ui,
        shaderpacks_dir: &std::path::Path,
        theme: &crate::theme::Theme,
    ) {
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.add(theme.accent_button("Add Shader")).clicked()
                && let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Shader Packs", &["zip"])
                    .set_title("Select shader pack(s)")
                    .pick_files()
                {
                    for path in paths {
                        if let Some(name) = path.file_name() {
                            let dest = shaderpacks_dir.join(name);
                            match std::fs::copy(&path, &dest) {
                                Ok(_) => self.shaders_needs_rescan = true,
                                Err(e) => {
                                    self.pending_toasts.push(crate::app::Toast::error(format!("Error copying shader: {e}")));
                                }
                            }
                        }
                    }
                }

            if ui.add(theme.accent_button("Open Folder")).clicked() {
                let _ = std::fs::create_dir_all(shaderpacks_dir);
                let _ = open::that(shaderpacks_dir);
            }
        });
        ui.add_space(4.0);

        if self.installed_shaders.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::SUN)
                        .size(48.0)
                        .color(theme.color("fg_muted")),
                );
                ui.add_space(8.0);
                ui.label(theme.subtext("No shader packs installed."));
                ui.add_space(4.0);
                ui.label(theme.subtext(
                    "Add shader packs with the button above, or drop .zip files into the shaderpacks folder.",
                ));
            });
            return;
        }

        ui.add(
            egui::TextEdit::singleline(&mut self.shaders_filter)
                .hint_text("Filter shaders...")
                .margin(egui::Margin::symmetric(4, 9)),
        );
        ui.add_space(4.0);

        let mut toggle_idx: Option<usize> = None;
        let mut remove_idx: Option<usize> = None;

        let filter_lower = self.shaders_filter.to_lowercase();
        let filtered: Vec<(usize, &ShaderPack)> = self
            .installed_shaders
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                filter_lower.is_empty()
                    || s.title.to_lowercase().contains(&filter_lower)
                    || s.filename.to_lowercase().contains(&filter_lower)
            })
            .collect();

        let outer_frame = crate::ui::helpers::card_frame(ui, theme);

        outer_frame.show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("installed_shaders_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (fi, &(orig_idx, s)) in filtered.iter().enumerate() {
                        if fi > 0 {
                            ui.separator();
                        }
                        let row_resp = ui.horizontal(|ui| {
                            if s.is_folder {
                                ui.small_button(egui_phosphor::regular::FOLDER)
                                    .on_hover_text("Folder (always enabled)");
                            } else {
                                let icon = if s.enabled {
                                    egui_phosphor::regular::CHECK_CIRCLE
                                } else {
                                    egui_phosphor::regular::CIRCLE
                                };
                                if ui
                                    .small_button(icon)
                                    .on_hover_text(if s.enabled { "Disable" } else { "Enable" })
                                    .clicked()
                                {
                                    toggle_idx = Some(orig_idx);
                                }
                            }
                            ui.label(theme.title(&s.title));
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
                                    let detail = if s.is_folder {
                                        format!("{} (folder)", s.filename)
                                    } else {
                                        s.filename.clone()
                                    };
                                    ui.label(theme.subtext(&detail));
                                },
                            );
                        });
                        row_hover_highlight(ui, row_resp.response.rect, theme);
                    }
                });
        });

        if let Some(idx) = toggle_idx {
            let s = &self.installed_shaders[idx];
            let result = if s.enabled {
                shaders::disable_shaderpack(shaderpacks_dir, &s.filename)
            } else {
                shaders::enable_shaderpack(shaderpacks_dir, &s.filename)
            };
            match result {
                Ok(_) => self.shaders_needs_rescan = true,
                Err(e) => {
                    self.pending_toasts.push(crate::app::Toast::error(format!("Error: {e}")));
                }
            }
        }
        if let Some(idx) = remove_idx {
            self.confirm_shader_delete = Some(idx);
        }

        // ── Shader delete confirmation dialog ────────────────────
        if let Some(del_idx) = self.confirm_shader_delete {
            if del_idx < self.installed_shaders.len() {
                let shader_name = self.installed_shaders[del_idx].title.clone();

                let mut open = true;
                egui::Window::new("Confirm Delete Shader")
                    .id(egui::Id::new(format!("confirm_shader_delete_{del_idx}")))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .open(&mut open)
                    .show(ui.ctx(), |ui| {
                        ui.label(format!("Remove shader pack \"{}\"?", shader_name));
                        ui.label(theme.subtext("This will permanently delete the shader pack file."));
                        ui.add_space(8.0);
                        let row_h = ui.spacing().interact_size.y + 4.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                            |ui| {
                                if ui.add(theme.danger_button("Delete")).clicked() {
                                    let s = &self.installed_shaders[del_idx];
                                    match shaders::remove_shaderpack(shaderpacks_dir, &s.filename) {
                                        Ok(()) => self.shaders_needs_rescan = true,
                                        Err(e) => {
                                            self.pending_toasts.push(crate::app::Toast::error(format!("Error: {e}")));
                                        }
                                    }
                                    self.confirm_shader_delete = None;
                                }
                                if ui.button("Cancel").clicked() {
                                    self.confirm_shader_delete = None;
                                }
                            });
                    });

                if !open {
                    self.confirm_shader_delete = None;
                }
            } else {
                self.confirm_shader_delete = None;
            }
        }
    }
}
