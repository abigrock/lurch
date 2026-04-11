use eframe::egui;
use std::time::Instant;

use crate::theme::Theme;
use crate::ui::helpers::{
    card_frame, card_grid, empty_state, format_downloads, icon_placeholder, load_more_button,
    project_tooltip, row_hover_highlight, show_category_tags, truncate_desc, SearchState, ViewMode,
};

// ── Normalised display item ─────────────────────────────────────────

/// A type-erased browse item that all four browse views can produce.
pub struct BrowseItem {
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub categories: Vec<String>,
    /// Primary identifier used for install actions (project_id / mod_id).
    pub id: String,
    /// URL-friendly slug used for "open page" links.
    pub slug: String,
    /// When false the item cannot be directly installed (CF distribution flag).
    /// The Install button becomes "Open in browser" and the globe button is hidden.
    pub allows_install: bool,
}

// ── Search result wrapper ───────────────────────────────────────────

pub struct BrowseSearchResult {
    pub items: Vec<BrowseItem>,
    pub total: u32,
}

// ── Per-frame configuration (borrowed) ──────────────────────────────

pub struct BrowseConfig<'a> {
    pub id_salt: &'a str,
    pub search_hint: &'a str,
    pub sort_labels: &'a [&'a str],
    pub category_labels: &'a [&'a str],
    pub has_version_filter: bool,
    pub version_filter_label: &'a str,
}

// ── Actions returned from show() ────────────────────────────────────

#[allow(dead_code)]
pub enum BrowseAction {
    /// Caller should fire a search with the current query / filters.
    FireSearch,
    /// Caller should open the install flow for item at the given index.
    Install(usize),
    /// Caller should open the project page for item at the given index.
    OpenPage(usize),
    /// Search polling returned an error.
    SearchError(String),
    /// The "show all versions" checkbox changed.
    VersionFilterChanged(bool),
}

// ── Persistent browse state ─────────────────────────────────────────

pub struct BrowseTab {
    pub search: SearchState<BrowseSearchResult>,
    pub results: Vec<BrowseItem>,
    pub view_mode: ViewMode,
    pub selected_sort: usize,
    pub selected_category: usize,
    pub search_all_versions: bool,
}

impl Default for BrowseTab {
    fn default() -> Self {
        Self {
            search: SearchState::default(),
            results: Vec::new(),
            view_mode: ViewMode::List,
            selected_sort: 0,
            selected_category: 0,
            search_all_versions: false,
        }
    }
}

impl BrowseTab {
    /// Render the full browse UI and return actions for the caller to handle.
    ///
    /// The caller is responsible for:
    /// - building `BrowseConfig` each frame
    /// - handling returned `BrowseAction`s (fire search, install, open page, etc.)
    /// - category fetching (populates `config.category_labels`)
    /// - toast / status polling (mod browsers read egui temp data)
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        config: &BrowseConfig<'_>,
    ) -> Vec<BrowseAction> {
        let mut actions: Vec<BrowseAction> = Vec::new();
        let mut do_search = false;
        let mut load_more_triggered = false;

        ui.add_space(4.0);

        // Auto-initialise on first frame
        if !self.search.initialized {
            self.search.initialized = true;
            do_search = true;
        }

        // ── Search bar row ───────────────────────────────────────
        let search_row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), search_row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
                let resp = ui.add_sized(
                    [ui.available_width() - 28.0, 32.0],
                    egui::TextEdit::singleline(&mut self.search.query)
                        .hint_text(config.search_hint)
                        .margin(egui::Margin::symmetric(4, 9)),
                );
                if resp.changed() {
                    self.search.last_edit = Some(Instant::now());
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    do_search = true;
                }
                if self.search.is_searching() {
                    ui.add(egui::Spinner::new().color(theme.color("accent")));
                }
            },
        );

        // ── Filter / sort / view-mode row ────────────────────────
        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
                // Version filter (mod browsers only)
                if config.has_version_filter {
                    if self.search_all_versions {
                        ui.label(theme.subtext("Searching all versions and loaders"));
                    } else {
                        ui.label(theme.subtext(config.version_filter_label));
                    }
                    let prev = self.search_all_versions;
                    ui.checkbox(&mut self.search_all_versions, "Show all versions");
                    if prev != self.search_all_versions {
                        do_search = true;
                        actions.push(BrowseAction::VersionFilterChanged(self.search_all_versions));
                    }
                    ui.separator();
                }

                // Sort combo
                let prev_sort = self.selected_sort;
                let sort_text = config
                    .sort_labels
                    .get(self.selected_sort)
                    .copied()
                    .unwrap_or("Sort");
                egui::ComboBox::from_id_salt(format!("{}_sort", config.id_salt))
                    .selected_text(sort_text)
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for (i, &label) in config.sort_labels.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_sort, i, label);
                        }
                    });
                if prev_sort != self.selected_sort {
                    do_search = true;
                }

                ui.separator();

                // Category combo
                let prev_cat = self.selected_category;
                let cat_text = if self.selected_category == 0 {
                    "All categories"
                } else {
                    config
                        .category_labels
                        .get(self.selected_category)
                        .copied()
                        .unwrap_or("All categories")
                };
                egui::ComboBox::from_id_salt(format!("{}_category", config.id_salt))
                    .selected_text(cat_text)
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_category, 0, "All categories");
                        for (i, &label) in config.category_labels.iter().enumerate().skip(1) {
                            ui.selectable_value(&mut self.selected_category, i, label);
                        }
                    });
                if prev_cat != self.selected_category {
                    do_search = true;
                }

                // View mode toggle (right-aligned)
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

        // ── Poll search results ──────────────────────────────────
        if let Some(result) = self.search.poll() {
            match result {
                Ok(resp) => {
                    if self.search.appending {
                        self.results.extend(resp.items);
                    } else {
                        self.results = resp.items;
                    }
                    self.search.total = resp.total;
                    self.search.appending = false;
                }
                Err(e) => {
                    actions.push(BrowseAction::SearchError(e));
                }
            }
        }

        ui.add_space(4.0);

        // ── Results display ──────────────────────────────────────
        if !self.results.is_empty() {
            ui.label(theme.subtext(&format!(
                "Showing {} of {}",
                self.results.len(),
                self.search.total
            )));

            let has_more = self.results.len() < self.search.total as usize;

            match self.view_mode {
                ViewMode::List => {
                    load_more_triggered |=
                        self.show_list(ui, theme, config, has_more, &mut actions);
                }
                ViewMode::Grid => {
                    load_more_triggered |=
                        self.show_grid(ui, theme, config, has_more, &mut actions);
                }
            }
        } else if !self.search.is_searching() && self.search.initialized {
            empty_state(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                "No results. Try a different search term.",
                theme,
            );
        }

        // ── Search triggers ──────────────────────────────────────
        if load_more_triggered {
            self.search.offset = self.results.len() as u32;
            self.search.appending = true;
        }
        if self.search.check_debounce(ui.ctx()) {
            do_search = true;
        }
        if do_search || load_more_triggered {
            if do_search {
                self.search.offset = 0;
                self.search.appending = false;
                self.search.last_edit = None;
            }
            actions.push(BrowseAction::FireSearch);
        }

        if self.search.is_searching() {
            ui.ctx().request_repaint();
        }

        actions
    }

    // ── List view ────────────────────────────────────────────────

    fn show_list(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        config: &BrowseConfig<'_>,
        has_more: bool,
        actions: &mut Vec<BrowseAction>,
    ) -> bool {
        let mut load_more = false;

        card_frame(ui, theme).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(format!("{}_list", config.id_salt))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (idx, item) in self.results.iter().enumerate() {
                        if idx > 0 {
                            ui.separator();
                        }
                        let tip_icon_url = item.icon_url.clone();
                        let tip_title = item.title.clone();
                        let tip_desc = item.description.clone();
                        let tip_downloads = item.downloads;
                        let tip_tags: Vec<String> = item.categories.clone();

                        let row_resp = ui.horizontal(|ui| {
                            // Icon
                            let icon_resp = if let Some(url) = &item.icon_url {
                                ui.add(
                                    egui::Image::new(url).fit_to_exact_size(egui::vec2(40.0, 40.0)),
                                )
                            } else {
                                icon_placeholder(ui, &item.title, 40.0, theme)
                            };
                            icon_resp.on_hover_ui(|ui| {
                                project_tooltip(
                                    ui,
                                    tip_icon_url.as_deref(),
                                    &tip_title,
                                    &tip_desc,
                                    tip_downloads,
                                    &tip_tags,
                                    theme,
                                );
                            });
                            // Info column
                            ui.vertical(|ui| {
                                ui.set_max_width(ui.available_width() - 220.0);
                                ui.label(theme.title(&item.title));
                                ui.label(theme.subtext(&truncate_desc(&item.description, 120)));
                                ui.label(theme.subtext(&format!(
                                    "{} downloads",
                                    format_downloads(item.downloads)
                                )));
                                let tags: Vec<&str> =
                                    item.categories.iter().map(|s| s.as_str()).collect();
                                show_category_tags(ui, &tags, 3, theme);
                            });
                            // Action buttons (right-aligned)
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if item.allows_install {
                                        if ui.add(theme.accent_button("Install")).clicked() {
                                            actions.push(BrowseAction::Install(idx));
                                        }
                                    } else {
                                        if ui.add(theme.accent_button("Open in browser")).clicked()
                                        {
                                            actions.push(BrowseAction::OpenPage(idx));
                                        }
                                    }
                                    if item.allows_install {
                                        let open_page = ui
                                            .add(theme.ghost_button(egui_phosphor::regular::GLOBE));
                                        if open_page.on_hover_text("Open Page").clicked() {
                                            actions.push(BrowseAction::OpenPage(idx));
                                        }
                                    }
                                },
                            );
                        });
                        row_hover_highlight(ui, row_resp.response.rect, theme);
                    }

                    if has_more
                        && load_more_button(
                            ui,
                            self.results.len(),
                            self.search.total as usize,
                            theme,
                        )
                    {
                        load_more = true;
                    }
                });
        });

        load_more
    }

    // ── Grid view ────────────────────────────────────────────────

    fn show_grid(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        config: &BrowseConfig<'_>,
        has_more: bool,
        actions: &mut Vec<BrowseAction>,
    ) -> bool {
        card_grid(
            ui,
            &format!("{}_grid", config.id_salt),
            &self.results,
            240.0,
            210.0,
            theme,
            has_more,
            self.search.total as usize,
            |ui, _idx, item| {
                let tip_icon_url = item.icon_url.clone();
                let tip_title = item.title.clone();
                let tip_desc = item.description.clone();
                let tip_downloads = item.downloads;
                let tip_tags: Vec<String> = item.categories.clone();

                ui.horizontal(|ui| {
                    let icon_resp = if let Some(url) = &item.icon_url {
                        ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(32.0, 32.0)))
                    } else {
                        icon_placeholder(ui, &item.title, 32.0, theme)
                    };
                    icon_resp.on_hover_ui(|ui| {
                        project_tooltip(
                            ui,
                            tip_icon_url.as_deref(),
                            &tip_title,
                            &tip_desc,
                            tip_downloads,
                            &tip_tags,
                            theme,
                        );
                    });
                    ui.add(egui::Label::new(theme.title(&item.title)).truncate());
                });
                ui.add_space(8.0);
                ui.label(theme.subtext(&truncate_desc(&item.description, 75)));
                ui.add_space(4.0);
                let tags: Vec<&str> = item.categories.iter().map(|s| s.as_str()).collect();
                show_category_tags(ui, &tags, 2, theme);
                let dl = format!("{} downloads", format_downloads(item.downloads));
                ui.label(
                    egui::RichText::new(dl)
                        .size(10.0)
                        .color(theme.color("fg_dim")),
                );
            },
            |ui, idx, item| {
                if item.allows_install {
                    if ui.add(theme.accent_button("Install")).clicked() {
                        actions.push(BrowseAction::Install(idx));
                    }
                } else {
                    if ui.add(theme.accent_button("Open in browser")).clicked() {
                        actions.push(BrowseAction::OpenPage(idx));
                    }
                }
                if item.allows_install {
                    let open_page = ui.add(theme.ghost_button(egui_phosphor::regular::GLOBE));
                    if open_page.on_hover_text("Open Page").clicked() {
                        actions.push(BrowseAction::OpenPage(idx));
                    }
                }
            },
        )
    }
}
