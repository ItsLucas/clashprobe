mod config;
mod influxdb;
mod parser;
mod probe_engine;
mod probe_result;
mod reporter;
mod subscription;
mod web;

use anyhow::Result;
use clap::Parser;
use clash_lib::{
    ProxyManager, app::dns::SystemResolver, app::outbound::manager::OutboundManager,
    setup_default_crypto_provider,
};
use std::sync::Arc;
use tracing::{error, info};

use config::WorkMode;
use influxdb::InfluxDbReporter;
use parser::parse_clash_subscription;
use probe_engine::ProbeEngine;
use subscription::fetch_subscription;
use web::{WebReporter, start_web_server};

#[derive(Parser, Debug)]
#[command(name = "clashprobe")]
#[command(
    about = "A tool to probe Clash subscription servers for health using proper protocol validation"
)]
struct Args {
    /// Config path
    #[arg(long, default_value = "config.toml")]
    config: String,

    /// Generate config
    #[arg(long, default_value = "false")]
    generate_config: bool,

    /// Node name override (overrides config file setting)
    #[arg(long)]
    node_name: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.generate_config {
        let default_toml = config::Config::generate_default_toml();
        // Write to config.toml
        std::fs::write(args.config, default_toml)?;
        return Ok(());
    }

    let mut config = crate::config::Config::load_from_file(args.config.as_str()).unwrap();

    // Override node name from CLI if provided
    if let Some(node_name) = args.node_name {
        config.influxdb.node_name = node_name;
    }

    // Initialize logging
    let level = if config.main.verbose {
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
    info!(
        "Fetching subscription from: {}",
        config.main.subscription_url
    );

    // Fetch subscription
    let subscription_content = fetch_subscription(&config.main.subscription_url).await?;
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
            .map_err(|e| anyhow::anyhow!("Failed to create DNS resolver: {}", e))?,
    );

    // Initialize proxy manager for health checking
    let proxy_manager = ProxyManager::new(dns_resolver);

    config
        .main
        .work_mode
        .validate()
        .map_err(|e| anyhow::anyhow!("Invalid work mode configuration: {}", e))?;

    let mut engine = ProbeEngine::new(config.clone(), proxy_manager, outbound_handlers);

    if config.main.work_mode.contains(WorkMode::WEB) {
        let app_state = Arc::new(start_web_server(config.web.port).await);
        engine.register_reporter(Box::new(WebReporter::new(app_state)));
    }

    if config.main.work_mode.contains(WorkMode::INFLUXDB) {
        engine.register_reporter(Box::new(InfluxDbReporter::new(&config)));
    }

    if config.main.work_mode.contains(WorkMode::TELOXIDE) {
        // TODO: Implement Teloxide reporter
        error!("Teloxide mode not implemented yet");
    }

    engine.run().await?;

    Ok(())
}
