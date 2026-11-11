use tracker_core::{Coord, EtaEngine, Route, Station};

fn test_stations() -> Vec<Station> {
    vec![
        Station {
            id: "stop_a".to_string(),
            name: "Stop A".to_string(),
            coord: Coord { lat: 21.0285, lng: 105.8542 },
            region: "hn".to_string(),
            provider: "test".to_string(),
        },
        Station {
            id: "stop_b".to_string(),
            name: "Stop B".to_string(),
            coord: Coord { lat: 21.0385, lng: 105.8642 },
            region: "hn".to_string(),
            provider: "test".to_string(),
        },
        Station {
            id: "stop_c".to_string(),
            name: "Stop C".to_string(),
            coord: Coord { lat: 21.0485, lng: 105.8742 },
            region: "hn".to_string(),
            provider: "test".to_string(),
        },
    ]
}

fn test_route() -> Route {
    Route {
        id: "route_01".to_string(),
        name: "01A".to_string(),
        region: "hn".to_string(),
        provider: "test".to_string(),
        stations: vec!["stop_a".to_string(), "stop_b".to_string(), "stop_c".to_string()],
        polyline: vec![
            Coord { lat: 21.0285, lng: 105.8542 },
            Coord { lat: 21.0335, lng: 105.8592 },
            Coord { lat: 21.0385, lng: 105.8642 },
            Coord { lat: 21.0435, lng: 105.8692 },
            Coord { lat: 21.0485, lng: 105.8742 },
        ],
    }
}

fn test_bus_near_a() -> tracker_core::Bus {
    tracker_core::Bus {
        vehicle_id: "bus-001".to_string(),
        route_id: "route_01".to_string(),
        route_no: Some("01A".to_string()),
        coord: Coord { lat: 21.0300, lng: 105.8560 },
        speed_kmh: 30.0,
        heading: Some(45.0),
        status: "running".to_string(),
        last_update: chrono::Utc::now().timestamp() - 30,
        distance_to_next_stop_m: Some(500.0),
        current_stop_id: Some("stop_b".to_string()),
    }
}

#[test]
fn test_eta_engine_basic_estimate() {
    let engine = EtaEngine::new()
        .with_data(vec![test_route()], test_stations());

    let buses = vec![test_bus_near_a()];
    let congestion = None;

    let result = engine.estimate(
        "stop_a",
        "stop_c",
        "hn",
        "test",
        &buses,
        congestion.as_ref(),
    );

    assert!(result.is_some(), "ETA estimate should return Some");
    let eta = result.unwrap();
    assert_eq!(eta.origin_station, "Stop A");
    assert_eq!(eta.dest_station, "Stop C");
    assert_eq!(eta.vehicle_id.as_deref(), Some("bus-001"));
    assert!(eta.eta_min_best >= 1, "ETA should be at least 1 minute");
    assert!(eta.eta_min_low <= eta.eta_min_best);
    assert!(eta.eta_min_best <= eta.eta_min_high);
    assert!(eta.distance_m > 0.0);
    assert!(!eta.raw_details.is_empty());
}

#[test]
fn test_eta_engine_no_matching_route() {
    let engine = EtaEngine::new()
        .with_data(vec![test_route()], test_stations());

    let buses = vec![test_bus_near_a()];
    let result = engine.estimate("stop_a", "stop_z", "hn", "test", &buses, None);
    assert!(result.is_none(), "No matching route should return None");
}

#[test]
fn test_eta_engine_wrong_region() {
    let engine = EtaEngine::new()
        .with_data(vec![test_route()], test_stations());

    let buses = vec![test_bus_near_a()];
    let result = engine.estimate("stop_a", "stop_c", "hcm", "test", &buses, None);
    assert!(result.is_none(), "Wrong region should return None");
}

#[test]
fn test_eta_engine_same_origin_dest() {
    let engine = EtaEngine::new()
        .with_data(vec![test_route()], test_stations());

    let buses = vec![test_bus_near_a()];
    // origin and dest are the same - route filter requires di > oi, so no match
    let result = engine.estimate("stop_a", "stop_a", "hn", "test", &buses, None);
    assert!(result.is_none(), "Same origin/dest should return None");
}

#[test]
fn test_eta_engine_confidence_levels() {
    let engine = EtaEngine::new()
        .with_data(vec![test_route()], test_stations());

    // Fresh bus with data
    let fresh_bus = tracker_core::Bus {
        vehicle_id: "bus-fresh".to_string(),
        route_id: "route_01".to_string(),
        route_no: Some("01A".to_string()),
        coord: Coord { lat: 21.0300, lng: 105.8560 },
        speed_kmh: 30.0,
        heading: None,
        status: "running".to_string(),
        last_update: chrono::Utc::now().timestamp() - 10,
        distance_to_next_stop_m: Some(500.0),
        current_stop_id: Some("stop_b".to_string()),
    };

    let result = engine.estimate(
        "stop_a",
        "stop_c",
        "hn",
        "test",
        &[fresh_bus],
        None,
    );
    assert!(result.is_some());
    let eta = result.unwrap();
    // Fresh data should be High or Medium confidence
    assert!(
        matches!(eta.confidence, tracker_core::ConfidenceLevel::High)
            || matches!(eta.confidence, tracker_core::ConfidenceLevel::Medium)
    );
}

#[test]
fn test_eta_engine_stale_data_low_confidence() {
    let engine = EtaEngine::new()
        .with_data(vec![test_route()], test_stations());

    // Stale bus data (1 hour old)
    let stale_bus = tracker_core::Bus {
        vehicle_id: "bus-stale".to_string(),
        route_id: "route_01".to_string(),
        route_no: Some("01A".to_string()),
        coord: Coord { lat: 21.0300, lng: 105.8560 },
        speed_kmh: 30.0,
        heading: None,
        status: "running".to_string(),
        last_update: chrono::Utc::now().timestamp() - 3600,
        distance_to_next_stop_m: Some(500.0),
        current_stop_id: Some("stop_b".to_string()),
    };

    let result = engine.estimate(
        "stop_a",
        "stop_c",
        "hn",
        "test",
        &[stale_bus],
        None,
    );
    assert!(result.is_some());
    let eta = result.unwrap();
    assert!(
        matches!(eta.confidence, tracker_core::ConfidenceLevel::Low),
        "Stale data should have Low confidence, got {:?}",
        eta.confidence
    );
}

#[test]
fn test_haversine_distance() {
    let hanoi = Coord { lat: 21.0285, lng: 105.8542 };
    let hcmc = Coord { lat: 10.7626, lng: 106.6602 };
    let dist = hanoi.haversine_meters(&hcmc);
    // Hanoi to HCMC is approximately 1,160 km
    assert!(dist > 1_000_000.0, "Distance should be > 1000km");
    assert!(dist < 1_500_000.0, "Distance should be < 1500km");
}

#[test]
fn test_congestion_from_speed() {
    let free_flow = tracker_core::EtaEngine::congestion_from_speed(50.0);
    assert_eq!(free_flow.level, 1);
    assert_eq!(free_flow.label, "Thông thoáng");
    assert_eq!(free_flow.multiplier, 1.0);

    let light = tracker_core::EtaEngine::congestion_from_speed(30.0);
    assert_eq!(light.level, 2);
    assert_eq!(light.label, "Ùn nhẹ");
    assert_eq!(light.multiplier, 1.3);

    let heavy = tracker_core::EtaEngine::congestion_from_speed(15.0);
    assert_eq!(heavy.level, 3);
    assert_eq!(heavy.label, "Ùn nặng");
    assert_eq!(heavy.multiplier, 1.7);

    let jam = tracker_core::EtaEngine::congestion_from_speed(5.0);
    assert_eq!(jam.level, 4);
    assert_eq!(jam.label, "Ùn tắc");
    assert_eq!(jam.multiplier, 2.0);
}

#[test]
fn test_eta_with_congestion() {
    let engine = EtaEngine::new()
        .with_data(vec![test_route()], test_stations());

    let buses = vec![test_bus_near_a()];
    let congestion = Some(tracker_core::CongestionInfo {
        level: 4,
        label: "Ùn tắc".to_string(),
        color: "#AF52DE".to_string(),
        multiplier: 2.0,
    });

    let result = engine.estimate(
        "stop_a",
        "stop_c",
        "hn",
        "test",
        &buses,
        congestion.as_ref(),
    );
    assert!(result.is_some());
    let eta = result.unwrap();
    assert_eq!(eta.congestion_label, "Ùn tắc");
    assert_eq!(eta.congestion_color, "#AF52DE");
}

#[test]
fn test_provider_registry_list() {
    let registry = tracker_core::ProviderRegistry::new();
    let providers = registry.list();
    assert!(!providers.is_empty(), "Should have at least one provider");

    let ids: Vec<&str> = providers.iter().map(|p| p.id()).collect();
    assert!(ids.contains(&"vinbus"), "Should include vinbus");
    assert!(ids.contains(&"gsm"), "Should include gsm");
    assert!(ids.contains(&"ihanoi"), "Should include ihanoi");
}

#[test]
fn test_provider_registry_get() {
    let registry = tracker_core::ProviderRegistry::new();
    let vinbus = registry.get("vinbus");
    assert!(vinbus.is_some());
    assert_eq!(vinbus.unwrap().name(), "VinBus");

    let nonexistent = registry.get("nonexistent");
    assert!(nonexistent.is_none());
}

#[test]
fn test_provider_configs() {
    let configs = tracker_core::ProviderRegistry::provider_configs();
    assert!(!configs.is_empty());

    let vinbus = configs.iter().find(|c| c.id == "vinbus").unwrap();
    assert_eq!(vinbus.name, "VinBus");
    assert!(vinbus.regions.contains(&"hn"));
    assert!(vinbus.regions.contains(&"hcm"));
}

#[test]
fn test_speed_cache_record() {
    let mut engine = EtaEngine::new();
    engine.record_speed("route_01:stop_a:stop_c", 25.0);
    engine.record_speed("route_01:stop_a:stop_c", 35.0);

    let cache = engine.speed_cache();
    let sample = cache.samples.get("route_01:stop_a:stop_c");
    assert!(sample.is_some());
    let s = sample.unwrap();
    assert_eq!(s.sample_count, 2);
    // Running average of 25 and 35 = 30
    assert!((s.avg_speed_kmh - 30.0).abs() < 0.1);
}

#[test]
fn test_data_source_enum() {
    assert_eq!(
        tracker_core::DataSource::Live as u8,
        0
    );
    assert_eq!(
        tracker_core::DataSource::Demo as u8,
        1
    );
}
