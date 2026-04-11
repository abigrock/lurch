use crate::app::RunningProcess;
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
        theme: Option<&crate::theme::Theme>,
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
            // Left: instance tabs
            let mut switch_to: Option<String> = None;
            for rp in running_processes.iter() {
                let is_active = self.active_instance_id.as_deref() == Some(&rp.instance_id);
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
                    let x_clicked = if let Some(t) = theme {
                        ui.add(t.ghost_button(egui_phosphor::regular::X)).clicked()
                    } else {
                        ui.small_button(egui_phosphor::regular::X).clicked()
                    };
                    if x_clicked {
                        tab_to_remove = Some(rp.instance_id.clone());
                    }
                }
            }
            if let Some(id) = switch_to {
                self.active_instance_id = Some(id);
            }

            // Right: controls
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Clear Finished (right-most)
                if running_processes.iter().any(|rp| !rp.is_alive()) {
                    let lbl = format!("{} Clear Finished", egui_phosphor::regular::BROOM);
                    let clicked = if let Some(t) = theme {
                        ui.add(t.ghost_button(&lbl)).clicked()
                    } else {
                        ui.button(&lbl).clicked()
                    };
                    if clicked {
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

                // Resolve active for controls
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
                            .map(|p| p.lock().unwrap().running)
                            .unwrap_or(false);

                        if is_running {
                            // Inline kill confirmation
                            if self.confirm_kill.as_deref() == Some(active_id.as_str()) {
                                let cancel_clicked = if let Some(t) = theme {
                                    ui.add(t.ghost_button("Cancel")).clicked()
                                } else {
                                    ui.button("Cancel").clicked()
                                };
                                if cancel_clicked {
                                    self.confirm_kill = None;
                                }
                                let confirm_clicked = if let Some(t) = theme {
                                    ui.add(t.danger_button(&format!(
                                        "{} Confirm Kill",
                                        egui_phosphor::regular::SKULL
                                    )))
                                    .clicked()
                                } else {
                                    ui.button(
                                        egui::RichText::new(format!(
                                            "{} Confirm Kill",
                                            egui_phosphor::regular::SKULL
                                        ))
                                        .color(egui::Color32::RED),
                                    )
                                    .clicked()
                                };
                                if confirm_clicked {
                                    kill_process = true;
                                    self.confirm_kill = None;
                                }
                            } else {
                                let kill_lbl = format!("{} Kill", egui_phosphor::regular::SKULL);
                                let clicked = if let Some(t) = theme {
                                    ui.add(t.ghost_button(&kill_lbl)).clicked()
                                } else {
                                    ui.button(&kill_lbl).clicked()
                                };
                                if clicked {
                                    self.confirm_kill = Some(active_id.clone());
                                }
                            }
                        }
                    }
                }

                // Auto-scroll checkbox
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
                        proc.lock().unwrap().kill();
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
                let p = running_processes[idx].progress.lock().unwrap();
                (p.message.clone(), p.done, p.error.clone())
            };

            if let Some(err) = error {
                let color = theme
                    .map(|t| t.color("error"))
                    .unwrap_or(egui::Color32::RED);
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
                    if let Some(t) = theme {
                        ui.add(egui::Spinner::new().color(t.color("accent")));
                    } else {
                        ui.spinner();
                    }
                    if let Some(t) = theme {
                        ui.label(t.subtext(&msg));
                    } else {
                        ui.weak(&msg);
                    }
                });
                ui.add_space(4.0);
            }
        }

        // ── Log Output ──────────────────────────────────────────────
        if let Some(proc) = &running_processes[idx].process {
            let lines = {
                let s = proc.lock().unwrap();
                s.log_lines.clone()
            };

            let auto_scroll = running_processes[idx].auto_scroll;
            let scroll_id = active_id.clone();
            let show_scroll = |ui: &mut egui::Ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("console_scroll", &scroll_id))
                    .auto_shrink([false, false])
                    .stick_to_bottom(auto_scroll)
                    .show(ui, |ui| {
                        for line in &lines {
                            if theme.is_some() {
                                ui.label(
                                    egui::RichText::new(line)
                                        .font(crate::theme::Theme::mono_font()),
                                );
                            } else {
                                ui.monospace(line);
                            }
                        }
                    });
            };

            if let Some(t) = theme {
                t.code_frame().show(ui, show_scroll);
            } else {
                show_scroll(ui);
            }
        }
    }

    fn show_empty(&self, ui: &mut egui::Ui, theme: Option<&crate::theme::Theme>) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            if let Some(t) = theme {
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::TERMINAL_WINDOW)
                        .size(48.0)
                        .color(t.color("fg_muted")),
                );
                ui.add_space(8.0);
                ui.label(t.subtext("Launch an instance to see output here."));
            } else {
                ui.label(egui::RichText::new(egui_phosphor::regular::TERMINAL_WINDOW).size(48.0));
                ui.add_space(8.0);
                ui.weak("Launch an instance to see output here.");
            }
        });
    }
}
