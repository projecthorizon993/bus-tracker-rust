use app::TrackerApp;
use eframe::egui;
use log::info;
use tracker_core::init_file_logger;

mod app;

fn main() -> eframe::Result<()> {
    init_file_logger().expect("Failed to initialize file logger");
    info!("Starting Bus Tracker desktop UI");

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
