use axum::{
    routing::{get, post},
    Router,
    extract::{Json, Query, State},
    response::{Html, Redirect},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracker_core::{
    init_file_logger, EtaEngine, ProviderRegistry, CongestionInfo, Station, Bus, Route,
};
use tracker_busmap::BusMapClient;

// ============ Shared App State ============

#[derive(Clone, Default)]
struct AppState {
    current_region: Arc<Mutex<String>>,
    current_provider: Arc<Mutex<String>>,
    stations: Arc<Mutex<Vec<Station>>>,
    buses: Arc<Mutex<Vec<Bus>>>,
    congestion: Arc<Mutex<Option<CongestionInfo>>>,
    route_no: Arc<Mutex<String>>,
    eta_result: Arc<Mutex<Option<EtaResultJson>>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct EtaResultJson {
    origin_station: String,
    dest_station: String,
    vehicle_id: Option<String>,
    eta_min_low: i32,
    eta_min_high: i32,
    eta_min_best: i32,
    confidence: String,
    distance_m: f64,
    avg_speed_kmh: f32,
    congestion_label: String,
    congestion_color: String,
    dwell_stops: u32,
    last_update: i64,
    raw_details: String,
}

impl From<&tracker_core::EtaEstimate> for EtaResultJson {
    fn from(e: &tracker_core::EtaEstimate) -> Self {
        Self {
            origin_station: e.origin_station.clone(),
            dest_station: e.dest_station.clone(),
            vehicle_id: e.vehicle_id.clone(),
            eta_min_low: e.eta_min_low,
            eta_min_high: e.eta_min_high,
            eta_min_best: e.eta_min_best,
            confidence: format!("{:?}", e.confidence),
            distance_m: e.distance_m,
            avg_speed_kmh: e.avg_speed_kmh,
            congestion_label: e.congestion_label.clone(),
            congestion_color: e.congestion_color.clone(),
            dwell_stops: e.dwell_stops,
            last_update: e.last_update,
            raw_details: e.raw_details.clone(),
        }
    }
}

#[derive(Deserialize)]
struct FetchBusesRequest {
    provider: String,
    region: String,
}

#[derive(Deserialize)]
struct ComputeEtaRequest {
    origin: String,
    dest: String,
}

#[derive(Deserialize)]
struct PlanRouteRequest {
    route_no: String,
    region: String,
}

#[derive(Serialize)]
struct ProviderInfo {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct BusInfo {
    vehicle_id: String,
    route_no: String,
    speed_kmh: f32,
    status: String,
    last_update: i64,
    lat: f64,
    lng: f64,
}

#[derive(Serialize)]
struct RoutePlanInfo {
    route_no: String,
    station_count: usize,
    buses: Vec<BusBoardItem>,
}

#[derive(Serialize)]
struct BusBoardItem {
    vehicle_id: String,
    next_stop: String,
    eta_min: i32,
    confidence: String,
    speed_kmh: f32,
    auth_gps: String,
    auth_route: String,
}

// ============ Main ============

#[tokio::main]
async fn main() {
    tracker_core::init_file_logger().expect("Failed to initialize file logger");
    log::info!("Starting bus-tracker web UI on http://localhost:3000");

    let state = AppState::default();

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/providers", get(providers_handler))
        .route("/plan", get(plan_page_handler))
        .route("/api/providers", get(api_providers))
        .route("/api/fetch-buses", post(fetch_buses_api))
        .route("/api/compute-eta", post(compute_eta_api))
        .route("/api/plan-route", post(plan_route_api))
        .route("/api/status", get(status_api))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ============ Page Handlers ============

async fn index_handler() -> Redirect {
    Redirect::to("/providers")
}

async fn providers_handler() -> Html<String> {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Bus Tracker - Select Provider</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); min-height: 100vh; padding: 40px 20px; }
        .container { max-width: 500px; margin: 0 auto; background: white; border-radius: 16px; padding: 40px; box-shadow: 0 20px 60px rgba(0,0,0,0.3); }
        h1 { color: #333; margin-bottom: 8px; font-size: 32px; }
        .subtitle { color: #666; margin-bottom: 32px; }
        .provider-card { background: #f8f9fa; border: 2px solid #e9ecef; border-radius: 12px; padding: 20px; margin-bottom: 16px; cursor: pointer; transition: all 0.2s; display: flex; justify-content: space-between; align-items: center; }
        .provider-card:hover { border-color: #667eea; transform: translateY(-2px); box-shadow: 0 8px 24px rgba(102,126,234,0.2); }
        .provider-card h3 { color: #333; margin-bottom: 4px; }
        .provider-card p { color: #666; font-size: 14px; }
        .arrow { color: #667eea; font-weight: bold; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🚌 Bus Tracker</h1>
        <p class="subtitle">Select a provider to begin</p>
        <div id="providers">Loading...</div>
    </div>
    <script>
        fetch('/api/providers')
            .then(r => r.json())
            .then(providers => {
                document.getElementById('providers').innerHTML = providers.map(p => `
                    <div class="provider-card" onclick="location.href='/plan?provider=${p.id}&region=hn'">
                        <div>
                            <h3>${p.name}</h3>
                            <p>${p.id === 'busmap' ? 'Live bus GPS + route data' : 'Provider for ' + p.id}</p>
                        </div>
                        <span class="arrow">→</span>
                    </div>
                `).join('');
            })
            .catch(e => { document.getElementById('providers').innerHTML = 'Error: ' + e; });
    </script>
</body>
</html>"#.to_string(),
    )
}

async fn plan_page_handler(
    Query(params): Query<HashMap<String, String>>,
) -> Html<String> {
    let provider = params.get("provider").cloned().unwrap_or_else(|| "busmap".to_string());
    let region = params.get("region").cloned().unwrap_or_else(|| "hn".to_string());
    let route_no = params.get("route").cloned().unwrap_or_else(|| "".to_string());

    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Bus Tracker - Route Plan</title>
    <style>
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f3f4f6; min-height: 100vh; padding: 20px; }}
        .container {{ max-width: 900px; margin: 0 auto; }}
        .header {{ background: white; border-radius: 12px; padding: 24px; box-shadow: 0 4px 6px rgba(0,0,0,0.05); margin-bottom: 24px; display: flex; justify-content: space-between; align-items: center; }}
        .header h1 {{ color: #111; font-size: 24px; }}
        .badge {{ background: #667eea; color: white; padding: 6px 16px; border-radius: 20px; font-size: 14px; }}
        .card {{ background: white; border-radius: 12px; padding: 24px; box-shadow: 0 4px 6px rgba(0,0,0,0.05); margin-bottom: 24px; }}
        .input-group {{ display: flex; gap: 12px; margin-bottom: 20px; }}
        input[type="text"] {{ flex: 1; padding: 12px 16px; border: 2px solid #e5e7eb; border-radius: 8px; font-size: 16px; }}
        input[type="text"]:focus {{ outline: none; border-color: #667eea; }}
        button {{ background: #667eea; color: white; border: none; padding: 12px 24px; border-radius: 8px; font-size: 16px; font-weight: 600; cursor: pointer; }}
        button:hover {{ background: #5568d3; }}
        button:disabled {{ background: #ccc; cursor: not-allowed; }}
        table {{ width: 100%; border-collapse: collapse; }}
        th {{ text-align: left; color: #666; padding: 12px; border-bottom: 2px solid #e5e7eb; font-size: 14px; }}
        td {{ padding: 16px; border-bottom: 1px solid #f3f4f6; }}
        tr:hover {{ background: #f9fafb; }}
        .eta-badge {{ display: inline-block; padding: 4px 10px; border-radius: 12px; color: white; font-size: 13px; font-weight: 600; }}
        .high {{ background: #10b981; }}
        .medium {{ background: #f59e0b; }}
        .low {{ background: #ef4444; }}
        .auth {{ font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px; }}
        .auth-ok {{ color: #10b981; }}
        .auth-warn {{ color: #f59e0b; }}
        .auth-bad {{ color: #ef4444; }}
        .empty {{ text-align: center; color: #666; padding: 40px; }}
        .back-btn {{ background: #e5e7eb; color: #333; }}
        .back-btn:hover {{ background: #d1d5db; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div>
                <h1>Route {route_no_display}</h1>
                <span class="badge">Region: {region} | Provider: {provider}</span>
            </div>
            <button class="back-btn" onclick="location.href='/'">← Back</button>
        </div>

        <div class="card">
            <h2 style="margin-bottom: 16px; color: #333;">Track Route</h2>
            <div class="input-group">
                <input type="text" id="routeNo" placeholder="Enter route number (e.g. 60B)" value="{route_no_display}">
                <button onclick="planRoute()">Track Route</button>
            </div>
            <p style="color: #666; font-size: 14px; margin-bottom: 0;">Enter a bus route number to see all live buses and their ETAs.</p>
        </div>

        <div class="card" id="boardCard" style="display: none;">
            <h2 style="margin-bottom: 16px; color: #333;">Live Bus Board</h2>
            <div id="busCount" style="color: #666; margin-bottom: 16px;"></div>
            <table>
                <thead>
                    <tr>
                        <th>Bus</th>
                        <th>Next Stop</th>
                        <th>ETA</th>
                        <th>Confidence</th>
                        <th>Speed</th>
                        <th>GPS</th>
                        <th>Route</th>
                    </tr>
                </thead>
                <tbody id="busTable"></tbody>
            </table>
        </div>

        <div id="error" class="card" style="display: none; border-color: #ef4444; background: #fef2f2;">
            <h3 style="color: #ef4444;">Error</h3>
            <p id="errorMsg"></p>
        </div>
    </div>
    <script>
        let currentRegion = "{region}";
        let currentProvider = "{provider}";

        async function planRoute() {{
            const routeNo = document.getElementById('routeNo').value.trim();
            if (!routeNo) {{ alert('Please enter a route number'); return; }}

            const res = await fetch('/api/plan-route', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ route_no: routeNo, region: currentRegion }})
            }});

            if (!res.ok) {{
                document.getElementById('error').style.display = 'block';
                document.getElementById('errorMsg').textContent = 'Route not found or API error';
                document.getElementById('boardCard').style.display = 'none';
                return;
            }}

            const data = await res.json();
            document.getElementById('error').style.display = 'none';
            document.getElementById('boardCard').style.display = 'block';
            document.getElementById('busCount').textContent = data.buses.length + ' live bus(es) found';

            const tbody = document.getElementById('busTable');
            tbody.innerHTML = data.buses.map(b => `
                <tr>
                    <td><strong>${{b.vehicle_id}}</strong></td>
                    <td>${{b.next_stop}}</td>
                    <td><span class="eta-badge ${{b.confidence.toLowerCase()}}">${{b.eta_min}} min</span></td>
                    <td>${{b.confidence}}</td>
                    <td>${{b.speed_kmh.toFixed(1)}} km/h</td>
                    <td class="auth ${{b.auth_gps.includes('fresh') ? 'auth-ok' : 'auth-bad'}}">${{b.auth_gps}}</td>
                    <td class="auth ${{b.auth_route.includes('on-route') ? 'auth-ok' : 'auth-warn'}}">${{b.auth_route}}</td>
                </tr>
            `).join('');
        }}

        // Auto-plan if route number in URL
        const urlParams = new URLSearchParams(window.location.search);
        const routeFromUrl = urlParams.get('route');
        if (routeFromUrl) {{
            document.getElementById('routeNo').value = routeFromUrl;
            planRoute();
        }}
    </script>
</body>
</html>"#,
        route_no_display = route_no,
        region = region,
        provider = provider
    ))
}

// ============ API Handlers ============

async fn api_providers() -> Json<Vec<ProviderInfo>> {
    Json(vec![
        ProviderInfo { id: "busmap".to_string(), name: "BusMap".to_string() },
        ProviderInfo { id: "vinbus".to_string(), name: "VinBus".to_string() },
        ProviderInfo { id: "gsm".to_string(), name: "GreenSM / Xanh SM".to_string() },
        ProviderInfo { id: "ihanoi".to_string(), name: "iHanoi Traffic Cameras".to_string() },
    ])
}

async fn fetch_buses_api(
    State(_state): State<AppState>,
    Json(req): Json<FetchBusesRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let registry = ProviderRegistry::new();
    let p = registry.get(&req.provider).ok_or(StatusCode::NOT_FOUND)?;

    let buses = p.fetch_buses(&req.region)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    {
        let mut b = _state.buses.lock().await;
        *b = buses.clone();
    }

    let bus_infos: Vec<BusInfo> = buses.iter().map(|b| BusInfo {
        vehicle_id: b.vehicle_id.clone(),
        route_no: b.route_no.clone().unwrap_or_default(),
        speed_kmh: b.speed_kmh,
        status: b.status.clone(),
        last_update: b.last_update,
        lat: b.coord.lat,
        lng: b.coord.lng,
    }).collect();

    Ok(Json(serde_json::json!({
        "success": true,
        "count": bus_infos.len(),
        "buses": bus_infos
    })))
}

async fn compute_eta_api(
    State(state): State<AppState>,
    Json(req): Json<ComputeEtaRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let stations = state.stations.lock().await;
    let buses = state.buses.lock().await;
    let congestion = state.congestion.lock().await;
    let region = state.current_region.lock().await;
    let provider = state.current_provider.lock().await;

    let origin = stations.iter().find(|s| s.id == req.origin);
    let dest = stations.iter().find(|s| s.id == req.dest);

    if origin.is_none() || dest.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let engine = EtaEngine::new()
        .with_data(Vec::new(), stations.clone());

    let estimate = engine.estimate(
        &req.origin,
        &req.dest,
        &region,
        &provider,
        &buses,
        congestion.as_ref(),
    );

    match estimate {
        Some(eta) => {
            let result: EtaResultJson = (&eta).into();
            {
                let mut e = state.eta_result.lock().await;
                *e = Some(result.clone());
            }
            Ok(Json(serde_json::json!({ "success": true, "eta": result })))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn plan_route_api(
    State(state): State<AppState>,
    Json(req): Json<PlanRouteRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let device_id = std::env::var("BUSMAP_DEVICE_ID").unwrap_or_else(|_| "web-cli".to_string());
    let rt = tokio::runtime::Runtime::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let busmap = BusMapClient::new(device_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let routes = rt.block_on(busmap.list_routes(&req.region))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let matching: Vec<_> = routes.iter()
        .filter(|r| r.route_no.as_deref() == Some(&req.route_no) || r.route_name.as_deref() == Some(&req.route_no))
        .collect();

    if matching.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }

    let route = &matching[0];
    let route_id = route.id.as_deref().unwrap_or("unknown");
    let direction = route.direction.unwrap_or(0);

    let route_info = rt.block_on(busmap.get_route_info(route_id, &req.region))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let gps_buses = rt.block_on(busmap.get_route_gps(&req.region, route_id, direction))
        .unwrap_or_default();

    let core_buses: Vec<Bus> = gps_buses.iter()
        .filter_map(|b| {
            let lat = b.lat?;
            let lng = b.lng?;
            Some(Bus {
                vehicle_id: b.vehicle_id.clone().unwrap_or_else(|| "unknown".to_string()),
                route_id: route.id.as_deref().unwrap_or("unknown").to_string(),
                route_no: Some(req.route_no.clone()),
                coord: tracker_core::Coord { lat, lng },
                speed_kmh: b.speed.unwrap_or(0.0),
                heading: b.heading,
                status: b.status.clone().unwrap_or_else(|| "unknown".to_string()),
                last_update: b.timestamp.unwrap_or_else(|| now_seconds()),
                distance_to_next_stop_m: None,
                current_stop_id: None,
            })
        })
        .collect();

    let stations: Vec<Station> = route_info
        .stations
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .enumerate()
        .map(|(i, s)| Station {
            id: s.id.as_deref().unwrap_or(&format!("stop_{}", i)).to_string(),
            name: s.name.as_deref().unwrap_or("Unknown").to_string(),
            coord: tracker_core::Coord { lat: s.lat.unwrap_or(21.0285), lng: s.lng.unwrap_or(105.8542) },
            region: req.region.clone(),
            provider: "busmap".to_string(),
        })
        .collect();

    let _engine = EtaEngine::new().with_data(
        vec![tracker_core::Route {
            id: route.id.as_deref().unwrap_or("unknown").to_string(),
            name: req.route_no.clone(),
            region: req.region.clone(),
            provider: "busmap".to_string(),
            stations: stations.iter().map(|s| s.id.clone()).collect(),
            polyline: route_info.polyline.as_ref().unwrap_or(&vec![]).iter()
                .map(|c| tracker_core::Coord { lat: c.lat, lng: c.lng }).collect(),
        }],
        stations.clone(),
    );

    let now = now_seconds();
    let congestion = state.congestion.lock().await;
    let traffic_mult = congestion.as_ref().map(|c| c.multiplier).unwrap_or(1.3);

    let mut board_items = Vec::new();
    for bus in &core_buses {
        let nearest_idx = find_nearest_stop_idx(bus, &stations);
        if let Some(idx) = nearest_idx {
            let stop = &stations[idx];
            let distance_m = bus.coord.haversine_meters(&stop.coord);
            let live_speed = bus.speed_kmh.max(1.0);
            let distance_km = distance_m / 1000.0;
            let base_seconds = (distance_km / live_speed as f64) * 3600.0;
            let adjusted_seconds = base_seconds * traffic_mult as f64;
            let eta_min = ((adjusted_seconds / 60.0).round() as i32).max(1);

            let gps_age = now - bus.last_update;
            let data_fresh = gps_age < 300;
            let confidence = if data_fresh { "High" } else { "Low" };

            let gps_auth = if data_fresh { "✓ fresh" } else { "✗ stale" };
            let route_auth = if is_bus_on_route(bus, &route_info) { "✓ on-route" } else { "⚠ off-route" };

            board_items.push(BusBoardItem {
                vehicle_id: bus.vehicle_id.clone(),
                next_stop: stop.name.clone(),
                eta_min,
                confidence: confidence.to_string(),
                speed_kmh: live_speed,
                auth_gps: gps_auth.to_string(),
                auth_route: route_auth.to_string(),
            });
        }
    }

    {
        let mut r = state.route_no.lock().await;
        *r = req.route_no.clone();
    }
    {
        let mut b = state.buses.lock().await;
        *b = core_buses.clone();
    }
    {
        let mut s = state.stations.lock().await;
        *s = stations.clone();
    }
    {
        let mut reg = state.current_region.lock().await;
        *reg = req.region.clone();
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "route_no": req.route_no,
        "station_count": stations.len(),
        "buses": board_items
    })))
}

async fn status_api(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let buses = state.buses.lock().await;
    let region = state.current_region.lock().await;
    let route_no = state.route_no.lock().await;

    Json(serde_json::json!({
        "provider": "busmap",
        "region": region.as_str(),
        "route_no": route_no.as_str(),
        "bus_count": buses.len()
    }))
}

// ============ Helpers ============

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn find_nearest_stop_idx(bus: &Bus, stations: &[Station]) -> Option<usize> {
    if stations.is_empty() { return None; }
    let mut nearest_idx = 0;
    let mut nearest_dist = f64::MAX;
    for (i, s) in stations.iter().enumerate() {
        let d = bus.coord.haversine_meters(&s.coord);
        if d < nearest_dist {
            nearest_dist = d;
            nearest_idx = i;
        }
    }
    Some(nearest_idx)
}

fn is_bus_on_route(bus: &Bus, route_info: &tracker_busmap::models::BusMapRouteInfoResponse) -> bool {
    if let Some(polyline) = &route_info.polyline {
        for p in polyline {
            if bus.coord.haversine_meters(&tracker_core::Coord { lat: p.lat, lng: p.lng }) < 500.0 {
                return true;
            }
        }
    }
    if let Some(stations) = &route_info.stations {
        for s in stations {
            if let (Some(lat), Some(lng)) = (s.lat, s.lng) {
                if bus.coord.haversine_meters(&tracker_core::Coord { lat, lng }) < 500.0 {
                    return true;
                }
            }
        }
    }
    false
}
