use crate::models::{
    Bus, CongestionInfo, ProviderConfig, PROVIDERS,
};
use crate::models::Coord;
use crate::models::keys::VINBUS_AUTH_KEY;
use log::{debug, error, info, warn};
use rand::Rng;
use reqwest::blocking::Client;
use serde_json;
use std::collections::HashMap;
use std::time::SystemTime;

/// Error type for provider operations
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Parse error: {0}")]
    Parse(String),
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
        // Try multiple VinBus API endpoints - the API endpoint changes periodically
        // and may return 404 if the path changed. We try multiple endpoints until
        // one returns data successfully.
        let region_code = region.to_lowercase();

        // Base URL for VinBus API
        let base_url = "https://www.timbus.vn:9149/SearchBuyt";

        // Multiple candidate endpoints to try
        // URL format per decompiled app: <base_url>/<method>?ReqParams=<encrypted_payload>
        let attempts: Vec<(String, &'static str)> = vec![
            (
                format!("{}/RouteBuytOptionV3", base_url),
                "VinBus API RouteBuytOptionV3",
            ),
            (
                format!("{}/VehicleState", base_url),
                "VinBus API VehicleState",
            ),
            (
                format!(
                    "https://api.busmap.vn/v2/public/busmap/vehicle_{}",
                    region_code
                ),
                "VinBus API v2 vehicle endpoint",
            ),
            (
                format!(
                    "https://api.busmap.vn/v1/client/public/bus_data/compress_data_by_time?regionCode={}&time={}",
                    region_code,
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                ),
                "VinBus API v1 compressed data",
            ),
            (
                format!(
                    "https://api.busmap.vn/v2/client/public/bus_data/compress_data_by_time?regionCode={}&time={}",
                    region_code,
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                ),
                "VinBus API v2 compressed data",
            ),
            (
                format!(
                    "https://api.busmap.vn/v1/public/bus_data/compress_data_by_time?regionCode={}&time={}",
                    region_code,
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                ),
                "VinBus API v1 public compress",
            ),
            (
                format!(
                    "https://api.busmap.vn/v2/public/bus_data/compress_data_by_time?regionCode={}&time={}",
                    region_code,
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                ),
                "VinBus API v2 public compress",
            ),
            (
                format!(
                    "https://vbcore-api.vinbus.vn/v2/client/public/bus_data/compress_data_by_time?regionCode={}&time={}",
                    region_code,
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                ),
                "VinBus API v2 base domain",
            ),
        ];

        info!("[VinBus] Fetching buses for region={}", region);

        let mut last_err: Option<String> = None;
        for (url, desc) in attempts {
            // Build encrypted payload for this attempt
            let encrypted_payload = self.build_encrypted_payload();
            let url_with_params = format!("{}?ReqParams={}", url, urlencoding::encode(&encrypted_payload));

            info!("[VinBus] Trying endpoint: {} ({})", url_with_params, desc);
            match self.fetch_buses_once(&url_with_params) {
                Ok(buses) if !buses.is_empty() => {
                    info!(
                        "[VinBus] Successfully fetched {} bus(es) from {}",
                        buses.len(), url_with_params
                    );
                    return Ok(buses);
                }
                Ok(_) => {
                    info!("[VinBus] Endpoint {} returned empty bus list", url_with_params);
                }
                Err(e) => {
                    warn!("[VinBus] Endpoint {} failed: {}", url_with_params, e);
                    last_err = Some(e.to_string());
                }
            }
        }

        let msg = format!(
            "VinBus API unavailable - all endpoints failed for region={}. \
             The VinBus API endpoint may have changed. Please check the API key and URL.",
            region
        );
        Err(ProviderError::Http(msg))
    }

    fn fetch_buses_once(&self, url: &str) -> Result<Vec<Bus>, ProviderError> {
        // url already contains ?ReqParams=<encrypted_payload> from fetch_buses()
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| {
                error!("[VinBus] Failed to build HTTP client: {}", e);
                ProviderError::Http(e.to_string())
            })?;

        let response = client
            .get(url)
            .header("client-version", "android|690")
            .header("device-id", format!("web-{}", SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()))
            .header("language", "en")
            .header("auth-key", VINBUS_AUTH_KEY)
            .header("Accept", "application/json")
            .header("User-Agent", "VinBus/1.0 (Android)")
            .send();

        let text = match response {
            Ok(resp) => {
                let status = resp.status();
                debug!("[VinBus] HTTP response status={}", status);
                if !status.is_success() {
                    error!("[VinBus] API returned non-success status: {}", status);
                }
                resp.text().map_err(|e| {
                    error!("[VinBus] Failed to read response body: {}", e);
                    ProviderError::Http(e.to_string())
                })?
            }
            Err(e) => {
                error!("[VinBus] HTTP request failed: {}", e);
                return Err(ProviderError::Http(e.to_string()));
            }
        };

        debug!("[VinBus] Response body length={} bytes", text.len());
        debug!(
            "[VinBus] Response body preview={}",
            &text[..std::cmp::min(200, text.len())]
        );

        // Try direct JSON first
        if let Ok(buses) = parse_vinbus_json(&text) {
            if !buses.is_empty() {
                info!("[VinBus] Parsed {} bus(es) from direct JSON", buses.len());
                return Ok(buses);
            }
            debug!("[VinBus] Direct JSON returned empty bus list");
        } else {
            debug!("[VinBus] Direct JSON parse failed");
        }

        Ok(vec![])
    }

    /// Build encrypted payload for TimBus API
    /// Encryption scheme from decompiled app (er2.m()):
    /// 1. Build payload: [random_int(4 bytes)] + [data_bytes]
    /// 2. Generate random key1 (10-15 bytes) and key2 (10-15 bytes)
    /// 3. Build bArr2 = key1 XOR key2, extended to max(key1_len, key2_len)
    /// 4. Output format:
    ///    byte 0: key1_len
    ///    bytes 1-2: data_length (little endian)
    ///    byte 3: key2_len
    ///    bytes 4..4+key1_len-1: key1
    ///    bytes 4+key1_len..4+key1_len+key2_len-1: key2
    ///    remaining bytes: encrypted data XORed with bArr2 cyclically
    /// 5. Base64 encode
    fn build_encrypted_payload(&self) -> String {
        use rand::Rng;

        // Build payload data: [random_int(4 bytes)] + [route_data_bytes]
        let mut payload_data = Vec::new();
        let rand_int: i32 = rand::thread_rng().gen();
        payload_data.extend_from_slice(&rand_int.to_le_bytes());
        let route_bytes = b"35B";
        payload_data.extend_from_slice(&route_bytes);

        // Generate random keys (10-15 bytes each, matching decompiled app)
        let key1_len = rand::thread_rng().gen_range(10..16);
        let key2_len = rand::thread_rng().gen_range(10..16);

        let key1: Vec<u8> = (0..key1_len).map(|_| rand::thread_rng().gen_range(48..127) as u8).collect();
        let key2: Vec<u8> = (0..key2_len).map(|_| rand::thread_rng().gen_range(48..127) as u8).collect();

        // Build bArr2 = key1 XOR key2, extended to max length
        let max_key_len = key1.len().max(key2.len());
        let mut b_arr2 = Vec::with_capacity(max_key_len);
        for i in 0..max_key_len {
            let k1 = if i < key1.len() { key1[i] } else { 0 };
            let k2 = if i < key2.len() { key2[i] } else { 0 };
            b_arr2.push(k1 ^ k2);
        }

        // XOR payload data with b_arr2 cyclically
        let mut encrypted_data = Vec::with_capacity(payload_data.len());
        for (i, byte) in payload_data.iter().enumerate() {
            encrypted_data.push(byte ^ b_arr2[i % b_arr2.len()]);
        }

        // Build output buffer:
        // byte 0: key1_len
        // bytes 1-2: data_length (little endian)
        // byte 3: key2_len
        // bytes 4..4+key1_len-1: key1
        // bytes 4+key1_len..4+key1_len+key2_len-1: key2
        // remaining bytes: encrypted data XORed with bArr2 cyclically
        let mut output = Vec::new();
        output.push(key1_len as u8);
        output.push((payload_data.len() & 0xFF) as u8);
        output.push(((payload_data.len() >> 8) & 0xFF) as u8);
        output.push(key2_len as u8);
        output.extend_from_slice(&key1);
        output.extend_from_slice(&key2);
        output.extend_from_slice(&encrypted_data);

        // Base64 encode
        base64::encode(&output)
    }

    fn fetch_stations(&self, region: &str) -> Result<Vec<crate::models::Station>, ProviderError> {
        let url = "https://api.busmap.vn/v1/stations".to_string();
        info!("[VinBus] Fetching stations for region={} url={}", region, url);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| {
                error!("[VinBus] Failed to build HTTP client: {}", e);
                ProviderError::Http(e.to_string())
            })?;

        let text = client
            .get(url)
            .header("auth-key", VINBUS_AUTH_KEY)
            .header("client-version", "android|690")
            .header("device-id", format!("web-{}", SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()))
            .header("language", "en")
            .header("Accept", "application/json")
            .header("User-Agent", "VinBus/1.0 (Android)")
            .send()
            .map_err(|e| {
                error!("[VinBus] Station fetch failed: {}", e);
                ProviderError::Http(e.to_string())
            })?
            .text()
            .map_err(|e| {
                error!("[VinBus] Station fetch read failed: {}", e);
                ProviderError::Http(e.to_string())
            })?;

        debug!("[VinBus] Stations response length={} bytes", text.len());

        let stations: Vec<crate::models::Station> = serde_json::from_str(&text).map_err(|e| {
            error!("[VinBus] Station parse failed: {}", e);
            ProviderError::Parse(e.to_string())
        })?;

        info!("[VinBus] Parsed {} stations", stations.len());
        Ok(stations)
    }

    fn fetch_route(&self, _route_id: &str) -> Result<Option<crate::models::Route>, ProviderError> {
        Ok(None)
    }

    fn fetch_congestion(&self, _region: &str) -> Result<Option<CongestionInfo>, ProviderError> {
        Ok(None)
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

    /// Sample demo buses for Hanoi (route 01A → hn_002 direction)
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

/// GSM-specific inherent methods (not part of the Provider trait)
impl GsmProvider {
    pub fn get_device_location(&self, device_id: &str) -> Result<serde_json::Value, ProviderError> {
        info!("[GreenSM] Fetching device location for device_id={}", device_id);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let url = format!("https://api.gsm-api.net/broadcasting/v1/public/location/{}", device_id);
        info!("[GreenSM] GET {}", url);
        let value: serde_json::Value = client
            .get(&url)
            .header("x-api-key", crate::models::keys::GSM_API_KEY)
            .header(
                "x-device-id",
                format!(
                    "web-{}",
                    SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                ),
            )
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
            debug!("[iHanoi] Region {} not supported for congestion", region);
            return Ok(None);
        }
        info!("[iHanoi] Fetching traffic cameras for region={}", region);
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let url = "https://cds.hanoi.gov.vn/api/1.0/public/video-wall-cameras-v2?page=1&refresh=0";
        info!("[iHanoi] GET {}", url);
        let _resp: serde_json::Value = client
            .get(url)
            .header("Accept", "application/json")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .map_err(|e| ProviderError::Http(e.to_string()))?
            .json()
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        info!("[iHanoi] Traffic cameras fetched successfully");
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

/// Parse VinBus JSON response (non-ZIP path)
fn parse_vinbus_json(text: &str) -> Result<Vec<Bus>, ProviderError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ProviderError::Parse(e.to_string()))?;

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
    let vehicle_id = v
        .get("vehicleNumber")
        .or_else(|| v.get("busId"))
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())?;

    let route_no = v.get("routeNo").and_then(|x| x.as_str()).map(|s| s.to_string());
    let route_id = route_no.clone().unwrap_or_else(|| vehicle_id.clone());

    let lat = v
        .get("lat")
        .and_then(|x| x.as_f64())
        .or_else(|| v.get("latitude").and_then(|x| x.as_f64()))
        .unwrap_or(21.0285);
    let lng = v
        .get("lng")
        .and_then(|x| x.as_f64())
        .or_else(|| v.get("longitude").and_then(|x| x.as_f64()))
        .or_else(|| v.get("lon").and_then(|x| x.as_f64()))
        .unwrap_or(105.8542);

    let speed_kmh = v.get("speed").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
    let heading = v.get("heading").and_then(|x| x.as_f64()).map(|x| x as f32);
    let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
    let last_update = v
        .get("lastServerUpdateTime")
        .and_then(|x| x.as_i64())
        .or_else(|| v.get("last_update").and_then(|x| x.as_i64()))
        .or_else(|| v.get("timestamp").and_then(|x| x.as_i64()))
        .unwrap_or(0);

    let distance_to_next_stop_m = v.get("distance").and_then(|x| x.as_f64()).map(|x| x as f32);

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