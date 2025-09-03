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

fn extract_server_and_port(_handler: &AnyOutboundHandler) -> (String, u16) {
    // Since OutboundHandler trait doesn't expose server/port and proxy names
    // typically don't contain this info, we'll use placeholder values.
    // The real server/port info is internal to each proxy implementation.
    ("N/A".to_string(), 0)
}
