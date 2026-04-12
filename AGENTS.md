# Lurch — Minecraft Launcher

## Repository Map

A full codemap is available at `codemap.md` in the project root.

Before working on any task, read `codemap.md` to understand:
- Project architecture and entry points
- Directory responsibilities and design patterns
- Data flow and integration points between modules

For deep work on a specific folder, also read that folder's `codemap.md`.

## Architecture Quick Reference

- **Language**: Rust (edition 2024)
- **GUI**: eframe/egui (immediate mode)
- **Entry**: `src/main.rs` → `src/app.rs` (central `App` struct, ~2200 LOC)
- **Pattern**: Background threads + `Arc<Mutex<T>>` polling, UI request flags → App dispatch
- **Total**: ~14k LOC core/UI, ~7.4k LOC in UI layer, ~6.5k LOC in `core/` business logic

## Key Conventions

### State & Data Flow
- All state lives in the `App` struct (`src/app.rs`)
- Background work uses `std::thread::spawn` + `Arc<Mutex<Option<Result<T>>>>` — polled each frame in `App::poll_background_tasks()`
- **Mutex poison safety** — all `.lock()` calls use `.lock_or_recover()` (trait `MutexExt` in `src/core/mod.rs`) which recovers from poisoned mutexes instead of panicking
- UI views set request flags (e.g., `launch_requested`), consumed by `App::handle_view_requests()`
- **View-level background tasks** — views can own their own `Arc<Mutex<Option<Result<T>>>>` fields (e.g., `export_task`, `import_task` in `InstancesView`) for view-scoped background work, polled in the view's `show()` method rather than `App::poll_background_tasks()`
- **Toast replacement** — for multi-step operations that show progress toasts, views use `pending_toasts: Vec<Toast>` (drained to `App.toasts` in `handle_view_requests()`) and `toast_removals: Vec<String>` (processed first to remove stale toasts before adding new ones) to cleanly swap "in-progress" toasts with result toasts in the same frame
- File downloads are SHA1-verified via `crate::core::sha1_hex()` (wraps `sha1_smol`). Downloads without SHA1 (Maven-style loader libs, Forge installer, some mods) use **post-download JAR validation** via `validate_jar()` / `is_jar_valid()` to detect truncated or corrupt `.jar` files.
- Shared utilities in `src/core/mod.rs`: `USER_AGENT`, `http_client()`, `sha1_hex()`, `validate_jar()`, `is_jar_valid()`, `maven_path()`, `extract_zip_overrides()`, `MutexExt` trait
- JSON persistence for config, instances, accounts in platform directories (`src/util/paths.rs`)
- Mod loaders: Vanilla, Forge, NeoForge, Fabric, Quilt — profiles merged in `src/core/loader_profiles.rs`
- **Image loading** — uses egui's built-in loaders (`egui_extras::install_image_loaders` in `main.rs`, `all_loaders` feature). Display with `egui::Image::new(url).fit_to_exact_size(size)`. No custom image cache.
- **Missing mod detection** — modpack installs write `.modpack_mods.json` (JSON array of `ModpackModEntry` structs with name, download_url, manual flag, slug, file_id, website_url) into `<minecraft_dir>/`. Pre-launch check in `do_launch()` reads this file, verifies each mod exists in `mods/`, and shows `MissingModsState` dialog if any are missing. Dialog offers "Download Missing" (auto-downloads + manual download flow for blocked mods), "Launch Anyway" (bypasses via `force_launch_requested`), or "Cancel". Backward-compatible with legacy `Vec<String>` format.
- **Modpack updates** — clicking "Update available" badge opens the version picker (pre-selects latest) instead of auto-updating. Updates propagate `mc_version`, `loader`, and `loader_version` to the instance via `UpdatedModpackMeta`. Stale mods (present in old `.modpack_mods.json` but absent from the new version) are automatically removed from `mods/` during version changes.

### Theme & Styling
- Theme engine in `src/theme/mod.rs` — 33 bundled themes + user JSON themes
- **Theme is always present** — `Theme` struct (not `Option<Theme>`). All UI code can call theme helpers directly without conditional checks.
- **Color roles** (17 semantic keys): `bg`, `bg_secondary`, `bg_tertiary`, `surface`, `surface_hover`, `surface_active`, `overlay`, `overlay_hover`, `overlay_active`, `fg`, `fg_dim`, `fg_muted`, `accent`, `accent_secondary`, `success`, `error`, `warning`
- **Button helpers** (`src/theme/mod.rs`) — all use `BUTTON_HEIGHT` (32px), corner radius 6:
  - `accent_button(label)` — accent fill + contrast-aware text
  - `danger_button(label)` — error fill + contrast-aware text
  - `ghost_button(label)` — transparent fill + surface_hover stroke + fg_dim text
  - `icon_button(icon)` — ghost style, square `BUTTON_HEIGHT × BUTTON_HEIGHT` for icon-only buttons
  - `accent_icon_button(icon)` — accent fill + contrast-aware text, square `BUTTON_HEIGHT × BUTTON_HEIGHT` for highlighted icon-only buttons (e.g., active filter indicator)
  - `menu_item(label)` — fg_dim text, no custom fill/stroke (denser, for popup menus)
- **Size constants**: `BUTTON_HEIGHT = 32.0`, `TAB_HEIGHT = 28.0`
- **Other helpers**: `section_header()` (15pt bold fg), `title()` (bold fg), `subtext()` (12pt fg_muted), `card_frame()` (bg_secondary fill), `sidebar_frame()`, `topbar_frame()` (bg_tertiary), `code_frame()` (bg_tertiary), `content_frame()` (bg fill), `badge_frame(fill)` (pill), `style_menu(ui)`, `mono_font()`
- UI helpers (`src/ui/helpers.rs`): `section_heading()` (wraps theme's `section_header`), `card_frame()`, `card_grid()`, `SearchState<R>` generic, `row_hover_highlight()`, `project_tooltip()`, `load_more_button()`, `empty_state()`, `format_human_timestamp()`, `tab_button()` (uses `TAB_HEIGHT`)
- **Browse component** (`src/ui/browse_common.rs`): shared `BrowseTab` struct used by mod and modpack browsers — handles search, filtering, sorting, list/grid rendering, pagination

### UI Layout Patterns
- **Vertical centering** — use `allocate_ui_with_layout` with centered cross-justify instead of `ui.horizontal` for rows with mixed-height widgets:
  ```rust
  let row_h = ui.spacing().interact_size.y + 4.0;
  ui.allocate_ui_with_layout(
      egui::vec2(ui.available_width(), row_h),
      egui::Layout::left_to_right(egui::Align::Center).with_cross_justify(true),
      |ui| { /* widgets */ },
  );
  ```
- **Row height**: `+4.0` for standard controls, `+12.0` for rows containing `section_heading` (15pt bold text)
- **TextEdit margins**: `egui::Margin::symmetric(4, 9)` for consistent vertical padding
- **Right-aligned sections**: nested `ui.with_layout(egui::Layout::right_to_left(egui::Align::Center).with_cross_justify(true), |ui| { ... })`
- **Horizontal-only clip for right-to-left sections** — prevents leftward overflow while preserving vertical hover borders:
  ```rust
  let mut clip = ui.clip_rect();
  clip.min.x = ui.max_rect().min.x;
  ui.set_clip_rect(clip);
  ```
- **Page headers**: title via `section_heading()` → `ui.add_space(8.0)` — use spacing instead of separators between control groups for a modern feel
- **ComboBox widths**: 100px for loader/sort combos, 120px for sort in browse views, 140px for category
- **Responsive search**: width `(available * 0.2).clamp(80.0, 160.0)` for toolbar search fields (use `0.4` multiplier when measuring inside a right-to-left sub-layout)
- **Text truncation**: use `ui.add(Label::new(richtext).truncate())` for **all** text in fixed-width rows — instance names, version labels, mod titles, filenames, server names, etc.
- **Progressive collapse** — header/toolbar rows with multiple controls use width-based breakpoints for responsive behavior:
  - Measure available width via `ui.available_width()` and define `is_wide` / `is_narrow` thresholds
  - **Wide**: full layout with text labels and inline controls
  - **Medium**: primary action buttons drop text → icon-only (e.g., `accent_icon_button` instead of `accent_button`)
  - **Narrow**: secondary controls (filters, sort, view toggles) collapse into a single icon button that opens a popover
  - Reference implementations: `src/ui/instances/mod.rs` header (800/550 breakpoints), `src/ui/browse_common.rs` filter row (600 breakpoint)
- **Filter popovers** — collapsed controls use `popup_below_widget` + `toggle_popup` (with `#[allow(deprecated)]`):
  - Vertical stack layout with `subtext()` labels ("LOADER", "SORT BY", "LAYOUT")
  - Full-width ComboBoxes inside popup; use distinct ID salts (e.g., `popup_loader_filter`) to avoid conflicts with inline versions
  - Active filter indicator: switch trigger button from `icon_button` to `accent_icon_button` when any non-default filter is applied
  - Icon: `egui_phosphor::regular::FADERS_HORIZONTAL` for filter/display-options popovers

### Build & Quality
- `cargo check` must pass with **zero warnings**
- Use `egui::Frame::new()` (not deprecated `none()`)
- No `ui.horizontal` for rows with mixed-height widgets — always use `allocate_ui_with_layout` pattern above
- All buttons must use themed helpers — no unstyled `ui.button()` calls
