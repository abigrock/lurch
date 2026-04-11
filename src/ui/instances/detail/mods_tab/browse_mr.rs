use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::super::InstanceDetailView;
use crate::core::instance::Instance;
use crate::core::modrinth::{self, SearchHit};
use crate::ui::helpers::{card_frame, empty_state, format_downloads, load_more_button, project_tooltip, row_hover_highlight, truncate_desc, ViewMode};

impl InstanceDetailView {
    pub(super) fn show_browse_tab(
        &mut self,
        ui: &mut egui::Ui,
        instance: &Instance,
        mods_dir: &std::path::Path,
        theme: Option<&crate::theme::Theme>,
    ) {
        ui.add_space(4.0);

        let mut do_search = false;
        if !self.mr_search.initialized {
            self.mr_search.initialized = true;
            do_search = true;
        }
        // Fetch MR categories if needed
        if self.mr_categories.is_none() && self.mr_categories_fetch.is_none() {
            let slot: Arc<Mutex<Option<Result<Vec<modrinth::MrCategory>, String>>>> = Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = modrinth::fetch_mr_categories("mod").map_err(|e| e.to_string());
                *slot_c.lock().unwrap() = Some(result);
                ctx.request_repaint();
            });
            self.mr_categories_fetch = Some(slot);
        }
        let mr_cats_result = self.mr_categories_fetch.as_ref().and_then(|f| f.lock().unwrap().take());
        if let Some(result) = mr_cats_result {
            match result {
                Ok(cats) => self.mr_categories = Some(cats),
                Err(_) => self.mr_categories = Some(Vec::new()),
            }
            self.mr_categories_fetch = None;
        }
        let search_row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), search_row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            let resp = ui.add_sized(
                [ui.available_width() - 28.0, 32.0],
                egui::TextEdit::singleline(&mut self.mr_search.query)
                    .hint_text("Search mods\u{2026}")
                    .margin(egui::Margin::symmetric(4, 9)),
            );
            if resp.changed() {
                self.mr_search.last_edit = Some(Instant::now());
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_search = true;
            }
            if self.mr_search.is_searching() {
                if let Some(t) = theme {
                    ui.add(egui::Spinner::new().color(t.color("accent")));
                } else {
                    ui.spinner();
                }
            }
        });

        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            let loader_str = instance.loader.to_string();
            let version_str = &instance.mc_version;

            if self.search_all_versions {
                if let Some(t) = theme {
                    ui.label(t.subtext("Searching all versions and loaders"));
                } else {
                    ui.weak("Searching all versions and loaders");
                }
            } else {
                let filter_text = if loader_str.to_lowercase() == "vanilla" {
                    format!("Filtering: {version_str}")
                } else {
                    format!("Filtering: {version_str} + {loader_str}")
                };
                if let Some(t) = theme {
                    ui.label(t.subtext(&filter_text));
                } else {
                    ui.weak(&filter_text);
                }
            }

            let prev = self.search_all_versions;
            ui.checkbox(&mut self.search_all_versions, "Show all versions");
            if prev != self.search_all_versions {
                do_search = true;
            }

            ui.separator();
            let prev_sort = self.mr_sort;
            egui::ComboBox::from_id_salt("mr_mod_sort")
                .selected_text(self.mr_sort.label())
                .width(120.0)
                .show_ui(ui, |ui| {
                    for &opt in modrinth::MrSortIndex::ALL {
                        ui.selectable_value(&mut self.mr_sort, opt, opt.label());
                    }
                });
            if prev_sort != self.mr_sort {
                do_search = true;
            }

            ui.separator();
            let prev_cat = self.mr_selected_category.clone();
            let mr_cat_label = self.mr_selected_category.as_deref().unwrap_or("All categories");
            egui::ComboBox::from_id_salt("mr_mod_category")
                .selected_text(mr_cat_label)
                .width(140.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.mr_selected_category, None, "All categories");
                    if let Some(ref cats) = self.mr_categories {
                        for cat in cats {
                            ui.selectable_value(
                                &mut self.mr_selected_category,
                                Some(cat.name.clone()),
                                &cat.name,
                            );
                        }
                    }
                });
            if prev_cat != self.mr_selected_category {
                do_search = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_cross_justify(true), |ui| {
                let grid_lbl = egui_phosphor::regular::GRID_FOUR.to_string();
                let list_lbl = egui_phosphor::regular::LIST.to_string();
                if ui
                    .selectable_label(self.mr_browse_view_mode == ViewMode::Grid, &grid_lbl)
                    .on_hover_text("Grid view")
                    .clicked()
                {
                    self.mr_browse_view_mode = ViewMode::Grid;
                }
                if ui
                    .selectable_label(self.mr_browse_view_mode == ViewMode::List, &list_lbl)
                    .on_hover_text("List view")
                    .clicked()
                {
                    self.mr_browse_view_mode = ViewMode::List;
                }
            });
        });

        ui.add_space(4.0);

        let mut load_more_triggered = false;

        if !self.search_results.is_empty() {
            if let Some(t) = theme {
                ui.label(t.subtext(&format!("Showing {} of {}", self.search_results.len(), self.mr_search.total)));
            } else {
                ui.weak(format!("Showing {} of {}", self.search_results.len(), self.mr_search.total));
            }

            match self.mr_browse_view_mode {
                ViewMode::List => {
                    card_frame(ui, theme).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("search_results_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let results: Vec<SearchHit> = self.search_results.clone();
                                let mc_ver = instance.mc_version.clone();
                                let loader = instance.loader.to_string().to_lowercase();
                                let mods_dir = mods_dir.to_path_buf();
                                let has_more = self.search_results.len() < self.mr_search.total as usize;

                                for (idx, hit) in results.iter().enumerate() {
                                    if idx > 0 {
                                        ui.separator();
                                    }
                                    let tip_icon_url = hit.icon_url.clone();
                                    let tip_title = hit.title.clone();
                                    let tip_desc = hit.description.clone();
                                    let tip_downloads = hit.downloads;
                                    let tip_tags: Vec<String> = hit.categories.clone();
                                    let tip_theme = theme.cloned();

                                    let row_resp = ui.horizontal(|ui| {
                                        let icon_resp = if let Some(url) = &hit.icon_url {
                                            ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(40.0, 40.0)))
                                        } else {
                                            crate::ui::helpers::icon_placeholder(ui, &hit.title, 40.0, theme)
                                        };
                                        icon_resp.on_hover_ui(|ui| {
                                            project_tooltip(ui, tip_icon_url.as_deref(), &tip_title, &tip_desc, tip_downloads, &tip_tags, tip_theme.as_ref());
                                        });
                                        ui.vertical(|ui| {
                                            ui.set_max_width(ui.available_width() - 220.0);
                                            if let Some(t) = theme {
                                                ui.label(t.title(&hit.title));
                                                ui.label(t.subtext(&truncate_desc(&hit.description, 120)));
                                                ui.label(t.subtext(&format!(
                                                    "{} downloads",
                                                    format_downloads(hit.downloads)
                                                )));
                                            } else {
                                                ui.strong(&hit.title);
                                                ui.weak(truncate_desc(&hit.description, 120));
                                                ui.weak(format!(
                                                    "{} downloads",
                                                    format_downloads(hit.downloads)
                                                ));
                                            }
                                            let tags: Vec<&str> = hit.categories.iter().map(|s| s.as_str()).collect();
                                            crate::ui::helpers::show_category_tags(ui, &tags, 3, theme);
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let project_id = hit.project_id.clone();
                                                let title = hit.title.clone();
                                                let mods_dir_c = mods_dir.clone();
                                                let mc_ver_c = mc_ver.clone();
                                                let loader_c = loader.clone();
                                                if if let Some(t) = theme {
                                                    ui.add(t.accent_button("Install")).clicked()
                                                } else {
                                                    ui.button("Install").clicked()
                                                } {
                                                    self.open_mr_mod_version_picker(
                                                        project_id,
                                                        title,
                                                        &mc_ver_c,
                                                        &loader_c,
                                                        &mods_dir_c,
                                                        ui.ctx(),
                                                    );
                                                }
                                                let open_page_lbl = egui_phosphor::regular::GLOBE;
                                                let open_page = if let Some(t) = theme {
                                                    ui.add(t.ghost_button(open_page_lbl))
                                                } else {
                                                    ui.button(open_page_lbl)
                                                };
                                                if open_page.on_hover_text("Open Page").clicked() {
                                                    let url = modrinth::modrinth_project_url(&hit.slug);
                                                    let _ = open::that(&url);
                                                }
                                            },
                                        );
                                    });
                                    row_hover_highlight(ui, row_resp.response.rect, theme);
                                }

                                if has_more {
                                    if load_more_button(ui, self.search_results.len(), self.mr_search.total as usize, theme) {
                                        load_more_triggered = true;
                                    }
                                }
                             });
                    });
                }
                ViewMode::Grid => {
                    let results: Vec<SearchHit> = self.search_results.clone();
                    let mc_ver = instance.mc_version.clone();
                    let loader = instance.loader.to_string().to_lowercase();
                    let mods_dir = mods_dir.to_path_buf();
                    let card_w = 240.0_f32;
                    let card_h = 210.0_f32;
                    let gap = ui.spacing().item_spacing.x;
                    ui.spacing_mut().item_spacing.y = gap;

                    egui::ScrollArea::vertical()
                        .id_salt("search_results_scroll_grid")
                        .show(ui, |ui| {
                            let gap = ui.spacing().item_spacing.x;
                            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                            let available = ui.available_width();
                            let cols = ((available + gap) / (card_w + gap)).floor().max(1.0) as usize;

                            for (row_idx, row) in results.chunks(cols).enumerate() {
                                let (row_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(available, card_h),
                                    egui::Sense::hover(),
                                );

                                let mut x = row_rect.min.x;
                                for (col_idx, hit) in row.iter().enumerate() {
                                    let _i = row_idx * cols + col_idx;
                                    let cell_rect = egui::Rect::from_min_size(
                                        egui::pos2(x, row_rect.min.y),
                                        egui::vec2(card_w, card_h),
                                    );

                                    let tip_icon_url = hit.icon_url.clone();
                                    let tip_title = hit.title.clone();
                                    let tip_desc = hit.description.clone();
                                    let tip_downloads = hit.downloads;
                                    let tip_tags: Vec<String> = hit.categories.clone();
                                    let tip_theme = theme.cloned();

                                    crate::ui::helpers::grid_card(
                                        ui,
                                        cell_rect,
                                        theme,
                                        |ui| {
                                            ui.horizontal(|ui| {
                                                 let icon_resp = if let Some(url) = &hit.icon_url {
                                                     ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(32.0, 32.0)))
                                                 } else {
                                                     crate::ui::helpers::icon_placeholder(
                                                         ui,
                                                         &hit.title,
                                                         32.0,
                                                         theme,
                                                     )
                                                 };
                                                icon_resp.on_hover_ui(|ui| {
                                                    project_tooltip(ui, tip_icon_url.as_deref(), &tip_title, &tip_desc, tip_downloads, &tip_tags, tip_theme.as_ref());
                                                });
                                                if let Some(t) = theme {
                                                    ui.add(egui::Label::new(t.title(&hit.title)).truncate());
                                                } else {
                                                    ui.add(egui::Label::new(egui::RichText::new(&hit.title).strong()).truncate());
                                                }
                                            });
                                            ui.add_space(8.0);
                                            if let Some(t) = theme {
                                                ui.label(t.subtext(&truncate_desc(&hit.description, 75)));
                                            } else {
                                                ui.weak(truncate_desc(&hit.description, 75));
                                            }
                                            ui.add_space(4.0);
                                            let tags: Vec<&str> = hit.categories.iter().map(|s| s.as_str()).collect();
                                            crate::ui::helpers::show_category_tags(ui, &tags, 2, theme);
                                            let dl = format!("{} downloads", format_downloads(hit.downloads));
                                            if let Some(t) = theme {
                                                ui.label(egui::RichText::new(dl).size(10.0).color(t.color("fg_dim")));
                                            } else {
                                                ui.label(egui::RichText::new(dl).size(10.0));
                                            }
                                        },
                                        |ui| {
                                            let project_id = hit.project_id.clone();
                                            let title = hit.title.clone();
                                            let mods_dir_c = mods_dir.clone();
                                            let mc_ver_c = mc_ver.clone();
                                            let loader_c = loader.clone();
                                            if if let Some(t) = theme {
                                                ui.add(t.accent_button("Install")).clicked()
                                            } else {
                                                ui.button("Install").clicked()
                                            } {
                                                self.open_mr_mod_version_picker(
                                                    project_id,
                                                    title,
                                                    &mc_ver_c,
                                                    &loader_c,
                                                    &mods_dir_c,
                                                    ui.ctx(),
                                                );
                                            }
                                            let open_page = if let Some(t) = theme {
                                                ui.add(t.ghost_button(egui_phosphor::regular::GLOBE))
                                            } else {
                                                ui.button(egui_phosphor::regular::GLOBE)
                                            };
                                            if open_page.on_hover_text("Open Page").clicked() {
                                                let url = modrinth::modrinth_project_url(&hit.slug);
                                                let _ = open::that(&url);
                                            }
                                        },
                                    );

                                    x += card_w + gap;
                                }
                            }

                            let has_more = self.search_results.len() < self.mr_search.total as usize;
                            if has_more {
                                if load_more_button(ui, self.search_results.len(), self.mr_search.total as usize, theme) {
                                    load_more_triggered = true;
                                }
                            }
                        });
                }
            }
        } else if !self.mr_search.is_searching() && self.mr_search.initialized {
            empty_state(ui, egui_phosphor::regular::MAGNIFYING_GLASS, "No results. Try a different search term.", theme);
        }

        if load_more_triggered {
            self.mr_search.offset = self.search_results.len() as u32;
            self.mr_search.appending = true;
        }
        if self.mr_search.check_debounce(ui.ctx()) {
            do_search = true;
        }
        if do_search || load_more_triggered {
            if do_search {
                self.mr_search.offset = 0;
                self.mr_search.appending = false;
                self.mr_search.last_edit = None;
            }
            let query = self.mr_search.query.clone();
            let mc_version = if self.search_all_versions {
                String::new()
            } else {
                instance.mc_version.clone()
            };
            let loader = if self.search_all_versions {
                String::new()
            } else {
                instance.loader.to_string().to_lowercase()
            };
            let offset = self.mr_search.offset;
            let mr_sort = self.mr_sort;
            let mr_category = self.mr_selected_category.clone();
            self.mr_search.fire_with_repaint(ui.ctx(), move || {
                modrinth::search_mods(&query, &mc_version, &loader, "mod", offset, mr_sort, mr_category.as_deref())
                    .map_err(|e| e.to_string())
            });
        }

        let maybe_status: Option<Arc<Mutex<Option<String>>>> = ui
            .ctx()
            .data(|d| d.get_temp(egui::Id::new("install_status")));
        if let Some(arc) = maybe_status
            && let Some(msg) = arc.lock().ok().and_then(|mut g| g.take()) {
                if msg.starts_with("Error") || msg.starts_with("Install failed") || msg.starts_with("Search failed") {
                    self.pending_toasts.push(crate::app::Toast::error(msg));
                } else {
                    self.pending_toasts.push(crate::app::Toast::success(msg));
                }
                self.needs_rescan = true;
                ui.ctx().data_mut(|d| {
                    d.remove::<Arc<Mutex<Option<String>>>>(egui::Id::new("install_status"))
                });
            }

        if self.mr_search.is_searching() {
            ui.ctx().request_repaint();
        }
    }
}
