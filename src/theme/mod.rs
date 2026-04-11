use egui::{
    style::{HandleShape, ScrollStyle, Selection, WidgetVisuals},
    vec2, Button, Color32, CornerRadius, FontId, Frame, Margin, RichText, Stroke, Visuals,
};
use serde::Deserialize;
use std::collections::HashMap;

/// Standard height for all themed buttons (accent, danger, ghost, icon, tab).
pub const BUTTON_HEIGHT: f32 = 32.0;

/// JSON theme file schema: only name + colors
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub colors: HashMap<String, String>,
}

/// Parsed theme ready to apply
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub colors: HashMap<String, Color32>,
}

impl Theme {
    /// Load a theme from a JSON string
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        let file: ThemeFile = serde_json::from_str(json)?;
        let mut colors = HashMap::new();
        for (key, hex) in &file.colors {
            colors.insert(key.clone(), parse_hex(hex)?);
        }
        Ok(Self {
            name: file.name,
            colors,
        })
    }

    /// Get a color by semantic role name, with fallback
    pub fn color(&self, name: &str) -> Color32 {
        self.colors
            .get(name)
            .copied()
            .unwrap_or(Color32::PLACEHOLDER)
    }

    /// Apply this theme's visuals + spacing to an egui context
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_visuals(self.to_visuals());
        let spacing_scroll_bar_width = 8.0;
        ctx.global_style_mut(|style| {
            style.spacing.item_spacing = vec2(8.0, 6.0);
            style.spacing.button_padding = vec2(12.0, 6.0);
            style.spacing.window_margin = Margin::same(16);
            style.spacing.menu_margin = Margin::same(8);
            style.spacing.indent = 20.0;
            style.spacing.scroll = ScrollStyle {
                bar_width: spacing_scroll_bar_width,
                ..style.spacing.scroll
            };
        });
    }

    /// Build egui Visuals from this theme
    pub fn to_visuals(&self) -> Visuals {
        let bg = self.color("bg");
        let fg = self.color("fg");
        let surface = self.color("surface");
        let surface_hover = self.color("surface_hover");
        let _surface_active = self.color("surface_active");
        let _overlay = self.color("overlay");
        let overlay_hover = self.color("overlay_hover");
        let _overlay_active = self.color("overlay_active");
        let fg_muted = self.color("fg_muted");
        let fg_dim = self.color("fg_dim");
        let accent = self.color("accent");
        let accent_secondary = self.color("accent_secondary");
        let error = self.color("error");
        let warning = self.color("warning");
        let bg_secondary = self.color("bg_secondary");
        let bg_tertiary = self.color("bg_tertiary");

        // Determine if dark based on bg color luminance
        let dark_mode = {
            let [r, g, b, _] = bg.to_array();
            (r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114) < 128.0
        };

        let mut visuals = if dark_mode {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.dark_mode = dark_mode;
        visuals.override_text_color = Some(fg);
        visuals.hyperlink_color = accent;
        visuals.faint_bg_color = bg_secondary;
        visuals.extreme_bg_color = bg_tertiary;
        visuals.code_bg_color = bg_secondary;
        visuals.warn_fg_color = warning;
        visuals.error_fg_color = error;
        visuals.window_fill = bg;
        visuals.window_stroke = Stroke::new(1.0, surface);
        visuals.panel_fill = bg;
        visuals.window_corner_radius = CornerRadius::same(10);
        visuals.menu_corner_radius = CornerRadius::same(8);

        // Enhanced visual features
        visuals.collapsing_header_frame = true;
        visuals.slider_trailing_fill = true;
        visuals.handle_shape = HandleShape::Rect { aspect_ratio: 0.5 };
        visuals.striped = false;

        visuals.selection = Selection {
            bg_fill: accent.linear_multiply(0.35),
            stroke: Stroke::new(1.0, accent),
        };

        // In light themes, surface colors are noticeably darker than bg,
        // creating prominent dark fills for collapsing headers and other
        // noninteractive backgrounds that cover large page areas.
        // Use bg itself so these blend into the page; the stroke still
        // provides visual grouping.
        let ni_bg = if dark_mode { surface } else { bg };

        visuals.widgets.noninteractive = WidgetVisuals {
            bg_fill: ni_bg,
            weak_bg_fill: ni_bg,
            bg_stroke: Stroke::new(1.0, if dark_mode { surface_hover } else { surface }),
            fg_stroke: Stroke::new(1.0, fg_muted),
            corner_radius: CornerRadius::same(6),
            expansion: 0.0,
        };

        visuals.widgets.inactive = WidgetVisuals {
            bg_fill: surface,
            weak_bg_fill: surface,
            bg_stroke: Stroke::new(1.0, surface_hover),
            fg_stroke: Stroke::new(1.0, fg_dim),
            corner_radius: CornerRadius::same(6),
            expansion: 0.0,
        };

        visuals.widgets.hovered = WidgetVisuals {
            bg_fill: surface_hover,
            weak_bg_fill: surface_hover,
            bg_stroke: Stroke::new(1.0, accent),
            fg_stroke: Stroke::new(1.5, fg),
            corner_radius: CornerRadius::same(6),
            expansion: 1.0,
        };

        visuals.widgets.active = WidgetVisuals {
            bg_fill: accent,
            weak_bg_fill: accent,
            bg_stroke: Stroke::new(1.0, accent_secondary),
            fg_stroke: Stroke::new(2.0, bg),
            corner_radius: CornerRadius::same(6),
            expansion: 1.0,
        };

        visuals.widgets.open = WidgetVisuals {
            bg_fill: surface_hover,
            weak_bg_fill: surface_hover,
            bg_stroke: Stroke::new(1.0, overlay_hover),
            fg_stroke: Stroke::new(1.0, fg),
            corner_radius: CornerRadius::same(6),
            expansion: 0.0,
        };

        visuals
    }

    // ── Styling helpers ──────────────────────────────────────────────────

    /// Pick a foreground color that contrasts well on accent-colored fills.
    /// Dark themes have pastel accents → use dark `bg_tertiary` text.
    /// Light themes have saturated accents → use light `bg` text.
    pub fn button_fg(&self) -> Color32 {
        let bg = self.color("bg");
        let [r, g, b, _] = bg.to_array();
        let lum = r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114;
        if lum < 128.0 {
            self.color("bg_tertiary")
        } else {
            self.color("bg")
        }
    }

    /// Card container — for instance cards, account entries, any elevated block
    pub fn card_frame(&self) -> Frame {
        Frame::new()
            .fill(self.color("bg_secondary"))
            .stroke(Stroke::new(1.0, self.color("surface")))
            .inner_margin(Margin::same(12))
            .outer_margin(Margin::symmetric(0, 3))
            .corner_radius(CornerRadius::same(8))
    }

    /// Sidebar panel background
    pub fn sidebar_frame(&self) -> Frame {
        Frame::new()
            .fill(self.color("bg_secondary"))
            .inner_margin(Margin::symmetric(8, 12))
            .stroke(Stroke::new(1.0, self.color("surface")))
    }

    /// Top bar background
    pub fn topbar_frame(&self) -> Frame {
        Frame::new()
            .fill(self.color("bg_tertiary"))
            .inner_margin(Margin::symmetric(16, 10))
            .stroke(Stroke::new(1.0, self.color("surface")))
    }

    /// Code/console block
    pub fn code_frame(&self) -> Frame {
        Frame::new()
            .fill(self.color("bg_tertiary"))
            .stroke(Stroke::new(1.0, self.color("surface")))
            .inner_margin(Margin::same(10))
            .corner_radius(CornerRadius::same(6))
    }

    /// Content area with breathing room
    pub fn content_frame(&self) -> Frame {
        Frame::new()
            .fill(self.color("bg"))
            .inner_margin(Margin::same(20))
    }

    /// Section header styled text
    pub fn section_header(&self, text: &str) -> RichText {
        RichText::new(text)
            .size(15.0)
            .color(self.color("fg"))
            .strong()
    }

    /// Card/item title — explicit text color so it's always visible on card backgrounds
    pub fn title(&self, text: &str) -> RichText {
        RichText::new(text).color(self.color("fg")).strong()
    }

    /// Subdued label for descriptions, secondary info
    pub fn subtext(&self, text: &str) -> RichText {
        RichText::new(text).size(12.0).color(self.color("fg_muted"))
    }

    /// Primary action button (Launch, Install, Save)
    pub fn accent_button(&self, label: &str) -> Button<'static> {
        Button::new(
            RichText::new(label.to_string())
                .color(self.button_fg())
                .strong(),
        )
        .fill(self.color("accent"))
        .min_size(vec2(0.0, BUTTON_HEIGHT))
        .corner_radius(CornerRadius::same(6))
    }

    /// Danger button (Delete, Remove)
    pub fn danger_button(&self, label: &str) -> Button<'static> {
        Button::new(
            RichText::new(label.to_string())
                .color(self.button_fg())
                .strong(),
        )
        .fill(self.color("error"))
        .min_size(vec2(0.0, BUTTON_HEIGHT))
        .corner_radius(CornerRadius::same(6))
    }

    /// Ghost/outline button — transparent fill with accent stroke, for secondary actions
    pub fn ghost_button(&self, label: &str) -> Button<'static> {
        Button::new(
            RichText::new(label.to_string())
                .color(self.color("fg_dim"))
                .strong(),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, self.color("surface_hover")))
        .min_size(vec2(0.0, BUTTON_HEIGHT))
        .corner_radius(CornerRadius::same(6))
    }

    /// Square icon-only button — ghost style, BUTTON_HEIGHT × BUTTON_HEIGHT
    pub fn icon_button(&self, icon: &str) -> Button<'static> {
        Button::new(
            RichText::new(icon.to_string())
                .color(self.color("fg_dim"))
                .strong(),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, self.color("surface_hover")))
        .min_size(vec2(BUTTON_HEIGHT, BUTTON_HEIGHT))
        .corner_radius(CornerRadius::same(6))
    }

    /// Menu item — themed text, transparent at rest, visible hover
    pub fn menu_item(&self, label: &str) -> Button<'static> {
        Button::new(
            RichText::new(label.to_string())
                .color(self.color("fg_dim"))
                .strong(),
        )
    }

    /// Apply menu styling to a popup Ui — items transparent at rest, themed hover
    pub fn style_menu(&self, ui: &mut egui::Ui) {
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
        ui.visuals_mut().widgets.hovered.bg_fill = self.color("surface_hover");
    }

    /// Pill badge frame — standard margin, corner radius, and fill.
    /// Use `button_fg()` for text on accent-colored fills,
    /// or `color("fg_dim")` for text on neutral surface fills.
    pub fn badge_frame(&self, fill: Color32) -> Frame {
        Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(6, 2))
    }

    /// Monospace font for console/code
    pub fn mono_font() -> FontId {
        FontId::monospace(12.0)
    }
}

/// Load all bundled themes
pub fn bundled_themes() -> Vec<Theme> {
    let jsons = [
        include_str!("themes/catppuccin-latte.json"),
        include_str!("themes/catppuccin-frappe.json"),
        include_str!("themes/catppuccin-macchiato.json"),
        include_str!("themes/catppuccin-mocha.json"),
        include_str!("themes/dracula.json"),
        include_str!("themes/nord.json"),
        include_str!("themes/gruvbox-dark.json"),
        include_str!("themes/gruvbox-light.json"),
        include_str!("themes/solarized-dark.json"),
        include_str!("themes/solarized-light.json"),
        include_str!("themes/tokyo-night.json"),
        include_str!("themes/one-dark.json"),
        include_str!("themes/one-light.json"),
        include_str!("themes/rose-pine.json"),
        include_str!("themes/rose-pine-moon.json"),
        include_str!("themes/rose-pine-dawn.json"),
        include_str!("themes/monokai.json"),
        include_str!("themes/everforest-dark.json"),
        include_str!("themes/everforest-light.json"),
        include_str!("themes/kanagawa.json"),
        include_str!("themes/ayu-dark.json"),
        include_str!("themes/ayu-light.json"),
        include_str!("themes/high-contrast.json"),
        include_str!("themes/oled-black.json"),
        include_str!("themes/creeper.json"),
        include_str!("themes/nether.json"),
        include_str!("themes/end.json"),
        include_str!("themes/redstone.json"),
        include_str!("themes/ocean-monument.json"),
        include_str!("themes/amethyst.json"),
        include_str!("themes/cherry-grove.json"),
        include_str!("themes/deep-dark.json"),
        include_str!("themes/lush-cave.json"),
    ];
    jsons
        .iter()
        .filter_map(|j| Theme::from_json(j).ok())
        .collect()
}

fn parse_hex(hex: &str) -> anyhow::Result<Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        anyhow::bail!("Invalid hex color: #{hex}");
    }
    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;
    Ok(Color32::from_rgb(r, g, b))
}

/// Seed the user themes directory with an example theme and README on first use.
/// Only writes files if the directory is completely empty.
pub fn seed_user_themes_dir() {
    let Ok(dir) = crate::util::paths::themes_dir() else {
        return;
    };

    // Only seed if the directory is empty
    let is_empty = std::fs::read_dir(&dir)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true);

    if !is_empty {
        return;
    }

    let example_theme = r##"{
  "name": "Example Theme",
  "colors": {
    "bg":               "#1e1e2e",
    "bg_secondary":     "#181825",
    "bg_tertiary":      "#11111b",
    "surface":          "#313244",
    "surface_hover":    "#45475a",
    "surface_active":   "#585b70",
    "overlay":          "#6c7086",
    "overlay_hover":    "#7f849c",
    "overlay_active":   "#9399b2",
    "fg_muted":         "#a6adc8",
    "fg_dim":           "#bac2de",
    "fg":               "#cdd6f4",
    "accent":           "#89b4fa",
    "accent_secondary": "#b4befe",
    "success":          "#a6e3a1",
    "error":            "#f38ba8",
    "warning":          "#fab387"
  }
}"##;

    let readme = r##"CREATING CUSTOM THEMES FOR LURCH
================================

Drop any .json theme file into this folder and it will appear
in Settings > Appearance under the "Custom" section.

QUICK START
-----------
1. Copy "example-theme.json" and give it a new name
2. Change "name" to whatever you'd like
3. Edit the hex color values to taste
4. Restart Lurch (or re-open Settings) to see your theme

FILE FORMAT
-----------
Each theme is a JSON file with two fields:

  {
    "name": "My Theme",
    "colors": { ... }
  }

All 17 color keys are required. Values are "#rrggbb" hex codes.

COLOR REFERENCE
---------------
Background layers (darkest to lightest on a dark theme):

  bg               Main window background
  bg_secondary     Panels, sidebars, cards
  bg_tertiary      Deepest insets (top bar, code blocks)

Interactive surfaces:

  surface          Widget resting state, card borders
  surface_hover    Widget hovered
  surface_active   Widget pressed / active

Overlays (popups, tooltips):

  overlay          Overlay background
  overlay_hover    Overlay hovered elements
  overlay_active   Overlay active elements

Text (brightest to dimmest):

  fg               Primary text
  fg_dim           Secondary text, ghost buttons
  fg_muted         Subtle labels, metadata

Accent & status:

  accent           Primary actions, links, selection highlight
  accent_secondary Active widget outlines, secondary emphasis
  success          Success indicators
  error            Error text, danger buttons
  warning          Warning indicators

TIPS
----
- Start from an existing theme that's close to what you want.
  The built-in themes are in the source at src/theme/themes/.
- Keep good contrast between bg and fg for readability.
- Test both light and dark approaches — the UI adapts text
  colors on theme cards automatically based on bg luminance.
- Delete this file and example-theme.json if you don't need
  them. They'll only be regenerated if this folder is empty.
"##;

    if let Err(e) = std::fs::write(dir.join("example-theme.json"), example_theme) {
        log::warn!("Failed to write example theme: {e}");
    }
    if let Err(e) = std::fs::write(dir.join("README.txt"), readme) {
        log::warn!("Failed to write theme README: {e}");
    }
}

/// Load user themes from the platform themes directory
pub fn load_user_themes() -> anyhow::Result<Vec<Theme>> {
    let dir = crate::util::paths::themes_dir()?;
    let mut themes = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match std::fs::read_to_string(&path) {
                Ok(json) => match Theme::from_json(&json) {
                    Ok(theme) => themes.push(theme),
                    Err(e) => log::warn!("Bad theme {}: {e}", path.display()),
                },
                Err(e) => log::warn!("Can't read {}: {e}", path.display()),
            }
        }
    }
    Ok(themes)
}
