use crate::config::Config;
use crate::probe_result::ProbeResult;
use crate::reporter::ProbeReporter;
use anyhow::Result;
use clash_lib::{ProxyManager, proxy::AnyOutboundHandler};
use futures::stream::{self, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;
use tracing::{error, info};

pub struct ProbeEngine {
    config: Arc<Config>,
    proxy_manager: Arc<ProxyManager>,
    outbound_handlers: Arc<Vec<AnyOutboundHandler>>,
    reporters: Vec<Box<dyn ProbeReporter>>,
}

impl ProbeEngine {
    pub fn new(
        config: Config,
        proxy_manager: ProxyManager,
        outbound_handlers: Vec<AnyOutboundHandler>,
    ) -> Self {
        Self {
            config: Arc::new(config),
            proxy_manager: Arc::new(proxy_manager),
            outbound_handlers: Arc::new(outbound_handlers),
            reporters: Vec::new(),
        }
    }

    pub fn register_reporter(&mut self, reporter: Box<dyn ProbeReporter>) -> &mut Self {
        self.reporters.push(reporter);
        self
    }

    pub async fn run(&self) -> Result<()> {
        if self.reporters.is_empty() {
            return Err(anyhow::anyhow!("No reporters registered"));
        }

        let is_continuous = self.has_continuous_reporters();

        if is_continuous {
            self.run_continuous().await
        } else {
            self.run_once().await
        }
    }

    async fn test_proxies_with_clash(
        proxy_manager: &ProxyManager,
        handlers: &[AnyOutboundHandler],
        test_url: &str,
        timeout: Duration,
    ) -> Vec<std::io::Result<(Duration, Duration)>> {
        let results = stream::iter(handlers)
            .map(|handler| async {
                proxy_manager
                    .url_test(handler.clone(), test_url, Some(timeout))
                    .await
            })
            .buffer_unordered(10) // Limit concurrency to avoid overwhelming
            .collect::<Vec<_>>()
            .await;

        results
    }

    async fn run_once(&self) -> Result<()> {
        info!("Starting single probe run");
        let results = self.execute_probe().await?;
        self.notify_reporters(&results).await?;
        Ok(())
    }

    async fn run_continuous(&self) -> Result<()> {
        let probe_interval = Duration::from_secs(self.config.main.probe_interval);
        info!(
            "Starting continuous probe loop with {}s interval",
            self.config.main.probe_interval
        );

        loop {
            let results = self.execute_probe().await?;
            self.notify_reporters(&results).await?;
            tokio::time::sleep(probe_interval).await;
        }
    }

    async fn execute_probe(&self) -> Result<Vec<ProbeResult>> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.config.main.timeout);

        let results = Self::test_proxies_with_clash(
            &self.proxy_manager,
            &self.outbound_handlers,
            &self.config.main.test_url,
            timeout,
        )
        .await;

        let elapsed = start_time.elapsed();
        let probe_results = self.build_and_sort_probe_results(&results);

        let alive_count = probe_results.iter().filter(|r| r.alive).count();
        info!(
            "Probe completed in {:.2}s - {}/{} proxies alive",
            elapsed.as_secs_f64(),
            alive_count,
            probe_results.len()
        );

        Ok(probe_results)
    }

    async fn notify_reporters(&self, results: &[ProbeResult]) -> Result<()> {
        for reporter in &self.reporters {
            if let Err(e) = reporter.report(results).await {
                error!("Reporter '{}' failed: {}", reporter.name(), e);
            }
        }
        Ok(())
    }

    fn has_continuous_reporters(&self) -> bool {
        self.reporters.iter().any(|r| r.is_continuous())
    }

    fn build_and_sort_probe_results(
        &self,
        results: &[std::io::Result<(Duration, Duration)>],
    ) -> Vec<ProbeResult> {
        let mut probe_results: Vec<ProbeResult> = self
            .outbound_handlers
            .iter()
            .zip(results.iter())
            .map(|(handler, result)| match result {
                Ok((delay, _)) => ProbeResult::from_success(handler, *delay),
                Err(e) => ProbeResult::from_error(handler, e),
            })
            .collect();

        probe_results.sort_by(|a, b| match (a.alive, b.alive) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => a.delay_ms.cmp(&b.delay_ms),
            (false, false) => a.name.cmp(&b.name),
        });

        probe_results
    }
}
