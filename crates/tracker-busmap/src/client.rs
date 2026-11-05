use crate::models::*;
use log::info;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Error type for BusMap client operations
#[derive(Debug, thiserror::Error)]
pub enum BusMapError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Config error: {0}")]
    Config(String),
}

/// BusMap API client with request signing
pub struct BusMapClient {
    client: reqwest::Client,
    device_id: String,
    base_city: String,
    base_vn: String,
}

impl BusMapClient {
    pub fn new(device_id: String) -> Result<Self, BusMapError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| BusMapError::Config(e.to_string()))?;

        Ok(Self {
            client,
            device_id,
            base_city: "https://api.busmap.city/v2".to_string(),
            base_vn: "https://api.busmap.vn/v2".to_string(),
        })
    }

    /// Build default headers for all BusMap requests
    fn default_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("package-name", HeaderValue::from_static("com.t7.busmaphn"));
        headers.insert("client-version", HeaderValue::from_static("android|20620"));
        headers.insert("device-id", HeaderValue::from_str(&self.device_id).unwrap());
        headers.insert("language", HeaderValue::from_static("vi"));
        headers.insert("Accept", HeaderValue::from_static("application/json"));
        headers
    }

    /// Compute the signed proof header: hash(epoch + ":Bus2019M@p_")
    fn sign_headers(&self, headers: &mut HeaderMap) {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let proof_input = format!("{}:Bus2019M@p_", epoch);
        let proof = Self::simple_hash(&proof_input);
        headers.insert("epoch", HeaderValue::from_str(&epoch.to_string()).unwrap());
        headers.insert("proof", HeaderValue::from_str(&proof).unwrap());
    }

    /// Simple hash for proof (matches the app's Y.H function behavior approximately)
    fn simple_hash(input: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// List available routes for a region
    pub async fn list_routes(&self, region_code: &str) -> Result<Vec<BusMapRoute>, BusMapError> {
        let url = format!("{}/route/public/list", self.base_city);
        info!("[BusMap] GET {}", url);

        let headers = self.default_headers();
        let resp = self
            .client
            .get(&url)
            .headers(headers)
            .query(&[("regionCode", region_code)])
            .send()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        if !status.is_success() {
            return Err(BusMapError::Http(format!(
                "Route list returned status {}: {}",
                status, text
            )));
        }

        // Try various response shapes
        let value: Value =
            serde_json::from_str(&text).map_err(|e| BusMapError::Parse(e.to_string()))?;
        let routes = extract_routes_from_value(&value);

        info!("[BusMap] Parsed {} routes", routes.len());
        Ok(routes)
    }

    /// Get route detail including stations and polyline
    pub async fn get_route_info(
        &self,
        route_id: &str,
        region_code: &str,
    ) -> Result<BusMapRouteInfoResponse, BusMapError> {
        let url = format!("{}/route/public/info", self.base_city);
        info!(
            "[BusMap] GET {}?routeId={}&regionCode={}",
            url, route_id, region_code
        );

        let headers = self.default_headers();
        let resp = self
            .client
            .get(&url)
            .headers(headers)
            .query(&[("routeId", route_id), ("regionCode", region_code)])
            .send()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        if !status.is_success() {
            return Err(BusMapError::Http(format!(
                "Route info returned status {}: {}",
                status, text
            )));
        }

        let info: BusMapRouteInfoResponse =
            serde_json::from_str(&text).map_err(|e| BusMapError::Parse(e.to_string()))?;

        info!(
            "[BusMap] Route info parsed: stations={:?}",
            info.stations.as_ref().map(|s| s.len())
        );
        Ok(info)
    }

    /// Get live GPS positions for buses on a route + direction (signed POST)
    pub async fn get_route_gps(
        &self,
        region_code: &str,
        route_id: &str,
        direction: i32,
    ) -> Result<Vec<BusMapBus>, BusMapError> {
        let url = format!("{}/public/busmap/route_bus_gps", self.base_city);
        info!(
            "[BusMap] POST {} (regionCode={}, routeId={}, direction={})",
            url, region_code, route_id, direction
        );

        let mut headers = self.default_headers();
        self.sign_headers(&mut headers);

        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .form(&[
                ("regionCode", region_code),
                ("routeId", route_id),
                ("direction", &direction.to_string()),
            ])
            .send()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        if !status.is_success() {
            return Err(BusMapError::Http(format!(
                "Route GPS returned status {}: {}",
                status, text
            )));
        }

        let value: Value =
            serde_json::from_str(&text).map_err(|e| BusMapError::Parse(e.to_string()))?;
        let buses = extract_buses_from_value(&value);

        info!("[BusMap] Parsed {} live bus positions", buses.len());
        Ok(buses)
    }

    /// Get ETA of buses to a specific station
    pub async fn estimate_bus_to_station(
        &self,
        region_code: &str,
        station_id: &str,
    ) -> Result<Vec<BusMapBus>, BusMapError> {
        let url = format!("{}/public/busmap/estimate_bus_to_station", self.base_vn);
        info!(
            "[BusMap] GET {} (regionCode={}, stationId={})",
            url, region_code, station_id
        );

        let headers = self.default_headers();
        let resp = self
            .client
            .get(&url)
            .headers(headers)
            .query(&[
                ("regionCode", region_code),
                ("stationId", station_id),
                ("group", "0"),
            ])
            .send()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| BusMapError::Http(e.to_string()))?;

        if !status.is_success() {
            return Err(BusMapError::Http(format!(
                "Station ETA returned status {}: {}",
                status, text
            )));
        }

        let value: Value =
            serde_json::from_str(&text).map_err(|e| BusMapError::Parse(e.to_string()))?;
        let buses = extract_buses_from_value(&value);

        info!("[BusMap] Parsed {} station ETA buses", buses.len());
        Ok(buses)
    }
}

/// Extract route array from various JSON response shapes
fn extract_routes_from_value(value: &Value) -> Vec<BusMapRoute> {
    if let Some(arr) = value.as_array() {
        return arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
    }
    for key in ["list", "data", "routes", "result"] {
        if let Some(arr) = value.get(key).and_then(|v| v.as_array()) {
            return arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
        }
    }
    vec![]
}

/// Extract bus array from various JSON response shapes
fn extract_buses_from_value(value: &Value) -> Vec<BusMapBus> {
    if let Some(arr) = value.as_array() {
        return arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
    }
    for key in ["buses", "list", "data", "result", "vehicles"] {
        if let Some(arr) = value.get(key).and_then(|v| v.as_array()) {
            return arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
        }
    }
    vec![]
}
