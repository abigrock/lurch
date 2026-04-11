use eframe::egui;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::theme::Theme;

pub const SEARCH_DEBOUNCE: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

/// Paint a rounded-rect placeholder with the first letter of `name` centered inside.
/// Used when an instance has no icon image loaded yet.
pub fn icon_placeholder(ui: &mut egui::Ui, name: &str, size: f32, theme: &Theme) -> egui::Response {
    let desired = egui::vec2(size, size);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let bg = theme.color("surface");
        let fg = theme.color("accent");
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), bg);
        let letter = name
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            letter,
            egui::FontId::proportional(size * 0.45),
            fg,
        );
    }
    response
}

pub fn format_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn truncate_desc(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        let truncated: String = first_line.chars().take(max).collect();
        format!("{truncated}…")
    }
}

pub fn tab_button(ui: &mut egui::Ui, label: &str, active: bool, theme: &Theme) -> bool {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .color(if active {
                theme.color("accent")
            } else {
                theme.color("fg_dim")
            })
            .strong(),
    )
    .fill(egui::Color32::TRANSPARENT)
    .stroke(egui::Stroke::new(
        1.0,
        if active {
            theme.color("accent")
        } else {
            theme.color("surface_hover")
        },
    ))
    .corner_radius(egui::CornerRadius::same(6))
    .min_size(egui::vec2(0.0, crate::theme::BUTTON_HEIGHT));
    ui.add(btn).clicked()
}

pub fn section_heading(text: &str, theme: &Theme) -> egui::RichText {
    theme.section_header(text)
}

pub fn card_frame(_ui: &egui::Ui, theme: &Theme) -> egui::Frame {
    theme.card_frame()
}

struct GridCardStyle {
    fill: egui::Color32,
    stroke: egui::Stroke,
    rounding: egui::CornerRadius,
    margin: f32,
}

impl GridCardStyle {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            fill: theme.color("bg_secondary"),
            stroke: egui::Stroke::new(1.0, theme.color("surface")),
            rounding: egui::CornerRadius::same(8),
            margin: 12.0,
        }
    }
}

fn paint_grid_card(
    ui: &mut egui::Ui,
    cell_rect: egui::Rect,
    style: &GridCardStyle,
    actions_h: f32,
    gap: f32,
    mut render_body: impl FnMut(&mut egui::Ui),
    mut render_actions: impl FnMut(&mut egui::Ui),
) {
    ui.painter().rect(
        cell_rect,
        style.rounding,
        style.fill,
        style.stroke,
        egui::StrokeKind::Inside,
    );

    let inner = cell_rect.shrink(style.margin);
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let body_rect = egui::Rect::from_min_max(
        inner.min,
        egui::pos2(inner.right(), inner.bottom() - actions_h - gap),
    );
    let actions_rect = egui::Rect::from_min_max(
        egui::pos2(inner.left(), inner.bottom() - actions_h),
        inner.max,
    );

    let mut body_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    body_ui.set_clip_rect(body_ui.clip_rect().intersect(body_rect));
    render_body(&mut body_ui);

    let mut actions_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(actions_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    actions_ui.set_clip_rect(actions_ui.clip_rect().intersect(actions_rect.expand(2.0)));
    render_actions(&mut actions_ui);
}

/// Wrapping grid of fixed-size cards. Background/stroke painted manually so borders are never
/// clipped. Content is split into a body (top, clipped) and actions row (bottom-pinned).
#[allow(clippy::too_many_arguments)]
pub fn card_grid<T>(
    ui: &mut egui::Ui,
    id_salt: &str,
    items: &[T],
    card_w: f32,
    card_h: f32,
    theme: &Theme,
    has_more: bool,
    total: usize,
    mut render_body: impl FnMut(&mut egui::Ui, usize, &T),
    mut render_actions: impl FnMut(&mut egui::Ui, usize, &T),
) -> bool {
    let style = GridCardStyle::from_theme(theme);
    let actions_h = 32.0_f32;
    let actions_gap = 8.0;
    let gap = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.y = gap;
    let mut load_more_clicked = false;

    egui::ScrollArea::vertical()
        .id_salt(id_salt)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            let available = ui.available_width();
            let cols = ((available + gap) / (card_w + gap)).floor().max(1.0) as usize;

            for (row_idx, row) in items.chunks(cols).enumerate() {
                let (row_rect, _) =
                    ui.allocate_exact_size(egui::vec2(available, card_h), egui::Sense::hover());

                let mut x = row_rect.min.x;
                for (col_idx, item) in row.iter().enumerate() {
                    let i = row_idx * cols + col_idx;
                    let cell_rect = egui::Rect::from_min_size(
                        egui::pos2(x, row_rect.min.y),
                        egui::vec2(card_w, card_h),
                    );

                    paint_grid_card(
                        ui,
                        cell_rect,
                        &style,
                        actions_h,
                        actions_gap,
                        |body_ui| render_body(body_ui, i, item),
                        |actions_ui| render_actions(actions_ui, i, item),
                    );

                    x += card_w + gap;
                }
            }

            if has_more {
                if load_more_button(ui, items.len(), total, theme) {
                    load_more_clicked = true;
                }
            }
        });
    load_more_clicked
}

pub fn grid_card(
    ui: &mut egui::Ui,
    cell_rect: egui::Rect,
    theme: &Theme,
    render_body: impl FnMut(&mut egui::Ui),
    render_actions: impl FnMut(&mut egui::Ui),
) {
    let style = GridCardStyle::from_theme(theme);
    let actions_h = 32.0_f32;
    let actions_gap = 8.0;
    paint_grid_card(
        ui,
        cell_rect,
        &style,
        actions_h,
        actions_gap,
        render_body,
        render_actions,
    );
}

pub struct SearchState<R: Send + 'static> {
    pub query: String,
    pub total: u32,
    pub offset: u32,
    pub appending: bool,
    pub last_edit: Option<Instant>,
    pub initialized: bool,
    #[allow(clippy::type_complexity)]
    pub pending: Option<Arc<Mutex<Option<Result<R, String>>>>>,
}

impl<R: Send + 'static> Default for SearchState<R> {
    fn default() -> Self {
        Self {
            query: String::new(),
            total: 0,
            offset: 0,
            appending: false,
            last_edit: None,
            initialized: false,
            pending: None,
        }
    }
}

/// Render category/tag pills inline. Shows at most `max_tags`, with a "+N more" overflow.
pub fn show_category_tags(ui: &mut egui::Ui, tags: &[&str], max_tags: usize, theme: &Theme) {
    if tags.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let shown = tags.len().min(max_tags);
        let remainder = tags.len().saturating_sub(max_tags);
        for &tag in tags.iter().take(shown) {
            let display_tag: std::borrow::Cow<'_, str> = if tag.chars().count() > 14 {
                let truncated: String = tag.chars().take(12).collect();
                std::borrow::Cow::Owned(format!("{truncated}…"))
            } else {
                std::borrow::Cow::Borrowed(tag)
            };
            let bg = theme.color("surface");
            let fg = theme.color("fg_dim");
            egui::Frame::new()
                .fill(bg)
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin {
                    left: 6,
                    right: 6,
                    top: 2,
                    bottom: 2,
                })
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(display_tag.as_ref())
                            .size(10.0)
                            .color(fg),
                    );
                });
        }
        if remainder > 0 {
            let fg = theme.color("fg_dim");
            ui.label(
                egui::RichText::new(format!("+{remainder} more"))
                    .size(10.0)
                    .color(fg),
            );
        }
    });
}

impl<R: Send + 'static> SearchState<R> {
    pub fn is_searching(&self) -> bool {
        self.pending.is_some()
    }

    pub fn poll(&mut self) -> Option<Result<R, String>> {
        let result = self.pending.as_ref()?.lock().ok()?.take();
        if result.is_some() {
            self.pending = None;
        }
        result
    }

    pub fn check_debounce(&mut self, ctx: &egui::Context) -> bool {
        if let Some(last_edit) = self.last_edit {
            if last_edit.elapsed() >= SEARCH_DEBOUNCE {
                if !self.is_searching() {
                    self.last_edit = None;
                    return true;
                }
                self.last_edit = None;
            } else {
                ctx.request_repaint_after(SEARCH_DEBOUNCE - last_edit.elapsed());
            }
        }
        false
    }

    pub fn fire_with_repaint<F>(&mut self, ctx: &egui::Context, search_fn: F)
    where
        F: FnOnce() -> Result<R, String> + Send + 'static,
    {
        let result: Arc<Mutex<Option<Result<R, String>>>> = Arc::new(Mutex::new(None));
        let result_clone = Arc::clone(&result);
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let outcome = search_fn();
            if let Ok(mut lock) = result_clone.lock() {
                *lock = Some(outcome);
            }
            ctx_clone.request_repaint();
        });
        self.pending = Some(result);
        ctx.request_repaint();
    }
}

pub fn format_human_timestamp(time: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = time.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// Paint a subtle hover highlight on a row's rectangle.
pub fn row_hover_highlight(ui: &egui::Ui, rect: egui::Rect, theme: &Theme) {
    if ui.rect_contains_pointer(rect) {
        let fg = theme.color("fg");
        let hover_color = egui::Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 12);
        ui.painter().rect_filled(rect, 4.0, hover_color);
    }
}

/// Show a project tooltip with icon, title, description, download count, and tags.
pub fn project_tooltip(
    ui: &mut egui::Ui,
    icon_url: Option<&str>,
    title: &str,
    description: &str,
    downloads: u64,
    tags: &[String],
    theme: &Theme,
) {
    ui.set_max_width(300.0);
    if let Some(url) = icon_url {
        ui.add(egui::Image::new(url).fit_to_exact_size(egui::vec2(64.0, 64.0)));
    }
    ui.label(theme.title(title));
    if !description.is_empty() {
        ui.label(theme.subtext(description));
    }
    let mut info: Vec<String> = Vec::new();
    info.push(format!("{} downloads", format_downloads(downloads)));
    if !tags.is_empty() {
        info.push(format!("Tags: {}", tags.join(", ")));
    }
    let info_text = info.join("\n");
    ui.label(theme.subtext(&info_text));
}

/// Show a "Load More" button with "Showing X of Y" count. Returns true if clicked.
pub fn load_more_button(ui: &mut egui::Ui, showing: usize, total: usize, theme: &Theme) -> bool {
    let mut clicked = false;
    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        if ui
            .add_sized([200.0, 32.0], theme.ghost_button("Load More"))
            .clicked()
        {
            clicked = true;
        }
        ui.add_space(4.0);
        let showing_text = format!("Showing {} of {}", showing, total);
        ui.label(theme.subtext(&showing_text));
    });
    ui.add_space(24.0);
    clicked
}

/// Show an empty state with a large icon and a message.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, message: &str, theme: &Theme) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(icon)
                .size(48.0)
                .color(theme.color("fg_muted")),
        );
        ui.add_space(8.0);
        ui.label(theme.subtext(message));
    });
}
