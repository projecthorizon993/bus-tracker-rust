use serde::{Deserialize, Serialize};

/// A live bus/vehicle from BusMap GPS endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapBus {
    pub vehicle_id: Option<String>,
    pub route_id: Option<String>,
    pub route_no: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub speed: Option<f32>,
    pub heading: Option<f32>,
    pub status: Option<String>,
    pub timestamp: Option<i64>,
}

/// A route from BusMap route list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapRoute {
    pub id: Option<String>,
    pub route_no: Option<String>,
    pub route_name: Option<String>,
    pub region_code: Option<String>,
    pub direction: Option<i32>,
}

/// Response from route list endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapRouteListResponse {
    pub list: Option<Vec<BusMapRoute>>,
    pub data: Option<Vec<BusMapRoute>>,
}

/// Response from route info endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapRouteInfoResponse {
    pub route_id: Option<String>,
    pub route_no: Option<String>,
    pub route_name: Option<String>,
    pub region_code: Option<String>,
    pub stations: Option<Vec<BusMapStation>>,
    pub polyline: Option<Vec<BusMapCoord>>,
}

/// A station in route info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapStation {
    pub id: Option<String>,
    pub name: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub sequence: Option<i32>,
}

/// A coordinate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapCoord {
    pub lat: f64,
    pub lng: f64,
}

/// Response from route_bus_gps endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapGpsResponse {
    pub buses: Option<Vec<BusMapBus>>,
    pub data: Option<Vec<BusMapBus>>,
    pub list: Option<Vec<BusMapBus>>,
}

/// Response from estimate_bus_to_station endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMapStationEtaResponse {
    pub station_id: Option<String>,
    pub buses: Option<Vec<BusMapBus>>,
    pub data: Option<Vec<BusMapBus>>,
}
