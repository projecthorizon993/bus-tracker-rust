use clap::{Parser, Subcommand};
use log::{info, warn};
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::Timelike;
use tracker_busmap::BusMapClient;
use tracker_core::{ConfidenceLevel, EtaEngine, Station};

#[derive(Parser)]
#[command(name = "bus-tracker")]
#[command(about = "Track any bus with station-to-station ETA estimation")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available providers
    Providers,
    /// Fetch live buses for a provider/region
    Buses {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        region: String,
    },
    /// Estimate ETA from one station to another
    Eta {
        #[arg(long)]
        provider: String,
        #[arg(long)]
        region: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Plan a trip by route number (e.g. "60B") — live buses + iHanoi congestion (free, no API key)
    Plan {
        #[arg(help = "Route number to track, e.g. 60B")]
        route_no: String,
        #[arg(help = "Region code, e.g. hn")]
        region: String,
    },
    /// Interactive mode
    Interactive,
}

fn main() {
    tracker_core::init_file_logger().expect("Failed to initialize file logger");
    info!("Starting bus-tracker CLI");

    let cli = Cli::parse();

    match cli.command {
        Commands::Providers => {
            let registry = tracker_core::ProviderRegistry::new();
            println!("Available providers:");
            for p in registry.list() {
                println!("  - {} ({})", p.name(), p.id());
            }
            println!("\nProvider configs:");
            for config in tracker_core::ProviderRegistry::provider_configs() {
                println!("  {}: regions {:?}", config.name, config.regions);
            }
        }
        Commands::Buses { provider, region } => {
            let registry = tracker_core::ProviderRegistry::new();
            let p = match registry.get(&provider) {
                Some(p) => p,
                None => {
                    eprintln!("Provider '{}' not found", provider);
                    std::process::exit(1);
                }
            };
            match p.fetch_buses(&region) {
                Ok(buses) => {
                    println!(
                        "Found {} bus(es) for {} in region {}",
                        buses.len(),
                        p.name(),
                        region
                    );
                    for b in buses.iter().take(30) {
                        println!(
                            "  {} | route {} | {:.1} km/h | {} | last: {}",
                            b.vehicle_id,
                            b.route_no.as_deref().unwrap_or("?"),
                            b.speed_kmh,
                            b.status,
                            b.last_update
                        );
                    }
                }
                Err(e) => eprintln!("Error fetching buses: {}", e),
            }
        }
        Commands::Eta { provider, region, from, to } => {
            let registry = tracker_core::ProviderRegistry::new();
            let p = match registry.get(&provider) {
                Some(p) => p,
                None => {
                    eprintln!("Provider '{}' not found", provider);
                    std::process::exit(1);
                }
            };

            let (stations, routes) = load_bundled_data(&region, &provider);
            let engine = EtaEngine::new().with_data(routes, stations);

            let congestion = p.fetch_congestion(&region).ok().flatten();

            let buses = match p.fetch_buses(&region) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error fetching buses: {}", e);
                    vec![]
                }
            };

            let estimate = engine.estimate(
                &from,
                &to,
                &region,
                &provider,
                &buses,
                congestion.as_ref(),
            );
            match estimate {
                Some(e) => {
                    println!("=== Station-to-Station ETA ===");
                    println!("From: {} → To: {}", e.origin_station, e.dest_station);
                    println!("Vehicle: {}", e.vehicle_id.as_deref().unwrap_or("unknown"));
                    println!("ETA: {}–{} min (best: {} min)", e.eta_min_low, e.eta_min_high, e.eta_min_best);
                    println!("Confidence: {:?}", e.confidence);
                    println!("Distance: {:.1} m | Avg speed: {:.1} km/h", e.distance_m, e.avg_speed_kmh);
                    println!("Congestion: {} ({})", e.congestion_label, e.congestion_color);
                    println!("Intermediate stops: {}", e.dwell_stops);
                    println!("Details: {}", e.raw_details);
                }
                None => {
                    println!("No route found from {} to {} on {}/{}", from, to, provider, region);
                }
            }
        }
        Commands::Plan { route_no, region } => {
            if let Err(e) = run_plan(&route_no, &region) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Interactive => {
            run_interactive();
        }
    }
}

/// Plan command: look up a route by number, fetch live buses, compute per-bus ETA with iHanoi congestion
fn run_plan(route_no: &str, region: &str) -> Result<(), Box<dyn std::error::Error>> {
    let device_id = std::env::var("BUSMAP_DEVICE_ID").unwrap_or_else(|_| "web-cli".to_string());

    let rt = tokio::runtime::Runtime::new()?;
    let busmap = BusMapClient::new(device_id)?;

    info!(
        "[PLAN] Looking up route {} in region {}",
        route_no, region
    );

    // Step 1: List routes and find the one matching route_no
    let routes = rt.block_on(busmap.list_routes(region))?;
    let matching: Vec<_> = routes
        .iter()
        .filter(|r| {
            r.route_no.as_deref() == Some(route_no)
                || r.route_name.as_deref() == Some(route_no)
        })
        .collect();

    if matching.is_empty() {
        eprintln!("Route '{}' not found in region '{}'", route_no, region);
        println!("\nAvailable routes (first 20):");
        for r in routes.iter().take(20) {
            println!(
                "  {} | {} | region={:?}",
                r.route_no.as_deref().unwrap_or("?"),
                r.route_name.as_deref().unwrap_or("?"),
                r.region_code
            );
        }
        return Ok(());
    }

    let route = &matching[0];
    let route_id = route.id.as_deref().unwrap_or("unknown");
    info!(
        "[PLAN] Found route {} (id={}) direction={:?}",
        route_no, route_id, route.direction
    );

    // Step 2: Get route detail with stations and polyline
    let route_info = rt.block_on(busmap.get_route_info(route_id, region))?;

    let stations: Vec<Station> = route_info
        .stations
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .enumerate()
        .map(|(i, s)| Station {
            id: s.id.as_deref().unwrap_or(&format!("stop_{}", i)).to_string(),
            name: s.name.as_deref().unwrap_or("Unknown").to_string(),
            coord: tracker_core::Coord {
                lat: s.lat.unwrap_or(21.0285),
                lng: s.lng.unwrap_or(105.8542),
            },
            region: region.to_string(),
            provider: "busmap".to_string(),
        })
        .collect();

    if stations.is_empty() {
        eprintln!("No stations found for route {}", route_no);
        return Ok(());
    }

    println!(
        "\n=== Route {} — {} stations ===\n",
        route_no,
        stations.len()
    );

    // Step 3: Get live GPS for buses on this route
    let direction = route.direction.unwrap_or(0);
    let gps_buses = match rt.block_on(busmap.get_route_gps(region, route_id, direction)) {
        Ok(buses) => buses,
        Err(e) => {
            warn!("[PLAN] Live GPS failed: {}", e);
            eprintln!("Warning: live GPS unavailable: {}", e);
            Vec::new()
        }
    };

    // Convert BusMapBus -> tracker_core::Bus
    let core_buses: Vec<tracker_core::Bus> = gps_buses
        .iter()
        .filter_map(|b| {
            let lat = b.lat?;
            let lng = b.lng?;
            Some(tracker_core::Bus {
                vehicle_id: b.vehicle_id.clone().unwrap_or_else(|| "unknown".to_string()),
                route_id: route.id.as_deref().unwrap_or("unknown").to_string(),
                route_no: Some(route_no.to_string()),
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

    if core_buses.is_empty() {
        println!("No live buses found for route {}", route_no);
        println!("(API may not be returning live positions right now)");
        return Ok(());
    }

    println!("Live buses: {}\n", core_buses.len());

    // Step 4: Get congestion from iHanoi (free, Hanoi-specific, no API key)
    let congestion = if region == "hn" {
        match tracker_core::ProviderRegistry::new()
            .get("ihanoi")
            .ok_or_else(|| "iHanoi provider not found".to_string())?
            .fetch_congestion(region)
        {
            Ok(Some(c)) => {
                info!("[PLAN] iHanoi congestion: {} (level={}, mult={})", c.label, c.level, c.multiplier);
                Some(c)
            }
            Ok(None) => {
                warn!("[PLAN] iHanoi returned no congestion data");
                None
            }
            Err(e) => {
                warn!("[PLAN] iHanoi congestion failed: {}", e);
                None
            }
        }
    } else {
        warn!("[PLAN] iHanoi congestion only available for region 'hn'");
        None
    };

    // Step 5: Build ETA engine for reference/historical
    let engine = EtaEngine::new().with_data(
        vec![tracker_core::Route {
            id: route.id.as_deref().unwrap_or("unknown").to_string(),
            name: route_no.to_string(),
            region: region.to_string(),
            provider: "busmap".to_string(),
            stations: stations.iter().map(|s| s.id.clone()).collect(),
            polyline: route_info
                .polyline
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(|c| tracker_core::Coord { lat: c.lat, lng: c.lng })
                .collect(),
        }],
        stations.clone(),
    );

    let now = now_seconds();

    // Step 6: Print ETA board — one row per physical bus
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("Route {} — Live ETA Board (region: {})", route_no, region);
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!(
        "{:<8} {:<24} {:<8} {:<10} {:<8} {:<10} {:<12}",
        "Bus", "Next Stop", "ETA", "Conf", "Speed", "Auth GPS", "Auth Route"
    );
    println!(
        "{:<8} {:<24} {:<8} {:<10} {:<8} {:<10} {:<12}",
        "────", "─────────", "───", "────", "─────", "────────", "────────"
    );

    for bus in &core_buses {
        let next_stop_idx = find_next_stop_index(bus, &stations);
        let next_stop = next_stop_idx.map(|i| &stations[i]);

        if let Some(stop) = next_stop {
            // Distance bus -> stop
            let distance_m = bus.coord.haversine_meters(&stop.coord);

            // Blended speed: live + historical
            let live_speed = bus.speed_kmh.max(1.0);
            let segment_key = format!("{}:{}:{}", route_id, "origin", stop.id);
            let hour = chrono::Utc::now().hour();
            let historical_speed = engine
                .speed_cache()
                .samples
                .get(&segment_key)
                .filter(|s| s.hour_of_day == hour)
                .map(|s| s.avg_speed_kmh)
                .unwrap_or(0.0);
            let blended_speed = if historical_speed > 0.0 {
                live_speed * 0.7 + historical_speed * 0.3
            } else {
                live_speed
            };

            // Congestion multiplier from iHanoi (or speed-based fallback)
            let traffic_mult = congestion
                .as_ref()
                .map(|c| c.multiplier)
                .unwrap_or_else(|| {
                    // Fallback: derive from live bus speed
                    match bus.speed_kmh {
                        s if s > 40.0 => 1.0,
                        s if s > 20.0 => 1.3,
                        s if s > 10.0 => 1.7,
                        _ => 2.0,
                    }
                });

            // ETA
            let distance_km = distance_m / 1000.0;
            let base_seconds = (distance_km / blended_speed as f64) * 3600.0;
            let adjusted_seconds = base_seconds * traffic_mult as f64;
            let eta_min = adjusted_seconds / 60.0;
            let eta_min = (eta_min.round() as i32).max(1);

            // Confidence
            let gps_age = now - bus.last_update;
            let data_fresh = gps_age < 300;
            let confidence = if data_fresh {
                if historical_speed > 0.0 {
                    ConfidenceLevel::High
                } else {
                    ConfidenceLevel::Medium
                }
            } else {
                ConfidenceLevel::Low
            };

            // Authenticity
            let gps_auth = if data_fresh { "✓ fresh" } else { "✗ stale" };
            let route_auth = if is_bus_on_route(bus, &route_info) {
                "✓ on-route"
            } else {
                "⚠ off-route"
            };

            println!(
                "{:<8} {:<24} {:<8} {:<10} {:<8} {:<10} {:<12}",
                bus.vehicle_id,
                stop.name,
                format!("{} min", eta_min),
                format!("{:?}", confidence),
                format!("{:.1}", blended_speed),
                gps_auth,
                route_auth
            );

            info!(
                "[PLAN] Bus {} → {}: ETA={}min conf={:?} dist={:.1}m speed={:.1}km/h traffic_mult={:.2}",
                bus.vehicle_id, stop.name, eta_min, confidence, distance_m, blended_speed, traffic_mult
            );
        } else {
            let gps_age = now - bus.last_update;
            let data_fresh = gps_age < 300;
            let gps_auth = if data_fresh { "✓ fresh" } else { "✗ stale" };

            println!(
                "{:<8} {:<24} {:<8} {:<10} {:<8} {:<10} {:<12}",
                bus.vehicle_id,
                "—",
                "—",
                "Low",
                format!("{:.1}", bus.speed_kmh),
                gps_auth,
                "—"
            );
        }
    }

    println!("═══════════════════════════════════════════════════════════════════════════════");

    if congestion.is_none() {
        println!("\nNote: iHanoi congestion data unavailable for this region or API call failed.");
    }

    Ok(())
}

/// Find the nearest stop index to the bus
fn find_next_stop_index(bus: &tracker_core::Bus, stations: &[Station]) -> Option<usize> {
    if stations.is_empty() {
        return None;
    }
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

/// Check if bus is on-route (within 500m of route polyline or a station)
fn is_bus_on_route(
    bus: &tracker_core::Bus,
    route_info: &tracker_busmap::models::BusMapRouteInfoResponse,
) -> bool {
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

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load_bundled_data(
    _region: &str,
    _provider: &str,
) -> (Vec<Station>, Vec<tracker_core::Route>) {
    (Vec::new(), Vec::new())
}

fn run_interactive() {
    let registry = tracker_core::ProviderRegistry::new();
    println!("=== Bus Tracker Interactive ===");
    println!("Providers:");
    for (i, p) in registry.list().iter().enumerate() {
        println!("  {}. {} ({})", i + 1, p.name(), p.id());
    }
    print!("\nSelect provider: ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let choice: usize = input.trim().parse().unwrap_or(0);
    if choice == 0 || choice > registry.list().len() {
        println!("Invalid choice");
        return;
    }
    let provider = registry.list()[choice - 1];
    println!("Selected: {}", provider.name());

    print!("Enter region (e.g. hn, hcm): ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut region = String::new();
    std::io::stdin().read_line(&mut region).unwrap();
    let region = region.trim();

    match provider.fetch_buses(region) {
        Ok(buses) => {
            println!("\nLive buses for {} in {}:", provider.name(), region);
            for b in buses.iter().take(20) {
                println!(
                    "  {} | {} | {:.1} km/h | {}",
                    b.vehicle_id,
                    b.route_no.as_deref().unwrap_or("?"),
                    b.speed_kmh,
                    b.status
                );
            }
            if buses.is_empty() {
                println!("  (no live bus data returned from API)");
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}
