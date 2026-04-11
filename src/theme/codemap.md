# src/theme/

## Responsibility
JSON-based theme engine — defines visual appearance, ships 33 bundled themes, supports user-defined themes, and provides styling helper functions for consistent UI rendering.

## Design
- **Theme struct**: Color `HashMap` keyed by role names (e.g., "background", "accent", "text"), deserialized from `ThemeFile` JSON format
- **apply()**: Transforms Theme colors into egui `Visuals` (window fill, widget colors, rounding, spacing)
- **33 bundled themes**: Catppuccin (Latte/Frappe/Macchiato/Mocha), Dracula, Nord, Gruvbox (Dark/Light), Solarized (Dark/Light), Tokyo Night, One Dark/Light, Rosé Pine (base/Moon/Dawn), Monokai, Everforest (Dark/Light), Kanagawa, Ayu (Dark/Light), High Contrast, OLED Black, plus 7 Minecraft-themed (Creeper, Nether, End, Redstone, Ocean Monument, Amethyst, Cherry Grove, Deep Dark, Lush Cave)
- **User themes**: Loaded from `data_dir/themes/*.json` via `load_user_themes()`
- **Styling helpers**: `card_frame()`, `sidebar_frame()`, `topbar_frame()`, `code_frame()`, `content_frame()`, `section_header()`, `title()`, `subtext()`, `accent_button()`, `danger_button()`, `ghost_button()`

## Flow
1. App loads theme name from config → resolves to Theme (bundled or user)
2. `apply()` called to set egui Visuals for the frame
3. UI modules call styling helpers for consistent frame/button rendering

## Integration
- **Consumed by**: `src/app.rs` (theme application), all `src/ui/` modules (styling helpers)
- **Depends on**: `src/util/paths.rs` (themes directory resolution)
