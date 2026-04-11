# src/ui/instances/detail/

## Responsibility
Per-instance detail view — tabbed interface for managing a single instance's content (mods, worlds, shaders, servers).

## Design
- **Tab coordinator**: `InstanceDetailView` in `mod.rs` owns all tab state and routes to tab-specific render functions
- **DetailTab enum**: `Mods`, `Shaders`, `Worlds`, `Servers` — each maps to a `*_tab.rs` file
- **Mods sub-tabs**: `ModsSubTab` enum in `mods_tab/mod.rs`; each sub-tab in its own file
- **State ownership**: `InstanceDetailView` holds installed mods list, search states, server list etc.; tab files receive `&mut self` on the parent
- **Private module**: `mod detail;` (not `pub mod`) — only `InstanceDetailView` is exposed via `ViewMode::Detail`

## Modules

| Module | LOC | Purpose |
|--------|-----|---------|
| `mod.rs` | ~383 | `InstanceDetailView`: tab coordinator, owns all tab state |
| `mods_tab/mod.rs` | ~405 | Tab switcher + version picker (shared) |
| `mods_tab/installed.rs` | ~476 | Installed mods list: scan, filter, toggle, update |
| `mods_tab/browse_mr.rs` | ~470 | Modrinth mod browse + install |
| `mods_tab/browse_cf.rs` | ~552 | CurseForge mod browse + install |
| `servers_tab.rs` | ~285 | Servers: NBT server list, add/edit/remove/reorder |
| `shaders_tab.rs` | ~243 | Shaders: scan/enable/disable/remove shader packs |
| `worlds_tab.rs` | ~184 | Worlds: list saves with sizes, delete |

## Flow
1. User selects instance → `InstanceDetailView::show()` called
2. Tab bar renders → routes to appropriate `*_tab.rs` render function
3. Each tab reads instance-specific data from filesystem
4. Actions (install mod, add server) trigger background operations via App request flags

## Integration
- **Consumed by**: `src/ui/instances/mod.rs` (via `ViewMode::Detail`)
- **Depends on**: `src/core/modrinth.rs`, `src/core/local_mods.rs`, `src/core/worlds.rs`, `src/core/shaders.rs`, `src/core/servers.rs`, `src/core/instance.rs`
