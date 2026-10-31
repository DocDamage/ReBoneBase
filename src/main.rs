#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ReBoneBase")
            .with_inner_size([1440., 940.])
            .with_min_inner_size([1000., 640.])
            .with_maximized(true),
        ..Default::default()
    };
    eframe::run_native(
        "ReBoneBase",
        options,
        Box::new(|cc| Ok(Box::new(app::ReboneApp::new(cc)))),
    )
}
