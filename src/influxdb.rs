use futures::prelude::*;
use influxdb2::models::DataPoint;
use influxdb2::Client;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::influxdb_config::Config;
use crate::output::ProbeResult;

pub struct InfluxUploader {
    client: Client,
    bucket: String,
}

impl InfluxUploader {
    pub fn new(config: &Config) -> Self {
        let client = Client::new(
            config.influxdb.host.clone(),
            config.influxdb.org.clone(),
            config.influxdb.token.clone(),
        );

        Self {
            client,
            bucket: config.influxdb.bucket.clone(),
        }
    }

    pub async fn upload_results(
        &self,
        results: &[ProbeResult],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos() as i64;

        let mut points = Vec::new();

        for result in results {
            let point = if result.alive {
                DataPoint::builder("probe")
                    .tag("name", &result.name)
                    .tag("protocol", &result.protocol)
                    .field("alive", true)
                    .field("delay_ms", result.delay_ms.unwrap() as i64)
                    .timestamp(timestamp)
                    .build()?
            } else {
                DataPoint::builder("probe")
                    .tag("name", &result.name)
                    .tag("protocol", &result.protocol)
                    .field("alive", false)
                    .field("delay_ms", 99999)
                    .timestamp(timestamp)
                    .build()?
            };
            points.push(point);
        }

        if !points.is_empty() {
            self.client
                .write(&self.bucket, stream::iter(points))
                .await?;
        }

        Ok(())
    }
}