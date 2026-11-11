use tracker_core::{Bus, Coord, DemoProvider, ProviderRegistry};

fn test_bus() -> Bus {
    Bus {
        vehicle_id: "test-bus-1".to_string(),
        route_id: "route_01".to_string(),
        route_no: Some("01A".to_string()),
        coord: Coord { lat: 21.0300, lng: 105.8560 },
        speed_kmh: 30.0,
        heading: Some(45.0),
        status: "running".to_string(),
        last_update: chrono::Utc::now().timestamp(),
        distance_to_next_stop_m: Some(500.0),
        current_stop_id: Some("stop_b".to_string()),
    }
}

#[test]
fn test_demo_provider_hanoi_stations() {
    let stations = DemoProvider::hanoi_stations();
    assert_eq!(stations.len(), 5);
    assert!(stations.iter().all(|s| s.region == "hn"));
    assert!(stations.iter().all(|s| s.provider == "vinbus"));
    assert_eq!(stations[0].name, "Bến Xe Yên Nghĩa");
    assert_eq!(stations[1].name, "Bến Xe Mỹ Đình");
}

#[test]
fn test_demo_provider_hcmc_stations() {
    let stations = DemoProvider::hcmc_stations();
    assert_eq!(stations.len(), 3);
    assert!(stations.iter().all(|s| s.region == "hcm"));
    assert_eq!(stations[0].name, "Bến Xe Miền Tây");
    assert_eq!(stations[1].name, "Bến Xe Miền Đông");
    assert_eq!(stations[2].name, "Bến Xe Chợ Lớn");
}

#[test]
fn test_demo_provider_hanoi_buses() {
    let buses = DemoProvider::hanoi_buses();
    assert_eq!(buses.len(), 3);
    assert_eq!(buses[0].vehicle_id, "60B");
    assert_eq!(buses[0].route_id, "01A");
    assert_eq!(buses[0].speed_kmh, 32.0);
    assert_eq!(buses[1].vehicle_id, "45C");
    assert_eq!(buses[1].speed_kmh, 18.0);
    assert_eq!(buses[2].vehicle_id, "12A");
    assert_eq!(buses[2].status, "stopped");
    assert_eq!(buses[2].speed_kmh, 8.0);
}

#[test]
fn test_demo_provider_hcmc_buses() {
    let buses = DemoProvider::hcmc_buses();
    assert_eq!(buses.len(), 2);
    assert_eq!(buses[0].vehicle_id, "88X");
    assert_eq!(buses[0].route_id, "15");
    assert_eq!(buses[1].vehicle_id, "33T");
    assert_eq!(buses[1].route_id, "15");
}

#[test]
fn test_provider_registry_default_providers() {
    let registry = ProviderRegistry::new();
    let providers = registry.list();

    // Should have exactly 3 providers
    assert_eq!(providers.len(), 3);

    let ids: Vec<&str> = providers.iter().map(|p| p.id()).collect();
    assert!(ids.contains(&"vinbus"));
    assert!(ids.contains(&"gsm"));
    assert!(ids.contains(&"ihanoi"));

    // Check names
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    assert!(names.contains(&"VinBus"));
    assert!(names.contains(&"GreenSM / Xanh SM"));
    assert!(names.contains(&"iHanoi Traffic Cameras"));
}

#[test]
fn test_provider_registry_get_by_index() {
    let registry = ProviderRegistry::new();
    let p0 = registry.get_by_index(0);
    assert!(p0.is_some());
    assert_eq!(p0.unwrap().id(), "vinbus");

    let p_out_of_range = registry.get_by_index(999);
    assert!(p_out_of_range.is_none());
}

#[test]
fn test_bus_default_values() {
    let bus = test_bus();
    assert_eq!(bus.vehicle_id, "test-bus-1");
    assert_eq!(bus.route_id, "route_01");
    assert_eq!(bus.route_no.as_deref(), Some("01A"));
    assert!((bus.speed_kmh - 30.0).abs() < 0.001);
    assert_eq!(bus.heading, Some(45.0));
    assert_eq!(bus.status, "running");
    assert_eq!(bus.distance_to_next_stop_m, Some(500.0));
    assert_eq!(bus.current_stop_id.as_deref(), Some("stop_b"));
}
