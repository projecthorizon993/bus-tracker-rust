use crate::models::SpeedCache;
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppStorage {
    pub speed_cache: SpeedCache,
}

pub struct SpeedStorage {
    path: PathBuf,
}

impl SpeedStorage {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let dir = config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("bus-tracker-rust");
        fs::create_dir_all(&dir)?;
        let path = dir.join("speed_cache.json");
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<SpeedCache, Box<dyn std::error::Error>> {
        if !self.path.exists() {
            return Ok(SpeedCache::default());
        }
        let data = fs::read_to_string(&self.path)?;
        let cache: SpeedCache = serde_json::from_str(&data)?;
        Ok(cache)
    }

    pub fn save(&self, cache: &SpeedCache) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_string_pretty(cache)?;
        fs::write(&self.path, data)?;
        Ok(())
    }
}