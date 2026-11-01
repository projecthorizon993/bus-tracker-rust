use eframe::egui;
use app::TrackerApp;

mod app;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Bus Tracker - VinBus / GreenSM / iHanoi",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1100.0, 700.0])
                .with_min_inner_size([800.0, 500.0]),
            ..Default::default()
        },
        Box::new(|cc| Box::new(TrackerApp::new(cc))),
    )
}