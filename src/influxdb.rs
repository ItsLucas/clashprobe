use futures::prelude::*;
use influxdb2::Client;
use influxdb2::models::DataPoint;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::probe_result::ProbeResult;
use crate::reporter::ProbeReporter;
use anyhow::Result;
use async_trait::async_trait;

pub struct InfluxUploader {
    client: Client,
    bucket: String,
    node_name: String,
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
            node_name: config.influxdb.node_name.clone(),
        }
    }

    pub async fn upload_results(
        &self,
        results: &[ProbeResult],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as i64;

        let mut points = Vec::new();

        for result in results {
            let point = if result.alive {
                DataPoint::builder("probe")
                    .tag("name", &result.name)
                    .tag("protocol", &result.protocol)
                    .tag("node", &self.node_name)
                    .field("alive", true)
                    .field("delay_ms", result.delay_ms.unwrap() as i64)
                    .timestamp(timestamp)
                    .build()?
            } else {
                DataPoint::builder("probe")
                    .tag("name", &result.name)
                    .tag("protocol", &result.protocol)
                    .tag("node", &self.node_name)
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

pub struct InfluxDbReporter {
    uploader: InfluxUploader,
}

impl InfluxDbReporter {
    pub fn new(config: &Config) -> Self {
        Self {
            uploader: InfluxUploader::new(config),
        }
    }
}

#[async_trait]
impl ProbeReporter for InfluxDbReporter {
    async fn report(&self, results: &[ProbeResult]) -> Result<()> {
        self.uploader
            .upload_results(results)
            .await
            .map_err(|e| anyhow::anyhow!("InfluxDB upload failed: {}", e))
    }

    fn name(&self) -> &str {
        "InfluxDB"
    }
}
