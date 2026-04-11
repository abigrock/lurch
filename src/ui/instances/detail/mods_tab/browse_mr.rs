use eframe::egui;
use std::sync::{Arc, Mutex};

use super::super::InstanceDetailView;
use crate::core::instance::Instance;
use crate::core::MutexExt;
use crate::core::modrinth;
use crate::ui::browse_common::{BrowseAction, BrowseConfig, BrowseItem, BrowseSearchResult};

impl InstanceDetailView {
    pub(super) fn show_browse_tab(
        &mut self,
        ui: &mut egui::Ui,
        instance: &Instance,
        mods_dir: &std::path::Path,
        theme: &crate::theme::Theme,
    ) {
        // ── Fetch MR categories if needed ────────────────────────
        if self.mr_categories.is_none() && self.mr_categories_fetch.is_none() {
            let slot: Arc<Mutex<Option<Result<Vec<modrinth::MrCategory>, String>>>> =
                Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = modrinth::fetch_mr_categories("mod").map_err(|e| e.to_string());
                *slot_c.lock_or_recover() = Some(result);
                ctx.request_repaint();
            });
            self.mr_categories_fetch = Some(slot);
        }
        if let Some(result) = self
            .mr_categories_fetch
            .as_ref()
            .and_then(|f| f.lock_or_recover().take())
        {
            match result {
                Ok(cats) => self.mr_categories = Some(cats),
                Err(_) => self.mr_categories = Some(Vec::new()),
            }
            self.mr_categories_fetch = None;
        }

        // ── Build category labels (index 0 = "All categories") ──
        let mut cat_labels: Vec<String> = vec!["All categories".into()];
        if let Some(ref cats) = self.mr_categories {
            cat_labels.extend(cats.iter().map(|c| c.name.clone()));
        }
        let cat_refs: Vec<&str> = cat_labels.iter().map(|s| s.as_str()).collect();

        // ── Sort labels ──────────────────────────────────────────
        let sort_labels: Vec<&str> = modrinth::MrSortIndex::ALL.iter().map(|s| s.label()).collect();

        // ── Version filter label ─────────────────────────────────
        let loader_str = instance.loader.to_string();
        let version_str = &instance.mc_version;
        let filter_label = if loader_str.to_lowercase() == "vanilla" {
            format!("Filtering: {version_str}")
        } else {
            format!("Filtering: {version_str} + {loader_str}")
        };

        let config = BrowseConfig {
            id_salt: "mr_mod_browse",
            search_hint: "Search mods\u{2026}",
            sort_labels: &sort_labels,
            category_labels: &cat_refs,
            has_version_filter: true,
            version_filter_label: &filter_label,
        };

        let actions = self.mr_browse.show(ui, theme, &config);

        // ── Handle actions ───────────────────────────────────────
        for action in actions {
            match action {
                BrowseAction::FireSearch => {
                    let query = self.mr_browse.search.query.clone();
                    let mc_version = if self.mr_browse.search_all_versions {
                        String::new()
                    } else {
                        instance.mc_version.clone()
                    };
                    let loader = if self.mr_browse.search_all_versions {
                        String::new()
                    } else {
                        instance.loader.to_string().to_lowercase()
                    };
                    let offset = self.mr_browse.search.offset;
                    let mr_sort = modrinth::MrSortIndex::ALL
                        .get(self.mr_browse.selected_sort)
                        .copied()
                        .unwrap_or_default();
                    let mr_category = if self.mr_browse.selected_category == 0 {
                        None
                    } else {
                        self.mr_categories.as_ref().and_then(|cats| {
                            cats.get(self.mr_browse.selected_category - 1)
                                .map(|c| c.name.clone())
                        })
                    };
                    self.mr_browse.search.fire_with_repaint(ui.ctx(), move || {
                        let resp = modrinth::search_mods(
                            &query,
                            &mc_version,
                            &loader,
                            "mod",
                            offset,
                            mr_sort,
                            mr_category.as_deref(),
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(BrowseSearchResult {
                            items: resp
                                .hits
                                .into_iter()
                                .map(|hit| BrowseItem {
                                    title: hit.title,
                                    description: hit.description,
                                    icon_url: hit.icon_url,
                                    downloads: hit.downloads,
                                    categories: hit.categories,
                                    id: hit.project_id,
                                    slug: hit.slug,
                                    allows_install: true,
                                })
                                .collect(),
                            total: resp.total_hits,
                        })
                    });
                }
                BrowseAction::Install(idx) => {
                    if let Some(item) = self.mr_browse.results.get(idx) {
                        let project_id = item.id.clone();
                        let title = item.title.clone();
                        let mc_ver = instance.mc_version.clone();
                        let loader = instance.loader.to_string().to_lowercase();
                        let mods_dir = mods_dir.to_path_buf();
                        self.open_mr_mod_version_picker(
                            project_id,
                            title,
                            &mc_ver,
                            &loader,
                            &mods_dir,
                            ui.ctx(),
                        );
                    }
                }
                BrowseAction::OpenPage(idx) => {
                    if let Some(item) = self.mr_browse.results.get(idx) {
                        let url = modrinth::modrinth_project_url(&item.slug);
                        let _ = open::that(&url);
                    }
                }
                BrowseAction::SearchError(e) => {
                    self.pending_toasts
                        .push(crate::app::Toast::error(format!("Search failed: {e}")));
                }
                BrowseAction::VersionFilterChanged(_) => {
                    // handled implicitly — FireSearch will also be emitted
                }
            }
        }

        // ── Toast status polling (install feedback) ──────────────
        let maybe_status: Option<Arc<Mutex<Option<String>>>> = ui
            .ctx()
            .data(|d| d.get_temp(egui::Id::new("install_status")));
        if let Some(arc) = maybe_status
            && let Some(msg) = arc.lock().ok().and_then(|mut g| g.take())
        {
            if msg.starts_with("Error")
                || msg.starts_with("Install failed")
                || msg.starts_with("Search failed")
            {
                self.pending_toasts.push(crate::app::Toast::error(msg));
            } else {
                self.pending_toasts.push(crate::app::Toast::success(msg));
            }
            self.needs_rescan = true;
            ui.ctx().data_mut(|d| {
                d.remove::<Arc<Mutex<Option<String>>>>(egui::Id::new("install_status"))
            });
        }
    }
}
