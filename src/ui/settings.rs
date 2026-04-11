use crate::core::config::AppConfig;
use crate::core::java::{self, JavaInstall};
use crate::theme::Theme;
use eframe::egui;
use std::sync::{Arc, Mutex};

pub struct SettingsView {
    pub java_version: u32,
    pub available_java_versions: Vec<u32>,
    pub java_versions_fetch: Option<Arc<Mutex<Option<Vec<u32>>>>>,
    pub java_provider: usize,
    pub confirm_java_remove: Option<usize>,
}

impl SettingsView {
    /// Create a new SettingsView and kick off a background fetch for available Java versions.
    pub fn new(ctx: &egui::Context) -> Self {
        let java_versions_fetch: Arc<Mutex<Option<Vec<u32>>>> = Arc::new(Mutex::new(None));
        {
            let slot = Arc::clone(&java_versions_fetch);
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                if let Ok(versions) = java::fetch_available_versions() {
                    *slot.lock().unwrap() = Some(versions);
                    ctx.request_repaint();
                }
            });
        }
        Self {
            java_version: 21,
            available_java_versions: vec![8, 11, 17, 21, 25],
            java_versions_fetch: Some(java_versions_fetch),
            java_provider: 0,
            confirm_java_remove: None,
        }
    }

    /// Poll background java version fetch. Call this every frame.
    pub fn poll(&mut self) {
        if let Some(fetch) = &self.java_versions_fetch {
            let taken = fetch.lock().unwrap().take();
            if let Some(versions) = taken {
                self.available_java_versions = versions;
                self.java_versions_fetch = None;
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        config: &mut AppConfig,
        themes: &[Theme],
        builtin_theme_count: usize,
        current_theme_idx: &mut usize,
        java_installs: &mut Vec<JavaInstall>,
        java_download: &mut Option<Arc<Mutex<crate::app::JavaDownloadState>>>,
        theme: &Theme,
    ) {
    ui.label(crate::ui::helpers::section_heading("Settings", theme));
    ui.separator();
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .id_salt("settings_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // ── Appearance ──
            let header_text = theme.section_header(&format!("{} Appearance", egui_phosphor::regular::PAINT_BRUSH));
            egui::CollapsingHeader::new(header_text)
                .default_open(true)
                .id_salt("settings_appearance")
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    let has_custom = themes.len() > builtin_theme_count;

                    // Helper closure to render a row of theme cards
                    let mut render_theme_cards = |ui: &mut egui::Ui, range: std::ops::Range<usize>| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                            for i in range {
                                let t = &themes[i];
                                let is_selected = i == *current_theme_idx;
                                let card_w = 120.0_f32;
                                let card_h = 72.0_f32;
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(card_w, card_h),
                                    egui::Sense::click(),
                                );

                                if ui.is_rect_visible(rect) {
                                    let bg_color = t.color("bg");
                                    let stroke = if is_selected {
                                        egui::Stroke::new(2.0, t.color("accent"))
                                    } else if response.hovered() {
                                        egui::Stroke::new(1.0, t.color("surface_hover"))
                                    } else {
                                        egui::Stroke::new(1.0, t.color("surface"))
                                    };
                                    ui.painter().rect(rect, egui::CornerRadius::same(8), bg_color, stroke, egui::StrokeKind::Inside);

                                    // Mini color bar showing accent, surface, fg
                                    let swatch_y = rect.top() + 8.0;
                                    let swatch_h = 16.0;
                                    let swatch_w = 24.0;
                                    let swatch_gap = 4.0;
                                    let swatches_total = 3.0 * swatch_w + 2.0 * swatch_gap;
                                    let swatch_start_x = rect.center().x - swatches_total / 2.0;
                                    for (j, color_name) in ["accent", "surface", "fg"].iter().enumerate() {
                                        let sx = swatch_start_x + j as f32 * (swatch_w + swatch_gap);
                                        let swatch_rect = egui::Rect::from_min_size(
                                            egui::pos2(sx, swatch_y),
                                            egui::vec2(swatch_w, swatch_h),
                                        );
                                        ui.painter().rect_filled(swatch_rect, egui::CornerRadius::same(3), t.color(color_name));
                                    }

                                    // Theme name
                                    let name_pos = egui::pos2(rect.center().x, rect.bottom() - 16.0);
                                    let [r, g, b, _] = bg_color.to_array();
                                    let lum = r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114;
                                    let name_color = if lum < 128.0 {
                                        egui::Color32::from_gray(220)
                                    } else {
                                        egui::Color32::from_gray(40)
                                    };
                                    ui.painter().text(
                                        name_pos,
                                        egui::Align2::CENTER_CENTER,
                                        &t.name,
                                        egui::FontId::proportional(11.0),
                                        name_color,
                                    );
                                }

                                if response.clicked() {
                                    *current_theme_idx = i;
                                    config.current_theme = themes[i].name.clone();
                                }
                            }
                        });
                    };

                    // ── Built-in themes ──
                    if has_custom {
                        ui.label(egui::RichText::new("Built-in").size(12.0).color(
                            theme.color("fg_muted")
                        ));
                        ui.add_space(2.0);
                    }
                    render_theme_cards(ui, 0..builtin_theme_count);

                    // ── Custom themes ──
                    if has_custom {
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("Custom").size(12.0).color(
                            theme.color("fg_muted")
                        ));
                        ui.add_space(2.0);
                        render_theme_cards(ui, builtin_theme_count..themes.len());
                    }

                    ui.add_space(4.0);
                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                        let open_lbl = format!("{} Open Themes Folder", egui_phosphor::regular::FOLDER_OPEN);
                        let open_clicked = ui.add(theme.accent_button(&open_lbl)).clicked();
                        if open_clicked
                            && let Ok(dir) = crate::util::paths::themes_dir()
                        {
                            let _ = open::that(dir);
                        }
                        ui.label(theme.subtext("Drop .json theme files here to add custom themes."));
                    });
                });
            ui.add_space(16.0);

            // ── Java Runtime (merged Download + Installations) ──
            let header_text = theme.section_header(&format!("{} Java Runtime", egui_phosphor::regular::COFFEE));
            egui::CollapsingHeader::new(header_text)
                .default_open(true)
                .id_salt("settings_java")
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    // Download progress (if active)
                    if let Some(state) = java_download.as_ref() {
                        let s = state.lock().unwrap();
                        if !s.done {
                            let row_h = ui.spacing().interact_size.y + 4.0;
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), row_h),
                                egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                                |ui| {
                                ui.add(egui::Spinner::new().color(theme.color("accent")));
                                ui.label(&s.message);
                            });
                            ui.add_space(4.0);
                        }
                    }

                    let is_downloading = java_download
                        .as_ref()
                        .is_some_and(|s| !s.lock().unwrap().done);

                    let all_installed_majors: std::collections::HashSet<u32> =
                        java_installs.iter().map(|j| j.major).collect();

                    let provider_installed_majors: std::collections::HashSet<u32> = java_installs
                        .iter()
                        .filter(|j| {
                            j.managed && if self.java_provider == 0 {
                                j.vendor == "Mojang"
                            } else {
                                j.vendor == "Adoptium"
                            }
                        })
                        .map(|j| j.major)
                        .collect();

                    let provider_versions: Vec<u32> = if self.java_provider == 0 {
                        java::mojang_available_versions()
                    } else {
                        self.available_java_versions.clone()
                    };

                    if !provider_versions.contains(&self.java_version)
                        && let Some(&first) = provider_versions.last()
                    {
                        self.java_version = first;
                    }

                    let already_installed = provider_installed_majors.contains(&self.java_version);

                    let mut download_clicked = false;
                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                        ui.label("Provider:");
                        let provider_label = match self.java_provider {
                            0 => "Mojang (Recommended)",
                            _ => "Adoptium",
                        };
                        egui::ComboBox::from_id_salt("java_provider")
                            .selected_text(provider_label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.java_provider, 0, "Mojang (Recommended)");
                                ui.selectable_value(&mut self.java_provider, 1, "Adoptium");
                            });
                    });

                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                        ui.label("Version:");
                        egui::ComboBox::from_id_salt("java_download_version")
                            .selected_text(format!("Java {}", self.java_version))
                            .show_ui(ui, |ui| {
                                for &v in &provider_versions {
                                    let label = if all_installed_majors.contains(&v) {
                                        format!("Java {} (installed)", v)
                                    } else {
                                        format!("Java {}", v)
                                    };
                                    ui.selectable_value(&mut self.java_version, v, label);
                                }
                            });
                        let label = if already_installed {
                            format!("Java {} already installed", self.java_version)
                        } else {
                            format!("{} Download Java {}", egui_phosphor::regular::ARROW_DOWN, self.java_version)
                        };
                        let btn = ui.add_enabled(!is_downloading && !already_installed, theme.accent_button(&label));
                        if btn.clicked() {
                            download_clicked = true;
                        }
                    });

                    if download_clicked {
                        let version = self.java_version;
                        let provider = self.java_provider;
                        let state = Arc::new(Mutex::new(crate::app::JavaDownloadState {
                            version,
                            message: format!("Starting Java {} download...", version),
                            done: false,
                            result: None,
                        }));
                        *java_download = Some(Arc::clone(&state));

                        let ctx = ui.ctx().clone();
                        if provider == 0 {
                            // Mojang download
                            let component = java::major_to_mojang_component(version)
                                .unwrap_or("java-runtime-delta")
                                .to_string();
                            std::thread::spawn(move || {
                                let client = reqwest::blocking::Client::builder()
                                    .connect_timeout(std::time::Duration::from_secs(10))
                                    .timeout(std::time::Duration::from_secs(600))
                                    .build();
                                let result = client.map_err(|e| e.to_string()).and_then(|c| {
                                    let state_for_cb = Arc::clone(&state);
                                    let ctx_for_cb = ctx.clone();
                                    java::download_mojang_java(&c, &component, move |msg| {
                                        state_for_cb.lock().unwrap().message = msg.to_string();
                                        ctx_for_cb.request_repaint();
                                    })
                                    .map_err(|e| e.to_string())
                                });
                                let mut s = state.lock().unwrap();
                                s.result = Some(result);
                                s.done = true;
                                drop(s);
                                ctx.request_repaint();
                            });
                        } else {
                            // Adoptium download
                            std::thread::spawn(move || {
                                let client = reqwest::blocking::Client::builder()
                                    .connect_timeout(std::time::Duration::from_secs(10))
                                    .timeout(std::time::Duration::from_secs(600))
                                    .build();
                                let result = client.map_err(|e| e.to_string()).and_then(|c| {
                                    let state_for_cb = Arc::clone(&state);
                                    let ctx_for_cb = ctx.clone();
                                    java::download_java(&c, version, move |msg| {
                                        state_for_cb.lock().unwrap().message = msg.to_string();
                                        ctx_for_cb.request_repaint();
                                    })
                                    .map_err(|e| e.to_string())
                                });
                                let mut s = state.lock().unwrap();
                                s.result = Some(result);
                                s.done = true;
                                drop(s);
                                ctx.request_repaint();
                            });
                        }
                    }

                    ui.add_space(12.0);

                    // Installed Java list
                    let row_h = ui.spacing().interact_size.y + 4.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                        |ui| {
                        let rescan_lbl = format!("{} Re-scan", egui_phosphor::regular::MAGNIFYING_GLASS);
                        if ui.add(theme.accent_button(&rescan_lbl)).clicked() {
                            *java_installs = java::detect_java_installations();
                        }
                    });
                    ui.add_space(4.0);

                    if java_installs.is_empty() {
                        ui.label("No Java installations detected.");
                    } else {
                        let badge_fill = theme.color("surface");
                        let badge_fg = theme.color("fg_dim");
                        let managed_fill = theme.color("accent");

                        let outer = crate::ui::helpers::card_frame(ui, theme);
                        outer.show(ui, |ui| {
                            for (idx, install) in java_installs.iter().enumerate() {
                                if idx > 0 {
                                    ui.separator();
                                }
                                let row_h = ui.spacing().interact_size.y + 4.0;
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), row_h),
                                    egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
                                    |ui| {
                                    let title = format!("Java {}", install.major);
                                    ui.label(theme.title(&title));

                                    ui.spacing_mut().item_spacing.x = 4.0;

                                    theme.badge_frame(badge_fill).show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(&install.version)
                                                .size(11.0)
                                                .color(badge_fg),
                                        );
                                    });

                                    theme.badge_frame(badge_fill).show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(&install.vendor)
                                                .size(11.0)
                                                .color(badge_fg),
                                        );
                                    });

                                    if install.managed {
                                        theme.badge_frame(managed_fill).show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new("Managed")
                                                    .size(11.0)
                                                    .color(theme.button_fg()),
                                            );
                                        });
                                    }

                                    if install.managed {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let btn = ui.add(theme.danger_button(&format!(
                                                    "{} Remove",
                                                    egui_phosphor::regular::TRASH
                                                )));
                                                 if btn.clicked() {
                                                    self.confirm_java_remove = Some(idx);
                                                }
                                            },
                                        );
                                    }
                                });
                                ui.label(theme.subtext(&install.path.display().to_string()));
                            }
                        });
                    }
                });

            // Java remove confirmation modal — must be outside CollapsingHeader (uses ctx)
            if let Some(rm_idx) = self.confirm_java_remove {
                let rm_label = java_installs
                    .get(rm_idx)
                    .map(|j| format!("Java {} — {}", j.major, j.version))
                    .unwrap_or_else(|| format!("Java install #{rm_idx}"));

                let mut open = true;
                egui::Window::new("Confirm Remove Java")
                    .id(egui::Id::new(format!("confirm_java_remove_{rm_idx}")))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .open(&mut open)
                    .show(ui.ctx(), |ui| {
                        ui.label(format!("Remove \"{}\"?", rm_label));
                        ui.label(theme.subtext(
                            "This will delete the managed Java installation from disk.",
                        ));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let confirm_clicked = ui.add(theme.danger_button("Remove")).clicked();
                            if confirm_clicked {
                                if rm_idx < java_installs.len() {
                                    let install = &java_installs[rm_idx];
                                    if let Err(e) = java::delete_managed_java(install) {
                                        eprintln!("Failed to delete Java install: {}", e);
                                    }
                                    java_installs.remove(rm_idx);
                                }
                                self.confirm_java_remove = None;
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm_java_remove = None;
                            }
                        });
                    });

                if !open {
                    self.confirm_java_remove = None;
                }
            }

            ui.add_space(16.0);

            // ── Default Memory & JVM ──
            let header_text = theme.section_header(&format!("{} Default Memory & JVM", egui_phosphor::regular::FLOPPY_DISK));
            egui::CollapsingHeader::new(header_text)
                .default_open(true)
                .id_salt("settings_memory")
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    // Use Grid for aligned form layout
                    egui::Grid::new("settings_memory_grid")
                        .num_columns(2)
                        .spacing([16.0, 12.0])
                        .show(ui, |ui| {
                            ui.label("Min Memory:");
                            let max_for_min = config.default_max_memory_mb;
                            ui.add(
                                egui::Slider::new(&mut config.default_min_memory_mb, 256..=max_for_min)
                                    .step_by(256.0)
                                    .text("MB")
                                    .show_value(true),
                            );
                            ui.end_row();

                            ui.label("Max Memory:");
                            let min_for_max = config.default_min_memory_mb;
                            ui.add(
                                egui::Slider::new(&mut config.default_max_memory_mb, min_for_max..=16384)
                                    .step_by(256.0)
                                    .text("MB")
                                    .show_value(true),
                            );
                            ui.end_row();
                        });

                    ui.add_space(8.0);

                    ui.label("Default JVM Arguments:");
                    ui.add_space(2.0);
                    let args_str = config.default_jvm_args.join(" ");
                    let mut args_edit = args_str;
                    if ui
                        .add(
                            egui::TextEdit::multiline(&mut args_edit)
                                .hint_text("-XX:+UseG1GC ...")
                                .desired_width(f32::INFINITY)
                                .desired_rows(3)
                                .margin(egui::Margin::symmetric(4, 9)),
                        )
                        .changed()
                    {
                        config.default_jvm_args = args_edit.split_whitespace().map(String::from).collect();
                    }
                });
            ui.add_space(16.0);

            // ── CurseForge API Key ──
            let header_text = theme.section_header(&format!("{} CurseForge API Key", egui_phosphor::regular::KEY));
            egui::CollapsingHeader::new(header_text)
                .default_open(true)
                .id_salt("settings_curseforge")
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    ui.label(theme.subtext(
                        "Optional. Override the built-in API key with your own CurseForge API key.",
                    ));
                    ui.add_space(4.0);
                    let mut key_text = config.curseforge_api_key.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut key_text)
                                .hint_text("Leave blank to use default")
                                .desired_width(f32::INFINITY)
                                .margin(egui::Margin::symmetric(4, 9)),
                        )
                        .changed()
                    {
                        config.curseforge_api_key = if key_text.trim().is_empty() {
                            None
                        } else {
                            Some(key_text)
                        };
                        // Save immediately so get_api_key() (which reads from disk) picks it up
                        let _ = config.save();
                    }
                });
            ui.add_space(16.0);

            // ── About ──
            let header_text = theme.section_header("ℹ About");
            egui::CollapsingHeader::new(header_text)
                .default_open(true)
                .id_salt("settings_about")
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    ui.label(theme.title("Lurch"));
                    ui.label(theme.subtext("Just a Minecraft launcher"));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Version:");
                        ui.label(theme.subtext(env!("CARGO_PKG_VERSION")));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Rust Edition:");
                        ui.label(theme.subtext("2024"));
                    });
                    ui.horizontal(|ui| {
                        ui.label("UI Framework:");
                        ui.label(theme.subtext("egui / eframe 0.34.1"));
                    });
                });
        }); // end ScrollArea
    }
}
