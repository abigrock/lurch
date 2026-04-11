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
- **Entry**: `src/main.rs` → `src/app.rs` (central `App` struct, ~2300 LOC)
- **Pattern**: Background threads + `Arc<Mutex<T>>` polling, UI request flags → App dispatch
- **Total**: ~15k LOC core/UI, ~8.4k LOC in UI layer, ~6.0k LOC in `core/` business logic

## Key Conventions

### State & Data Flow
- All state lives in the `App` struct (`src/app.rs`)
- Background work uses `std::thread::spawn` + `Arc<Mutex<Option<Result<T>>>>` — polled each frame in `App::poll_background_tasks()`
- UI views set request flags (e.g., `launch_requested`), consumed by `App::handle_view_requests()`
- File downloads are SHA1-verified via `crate::core::sha1_hex()` (wraps `sha1_smol`)
- Shared utilities in `src/core/mod.rs`: `USER_AGENT`, `http_client()`, `sha1_hex()`, `maven_path()`, `extract_zip_overrides()`
- JSON persistence for config, instances, accounts in platform directories (`src/util/paths.rs`)
- Mod loaders: Vanilla, Forge, NeoForge, Fabric, Quilt — profiles merged in `src/core/loader_profiles.rs`
- **Image loading** — uses egui's built-in loaders (`egui_extras::install_image_loaders` in `main.rs`, `all_loaders` feature). Display with `egui::Image::new(url).fit_to_exact_size(size)`. No custom image cache.
- **Missing mod detection** — modpack installs write `.modpack_mods.json` (JSON array of `ModpackModEntry` structs with name, download_url, manual flag, slug, file_id, website_url) into `<minecraft_dir>/`. Pre-launch check in `do_launch()` reads this file, verifies each mod exists in `mods/`, and shows `MissingModsState` dialog if any are missing. Dialog offers "Download Missing" (auto-downloads + manual download flow for blocked mods), "Launch Anyway" (bypasses via `force_launch_requested`), or "Cancel". Backward-compatible with legacy `Vec<String>` format.
- **Modpack updates** — clicking "Update available" badge opens the version picker (pre-selects latest) instead of auto-updating. Updates propagate `mc_version`, `loader`, and `loader_version` to the instance via `UpdatedModpackMeta`. Stale mods (present in old `.modpack_mods.json` but absent from the new version) are automatically removed from `mods/` during version changes.

### Theme & Styling
- Theme engine in `src/theme/mod.rs` — 33 bundled themes + user JSON themes
- **Dual-path theme pattern** — all themed UI must handle both paths:
  ```rust
  if let Some(t) = self.theme.as_ref() {
      // themed: use t.section_header(), t.accent_button(), etc.
  } else {
      // fallback: use RichText, plain Button, etc.
  }
  ```
- Theme helpers (`src/theme/mod.rs`): `section_header()` (15.0pt bold + strong + color), `title()`, `subtext()`, `accent_button()`, `danger_button()`, `ghost_button()`, `card_frame()`, `sidebar_frame()`, `topbar_frame()`, `code_frame()`, `content_frame()`
- UI helpers (`src/ui/helpers.rs`): `section_heading()` (wraps theme's `section_header` or plain heading), `card_frame()`, `card_grid()`, `SearchState<R>` generic, `row_hover_highlight()`, `project_tooltip()`, `load_more_button()`, `empty_state()`, `format_human_timestamp()`

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
- **Page headers**: title via `section_heading()` → `ui.separator()` → `ui.add_space(8.0)`
- **ComboBox widths**: 120px for sort, 140px for category (established pattern)

### Build & Quality
- `cargo check` must pass with **zero warnings**
- Use `egui::Frame::new()` (not deprecated `none()`)
- No `ui.horizontal` for rows with mixed-height widgets — always use `allocate_ui_with_layout` pattern above
