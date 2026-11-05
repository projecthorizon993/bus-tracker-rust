use crate::models::*;
use chrono::Timelike;
use log::{debug, info, warn};
use std::collections::HashMap;

/// Station-to-station ETA estimation engine
pub struct EtaEngine {
    routes: HashMap<String, Route>,
    stations: HashMap<String, Station>,
    speed_cache: SpeedCache,
}

impl EtaEngine {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            stations: HashMap::new(),
            speed_cache: SpeedCache::default(),
        }
    }

    pub fn with_data(mut self, routes: Vec<Route>, stations: Vec<Station>) -> Self {
        for r in routes {
            self.routes.insert(r.id.clone(), r);
        }
        for s in stations {
            self.stations.insert(s.id.clone(), s);
        }
        self
    }

    pub fn add_route(&mut self, route: Route) {
        self.routes.insert(route.id.clone(), route);
    }

    pub fn add_station(&mut self, station: Station) {
        self.stations.insert(station.id.clone(), station);
    }

    /// Estimate ETA from origin station to destination station.
    /// Returns the best matching bus on that route and its ETA.
    pub fn estimate(
        &self,
        origin_id: &str,
        dest_id: &str,
        region: &str,
        provider: &str,
        buses: &[Bus],
        congestion: Option<&CongestionInfo>,
    ) -> Option<EtaEstimate> {
        info!(
            "[ETA] Estimating ETA: origin={} dest={} region={} provider={} buses={}",
            origin_id,
            dest_id,
            region,
            provider,
            buses.len()
        );

        let origin = self.stations.get(origin_id)?;
        let dest = self.stations.get(dest_id)?;

        debug!(
            "[ETA] Origin: {} ({}, {})",
            origin.name, origin.coord.lat, origin.coord.lng
        );
        debug!(
            "[ETA] Destination: {} ({}, {})",
            dest.name, dest.coord.lat, dest.coord.lng
        );

        // Find routes that contain both stations
        let matching_routes: Vec<&Route> = self
            .routes
            .values()
            .filter(|r| r.region == region && r.provider == provider)
            .filter(|r| {
                let oi = r.stations.iter().position(|s| s == origin_id);
                let di = r.stations.iter().position(|s| s == dest_id);
                oi.is_some() && di.is_some() && di.unwrap() > oi.unwrap()
            })
            .collect();

        if matching_routes.is_empty() {
            warn!(
                "[ETA] No matching routes found for origin={} dest={} region={} provider={}",
                origin_id, dest_id, region, provider
            );
            return None;
        }

        debug!("[ETA] Found {} matching routes", matching_routes.len());

        // Pick the route with fewest stops between origin and destination
        let route = matching_routes.iter().min_by_key(|r| {
            let oi = r.stations.iter().position(|s| s == origin_id).unwrap();
            let di = r.stations.iter().position(|s| s == dest_id).unwrap();
            di - oi
        })?;

        let oi = route.stations.iter().position(|s| s == origin_id).unwrap();
        let di = route.stations.iter().position(|s| s == dest_id).unwrap();
        let dwell_stops = (di - oi - 1) as u32; // intermediate stops

        debug!(
            "[ETA] Route: {} (stops={}, dwell_stops={})",
            route.id,
            route.stations.len(),
            dwell_stops
        );

        // Find buses on this route
        let route_buses: Vec<&Bus> = buses
            .iter()
            .filter(|b| b.route_id == route.id || b.route_no.as_deref() == Some(&route.name))
            .collect();

        debug!("[ETA] Buses on route {}: {}", route.id, route_buses.len());

        // If no bus on this specific route, try any bus in region
        let candidate_bus = if let Some(b) = route_buses.first() {
            Some(*b)
        } else {
            debug!(
                "[ETA] No bus on route {}, trying any bus in region",
                route.id
            );
            buses.first()
        };

        let candidate_bus = candidate_bus?;

        debug!(
            "[ETA] Candidate bus: {} speed={}km/h last_update={}",
            candidate_bus.vehicle_id, candidate_bus.speed_kmh, candidate_bus.last_update
        );

        // Compute along-route distance from bus to destination
        let along_route = self.compute_along_route_distance(candidate_bus, route, dest_id);

        // Determine speed
        let live_speed = candidate_bus.speed_kmh.max(1.0);
        let segment_key = format!("{}:{}:{}", route.id, origin_id, dest_id);
        let hour = chrono::Utc::now().hour();

        let historical_speed = self
            .speed_cache
            .samples
            .get(&segment_key)
            .filter(|s| s.hour_of_day == hour)
            .map(|s| s.avg_speed_kmh)
            .unwrap_or(0.0);

        if historical_speed > 0.0 {
            debug!(
                "[ETA] Historical speed for segment {} hour {}: {} km/h",
                segment_key, hour, historical_speed
            );
        } else {
            debug!(
                "[ETA] No historical speed for segment {} hour {}",
                segment_key, hour
            );
        }

        // Blend: 70% live, 30% historical if available
        let blended_speed = if historical_speed > 0.0 {
            live_speed * 0.7 + historical_speed * 0.3
        } else {
            live_speed
        };

        // Congestion multiplier
        let default_congestion = CongestionInfo {
            level: 2,
            label: "Unknown".to_string(),
            color: "#8E8E93".to_string(),
            multiplier: 1.3,
        };
        let congestion = congestion.unwrap_or(&default_congestion);
        debug!(
            "[ETA] Congestion: {} (level={}, multiplier={})",
            congestion.label, congestion.level, congestion.multiplier
        );

        // Base seconds = distance_km / speed_kmh * 3600
        let distance_km = along_route.bus_to_dest_meters / 1000.0;
        let base_seconds = if blended_speed > 0.0 {
            (distance_km / blended_speed as f64) * 3600.0
        } else {
            distance_km * 60.0 // fallback: 1 min per km
        };

        // Apply congestion + dwell (30 seconds per intermediate stop)
        let adjusted_seconds =
            base_seconds * congestion.multiplier as f64 + (dwell_stops as f64 * 30.0);

        let eta_min = (adjusted_seconds / 60.0).round() as i32;
        let eta_min = eta_min.max(1);

        // Confidence
        let now = chrono::Utc::now().timestamp();
        let data_fresh = candidate_bus.last_update > 0 && now - candidate_bus.last_update < 300;
        let confidence = if data_fresh {
            if historical_speed > 0.0 {
                ConfidenceLevel::High
            } else {
                ConfidenceLevel::Medium
            }
        } else {
            ConfidenceLevel::Low
        };

        debug!(
            "[ETA] Data fresh={}, confidence={:?}, data_age_seconds={}",
            data_fresh,
            confidence,
            if candidate_bus.last_update > 0 {
                now - candidate_bus.last_update
            } else {
                -1
            }
        );

        // Range: +/- 20%
        let eta_low = (eta_min as f32 * 0.8).round() as i32;
        let eta_high = (eta_min as f32 * 1.25).round() as i32;

        info!(
            "[ETA] ETA result: {}–{} min (best={}) confidence={:?} distance={:.1}m speed={:.1}km/h",
            eta_low, eta_high, eta_min, confidence, along_route.bus_to_dest_meters, blended_speed
        );

        Some(EtaEstimate {
            origin_station: origin.name.clone(),
            dest_station: dest.name.clone(),
            vehicle_id: Some(candidate_bus.vehicle_id.clone()),
            eta_min_low: eta_low.max(1),
            eta_min_high: eta_high,
            eta_min_best: eta_min,
            confidence,
            distance_m: along_route.bus_to_dest_meters,
            avg_speed_kmh: blended_speed,
            congestion_label: congestion.label.clone(),
            congestion_color: congestion.color.clone(),
            dwell_stops,
            last_update: candidate_bus.last_update,
            raw_details: format!(
                "Route: {} | Bus: {} | Speed: {:.1} km/h | Distance: {:.1} km | Stops: {} | Congestion: {}",
                route.id, candidate_bus.vehicle_id, blended_speed, distance_km, dwell_stops, congestion.label
            ),
        })
    }

    /// Compute distance along the route polyline from bus position to destination station.
    /// Falls back to straight-line haversine if polyline is empty.
    fn compute_along_route_distance(
        &self,
        bus: &Bus,
        route: &Route,
        dest_id: &str,
    ) -> AlongRouteInfo {
        let dest = match self.stations.get(dest_id) {
            Some(d) => d,
            None => {
                return AlongRouteInfo {
                    bus_to_dest_meters: bus.coord.haversine_meters(&Coord { lat: 0.0, lng: 0.0 }),
                    stations_between: 0,
                    nearest_route_segment: None,
                }
            }
        };

        let di = route
            .stations
            .iter()
            .position(|s| s == dest_id)
            .unwrap_or(0);
        let stations_between = di as u32;

        if route.polyline.is_empty() {
            // Fallback: straight-line
            return AlongRouteInfo {
                bus_to_dest_meters: bus.coord.haversine_meters(&dest.coord),
                stations_between,
                nearest_route_segment: None,
            };
        }

        // Find nearest point on polyline to bus
        let mut nearest_idx = 0usize;
        let mut nearest_dist = f64::MAX;
        for (i, p) in route.polyline.iter().enumerate() {
            let d = bus.coord.haversine_meters(p);
            if d < nearest_dist {
                nearest_dist = d;
                nearest_idx = i;
            }
        }

        // Find polyline index closest to destination station
        let mut dest_idx = 0usize;
        let mut dest_dist = f64::MAX;
        for (i, p) in route.polyline.iter().enumerate() {
            let d = dest.coord.haversine_meters(p);
            if d < dest_dist {
                dest_dist = d;
                dest_idx = i;
            }
        }

        // Sum segment distances along polyline from nearest to destination
        let mut total_m = 0.0;
        let start = nearest_idx.min(dest_idx);
        let end = nearest_idx.max(dest_idx);
        for i in start..end {
            if i + 1 < route.polyline.len() {
                total_m += route.polyline[i].haversine_meters(&route.polyline[i + 1]);
            }
        }

        AlongRouteInfo {
            bus_to_dest_meters: total_m,
            stations_between,
            nearest_route_segment: Some((nearest_idx, dest_idx)),
        }
    }

    /// Record a historical speed sample for a route segment
    pub fn record_speed(&mut self, segment_key: &str, speed_kmh: f32) {
        let hour = chrono::Utc::now().hour();
        let entry = self
            .speed_cache
            .samples
            .entry(segment_key.to_string())
            .or_insert(SpeedSample {
                segment_key: segment_key.to_string(),
                hour_of_day: hour,
                avg_speed_kmh: speed_kmh,
                sample_count: 1,
                last_updated: chrono::Utc::now().timestamp(),
            });
        // Running average
        let n = entry.sample_count as f32;
        entry.avg_speed_kmh = (entry.avg_speed_kmh * n + speed_kmh) / (n + 1.0);
        entry.sample_count += 1;
        entry.last_updated = chrono::Utc::now().timestamp();
    }

    /// Public read-only access to the speed cache (for historical speed lookups)
    pub fn speed_cache(&self) -> &SpeedCache {
        &self.speed_cache
    }

    pub fn congestion_from_speed(speed_kmh: f32) -> CongestionInfo {
        if speed_kmh > 40.0 {
            CongestionInfo {
                level: 1,
                label: "Thông thoáng".to_string(),
                color: "#34C759".to_string(),
                multiplier: 1.0,
            }
        } else if speed_kmh > 20.0 {
            CongestionInfo {
                level: 2,
                label: "Ùn nhẹ".to_string(),
                color: "#FF9500".to_string(),
                multiplier: 1.3,
            }
        } else if speed_kmh > 10.0 {
            CongestionInfo {
                level: 3,
                label: "Ùn nặng".to_string(),
                color: "#FF3B30".to_string(),
                multiplier: 1.7,
            }
        } else {
            CongestionInfo {
                level: 4,
                label: "Ùn tắc".to_string(),
                color: "#AF52DE".to_string(),
                multiplier: 2.0,
            }
        }
    }
}

impl Default for EtaEngine {
    fn default() -> Self {
        Self::new()
    }
}
