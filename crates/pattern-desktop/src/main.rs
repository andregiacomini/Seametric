mod app;
mod canvas;
mod storage;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 800.0])
            .with_min_inner_size([850.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Seametric · Pattern CAD 0.1",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_theme(eframe::egui::Theme::Dark);
            Ok(Box::new(app::PatternApp::default()))
        }),
    )
}
