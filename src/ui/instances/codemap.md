# src/ui/instances/

## Responsibility
Instance management UI — instance list/grid, creation dialogs, modpack browsing, and routing to per-instance detail views. Heaviest UI submodule (~5000 LOC).

## Design
- **ViewMode enum**: `List` (instance grid + create dialog), `Modpacks` (delegates to `ModpackBrowser`), `Detail(view)` (delegates to `InstanceDetailView`)
- **Install requests as Option fields**: `install_requested: Option<ModpackInstallRequest>`, set by UI click, consumed by `App::handle_view_requests()` via `take()`
- **SearchState<R>**: Generic search state from `helpers.rs` used for Modrinth/CurseForge searches
- **Background polling**: `loader_versions_fetch: Option<Arc<Mutex<Option<Result<...>>>>>` follows standard Arc<Mutex> polling pattern
- **ModpackBrowser** now uses `BrowseTab` from `browse_common.rs` for shared search/filter/browse logic

## Modules

| Module | LOC | Purpose |
|--------|-----|---------|
| `mod.rs` | ~1490 | `InstancesView`: instance list, modpack view delegation; responsive header with progressive collapse (wide >800 / medium / narrow ≤550); background export/import with toast replacement; delegates create/edit to child modules |
| `modpack_browser.rs` | ~600 | `ModpackBrowser`: Modrinth + CurseForge modpack search/browse/install UI |
| `create_dialog.rs` | ~460 | Create instance dialog: Vanilla + import tabs; spawns import background thread |
| `edit_dialog.rs` | ~488 | Edit instance dialog + delete confirmation |
| `detail/` | ~2000 | Per-instance detail view with tabbed content management |

## Flow
1. Instance list renders all loaded instances as cards
2. Click instance → `ViewMode::Detail` → `InstanceDetailView`
3. Create new → dialog for name, version, loader selection
4. Modpacks tab → `ModpackBrowser` → search APIs → trigger install via request flags

## Integration
- **Consumed by**: `src/ui/mod.rs` (view routing)
- **Depends on**: `src/core/instance.rs`, `src/core/modrinth_modpack.rs`, `src/core/curseforge_modpack.rs`, `src/ui/helpers.rs`
