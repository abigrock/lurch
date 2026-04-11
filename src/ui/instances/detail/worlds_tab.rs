use eframe::egui;

use super::InstanceDetailView;
use crate::core::worlds::{self, World};
use crate::ui::helpers::row_hover_highlight;

impl InstanceDetailView {
    pub(super) fn show_worlds_tab(
        &mut self,
        ui: &mut egui::Ui,
        saves_dir: &std::path::Path,
        theme: &crate::theme::Theme,
    ) {
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.add(theme.accent_button("Open Saves Folder")).clicked() {
                let _ = std::fs::create_dir_all(saves_dir);
                let _ = open::that(saves_dir);
            }
        });
        ui.add_space(4.0);

        if self.installed_worlds.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::GLOBE_HEMISPHERE_WEST)
                        .size(48.0)
                        .color(theme.color("fg_muted")),
                );
                ui.add_space(8.0);
                ui.label(theme.subtext("No worlds found."));
                ui.add_space(4.0);
                ui.label(
                    theme.subtext("Worlds will appear here after you create or join one in-game."),
                );
            });
            return;
        }

        ui.add(
            egui::TextEdit::singleline(&mut self.worlds_filter)
                .hint_text("Filter worlds...")
                .margin(egui::Margin::symmetric(4, 9)),
        );
        ui.add_space(4.0);

        let filter_lower = self.worlds_filter.to_lowercase();
        let filtered: Vec<&World> = self
            .installed_worlds
            .iter()
            .filter(|w| {
                filter_lower.is_empty()
                    || w.display_name.to_lowercase().contains(&filter_lower)
                    || w.dir_name.to_lowercase().contains(&filter_lower)
            })
            .collect();

        let outer_frame = crate::ui::helpers::card_frame(ui, theme);

        outer_frame.show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("worlds_list_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (fi, w) in filtered.iter().enumerate() {
                        if fi > 0 {
                            ui.separator();
                        }
                        let row_resp = ui.horizontal(|ui| {
                            ui.label(theme.title(&w.display_name));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(theme.danger_button(egui_phosphor::regular::TRASH))
                                        .on_hover_text("Delete world")
                                        .clicked()
                                    {
                                        self.confirm_world_delete = Some(w.dir_name.clone());
                                    }
                                    let detail = format!(
                                        "{}  {}",
                                        worlds::format_size(w.size_bytes),
                                        w.last_modified,
                                    );
                                    ui.label(theme.subtext(&detail));
                                },
                            );
                        });
                        row_hover_highlight(ui, row_resp.response.rect, theme);
                    }
                });
        });

        if let Some(ref del_name) = self.confirm_world_delete.clone() {
            let display = self
                .installed_worlds
                .iter()
                .find(|w| w.dir_name == *del_name)
                .map(|w| w.display_name.clone())
                .unwrap_or_else(|| del_name.clone());

            let mut open = true;
            egui::Window::new("Confirm Delete World")
                .id(egui::Id::new(format!("confirm_world_delete_{del_name}")))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Delete world \"{}\"?", display));
                    ui.label(
                        theme.subtext("This will permanently delete the world and all save data."),
                    );
                    ui.add_space(8.0);
                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                            if ui.add(theme.danger_button("Delete")).clicked() {
                                match worlds::remove_world(saves_dir, del_name) {
                                    Ok(()) => self.worlds_needs_rescan = true,
                                    Err(e) => {
                                        self.pending_toasts
                                            .push(crate::app::Toast::error(format!("Error: {e}")));
                                    }
                                }
                                self.confirm_world_delete = None;
                            }
                            if ui.add(theme.ghost_button("Cancel")).clicked() {
                                self.confirm_world_delete = None;
                            }
                        },
                    );
                });

            if !open {
                self.confirm_world_delete = None;
            }
        }
    }
}
