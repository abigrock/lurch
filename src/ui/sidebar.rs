use crate::theme::Theme;
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum View {
    #[default]
    Instances,
    Settings,
    Accounts,
    Console,
}

pub fn show(ui: &mut egui::Ui, current_view: &mut View, theme: &Theme) {
    ui.vertical(|ui| {
        ui.add_space(4.0);

        let views = [
            (
                View::Instances,
                egui_phosphor::regular::GAME_CONTROLLER,
                "Instances",
            ),
            (View::Settings, egui_phosphor::regular::GEAR_SIX, "Settings"),
            (View::Accounts, egui_phosphor::regular::USER, "Accounts"),
            (
                View::Console,
                egui_phosphor::regular::TERMINAL_WINDOW,
                "Console",
            ),
        ];

        for (view, icon, label) in &views {
            let selected = *current_view == *view;
            let response = nav_item(ui, theme, icon, label, selected);
            if response.clicked() {
                *current_view = *view;
            }
        }

        // Push version info to bottom with separator
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(4.0);
            ui.label(theme.subtext(concat!("Lurch v", env!("CARGO_PKG_VERSION"))));
            ui.separator();
        });
    });
}

/// Custom nav item with accent-colored active indicator
fn nav_item(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: &str,
    label: &str,
    active: bool,
) -> egui::Response {
    let desired_size = egui::vec2(ui.available_width() - 16.0, 34.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let bg = if active {
            theme.color("surface")
        } else if response.hovered() {
            theme.color("surface_hover")
        } else {
            egui::Color32::TRANSPARENT
        };
        let text_color = if active {
            theme.color("accent")
        } else if response.hovered() {
            theme.color("fg")
        } else {
            theme.color("fg_dim")
        };
        let accent = theme.color("accent");

        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), bg);

        if active {
            let bar_rect = egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(4.0, 4.0),
                egui::vec2(3.0, rect.height() - 8.0),
            );
            ui.painter()
                .rect_filled(bar_rect, egui::CornerRadius::same(2), accent);
        }

        // Icon at fixed X=16
        ui.painter().text(
            rect.left_center() + egui::vec2(16.0, 0.0),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(16.0),
            text_color,
        );
        // Label at fixed X=40
        ui.painter().text(
            rect.left_center() + egui::vec2(40.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            text_color,
        );
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    response
}
