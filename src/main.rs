// Hide the console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod core;
mod theme;
mod ui;
mod util;

use eframe::egui;

fn placeholder_icon() -> egui::IconData {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let accent = [76u8, 175, 80, 255];
    let dark = [33u8, 110, 38, 255];
    let border = [20u8, 66, 23, 255];

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let on_edge = x < 2 || x >= size - 2 || y < 2 || y >= size - 2;
            let color = if on_edge {
                &border
            } else if ((x / 4) + (y / 4)) % 2 == 0 {
                &accent
            } else {
                &dark
            };
            rgba[idx..idx + 4].copy_from_slice(color);
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Lurch")
            .with_icon(std::sync::Arc::new(placeholder_icon())),
        ..Default::default()
    };

    eframe::run_native(
        "lurch",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(app::App::new(cc.egui_ctx.clone())))
        }),
    )
}
