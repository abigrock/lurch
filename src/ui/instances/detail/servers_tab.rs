use eframe::egui;

use super::InstanceDetailView;
use crate::core::servers::{self, Server};
use crate::ui::helpers::row_hover_highlight;

impl InstanceDetailView {
    pub(super) fn show_servers_tab(
        &mut self,
        ui: &mut egui::Ui,
        servers_dat: &std::path::Path,
        theme: &crate::theme::Theme,
    ) {
        ui.add_space(4.0);

        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            let is_editing = self.editing_server_idx.is_some();
            let form_label = if is_editing { "Edit Server" } else { "Add Server" };
            ui.label(theme.subtext(form_label));
        });
        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            ui.add_sized(
                [160.0, 32.0],
                egui::TextEdit::singleline(&mut self.server_edit_name)
                    .hint_text("Server name")
                    .margin(egui::Margin::symmetric(4, 9)),
            );
            // Reserve space for Save/Cancel buttons (~170px) so the address field doesn't overflow
            let btn_reserve = 170.0;
            let addr_w = (ui.available_width() - btn_reserve).max(80.0);
            ui.add_sized(
                [addr_w, 32.0],
                egui::TextEdit::singleline(&mut self.server_edit_ip)
                    .hint_text("Address (e.g. play.example.com)")
                    .margin(egui::Margin::symmetric(4, 9)),
            );

            let can_save =
                !self.server_edit_name.trim().is_empty() && !self.server_edit_ip.trim().is_empty();

            if let Some(edit_idx) = self.editing_server_idx {
                if ui.add_enabled(can_save, theme.accent_button("Save")).clicked() && can_save {
                    if edit_idx < self.server_list.len() {
                        self.server_list[edit_idx].name = self.server_edit_name.trim().to_string();
                        self.server_list[edit_idx].ip = self.server_edit_ip.trim().to_string();
                        let _ = servers::write_servers(servers_dat, &self.server_list);
                    }
                    self.server_edit_name.clear();
                    self.server_edit_ip.clear();
                    self.editing_server_idx = None;
                    self.servers_needs_rescan = true;
                }
                if ui.add(theme.ghost_button("Cancel")).clicked() {
                    self.server_edit_name.clear();
                    self.server_edit_ip.clear();
                    self.editing_server_idx = None;
                }
            } else {
                if ui.add_enabled(can_save, theme.accent_button("Add")).clicked() && can_save {
                    self.server_list.push(Server {
                        name: self.server_edit_name.trim().to_string(),
                        ip: self.server_edit_ip.trim().to_string(),
                        accept_textures: None,
                        hidden: false,
                    });
                    let _ = servers::write_servers(servers_dat, &self.server_list);
                    self.server_edit_name.clear();
                    self.server_edit_ip.clear();
                    self.servers_needs_rescan = true;
                }
            }
        });
        ui.add_space(4.0);

        if self.server_list.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::WIFI_HIGH)
                        .size(48.0)
                        .color(theme.color("fg_muted")),
                );
                ui.add_space(8.0);
                ui.label(theme.subtext("No servers configured."));
                ui.add_space(4.0);
                ui.label(theme.subtext(
                    "Add a server above. Changes are saved to servers.dat for use in-game.",
                ));
            });
            return;
        }

        let mut remove_idx: Option<usize> = None;
        let mut edit_idx: Option<usize> = None;
        let mut move_up_idx: Option<usize> = None;
        let mut move_down_idx: Option<usize> = None;
        let server_count = self.server_list.len();

        let outer_frame = crate::ui::helpers::card_frame(ui, theme);

        outer_frame.show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("servers_list_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (idx, server) in self.server_list.iter().enumerate() {
                        if idx > 0 {
                            ui.separator();
                        }
                        let row_h = ui.spacing().interact_size.y + 4.0;
                        let row_resp = ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                            |ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let up_enabled = idx > 0;
                                if ui
                                    .add_enabled(up_enabled, theme.ghost_button(egui_phosphor::regular::CARET_UP))
                                    .on_hover_text("Move up")
                                    .clicked()
                                {
                                    move_up_idx = Some(idx);
                                }
                                let down_enabled = idx + 1 < server_count;
                                if ui
                                    .add_enabled(down_enabled, theme.ghost_button(egui_phosphor::regular::CARET_DOWN))
                                    .on_hover_text("Move down")
                                    .clicked()
                                {
                                    move_down_idx = Some(idx);
                                }
                            });
                            ui.vertical(|ui| {
                                ui.add(egui::Label::new(theme.title(&server.name)).truncate());
                                ui.add(egui::Label::new(theme.subtext(&server.ip)).truncate());
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.add(theme.danger_button(egui_phosphor::regular::TRASH))
                                        .on_hover_text("Remove server")
                                        .clicked()
                                    {
                                        remove_idx = Some(idx);
                                    }
                                    if ui.add(theme.accent_button("Edit")).clicked() {
                                        edit_idx = Some(idx);
                                    }
                                },
                            );
                        });
                        row_hover_highlight(ui, row_resp.response.rect, theme);
                    }
                });
        });

        if let Some(idx) = remove_idx {
            self.confirm_server_delete = Some(idx);
        }

        // ── Server delete confirmation dialog ────────────────────
        if let Some(del_idx) = self.confirm_server_delete {
            if del_idx < self.server_list.len() {
                let server_name = self.server_list[del_idx].name.clone();

                let mut open = true;
                egui::Window::new("Confirm Delete Server")
                    .id(egui::Id::new(format!("confirm_server_delete_{del_idx}")))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .open(&mut open)
                    .show(ui.ctx(), |ui| {
                        ui.label(format!("Remove server \"{}\"?", server_name));
                        ui.label(theme.subtext("This will remove the server from servers.dat."));
                        ui.add_space(8.0);
                        let row_h = ui.spacing().interact_size.y + 4.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                            |ui| {
                            if ui.add(theme.danger_button("Delete")).clicked() {
                                self.server_list.remove(del_idx);
                                let _ = servers::write_servers(servers_dat, &self.server_list);
                                self.servers_needs_rescan = true;
                                if self.editing_server_idx == Some(del_idx) {
                                    self.editing_server_idx = None;
                                    self.server_edit_name.clear();
                                    self.server_edit_ip.clear();
                                }
                                self.confirm_server_delete = None;
                            }
                            if ui.add(theme.ghost_button("Cancel")).clicked() {
                                self.confirm_server_delete = None;
                            }
                        },
                        );
                    });

                if !open {
                    self.confirm_server_delete = None;
                }
            } else {
                self.confirm_server_delete = None;
            }
        }
        if let Some(idx) = edit_idx {
            self.server_edit_name = self.server_list[idx].name.clone();
            self.server_edit_ip = self.server_list[idx].ip.clone();
            self.editing_server_idx = Some(idx);
        }
        if let Some(idx) = move_up_idx
            && idx > 0 {
                self.server_list.swap(idx, idx - 1);
                let _ = servers::write_servers(servers_dat, &self.server_list);
                self.servers_needs_rescan = true;
            }
        if let Some(idx) = move_down_idx
            && idx + 1 < self.server_list.len() {
                self.server_list.swap(idx, idx + 1);
                let _ = servers::write_servers(servers_dat, &self.server_list);
                self.servers_needs_rescan = true;
            }
    }
}
