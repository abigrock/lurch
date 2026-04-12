# instances/ — Instance Management UI

## OVERVIEW
Heaviest UI submodule (~5000 LOC). Instance list + creation dialog, modpack browsing (Modrinth + CurseForge), and per-instance detail with tabbed content management. Mod/modpack browsers use shared `BrowseTab` from `src/ui/browse_common.rs`.

## MODULE MAP
| Module | LOC | Role |
|--------|-----|------|
| `mod.rs` | ~1390 | `InstancesView`: instance list, modpack view delegation, `ViewMode` enum; delegates create/edit to `create_dialog.rs` and `edit_dialog.rs` |
| `modpack_browser.rs` | ~600 | `ModpackBrowser`: Modrinth + CurseForge modpack search/browse/install UI (uses `BrowseTab`) |
| `create_dialog.rs` | ~452 | Create instance dialog: Vanilla + import tabs |
| `edit_dialog.rs` | ~488 | Edit instance dialog + delete confirmation |
| `detail/mod.rs` | ~327 | `InstanceDetailView`: tab coordinator (Mods/Shaders/Worlds/Servers), owns all tab state |
| `detail/mods_tab/mod.rs` | ~368 | Tab switcher + version picker (shared) |
| `detail/mods_tab/installed.rs` | ~397 | Installed mods list: scan, filter, toggle, update |
| `detail/mods_tab/browse_mr.rs` | ~185 | Modrinth mod browse + install (uses `BrowseTab`) |
| `detail/mods_tab/browse_cf.rs` | ~189 | CurseForge mod browse + install (uses `BrowseTab`) |
| `detail/shaders_tab.rs` | ~200 | Shaders tab: scan/enable/disable/remove shader packs |
| `detail/worlds_tab.rs` | ~145 | Worlds tab: list saves with sizes, delete |
| `detail/servers_tab.rs` | ~229 | Servers tab: NBT server list, add/edit/remove/reorder |

## ARCHITECTURE
```
InstancesView (mod.rs)
├── ViewMode::List           → instance grid + create_dialog.rs / edit_dialog.rs
├── ViewMode::Modpacks       → delegates to ModpackBrowser (modpack_browser.rs)
└── ViewMode::Detail(view)   → delegates to InstanceDetailView (detail/)
    ├── DetailTab::Mods      → mods_tab/ (3 sub-tabs: Installed/BrowseModrinth/BrowseCurseForge)
    ├── DetailTab::Shaders   → shaders_tab.rs
    ├── DetailTab::Worlds    → worlds_tab.rs
    └── DetailTab::Servers   → servers_tab.rs
```

## WHERE TO LOOK
| Task | Start here |
|------|------------|
| Change instance list/grid | `mod.rs` — search `show_instance_list` |
| Change create dialog | `create_dialog.rs` |
| Add content tab | `detail/mod.rs` — add `DetailTab` variant + new `*_tab.rs` file + match arm in `show()` |
| Change mod browsing | `detail/mods_tab/` — `ModsSubTab` enum in `mod.rs` controls Installed/BrowseModrinth/BrowseCurseForge |
| Change modpack source | `modpack_browser.rs` — `ModpackSource` enum + corresponding search/render block |
| Change browse UI | `src/ui/browse_common.rs` — shared `BrowseTab` struct used by all mod/modpack browsers |
| Modify install flow | Install requests bubble up: `ModpackBrowser.install_requested` → `InstancesView` → `App::handle_view_requests()` |

## CONVENTIONS (instances-specific)
- **`detail/` is private** — `mod detail;` (not `pub mod`); only `InstanceDetailView` is exposed via `ViewMode::Detail`
- **`modpack_browser` is public** — `pub mod modpack_browser;` because `App` reads `ModpackBrowser` install request fields
- **Tab state owned by coordinator** — `InstanceDetailView` owns all tab state (installed mods, search states, server list, etc.); tab files receive `&mut self` on the parent
- **Install requests as Option fields** — `install_requested: Option<ModpackInstallRequest>`, set by UI click, consumed by `App::handle_view_requests()` which calls `take()`
- **`SearchState<R>` usage** — both `ModpackBrowser` and `InstanceDetailView` use `SearchState` from `helpers.rs` for Modrinth/CurseForge searches
- **`BrowseTab` delegation** — `browse_mr.rs`, `browse_cf.rs`, and `modpack_browser.rs` all use `BrowseTab` from `browse_common.rs` for search/filter/browse rendering
- **Loader version fetch** — `loader_versions_fetch: Option<Arc<Mutex<Option<Result<...>>>>>` in `InstancesView` follows standard background polling pattern
- **`#[allow(clippy::too_many_arguments)]`** — on `show_instance_list` and `show_modpacks_view` due to many App state params passed through
- **Export/import background tasks** — `export_task` and `import_task` are `Option<Arc<Mutex<Option<Result<T>>>>>` fields on `InstancesView`, polled in `show()`. Export spawns after file dialog; import spawns from `create_dialog.rs`. On import completion, auto-deduplicates instance name (appends ` (2)`, ` (3)`, etc.) and sets `mod_counts_dirty = true`
- **Toast replacement** — `pending_toasts` + `toast_removals` fields enable clean progress → result toast transitions. Kickoff pushes "Exporting/Importing instance..." toast; completion pushes removal of that message + new result toast, both processed in same frame via `handle_view_requests()`
