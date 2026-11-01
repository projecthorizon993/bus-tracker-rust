use clap::{Parser, Subcommand};
use tracker_core::{EtaEngine, ProviderRegistry, Station, Route};

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
    /// Interactive mode
    Interactive,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Providers => {
            let registry = ProviderRegistry::new();
            println!("Available providers:");
            for p in registry.list() {
                println!("  - {} ({})", p.name(), p.id());
            }
            println!("\nProvider configs:");
            for config in ProviderRegistry::provider_configs() {
                println!("  {}: regions {:?}", config.name, config.regions);
            }
        }
        Commands::Buses { provider, region } => {
            let registry = ProviderRegistry::new();
            let p = match registry.get(&provider) {
                Some(p) => p,
                None => {
                    eprintln!("Provider '{}' not found", provider);
                    std::process::exit(1);
                }
            };
            match p.fetch_buses(&region) {
                Ok(buses) => {
                    println!("Found {} bus(es) for {} in region {}", buses.len(), p.name(), region);
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
            let registry = ProviderRegistry::new();
            let p = match registry.get(&provider) {
                Some(p) => p,
                None => {
                    eprintln!("Provider '{}' not found", provider);
                    std::process::exit(1);
                }
            };

            // Load bundled station/route data
            let (stations, routes) = load_bundled_data(&region, &provider);
            let engine = EtaEngine::new().with_data(routes, stations);

            // Fetch congestion
            let congestion = p.fetch_congestion(&region).ok().flatten();

            // Fetch buses
            let buses = match p.fetch_buses(&region) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Error fetching buses: {}", e);
                    vec![]
                }
            };

            let estimate = engine.estimate(&from, &to, &region, &provider, &buses, congestion.as_ref());
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
        Commands::Interactive => {
            run_interactive();
        }
    }
}

fn load_bundled_data(_region: &str, _provider: &str) -> (Vec<Station>, Vec<Route>) {
    // In a real app, this would load from data/stations.json
    // For now, return empty so the ETA engine works with live bus matching only
    (Vec::new(), Vec::new())
}

fn run_interactive() {
    let registry = ProviderRegistry::new();
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
                println!("  {} | {} | {:.1} km/h | {}", b.vehicle_id, b.route_no.as_deref().unwrap_or("?"), b.speed_kmh, b.status);
            }
            if buses.is_empty() {
                println!("  (no live bus data returned from API)");
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}