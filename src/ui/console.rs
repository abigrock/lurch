use crate::app::RunningProcess;
use crate::core::MutexExt;
use crate::ui::helpers::tab_button;
use eframe::egui;

#[derive(Default)]
pub struct ConsoleView {
    pub active_instance_id: Option<String>,
    confirm_kill: Option<String>,
}

impl ConsoleView {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        theme: &crate::theme::Theme,
        running_processes: &mut Vec<RunningProcess>,
    ) {
        if running_processes.is_empty() {
            self.show_empty(ui, theme);
            return;
        }

        // ── Unified Action Bar ───────────────────────────────────────
        let mut tab_to_remove: Option<String> = None;
        let mut kill_process = false;

        ui.horizontal(|ui| {
            // ── Left: scrollable instance tabs (bounded width) ──
            ui.style_mut().always_scroll_the_only_direction = true;
            let controls_reserve = 300.0;
            let tabs_max_w = (ui.available_width() - controls_reserve).max(100.0);

            let mut switch_to: Option<String> = None;
            egui::ScrollArea::horizontal()
                .id_salt("console_tabs_scroll")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .max_width(tabs_max_w)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let default_spacing = ui.spacing().item_spacing.x;
                        for rp in running_processes.iter() {
                            let is_active =
                                self.active_instance_id.as_deref() == Some(&rp.instance_id);
                            let status_icon = if rp.is_alive() {
                                egui_phosphor::regular::PLAY_CIRCLE
                            } else {
                                egui_phosphor::regular::STOP_CIRCLE
                            };
                            let tab_label = format!("{status_icon} {}", rp.instance_name);

                            if tab_button(ui, &tab_label, is_active, theme) {
                                switch_to = Some(rp.instance_id.clone());
                            }
                            if !rp.is_alive() {
                                // Tight spacing to group X with its tab
                                ui.spacing_mut().item_spacing.x = 2.0;
                                let err = theme.color("error");
                                let x_clicked = ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(egui_phosphor::regular::X)
                                                .color(err)
                                                .strong(),
                                        )
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::new(1.0, err))
                                        .corner_radius(egui::CornerRadius::same(6))
                                        .min_size(egui::vec2(0.0, crate::theme::BUTTON_HEIGHT)),
                                    )
                                    .clicked();
                                ui.spacing_mut().item_spacing.x = default_spacing;
                                // Extra gap after X to separate from next tab group
                                ui.add_space(8.0);
                                if x_clicked {
                                    tab_to_remove = Some(rp.instance_id.clone());
                                }
                            }
                        }
                    });
                });
            if let Some(id) = switch_to {
                self.active_instance_id = Some(id);
            }

            ui.separator();

            // ── Right: fixed controls ──
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if running_processes.iter().any(|rp| !rp.is_alive()) {
                    let lbl = format!("{} Clear Finished", egui_phosphor::regular::BROOM);
                    if ui.add(theme.ghost_button(&lbl)).clicked() {
                        running_processes.retain(|rp| rp.is_alive());
                        if let Some(ref active_id) = self.active_instance_id {
                            if !running_processes
                                .iter()
                                .any(|rp| rp.instance_id == *active_id)
                            {
                                self.active_instance_id =
                                    running_processes.first().map(|rp| rp.instance_id.clone());
                            }
                        }
                    }
                }

                let active_id = self
                    .active_instance_id
                    .clone()
                    .or_else(|| running_processes.first().map(|rp| rp.instance_id.clone()));

                if let Some(ref active_id) = active_id {
                    if let Some(rp) = running_processes
                        .iter()
                        .find(|rp| rp.instance_id == *active_id)
                    {
                        let is_running = rp
                            .process
                            .as_ref()
                            .map(|p| p.lock_or_recover().running)
                            .unwrap_or(false);

                        if is_running {
                            let kill_lbl = format!("{} Kill", egui_phosphor::regular::SKULL);
                            if ui.add(theme.danger_button(&kill_lbl)).clicked() {
                                self.confirm_kill = Some(active_id.clone());
                            }
                        }
                    }
                }

                if let Some(ref active_id) = active_id {
                    if let Some(rp) = running_processes
                        .iter_mut()
                        .find(|rp| rp.instance_id == *active_id)
                    {
                        ui.checkbox(&mut rp.auto_scroll, "Auto-scroll");
                    }
                }
            });
        });

        // ── Kill confirmation dialog ────────────────────────────────
        if let Some(ref kill_id) = self.confirm_kill.clone() {
            let inst_name = running_processes
                .iter()
                .find(|rp| rp.instance_id == *kill_id)
                .map(|rp| rp.instance_name.clone())
                .unwrap_or_default();

            let mut open = true;
            egui::Window::new("Confirm Kill")
                .id(egui::Id::new(format!("confirm_kill_{kill_id}")))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Kill \"{}\"?", inst_name));
                    ui.label(theme.subtext("This will forcefully terminate the running instance."));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                theme.danger_button(&format!(
                                    "{} Kill",
                                    egui_phosphor::regular::SKULL
                                )),
                            )
                            .clicked()
                        {
                            kill_process = true;
                            self.confirm_kill = None;
                        }
                        if ui.add(theme.ghost_button("Cancel")).clicked() {
                            self.confirm_kill = None;
                        }
                    });
                });

            if !open {
                self.confirm_kill = None;
            }
        }

        // Handle tab removal
        if let Some(ref remove_id) = tab_to_remove {
            running_processes.retain(|rp| rp.instance_id != *remove_id);
            if self.active_instance_id.as_deref() == Some(remove_id) {
                self.active_instance_id =
                    running_processes.first().map(|rp| rp.instance_id.clone());
            }
        }

        // Execute kill
        if kill_process {
            if let Some(ref active_id) = self.active_instance_id {
                if let Some(rp) = running_processes
                    .iter()
                    .find(|rp| rp.instance_id == *active_id)
                {
                    if let Some(proc) = &rp.process {
                        proc.lock_or_recover().kill();
                    }
                }
            }
        }

        // ── Resolve active process ───────────────────────────────────
        let active_id = self
            .active_instance_id
            .clone()
            .or_else(|| running_processes.first().map(|rp| rp.instance_id.clone()));

        let Some(active_id) = active_id else {
            self.show_empty(ui, theme);
            return;
        };

        let Some(idx) = running_processes
            .iter()
            .position(|rp| rp.instance_id == active_id)
        else {
            self.show_empty(ui, theme);
            return;
        };

        ui.separator();
        ui.add_space(4.0);

        // ── Contextual Status Banner ─────────────────────────────────
        {
            let (msg, done, error) = {
                let p = running_processes[idx].progress.lock_or_recover();
                (p.message.clone(), p.done, p.error.clone())
            };

            if let Some(err) = error {
                let color = theme.color("error");
                ui.label(
                    egui::RichText::new(format!(
                        "{} Error: {err}",
                        egui_phosphor::regular::WARNING
                    ))
                    .color(color),
                );
                ui.add_space(4.0);
            } else if !done {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(theme.color("accent")));
                    ui.label(theme.subtext(&msg));
                });
                ui.add_space(4.0);
            }
        }

        // ── Log Output ──────────────────────────────────────────────
        if let Some(proc) = &running_processes[idx].process {
            let auto_scroll = running_processes[idx].auto_scroll;
            let scroll_id = active_id.clone();
            let proc = proc.clone();
            theme.code_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("console_scroll", &scroll_id))
                    .auto_shrink([false, false])
                    .stick_to_bottom(auto_scroll)
                    .show(ui, |ui| {
                        let s = proc.lock_or_recover();
                        for line in &s.log_lines {
                            ui.label(
                                egui::RichText::new(line).font(crate::theme::Theme::mono_font()),
                            );
                        }
                    });
            });
        }
    }

    fn show_empty(&self, ui: &mut egui::Ui, theme: &crate::theme::Theme) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new(egui_phosphor::regular::TERMINAL_WINDOW)
                    .size(48.0)
                    .color(theme.color("fg_muted")),
            );
            ui.add_space(8.0);
            ui.label(theme.subtext("Launch an instance to see output here."));
        });
    }
}
