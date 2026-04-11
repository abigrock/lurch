mod create_dialog;
mod detail;
mod edit_dialog;
pub mod modpack_browser;

use crate::app::ManifestState;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum AddInstanceTab {
    #[default]
    Vanilla,
    Modrinth,
    CurseForge,
    Import,
}
use crate::core::instance::{Instance, ModLoader};
use crate::core::java::JavaInstall;
use crate::ui::helpers::ViewMode;
use eframe::egui;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InstanceSortMode {
    #[default]
    LastPlayed,
    NameAsc,
    CreatedDesc,
    McVersion,
}

impl InstanceSortMode {
    fn label(&self) -> &'static str {
        match self {
            Self::LastPlayed => "Last Played",
            Self::NameAsc => "Name (A-Z)",
            Self::CreatedDesc => "Newest First",
            Self::McVersion => "MC Version",
        }
    }
}

pub struct InstancesView {
    pub theme: Option<crate::theme::Theme>,
    pub view_mode: ViewMode,
    pub show_add_instance: bool,
    pub(crate) add_instance_tab: AddInstanceTab,
    pub new_name: String,
    pub new_mc_version: String,
    pub new_loader: ModLoader,
    pub new_group: String,
    pub renaming: Option<String>,
    pub rename_text: String,
    pub editing: Option<String>,
    pub confirm_delete: Option<String>,
    pub show_snapshots: bool,
    pub launch_requested: Option<String>,
    pub detail_view: Option<detail::InstanceDetailView>,
    pub export_requested: Option<usize>,
    pub pending_toasts: Vec<crate::app::Toast>,
    pub modpack_browser: modpack_browser::ModpackBrowser,
    pub local_mrpack_import: Option<std::path::PathBuf>,
    pub local_cf_modpack_import: Option<std::path::PathBuf>,
    pub loader_versions: Vec<(String, bool)>,
    pub loader_versions_loading: bool,
    pub loader_versions_error: Option<String>,
    #[allow(clippy::type_complexity)]
    pub loader_versions_fetch: Option<Arc<Mutex<Option<Result<Vec<(String, bool)>, String>>>>>,
    pub new_loader_version: String,
    pub mod_counts: HashMap<String, usize>,
    pub mod_counts_dirty: bool,
    pub modpack_updates: HashMap<String, crate::core::update::ModpackUpdateInfo>,
    pub update_modpack_requested: Option<String>,
    pub recheck_modpack_updates: bool,
    pub running_instance_ids: HashSet<String>,
    pub console_requested: Option<String>,
    pub kill_requested: Option<String>,
    name_auto_generated: bool,
    pub search_query: String,
    pub sort_mode: InstanceSortMode,
    pub loader_filter: Option<ModLoader>,
    // Edit dialog state for MC version / loader switching
    pub edit_mc_version: String,
    pub edit_loader: ModLoader,
    pub edit_loader_version: String,
    pub edit_show_snapshots: bool,
    pub edit_loader_versions: Vec<(String, bool)>,
    pub edit_loader_versions_loading: bool,
    pub edit_loader_versions_error: Option<String>,
    #[allow(clippy::type_complexity)]
    pub edit_loader_versions_fetch: Option<Arc<Mutex<Option<Result<Vec<(String, bool)>, String>>>>>,
    edit_initialized_for: Option<String>,
    modpack_version_picker: Option<ModpackVersionPickerState>,
    pub change_modpack_version: Option<(String, crate::core::update::ModpackUpdateInfo)>,
}

struct ModpackVersionPickerState {
    instance_id: String,
    instance_name: String,
    source: String,
    project_id: String,
    current_version_id: String,
    mr_versions: Vec<crate::core::modrinth::ProjectVersion>,
    cf_files: Vec<crate::core::curseforge::CfFile>,
    selected_index: usize,
    preselect_latest: bool,
    fetch_handle: Option<Arc<Mutex<Option<modpack_browser::VersionFetchResult>>>>,
}

enum ModpackVersionPickerAction {
    Apply,
    Cancel,
}

impl Default for InstancesView {
    fn default() -> Self {
        Self {
            theme: None,
            view_mode: ViewMode::default(),
            show_add_instance: false,
            add_instance_tab: AddInstanceTab::default(),
            new_name: String::new(),
            new_mc_version: String::new(),
            new_loader: ModLoader::default(),
            new_group: String::new(),
            renaming: None,
            rename_text: String::new(),
            editing: None,
            confirm_delete: None,
            show_snapshots: false,
            launch_requested: None,
            detail_view: None,
            export_requested: None,
            pending_toasts: Vec::new(),
            modpack_browser: modpack_browser::ModpackBrowser::default(),
            local_mrpack_import: None,
            local_cf_modpack_import: None,
            loader_versions: Vec::new(),
            loader_versions_loading: false,
            loader_versions_error: None,
            loader_versions_fetch: None,
            new_loader_version: String::new(),
            mod_counts: HashMap::new(),
            mod_counts_dirty: true,
            modpack_updates: HashMap::new(),
            update_modpack_requested: None,
            recheck_modpack_updates: false,
            running_instance_ids: HashSet::new(),
            console_requested: None,
            kill_requested: None,
            name_auto_generated: false,
            search_query: String::new(),
            sort_mode: InstanceSortMode::default(),
            loader_filter: None,
            edit_mc_version: String::new(),
            edit_loader: ModLoader::default(),
            edit_loader_version: String::new(),
            edit_show_snapshots: false,
            edit_loader_versions: Vec::new(),
            edit_loader_versions_loading: false,
            edit_loader_versions_error: None,
            edit_loader_versions_fetch: None,
            edit_initialized_for: None,
            modpack_version_picker: None,
            change_modpack_version: None,
        }
    }
}

impl InstancesView {
    pub fn has_detail_view(&self) -> bool {
        self.detail_view.is_some()
    }

    pub fn close_detail_view(&mut self) {
        if let Some(ref mut detail) = self.detail_view {
            detail.back_requested = true;
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        instances: &mut Vec<Instance>,
        manifest: &Arc<Mutex<ManifestState>>,
        java_installs: &[JavaInstall],
        config: &crate::core::config::AppConfig,
    ) {
        // Overlay dialogs (egui::Window) — always rendered regardless of main content
        {
            let ctx = ui.ctx().clone();
            if let Some(edit_id) = self.editing.clone() {
                self.edit_dialog(&ctx, instances, &edit_id, java_installs, manifest);
            }
            if let Some(del_id) = self.confirm_delete.clone() {
                self.delete_confirm_dialog(&ctx, instances, &del_id);
            }
            self.poll_modpack_version_picker();
            self.show_modpack_version_picker(ui);
            if self.modpack_version_picker.as_ref().is_some_and(|vp| vp.fetch_handle.is_some()) {
                ui.ctx().request_repaint();
            }
        }

        // If a detail view is open, render it instead of the instances grid
        if let Some(ref mut detail) = self.detail_view {
            let inst = instances.iter().find(|i| i.id == detail.instance_id());
            if let Some(inst) = inst {
                detail.show(ui, inst, self.theme.as_ref());
                self.pending_toasts.append(&mut detail.pending_toasts);
                if detail.back_requested {
                    self.detail_view = None;
                }
                return;
            } else {
                // Instance was deleted externally — close detail
                self.detail_view = None;
            }
        }

        // Add Instance view — takes over the content area
        if self.show_add_instance {
            self.show_add_instance_view(ui, instances, manifest, config);
            return;
        }

        // Populate mod counts (cached, only when dirty)
        if self.mod_counts_dirty {
            self.mod_counts.clear();
            for inst in instances.iter() {
                if let Ok(mc_dir) = inst.minecraft_dir() {
                    let mods_dir = mc_dir.join("mods");
                    let count = crate::core::local_mods::scan_installed_mods(&mods_dir, &[]).len();
                    self.mod_counts.insert(inst.id.clone(), count);
                }
            }
            self.mod_counts_dirty = false;
        }

        // Header row: title, search, filters, sort, view toggle, actions
        // section_header is 15pt bold — needs more vertical room than interact_size.y + 4
        let row_h = ui.spacing().interact_size.y + 12.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
                if let Some(t) = self.theme.as_ref() {
                    ui.label(t.section_header("Instances"));
                } else {
                    ui.heading("Instances");
                }

                ui.separator();

                // Search
                let search_icon = egui_phosphor::regular::MAGNIFYING_GLASS;
                ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text(format!("{} Search…", search_icon))
                        .desired_width(160.0)
                        .margin(egui::Margin::symmetric(4, 9)),
                );

                // Loader filter
                let loader_text = match &self.loader_filter {
                    Some(l) => format!("{:?}", l),
                    None => "All Loaders".to_string(),
                };
                egui::ComboBox::from_id_salt("instance_loader_filter")
                    .selected_text(&loader_text)
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.loader_filter, None, "All Loaders");
                        ui.selectable_value(&mut self.loader_filter, Some(ModLoader::Vanilla), "Vanilla");
                        ui.selectable_value(&mut self.loader_filter, Some(ModLoader::Fabric), "Fabric");
                        ui.selectable_value(&mut self.loader_filter, Some(ModLoader::Forge), "Forge");
                        ui.selectable_value(&mut self.loader_filter, Some(ModLoader::NeoForge), "NeoForge");
                        ui.selectable_value(&mut self.loader_filter, Some(ModLoader::Quilt), "Quilt");
                    });

                // Sort dropdown
                egui::ComboBox::from_id_salt("instance_sort")
                    .selected_text(self.sort_mode.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.sort_mode, InstanceSortMode::LastPlayed, "Last Played");
                        ui.selectable_value(&mut self.sort_mode, InstanceSortMode::NameAsc, "Name (A-Z)");
                        ui.selectable_value(&mut self.sort_mode, InstanceSortMode::CreatedDesc, "Newest First");
                        ui.selectable_value(&mut self.sort_mode, InstanceSortMode::McVersion, "MC Version");
                    });

                // Right-aligned: view toggle, refresh, Add Instance
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center).with_cross_justify(true),
                    |ui| {
                        // Add Instance (rightmost)
                        let add_lbl = format!("{} Add Instance", egui_phosphor::regular::PLUS);
                        let add_clicked = if let Some(t) = self.theme.as_ref() {
                            ui.add(t.accent_button(&add_lbl)).clicked()
                        } else {
                            ui.button(&add_lbl).clicked()
                        };
                        if add_clicked {
                            self.show_add_instance = true;
                            self.add_instance_tab = AddInstanceTab::Vanilla;
                            self.new_name.clear();
                            self.new_mc_version.clear();
                            self.new_loader = ModLoader::default();
                            self.new_group.clear();
                            self.loader_versions.clear();
                            self.loader_versions_loading = false;
                            self.loader_versions_error = None;
                            self.loader_versions_fetch = None;
                            self.new_loader_version.clear();
                            self.name_auto_generated = false;
                        }

                        // Refresh
                        let refresh_lbl = egui_phosphor::regular::ARROWS_CLOCKWISE.to_string();
                        let refresh_clicked = if let Some(t) = self.theme.as_ref() {
                            ui.add(t.ghost_button(&refresh_lbl))
                                .on_hover_text("Check for modpack updates")
                                .clicked()
                        } else {
                            ui.add(egui::Button::new(&refresh_lbl).frame(false))
                                .on_hover_text("Check for modpack updates")
                                .clicked()
                        };
                        if refresh_clicked {
                            self.recheck_modpack_updates = true;
                        }

                        ui.separator();

                        // View toggle (right-to-left: render grid first so list appears left)
                        ui.selectable_value(
                            &mut self.view_mode,
                            ViewMode::Grid,
                            egui_phosphor::regular::GRID_FOUR,
                        );
                        ui.selectable_value(
                            &mut self.view_mode,
                            ViewMode::List,
                            egui_phosphor::regular::LIST,
                        );
                    },
                );
            },
        );

        ui.separator();
        ui.add_space(8.0);

        if instances.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                if let Some(t) = self.theme.as_ref() {
                    ui.label(
                        egui::RichText::new(egui_phosphor::regular::GAME_CONTROLLER)
                            .size(48.0)
                            .color(t.color("fg_muted")),
                    );
                    ui.add_space(12.0);
                    ui.label(t.section_header("No instances yet"));
                    ui.add_space(4.0);
                    ui.label(t.subtext("Create a new instance or browse modpacks to get started."));
                    ui.add_space(16.0);
                    let new_inst_lbl = format!("{} Add Instance", egui_phosphor::regular::PLUS);
                    if ui.add(t.accent_button(&new_inst_lbl)).clicked() {
                        self.show_add_instance = true;
                        self.add_instance_tab = AddInstanceTab::Vanilla;
                        self.new_name.clear();
                        self.new_mc_version.clear();
                        self.new_loader = ModLoader::default();
                        self.new_group.clear();
                        self.loader_versions.clear();
                        self.loader_versions_loading = false;
                        self.loader_versions_error = None;
                        self.loader_versions_fetch = None;
                        self.new_loader_version.clear();
                        self.name_auto_generated = false;
                    }
                } else {
                    ui.label(
                        egui::RichText::new(egui_phosphor::regular::GAME_CONTROLLER).size(48.0),
                    );
                    ui.add_space(12.0);
                    ui.heading("No instances yet");
                    ui.add_space(4.0);
                    ui.label("Create a new instance or browse modpacks to get started.");
                    ui.add_space(16.0);
                    if ui
                        .button(format!("{} Add Instance", egui_phosphor::regular::PLUS))
                        .clicked()
                    {
                        self.show_add_instance = true;
                        self.add_instance_tab = AddInstanceTab::Vanilla;
                        self.new_name.clear();
                        self.new_mc_version.clear();
                        self.new_loader = ModLoader::default();
                        self.new_group.clear();
                        self.loader_versions.clear();
                        self.loader_versions_loading = false;
                        self.loader_versions_error = None;
                        self.loader_versions_fetch = None;
                        self.new_loader_version.clear();
                        self.name_auto_generated = false;
                    }
                }
            });
        } else {
            // Collect pending mutations to apply after iteration
            let mut to_delete: Option<String> = None;
            let mut to_duplicate: Option<usize> = None;
            let mut to_rename_confirm: Option<(String, String)> = None;
            let mut rename_cancelled = false;

            // Filter instances
            let search_lower = self.search_query.trim().to_lowercase();
            let filtered_indices: Vec<usize> = instances
                .iter()
                .enumerate()
                .filter(|(_, inst)| {
                    // Name search filter
                    if !search_lower.is_empty()
                        && !inst.name.to_lowercase().contains(&search_lower)
                    {
                        return false;
                    }
                    // Loader filter
                    if let Some(ref lf) = self.loader_filter {
                        if &inst.loader != lf {
                            return false;
                        }
                    }
                    true
                })
                .map(|(idx, _)| idx)
                .collect();

            // Sort filtered indices
            let mut sorted_indices = filtered_indices;
            match self.sort_mode {
                InstanceSortMode::LastPlayed => {
                    sorted_indices.sort_by(|&a, &b| {
                        let la = instances[a].last_played.as_deref().unwrap_or("");
                        let lb = instances[b].last_played.as_deref().unwrap_or("");
                        lb.cmp(la)
                    });
                }
                InstanceSortMode::NameAsc => {
                    sorted_indices.sort_by(|&a, &b| {
                        instances[a]
                            .name
                            .to_lowercase()
                            .cmp(&instances[b].name.to_lowercase())
                    });
                }
                InstanceSortMode::CreatedDesc => {
                    sorted_indices.sort_by(|&a, &b| {
                        let ca = instances[a].created.unwrap_or(0);
                        let cb = instances[b].created.unwrap_or(0);
                        cb.cmp(&ca)
                    });
                }
                InstanceSortMode::McVersion => {
                    sorted_indices.sort_by(|&a, &b| {
                        let va = &instances[a].mc_version;
                        let vb = &instances[b].mc_version;
                        // Parse version segments for proper numeric comparison
                        let parse_ver = |v: &str| -> Vec<u32> {
                            v.split('.').filter_map(|s| s.parse().ok()).collect()
                        };
                        parse_ver(vb).cmp(&parse_ver(va))
                    });
                }
            }

            // Group sorted+filtered indices
            let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            let mut ungrouped: Vec<usize> = Vec::new();
            for idx in &sorted_indices {
                match &instances[*idx].group {
                    Some(g) if !g.trim().is_empty() => {
                        groups.entry(g.clone()).or_default().push(*idx);
                    }
                    _ => ungrouped.push(*idx),
                }
            }

            let is_searching = !self.search_query.trim().is_empty();

            // Show result count when filtering
            if is_searching || self.loader_filter.is_some() {
                if let Some(t) = self.theme.as_ref() {
                    ui.label(t.subtext(&format!(
                        "Showing {} of {} instances",
                        sorted_indices.len(),
                        instances.len()
                    )));
                } else {
                    ui.label(format!(
                        "Showing {} of {} instances",
                        sorted_indices.len(),
                        instances.len()
                    ));
                }
                ui.add_space(4.0);
            }

            egui::ScrollArea::vertical().id_salt("instance_list").auto_shrink([false, false]).show(ui, |ui| {
                if self.view_mode == ViewMode::Grid {
                    let gap = ui.spacing().item_spacing.x;
                    ui.spacing_mut().item_spacing.y = gap;
                }
                let group_keys: Vec<String> = groups.keys().cloned().collect();
                for group_name in group_keys {
                    let indices = groups[&group_name].clone();
                    let header_text = if let Some(t) = self.theme.as_ref() {
                        t.subtext(&group_name)
                    } else {
                        egui::RichText::new(&group_name).strong()
                    };
                    egui::CollapsingHeader::new(header_text)
                        .default_open(true)
                        .open(if is_searching { Some(true) } else { None })
                        .id_salt(format!("group_{group_name}"))
                        .show(ui, |ui| match self.view_mode {
                            ViewMode::List => {
                                self.show_list(
                                    ui,
                                    instances,
                                    &indices,
                                    &mut to_delete,
                                    &mut to_duplicate,
                                    &mut to_rename_confirm,
                                    &mut rename_cancelled,
                                );
                            }
                            ViewMode::Grid => {
                                self.show_grid(
                                    ui,
                                    instances,
                                    &indices,
                                    &mut to_delete,
                                    &mut to_duplicate,
                                    &mut to_rename_confirm,
                                    &mut rename_cancelled,
                                );
                            }
                        });
                    ui.add_space(4.0);
                }

                if !ungrouped.is_empty() {
                    let ungrouped_header = if let Some(t) = self.theme.as_ref() {
                        t.subtext("Ungrouped")
                    } else {
                        egui::RichText::new("Ungrouped").weak()
                    };
                    egui::CollapsingHeader::new(ungrouped_header)
                        .default_open(true)
                        .open(if is_searching { Some(true) } else { None })
                        .id_salt("group_ungrouped")
                        .show(ui, |ui| match self.view_mode {
                            ViewMode::List => {
                                self.show_list(
                                    ui,
                                    instances,
                                    &ungrouped,
                                    &mut to_delete,
                                    &mut to_duplicate,
                                    &mut to_rename_confirm,
                                    &mut rename_cancelled,
                                );
                            }
                            ViewMode::Grid => {
                                self.show_grid(
                                    ui,
                                    instances,
                                    &ungrouped,
                                    &mut to_delete,
                                    &mut to_duplicate,
                                    &mut to_rename_confirm,
                                    &mut rename_cancelled,
                                );
                            }
                        });
                }
            });

            // Apply mutations
            if rename_cancelled {
                self.renaming = None;
                self.rename_text.clear();
            }
            if let Some((id, new_name)) = to_rename_confirm {
                if let Some(inst) = instances.iter_mut().find(|i| i.id == id) {
                    inst.name = new_name;
                    let _ = inst.save_to_dir();
                }
                self.renaming = None;
                self.rename_text.clear();
            }
            if let Some(idx) = to_duplicate {
                let new_inst = instances[idx].duplicate();
                let _ = new_inst.create_dirs();
                instances.push(new_inst);
            }
            if let Some(ref id) = to_delete.clone() {
                self.confirm_delete = Some(id.clone());
            }

            // Handle export
            if let Some(idx) = self.export_requested.take()
                && let Some(inst) = instances.get(idx)
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("Export Instance")
                        .set_file_name(format!("{}.zip", inst.name))
                        .add_filter("Zip Archive", &["zip"])
                        .save_file()
                    {
                        match crate::core::import_export::export_instance(inst, &path) {
                            Ok(()) => {
                                self.pending_toasts.push(crate::app::Toast::success(format!("Exported \"{}\"", inst.name)));
                            }
                            Err(e) => {
                                self.pending_toasts.push(crate::app::Toast::error(format!("Export failed: {e}")));
                            }
                        }
                    }
        }

    }

    // ── List view ────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn show_list(
        &mut self,
        ui: &mut egui::Ui,
        instances: &mut [Instance],
        indices: &[usize],
        to_delete: &mut Option<String>,
        to_duplicate: &mut Option<usize>,
        to_rename_confirm: &mut Option<(String, String)>,
        rename_cancelled: &mut bool,
    ) {
        let outer_frame = crate::ui::helpers::card_frame(ui, self.theme.as_ref());
        outer_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width() - 8.0);
            for (list_idx, &idx) in indices.iter().enumerate() {
                if list_idx > 0 {
                    ui.separator();
                }
                let inst = &instances[idx];
                let inst_id = inst.id.clone();
                let is_running = self.running_instance_ids.contains(&inst_id);
                let is_renaming = self.renaming.as_deref() == Some(&inst_id);

                ui.horizontal(|ui| {
                    let icon_resp = if let Some(url) = &inst.icon {
                        ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(48.0, 48.0)))
                    } else {
                        crate::ui::helpers::icon_placeholder(
                            ui,
                            &inst.name,
                            48.0,
                            self.theme.as_ref(),
                        )
                    };
                    if is_running {
                        let color = self
                            .theme
                            .as_ref()
                            .map(|t| t.color("success"))
                            .unwrap_or(egui::Color32::from_rgb(76, 175, 80));
                        let dot_rect = egui::Rect::from_min_size(
                            icon_resp.rect.right_bottom() - egui::vec2(14.0, 14.0),
                            egui::vec2(10.0, 10.0),
                        );
                        ui.painter().circle_filled(dot_rect.center(), 5.0, color);
                    }
                    ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width() - 180.0);
                        if is_renaming {
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.rename_text)
                                    .margin(egui::Margin::symmetric(4, 9)),
                            );
                            if response.lost_focus() {
                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    *rename_cancelled = true;
                                } else {
                                    *to_rename_confirm =
                                        Some((inst_id.clone(), self.rename_text.clone()));
                                }
                            } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                *to_rename_confirm =
                                    Some((inst_id.clone(), self.rename_text.clone()));
                            }
                            response.request_focus();
                        } else if let Some(t) = self.theme.as_ref() {
                            ui.label(t.title(&inst.name));
                        } else {
                            ui.strong(&inst.name);
                        }
                        let mut info_parts: Vec<String> = vec![inst.mc_version.clone()];
                        if inst.loader != ModLoader::Vanilla {
                            info_parts.push(inst.loader.to_string());
                        }
                        let mod_count =
                            self.mod_counts.get(&inst.id).copied().unwrap_or(0);
                        if mod_count > 0 {
                            info_parts.push(format!(
                                "{} {mod_count}",
                                egui_phosphor::regular::PUZZLE_PIECE,
                            ));
                        }
                        let tag_refs: Vec<&str> =
                            info_parts.iter().map(|s| s.as_str()).collect();
                        crate::ui::helpers::show_category_tags(
                            ui,
                            &tag_refs,
                            10,
                            self.theme.as_ref(),
                        );
                        let lp_text = inst.last_played.as_deref().unwrap_or("Never played");
                        if let Some(t) = self.theme.as_ref() {
                            ui.label(t.subtext(&format!("{} {lp_text}", egui_phosphor::regular::CLOCK)).size(11.0));
                        } else {
                            ui.label(egui::RichText::new(format!("{} {lp_text}", egui_phosphor::regular::CLOCK)).size(11.0).weak());
                        }
                        if self.modpack_updates.contains_key(&inst.id) {
                            let update_fill = if let Some(t) = self.theme.as_ref() {
                                t.color("accent")
                            } else {
                                egui::Color32::from_rgb(76, 175, 80)
                            };
                            let badge_resp = if let Some(t) = self.theme.as_ref() {
                                t.badge_frame(update_fill).show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} Update available",
                                            egui_phosphor::regular::ARROW_CIRCLE_UP,
                                        ))
                                        .size(11.0)
                                        .color(t.button_fg()),
                                    )
                                })
                            } else {
                                egui::Frame::new()
                                    .fill(update_fill)
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::symmetric(6, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} Update available",
                                                egui_phosphor::regular::ARROW_CIRCLE_UP,
                                            ))
                                            .size(11.0)
                                            .color(egui::Color32::WHITE),
                                        )
                                    })
                            };
                            if badge_resp.response.interact(egui::Sense::click()).clicked() {
                                self.update_modpack_requested = Some(inst_id.clone());
                            }
                        }
                    });

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let more_btn = if let Some(t) = self.theme.as_ref() {
                                ui.add(t.ghost_button(egui_phosphor::regular::DOTS_THREE).min_size(egui::vec2(32.0, 32.0)))
                            } else {
                                ui.add(egui::Button::new(egui_phosphor::regular::DOTS_THREE).min_size(egui::vec2(32.0, 32.0)))
                            };
                            let more_btn = more_btn.on_hover_text("More actions");
                            egui::Popup::menu(&more_btn).show(|ui| {
                                ui.set_min_width(140.0);
                                if let Some(t) = self.theme.as_ref() {
                                    t.style_menu(ui);
                                }
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add(t.menu_item(&format!("{} Manage", egui_phosphor::regular::WRENCH)))
                                } else {
                                    ui.add(egui::Button::new(format!("{} Manage", egui_phosphor::regular::WRENCH)).frame(false))
                                }.clicked()
                                {
                                    self.detail_view =
                                        Some(detail::InstanceDetailView::new(inst_id.clone()));
                                }
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add_enabled(!is_running, t.menu_item(&format!("{} Rename", egui_phosphor::regular::PENCIL)))
                                } else {
                                    ui.add_enabled(!is_running, egui::Button::new(format!("{} Rename", egui_phosphor::regular::PENCIL)).frame(false))
                                }.clicked()
                                {
                                    self.renaming = Some(inst_id.clone());
                                    self.rename_text = instances[idx].name.clone();
                                }
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add_enabled(!is_running, t.menu_item(&format!("{} Configure", egui_phosphor::regular::GEAR_SIX)))
                                } else {
                                    ui.add_enabled(!is_running, egui::Button::new(format!("{} Configure", egui_phosphor::regular::GEAR_SIX)).frame(false))
                                }.clicked()
                                {
                                    self.editing = Some(inst_id.clone());
                                }
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add(t.menu_item(&format!("{} Open Folder", egui_phosphor::regular::FOLDER)))
                                } else {
                                    ui.add(egui::Button::new(format!("{} Open Folder", egui_phosphor::regular::FOLDER)).frame(false))
                                }.clicked()
                                    && let Ok(dir) = instances[idx].instance_dir()
                                {
                                    let _ = open::that(dir);
                                }
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add_enabled(!is_running, t.menu_item(&format!("{} Duplicate", egui_phosphor::regular::CLIPBOARD)))
                                } else {
                                    ui.add_enabled(!is_running, egui::Button::new(format!("{} Duplicate", egui_phosphor::regular::CLIPBOARD)).frame(false))
                                }.clicked()
                                {
                                    *to_duplicate = Some(idx);
                                }
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add_enabled(!is_running, t.menu_item(&format!("{} Export", egui_phosphor::regular::EXPORT)))
                                } else {
                                    ui.add_enabled(!is_running, egui::Button::new(format!("{} Export", egui_phosphor::regular::EXPORT)).frame(false))
                                }.clicked()
                                {
                                    self.export_requested = Some(idx);
                                }
                                if let Some(origin) = &inst.modpack_origin {
                                    let source_name = match origin.source.as_str() {
                                        "modrinth" => "Modrinth",
                                        "curseforge" => "CurseForge",
                                        _ => "source",
                                    };
                                    if if let Some(t) = self.theme.as_ref() {
                                        ui.add(t.menu_item(&format!("{} Open on {source_name}", egui_phosphor::regular::GLOBE)))
                                    } else {
                                        ui.add(egui::Button::new(format!("{} Open on {source_name}", egui_phosphor::regular::GLOBE)).frame(false))
                                    }.clicked()
                                    {
                                        if let Some(url) = crate::core::local_mods::modpack_project_url(&origin.source, &origin.project_id) {
                                            let _ = open::that(&url);
                                        }
                                    }
                                    if if let Some(t) = self.theme.as_ref() {
                                        ui.add_enabled(!is_running, t.menu_item(&format!("{} Change Version", egui_phosphor::regular::ARROWS_DOWN_UP)))
                                    } else {
                                        ui.add_enabled(!is_running, egui::Button::new(format!("{} Change Version", egui_phosphor::regular::ARROWS_DOWN_UP)).frame(false))
                                    }.clicked() {
                                        self.open_modpack_version_picker(
                                            &inst.id,
                                            &inst.name,
                                            &origin.source,
                                            &origin.project_id,
                                            &origin.version_id,
                                            false,
                                            ui.ctx(),
                                        );
                                    }
                                }
                                if is_running {
                                    ui.separator();
                                    let kill_lbl = format!("{} Kill", egui_phosphor::regular::SKULL);
                                    let kill_clicked = if let Some(t) = self.theme.as_ref() {
                                        ui.add(t.danger_button(&kill_lbl)).clicked()
                                    } else {
                                        ui.button(egui::RichText::new(&kill_lbl).color(egui::Color32::RED)).clicked()
                                    };
                                    if kill_clicked {
                                        self.kill_requested = Some(inst_id.clone());
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                let del_lbl =
                                    format!("{} Delete", egui_phosphor::regular::TRASH);
                                if is_running {
                                    ui.add_enabled(false, egui::Button::new(&del_lbl));
                                } else {
                                    let delete_clicked =
                                        if let Some(t) = self.theme.as_ref() {
                                            ui.add(t.danger_button(&del_lbl)).clicked()
                                        } else {
                                            ui.button(&del_lbl).clicked()
                                        };
                                    if delete_clicked {
                                        *to_delete = Some(inst_id.clone());
                                    }
                                }
                            });

                            if !is_running {
                                let launch_clicked = if let Some(t) = self.theme.as_ref() {
                                    ui.add(t.accent_button("▶"))
                                        .on_hover_text("Launch")
                                        .clicked()
                                } else {
                                    ui.button("▶").on_hover_text("Launch").clicked()
                                };
                                if launch_clicked {
                                    self.launch_requested = Some(inst_id.clone());
                                }
                            } else {
                                let console_clicked = if let Some(t) = self.theme.as_ref() {
                                    ui.add(t.accent_button(&format!("{} Console", egui_phosphor::regular::TERMINAL_WINDOW)))
                                        .on_hover_text("Open console")
                                        .clicked()
                                } else {
                                    ui.button(&format!("{} Console", egui_phosphor::regular::TERMINAL_WINDOW))
                                        .on_hover_text("Open console")
                                        .clicked()
                                };
                                if console_clicked {
                                    self.console_requested = Some(inst_id.clone());
                                }
                            }
                        },
                    );
                });
            }
        });
    }

    // ── Grid view ────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn show_grid(
        &mut self,
        ui: &mut egui::Ui,
        instances: &mut [Instance],
        indices: &[usize],
        to_delete: &mut Option<String>,
        to_duplicate: &mut Option<usize>,
        to_rename_confirm: &mut Option<(String, String)>,
        rename_cancelled: &mut bool,
    ) {
        let card_w = 220.0_f32;
        let card_h = 190.0_f32;
        let gap = ui.spacing().item_spacing.x;
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        let available = ui.available_width();
        let cols = ((available + gap) / (card_w + gap)).floor().max(1.0) as usize;

        for row_chunk in indices.chunks(cols) {
            let (row_rect, _) = ui.allocate_exact_size(
                egui::vec2(available, card_h),
                egui::Sense::hover(),
            );

            let mut x = row_rect.min.x;
            for &idx in row_chunk {
                let inst = &instances[idx];
                let inst_id = inst.id.clone();
                let is_running = self.running_instance_ids.contains(&inst_id);
                let is_renaming = self.renaming.as_deref() == Some(&inst_id);

                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(x, row_rect.min.y),
                    egui::vec2(card_w, card_h),
                );

                // RefCell splits the borrow so both closures can access rename_text
                let rename_cell = std::cell::RefCell::new(
                    std::mem::take(&mut self.rename_text),
                );
                // Collected request to open version picker (can't call &mut self inside closure)
                let mut open_version_picker_req: Option<(String, String, String, String, String)> = None;

                crate::ui::helpers::grid_card(
                    ui,
                    cell_rect,
                    self.theme.as_ref(),
                    |ui| {
                        let mod_count =
                            self.mod_counts.get(&inst.id).copied().unwrap_or(0);

                            ui.horizontal(|ui| {
                            let icon_resp = if let Some(url) = &inst.icon {
                                ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(36.0, 36.0)))
                            } else {
                                crate::ui::helpers::icon_placeholder(
                                    ui,
                                    &inst.name,
                                    36.0,
                                    self.theme.as_ref(),
                                )
                            };
                            if is_running {
                                let color = self
                                    .theme
                                    .as_ref()
                                    .map(|t| t.color("success"))
                                    .unwrap_or(egui::Color32::from_rgb(76, 175, 80));
                                let dot_rect = egui::Rect::from_min_size(
                                    icon_resp.rect.right_bottom() - egui::vec2(12.0, 12.0),
                                    egui::vec2(10.0, 10.0),
                                );
                                ui.painter().circle_filled(dot_rect.center(), 5.0, color);
                            }
                            if is_renaming {
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut *rename_cell.borrow_mut())
                                        .margin(egui::Margin::symmetric(4, 9)),
                                );
                                if response.lost_focus() {
                                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        *rename_cancelled = true;
                                    } else {
                                        *to_rename_confirm = Some((
                                            inst_id.clone(),
                                            rename_cell.borrow().clone(),
                                        ));
                                    }
                                } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    *to_rename_confirm = Some((
                                        inst_id.clone(),
                                        rename_cell.borrow().clone(),
                                    ));
                                }
                                response.request_focus();
                            } else if let Some(t) = self.theme.as_ref() {
                                ui.add(egui::Label::new(t.title(&inst.name)).truncate());
                            } else {
                                ui.add(egui::Label::new(egui::RichText::new(&inst.name).strong()).truncate());
                            }
                        });
                        ui.add_space(4.0);
                        let mut meta_parts: Vec<String> = vec![inst.mc_version.clone()];
                        if inst.loader != ModLoader::Vanilla {
                            meta_parts.push(inst.loader.to_string());
                        }
                        if mod_count > 0 {
                            meta_parts.push(format!("{mod_count} mods"));
                        }
                        let tag_refs: Vec<&str> = meta_parts.iter().map(|s| s.as_str()).collect();
                        crate::ui::helpers::show_category_tags(ui, &tag_refs, 10, self.theme.as_ref());
                        if self.modpack_updates.contains_key(&inst.id) {
                            let update_fill = if let Some(t) = self.theme.as_ref() {
                                t.color("accent")
                            } else {
                                egui::Color32::from_rgb(76, 175, 80)
                            };
                            let badge_resp = if let Some(t) = self.theme.as_ref() {
                                t.badge_frame(update_fill).show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} Update available",
                                            egui_phosphor::regular::ARROW_CIRCLE_UP,
                                        ))
                                        .size(11.0)
                                        .color(t.button_fg()),
                                    )
                                })
                            } else {
                                egui::Frame::new()
                                    .fill(update_fill)
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::symmetric(6, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} Update available",
                                                egui_phosphor::regular::ARROW_CIRCLE_UP,
                                            ))
                                            .size(11.0)
                                            .color(egui::Color32::WHITE),
                                        )
                                    })
                            };
                            if badge_resp.response.interact(egui::Sense::click()).clicked() {
                                self.update_modpack_requested = Some(inst_id.clone());
                            }
                        }
                        let lp_text = inst.last_played.as_deref().unwrap_or("Never played");
                        if let Some(t) = self.theme.as_ref() {
                            ui.label(t.subtext(&format!(
                                "{} {lp_text}",
                                egui_phosphor::regular::CLOCK,
                            )).size(11.0));
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} {lp_text}",
                                    egui_phosphor::regular::CLOCK,
                                )).size(11.0).weak(),
                            );
                        }
                    },
                    |ui| {
                        if !is_running {
                            let btn_width = ui.available_width() - 36.0 - ui.spacing().item_spacing.x;
                            let launch_clicked = if let Some(t) = self.theme.as_ref() {
                                ui.add(t.accent_button("▶ Launch").min_size(egui::vec2(btn_width, 0.0)))
                                    .clicked()
                            } else {
                                ui.add(egui::Button::new("▶ Launch").min_size(egui::vec2(btn_width, 0.0)))
                                    .clicked()
                            };
                            if launch_clicked {
                                self.launch_requested = Some(inst_id.clone());
                            }
                        } else {
                            let btn_width = ui.available_width() - 36.0 - ui.spacing().item_spacing.x;
                            let console_clicked = if let Some(t) = self.theme.as_ref() {
                                ui.add(t.accent_button(&format!("{} Console", egui_phosphor::regular::TERMINAL_WINDOW)).min_size(egui::vec2(btn_width, 0.0)))
                                    .clicked()
                            } else {
                                ui.add(egui::Button::new(format!("{} Console", egui_phosphor::regular::TERMINAL_WINDOW)).min_size(egui::vec2(btn_width, 0.0)))
                                    .clicked()
                            };
                            if console_clicked {
                                self.console_requested = Some(inst_id.clone());
                            }
                        }
                        let more_btn = if let Some(t) = self.theme.as_ref() {
                            ui.add(t.ghost_button(egui_phosphor::regular::DOTS_THREE).min_size(egui::vec2(32.0, 32.0)))
                        } else {
                            ui.add(egui::Button::new(egui_phosphor::regular::DOTS_THREE).min_size(egui::vec2(32.0, 32.0)))
                        };
                        let more_btn = more_btn.on_hover_text("More actions");
                        egui::Popup::menu(&more_btn).show(|ui| {
                            ui.set_min_width(140.0);
                            if let Some(t) = self.theme.as_ref() {
                                t.style_menu(ui);
                            }
                            if if let Some(t) = self.theme.as_ref() {
                                ui.add(t.menu_item(&format!("{} Manage", egui_phosphor::regular::WRENCH)))
                            } else {
                                ui.add(egui::Button::new(format!("{} Manage", egui_phosphor::regular::WRENCH)).frame(false))
                            }.clicked()
                            {
                                self.detail_view = Some(
                                    detail::InstanceDetailView::new(inst_id.clone()),
                                );
                            }
                            if if let Some(t) = self.theme.as_ref() {
                                ui.add_enabled(!is_running, t.menu_item(&format!("{} Rename", egui_phosphor::regular::PENCIL)))
                            } else {
                                ui.add_enabled(!is_running, egui::Button::new(format!("{} Rename", egui_phosphor::regular::PENCIL)).frame(false))
                            }.clicked()
                            {
                                self.renaming = Some(inst_id.clone());
                                *rename_cell.borrow_mut() = instances[idx].name.clone();
                            }
                            if if let Some(t) = self.theme.as_ref() {
                                ui.add_enabled(!is_running, t.menu_item(&format!("{} Configure", egui_phosphor::regular::GEAR_SIX)))
                            } else {
                                ui.add_enabled(!is_running, egui::Button::new(format!("{} Configure", egui_phosphor::regular::GEAR_SIX)).frame(false))
                            }.clicked()
                            {
                                self.editing = Some(inst_id.clone());
                            }
                            if if let Some(t) = self.theme.as_ref() {
                                ui.add(t.menu_item(&format!("{} Open Folder", egui_phosphor::regular::FOLDER)))
                            } else {
                                ui.add(egui::Button::new(format!("{} Open Folder", egui_phosphor::regular::FOLDER)).frame(false))
                            }.clicked()
                                && let Ok(dir) = instances[idx].instance_dir()
                            {
                                let _ = open::that(dir);
                            }
                            if if let Some(t) = self.theme.as_ref() {
                                ui.add_enabled(!is_running, t.menu_item(&format!("{} Duplicate", egui_phosphor::regular::CLIPBOARD)))
                            } else {
                                ui.add_enabled(!is_running, egui::Button::new(format!("{} Duplicate", egui_phosphor::regular::CLIPBOARD)).frame(false))
                            }.clicked()
                            {
                                *to_duplicate = Some(idx);
                            }
                            if if let Some(t) = self.theme.as_ref() {
                                ui.add_enabled(!is_running, t.menu_item(&format!("{} Export", egui_phosphor::regular::EXPORT)))
                            } else {
                                ui.add_enabled(!is_running, egui::Button::new(format!("{} Export", egui_phosphor::regular::EXPORT)).frame(false))
                            }.clicked()
                            {
                                self.export_requested = Some(idx);
                            }
                            if let Some(origin) = &inst.modpack_origin {
                                let source_name = match origin.source.as_str() {
                                    "modrinth" => "Modrinth",
                                    "curseforge" => "CurseForge",
                                    _ => "source",
                                };
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add(t.menu_item(&format!("{} Open on {source_name}", egui_phosphor::regular::GLOBE)))
                                } else {
                                    ui.add(egui::Button::new(format!("{} Open on {source_name}", egui_phosphor::regular::GLOBE)).frame(false))
                                }.clicked()
                                {
                                    if let Some(url) = crate::core::local_mods::modpack_project_url(&origin.source, &origin.project_id) {
                                        let _ = open::that(&url);
                                    }
                                }
                                if if let Some(t) = self.theme.as_ref() {
                                    ui.add_enabled(!is_running, t.menu_item(&format!("{} Change Version", egui_phosphor::regular::ARROWS_DOWN_UP)))
                                } else {
                                    ui.add_enabled(!is_running, egui::Button::new(format!("{} Change Version", egui_phosphor::regular::ARROWS_DOWN_UP)).frame(false))
                                }.clicked() {
                                    open_version_picker_req = Some((
                                        inst.id.clone(),
                                        inst.name.clone(),
                                        origin.source.clone(),
                                        origin.project_id.clone(),
                                        origin.version_id.clone(),
                                    ));
                                }
                            }
                            if is_running {
                                ui.separator();
                                let kill_lbl = format!("{} Kill", egui_phosphor::regular::SKULL);
                                let kill_clicked = if let Some(t) = self.theme.as_ref() {
                                    ui.add(t.danger_button(&kill_lbl)).clicked()
                                } else {
                                    ui.button(egui::RichText::new(&kill_lbl).color(egui::Color32::RED)).clicked()
                                };
                                if kill_clicked {
                                    self.kill_requested = Some(inst_id.clone());
                                    ui.close();
                                }
                            }
                            ui.separator();
                            let del_lbl = format!(
                                "{} Delete",
                                egui_phosphor::regular::TRASH
                            );
                            if is_running {
                                ui.add_enabled(false, egui::Button::new(&del_lbl));
                            } else {
                                let delete_clicked =
                                    if let Some(t) = self.theme.as_ref() {
                                        ui.add(t.danger_button(&del_lbl)).clicked()
                                    } else {
                                        ui.button(&del_lbl).clicked()
                                    };
                                if delete_clicked {
                                    *to_delete = Some(inst_id.clone());
                                }
                            }
                        });
                    },
                );

                self.rename_text = rename_cell.into_inner();

                if let Some((id, name, source, pid, vid)) = open_version_picker_req {
                    self.open_modpack_version_picker(&id, &name, &source, &pid, &vid, false, ui.ctx());
                }

                x += card_w + gap;
            }
        }
    }

    pub fn open_modpack_version_picker(
        &mut self,
        instance_id: &str,
        instance_name: &str,
        source: &str,
        project_id: &str,
        current_version_id: &str,
        preselect_latest: bool,
        ctx: &egui::Context,
    ) {
        let slot: Arc<Mutex<Option<modpack_browser::VersionFetchResult>>> =
            Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&slot);
        let ctx_clone = ctx.clone();

        let src = source.to_string();
        let pid = project_id.to_string();

        std::thread::spawn(move || {
            let result = match src.as_str() {
                "modrinth" => {
                    let r = crate::core::modrinth::get_project_versions(&pid, None, None)
                        .map_err(|e| e.to_string());
                    modpack_browser::VersionFetchResult::MrVersions(r)
                }
                "curseforge" => {
                    let mod_id: u64 = pid.parse().unwrap_or(0);
                    let r = crate::core::curseforge::get_cf_mod_files(mod_id, "", None)
                        .map_err(|e| e.to_string());
                    modpack_browser::VersionFetchResult::CfFiles(r)
                }
                _ => {
                    modpack_browser::VersionFetchResult::MrVersions(Err(
                        format!("Unknown source: {src}"),
                    ))
                }
            };
            *slot_clone.lock().unwrap() = Some(result);
            ctx_clone.request_repaint();
        });

        self.modpack_version_picker = Some(ModpackVersionPickerState {
            instance_id: instance_id.to_string(),
            instance_name: instance_name.to_string(),
            source: source.to_string(),
            project_id: project_id.to_string(),
            current_version_id: current_version_id.to_string(),
            mr_versions: Vec::new(),
            cf_files: Vec::new(),
            selected_index: 0,
            preselect_latest,
            fetch_handle: Some(slot),
        });
    }

    fn poll_modpack_version_picker(&mut self) {
        let Some(vp) = &mut self.modpack_version_picker else {
            return;
        };
        let Some(handle) = &vp.fetch_handle else {
            return;
        };
        let taken = handle.lock().unwrap().take();
        if let Some(result) = taken {
            vp.fetch_handle = None;
            match result {
                modpack_browser::VersionFetchResult::MrVersions(Ok(versions)) => {
                    if !vp.preselect_latest {
                        vp.selected_index = versions.iter().position(|v| v.id == vp.current_version_id).unwrap_or(0);
                    }
                    vp.mr_versions = versions;
                }
                modpack_browser::VersionFetchResult::CfFiles(Ok(files)) => {
                    if !vp.preselect_latest {
                        vp.selected_index = files.iter().position(|f| f.id.to_string() == vp.current_version_id).unwrap_or(0);
                    }
                    vp.cf_files = files;
                }
                modpack_browser::VersionFetchResult::MrVersions(Err(e))
                | modpack_browser::VersionFetchResult::CfFiles(Err(e)) => {
                    log::warn!("Failed to fetch modpack versions: {e}");
                    self.modpack_version_picker = None;
                }
            }
        }
    }

    fn show_modpack_version_picker(&mut self, ui: &mut egui::Ui) {
        if self.modpack_version_picker.is_none() {
            return;
        }

        let mut action: Option<ModpackVersionPickerAction> = None;

        // Extract scalars without holding a borrow across the closure
        let is_loading = self.modpack_version_picker.as_ref().unwrap().fetch_handle.is_some();
        let title = self.modpack_version_picker.as_ref().unwrap().instance_name.clone();
        let theme = self.theme.clone();

        egui::Window::new(format!("Change Version — \"{}\"", title))
            .collapsible(false)
            .resizable(true)
            .default_size([500.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                if is_loading {
                    ui.horizontal(|ui| {
                        if let Some(ref t) = theme {
                            ui.add(egui::Spinner::new().color(t.color("accent")));
                        } else {
                            ui.spinner();
                        }
                        ui.label("Fetching versions...");
                    });
                    return;
                }

                if let Some(ref t) = theme {
                    ui.label(t.subtext("Select a version:"));
                } else {
                    ui.weak("Select a version:");
                }
                ui.add_space(4.0);

                let vp = self.modpack_version_picker.as_mut().unwrap();

                match vp.source.as_str() {
                    "modrinth" => {
                        if vp.mr_versions.is_empty() {
                            ui.label("No versions available.");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for (i, version) in vp.mr_versions.iter().enumerate() {
                                        let selected = vp.selected_index == i;
                                        let is_current = version.id == vp.current_version_id;
                                        let game_vers = version.game_versions.join(", ");
                                        let loaders = version.loaders.join(", ");
                                        let mut label = format!(
                                            "{} (MC {}) [{}]",
                                            version.name, game_vers, loaders
                                        );
                                        if is_current {
                                            label = format!("{} {label}", egui_phosphor::regular::CHECK_CIRCLE);
                                        }
                                        if ui.selectable_label(selected, &label).clicked() {
                                            vp.selected_index = i;
                                        }
                                    }
                                });
                        }
                    }
                    "curseforge" => {
                        if vp.cf_files.is_empty() {
                            ui.label("No versions available.");
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    for (i, file) in vp.cf_files.iter().enumerate() {
                                        let selected = vp.selected_index == i;
                                        let is_current = file.id.to_string() == vp.current_version_id;
                                        let release_tag = match file.release_type {
                                            1 => "Release",
                                            2 => "Beta",
                                            3 => "Alpha",
                                            _ => "Unknown",
                                        };
                                        let game_vers = file.game_versions.join(", ");
                                        let mut label = format!(
                                            "{} [{}] ({})",
                                            file.display_name, release_tag, game_vers
                                        );
                                        if is_current {
                                            label = format!("{} {label}", egui_phosphor::regular::CHECK_CIRCLE);
                                        }
                                        if ui.selectable_label(selected, &label).clicked() {
                                            vp.selected_index = i;
                                        }
                                    }
                                });
                        }
                    }
                    _ => {
                        ui.label("Unknown source.");
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let has_versions = match vp.source.as_str() {
                        "modrinth" => !vp.mr_versions.is_empty(),
                        "curseforge" => !vp.cf_files.is_empty(),
                        _ => false,
                    };
                    let apply_clicked = if let Some(ref t) = theme {
                        ui.add_enabled(has_versions, t.accent_button("Apply")).clicked()
                    } else {
                        ui.add_enabled(has_versions, egui::Button::new("Apply")).clicked()
                    };
                    if apply_clicked {
                        action = Some(ModpackVersionPickerAction::Apply);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(ModpackVersionPickerAction::Cancel);
                    }
                });
            });

        match action {
            Some(ModpackVersionPickerAction::Apply) => {
                let vp = self.modpack_version_picker.take().unwrap();
                let (version_id, version_name) = match vp.source.as_str() {
                    "modrinth" => {
                        if let Some(v) = vp.mr_versions.get(vp.selected_index) {
                            (v.id.clone(), v.name.clone())
                        } else {
                            return;
                        }
                    }
                    "curseforge" => {
                        if let Some(f) = vp.cf_files.get(vp.selected_index) {
                            (f.id.to_string(), f.display_name.clone())
                        } else {
                            return;
                        }
                    }
                    _ => return,
                };
                self.change_modpack_version = Some((
                    vp.instance_id,
                    crate::core::update::ModpackUpdateInfo {
                        latest_version_id: version_id,
                        latest_version_name: version_name,
                        current_version_id: vp.current_version_id.clone(),
                        current_version_name: String::new(),
                        source: vp.source,
                        project_id: vp.project_id,
                    },
                ));
            }
            Some(ModpackVersionPickerAction::Cancel) => {
                self.modpack_version_picker = None;
            }
            None => {}
        }
    }
}
