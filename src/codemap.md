# src/

## Responsibility
Application root — contains the entry point, central App orchestrator, and all source modules organized by concern.

## Design
- **`main.rs`**: Entry point — configures eframe native options (1100x700 window, icon), loads Phosphor icon fonts, creates `App` instance, runs event loop
- **`app.rs`**: Central `App` struct (~1900 lines) — holds all application state, implements `eframe::App`. Orchestrates:
  - Background task polling via `Arc<Mutex<T>>` slots
  - View request handling (UI sets flags → App dispatches to core logic)
  - Game launching, modpack installs/updates, Java management
  - State management for instances, accounts, config, running processes
- **`core/`**: Business logic layer (21 modules)
- **`ui/`**: GUI presentation layer (7 modules + nested instance views)
- **`theme/`**: JSON-based theme engine with 33 bundled themes
- **`util/`**: Platform directory resolution

## Architecture Patterns
- **Immediate mode GUI**: App implements `eframe::App`, all state centralized in App struct
- **Background threading**: `std::thread::spawn` + `BgTaskSlot<T>` for async operations
- **Polling**: `App::poll_background_tasks()` checks mutex slots each frame; extracted helpers: `handle_skipped_mods()` (blocked CurseForge mod handling), `poll_manual_downloads()` (Downloads-dir watcher)
- **Request flags**: UI views set boolean flags, `App::handle_view_requests()` dispatches

## Integration
- `main.rs` → creates App → runs eframe event loop
- `app.rs` → imports and orchestrates `core/*`, `ui/*`, `theme/*`, `util/*`
