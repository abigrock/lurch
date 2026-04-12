# src/theme/

## Responsibility
JSON-based theme engine — defines visual appearance, ships 33 bundled themes, supports user-defined themes, and provides styling helper functions for consistent UI rendering.

## Design
- **Theme struct**: Color `HashMap` keyed by 17 semantic role names (bg, bg_secondary, bg_tertiary, surface, surface_hover, surface_active, overlay, overlay_hover, overlay_active, fg, fg_dim, fg_muted, accent, accent_secondary, success, error, warning), deserialized from `ThemeFile` JSON format
- **Always present**: Theme is a plain `Theme` struct (not `Option<Theme>`) — all UI code can call theme helpers directly without conditional checks
- **to_visuals()**: Transforms Theme colors into egui `Visuals` (window fill, widget colors, rounding, spacing)
- **33 bundled themes**: Catppuccin (Latte/Frappe/Macchiato/Mocha), Dracula, Nord, Gruvbox (Dark/Light), Solarized (Dark/Light), Tokyo Night, One Dark/Light, Rosé Pine (base/Moon/Dawn), Monokai, Everforest (Dark/Light), Kanagawa, Ayu (Dark/Light), High Contrast, OLED Black, plus 7 Minecraft-themed (Creeper, Nether, End, Redstone, Ocean Monument, Amethyst, Cherry Grove, Deep Dark, Lush Cave)
- **User themes**: Loaded from `data_dir/themes/*.json` via `load_user_themes()`
- **Styling helpers**:
  - Size constants: `BUTTON_HEIGHT` (32px), `TAB_HEIGHT` (28px)
  - Button helpers: `accent_button()`, `danger_button()`, `ghost_button()`, `icon_button()`, `accent_icon_button()`, `menu_item()`
  - Frame helpers: `card_frame()`, `sidebar_frame()`, `topbar_frame()`, `code_frame()`, `content_frame()`, `badge_frame()`
  - Text helpers: `section_header()`, `title()`, `subtext()`, `button_fg()`, `mono_font()`
  - Other: `style_menu()`

## Flow
1. App loads theme name from config → resolves to Theme (bundled or user)
2. `to_visuals()` called to set egui Visuals for the frame
3. UI modules call styling helpers for consistent frame/button rendering

## Integration
- **Consumed by**: `src/app.rs` (theme application), all `src/ui/` modules (styling helpers)
- **Depends on**: `src/util/paths.rs` (themes directory resolution)
