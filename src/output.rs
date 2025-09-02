use clash_lib::proxy::AnyOutboundHandler;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub name: String,
    pub server: String,
    pub port: u16,
    pub protocol: String,
    pub alive: bool,
    pub delay_ms: Option<u64>,
    pub error: Option<String>,
}

impl ProbeResult {
    pub fn from_success(handler: &AnyOutboundHandler, delay: Duration) -> Self {
        let (server, port) = extract_server_and_port(handler);
        ProbeResult {
            name: handler.name().to_string(),
            server,
            port,
            protocol: format!("{}", handler.proto()),
            alive: true,
            delay_ms: Some(delay.as_millis() as u64),
            error: None,
        }
    }

    pub fn from_error(handler: &AnyOutboundHandler, error: &std::io::Error) -> Self {
        let (server, port) = extract_server_and_port(handler);
        ProbeResult {
            name: handler.name().to_string(),
            server,
            port,
            protocol: format!("{}", handler.proto()),
            alive: false,
            delay_ms: None,
            error: Some(error.to_string()),
        }
    }
}

pub fn display_results(results: &[ProbeResult], verbose: bool) {
    println!("\n=== ClashProbe Results ===");
    println!("{:<25} {:<20} {:<8} {:<12} {:<8} {}", 
        "Name", "Server", "Port", "Protocol", "Status", "Delay");
    println!("{}", "=".repeat(90));

    let mut alive_count = 0;
    for result in results {
        let status = if result.alive {
            alive_count += 1;
            "✓ ALIVE"
        } else {
            "✗ DEAD"
        };

        let delay_str = if let Some(delay) = result.delay_ms {
            format!("{}ms", delay)
        } else {
            "-".to_string()
        };

        println!("{:<25} {:<20} {:<8} {:<12} {:<8} {}", 
            truncate(&result.name, 24),
            truncate(&result.server, 19),
            result.port,
            truncate(&result.protocol, 11),
            status,
            delay_str
        );

        if verbose && result.error.is_some() {
            println!("    Error: {}", result.error.as_ref().unwrap());
        }
    }

    println!("\n=== Summary ===");
    println!("Total servers: {}", results.len());
    println!("Alive servers: {}", alive_count);
    println!("Dead servers: {}", results.len() - alive_count);
    println!("Success rate: {:.1}%", (alive_count as f64 / results.len() as f64) * 100.0);
}

fn extract_server_and_port(_handler: &AnyOutboundHandler) -> (String, u16) {
    // Since OutboundHandler trait doesn't expose server/port and proxy names 
    // typically don't contain this info, we'll use placeholder values.
    // The real server/port info is internal to each proxy implementation.
    ("N/A".to_string(), 0)
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut char_count = 0;
        for c in s.chars() {
            if char_count + 3 >= max_len { // Reserve 3 chars for "..."
                break;
            }
            result.push(c);
            char_count += 1;
        }
        format!("{}...", result)
    }
}