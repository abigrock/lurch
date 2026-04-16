# src/ui/

## Responsibility
GUI presentation layer — renders all views using egui immediate-mode framework. Handles user interaction, delegates actions to `App` via request flags and mutable state references.

## Design
- **View enum**: `sidebar.rs` defines `View {Instances, Modpacks, Settings, Accounts, Console}` for navigation routing
- **Request pattern**: UI views set boolean flags (e.g., `launch_requested`, `install_requested`) on shared state; `App::handle_view_requests()` processes them each frame
- **View structs**: Views are now structs (AccountsView, SettingsView, InstancesView, etc.) — each owns its own state and exposes a `show()` method taking `&mut App` or relevant state slices
- **Theme integration**: Views use `Theme` styling helpers (`card_frame`, `accent_button`, `section_header`, etc.) for consistent appearance. Theme is always present — no conditional checks needed.
- **Vertical centering**: All rows with mixed-height widgets use `allocate_ui_with_layout` with `left_to_right(Center).with_cross_justify(true)` instead of `ui.horizontal`
- **Progressive collapse**: Header/toolbar rows use width-based breakpoints — wide (full layout) → medium (icon-only buttons) → narrow (controls collapse into filter popover). Reference implementations: `instances/mod.rs` header, `browse_common.rs` filter row
- **Filter popovers**: Collapsed controls use `popup_below_widget` + `toggle_popup`; vertical stack with `subtext()` labels, full-width ComboBoxes, active filter indicator via `accent_icon_button`

## Modules

| Module | Purpose |
|--------|---------|
| `mod.rs` | View state definitions and shared UI types |
| `notifications.rs` | Toast notification models | `Toast` |
| `sidebar.rs` | Left navigation panel with View enum, custom styled nav items with accent indicator |
| `instances/` | Instance list + detail views (tabbed: mods, worlds, shaders, servers) |
| `accounts.rs` | Account management — add/remove Microsoft and offline accounts |
| `console.rs` | Game console/log viewer for running instances |
| `settings.rs` | `SettingsView`: settings page (theme, Java, memory, JVM args, CF API key) |
| `helpers.rs` | Reusable UI utility functions: `tab_button()`, `SearchState<R>`, `section_heading()`, `card_frame()`, `row_hover_highlight()`, `project_tooltip()`, and more |
| `instances/modpack_browser.rs` | Modpack browser (Modrinth + CurseForge search/install) |
| `browse_common.rs` | Shared `BrowseTab` struct: search, filtering, sorting, list/grid rendering, pagination for mod/modpack browsers; responsive filter row with progressive collapse popover |

## Flow
1. `App::update()` calls sidebar render → gets active `View`
2. Routes to appropriate view function based on active View
3. View reads App state, renders widgets, sets request flags
4. `App::handle_view_requests()` processes flags → dispatches to `core/` logic

## Integration
- **Consumed by**: `src/app.rs` (calls view render functions)
- **Depends on**: `src/core/` types (Instance, Account, Config), `src/theme/` (styling), egui built-in image loaders
