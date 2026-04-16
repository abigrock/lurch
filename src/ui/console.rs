use crate::app::RunningProcess;
use crate::core::MutexExt;
use crate::ui::helpers::closable_tab_button;
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
            ui.spacing_mut().item_spacing.x = 0.0;
            egui::ScrollArea::horizontal()
                .id_salt("console_tabs_scroll")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .max_width(tabs_max_w)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for rp in running_processes.iter() {
                            let is_active =
                                self.active_instance_id.as_deref() == Some(&rp.instance_id);
                            let status_icon = if rp.is_alive() {
                                egui_phosphor::regular::PLAY_CIRCLE
                            } else {
                                egui_phosphor::regular::STOP_CIRCLE
                            };
                            let tab_label = format!("{status_icon} {}", rp.instance_name);

                            let (clicked, close) = closable_tab_button(ui, &tab_label, is_active, rp.is_alive(), theme);
                            if clicked {
                                switch_to = Some(rp.instance_id.clone());
                            }
                            if close {
                                // Check if we should cancel presetup or show kill confirmation
                                let mut should_cancel_presetup = false;
                                let mut should_kill_process = false;
                                
                                // First check if process exists (no lock needed for this check)
                                if rp.process.is_none() {
                                    // No process yet, check if we're in presetup phase
                                    let progress = rp.progress.lock_or_recover();
                                    if !progress.done && progress.error.is_none() {
                                        should_cancel_presetup = true;
                                    }
                                } else if rp.is_alive() {
                                    should_kill_process = true;
                                }
                                
                                if should_cancel_presetup {
                                    // Cancel ongoing presetup
                                    let mut p = rp.progress.lock_or_recover();
                                    p.cancelled = true;
                                    p.done = true;
                                    p.error = Some("Cancelled by user".to_string());
                                } else if should_kill_process {
                                    // Process is running, show kill confirmation
                                    self.confirm_kill = Some(rp.instance_id.clone());
                                } else {
                                    // Presetup is complete or failed, safe to remove tab
                                    tab_to_remove = Some(rp.instance_id.clone());
                                }
                            }
                        }
                    });
                });
            if let Some(id) = switch_to {
                self.active_instance_id = Some(id);
            }

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // ── Right: fixed controls ──
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;

                if running_processes.iter().any(|rp| !rp.is_alive()) {
                    let lbl = format!("{} Clear Finished", egui_phosphor::regular::BROOM);
                    if ui.add(theme.ghost_button(&lbl)).clicked() {
                        running_processes.retain(|rp| rp.is_alive());
                        if let Some(ref active_id) = self.active_instance_id
                            && !running_processes
                                .iter()
                                .any(|rp| rp.instance_id == *active_id)
                        {
                            self.active_instance_id =
                                running_processes.first().map(|rp| rp.instance_id.clone());
                        }
                    }
                }

                let active_id = self
                    .active_instance_id
                    .clone()
                    .or_else(|| running_processes.first().map(|rp| rp.instance_id.clone()));

                if let Some(ref active_id) = active_id
                    && let Some(rp) = running_processes
                        .iter_mut()
                        .find(|rp| rp.instance_id == *active_id)
                {
                    ui.checkbox(&mut rp.auto_scroll, "Auto-scroll");
                    ui.checkbox(&mut rp.line_wrap, "Line wrap");
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
        if kill_process
            && let Some(ref active_id) = self.active_instance_id
            && let Some(rp) = running_processes
                .iter()
                .find(|rp| rp.instance_id == *active_id)
            && let Some(proc) = &rp.process
        {
            proc.lock_or_recover().kill();
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

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(theme.ghost_button("Cancel"))
                            .on_hover_text("Cancel launch process")
                            .clicked()
                        {
                            let mut p = running_processes[idx].progress.lock_or_recover();
                            p.cancelled = true;
                            p.done = true;
                            p.error = Some("Cancelled by user".to_string());
                        }
                    });
                });
                ui.add_space(4.0);
            }
        }

        // ── Log Output ──────────────────────────────────────────────
        if let Some(proc) = &running_processes[idx].process {
            let auto_scroll = running_processes[idx].auto_scroll;
            let line_wrap = running_processes[idx].line_wrap;
            let scroll_id = active_id.clone();
            let proc = proc.clone();
            theme.code_frame().show(ui, |ui| {
                let scroll_area = if line_wrap {
                    egui::ScrollArea::vertical()
                } else {
                    egui::ScrollArea::both()
                };
                scroll_area
                    .id_salt(("console_scroll", &scroll_id))
                    .auto_shrink([false, false])
                    .stick_to_bottom(auto_scroll)
                    .show(ui, |ui| {
                        if !line_wrap {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        }
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
