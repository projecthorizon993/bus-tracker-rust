use crate::models::{
    Bus, CongestionInfo, ProviderConfig, PROVIDERS,
};
use crate::models::Coord;
use crate::models::keys::{VINBUS_AUTH_KEY, VINBUS_ZIP_PASSWORD};
use reqwest::blocking::Client;
use std::collections::HashMap;
use std::io::Cursor;
use std::time::SystemTime;
use zip::read::ZipArchive;

/// Error type for provider operations
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("ZIP error: {0}")]
    Zip(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

/// Marker: whether data came from live API or demo fallback
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataSource {
    Live,
    Demo,
}

/// Trait that every provider implements
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn fetch_buses(&self, region: &str) -> Result<Vec<Bus>, ProviderError>;
    fn fetch_route(&self, route_id: &str) -> Result<Option<crate::models::Route>, ProviderError>;
    fn fetch_congestion(&self, region: &str) -> Result<Option<CongestionInfo>, ProviderError>;
    fn fetch_stations(&self, region: &str) -> Result<Vec<crate::models::Station>, ProviderError>;
}

/// Registry of all providers
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn Provider>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        let mut providers: HashMap<String, Box<dyn Provider>> = HashMap::new();
        providers.insert("vinbus".to_string(), Box::new(VinBusProvider));
        providers.insert("gsm".to_string(), Box::new(GsmProvider));
        providers.insert("ihanoi".to_string(), Box::new(IhanoiProvider));
        Self { providers }
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn list(&self) -> Vec<&dyn Provider> {
        self.providers.values().map(|p| &**p).collect()
    }

    pub fn get_by_index(&self, index: usize) -> Option<&dyn Provider> {
        self.providers.values().nth(index).map(|p| &**p)
    }

    pub fn get(&self, id: &str) -> Option<&dyn Provider> {
        self.providers.get(id).map(|p| &**p)
    }

    pub fn provider_configs() -> &'static [ProviderConfig] {
        PROVIDERS
    }
}

/// ---------- VinBus Provider ----------
pub struct VinBusProvider;

impl Provider for VinBusProvider {
    fn id(&self) -> &str {
        "vinbus"
    }

    fn name(&self) -> &str {
        "VinBus"
    }

    fn fetch_buses(&self, region: &str) -> Result<Vec<Bus>, ProviderError> {
        let time = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let url = format!(
            "{}?regionCode={}&time={}",
            "https://vbcore-api.vinbus.vn/v1/client/public/bus_data/compress_data_by_time",
            region,
            time
        );

        let client = Client::builder().timeout(std::time::Duration::from_secs(15)).build()
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let text = client
            .get(&url)
            .header("client-version", "android|690")
            .header("device-id", format!("web-{}", time))
            .header("language", "en")
            .header("auth-key", VINBUS_AUTH_KEY)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| ProviderError::Http(e.to_string()))?
            .text()
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        // Try direct JSON first
        if let Ok(buses) = parse_vinbus_json(&text) {
            if !buses.is_empty() {
                return Ok(buses);
            }
        }

        // Try to find ZIP URL in response (ZIP is AES-encrypted — standard zip crate can't decrypt)
        if let Some(zip_url) = extract_zip_url(&text) {
            return fetch_vinbus_zip(&client, &zip_url).map_err(|e| {
                ProviderError::Zip(format!(
                    "ZIP is AES-encrypted (password: {}). Standard zip crate cannot decrypt. {}",
                    VINBUS_ZIP_PASSWORD, e
                ))
            });
        }

        Ok(vec![])
    }

    fn fetch_route(&self, _route_id: &str) -> Result<Option<crate::models::Route>, ProviderError> {
        Ok(None)
    }

    fn fetch_congestion(&self, _region: &str) -> Result<Option<CongestionInfo>, ProviderError> {
        Ok(None)
    }

    fn fetch_stations(&self, _region: &str) -> Result<Vec<crate::models::Station>, ProviderError> {
        Ok(vec![])
    }
}

/// ---------- Demo data provider (fallback when APIs fail) ----------
pub struct DemoProvider;

impl DemoProvider {
    /// Sample Hanoi stations
    pub fn hanoi_stations() -> Vec<crate::models::Station> {
        vec![
            crate::models::Station {
                id: "hn_001".to_string(),
                name: "Bến Xe Yên Nghĩa".to_string(),
                coord: Coord { lat: 20.9360, lng: 105.7520 },
                region: "hn".to_string(),
                provider: "vinbus".to_string(),
            },
            crate::models::Station {
                id: "hn_002".to_string(),
                name: "Bến Xe Mỹ Đình".to_string(),
                coord: Coord { lat: 21.0570, lng: 105.7760 },
                region: "hn".to_string(),
                provider: "vinbus".to_string(),
            },
            crate::models::Station {
                id: "hn_003".to_string(),
                name: "Bến Xe Gia Lâm".to_string(),
                coord: Coord { lat: 21.0450, lng: 105.8370 },
                region: "hn".to_string(),
                provider: "vinbus".to_string(),
            },
            crate::models::Station {
                id: "hn_004".to_string(),
                name: "Bến Xe Bắc Long Biên".to_string(),
                coord: Coord { lat: 21.0380, lng: 105.8570 },
                region: "hn".to_string(),
                provider: "vinbus".to_string(),
            },
            crate::models::Station {
                id: "hn_005".to_string(),
                name: "Bến Xe Bắc Sơn".to_string(),
                coord: Coord { lat: 21.0200, lng: 105.8350 },
                region: "hn".to_string(),
                provider: "vinbus".to_string(),
            },
        ]
    }

    /// Sample HCMC stations
    pub fn hcmc_stations() -> Vec<crate::models::Station> {
        vec![
            crate::models::Station {
                id: "hcm_001".to_string(),
                name: "Bến Xe Miền Tây".to_string(),
                coord: Coord { lat: 10.7560, lng: 106.6290 },
                region: "hcm".to_string(),
                provider: "vinbus".to_string(),
            },
            crate::models::Station {
                id: "hcm_002".to_string(),
                name: "Bến Xe Miền Đông".to_string(),
                coord: Coord { lat: 10.8050, lng: 106.7310 },
                region: "hcm".to_string(),
                provider: "vinbus".to_string(),
            },
            crate::models::Station {
                id: "hcm_003".to_string(),
                name: "Bến Xe Chợ Lớn".to_string(),
                coord: Coord { lat: 10.7560, lng: 106.6640 },
                region: "hcm".to_string(),
                provider: "vinbus".to_string(),
            },
        ]
    }

    /// Sample demo buses for Hanoi (route hn_001 → hn_002 direction)
    pub fn hanoi_buses() -> Vec<Bus> {
        vec![
            Bus {
                vehicle_id: "60B".to_string(),
                route_id: "01A".to_string(),
                route_no: Some("01A".to_string()),
                coord: Coord { lat: 21.0400, lng: 105.8000 },
                speed_kmh: 32.0,
                heading: Some(45.0),
                status: "running".to_string(),
                last_update: SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                distance_to_next_stop_m: Some(1200.0),
                current_stop_id: Some("hn_002".to_string()),
            },
            Bus {
                vehicle_id: "45C".to_string(),
                route_id: "01A".to_string(),
                route_no: Some("01A".to_string()),
                coord: Coord { lat: 21.0300, lng: 105.7900 },
                speed_kmh: 18.0,
                heading: Some(90.0),
                status: "running".to_string(),
                last_update: SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    - 60,
                distance_to_next_stop_m: Some(800.0),
                current_stop_id: Some("hn_002".to_string()),
            },
            Bus {
                vehicle_id: "12A".to_string(),
                route_id: "05B".to_string(),
                route_no: Some("05B".to_string()),
                coord: Coord { lat: 21.0500, lng: 105.7800 },
                speed_kmh: 8.0,
                heading: Some(180.0),
                status: "stopped".to_string(),
                last_update: SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    - 120,
                distance_to_next_stop_m: Some(200.0),
                current_stop_id: Some("hn_004".to_string()),
            },
        ]
    }

    /// Sample demo buses for HCMC
    pub fn hcmc_buses() -> Vec<Bus> {
        vec![
            Bus {
                vehicle_id: "88X".to_string(),
                route_id: "15".to_string(),
                route_no: Some("15".to_string()),
                coord: Coord { lat: 10.7800, lng: 106.6800 },
                speed_kmh: 25.0,
                heading: None,
                status: "running".to_string(),
                last_update: SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                distance_to_next_stop_m: Some(1500.0),
                current_stop_id: Some("hcm_002".to_string()),
            },
            Bus {
                vehicle_id: "33T".to_string(),
                route_id: "15".to_string(),
                route_no: Some("15".to_string()),
                coord: Coord { lat: 10.7600, lng: 106.6500 },
                speed_kmh: 12.0,
                heading: None,
                status: "running".to_string(),
                last_update: SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
                    - 90,
                distance_to_next_stop_m: Some(600.0),
                current_stop_id: Some("hcm_002".to_string()),
            },
        ]
    }
}

/// Parse VinBus JSON response (non-ZIP path)
fn parse_vinbus_json(text: &str) -> Result<Vec<Bus>, ProviderError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| ProviderError::Parse(e.to_string()))?;

    let buses = extract_bus_array(&value);
    let mut result = Vec::new();

    for b in buses {
        if let Some(bus) = bus_from_json_value(&b) {
            result.push(bus);
        }
    }

    Ok(result)
}

/// Extract array of bus objects from various JSON shapes
fn extract_bus_array(value: &serde_json::Value) -> Vec<serde_json::Value> {
    if let Some(arr) = value.as_array() {
        return arr.clone();
    }
    if let Some(list) = value.get("list").and_then(|v| v.as_array()) {
        return list.clone();
    }
    if let Some(buses) = value.get("buses").and_then(|v| v.as_array()) {
        return buses.clone();
    }
    if let Some(data) = value.get("data").and_then(|v| v.as_array()) {
        return data.clone();
    }
    if let Some(vehicles) = value.get("vehicles").and_then(|v| v.as_array()) {
        return vehicles.clone();
    }
    // Some APIs wrap in { result: [...] }
    if let Some(result) = value.get("result").and_then(|v| v.as_array()) {
        return result.clone();
    }
    vec![]
}

/// Convert a JSON value to Bus if it has the needed fields
fn bus_from_json_value(v: &serde_json::Value) -> Option<Bus> {
    let vehicle_id = v.get("vehicleNumber")
        .or_else(|| v.get("busId"))
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())?;

    let route_no = v.get("routeNo").and_then(|x| x.as_str()).map(|s| s.to_string());
    let route_id = route_no.clone().unwrap_or_else(|| vehicle_id.clone());

    let lat = v.get("lat").and_then(|x| x.as_f64())
        .or_else(|| v.get("latitude").and_then(|x| x.as_f64()))
        .unwrap_or(21.0285);
    let lng = v.get("lng").and_then(|x| x.as_f64())
        .or_else(|| v.get("longitude").and_then(|x| x.as_f64()))
        .or_else(|| v.get("lon").and_then(|x| x.as_f64()))
        .unwrap_or(105.8542);

    let speed_kmh = v.get("speed").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
    let heading = v.get("heading").and_then(|x| x.as_f64()).map(|x| x as f32);
    let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
    let last_update = v.get("lastServerUpdateTime")
        .and_then(|x| x.as_i64())
        .or_else(|| v.get("last_update").and_then(|x| x.as_i64()))
        .or_else(|| v.get("timestamp").and_then(|x| x.as_i64()))
        .unwrap_or(0);

    let distance_to_next_stop_m = v.get("distance")
        .and_then(|x| x.as_f64()).map(|x| x as f32);

    Some(Bus {
        vehicle_id,
        route_id,
        route_no,
        coord: Coord { lat, lng },
        speed_kmh,
        heading,
        status,
        last_update,
        distance_to_next_stop_m,
        current_stop_id: v.get("stopId").and_then(|x| x.as_str()).map(|s| s.to_string()),
    })
}

/// Extract ZIP URL from API response
fn extract_zip_url(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let zip_pos = lower.find(".zip")?;
    let start = text[..zip_pos].rfind("http").unwrap_or(0);
    let _end = text[zip_pos..]
        .find(|c: char| ['"', '\'', ' ', '<', '>'].contains(&c))
        .unwrap_or(text.len() - zip_pos);
    let url = &text[start..zip_pos + 4];
    if url.starts_with("http") {
        Some(url.to_string())
    } else {
        None
    }
}

/// Download and parse AES-encrypted ZIP from VinBus
fn fetch_vinbus_zip(client: &Client, zip_url: &str) -> Result<Vec<Bus>, ProviderError> {
    let bytes = client
        .get(zip_url)
        .header("auth-key", VINBUS_AUTH_KEY)
        .send()
        .map_err(|e| ProviderError::Http(e.to_string()))?
        .bytes()
        .map_err(|e| ProviderError::Http(e.to_string()))?;

    // Try to open ZIP. AES-encrypted entries will fail with standard zip crate.
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| ProviderError::Zip(e.to_string()))?;

    let mut buses = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| ProviderError::Zip(e.to_string()))?;
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut file, &mut contents).ok();
        if let Ok(parsed) = parse_vinbus_json(&contents) {
            buses.extend(parsed);
        }
    }

    Ok(buses)
}

/// ---------- GreenSM / Xanh SM Provider ----------
pub struct GsmProvider;

impl Provider for GsmProvider {
    fn id(&self) -> &str {
        "gsm"
    }

    fn name(&self) -> &str {
        "GreenSM / Xanh SM"
    }

    fn fetch_buses(&self, _region: &str) -> Result<Vec<Bus>, ProviderError> {
        // GSM uses device-id based tracking, not region-based bus list.
        Ok(vec![])
    }

    fn fetch_route(&self, _route_id: &str) -> Result<Option<crate::models::Route>, ProviderError> {
        Ok(None)
    }

    fn fetch_congestion(&self, _region: &str) -> Result<Option<CongestionInfo>, ProviderError> {
        Ok(None)
    }

    fn fetch_stations(&self, _region: &str) -> Result<Vec<crate::models::Station>, ProviderError> {
        Ok(vec![])
    }
}

/// GSM-specific: get location for a device (inherent method, not part of Provider trait)
impl GsmProvider {
    pub fn get_device_location(&self, device_id: &str) -> Result<serde_json::Value, ProviderError> {
        let client = Client::builder().timeout(std::time::Duration::from_secs(15)).build()
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let url = format!("{}/broadcasting/v1/public/location/{}", "https://api.gsm-api.net", device_id);
        let value: serde_json::Value = client
            .get(&url)
            .header("x-api-key", crate::models::keys::GSM_API_KEY)
            .header("x-device-id", format!("web-{}", SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()))
            .send()
            .map_err(|e| ProviderError::Http(e.to_string()))?
            .json()
            .map_err(|e| ProviderError::Parse(e.to_string()))?;
        Ok(value)
    }
}

/// ---------- iHanoi Traffic Camera Provider ----------
pub struct IhanoiProvider;

impl Provider for IhanoiProvider {
    fn id(&self) -> &str {
        "ihanoi"
    }

    fn name(&self) -> &str {
        "iHanoi Traffic Cameras"
    }

    fn fetch_buses(&self, _region: &str) -> Result<Vec<Bus>, ProviderError> {
        Ok(vec![])
    }

    fn fetch_route(&self, _route_id: &str) -> Result<Option<crate::models::Route>, ProviderError> {
        Ok(None)
    }

    fn fetch_congestion(&self, region: &str) -> Result<Option<CongestionInfo>, ProviderError> {
        if region != "hn" {
            return Ok(None);
        }
        let client = Client::builder().timeout(std::time::Duration::from_secs(15)).build()
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let url = "https://cds.hanoi.gov.vn/api/1.0/public/video-wall-cameras-v2?page=1&refresh=0";
        let _resp: serde_json::Value = client
            .get(url)
            .header("Accept", "application/json")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .map_err(|e| ProviderError::Http(e.to_string()))?
            .json()
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        Ok(Some(CongestionInfo {
            level: 2,
            label: "Unknown (camera ref)".to_string(),
            color: "#8E8E93".to_string(),
            multiplier: 1.3,
        }))
    }

    fn fetch_stations(&self, _region: &str) -> Result<Vec<crate::models::Station>, ProviderError> {
        Ok(vec![])
    }
}