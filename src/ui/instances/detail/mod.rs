mod mods_tab;
mod servers_tab;
mod shaders_tab;
mod worlds_tab;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::core::curseforge::CfCategory;
use crate::core::MutexExt;
use crate::core::instance::{Instance, ModOrigin};
use crate::core::local_mods::{self, InstalledMod};
use crate::core::modrinth::MrCategory;
use crate::core::servers::{self, Server};
use crate::core::shaders::{self, ShaderPack};
use crate::core::update::{ModUpdateInfo, ModUpdateMap};
use crate::core::worlds::{self, World};
use crate::ui::browse_common::BrowseTab;
use crate::ui::helpers::tab_button;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Mods,
    Shaders,
    Worlds,
    Servers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModsSubTab {
    Installed,
    BrowseModrinth,
    BrowseCurseForge,
}

pub struct InstanceDetailView {
    /// ID of the instance we're managing
    instance_id: String,
    /// Set to true when back navigation is requested
    pub back_requested: bool,

    // ── Top-level tab ────────────────────────────────────────────
    selected_tab: DetailTab,

    // ── Mods state ───────────────────────────────────────────────
    mods_sub_tab: ModsSubTab,
    installed: Vec<InstalledMod>,
    pub mr_browse: BrowseTab,
    pub cf_browse: BrowseTab,
    pub pending_toasts: Vec<crate::ui::notifications::Toast>,
    needs_rescan: bool,
    // Category fetch (kept in caller, not in BrowseTab)
    mr_categories: Option<Vec<MrCategory>>,
    mr_categories_fetch: Option<crate::core::BgTaskSlot<Vec<MrCategory>>>,
    cf_categories: Option<Vec<CfCategory>>,
    cf_categories_fetch: Option<crate::core::BgTaskSlot<Vec<CfCategory>>>,
    installed_filter: String,
    pending_origins: Arc<Mutex<Vec<ModOrigin>>>,
    pub mod_origin_updates: Vec<ModOrigin>,
    pub reconcile_origins_requested: bool,
    mod_update_check: Option<Arc<Mutex<Option<ModUpdateMap>>>>,
    mod_updates: HashMap<String, ModUpdateInfo>,
    mod_updates_checked: bool,
    mod_version_picker: Option<mods_tab::ModVersionPickerState>,
    confirm_mod_delete: Option<usize>,

    // ── Shaders state ────────────────────────────────────────────
    installed_shaders: Vec<ShaderPack>,
    shaders_filter: String,
    shaders_needs_rescan: bool,

    // ── Worlds state ─────────────────────────────────────────────
    installed_worlds: Vec<World>,
    worlds_filter: String,
    worlds_needs_rescan: bool,
    confirm_world_delete: Option<String>,

    // ── Shaders delete confirm ───────────────────────────────────
    confirm_shader_delete: Option<usize>,

    // ── Servers state ────────────────────────────────────────────
    server_list: Vec<Server>,
    servers_needs_rescan: bool,
    server_edit_name: String,
    server_edit_ip: String,
    editing_server_idx: Option<usize>,
    confirm_server_delete: Option<usize>,
}

impl InstanceDetailView {
    pub fn new(instance_id: String) -> Self {
        Self {
            instance_id,
            back_requested: false,
            selected_tab: DetailTab::Mods,
            mods_sub_tab: ModsSubTab::Installed,
            installed: Vec::new(),
            mr_browse: BrowseTab::default(),
            cf_browse: BrowseTab::default(),
            pending_toasts: Vec::new(),
            needs_rescan: true,
            mr_categories: None,
            mr_categories_fetch: None,
            cf_categories: None,
            cf_categories_fetch: None,
            installed_filter: String::new(),
            pending_origins: Arc::new(Mutex::new(Vec::new())),
            mod_origin_updates: Vec::new(),
            reconcile_origins_requested: false,
            mod_update_check: None,
            mod_updates: HashMap::new(),
            mod_updates_checked: false,
            mod_version_picker: None,
            confirm_mod_delete: None,
            installed_shaders: Vec::new(),
            shaders_filter: String::new(),
            shaders_needs_rescan: true,
            installed_worlds: Vec::new(),
            worlds_filter: String::new(),
            worlds_needs_rescan: true,
            confirm_world_delete: None,
            confirm_shader_delete: None,
            server_list: Vec::new(),
            servers_needs_rescan: true,
            server_edit_name: String::new(),
            server_edit_ip: String::new(),
            editing_server_idx: None,
            confirm_server_delete: None,
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub fn installed_filenames(&self) -> Vec<String> {
        self.installed.iter().map(|m| m.filename.clone()).collect()
    }

    /// Open for a specific tab (e.g., when coming from a specific button).
    #[allow(dead_code)]
    pub fn with_tab(mut self, tab: u8) -> Self {
        self.selected_tab = match tab {
            0 => DetailTab::Mods,
            1 => DetailTab::Shaders,
            2 => DetailTab::Worlds,
            3 => DetailTab::Servers,
            _ => DetailTab::Mods,
        };
        self
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        instance: &Instance,
        theme: &crate::theme::Theme,
    ) {
        // ── Header: back button + instance info ──────────────────
        let row_h = ui.spacing().interact_size.y + 12.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            if ui.add(theme.ghost_button(egui_phosphor::regular::ARROW_LEFT)).clicked() {
                self.back_requested = true;
            }
            ui.separator();
            let version_info = if instance.loader != crate::core::instance::ModLoader::Vanilla {
                format!("{} - {}", instance.mc_version, instance.loader)
            } else {
                instance.mc_version.clone()
            };
            // Icon + name/version on the left for all instances
            let icon_size = 40.0;
            if let Some(url) = &instance.icon {
                ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(icon_size, icon_size)).corner_radius(6));
            } else {
                crate::ui::helpers::icon_placeholder(ui, &instance.name, icon_size, theme);
            }
            ui.vertical(|ui| {
                ui.add(egui::Label::new(crate::ui::helpers::section_heading(&instance.name, theme)).truncate());
                ui.add(egui::Label::new(theme.subtext(&version_info)).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut clip = ui.clip_rect();
                clip.min.x = ui.max_rect().min.x;
                ui.set_clip_rect(clip);
                if let Some(origin) = &instance.modpack_origin {
                    let source_name = match origin.source.as_str() {
                        "modrinth" => "Modrinth",
                        "curseforge" => "CurseForge",
                        _ => "source",
                    };
                    let open_page = ui.add(theme.ghost_button(&format!(
                        "{}  Open on {}",
                        egui_phosphor::regular::GLOBE,
                        source_name
                    )));
                    if open_page.clicked()
                        && let Some(url) = crate::core::local_mods::modpack_project_url(&origin.source, &origin.project_id) {
                            let _ = open::that(&url);
                        }
                }
            });
        });
        ui.add_space(4.0);

        // ── Resolve directories ──────────────────────────────────
        let mc_dir = match instance.minecraft_dir() {
            Ok(d) => d,
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("Error: {e}"));
                return;
            }
        };
        let mods_dir = mc_dir.join("mods");
        let shaderpacks_dir = mc_dir.join("shaderpacks");
        let saves_dir = mc_dir.join("saves");
        let servers_dat = mc_dir.join("servers.dat");

        // ── Rescan if needed ─────────────────────────────────────
        if self.needs_rescan {
            self.installed = local_mods::scan_installed_mods(&mods_dir, &instance.mod_origins);
            self.reconcile_origins_requested = true;
            self.needs_rescan = false;
        }

        // ── Drain pending mod origins from background install threads ─
        {
            let mut pending = self.pending_origins.lock_or_recover();
            self.mod_origin_updates.append(&mut *pending);
        }

        if !self.mod_updates_checked
            && self.mod_update_check.is_none()
            && !instance.mod_origins.is_empty()
        {
            self.mod_updates_checked = true;
            let origins = instance.mod_origins.clone();
            let mc_version = instance.mc_version.clone();
            let loader = instance.loader.clone();
            let slot: Arc<Mutex<Option<ModUpdateMap>>> = Arc::new(Mutex::new(None));
            let slot_clone = Arc::clone(&slot);
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let results =
                    crate::core::update::check_mod_updates(&origins, &mc_version, &loader);
                *slot_clone.lock_or_recover() = Some(results);
                ctx.request_repaint();
            });
            self.mod_update_check = Some(slot);
        }

        if let Some(updates) = self
            .mod_update_check
            .as_ref()
            .and_then(|slot| slot.lock_or_recover().take())
        {
            self.mod_updates = updates;
            self.mod_update_check = None;
        }

        if self.shaders_needs_rescan {
            self.installed_shaders = shaders::scan_shaderpacks(&shaderpacks_dir);
            self.shaders_needs_rescan = false;
        }
        if self.worlds_needs_rescan {
            self.installed_worlds = worlds::scan_worlds(&saves_dir);
            self.worlds_needs_rescan = false;
        }
        if self.servers_needs_rescan {
            self.server_list = servers::read_servers(&servers_dat);
            self.servers_needs_rescan = false;
        }

        // ── Top-level tab bar ────────────────────────────────────
        let row_h = ui.spacing().interact_size.y + 4.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
            |ui| {
            for (tab, name, count) in [
                (DetailTab::Mods, "Mods", self.installed.len()),
                (DetailTab::Shaders, "Shaders", self.installed_shaders.len()),
                (DetailTab::Worlds, "Worlds", self.installed_worlds.len()),
                (DetailTab::Servers, "Servers", self.server_list.len()),
            ] {
                let active = self.selected_tab == tab;
                if tab_button(ui, name, active, theme) {
                    self.selected_tab = tab;
                }
                if count > 0 {
                    let (fill, text_color) = if active {
                        (theme.color("accent"), theme.button_fg())
                    } else {
                        (theme.color("surface"), theme.color("fg_dim"))
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(count.to_string())
                                    .size(10.0)
                                    .color(text_color),
                            );
                        });
                }
                ui.add_space(4.0);
            }
        });
        ui.separator();

        // ── Tab content ──────────────────────────────────────────
        match self.selected_tab {
            DetailTab::Mods => self.show_mods_tab(ui, instance, &mods_dir, theme),
            DetailTab::Shaders => self.show_shaders_tab(ui, &shaderpacks_dir, theme),
            DetailTab::Worlds => self.show_worlds_tab(ui, &saves_dir, theme),
            DetailTab::Servers => self.show_servers_tab(ui, &servers_dat, theme),
        }
    }
}
