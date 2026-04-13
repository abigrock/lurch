// Hide the console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod core;
mod theme;
mod ui;
mod util;

use eframe::egui;

fn logo_icon() -> egui::IconData {
    let png_data = include_bytes!("../assets/logo.png");
    let img = image::load_from_memory(png_data)
        .expect("Failed to decode logo PNG")
        .into_rgba8();

    let (width, height) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Lurch")
            .with_icon(std::sync::Arc::new(logo_icon())),
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
