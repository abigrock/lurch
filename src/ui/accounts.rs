use eframe::egui;
use std::sync::{Arc, Mutex};

use crate::core::BgTaskSlot;
use crate::core::account::{self, Account, AccountStore};

enum AuthFlowState {
    Idle,
    /// Waiting for user to enter code at microsoft.com/devicelogin
    WaitingForUser {
        user_code: String,
        verification_uri: String,
        result: BgTaskSlot<Account>,
    },
    /// Auth completed with an error
    Error(String),
}

pub struct AccountsView {
    auth_state: AuthFlowState,
    offline_username: String,
    confirm_remove: Option<String>,
}

impl Default for AccountsView {
    fn default() -> Self {
        Self {
            auth_state: AuthFlowState::Idle,
            offline_username: String::new(),
            confirm_remove: None,
        }
    }
}

impl AccountsView {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        store: &mut AccountStore,
        theme: &crate::theme::Theme,
    ) {
        ui.label(crate::ui::helpers::section_heading("Accounts", theme));
        ui.separator();
        ui.add_space(8.0);

        // --- Account list ---
        let uuids: Vec<String> = store.accounts.iter().map(|a| a.uuid.clone()).collect();

        let mut set_active_uuid: Option<String> = None;
        let mut remove_uuid: Option<String> = None;
        let mut refresh_uuid: Option<String> = None;

        for uuid in &uuids {
            if let Some(account) = store.accounts.iter().find(|a| &a.uuid == uuid) {
                let is_active = account.active;
                let is_offline = account.offline;
                let username = account.username.clone();
                let uuid_str = account.uuid.clone();

                let avatar_url = {
                    let identifier = if is_offline { &username } else { &uuid_str };
                    format!("https://mc-heads.net/avatar/{}/64", identifier)
                };

                let show_card = |ui: &mut egui::Ui| {
                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                            // Avatar
                            ui.add(
                                egui::Image::new(&avatar_url)
                                    .fit_to_exact_size(egui::vec2(32.0, 32.0)),
                            );
                            ui.vertical(|ui| {
                                let display_name = if is_offline {
                                    format!("{username} (Offline)")
                                } else {
                                    username.clone()
                                };
                                if is_active {
                                    ui.label(theme.title(&display_name));
                                    ui.label(
                                        egui::RichText::new("Active")
                                            .small()
                                            .color(theme.color("success")),
                                    );
                                } else {
                                    ui.label(theme.title(&display_name));
                                }
                                ui.label(theme.subtext(&uuid_str));
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(theme.ghost_button(egui_phosphor::regular::TRASH))
                                        .on_hover_text("Remove account")
                                        .clicked()
                                    {
                                        remove_uuid = Some(uuid_str.clone());
                                    }
                                    if !is_offline
                                        && ui
                                            .add(theme.ghost_button(
                                                egui_phosphor::regular::ARROWS_CLOCKWISE,
                                            ))
                                            .on_hover_text("Refresh session")
                                            .clicked()
                                    {
                                        refresh_uuid = Some(uuid_str.clone());
                                    }
                                    if !is_active
                                        && ui.add(theme.accent_button("Set Active")).clicked()
                                    {
                                        set_active_uuid = Some(uuid_str.clone());
                                    }
                                },
                            );
                        },
                    );
                };

                let mut frame = theme.card_frame();
                if is_active {
                    frame = frame.stroke(egui::Stroke::new(1.5, theme.color("accent")));
                }
                frame.show(ui, show_card);
            }
        }

        // Empty state
        if store.accounts.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(32.0);
                ui.label(
                    egui::RichText::new(egui_phosphor::regular::USERS)
                        .size(48.0)
                        .weak(),
                );
                ui.add_space(8.0);
                ui.label(theme.title("No accounts added"));
                ui.add_space(4.0);
                ui.label(theme.subtext("Add a Microsoft or offline account to start playing."));
            });
        }

        // Apply deferred mutations
        if let Some(uuid) = set_active_uuid {
            store.set_active(&uuid);
            let _ = store.save();
        }

        if let Some(uuid) = remove_uuid {
            self.confirm_remove = Some(uuid);
        }

        if let Some(uuid) = refresh_uuid
            && let Some(account) = store.accounts.iter().find(|a| a.uuid == uuid).cloned()
        {
            let result: BgTaskSlot<Account> = Arc::new(Mutex::new(None));
            let result_clone = Arc::clone(&result);
            let ctx = ui.ctx().clone();

            std::thread::spawn(move || {
                let outcome = account::refresh_account(&account).map_err(|e| e.to_string());
                if let Ok(mut lock) = result_clone.lock() {
                    *lock = Some(outcome);
                }
                ctx.request_repaint();
            });

            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new("refresh_result").with(&uuid), result);
            });
        }

        // Poll any pending refresh results
        let refresh_uuids_to_check: Vec<String> =
            store.accounts.iter().map(|a| a.uuid.clone()).collect();
        for uuid in refresh_uuids_to_check {
            let key = egui::Id::new("refresh_result").with(&uuid);
            #[allow(clippy::type_complexity)]
            let maybe_result: Option<BgTaskSlot<Account>> = ui.ctx().data(|d| d.get_temp(key));
            if let Some(arc) = maybe_result {
                let finished = arc
                    .lock()
                    .ok()
                    .and_then(|g| g.as_ref().map(|r| r.is_ok() || r.is_err()))
                    .unwrap_or(false);
                if finished {
                    let result = arc.lock().ok().and_then(|mut g| g.take());
                    ui.ctx().data_mut(|d| d.remove::<BgTaskSlot<Account>>(key));
                    match result {
                        Some(Ok(updated)) => {
                            store.add_or_update(updated);
                            let _ = store.save();
                        }
                        Some(Err(e)) => {
                            self.auth_state = AuthFlowState::Error(format!("Refresh failed: {e}"));
                        }
                        None => {}
                    }
                }
            }
        }

        ui.add_space(8.0);

        // --- Auth flow UI ---
        // Extract data needed for WaitingForUser before the match to avoid double-borrow
        #[allow(clippy::type_complexity)]
        let waiting_data: Option<(String, String, BgTaskSlot<Account>)> =
            if let AuthFlowState::WaitingForUser {
                user_code,
                verification_uri,
                result,
            } = &self.auth_state
            {
                Some((
                    user_code.clone(),
                    verification_uri.clone(),
                    Arc::clone(result),
                ))
            } else {
                None
            };

        // Show error state (styled alert) and allow retry
        if let AuthFlowState::Error(msg) = &self.auth_state {
            let msg = msg.clone();
            let error_frame = egui::Frame::new()
                .fill(theme.color("error").linear_multiply(0.15))
                .stroke(egui::Stroke::new(1.0, theme.color("error")))
                .inner_margin(egui::Margin::same(12))
                .corner_radius(egui::CornerRadius::same(6));
            error_frame.show(ui, |ui| {
                let row_h = ui.spacing().interact_size.y + 4.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), row_h),
                    egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                    |ui| {
                        ui.label(
                            egui::RichText::new(egui_phosphor::regular::WARNING_CIRCLE)
                                .color(theme.color("error")),
                        );
                        ui.label(egui::RichText::new(&msg).color(theme.color("error")));
                    },
                );
            });
            ui.add_space(4.0);
            if ui.add(theme.ghost_button("Dismiss")).clicked() {
                self.auth_state = AuthFlowState::Idle;
            }
            ui.add_space(8.0);
        }

        match &self.auth_state {
            AuthFlowState::Idle | AuthFlowState::Error(_) => {
                if ui
                    .add(theme.accent_button(&format!(
                        "{} Add Microsoft Account",
                        egui_phosphor::regular::KEY
                    )))
                    .clicked()
                {
                    self.start_microsoft_login(ui);
                }

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // --- Offline account section ---
                let offline_frame = crate::ui::helpers::card_frame(ui, theme);
                offline_frame.show(ui, |ui| {
                    ui.label(theme.section_header("Add Offline Account"));
                    ui.add_space(4.0);
                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                            let response = ui.add_sized(
                                [180.0, 32.0],
                                egui::TextEdit::singleline(&mut self.offline_username)
                                    .hint_text("Username")
                                    .margin(egui::Margin::symmetric(4, 9)),
                            );
                            let can_add = !self.offline_username.trim().is_empty();
                            let add_clicked = ui
                                .add_enabled(can_add, theme.accent_button("Add"))
                                .clicked();
                            let enter_pressed = response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter));

                            if (add_clicked || enter_pressed) && can_add {
                                let username = self.offline_username.trim().to_string();
                                let account = Account::offline(username);
                                store.add_or_update(account);
                                let _ = store.save();
                                self.offline_username.clear();
                            }
                        },
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} Offline accounts cannot join online-mode servers.",
                            egui_phosphor::regular::WARNING_CIRCLE
                        ))
                        .small()
                        .weak(),
                    );
                });
            }

            AuthFlowState::WaitingForUser { .. } => {
                // waiting_data is guaranteed Some here
                let (user_code, verification_uri, result) = waiting_data.unwrap();

                // Check for completed auth result each frame
                let finished_result: Option<Result<Account, String>> =
                    result.lock().ok().and_then(|mut g| g.take());

                if let Some(outcome) = finished_result {
                    match outcome {
                        Ok(account) => {
                            store.add_or_update(account);
                            let _ = store.save();
                            self.auth_state = AuthFlowState::Idle;
                            return;
                        }
                        Err(e) => {
                            self.auth_state = AuthFlowState::Error(e);
                            return;
                        }
                    }
                }

                // Show login card
                let mut cancel_clicked = false;
                let frame = egui::Frame::default()
                    .inner_margin(egui::Margin::same(16i8))
                    .fill(theme.color("bg_secondary"))
                    .stroke(egui::Stroke::new(1.0, theme.color("surface")))
                    .corner_radius(egui::CornerRadius::same(8));
                frame.show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(theme.title("Microsoft Login").heading());
                        ui.add_space(8.0);
                        ui.label("1. Go to:");
                        ui.hyperlink_to(&verification_uri, &verification_uri);
                        ui.add_space(4.0);
                        ui.label("2. Enter this code:");
                        let code_text = egui::RichText::new(&user_code)
                            .monospace()
                            .size(28.0)
                            .strong()
                            .color(theme.color("fg"));
                        ui.label(code_text);
                        ui.add_space(4.0);
                        if ui
                            .add(theme.accent_button(&format!(
                                "{} Copy Code",
                                egui_phosphor::regular::COPY
                            )))
                            .clicked()
                        {
                            ui.output_mut(|o| {
                                o.commands
                                    .push(egui::OutputCommand::CopyText(user_code.clone()))
                            });
                        }
                        ui.add_space(8.0);
                        ui.add(egui::Spinner::new().color(theme.color("accent")));
                        ui.add_space(8.0);
                        if ui.add(theme.ghost_button("Cancel")).clicked() {
                            cancel_clicked = true;
                        }
                    });
                });

                if cancel_clicked {
                    self.auth_state = AuthFlowState::Idle;
                }

                ui.ctx().request_repaint();
            }
        }

        // Confirm-remove dialog
        if let Some(ref uuid) = self.confirm_remove.clone() {
            let username = store
                .accounts
                .iter()
                .find(|a| a.uuid == *uuid)
                .map(|a| a.username.clone())
                .unwrap_or_else(|| uuid.clone());

            let mut open = true;
            egui::Window::new("Confirm Remove")
                .id(egui::Id::new(format!("confirm_remove_{uuid}")))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Remove account \"{username}\"?"));
                    ui.label(theme.subtext("You can re-add this account later."));
                    ui.add_space(8.0);
                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                            let remove_clicked = ui.add(theme.danger_button("Remove")).clicked();
                            if remove_clicked {
                                store.remove(uuid);
                                let _ = store.save();
                                self.confirm_remove = None;
                            }
                            if ui.add(theme.ghost_button("Cancel")).clicked() {
                                self.confirm_remove = None;
                            }
                        },
                    );
                });

            if !open {
                self.confirm_remove = None;
            }
        }
    }

    fn start_microsoft_login(&mut self, ui: &mut egui::Ui) {
        let result: BgTaskSlot<Account> = Arc::new(Mutex::new(None));
        let result_clone = Arc::clone(&result);
        let ctx = ui.ctx().clone();

        match account::request_device_code() {
            Ok(device_resp) => {
                let user_code = device_resp.user_code.clone();
                let verification_uri = device_resp.verification_uri.clone();
                let device_code = device_resp.device_code.clone();
                let interval = device_resp.interval;

                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(interval));
                        match account::poll_device_code_token(&device_code) {
                            Ok(Some(ms_token)) => {
                                let outcome =
                                    account::complete_auth(&ms_token).map_err(|e| e.to_string());
                                if let Ok(mut lock) = result_clone.lock() {
                                    *lock = Some(outcome);
                                }
                                ctx.request_repaint();
                                break;
                            }
                            Ok(None) => {
                                ctx.request_repaint();
                            }
                            Err(e) => {
                                if let Ok(mut lock) = result_clone.lock() {
                                    *lock = Some(Err(e.to_string()));
                                }
                                ctx.request_repaint();
                                break;
                            }
                        }
                    }
                });

                self.auth_state = AuthFlowState::WaitingForUser {
                    user_code,
                    verification_uri,
                    result,
                };
            }
            Err(e) => {
                self.auth_state = AuthFlowState::Error(format!("Failed to start login: {e}"));
            }
        }
    }
}
