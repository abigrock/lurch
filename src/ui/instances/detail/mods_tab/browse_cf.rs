use eframe::egui;
use std::sync::{Arc, Mutex};

use super::super::InstanceDetailView;
use crate::core::curseforge;
use crate::core::MutexExt;
use crate::core::instance::Instance;
use crate::ui::browse_common::{BrowseAction, BrowseConfig, BrowseItem, BrowseSearchResult};

impl InstanceDetailView {
    pub(super) fn show_browse_curseforge_tab(
        &mut self,
        ui: &mut egui::Ui,
        instance: &Instance,
        mods_dir: &std::path::Path,
        theme: &crate::theme::Theme,
    ) {
        let loader_type = curseforge::mod_loader_type(&instance.loader);

        // ── Fetch CF categories if needed ────────────────────────
        if self.cf_categories.is_none() && self.cf_categories_fetch.is_none() {
            let slot: crate::core::BgTaskSlot<Vec<curseforge::CfCategory>> =
                Arc::new(Mutex::new(None));
            let slot_c = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let result = curseforge::fetch_cf_categories(curseforge::CLASS_MODS)
                    .map_err(|e| e.to_string());
                *slot_c.lock_or_recover() = Some(result);
                ctx.request_repaint();
            });
            self.cf_categories_fetch = Some(slot);
        }
        if let Some(result) = self
            .cf_categories_fetch
            .as_ref()
            .and_then(|f| f.lock_or_recover().take())
        {
            match result {
                Ok(cats) => self.cf_categories = Some(cats),
                Err(_) => self.cf_categories = Some(Vec::new()),
            }
            self.cf_categories_fetch = None;
        }

        // ── Build category labels (index 0 = "All categories") ──
        let mut cat_labels: Vec<String> = vec!["All categories".into()];
        if let Some(ref cats) = self.cf_categories {
            cat_labels.extend(cats.iter().map(|c| c.name.clone()));
        }
        let cat_refs: Vec<&str> = cat_labels.iter().map(|s| s.as_str()).collect();

        // ── Sort labels ──────────────────────────────────────────
        let sort_labels: Vec<&str> = curseforge::CfSortField::ALL.iter().map(|s| s.label()).collect();

        // ── Version filter label ─────────────────────────────────
        let loader_str = instance.loader.to_string();
        let version_str = &instance.mc_version;
        let filter_label = if loader_str.to_lowercase() == "vanilla" {
            format!("Filtering: {version_str}")
        } else {
            format!("Filtering: {version_str} + {loader_str}")
        };

        let config = BrowseConfig {
            id_salt: "cf_mod_browse",
            search_hint: "Search mods\u{2026}",
            sort_labels: &sort_labels,
            category_labels: &cat_refs,
            has_version_filter: true,
            version_filter_label: &filter_label,
        };

        let actions = self.cf_browse.show(ui, theme, &config);

        // ── Handle actions ───────────────────────────────────────
        for action in actions {
            match action {
                BrowseAction::FireSearch => {
                    let query = self.cf_browse.search.query.clone();
                    let mc_version = if self.cf_browse.search_all_versions {
                        String::new()
                    } else {
                        instance.mc_version.clone()
                    };
                    let ltype = if self.cf_browse.search_all_versions {
                        None
                    } else {
                        loader_type
                    };
                    let offset = self.cf_browse.search.offset;
                    let cf_sort = curseforge::CfSortField::ALL
                        .get(self.cf_browse.selected_sort)
                        .copied()
                        .unwrap_or_default();
                    let cf_category_id = if self.cf_browse.selected_category == 0 {
                        None
                    } else {
                        self.cf_categories.as_ref().and_then(|cats| {
                            cats.get(self.cf_browse.selected_category - 1)
                                .map(|c| c.id)
                        })
                    };
                    self.cf_browse.search.fire_with_repaint(ui.ctx(), move || {
                        let resp = curseforge::search_cf_mods(
                            &query,
                            &mc_version,
                            ltype,
                            curseforge::CLASS_MODS,
                            offset,
                            cf_sort,
                            cf_category_id,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(BrowseSearchResult {
                            items: resp
                                .data
                                .into_iter()
                                .map(|hit| BrowseItem {
                                    title: hit.name.clone(),
                                    description: hit.summary.clone(),
                                    icon_url: hit.logo.as_ref().map(|l| l.best_url().to_string()),
                                    downloads: hit.download_count,
                                    categories: hit
                                        .categories
                                        .iter()
                                        .map(|c| c.name.clone())
                                        .collect(),
                                    id: hit.id.to_string(),
                                    slug: hit.slug.clone(),
                                    allows_install: hit
                                        .allow_mod_distribution
                                        .unwrap_or(true),
                                })
                                .collect(),
                            total: resp.pagination.total_count,
                        })
                    });
                }
                BrowseAction::Install(idx) => {
                    if let Some(item) = self.cf_browse.results.get(idx) {
                        let mod_id: u64 = item.id.parse().unwrap_or(0);
                        let title = item.title.clone();
                        let mc_ver = instance.mc_version.clone();
                        let mods_dir = mods_dir.to_path_buf();
                        self.open_cf_mod_version_picker(
                            mod_id, title, &mc_ver, loader_type, &mods_dir, ui.ctx(),
                        );
                    }
                }
                BrowseAction::OpenPage(idx) => {
                    if let Some(item) = self.cf_browse.results.get(idx) {
                        let mod_id: u64 = item.id.parse().unwrap_or(0);
                        let url = curseforge::curseforge_mod_url(mod_id, &item.slug);
                        let _ = open::that(&url);
                    }
                }
                BrowseAction::SearchError(e) => {
                    self.pending_toasts
                        .push(crate::ui::notifications::Toast::error(format!("Search failed: {e}")));
                }
                BrowseAction::VersionFilterChanged(_) => {
                    // handled implicitly — FireSearch will also be emitted
                }
            }
        }

        // ── Toast status polling (install feedback) ──────────────
        let maybe_status: Option<Arc<Mutex<Option<String>>>> = ui
            .ctx()
            .data(|d| d.get_temp(egui::Id::new("cf_install_status")));
        if let Some(arc) = maybe_status
            && let Some(msg) = arc.lock().ok().and_then(|mut g| g.take())
        {
            if msg.starts_with("Error")
                || msg.starts_with("Install failed")
                || msg.starts_with("Search failed")
            {
                self.pending_toasts.push(crate::ui::notifications::Toast::error(msg));
            } else {
                self.pending_toasts.push(crate::ui::notifications::Toast::success(msg));
            }
            self.needs_rescan = true;
            ui.ctx().data_mut(|d| {
                d.remove::<Arc<Mutex<Option<String>>>>(egui::Id::new("cf_install_status"))
            });
        }
    }
}
