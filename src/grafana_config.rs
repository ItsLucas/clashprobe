use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub influxdb: InfluxDbConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InfluxDbConfig {
    pub host: String,
    pub org: String,
    pub token: String,
    pub bucket: String,
}

impl Config {
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn load_from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let influxdb = InfluxDbConfig {
            host: std::env::var("INFLUXDB_HOST").unwrap_or_else(|_| "http://localhost:8086".to_string()),
            org: std::env::var("INFLUXDB_ORG")?,
            token: std::env::var("INFLUXDB_TOKEN")?,
            bucket: std::env::var("INFLUXDB_BUCKET").unwrap_or_else(|_| "macstats".to_string()),
        };

        Ok(Config {
            influxdb,
        })
    }
}