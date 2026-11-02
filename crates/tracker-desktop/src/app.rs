use eframe::egui;
use tracker_core::{
    EtaEngine, ProviderRegistry, Station, Route, CongestionInfo, Bus, Provider,
    ConfidenceLevel, DataSource, DemoProvider,
};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};
use serde_json;

use tracker_core::Coord;

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    ProviderPicker,
    StationPicker,
    LiveBoard,
}

pub struct TrackerApp {
    registry: ProviderRegistry,
    screen: Screen,
    selected_provider_idx: Option<usize>,
    selected_region: String,
    stations: Vec<Station>,
    routes: Vec<Route>,
    selected_origin: Option<usize>,
    selected_dest: Option<usize>,
    buses: Vec<Bus>,
    eta: Option<tracker_core::EtaEstimate>,
    congestion: Option<CongestionInfo>,
    loading: bool,
    status_message: String,
    refresh_timer: f32,
    provider_names: Vec<String>,
    data_source: DataSource,
    last_error: Option<String>,
}

impl TrackerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let registry = ProviderRegistry::new();
        let provider_names: Vec<String> = registry.list().iter().map(|p| p.name().to_string()).collect();

        Self {
            registry,
            screen: Screen::ProviderPicker,
            selected_provider_idx: None,
            selected_region: "hn".to_string(),
            stations: Vec::new(),
            routes: Vec::new(),
            selected_origin: None,
            selected_dest: None,
            buses: Vec::new(),
            eta: None,
            congestion: None,
            loading: false,
            status_message: "Welcome. Select a provider to begin.".to_string(),
            refresh_timer: 0.0,
            provider_names,
            data_source: DataSource::Live,
            last_error: None,
        }
    }

    fn selected_provider(&self) -> Option<&dyn Provider> {
        self.selected_provider_idx
            .and_then(|i| self.registry.get_by_index(i))
    }
}

impl eframe::App for TrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_timer += ctx.input(|i| i.unstable_dt);
        if self.refresh_timer > 15.0 {
            self.refresh_timer = 0.0;
            if self.screen == Screen::LiveBoard {
                self.refresh_data(ctx);
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new("🚌 Bus Tracker").size(28.0));
            ui.separator();

            match self.screen {
                Screen::ProviderPicker => self.render_provider_picker(ui),
                Screen::StationPicker => self.render_station_picker(ui),
                Screen::LiveBoard => self.render_live_board(ui),
            }

            ui.separator();

            // Status bar with data source indicator and errors
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(20, 20, 25))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Data source badge
                        let (badge_text, badge_color) = match self.data_source {
                            DataSource::Live => ("● LIVE", egui::Color32::GREEN),
                            DataSource::Demo => ("▲ DEMO (API unreachable)", egui::Color32::from_rgb(255, 165, 0)),
                        };
                        ui.label(egui::RichText::new(badge_text).color(badge_color).small());
                        ui.separator();
                        ui.label(&self.status_message);
                        if let Some(ref err) = self.last_error {
                            ui.separator();
                            ui.label(egui::RichText::new(format!("⚠ {}", err)).color(egui::Color32::RED).small());
                        }
                    });
                });
        });
    }
}

impl TrackerApp {
    fn render_provider_picker(&mut self, ui: &mut egui::Ui) {
        ui.label("Select a provider:");
        for (i, name) in self.provider_names.iter().enumerate() {
            if ui.button(format!("{} — {}", i + 1, name)).clicked() {
                self.selected_provider_idx = Some(i);
                self.screen = Screen::StationPicker;
                self.status_message = format!("Selected {}. Pick origin & destination stations.", name);
            }
        }
        ui.add_space(10.0);
        if ui.button("Refresh provider list").clicked() {
            self.status_message = "Provider list refreshed.".to_string();
        }
        ui.add_space(20.0);
        ui.label(egui::RichText::new("Note: VinBus ZIP is AES-encrypted. If the live API fails, demo data is used.").small().color(egui::Color32::GRAY));
    }

    fn render_station_picker(&mut self, ui: &mut egui::Ui) {
        let Some(provider) = self.selected_provider() else {
            return;
        };

        ui.label(format!("Provider: {}", provider.name()));
        ui.add_space(5.0);

        let region_label = format!(
            "{} ({})",
            self.selected_region.to_uppercase(),
            provider.name()
        );
        ui.horizontal(|ui| {
            ui.label("Region:");
            egui::ComboBox::from_label("")
                .selected_text(&region_label)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.selected_region == "hn", "Hanoi (hn)").clicked() {
                        self.selected_region = "hn".to_string();
                    }
                    if ui.selectable_label(self.selected_region == "hcm", "Ho Chi Minh City (hcm)").clicked() {
                        self.selected_region = "hcm".to_string();
                    }
                });
        });

        ui.add_space(10.0);

        if ui.button("Load stations & buses").clicked() {
            let provider_id = self.registry.list()[self.selected_provider_idx.unwrap()].id().to_string();
            let region = self.selected_region.clone();
            self.load_stations_and_buses(&provider_id, &region);
        }

        if self.loading {
            ui.spinner();
            ui.label("Loading...");
        }

        ui.add_space(10.0);
        ui.label("Origin station:");
        egui::ComboBox::from_label("")
            .selected_text(if let Some(idx) = self.selected_origin {
                self.stations.get(idx).map(|s| s.name.clone()).unwrap_or_default()
            } else {
                "Select origin".to_string()
            })
            .show_ui(ui, |ui| {
                for (i, s) in self.stations.iter().enumerate() {
                    if ui.selectable_label(self.selected_origin == Some(i), &s.name).clicked() {
                        self.selected_origin = Some(i);
                    }
                }
            });

        ui.add_space(5.0);
        ui.label("Destination station:");
        egui::ComboBox::from_label("")
            .selected_text(if let Some(idx) = self.selected_dest {
                self.stations.get(idx).map(|s| s.name.clone()).unwrap_or_default()
            } else {
                "Select destination".to_string()
            })
            .show_ui(ui, |ui| {
                for (i, s) in self.stations.iter().enumerate() {
                    if ui.selectable_label(self.selected_dest == Some(i), &s.name).clicked() {
                        self.selected_dest = Some(i);
                    }
                }
            });

        ui.add_space(10.0);

        let can_compute = self.selected_origin.is_some()
            && self.selected_dest.is_some()
            && self.selected_origin != self.selected_dest;

        let compute_btn = egui::Button::new("Compute ETA");
        if ui.add_enabled(can_compute, compute_btn).clicked() {
            let provider_id = self.registry.list()[self.selected_provider_idx.unwrap()].id().to_string();
            self.compute_eta(provider_id);
        }

        if ui.button("← Back to providers").clicked() {
            self.screen = Screen::ProviderPicker;
        }
    }

    fn render_live_board(&mut self, ui: &mut egui::Ui) {
        let Some(provider) = self.selected_provider() else {
            return;
        };

        ui.heading("Live Bus Board");
        ui.label(format!("{}: {} → {}", provider.name(),
            self.stations.get(self.selected_origin.unwrap_or(0)).map(|s| s.name.as_str()).unwrap_or("?"),
            self.stations.get(self.selected_dest.unwrap_or(0)).map(|s| s.name.as_str()).unwrap_or("?")));

        ui.add_space(10.0);

        if let Some(ref eta) = self.eta {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(25, 25, 35))
                .inner_margin(20.0)
                .rounding(10.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("ETA").small().color(egui::Color32::GRAY));
                            ui.label(egui::RichText::new(format!("{}–{} min", eta.eta_min_low, eta.eta_min_high))
                                .size(48.0)
                                .strong());
                            ui.label(egui::RichText::new(format!("best: {} min | {}", eta.eta_min_best, eta.confidence))
                                .color(match eta.confidence {
                                    ConfidenceLevel::High => egui::Color32::GREEN,
                                    ConfidenceLevel::Medium => egui::Color32::from_rgb(255, 165, 0),
                                    ConfidenceLevel::Low => egui::Color32::RED,
                                    ConfidenceLevel::Unknown => egui::Color32::GRAY,
                                }));
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Distance").small().color(egui::Color32::GRAY));
                            ui.label(egui::RichText::new(format!("{:.1} m", eta.distance_m)).strong());
                            ui.label(egui::RichText::new("Speed").small().color(egui::Color32::GRAY));
                            ui.label(egui::RichText::new(format!("{:.1} km/h", eta.avg_speed_kmh)).strong());
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Congestion").small().color(egui::Color32::GRAY));
                            let color = egui::Color32::from_hex(&eta.congestion_color).unwrap_or(egui::Color32::GRAY);
                            ui.label(egui::RichText::new(&eta.congestion_label).color(color).strong());
                            ui.label(egui::RichText::new(format!("Stops: {}", eta.dwell_stops)).small());
                        });
                    });
                });

            ui.add_space(10.0);
            ui.label(egui::RichText::new(&eta.raw_details).small().color(egui::Color32::GRAY));
        } else {
            ui.label("No ETA computed yet. Select stations and click Compute ETA.");
        }

        ui.add_space(20.0);
        ui.label("Live buses:");
        egui::Grid::new("bus_grid").striped(true).show(ui, |ui| {
            ui.label("Vehicle");
            ui.label("Route");
            ui.label("Speed");
            ui.label("Status");
            ui.label("Last update");
            ui.end_row();
            for b in &self.buses {
                ui.label(&b.vehicle_id);
                ui.label(b.route_no.as_deref().unwrap_or("?"));
                ui.label(format!("{:.1} km/h", b.speed_kmh));
                ui.label(&b.status);
                let ago = SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    - b.last_update;
                ui.label(format!("{}s ago", ago.max(0)));
                ui.end_row();
            }
        });

        if self.buses.is_empty() {
            ui.label("No live buses returned from API.");
        }

        ui.add_space(10.0);
        if ui.button("Refresh now").clicked() {
            let provider_id = self.registry.list()[self.selected_provider_idx.unwrap()].id().to_string();
            self.refresh_buses_only(&provider_id);
        }
        if ui.button("← Back to station picker").clicked() {
            self.screen = Screen::StationPicker;
        }
    }

    fn load_stations_and_buses(&mut self, provider_id: &str, region: &str) {
        self.loading = true;
        self.status_message = "Loading stations and buses...".to_string();
        self.last_error = None;

        let provider = self
            .registry
            .list()
            .iter()
            .find(|p| p.id() == provider_id)
            .map(|p| *p);
        let Some(provider) = provider else {
            self.loading = false;
            self.status_message = "Provider not found.".to_string();
            return;
        };

        // Try live API first
        let live_congestion = provider.fetch_congestion(region).ok().flatten();
        let live_buses = provider.fetch_buses(region);

        match live_buses {
            Ok(buses) if !buses.is_empty() => {
                // Live data worked
                self.stations = load_stations_for_region(region, provider_id);
                self.congestion = live_congestion;
                self.buses = buses;
                self.data_source = DataSource::Live;
                self.loading = false;
                self.status_message = format!("Loaded {} buses (LIVE API)", self.buses.len());
            }
            Ok(_) | Err(_) => {
                // API returned empty or error — fall back to demo data
                let err_msg = match &live_buses {
                    Err(e) => Some(e.to_string()),
                    _ => None,
                };
                self.last_error = err_msg.map(|e| format!("Live API failed: {}", e));

                // Use demo stations and buses
                if provider_id == "vinbus" {
                    if region == "hn" {
                        self.stations = DemoProvider::hanoi_stations();
                        self.buses = DemoProvider::hanoi_buses();
                    } else if region == "hcm" {
                        self.stations = DemoProvider::hcmc_stations();
                        self.buses = DemoProvider::hcmc_buses();
                    } else {
                        self.stations = DemoProvider::hanoi_stations();
                        self.buses = DemoProvider::hanoi_buses();
                    }
                } else {
                    // For GSM/iHanoi, just use Hanoi demo stations
                    self.stations = DemoProvider::hanoi_stations();
                    self.buses = DemoProvider::hanoi_buses();
                }
                self.congestion = Some(CongestionInfo {
                    level: 2,
                    label: "Demo mode".to_string(),
                    color: "#FF9500".to_string(),
                    multiplier: 1.3,
                });
                self.data_source = DataSource::Demo;
                self.loading = false;
                self.status_message = format!(
                    "Using demo data ({} buses). Live API unavailable.",
                    self.buses.len()
                );
            }
        }
    }

    fn refresh_buses_only(&mut self, provider_id: &str) {
        let region = self.selected_region.clone();

        let provider = self
            .registry
            .list()
            .iter()
            .find(|p| p.id() == provider_id)
            .map(|p| *p);
        let Some(provider) = provider else {
            self.status_message = "Provider not found.".to_string();
            return;
        };

        let live_buses = provider.fetch_buses(&region);
        match live_buses {
            Ok(buses) if !buses.is_empty() => {
                self.buses = buses;
                self.data_source = DataSource::Live;
                self.last_error = None;
                if self.selected_origin.is_some() && self.selected_dest.is_some() {
                    self.compute_eta(provider_id.to_string());
                }
                self.status_message = format!("Refreshed: {} buses (LIVE)", self.buses.len());
            }
            _ => {
                self.data_source = DataSource::Demo;
                self.last_error = Some("Live API unreachable — keeping demo data".to_string());
                self.status_message = format!("Demo mode: {} buses", self.buses.len());
            }
        }
    }

    fn refresh_data(&mut self, ctx: &egui::Context) {
        let provider_id = self
            .registry
            .list()
            .iter()
            .find(|p| p.id() == self.registry.list()[self.selected_provider_idx.unwrap()].id())
            .map(|_| self.registry.list()[self.selected_provider_idx.unwrap()].id().to_string());
        let Some(provider_id) = provider_id else {
            return;
        };
        self.refresh_buses_only(&provider_id);
        ctx.request_repaint();
    }

    fn compute_eta(&mut self, provider_id: String) {
        let Some(origin_idx) = self.selected_origin else { return };
        let Some(dest_idx) = self.selected_dest else { return };
        if origin_idx == dest_idx {
            return;
        }

        let origin_id = self.stations[origin_idx].id.clone();
        let dest_id = self.stations[dest_idx].id.clone();
        let region = self.selected_region.clone();
        let buses = self.buses.clone();
        let congestion = self.congestion.clone();

        let mut engine = EtaEngine::new().with_data(self.routes.clone(), self.stations.clone());

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let estimate = engine.estimate(
                &origin_id,
                &dest_id,
                &region,
                &provider_id,
                &buses,
                congestion.as_ref(),
            );
            let _ = tx.send(estimate);
        });

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Some(eta)) => {
                self.eta = Some(eta);
                self.screen = Screen::LiveBoard;
                self.status_message = "ETA computed. Viewing live board.".to_string();
            }
            Ok(None) => {
                self.status_message = "No route found between these stations.".to_string();
            }
            Err(_) => {
                self.status_message = "ETA computation timed out.".to_string();
            }
        }
    }
}

fn load_stations_for_region(region: &str, provider_id: &str) -> Vec<Station> {
    if let Some(stations) = load_stations_from_file() {
        return stations
            .into_iter()
            .filter(|s| s.region == region && s.provider == provider_id)
            .collect();
    }

    if region == "hn" && provider_id == "vinbus" {
        return DemoProvider::hanoi_stations();
    }
    if region == "hcm" && provider_id == "vinbus" {
        return DemoProvider::hcmc_stations();
    }

    DemoProvider::hanoi_stations()
}

fn load_stations_from_file() -> Option<Vec<Station>> {
    let path = std::path::Path::new("data/stations.json");
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let stations: Vec<Station> = serde_json::from_str(&content).ok()?;
    Some(stations)
}