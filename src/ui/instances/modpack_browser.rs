use crate::core::curseforge::{self, CfCategory, CfFile, CfMod, CfSortField, CLASS_MODPACKS};
use crate::core::modrinth::{self, MrCategory, MrSortIndex, ProjectVersion, SearchHit};
use crate::theme::Theme;
use crate::ui::helpers::{
    card_frame, card_grid, empty_state, format_downloads, load_more_button, project_tooltip,
    row_hover_highlight, truncate_desc, SearchState, ViewMode,
};
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ModpackSource {
    #[default]
    CurseForge,
    Modrinth,
}

pub struct ModpackInstallRequest {
    pub project_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub version_id: Option<String>,
    pub version_name: Option<String>,
}

pub struct CfModpackInstallRequest {
    pub mod_id: u64,
    pub title: String,
    pub icon_url: Option<String>,
    pub file_id: Option<u64>,
    pub file_name: Option<String>,
}

pub struct ModpackBrowser {
    pub view_mode: ViewMode,
    pub mr_search: SearchState<crate::core::modrinth::SearchResponse>,
    pub mr_results: Vec<SearchHit>,
    pub install_requested: Option<ModpackInstallRequest>,
    pub cf_search: SearchState<crate::core::curseforge::CfSearchResponse>,
    pub cf_results: Vec<CfMod>,
    pub cf_install_requested: Option<CfModpackInstallRequest>,
    pub version_picker: Option<VersionPickerState>,
    pub mr_sort: MrSortIndex,
    pub cf_sort: CfSortField,
    // Category filtering
    mr_categories: Option<Vec<MrCategory>>,
    mr_categories_fetch: Option<Arc<Mutex<Option<Result<Vec<MrCategory>, String>>>>>,
    mr_selected_category: Option<String>,
    cf_categories: Option<Vec<CfCategory>>,
    cf_categories_fetch: Option<Arc<Mutex<Option<Result<Vec<CfCategory>, String>>>>>,
    cf_selected_category: Option<u64>,
}

pub struct VersionPickerState {
    pub title: String,
    pub icon_url: Option<String>,
    pub source: ModpackSource,
    pub mr_project_id: Option<String>,
    pub mr_versions: Vec<ProjectVersion>,
    pub cf_mod_id: Option<u64>,
    pub cf_files: Vec<CfFile>,
    pub fetch_handle: Option<Arc<Mutex<Option<VersionFetchResult>>>>,
    pub selected_index: usize,
}

pub enum VersionFetchResult {
    MrVersions(Result<Vec<ProjectVersion>, String>),
    CfFiles(Result<Vec<CfFile>, String>),
}

impl Default for ModpackBrowser {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::List,
            mr_search: SearchState::default(),
            mr_results: Vec::new(),
            install_requested: None,
            cf_search: SearchState::default(),
            cf_results: Vec::new(),
            cf_install_requested: None,
            version_picker: None,
            mr_sort: MrSortIndex::default(),
            cf_sort: CfSortField::default(),
            mr_categories: None,
            mr_categories_fetch: None,
            mr_selected_category: None,
            cf_categories: None,
            cf_categories_fetch: None,
            cf_selected_category: None,
        }
    }
}

use crate::ui::helpers::show_category_tags;

impl ModpackBrowser {
    pub fn show_for_source(
        &mut self,
        ui: &mut egui::Ui,
        source: ModpackSource,
        theme: Option<&Theme>,
        pending_toasts: &mut Vec<crate::app::Toast>,
    ) {
        match source {
            ModpackSource::CurseForge => {
                self.show_curseforge(ui, theme, pending_toasts);
            }
            ModpackSource::Modrinth => {
                self.show_modrinth(ui, theme, pending_toasts);
            }
        }

        if self.mr_search.is_searching() || self.cf_search.is_searching() {
            ui.ctx().request_repaint();
        }

        self.poll_version_picker();
        self.show_version_picker(ui, theme);

        if self
            .version_picker
            .as_ref()
            .is_some_and(|vp| vp.fetch_handle.is_some())
        {
            ui.ctx().request_repaint();
        }
    }

    fn show_modrinth(
        &mut self,
        ui: &mut egui::Ui,
        theme: Option<&Theme>,
        pending_toasts: &mut Vec<crate::app::Toast>,
    ) {
        ui.set_min_width(ui.available_width());
        let mut do_search = false;
        let mut load_more_triggered = false;
        let is_searching = self.mr_search.is_searching();

        if !self.mr_search.initialized {
            self.mr_search.initialized = true;
            do_search = true;
        }

        // Fetch categories if needed
        if self.mr_categories.is_none() && self.mr_categories_fetch.is_none() {
            let slot: Arc<Mutex<Option<Result<Vec<MrCategory>, String>>>> =
                Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = modrinth::fetch_mr_categories("modpack").map_err(|e| e.to_string());
                *slot_c.lock().unwrap() = Some(result);
                ctx.request_repaint();
            });
            self.mr_categories_fetch = Some(slot);
        }
        // Poll categories
        let mr_cat_result = self
            .mr_categories_fetch
            .as_ref()
            .and_then(|f| f.lock().unwrap().take());
        if let Some(result) = mr_cat_result {
            match result {
                Ok(cats) => self.mr_categories = Some(cats),
                Err(_) => self.mr_categories = Some(Vec::new()),
            }
            self.mr_categories_fetch = None;
        }

        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.mr_search.query)
                            .desired_width(ui.available_width())
                            .hint_text("Search modpacks\u{2026}")
                            .margin(egui::Margin::symmetric(4, 9)),
                    );
                    if resp.changed() {
                        self.mr_search.last_edit = Some(Instant::now());
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        do_search = true;
                    }
                });
            },
        );

        if self.mr_search.check_debounce(ui.ctx()) {
            do_search = true;
        }

        // Sort + view mode row
        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
                let prev_mr_sort = self.mr_sort;
                egui::ComboBox::from_id_salt("mr_modpack_sort")
                    .selected_text(self.mr_sort.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for &opt in MrSortIndex::ALL {
                            ui.selectable_value(&mut self.mr_sort, opt, opt.label());
                        }
                    });
                if prev_mr_sort != self.mr_sort {
                    do_search = true;
                }

                ui.separator();
                let prev_cat = self.mr_selected_category.clone();
                let cat_label = self
                    .mr_selected_category
                    .as_deref()
                    .unwrap_or("All categories");
                egui::ComboBox::from_id_salt("mr_modpack_category")
                    .selected_text(cat_label)
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

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center).with_cross_justify(true),
                    |ui| {
                        let grid_lbl = egui_phosphor::regular::GRID_FOUR.to_string();
                        let list_lbl = egui_phosphor::regular::LIST.to_string();
                        if ui
                            .selectable_label(self.view_mode == ViewMode::Grid, &grid_lbl)
                            .on_hover_text("Grid view")
                            .clicked()
                        {
                            self.view_mode = ViewMode::Grid;
                        }
                        if ui
                            .selectable_label(self.view_mode == ViewMode::List, &list_lbl)
                            .on_hover_text("List view")
                            .clicked()
                        {
                            self.view_mode = ViewMode::List;
                        }
                    },
                );
            },
        );

        if let Some(result) = self.mr_search.poll() {
            match result {
                Ok(resp) => {
                    if self.mr_search.appending {
                        self.mr_results.extend(resp.hits);
                    } else {
                        self.mr_results = resp.hits;
                    }
                    self.mr_search.total = resp.total_hits;
                    self.mr_search.appending = false;
                }
                Err(e) => {
                    pending_toasts.push(crate::app::Toast::error(format!("Search failed: {e}")));
                }
            }
        }

        ui.separator();

        if is_searching {
            ui.horizontal(|ui| {
                if let Some(t) = theme {
                    ui.add(egui::Spinner::new().color(t.color("accent")));
                } else {
                    ui.spinner();
                }
                if let Some(t) = theme {
                    ui.label(t.subtext("Searching..."));
                } else {
                    ui.label("Searching...");
                }
            });
        } else if !self.mr_results.is_empty() {
            if let Some(t) = theme {
                ui.label(t.subtext(&format!(
                    "Showing {} of {}",
                    self.mr_results.len(),
                    self.mr_search.total
                )));
            } else {
                ui.weak(format!(
                    "Showing {} of {}",
                    self.mr_results.len(),
                    self.mr_search.total
                ));
            }

            let results = &self.mr_results;
            let mut open_mr_picker = None;
            let has_more = self.mr_results.len() < self.mr_search.total as usize;

            match self.view_mode {
                ViewMode::List => {
                    ui.add_space(4.0);
                    let outer_frame = card_frame(ui, theme);
                    outer_frame.show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("modpack_search_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
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
                                            ui.add(
                                                egui::Image::new(url)
                                                    .fit_to_exact_size(egui::vec2(40.0, 40.0)),
                                            )
                                        } else {
                                            crate::ui::helpers::icon_placeholder(
                                                ui, &hit.title, 40.0, theme,
                                            )
                                        };
                                        icon_resp.on_hover_ui(|ui| {
                                            project_tooltip(
                                                ui,
                                                tip_icon_url.as_deref(),
                                                &tip_title,
                                                &tip_desc,
                                                tip_downloads,
                                                &tip_tags,
                                                tip_theme.as_ref(),
                                            );
                                        });
                                        ui.vertical(|ui| {
                                            ui.set_max_width(ui.available_width() - 220.0);
                                            if let Some(t) = theme {
                                                ui.label(t.title(&hit.title));
                                                ui.label(t.subtext(&truncate_desc(
                                                    &hit.description,
                                                    120,
                                                )));
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
                                            let tags: Vec<&str> =
                                                hit.categories.iter().map(|s| s.as_str()).collect();
                                            show_category_tags(ui, &tags, 3, theme);
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let install_clicked = if let Some(t) = theme {
                                                    ui.add(t.accent_button("Install")).clicked()
                                                } else {
                                                    ui.button("Install").clicked()
                                                };
                                                if install_clicked {
                                                    open_mr_picker = Some((
                                                        hit.project_id.clone(),
                                                        hit.title.clone(),
                                                        hit.icon_url.clone(),
                                                    ));
                                                }
                                                let open_page_lbl = egui_phosphor::regular::GLOBE;
                                                let open_page = if let Some(t) = theme {
                                                    ui.add(t.ghost_button(open_page_lbl))
                                                } else {
                                                    ui.button(open_page_lbl)
                                                };
                                                if open_page.on_hover_text("Open Page").clicked() {
                                                    let url =
                                                        modrinth::modrinth_project_url(&hit.slug);
                                                    let _ = open::that(&url);
                                                }
                                            },
                                        );
                                    });
                                    row_hover_highlight(ui, row_resp.response.rect, theme);
                                }
                                if has_more {
                                    if load_more_button(
                                        ui,
                                        results.len(),
                                        self.mr_search.total as usize,
                                        theme,
                                    ) {
                                        load_more_triggered = true;
                                    }
                                }
                            });
                    });
                }
                ViewMode::Grid => {
                    if card_grid(
                        ui,
                        "modpack_search_scroll_grid",
                        &results,
                        240.0,
                        210.0,
                        theme,
                        has_more,
                        self.mr_search.total as usize,
                        |ui, _i, hit| {
                            let tip_icon_url = hit.icon_url.clone();
                            let tip_title = hit.title.clone();
                            let tip_desc = hit.description.clone();
                            let tip_downloads = hit.downloads;
                            let tip_tags: Vec<String> = hit.categories.clone();
                            let tip_theme = theme.cloned();

                            ui.horizontal(|ui| {
                                let icon_resp = if let Some(url) = &hit.icon_url {
                                    ui.add(
                                        egui::Image::new(url)
                                            .fit_to_exact_size(egui::vec2(32.0, 32.0)),
                                    )
                                } else {
                                    crate::ui::helpers::icon_placeholder(
                                        ui, &hit.title, 32.0, theme,
                                    )
                                };
                                icon_resp.on_hover_ui(|ui| {
                                    project_tooltip(
                                        ui,
                                        tip_icon_url.as_deref(),
                                        &tip_title,
                                        &tip_desc,
                                        tip_downloads,
                                        &tip_tags,
                                        tip_theme.as_ref(),
                                    );
                                });
                                if let Some(t) = theme {
                                    ui.add(egui::Label::new(t.title(&hit.title)).truncate());
                                } else {
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(&hit.title).strong())
                                            .truncate(),
                                    );
                                }
                            });
                            ui.add_space(8.0);
                            if let Some(t) = theme {
                                ui.label(t.subtext(&truncate_desc(&hit.description, 75)));
                            } else {
                                ui.weak(truncate_desc(&hit.description, 75));
                            }
                            ui.add_space(4.0);
                            let tags: Vec<&str> =
                                hit.categories.iter().map(|s| s.as_str()).collect();
                            show_category_tags(ui, &tags, 2, theme);
                            let dl = format!("{} downloads", format_downloads(hit.downloads));
                            if let Some(t) = theme {
                                ui.label(
                                    egui::RichText::new(dl).size(10.0).color(t.color("fg_dim")),
                                );
                            } else {
                                ui.label(egui::RichText::new(dl).size(10.0));
                            }
                        },
                        |ui, _i, hit| {
                            let open_page = if let Some(t) = theme {
                                ui.add(t.ghost_button(egui_phosphor::regular::GLOBE))
                            } else {
                                ui.button(egui_phosphor::regular::GLOBE)
                            };
                            if open_page.on_hover_text("Open Page").clicked() {
                                let url = crate::core::modrinth::modrinth_project_url(&hit.slug);
                                let _ = open::that(&url);
                            }
                            let install_clicked = if let Some(t) = theme {
                                ui.add(t.accent_button("Install")).clicked()
                            } else {
                                ui.button("Install").clicked()
                            };
                            if install_clicked {
                                open_mr_picker = Some((
                                    hit.project_id.clone(),
                                    hit.title.clone(),
                                    hit.icon_url.clone(),
                                ));
                            }
                        },
                    ) {
                        load_more_triggered = true;
                    }
                }
            }

            if let Some((project_id, title, icon_url)) = open_mr_picker {
                self.open_mr_version_picker(project_id, title, icon_url, ui.ctx());
            }
        } else if !self.mr_search.is_searching() && self.mr_search.initialized {
            empty_state(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                "No results. Try a different search term.",
                theme,
            );
        }

        if load_more_triggered {
            self.mr_search.offset = self.mr_results.len() as u32;
            self.mr_search.appending = true;
        }
        if do_search || load_more_triggered {
            if do_search {
                self.mr_search.offset = 0;
                self.mr_search.appending = false;
                self.mr_search.last_edit = None;
            }
            let query = self.mr_search.query.clone();
            let mr_sort = self.mr_sort;
            let mr_category = self.mr_selected_category.clone();
            let offset = self.mr_search.offset;
            self.mr_search.fire_with_repaint(ui.ctx(), move || {
                crate::core::modrinth::search_mods(
                    &query,
                    "",
                    "",
                    "modpack",
                    offset,
                    mr_sort,
                    mr_category.as_deref(),
                )
                .map_err(|e| e.to_string())
            });
        }
    }

    fn show_curseforge(
        &mut self,
        ui: &mut egui::Ui,
        theme: Option<&Theme>,
        pending_toasts: &mut Vec<crate::app::Toast>,
    ) {
        ui.set_min_width(ui.available_width());
        let mut do_search = false;
        let mut load_more_triggered = false;
        let is_searching = self.cf_search.is_searching();

        if !self.cf_search.initialized {
            self.cf_search.initialized = true;
            do_search = true;
        }

        if self.cf_categories.is_none() && self.cf_categories_fetch.is_none() {
            let slot: Arc<Mutex<Option<Result<Vec<CfCategory>, String>>>> =
                Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result =
                    curseforge::fetch_cf_categories(CLASS_MODPACKS).map_err(|e| e.to_string());
                *slot_c.lock().unwrap() = Some(result);
                ctx.request_repaint();
            });
            self.cf_categories_fetch = Some(slot);
        }
        let cf_cats_result = self
            .cf_categories_fetch
            .as_ref()
            .and_then(|f| f.lock().unwrap().take());
        if let Some(result) = cf_cats_result {
            match result {
                Ok(cats) => self.cf_categories = Some(cats),
                Err(_) => self.cf_categories = Some(Vec::new()),
            }
            self.cf_categories_fetch = None;
        }

        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.cf_search.query)
                            .desired_width(ui.available_width())
                            .hint_text("Search modpacks\u{2026}")
                            .margin(egui::Margin::symmetric(4, 9)),
                    );
                    if resp.changed() {
                        self.cf_search.last_edit = Some(Instant::now());
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        do_search = true;
                    }
                });
            },
        );

        if self.cf_search.check_debounce(ui.ctx()) {
            do_search = true;
        }

        // Sort + view mode row
        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
                let prev_cf_sort = self.cf_sort;
                egui::ComboBox::from_id_salt("cf_modpack_sort")
                    .selected_text(self.cf_sort.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for &opt in CfSortField::ALL {
                            ui.selectable_value(&mut self.cf_sort, opt, opt.label());
                        }
                    });
                if prev_cf_sort != self.cf_sort {
                    do_search = true;
                }

                ui.separator();
                let prev_cat = self.cf_selected_category;
                let cf_cat_label = self
                    .cf_selected_category
                    .and_then(|id| self.cf_categories.as_ref()?.iter().find(|c| c.id == id))
                    .map(|c| c.name.as_str())
                    .unwrap_or("All categories");
                let cf_cat_label_owned = cf_cat_label.to_string();
                egui::ComboBox::from_id_salt("cf_modpack_category")
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

                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center).with_cross_justify(true),
                    |ui| {
                        let grid_lbl = egui_phosphor::regular::GRID_FOUR.to_string();
                        let list_lbl = egui_phosphor::regular::LIST.to_string();
                        if ui
                            .selectable_label(self.view_mode == ViewMode::Grid, &grid_lbl)
                            .on_hover_text("Grid view")
                            .clicked()
                        {
                            self.view_mode = ViewMode::Grid;
                        }
                        if ui
                            .selectable_label(self.view_mode == ViewMode::List, &list_lbl)
                            .on_hover_text("List view")
                            .clicked()
                        {
                            self.view_mode = ViewMode::List;
                        }
                    },
                );
            },
        );

        if let Some(result) = self.cf_search.poll() {
            match result {
                Ok(resp) => {
                    if self.cf_search.appending {
                        self.cf_results.extend(resp.data);
                    } else {
                        self.cf_results = resp.data;
                    }
                    self.cf_search.total = resp.pagination.total_count;
                    self.cf_search.appending = false;
                }
                Err(e) => {
                    pending_toasts.push(crate::app::Toast::error(format!(
                        "CurseForge search failed: {e}"
                    )));
                }
            }
        }

        ui.separator();

        if is_searching {
            ui.horizontal(|ui| {
                if let Some(t) = theme {
                    ui.add(egui::Spinner::new().color(t.color("accent")));
                } else {
                    ui.spinner();
                }
                if let Some(t) = theme {
                    ui.label(t.subtext("Searching..."));
                } else {
                    ui.label("Searching...");
                }
            });
        } else if !self.cf_results.is_empty() {
            if let Some(t) = theme {
                ui.label(t.subtext(&format!(
                    "Showing {} of {}",
                    self.cf_results.len(),
                    self.cf_search.total
                )));
            } else {
                ui.weak(format!(
                    "Showing {} of {}",
                    self.cf_results.len(),
                    self.cf_search.total
                ));
            }

            let results = &self.cf_results;
            let mut open_cf_picker = None;
            let has_more = self.cf_results.len() < self.cf_search.total as usize;

            match self.view_mode {
                ViewMode::List => {
                    ui.add_space(4.0);
                    let outer_frame = card_frame(ui, theme);
                    outer_frame.show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("cf_modpack_search_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for (idx, cf_mod) in results.iter().enumerate() {
                                    if idx > 0 {
                                        ui.separator();
                                    }
                                    let tip_icon_url =
                                        cf_mod.logo.as_ref().map(|l| l.best_url().to_string());
                                    let tip_title = cf_mod.name.clone();
                                    let tip_desc = cf_mod.summary.clone();
                                    let tip_downloads = cf_mod.download_count;
                                    let tip_tags: Vec<String> =
                                        cf_mod.categories.iter().map(|c| c.name.clone()).collect();
                                    let tip_theme = theme.cloned();

                                    let row_resp = ui.horizontal(|ui| {
                                        let icon_resp = if let Some(logo) = &cf_mod.logo {
                                            ui.add(
                                                egui::Image::new(logo.best_url())
                                                    .fit_to_exact_size(egui::vec2(40.0, 40.0)),
                                            )
                                        } else {
                                            crate::ui::helpers::icon_placeholder(
                                                ui,
                                                &cf_mod.name,
                                                40.0,
                                                theme,
                                            )
                                        };
                                        icon_resp.on_hover_ui(|ui| {
                                            project_tooltip(
                                                ui,
                                                tip_icon_url.as_deref(),
                                                &tip_title,
                                                &tip_desc,
                                                tip_downloads,
                                                &tip_tags,
                                                tip_theme.as_ref(),
                                            );
                                        });
                                        ui.vertical(|ui| {
                                            ui.set_max_width(ui.available_width() - 220.0);
                                            if let Some(t) = theme {
                                                ui.label(t.title(&cf_mod.name));
                                                ui.label(
                                                    t.subtext(&truncate_desc(&cf_mod.summary, 120)),
                                                );
                                                ui.label(t.subtext(&format!(
                                                    "{} downloads",
                                                    format_downloads(cf_mod.download_count)
                                                )));
                                            } else {
                                                ui.strong(&cf_mod.name);
                                                ui.weak(truncate_desc(&cf_mod.summary, 120));
                                                ui.weak(format!(
                                                    "{} downloads",
                                                    format_downloads(cf_mod.download_count)
                                                ));
                                            }
                                            let tags: Vec<&str> = cf_mod
                                                .categories
                                                .iter()
                                                .map(|c| c.name.as_str())
                                                .collect();
                                            show_category_tags(ui, &tags, 3, theme);
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let install_clicked = if let Some(t) = theme {
                                                    ui.add(t.accent_button("Install")).clicked()
                                                } else {
                                                    ui.button("Install").clicked()
                                                };
                                                if install_clicked {
                                                    open_cf_picker = Some((
                                                        cf_mod.id,
                                                        cf_mod.name.clone(),
                                                        cf_mod
                                                            .logo
                                                            .as_ref()
                                                            .map(|l| l.best_url().to_string()),
                                                    ));
                                                }
                                                let open_page_lbl = egui_phosphor::regular::GLOBE;
                                                let open_page = if let Some(t) = theme {
                                                    ui.add(t.ghost_button(open_page_lbl))
                                                } else {
                                                    ui.button(open_page_lbl)
                                                };
                                                if open_page.on_hover_text("Open Page").clicked() {
                                                    let url = curseforge::curseforge_modpack_url(
                                                        cf_mod.id,
                                                        &cf_mod.slug,
                                                    );
                                                    let _ = open::that(&url);
                                                }
                                            },
                                        );
                                    });
                                    row_hover_highlight(ui, row_resp.response.rect, theme);
                                }
                                if has_more {
                                    if load_more_button(
                                        ui,
                                        results.len(),
                                        self.cf_search.total as usize,
                                        theme,
                                    ) {
                                        load_more_triggered = true;
                                    }
                                }
                            });
                    });
                }
                ViewMode::Grid => {
                    if card_grid(
                        ui,
                        "cf_modpack_search_scroll_grid",
                        &results,
                        240.0,
                        210.0,
                        theme,
                        has_more,
                        self.cf_search.total as usize,
                        |ui, _i, cf_mod| {
                            let tip_icon_url =
                                cf_mod.logo.as_ref().map(|l| l.best_url().to_string());
                            let tip_title = cf_mod.name.clone();
                            let tip_desc = cf_mod.summary.clone();
                            let tip_downloads = cf_mod.download_count;
                            let tip_tags: Vec<String> =
                                cf_mod.categories.iter().map(|c| c.name.clone()).collect();
                            let tip_theme = theme.cloned();

                            ui.horizontal(|ui| {
                                let icon_resp = if let Some(logo) = &cf_mod.logo {
                                    ui.add(
                                        egui::Image::new(logo.best_url())
                                            .fit_to_exact_size(egui::vec2(32.0, 32.0)),
                                    )
                                } else {
                                    crate::ui::helpers::icon_placeholder(
                                        ui,
                                        &cf_mod.name,
                                        32.0,
                                        theme,
                                    )
                                };
                                icon_resp.on_hover_ui(|ui| {
                                    project_tooltip(
                                        ui,
                                        tip_icon_url.as_deref(),
                                        &tip_title,
                                        &tip_desc,
                                        tip_downloads,
                                        &tip_tags,
                                        tip_theme.as_ref(),
                                    );
                                });
                                if let Some(t) = theme {
                                    ui.add(egui::Label::new(t.title(&cf_mod.name)).truncate());
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&cf_mod.name).strong(),
                                        )
                                        .truncate(),
                                    );
                                }
                            });
                            ui.add_space(8.0);
                            if let Some(t) = theme {
                                ui.label(t.subtext(&truncate_desc(&cf_mod.summary, 75)));
                            } else {
                                ui.weak(truncate_desc(&cf_mod.summary, 75));
                            }
                            ui.add_space(4.0);
                            let tags: Vec<&str> =
                                cf_mod.categories.iter().map(|c| c.name.as_str()).collect();
                            show_category_tags(ui, &tags, 2, theme);
                            let dl =
                                format!("{} downloads", format_downloads(cf_mod.download_count));
                            if let Some(t) = theme {
                                ui.label(
                                    egui::RichText::new(dl).size(10.0).color(t.color("fg_dim")),
                                );
                            } else {
                                ui.label(egui::RichText::new(dl).size(10.0));
                            }
                        },
                        |ui, _i, cf_mod| {
                            let open_page = if let Some(t) = theme {
                                ui.add(t.ghost_button(egui_phosphor::regular::GLOBE))
                            } else {
                                ui.button(egui_phosphor::regular::GLOBE)
                            };
                            if open_page.on_hover_text("Open Page").clicked() {
                                let url =
                                    curseforge::curseforge_modpack_url(cf_mod.id, &cf_mod.slug);
                                let _ = open::that(&url);
                            }
                            let install_clicked = if let Some(t) = theme {
                                ui.add(t.accent_button("Install")).clicked()
                            } else {
                                ui.button("Install").clicked()
                            };
                            if install_clicked {
                                open_cf_picker = Some((
                                    cf_mod.id,
                                    cf_mod.name.clone(),
                                    cf_mod.logo.as_ref().map(|l| l.best_url().to_string()),
                                ));
                            }
                        },
                    ) {
                        load_more_triggered = true;
                    }
                }
            }

            if let Some((mod_id, title, icon_url)) = open_cf_picker {
                self.open_cf_version_picker(mod_id, title, icon_url, ui.ctx());
            }
        } else if !self.cf_search.is_searching() && self.cf_search.initialized {
            empty_state(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                "No results. Try a different search term.",
                theme,
            );
        }

        if load_more_triggered {
            self.cf_search.offset = self.cf_results.len() as u32;
            self.cf_search.appending = true;
        }
        if do_search || load_more_triggered {
            if do_search {
                self.cf_search.offset = 0;
                self.cf_search.appending = false;
                self.cf_search.last_edit = None;
            }
            let query = self.cf_search.query.clone();
            let cf_sort = self.cf_sort;
            let cf_category_id = self.cf_selected_category;
            let offset = self.cf_search.offset;
            self.cf_search.fire_with_repaint(ui.ctx(), move || {
                curseforge::search_cf_mods(
                    &query,
                    "",
                    None,
                    CLASS_MODPACKS,
                    offset,
                    cf_sort,
                    cf_category_id,
                )
                .map_err(|e| e.to_string())
            });
        }
    }

    fn open_mr_version_picker(
        &mut self,
        project_id: String,
        title: String,
        icon_url: Option<String>,
        ctx: &egui::Context,
    ) {
        let slot: Arc<Mutex<Option<VersionFetchResult>>> = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&slot);
        let pid = project_id.clone();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let result =
                modrinth::get_project_versions(&pid, None, None).map_err(|e| e.to_string());
            *slot_clone.lock().unwrap() = Some(VersionFetchResult::MrVersions(result));
            ctx_clone.request_repaint();
        });

        self.version_picker = Some(VersionPickerState {
            title,
            icon_url,
            source: ModpackSource::Modrinth,
            mr_project_id: Some(project_id),
            mr_versions: Vec::new(),
            cf_mod_id: None,
            cf_files: Vec::new(),
            fetch_handle: Some(slot),
            selected_index: 0,
        });
    }

    fn open_cf_version_picker(
        &mut self,
        mod_id: u64,
        title: String,
        icon_url: Option<String>,
        ctx: &egui::Context,
    ) {
        let slot: Arc<Mutex<Option<VersionFetchResult>>> = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&slot);
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let result = curseforge::get_cf_mod_files(mod_id, "", None).map_err(|e| e.to_string());
            *slot_clone.lock().unwrap() = Some(VersionFetchResult::CfFiles(result));
            ctx_clone.request_repaint();
        });

        self.version_picker = Some(VersionPickerState {
            title,
            icon_url,
            source: ModpackSource::CurseForge,
            mr_project_id: None,
            mr_versions: Vec::new(),
            cf_mod_id: Some(mod_id),
            cf_files: Vec::new(),
            fetch_handle: Some(slot),
            selected_index: 0,
        });
    }

    fn poll_version_picker(&mut self) {
        let Some(vp) = &mut self.version_picker else {
            return;
        };
        let Some(handle) = &vp.fetch_handle else {
            return;
        };
        let taken = handle.lock().unwrap().take();
        if let Some(result) = taken {
            vp.fetch_handle = None;
            match result {
                VersionFetchResult::MrVersions(Ok(versions)) => {
                    vp.mr_versions = versions;
                }
                VersionFetchResult::CfFiles(Ok(files)) => {
                    vp.cf_files = files;
                }
                VersionFetchResult::MrVersions(Err(e)) | VersionFetchResult::CfFiles(Err(e)) => {
                    log::warn!("Failed to fetch versions: {e}");
                    self.version_picker = None;
                }
            }
        }
    }

    fn show_version_picker(&mut self, ui: &mut egui::Ui, theme: Option<&Theme>) {
        if self.version_picker.is_none() {
            return;
        }

        let mut action: Option<VersionPickerAction> = None;

        let vp = self.version_picker.as_mut().unwrap();
        let is_loading = vp.fetch_handle.is_some();
        let title = vp.title.clone();

        egui::Window::new(format!("Install \"{}\"", title))
            .collapsible(false)
            .resizable(true)
            .default_size([450.0, 400.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                if is_loading {
                    ui.horizontal(|ui| {
                        if let Some(t) = theme {
                            ui.add(egui::Spinner::new().color(t.color("accent")));
                        } else {
                            ui.spinner();
                        }
                        ui.label("Fetching versions...");
                    });
                    return;
                }

                if let Some(t) = theme {
                    ui.label(t.subtext("Select a version to install:"));
                } else {
                    ui.weak("Select a version to install:");
                }
                ui.add_space(4.0);

                match vp.source {
                    ModpackSource::Modrinth => {
                        if vp.mr_versions.is_empty() {
                            ui.label("No versions available.");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for (i, version) in vp.mr_versions.iter().enumerate() {
                                        let selected = vp.selected_index == i;
                                        let game_vers = version.game_versions.join(", ");
                                        let loaders = version.loaders.join(", ");
                                        let label = format!(
                                            "{} (MC {}) [{}]",
                                            version.name, game_vers, loaders
                                        );
                                        if ui.selectable_label(selected, &label).clicked() {
                                            vp.selected_index = i;
                                        }
                                    }
                                });
                        }
                    }
                    ModpackSource::CurseForge => {
                        if vp.cf_files.is_empty() {
                            ui.label("No versions available.");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for (i, file) in vp.cf_files.iter().enumerate() {
                                        let selected = vp.selected_index == i;
                                        let release_tag = match file.release_type {
                                            1 => "Release",
                                            2 => "Beta",
                                            3 => "Alpha",
                                            _ => "Unknown",
                                        };
                                        let game_vers = file.game_versions.join(", ");
                                        let label = format!(
                                            "{} [{}] ({})",
                                            file.display_name, release_tag, game_vers
                                        );
                                        if ui.selectable_label(selected, &label).clicked() {
                                            vp.selected_index = i;
                                        }
                                    }
                                });
                        }
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let has_versions = match vp.source {
                        ModpackSource::Modrinth => !vp.mr_versions.is_empty(),
                        ModpackSource::CurseForge => !vp.cf_files.is_empty(),
                    };
                    let install_clicked = if let Some(t) = theme {
                        ui.add_enabled(has_versions, t.accent_button("Install"))
                            .clicked()
                    } else {
                        ui.add_enabled(has_versions, egui::Button::new("Install"))
                            .clicked()
                    };
                    if install_clicked {
                        action = Some(VersionPickerAction::Install);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(VersionPickerAction::Cancel);
                    }
                });
            });

        match action {
            Some(VersionPickerAction::Install) => {
                let vp = self.version_picker.take().unwrap();
                match vp.source {
                    ModpackSource::Modrinth => {
                        if let Some(version) = vp.mr_versions.get(vp.selected_index) {
                            self.install_requested = Some(ModpackInstallRequest {
                                project_id: vp.mr_project_id.unwrap_or_default(),
                                title: vp.title,
                                icon_url: vp.icon_url,
                                version_id: Some(version.id.clone()),
                                version_name: Some(version.name.clone()),
                            });
                        }
                    }
                    ModpackSource::CurseForge => {
                        if let Some(file) = vp.cf_files.get(vp.selected_index) {
                            self.cf_install_requested = Some(CfModpackInstallRequest {
                                mod_id: vp.cf_mod_id.unwrap_or(0),
                                title: vp.title,
                                icon_url: vp.icon_url,
                                file_id: Some(file.id),
                                file_name: Some(file.display_name.clone()),
                            });
                        }
                    }
                }
            }
            Some(VersionPickerAction::Cancel) => {
                self.version_picker = None;
            }
            None => {}
        }
    }
}

enum VersionPickerAction {
    Install,
    Cancel,
}
