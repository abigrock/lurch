use crate::core::curseforge::{self, CfCategory, CfFile, CfSortField, CLASS_MODPACKS};
use crate::core::MutexExt;
use crate::core::modrinth::{self, MrCategory, MrSortIndex, ProjectVersion};
use crate::theme::Theme;
use crate::ui::browse_common::{
    BrowseAction, BrowseConfig, BrowseItem, BrowseSearchResult, BrowseTab,
};
use eframe::egui;
use std::sync::{Arc, Mutex};

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
    pub mr_browse: BrowseTab,
    pub cf_browse: BrowseTab,
    pub install_requested: Option<ModpackInstallRequest>,
    pub cf_install_requested: Option<CfModpackInstallRequest>,
    pub version_picker: Option<VersionPickerState>,
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
            mr_browse: BrowseTab::default(),
            cf_browse: BrowseTab::default(),
            install_requested: None,
            cf_install_requested: None,
            version_picker: None,
            mr_categories: None,
            mr_categories_fetch: None,
            mr_selected_category: None,
            cf_categories: None,
            cf_categories_fetch: None,
            cf_selected_category: None,
        }
    }
}

impl ModpackBrowser {
    pub fn show_for_source(
        &mut self,
        ui: &mut egui::Ui,
        source: ModpackSource,
        theme: &Theme,
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

        if self.mr_browse.search.is_searching() || self.cf_browse.search.is_searching() {
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
        theme: &Theme,
        pending_toasts: &mut Vec<crate::app::Toast>,
    ) {
        // 1. Category fetch + poll
        if self.mr_categories.is_none() && self.mr_categories_fetch.is_none() {
            let slot: Arc<Mutex<Option<Result<Vec<MrCategory>, String>>>> =
                Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = modrinth::fetch_mr_categories("modpack").map_err(|e| e.to_string());
                *slot_c.lock_or_recover() = Some(result);
                ctx.request_repaint();
            });
            self.mr_categories_fetch = Some(slot);
        }
        let mr_cat_result = self
            .mr_categories_fetch
            .as_ref()
            .and_then(|f| f.lock_or_recover().take());
        if let Some(result) = mr_cat_result {
            match result {
                Ok(cats) => self.mr_categories = Some(cats),
                Err(_) => self.mr_categories = Some(Vec::new()),
            }
            self.mr_categories_fetch = None;
        }

        // 2. Build category labels
        let mut cat_label_strs: Vec<&str> = vec!["All categories"];
        if let Some(ref cats) = self.mr_categories {
            for cat in cats {
                cat_label_strs.push(&cat.name);
            }
        }

        // 3. Build sort labels from MrSortIndex::ALL
        let sort_label_strs: Vec<&str> = MrSortIndex::ALL.iter().map(|s| s.label()).collect();

        // 4. Build config
        let config = BrowseConfig {
            id_salt: "mr_modpack",
            search_hint: "Search modpacks\u{2026}",
            sort_labels: &sort_label_strs,
            category_labels: &cat_label_strs,
            has_version_filter: false,
            version_filter_label: "",
        };

        // 5. Call show()
        let actions = self.mr_browse.show(ui, theme, &config);

        // 6. Handle actions
        for action in actions {
            match action {
                BrowseAction::FireSearch => {
                    let query = self.mr_browse.search.query.clone();
                    let mr_sort = MrSortIndex::ALL[self.mr_browse.selected_sort];
                    let mr_category = if self.mr_browse.selected_category == 0 {
                        None
                    } else {
                        self.mr_categories
                            .as_ref()
                            .and_then(|cats| cats.get(self.mr_browse.selected_category - 1))
                            .map(|c| c.name.clone())
                    };
                    // Sync mr_selected_category for next search
                    self.mr_selected_category = mr_category.clone();
                    let offset = self.mr_browse.search.offset;
                    self.mr_browse.search.fire_with_repaint(ui.ctx(), move || {
                        crate::core::modrinth::search_mods(
                            &query,
                            "",
                            "",
                            "modpack",
                            offset,
                            mr_sort,
                            mr_category.as_deref(),
                        )
                        .map(|resp| BrowseSearchResult {
                            items: resp
                                .hits
                                .into_iter()
                                .map(|hit| BrowseItem {
                                    id: hit.project_id.clone(),
                                    title: hit.title,
                                    description: hit.description,
                                    icon_url: hit.icon_url,
                                    downloads: hit.downloads,
                                    categories: hit.categories,
                                    slug: hit.slug,
                                    allows_install: true,
                                })
                                .collect(),
                            total: resp.total_hits,
                        })
                        .map_err(|e| e.to_string())
                    });
                }
                BrowseAction::Install(idx) => {
                    if let Some(item) = self.mr_browse.results.get(idx) {
                        self.open_mr_version_picker(
                            item.id.clone(),
                            item.title.clone(),
                            item.icon_url.clone(),
                            ui.ctx(),
                        );
                    }
                }
                BrowseAction::OpenPage(idx) => {
                    if let Some(item) = self.mr_browse.results.get(idx) {
                        let url = crate::core::modrinth::modrinth_project_url(&item.slug);
                        let _ = open::that(&url);
                    }
                }
                BrowseAction::SearchError(e) => {
                    pending_toasts.push(crate::app::Toast::error(format!("Search failed: {e}")));
                }
                BrowseAction::VersionFilterChanged(_) => {} // not used for modpacks
            }
        }
    }

    fn show_curseforge(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        pending_toasts: &mut Vec<crate::app::Toast>,
    ) {
        // 1. Category fetch + poll
        if self.cf_categories.is_none() && self.cf_categories_fetch.is_none() {
            let slot: Arc<Mutex<Option<Result<Vec<CfCategory>, String>>>> =
                Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result =
                    curseforge::fetch_cf_categories(CLASS_MODPACKS).map_err(|e| e.to_string());
                *slot_c.lock_or_recover() = Some(result);
                ctx.request_repaint();
            });
            self.cf_categories_fetch = Some(slot);
        }
        let cf_cats_result = self
            .cf_categories_fetch
            .as_ref()
            .and_then(|f| f.lock_or_recover().take());
        if let Some(result) = cf_cats_result {
            match result {
                Ok(cats) => self.cf_categories = Some(cats),
                Err(_) => self.cf_categories = Some(Vec::new()),
            }
            self.cf_categories_fetch = None;
        }

        // 2. Build category labels
        let mut cat_label_strs: Vec<&str> = vec!["All categories"];
        if let Some(ref cats) = self.cf_categories {
            for cat in cats {
                cat_label_strs.push(&cat.name);
            }
        }

        // 3. Build sort labels from CfSortField::ALL
        let sort_label_strs: Vec<&str> = CfSortField::ALL.iter().map(|s| s.label()).collect();

        // 4. Build config
        let config = BrowseConfig {
            id_salt: "cf_modpack",
            search_hint: "Search modpacks\u{2026}",
            sort_labels: &sort_label_strs,
            category_labels: &cat_label_strs,
            has_version_filter: false,
            version_filter_label: "",
        };

        // 5. Call show()
        let actions = self.cf_browse.show(ui, theme, &config);

        // 6. Handle actions
        for action in actions {
            match action {
                BrowseAction::FireSearch => {
                    let query = self.cf_browse.search.query.clone();
                    let cf_sort = CfSortField::ALL[self.cf_browse.selected_sort];
                    let cf_category_id = if self.cf_browse.selected_category == 0 {
                        None
                    } else {
                        self.cf_categories
                            .as_ref()
                            .and_then(|cats| cats.get(self.cf_browse.selected_category - 1))
                            .map(|c| c.id)
                    };
                    // Sync cf_selected_category for next search
                    self.cf_selected_category = cf_category_id;
                    let offset = self.cf_browse.search.offset;
                    self.cf_browse.search.fire_with_repaint(ui.ctx(), move || {
                        curseforge::search_cf_mods(
                            &query,
                            "",
                            None,
                            CLASS_MODPACKS,
                            offset,
                            cf_sort,
                            cf_category_id,
                        )
                        .map(|resp| BrowseSearchResult {
                            items: resp
                                .data
                                .into_iter()
                                .map(|hit| BrowseItem {
                                    id: hit.id.to_string(),
                                    title: hit.name,
                                    description: hit.summary,
                                    icon_url: hit.logo.as_ref().map(|l| l.best_url().to_string()),
                                    downloads: hit.download_count,
                                    categories: hit
                                        .categories
                                        .iter()
                                        .map(|c| c.name.clone())
                                        .collect(),
                                    slug: hit.slug,
                                    allows_install: true,
                                })
                                .collect(),
                            total: resp.pagination.total_count,
                        })
                        .map_err(|e| e.to_string())
                    });
                }
                BrowseAction::Install(idx) => {
                    if let Some(item) = self.cf_browse.results.get(idx) {
                        let mod_id: u64 = item.id.parse().unwrap_or(0);
                        self.open_cf_version_picker(
                            mod_id,
                            item.title.clone(),
                            item.icon_url.clone(),
                            ui.ctx(),
                        );
                    }
                }
                BrowseAction::OpenPage(idx) => {
                    if let Some(item) = self.cf_browse.results.get(idx) {
                        let mod_id: u64 = item.id.parse().unwrap_or(0);
                        let url = curseforge::curseforge_modpack_url(mod_id, &item.slug);
                        let _ = open::that(&url);
                    }
                }
                BrowseAction::SearchError(e) => {
                    pending_toasts.push(crate::app::Toast::error(format!(
                        "CurseForge search failed: {e}"
                    )));
                }
                BrowseAction::VersionFilterChanged(_) => {} // not used for modpacks
            }
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
            *slot_clone.lock_or_recover() = Some(VersionFetchResult::MrVersions(result));
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
            *slot_clone.lock_or_recover() = Some(VersionFetchResult::CfFiles(result));
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
        let taken = handle.lock_or_recover().take();
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

    fn show_version_picker(&mut self, ui: &mut egui::Ui, theme: &Theme) {
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
                        ui.add(egui::Spinner::new().color(theme.color("accent")));
                        ui.label("Fetching versions...");
                    });
                    return;
                }

                ui.label(theme.subtext("Select a version to install:"));
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
                    if ui
                        .add_enabled(has_versions, theme.accent_button("Install"))
                        .clicked()
                    {
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
