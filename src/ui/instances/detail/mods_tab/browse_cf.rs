use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::super::InstanceDetailView;
use crate::core::curseforge::{self, CfMod};
use crate::core::instance::Instance;
use crate::ui::helpers::{card_frame, empty_state, format_downloads, load_more_button, project_tooltip, row_hover_highlight, truncate_desc, ViewMode};

impl InstanceDetailView {
    pub(super) fn show_browse_curseforge_tab(
        &mut self,
        ui: &mut egui::Ui,
        instance: &Instance,
        mods_dir: &std::path::Path,
        theme: &crate::theme::Theme,
    ) {
        ui.add_space(4.0);

        let mut do_search = false;
        if !self.cf_search.initialized {
            self.cf_search.initialized = true;
            do_search = true;
        }
        // Fetch CF categories if needed
        if self.cf_categories.is_none() && self.cf_categories_fetch.is_none() {
            let slot: Arc<Mutex<Option<Result<Vec<curseforge::CfCategory>, String>>>> = Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = curseforge::fetch_cf_categories(curseforge::CLASS_MODS).map_err(|e| e.to_string());
                *slot_c.lock().unwrap() = Some(result);
                ctx.request_repaint();
            });
            self.cf_categories_fetch = Some(slot);
        }
        let cf_cats_result = self.cf_categories_fetch.as_ref().and_then(|f| f.lock().unwrap().take());
        if let Some(result) = cf_cats_result {
            match result {
                Ok(cats) => self.cf_categories = Some(cats),
                Err(_) => self.cf_categories = Some(Vec::new()),
            }
            self.cf_categories_fetch = None;
        }
        let search_row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), search_row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            let resp = ui.add_sized(
                [ui.available_width() - 28.0, 32.0],
                egui::TextEdit::singleline(&mut self.cf_search.query)
                    .hint_text("Search mods\u{2026}")
                    .margin(egui::Margin::symmetric(4, 9)),
            );
            if resp.changed() {
                self.cf_search.last_edit = Some(Instant::now());
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_search = true;
            }
            if self.cf_search.is_searching() {
                ui.add(egui::Spinner::new().color(theme.color("accent")));
            }
        });

        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            let loader_str = instance.loader.to_string();
            let version_str = &instance.mc_version;

            if self.cf_search_all_versions {
                ui.label(theme.subtext("Searching all versions and loaders"));
            } else {
                let filter_text = if loader_str.to_lowercase() == "vanilla" {
                    format!("Filtering: {version_str}")
                } else {
                    format!("Filtering: {version_str} + {loader_str}")
                };
                ui.label(theme.subtext(&filter_text));
            }

            let prev = self.cf_search_all_versions;
            ui.checkbox(&mut self.cf_search_all_versions, "Show all versions");
            if prev != self.cf_search_all_versions {
                do_search = true;
            }

            ui.separator();
            let prev_sort = self.cf_sort;
            egui::ComboBox::from_id_salt("cf_mod_sort")
                .selected_text(self.cf_sort.label())
                .width(120.0)
                .show_ui(ui, |ui| {
                    for &opt in curseforge::CfSortField::ALL {
                        ui.selectable_value(&mut self.cf_sort, opt, opt.label());
                    }
                });
            if prev_sort != self.cf_sort {
                do_search = true;
            }

            ui.separator();
            let prev_cat = self.cf_selected_category;
            let cf_cat_label = self.cf_selected_category
                .and_then(|id| self.cf_categories.as_ref()?.iter().find(|c| c.id == id))
                .map(|c| c.name.as_str())
                .unwrap_or("All categories");
            let cf_cat_label_owned = cf_cat_label.to_string();
            egui::ComboBox::from_id_salt("cf_mod_category")
                .selected_text(&cf_cat_label_owned)
                .width(140.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.cf_selected_category, None, "All categories");
                    if let Some(ref cats) = self.cf_categories {
                        for cat in cats {
                            ui.selectable_value(
                                &mut self.cf_selected_category,
                                Some(cat.id),
                                &cat.name,
                            );
                        }
                    }
                });
            if prev_cat != self.cf_selected_category {
                do_search = true;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_cross_justify(true), |ui| {
                let grid_lbl = egui_phosphor::regular::GRID_FOUR.to_string();
                let list_lbl = egui_phosphor::regular::LIST.to_string();
                if ui
                    .selectable_label(self.cf_browse_view_mode == ViewMode::Grid, &grid_lbl)
                    .on_hover_text("Grid view")
                    .clicked()
                {
                    self.cf_browse_view_mode = ViewMode::Grid;
                }
                if ui
                    .selectable_label(self.cf_browse_view_mode == ViewMode::List, &list_lbl)
                    .on_hover_text("List view")
                    .clicked()
                {
                    self.cf_browse_view_mode = ViewMode::List;
                }
            });
        });

        ui.add_space(4.0);

        let mut load_more_triggered = false;

        if !self.cf_search_results.is_empty() {
            ui.label(theme.subtext(&format!("Showing {} of {}", self.cf_search_results.len(), self.cf_search.total)));

            match self.cf_browse_view_mode {
                ViewMode::List => {
                    card_frame(ui, theme).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("cf_search_results_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let results: Vec<CfMod> = self.cf_search_results.clone();
                                let mc_ver = instance.mc_version.clone();
                                let loader_type = curseforge::mod_loader_type(&instance.loader);
                                let mods_dir = mods_dir.to_path_buf();
                                let has_more = self.cf_search_results.len() < self.cf_search.total as usize;

                                for (idx, hit) in results.iter().enumerate() {
                                    if idx > 0 {
                                        ui.separator();
                                    }
                                    let tip_icon_url = hit.logo.as_ref().map(|l| l.best_url().to_string());
                                    let tip_title = hit.name.clone();
                                    let tip_desc = hit.summary.clone();
                                    let tip_downloads = hit.download_count;
                                    let tip_tags: Vec<String> = hit
                                        .categories
                                        .iter()
                                        .map(|c| c.name.clone())
                                        .collect();

                                    let row_resp = ui.horizontal(|ui| {
                                        let icon_resp = if let Some(logo) = &hit.logo {
                                            ui.add(egui::Image::new(logo.best_url()).fit_to_exact_size(egui::vec2(40.0, 40.0)))
                                        } else {
                                            crate::ui::helpers::icon_placeholder(ui, &hit.name, 40.0, theme)
                                        };
                                        icon_resp.on_hover_ui(|ui| {
                                            project_tooltip(ui, tip_icon_url.as_deref(), &tip_title, &tip_desc, tip_downloads, &tip_tags, theme);
                                        });
                                        ui.vertical(|ui| {
                                            ui.set_max_width(ui.available_width() - 220.0);
                                            ui.label(theme.title(&hit.name));
                                            ui.label(theme.subtext(&truncate_desc(&hit.summary, 120)));
                                            ui.label(theme.subtext(&format!(
                                                "{} downloads",
                                                format_downloads(hit.download_count)
                                            )));
                                            let tags: Vec<&str> = hit
                                                .categories
                                                .iter()
                                                .map(|c| c.name.as_str())
                                                .collect();
                                            crate::ui::helpers::show_category_tags(ui, &tags, 3, theme);
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let allows_distribution =
                                                    hit.allow_mod_distribution.unwrap_or(true);
                                                if allows_distribution {
                                                    let mod_id = hit.id;
                                                    let title = hit.name.clone();
                                                    let mc_ver_c = mc_ver.clone();
                                                    let loader_c = loader_type;
                                                    let mods_dir_c = mods_dir.clone();
                                                    if ui.add(theme.accent_button("Install")).clicked() {
                                                        self.open_cf_mod_version_picker(
                                                            mod_id,
                                                            title,
                                                            &mc_ver_c,
                                                            loader_c,
                                                            &mods_dir_c,
                                                            ui.ctx(),
                                                        );
                                                    }
                                                } else {
                                                    let slug = hit.slug.clone();
                                                    let mod_id = hit.id;
                                                    if ui.add(theme.accent_button("Open in browser")).clicked() {
                                                        let url =
                                                            curseforge::curseforge_mod_url(mod_id, &slug);
                                                        let _ = open::that(&url);
                                                    }
                                                }
                                                if allows_distribution {
                                                    let open_page_lbl = egui_phosphor::regular::GLOBE;
                                                    let open_page = ui.add(theme.ghost_button(open_page_lbl));
                                                    if open_page.on_hover_text("Open Page").clicked() {
                                                        let url =
                                                            curseforge::curseforge_mod_url(hit.id, &hit.slug);
                                                        let _ = open::that(&url);
                                                    }
                                                }
                                            },
                                        );
                                    });
                                    row_hover_highlight(ui, row_resp.response.rect, theme);
                                }

                                if has_more {
                                    if load_more_button(ui, self.cf_search_results.len(), self.cf_search.total as usize, theme) {
                                        load_more_triggered = true;
                                    }
                                }
                            });
                    });
                }
                ViewMode::Grid => {
                    let results: Vec<CfMod> = self.cf_search_results.clone();
                    let mc_ver = instance.mc_version.clone();
                    let loader_type = curseforge::mod_loader_type(&instance.loader);
                    let mods_dir = mods_dir.to_path_buf();
                    let card_w = 240.0_f32;
                    let card_h = 210.0_f32;
                    let gap = ui.spacing().item_spacing.x;
                    ui.spacing_mut().item_spacing.y = gap;

                    egui::ScrollArea::vertical()
                        .id_salt("cf_search_results_scroll_grid")
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

                                    let tip_icon_url = hit.logo.as_ref().map(|l| l.best_url().to_string());
                                    let tip_title = hit.name.clone();
                                    let tip_desc = hit.summary.clone();
                                    let tip_downloads = hit.download_count;
                                    let tip_tags: Vec<String> = hit
                                        .categories
                                        .iter()
                                        .map(|c| c.name.clone())
                                        .collect();

                                    crate::ui::helpers::grid_card(
                                        ui,
                                        cell_rect,
                                        theme,
                                        |ui| {
                                            ui.horizontal(|ui| {
                                                 let icon_resp = if let Some(logo) = &hit.logo {
                                                     ui.add(egui::Image::new(logo.best_url()).fit_to_exact_size(egui::vec2(32.0, 32.0)))
                                                 } else {
                                                     crate::ui::helpers::icon_placeholder(
                                                         ui,
                                                         &hit.name,
                                                         32.0,
                                                         theme,
                                                     )
                                                 };
                                                icon_resp.on_hover_ui(|ui| {
                                                    project_tooltip(ui, tip_icon_url.as_deref(), &tip_title, &tip_desc, tip_downloads, &tip_tags, theme);
                                                });
                                                ui.add(egui::Label::new(theme.title(&hit.name)).truncate());
                                            });
                                            ui.add_space(8.0);
                                            ui.label(theme.subtext(&truncate_desc(&hit.summary, 75)));
                                            ui.add_space(4.0);
                                            let tags: Vec<&str> = hit.categories.iter().map(|c| c.name.as_str()).collect();
                                            crate::ui::helpers::show_category_tags(ui, &tags, 2, theme);
                                            let dl = format!("{} downloads", format_downloads(hit.download_count));
                                            ui.label(egui::RichText::new(dl).size(10.0).color(theme.color("fg_dim")));
                                        },
                                        |ui| {
                                            let allows_distribution =
                                                hit.allow_mod_distribution
                                                    .unwrap_or(true);
                                            if allows_distribution {
                                                let mod_id = hit.id;
                                                let title = hit.name.clone();
                                                let mc_ver_c = mc_ver.clone();
                                                let loader_c = loader_type;
                                                let mods_dir_c = mods_dir.clone();
                                                if ui.add(theme.accent_button("Install")).clicked() {
                                                    self.open_cf_mod_version_picker(
                                                        mod_id,
                                                        title,
                                                        &mc_ver_c,
                                                        loader_c,
                                                        &mods_dir_c,
                                                        ui.ctx(),
                                                    );
                                                }
                                            } else {
                                                let slug = hit.slug.clone();
                                                let mod_id = hit.id;
                                                if ui.add(theme.accent_button("Open in browser")).clicked() {
                                                    let url =
                                                        curseforge::curseforge_mod_url(
                                                            mod_id, &slug,
                                                        );
                                                    let _ = open::that(&url);
                                                }
                                            }
                                            if allows_distribution {
                                                let open_page = ui.add(theme.ghost_button(egui_phosphor::regular::GLOBE));
                                                if open_page.on_hover_text("Open Page").clicked() {
                                                    let url = curseforge::curseforge_mod_url(
                                                        hit.id, &hit.slug,
                                                    );
                                                    let _ = open::that(&url);
                                                }
                                            }
                                        },
                                    );

                                    x += card_w + gap;
                                }
                            }

                            let has_more =
                                self.cf_search_results.len() < self.cf_search.total as usize;
                            if has_more {
                                if load_more_button(ui, self.cf_search_results.len(), self.cf_search.total as usize, theme) {
                                    load_more_triggered = true;
                                }
                            }
                        });
                }
            }
        } else if !self.cf_search.is_searching() && self.cf_search.initialized {
            empty_state(ui, egui_phosphor::regular::MAGNIFYING_GLASS, "No results. Try a different search term.", theme);
        }

        if load_more_triggered {
            self.cf_search.offset = self.cf_search_results.len() as u32;
            self.cf_search.appending = true;
        }
        if self.cf_search.check_debounce(ui.ctx()) {
            do_search = true;
        }
        if do_search || load_more_triggered {
            if do_search {
                self.cf_search.offset = 0;
                self.cf_search.appending = false;
                self.cf_search.last_edit = None;
            }
            let query = self.cf_search.query.clone();
            let mc_version = if self.cf_search_all_versions {
                String::new()
            } else {
                instance.mc_version.clone()
            };
            let loader_type = if self.cf_search_all_versions {
                None
            } else {
                curseforge::mod_loader_type(&instance.loader)
            };
            let offset = self.cf_search.offset;
            let cf_sort = self.cf_sort;
            let cf_category_id = self.cf_selected_category;
            self.cf_search.fire_with_repaint(ui.ctx(), move || {
                curseforge::search_cf_mods(
                    &query,
                    &mc_version,
                    loader_type,
                    curseforge::CLASS_MODS,
                    offset,
                    cf_sort,
                    cf_category_id,
                )
                .map_err(|e| e.to_string())
            });
        }

        let maybe_status: Option<Arc<Mutex<Option<String>>>> = ui
            .ctx()
            .data(|d| d.get_temp(egui::Id::new("cf_install_status")));
        if let Some(arc) = maybe_status
            && let Some(msg) = arc.lock().ok().and_then(|mut g| g.take()) {
                if msg.starts_with("Error") || msg.starts_with("Install failed") || msg.starts_with("Search failed") {
                    self.pending_toasts.push(crate::app::Toast::error(msg));
                } else {
                    self.pending_toasts.push(crate::app::Toast::success(msg));
                }
                self.needs_rescan = true;
                ui.ctx().data_mut(|d| {
                    d.remove::<Arc<Mutex<Option<String>>>>(egui::Id::new("cf_install_status"))
                });
            }

        if self.cf_search.is_searching() {
            ui.ctx().request_repaint();
        }
    }
}
