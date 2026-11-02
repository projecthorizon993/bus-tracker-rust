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
    EtaEngine, ProviderRegistry, CongestionInfo,
};

// ============ Shared App State ============

#[derive(Clone, Default)]
struct AppState {
    current_region: Arc<Mutex<String>>,
    current_provider: Arc<Mutex<String>>,
    stations: Arc<Mutex<Vec<tracker_core::Station>>>,
    buses: Arc<Mutex<Vec<tracker_core::Bus>>>,
    congestion: Arc<Mutex<Option<CongestionInfo>>>,
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

// ============ Request/Response Types ============

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

#[derive(Serialize)]
struct ProviderInfo {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct StationInfo {
    id: String,
    name: String,
    region: String,
    provider: String,
}

#[derive(Serialize)]
struct BusInfo {
    vehicle_id: String,
    route_no: String,
    speed_kmh: f32,
    status: String,
    last_update: i64,
    distance_m: Option<f32>,
}

// ============ Main ============

#[tokio::main]
async fn main() {
    let state = AppState::default();

    let app = Router::new()
        // Pages
        .route("/", get(index_handler))
        .route("/providers", get(providers_handler))
        .route("/stations", get(stations_handler))
        .route("/buses", get(buses_handler))
        .route("/eta", get(eta_handler))
        // API
        .route("/api/fetch", post(fetch_buses_api))
        .route("/api/compute-eta", post(compute_eta_api))
        .route("/api/status", get(status_api))
        .route("/api/providers", get(api_providers))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🚌 Bus Tracker Web UI running at http://localhost:3000");
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
        * { box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f5f7fa; color: #111; margin: 0; padding: 40px; }
        .container { max-width: 600px; margin: 0 auto; }
        h1 { color: #007AFF; margin-bottom: 30px; }
        .card { background: white; border-radius: 12px; padding: 24px; box-shadow: 0 4px 6px rgba(0,0,0,0.1); margin-bottom: 20px; }
        .provider-btn { display: block; width: 100%; padding: 16px; margin: 10px 0; background: #fff; border: 2px solid #e5e7eb; border-radius: 8px; text-align: left; cursor: pointer; transition: all 0.2s; font-size: 16px; font-weight: 600; text-decoration: none; color: #111; }
        .provider-btn:hover { border-color: #007AFF; background: #f0f7ff; transform: translateY(-2px); }
        .provider-btn .name { font-size: 18px; }
        .provider-btn .desc { color: #666; font-size: 14px; margin-top: 4px; display: block; }
        .provider-btn .arrow { float: right; }
        .subtitle { color: #666; margin-bottom: 24px; }
    </style>
</head>
<body>
    <div class="container">
        <h1>🚌 Bus Tracker</h1>
        <p class="subtitle">Select a provider to begin tracking buses</p>
        <div class="card">
            <h2>Choose Provider</h2>
            <div id="providers">Loading...</div>
        </div>
    </div>
    <script>
        fetch('/api/providers')
            .then(r => r.json())
            .then(providers => {
                const container = document.getElementById('providers');
                container.innerHTML = providers.map(p => `
                    <a href="/stations?provider=${p.id}" class="provider-btn">
                        <span class="name">${p.name}</span>
                        <span class="desc">Click to select stations</span>
                        <span class="arrow">→</span>
                    </a>
                `).join('');
            })
            .catch(e => { document.getElementById('providers').innerHTML = 'Error loading providers: ' + e; });
    </script>
</body>
</html>"#.to_string(),
    )
}

async fn stations_handler(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> Html<String> {
    let provider_id = params.get("provider").cloned().unwrap_or_else(|| "vinbus".to_string());
    let region = params.get("region").cloned().unwrap_or_else(|| "hn".to_string());

    // Store selection
    {
        let mut p = state.current_provider.lock().await;
        *p = provider_id.clone();
    }
    {
        let mut r = state.current_region.lock().await;
        *r = region.clone();
    }

    let stations = get_stations_for_region(&region, &provider_id);

    {
        let mut s = state.stations.lock().await;
        *s = stations.clone();
    }

    let station_rows: String = stations
        .iter()
        .map(|s| {
            format!(
                "<tr><td><input type='radio' name='origin' value='{}' onclick='selectOrigin(this)'></td><td><input type='radio' name='dest' value='{}' onclick='selectDest(this)'></td><td>{}</td></tr>",
                s.id, s.id, s.name
            )
        })
        .collect::<Vec<_>>()
        .join("");

    Html(format!(
        "<!DOCTYPE html>
<html lang='en'>
<head>
    <meta charset='UTF-8'>
    <meta name='viewport' content='width=device-width, initial-scale=1.0'>
    <title>Bus Tracker - Select Stations</title>
    <style>
        * {{ box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f5f7fa; color: #111; margin: 0; padding: 20px; }}
        .container {{ max-width: 800px; margin: 0 auto; }}
        .nav {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; }}
        .back-btn {{ background: #007AFF; color: white; border: none; padding: 10px 20px; border-radius: 6px; cursor: pointer; font-size: 14px; }}
        .back-btn:hover {{ background: #0056b3; }}
        h1 {{ color: #007AFF; }}
        .card {{ background: white; border-radius: 12px; padding: 24px; box-shadow: 0 4px 6px rgba(0,0,0,0.1); }}
        table {{ width: 100%; border-collapse: collapse; }}
        th {{ text-align: left; color: #666; padding: 12px; border-bottom: 1px solid #e5e7eb; }}
        td {{ padding: 12px; border-bottom: 1px solid #f3f4f6; }}
        tr:hover {{ background: #f9fafb; }}
        .computed {{ margin-top: 20px; padding: 16px; background: #f0f7ff; border-radius: 8px; display: none; }}
        .compute-btn {{ background: #007AFF; color: white; border: none; padding: 12px 24px; border-radius: 6px; cursor: pointer; font-size: 16px; font-weight: 600; margin-top: 20px; }}
        .compute-btn:disabled {{ background: #ccc; cursor: not-allowed; }}
        .subtitle {{ color: #666; margin-bottom: 16px; }}
        .region-badge {{ background: #e5e7eb; padding: 4px 12px; border-radius: 20px; font-size: 12px; }}
    </style>
</head>
<body>
    <div class='container'>
        <div class='nav'>
            <button class='back-btn' onclick=\"location.href='/'\">← Back</button>
            <span class='region-badge'>Region: {region} | Provider: {provider_id}</span>
        </div>
        <h1>Select Stations</h1>
        <p class='subtitle'>Pick origin and destination for ETA estimation</p>
        <div class='card'>
            <table>
                <thead>
                    <tr>
                        <th width='60'>Origin</th>
                        <th width='60'>Destination</th>
                        <th>Station Name</th>
                    </tr>
                </thead>
                <tbody>
                    {station_rows}
                </tbody>
            </table>
        </div>
        <button class='compute-btn' id='computeBtn' disabled onclick='computeEta()'>Compute ETA</button>
        <div id='result' class='computed'></div>
    </div>
    <script>
        let originId = null;
        let destId = null;
        let computeBtn = document.getElementById('computeBtn');

        function selectOrigin(radio) {{ originId = radio.value; updateBtn(); }}
        function selectDest(radio) {{ destId = radio.value; updateBtn(); }}

        function updateBtn() {{
            computeBtn.disabled = !(originId && destId && originId !== destId);
        }}

        async function computeEta() {{
            const res = await fetch('/api/compute-eta', {{
                method: 'POST',
                headers: {{ 'Content-Type': 'application/json' }},
                body: JSON.stringify({{ origin: originId, dest: destId }})
            }});
            const data = await res.json();
            const resultDiv = document.getElementById('result');
            resultDiv.style.display = 'block';
            resultDiv.innerHTML = `
                <h3>ETA Result</h3>
                <p><strong>From:</strong> ${data.origin_station} → <strong>To:</strong> ${data.dest_station}</p>
                <p><strong>ETA:</strong> ${data.eta_min_low}–${data.eta_min_high} min (best: ${data.eta_min_best} min)</p>
                <p><strong>Confidence:</strong> ${data.confidence}</p>
                <p><strong>Distance:</strong> ${(data.distance_m / 1000).toFixed(1)} km | <strong>Speed:</strong> ${data.avg_speed_kmh.toFixed(1)} km/h</p>
                <p><strong>Congestion:</strong> ${data.congestion_label}</p>
                <p><strong>Vehicle:</strong> ${data.vehicle_id || 'N/A'}</p>
                <p><strong>Details:</strong> ${data.raw_details}</p>
            `;
        }}
    </script>
</body>
</html>"
    ))
}

async fn buses_handler() -> Html<String> {
    Html("<h1>Buses</h1><p>See /api/status for live bus data</p>".to_string())
}

async fn eta_handler() -> Html<String> {
    Html("<h1>ETA</h1><p>See /api/status for ETA data</p>".to_string())
}

// ============ API Handlers ============

async fn fetch_buses_api(
    State(_state): State<AppState>,
    Json(req): Json<FetchBusesRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Create provider registry and fetch
    let registry = ProviderRegistry::new();
    let p = registry.get(&req.provider).ok_or(StatusCode::NOT_FOUND)?;

    let buses = p.fetch_buses(&req.region)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let bus_infos: Vec<BusInfo> = buses.iter().map(|b| BusInfo {
        vehicle_id: b.vehicle_id.clone(),
        route_no: b.route_no.clone().unwrap_or_default(),
        speed_kmh: b.speed_kmh,
        status: b.status.clone(),
        last_update: b.last_update,
        distance_m: b.distance_to_next_stop_m,
    }).collect();

    // Store buses in state
    {
        let mut b = _state.buses.lock().await;
        *b = buses;
    }

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

    let origin_id = &req.origin;
    let dest_id = &req.dest;

    let origin = stations.iter().find(|s| s.id == *origin_id);
    let dest = stations.iter().find(|s| s.id == *dest_id);

    if origin.is_none() || dest.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut engine = EtaEngine::new()
        .with_data(Vec::new(), stations.clone());

    let estimate = engine.estimate(
        origin_id,
        dest_id,
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
            Ok(Json(serde_json::json!({
                "success": true,
                "eta": result
            })))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn status_api(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let buses = state.buses.lock().await;
    let eta = state.eta_result.lock().await;
    let region = state.current_region.lock().await;
    let provider = state.current_provider.lock().await;

    Json(serde_json::json!({
        "provider": provider.as_str(),
        "region": region.as_str(),
        "bus_count": buses.len(),
        "eta": eta.as_ref()
    }))
}

async fn api_providers() -> Json<Vec<ProviderInfo>> {
    Json(vec![
        ProviderInfo { id: "vinbus".to_string(), name: "VinBus".to_string() },
        ProviderInfo { id: "gsm".to_string(), name: "GreenSM / Xanh SM".to_string() },
        ProviderInfo { id: "ihanoi".to_string(), name: "iHanoi Traffic Cameras".to_string() },
    ])
}

// ============ Helpers ============

fn get_stations_for_region(region: &str, provider_id: &str) -> Vec<tracker_core::Station> {
    // Try bundled data file first
    if let Ok(content) = std::fs::read_to_string("data/stations.json") {
        if let Ok(stations) = serde_json::from_str::<Vec<tracker_core::Station>>(&content) {
            return stations.into_iter()
                .filter(|s| s.region == region && s.provider == provider_id)
                .collect();
        }
    }

    // Fallback demo stations
    if region == "hn" && provider_id == "vinbus" {
        return vec![
            tracker_core::Station { id: "hn_001".to_string(), name: "Bến Xe Yên Nghĩa".to_string(), coord: tracker_core::Coord { lat: 20.9360, lng: 105.7520 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            tracker_core::Station { id: "hn_002".to_string(), name: "Bến Xe Mỹ Đình".to_string(), coord: tracker_core::Coord { lat: 21.0570, lng: 105.7760 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            tracker_core::Station { id: "hn_003".to_string(), name: "Bến Xe Gia Lâm".to_string(), coord: tracker_core::Coord { lat: 21.0450, lng: 105.8370 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            tracker_core::Station { id: "hn_004".to_string(), name: "Bến Xe Bắc Long Biên".to_string(), coord: tracker_core::Coord { lat: 21.0380, lng: 105.8570 }, region: "hn".to_string(), provider: "vinbus".to_string() },
            tracker_core::Station { id: "hn_005".to_string(), name: "Bến Xe Bắc Sơn".to_string(), coord: tracker_core::Coord { lat: 21.0200, lng: 105.8350 }, region: "hn".to_string(), provider: "vinbus".to_string() },
        ];
    }
    if region == "hcm" && provider_id == "vinbus" {
        return vec![
            tracker_core::Station { id: "hcm_001".to_string(), name: "Bến Xe Miền Tây".to_string(), coord: tracker_core::Coord { lat: 10.7560, lng: 106.6290 }, region: "hcm".to_string(), provider: "vinbus".to_string() },
            tracker_core::Station { id: "hcm_002".to_string(), name: "Bến Xe Miền Đông".to_string(), coord: tracker_core::Coord { lat: 10.8050, lng: 106.7310 }, region: "hcm".to_string(), provider: "vinbus".to_string() },
            tracker_core::Station { id: "hcm_003".to_string(), name: "Bến Xe Chợ Lớn".to_string(), coord: tracker_core::Coord { lat: 10.7560, lng: 106.6640 }, region: "hcm".to_string(), provider: "vinbus".to_string() },
        ];
    }
    vec![]
}