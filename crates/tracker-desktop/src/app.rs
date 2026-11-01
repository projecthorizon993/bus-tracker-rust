use eframe::egui;
use tracker_core::{
    EtaEngine, ProviderRegistry, Station, Route, CongestionInfo, Bus, Provider,
    ConfidenceLevel,
};
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tracker_core::Coord;

use serde_json;
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
    region_names: Vec<String>,
}

impl TrackerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let registry = ProviderRegistry::new();
        let provider_names: Vec<String> = registry.list().iter().map(|p| p.name().to_string()).collect();
        let region_names = vec!["Hanoi (hn)".to_string(), "Ho Chi Minh City (hcm)".to_string()];

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
            region_names,
        }
    }

    fn provider(&self) -> Option<&dyn Provider> {
        self.selected_provider_idx.and_then(|i| self.registry.list().get(i).map(|p| *p))
    }
}

impl eframe::App for TrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Auto-refresh timer
        self.refresh_timer += ctx.input(|i| i.unstable_dt) as f32;
        if self.refresh_timer > 15.0 {
            self.refresh_timer = 0.0;
            if self.screen == Screen::LiveBoard {
                self.fetch_data(ctx);
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
            ui.label(&self.status_message);
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
    }

    fn render_station_picker(&mut self, ui: &mut egui::Ui) {
        let provider = match self.provider() {
            Some(p) => p,
            None => return,
        };

        ui.label(format!("Provider: {}", provider.name()));
        ui.add_space(5.0);

        // Region selector
        ui.horizontal(|ui| {
            ui.label("Region:");
            egui::ComboBox::from_label("")
                .selected_text(format!("{} ({})", self.selected_region.to_uppercase(), provider.name()))
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

        // Load stations button
        if ui.button("Load stations & routes").clicked() {
            self.load_stations_and_buses(provider);
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

        let compute_btn = egui::Button::new("Compute ETA").enabled(can_compute);
        if ui.add(compute_btn).clicked() {
            self.compute_eta(provider);
        }

        if ui.button("← Back to providers").clicked() {
            self.screen = Screen::ProviderPicker;
        }
    }

    fn render_live_board(&mut self, ui: &mut egui::Ui) {
        let provider = match self.provider() {
            Some(p) => p,
            None => return,
        };

        ui.heading("Live Bus Board");
        ui.label(format!("{}: {} → {}", provider.name(),
            self.stations.get(self.selected_origin.unwrap_or(0)).map(|s| s.name.as_str()).unwrap_or("?"),
            self.stations.get(self.selected_dest.unwrap_or(0)).map(|s| s.name.as_str()).unwrap_or("?")));

        ui.add_space(10.0);

        if let Some(ref eta) = self.eta {
            // Big ETA card
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
                ui.label(format!("{}s ago", b.last_update));
                ui.end_row();
            }
        });

        if self.buses.is_empty() {
            ui.label("No live buses returned from API.");
        }

        ui.add_space(10.0);
        if ui.button("Refresh now").clicked() {
            self.fetch_buses_only(provider);
        }
        if ui.button("← Back to station picker").clicked() {
            self.screen = Screen::StationPicker;
        }
    }

    fn load_stations_and_buses(&mut self, provider: &dyn Provider) {
        self.loading = true;
        self.status_message = "Loading stations and buses...".to_string();

        let region = self.selected_region.clone();
        let provider_id = provider.id().to_string();
        let provider_name = provider.name().to_string();

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            // Fetch stations from bundled data (empty for now, will be populated from data file)
            let stations = load_stations_for_region(&region, &provider_id);

            // Fetch congestion
            let congestion = provider.fetch_congestion(&region).ok().flatten();

            // Fetch buses
            let buses = match provider.fetch_buses(&region) {
                Ok(b) => b,
                Err(_) => vec![],
            };

            let _ = tx.send((stations, congestion, buses));
        });

        // We can't block in egui, so store the receiver and check next frame
        // For simplicity in this prototype, we do a blocking fetch with a short timeout
        // In production, use eframe::epi or async with egui_extras
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok((stations, congestion, buses)) => {
                self.stations = stations;
                self.congestion = congestion;
                self.buses = buses;
                self.loading = false;
                self.status_message = format!("Loaded {} buses from {}", self.buses.len(), provider_name);
                if self.buses.is_empty() {
                    self.status_message.push_str(" (API returned no live data — try again or check connection)");
                }
            }
            Err(_) => {
                self.loading = false;
                self.status_message = "Timeout loading data. Check your internet connection.".to_string();
            }
        }
    }

    fn fetch_buses_only(&mut self, provider: &dyn Provider) {
        let region = self.selected_region.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let buses = match provider.fetch_buses(&region) {
                Ok(b) => b,
                Err(_) => vec![],
            };
            let _ = tx.send(buses);
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(buses) => {
                self.buses = buses;
                if self.selected_origin.is_some() && self.selected_dest.is_some() {
                    self.compute_eta(provider);
                }
                self.status_message = format!("Refreshed: {} buses", self.buses.len());
            }
            Err(_) => {
                self.status_message = "Refresh timed out.".to_string();
            }
        }
    }

    fn fetch_data(&mut self, ctx: &egui::Context) {
        let provider = match self.provider() {
            Some(p) => p,
            None => return,
        };
        self.fetch_buses_only(provider);
        ctx.request_repaint();
    }

    fn compute_eta(&mut self, provider: &dyn Provider) {
        let origin_idx = self.selected_origin.unwrap();
        let dest_idx = self.selected_dest.unwrap();
        let origin_id = self.stations[origin_idx].id.clone();
        let dest_id = self.stations[dest_idx].id.clone();
        let region = self.selected_region.clone();
        let provider_id = provider.id().to_string();
        let congestion = self.congestion.clone();
        let buses = self.buses.clone();

        let mut engine = EtaEngine::new().with_data(self.routes.clone(), self.stations.clone());

        // Run in thread to avoid blocking UI
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let estimate = engine.estimate(&origin_id, &dest_id, &region, &provider_id, &buses, congestion.as_ref());
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
    // Try to load from bundled data file first
    if let Some(stations) = load_stations_from_file() {
        return stations.into_iter()
            .filter(|s| s.region == region && s.provider == provider_id)
            .collect();
    }

    // Fallback: hardcoded sample stations for Hanoi (VinBus)
    if region == "hn" && provider_id == "vinbus" {
        return vec![
            Station { id: "hn_001".to_string(), name: "Bến Xe Yên Nghĩa".to_string(), coord: Coord { lat: 20.9360, lng: 105.7520 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            Station { id: "hn_002".to_string(), name: "Bến Xe Mỹ Đình".to_string(), coord: Coord { lat: 21.0570, lng: 105.7760 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            Station { id: "hn_003".to_string(), name: "Bến Xe Gia Lâm".to_string(), coord: Coord { lat: 21.0450, lng: 105.8370 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            Station { id: "hn_004".to_string(), name: "Bến Xe Bắc Long Biên".to_string(), coord: Coord { lat: 21.0380, lng: 105.8570 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            Station { id: "hn_005".to_string(), name: "Bến Xe Bắc Sơn".to_string(), coord: Coord { lat: 21.0200, lng: 105.8350 }, region: "hn".to_string(), provider: "vinbus".to_string() },
        ];
    }

    // Fallback: HCMC
    if region == "hcm" && provider_id == "vinbus" {
        return vec![
            Station { id: "hcm_001".to_string(), name: "Bến Xe Miền Tây".to_string(), coord: Coord { lat: 10.7560, lng: 106.6290 }, region: "hcm".to_string(), provider: "vinbus".to_string() },
            Station { id: "hcm_002".to_string(), name: "Bến Xe Miền Đông".to_string(), coord: Coord { lat: 10.8050, lng: 106.7310 }, region: "hcm".to_string(), provider: "vinbus".to_string() },
            Station { id: "hcm_003".to_string(), name: "Bến Xe Chợ Lớn".to_string(), coord: Coord { lat: 10.7560, lng: 106.6640 }, region: "hcm".to_string(), provider: "vinbus".to_string() },
        ];
    }

    vec![]
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