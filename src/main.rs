mod subscription;
mod parser;
mod probe;
mod output;
mod web;
mod influxdb_config;
mod influxdb;

use anyhow::Result;
use clap::Parser;
use clash_lib::{
    app::outbound::manager::OutboundManager,
    ProxyManager, 
    app::dns::SystemResolver,
    setup_default_crypto_provider,
    proxy::AnyOutboundHandler,
};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;
use tracing::{error, info};

use subscription::fetch_subscription;
use parser::parse_clash_subscription;
use probe::test_proxies_with_clash;
use output::{ProbeResult, display_results};
use web::start_web_server;

#[derive(Parser, Debug)]
#[command(name = "clashprobe")]
#[command(about = "A tool to probe Clash subscription servers for health using proper protocol validation")]
struct Args {
    /// Subscription URL to fetch and probe
    #[arg(short, long)]
    url: String,

    /// Test URL for health checking (default: http://www.gstatic.com/generate_204)
    #[arg(short, long, default_value = "http://www.gstatic.com/generate_204")]
    test_url: String,

    /// Timeout for each probe in seconds
    #[arg(short = 'T', long, default_value = "5")]
    timeout: u64,

    /// Number of concurrent probes
    #[arg(short, long, default_value = "10")]
    concurrent: usize,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Mode selector
    #[arg(short = 'm', long, default_value = "cli")]
    mode: String,

    /// Web server port
    #[arg(long, default_value = "8080")]
    web_port: u16,

    /// Probe interval in seconds (web/influxdb mode only)
    #[arg(long, default_value = "30")]
    probe_interval: u64,

    /// Grafana config path
    #[arg(long)]
    influxdb_config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .init();

    // Setup crypto provider for TLS
    setup_default_crypto_provider();

    info!("ClashProbe starting...");
    info!("Fetching subscription from: {}", args.url);

    // Fetch subscription
    let subscription_content = fetch_subscription(&args.url).await?;
    info!("Subscription fetched successfully");

    // Parse proxies from subscription using proper Clash parsing
    let proxies = parse_clash_subscription(&subscription_content)?;
    info!("Parsed {} proxies from subscription", proxies.len());

    if proxies.is_empty() {
        error!("No valid proxies found in subscription");
        return Ok(());
    }

    // Create outbound handlers from proxy configs using Clash logic
    let outbound_handlers = OutboundManager::load_plain_outbounds(proxies);
    info!("Loaded {} outbound handlers", outbound_handlers.len());

    // Initialize DNS resolver
    let dns_resolver = Arc::new(
        SystemResolver::new(false)
            .map_err(|e| anyhow::anyhow!("Failed to create DNS resolver: {}", e))?
    );

    // Initialize proxy manager for health checking
    let proxy_manager = ProxyManager::new(dns_resolver);

    match args.mode.as_str() {
        "web" => run_web_server_mode(&args, &proxy_manager, &outbound_handlers).await?,
        "cli" => run_cli_mode(&args, &proxy_manager, &outbound_handlers).await?,
        "influxdb" => run_influxdb_mode(&args, &proxy_manager, &outbound_handlers).await?,
        &_ => panic!("Unknown mode: {}", args.mode)
    }

    Ok(())
}

fn build_and_sort_probe_results(
    outbound_handlers: &[AnyOutboundHandler],
    results: &[std::io::Result<(Duration, Duration)>]
) -> Vec<ProbeResult> {
    let mut probe_results: Vec<ProbeResult> = outbound_handlers
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

async fn run_web_server_mode(
    args: &Args,
    proxy_manager: &ProxyManager,
    outbound_handlers: &[AnyOutboundHandler]
) -> Result<()> {
    info!("Starting web server mode on port {}", args.web_port);
    let app_state = start_web_server(args.web_port).await;

    let probe_interval = Duration::from_secs(args.probe_interval);
    let timeout = Duration::from_secs(args.timeout);

    info!("Starting continuous probe loop with {}s interval", args.probe_interval);

    loop {
        let start_time = Instant::now();
        let results = test_proxies_with_clash(proxy_manager, outbound_handlers, &args.test_url, timeout).await;
        let elapsed = start_time.elapsed();

        let probe_results = build_and_sort_probe_results(outbound_handlers, &results);
        let alive_count = probe_results.iter().filter(|r| r.alive).count();
        info!(
            "Probe completed in {:.2}s - {}/{} proxies alive",
            elapsed.as_secs_f64(),
            alive_count,
            probe_results.len()
        );

        app_state.update_results(probe_results).await;
        tokio::time::sleep(probe_interval).await;
    }
    // unreachable
    #[allow(unreachable_code)]
    Ok(())
}

async fn run_cli_mode(
    args: &Args,
    proxy_manager: &ProxyManager,
    outbound_handlers: &[AnyOutboundHandler]
) -> Result<()> {
    info!("Starting proxy health check with test URL: {}", args.test_url);
    let start_time = Instant::now();

    let timeout = Duration::from_secs(args.timeout);
    let results = test_proxies_with_clash(proxy_manager, outbound_handlers, &args.test_url, timeout).await;
    let elapsed = start_time.elapsed();
    info!("Proxy health check completed in {:.2}s", elapsed.as_secs_f64());

    let probe_results = build_and_sort_probe_results(outbound_handlers, &results);
    display_results(&probe_results, args.verbose);
    Ok(())
}

async fn run_influxdb_mode(
    args: &Args,
    proxy_manager: &ProxyManager,
    outbound_handlers: &[AnyOutboundHandler]
) -> Result<()> {
    let config = crate::influxdb_config::Config::load_from_file(args.influxdb_config.as_str()).unwrap();
    let probe_interval = Duration::from_secs(args.probe_interval);
    let timeout = Duration::from_secs(args.timeout);
    let influxdb_uploader = influxdb::InfluxUploader::new(&config);
    info!("Loaded InfluxDB config from: {:?}", args.influxdb_config);

    loop {
        let start_time = Instant::now();
        let results = test_proxies_with_clash(proxy_manager, outbound_handlers, &args.test_url, timeout).await;
        let elapsed = start_time.elapsed();

        let probe_results = build_and_sort_probe_results(outbound_handlers, &results);
        match influxdb_uploader.upload_results(&probe_results).await {
            Ok(_) => (),
            Err(e) => error!("Failed to upload probe results to InfluxDB: {}", e),
        }
        tokio::time::sleep(probe_interval).await;
    }

    #[allow(unreachable_code)]
    Ok(())
}