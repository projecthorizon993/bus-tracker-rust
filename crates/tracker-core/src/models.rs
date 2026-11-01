use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A geographic coordinate
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Coord {
    pub lat: f64,
    pub lng: f64,
}

impl Coord {
    pub fn haversine_meters(&self, other: &Coord) -> f64 {
        const R: f64 = 6_371_000.0; // Earth radius in meters
        let to_rad = |deg: f64| deg.to_radians();
        let dlat = to_rad(other.lat - self.lat);
        let dlng = to_rad(other.lng - self.lng);
        let a = (dlat / 2.0).sin().powi(2)
            + to_rad(self.lat).cos() * to_rad(other.lat).cos() * (dlng / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().asin();
        R * c
    }
}

/// A bus stop / station
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub coord: Coord,
    pub region: String,
    pub provider: String,
}

/// A live bus / vehicle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bus {
    pub vehicle_id: String,
    pub route_id: String,
    pub route_no: Option<String>,
    pub coord: Coord,
    pub speed_kmh: f32,
    pub heading: Option<f32>,
    pub status: String,
    pub last_update: i64, // unix seconds
    pub distance_to_next_stop_m: Option<f32>,
    pub current_stop_id: Option<String>,
}

/// A route definition with ordered stations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub name: String,
    pub region: String,
    pub provider: String,
    pub stations: Vec<String>, // ordered station IDs
    pub polyline: Vec<Coord>,   // route geometry
}

/// Distance along a route polyline from bus position to target station
#[derive(Debug, Clone)]
pub struct AlongRouteInfo {
    pub bus_to_dest_meters: f64,
    pub stations_between: u32,
    pub nearest_route_segment: Option<(usize, usize)>, // indices in polyline
}

/// ETA result with confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtaEstimate {
    pub origin_station: String,
    pub dest_station: String,
    pub vehicle_id: Option<String>,
    pub eta_min_low: i32,
    pub eta_min_high: i32,
    pub eta_min_best: i32,
    pub confidence: ConfidenceLevel,
    pub distance_m: f64,
    pub avg_speed_kmh: f32,
    pub congestion_label: String,
    pub congestion_color: String,
    pub dwell_stops: u32,
    pub last_update: i64,
    pub raw_details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
    Unknown,
}

impl std::fmt::Display for ConfidenceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfidenceLevel::High => write!(f, "high"),
            ConfidenceLevel::Medium => write!(f, "medium"),
            ConfidenceLevel::Low => write!(f, "low"),
            ConfidenceLevel::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CongestionInfo {
    pub level: u8,
    pub label: String,
    pub color: String,
    pub multiplier: f32,
}

/// Historical speed sample for a route segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedSample {
    pub segment_key: String,
    pub hour_of_day: u32,
    pub avg_speed_kmh: f32,
    pub sample_count: u32,
    pub last_updated: i64,
}

/// Provider configuration
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub regions: &'static [&'static str],
}

/// All providers extracted from decompiled apps
pub const PROVIDERS: &[ProviderConfig] = &[
    ProviderConfig {
        id: "vinbus",
        name: "VinBus",
        base_url: "https://vbcore-api.vinbus.vn",
        regions: &["hn", "hcm"],
    },
    ProviderConfig {
        id: "gsm",
        name: "GreenSM / Xanh SM",
        base_url: "https://api.gsm-api.net",
        regions: &["all"],
    },
    ProviderConfig {
        id: "ihanoi",
        name: "iHanoi Traffic Cameras",
        base_url: "https://cds.hanoi.gov.vn",
        regions: &["hn"],
    },
];

/// API keys extracted from decompiled APKs
pub mod keys {
    pub const VINBUS_AUTH_KEY: &str = "ViNbUs2@2lElectricDtPZG+ulbseO1WGN";
    pub const VINBUS_ZIP_PASSWORD: &str = "v!n#bUS@2023";
    pub const GSM_API_KEY: &str = "VSve2734sdfaxTj1SEscrsRepxCzhsrgS4JHEqMJllJF71rjCFtM0T5F1at83bwmI1eSwspEc";
}

/// Cache entry for historical speeds
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpeedCache {
    pub samples: HashMap<String, SpeedSample>,
}
