use anyhow::Result;
use base64::{prelude::BASE64_STANDARD, Engine};
use clash_lib::config::internal::proxy::OutboundProxyProtocol;
use serde_json;
use std::collections::HashMap;
use tracing::{debug, warn};
use url;
use urlencoding;

use crate::subscription::is_base64;

/// Parse Clash subscription content properly using clash-lib structures
pub fn parse_clash_subscription(content: &str) -> Result<Vec<OutboundProxyProtocol>> {
    // Try to decode base64 if needed
    let decoded_content = if is_base64(content) {
        match BASE64_STANDARD.decode(content.trim()) {
            Ok(decoded) => String::from_utf8(decoded)?,
            Err(_) => content.to_string(),
        }
    } else {
        content.to_string()
    };

    // First try to parse as YAML (Clash config format)
    if let Ok(clash_config) = serde_yaml::from_str::<serde_yaml::Value>(&decoded_content) {
        if let Some(proxies) = clash_config.get("proxies").and_then(|p| p.as_sequence()) {
            let mut outbound_proxies = Vec::new();
            for proxy_value in proxies {
                if let Ok(proxy) = parse_clash_proxy_from_yaml(proxy_value) {
                    outbound_proxies.push(proxy);
                }
            }
            if !outbound_proxies.is_empty() {
                return Ok(outbound_proxies);
            }
        }
    }

    // Fall back to parsing URLs line by line (subscription format)
    let mut proxies = Vec::new();
    for line in decoded_content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        if let Ok(proxy) = parse_proxy_url_to_clash_config(line) {
            proxies.push(proxy);
        } else {
            debug!("Failed to parse proxy URL: {}", line);
        }
    }
    
    Ok(proxies)
}

fn parse_clash_proxy_from_yaml(value: &serde_yaml::Value) -> Result<OutboundProxyProtocol> {
    // Convert YAML value to a HashMap for easier processing
    let map = value.as_mapping()
        .ok_or_else(|| anyhow::anyhow!("Proxy config must be a map"))?;
    
    let mut config_map = HashMap::new();
    for (k, v) in map {
        if let Some(key_str) = k.as_str() {
            config_map.insert(key_str.to_string(), v.clone());
        }
    }
    
    OutboundProxyProtocol::try_from(config_map)
        .map_err(|e| anyhow::anyhow!("Failed to parse proxy config: {}", e))
}

fn parse_proxy_url_to_clash_config(url: &str) -> Result<OutboundProxyProtocol> {
    let parsed_url = url::Url::parse(url)?;
    
    let protocol = parsed_url.scheme();
    
    // For VMess, the "host" is actually the base64-encoded config
    let (server, port, name, vmess_config) = if protocol == "vmess" {
        let base64_config = parsed_url.host_str()
            .ok_or_else(|| anyhow::anyhow!("VMess URL missing base64 config"))?;
        
        let decoded = BASE64_STANDARD.decode(base64_config)
            .map_err(|e| anyhow::anyhow!("Failed to decode VMess base64: {}", e))?;
        
        let json_str = String::from_utf8(decoded)
            .map_err(|e| anyhow::anyhow!("VMess config is not valid UTF8: {}", e))?;
        
        let vmess_json: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse VMess JSON: {}", e))?;
        
        let server = vmess_json.get("add")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("VMess config missing server address"))?
            .to_string();
        
        let port = vmess_json.get("port")
            .and_then(|v| v.as_str().and_then(|s| s.parse::<u16>().ok()))
            .unwrap_or(10086);
        
        let name = vmess_json.get("ps")
            .and_then(|v| v.as_str())
            .unwrap_or(&format!("{}:{}", server, port))
            .to_string();
        
        (server, port, name, Some(vmess_json))
    } else {
        let server = parsed_url.host_str()
            .ok_or_else(|| anyhow::anyhow!("No host in URL"))?
            .to_string();
        let port = parsed_url.port().unwrap_or(match protocol {
            "ss" => 8388,
            "trojan" => 443,
            "vless" => 443,
            "socks5" => 1080,
            _ => 8080,
        });
        let name = parsed_url.fragment()
            .map(|s| urlencoding::decode(s).unwrap_or_else(|_| s.into()).to_string())
            .unwrap_or_else(|| format!("{}:{}", server, port));
        (server, port, name, None)
    };

    // Build configuration map for clash-lib
    let mut config = HashMap::new();
    config.insert("name".to_string(), serde_yaml::Value::String(name));
    config.insert("server".to_string(), serde_yaml::Value::String(server.to_string()));
    config.insert("port".to_string(), serde_yaml::Value::Number(port.into()));
    config.insert("type".to_string(), serde_yaml::Value::String(protocol.to_string()));

    match protocol {
        "ss" => {
            // For Shadowsocks, decode user info for method and password
            if !parsed_url.username().is_empty() {
                let user_info = if let Some(password) = parsed_url.password() {
                    format!("{}:{}", parsed_url.username(), password)
                } else {
                    // Try to decode base64 user info
                    match BASE64_STANDARD.decode(parsed_url.username()) {
                        Ok(decoded) => String::from_utf8(decoded)
                            .unwrap_or_else(|_| parsed_url.username().to_string()),
                        Err(_) => parsed_url.username().to_string(),
                    }
                };
                
                if let Some((cipher, password)) = user_info.split_once(':') {
                    config.insert("cipher".to_string(), serde_yaml::Value::String(cipher.to_string()));
                    config.insert("password".to_string(), serde_yaml::Value::String(password.to_string()));
                }
            }
        }
        "trojan" => {
            if !parsed_url.username().is_empty() {
                config.insert("password".to_string(), 
                    serde_yaml::Value::String(parsed_url.username().to_string()));
            }
        }
        "socks5" => {
            if !parsed_url.username().is_empty() {
                config.insert("username".to_string(), 
                    serde_yaml::Value::String(parsed_url.username().to_string()));
                if let Some(password) = parsed_url.password() {
                    config.insert("password".to_string(), 
                        serde_yaml::Value::String(password.to_string()));
                }
            }
        }
        "vmess" => {
            if let Some(vmess_json) = vmess_config {
                // Extract UUID
                if let Some(uuid) = vmess_json.get("id").and_then(|v| v.as_str()) {
                    config.insert("uuid".to_string(), serde_yaml::Value::String(uuid.to_string()));
                }
                
                // Extract AlterID
                let alter_id = vmess_json.get("aid")
                    .and_then(|v| v.as_str().and_then(|s| s.parse::<u16>().ok()))
                    .unwrap_or(0);
                config.insert("alterId".to_string(), serde_yaml::Value::Number(alter_id.into()));
                
                // Extract cipher/security
                if let Some(security) = vmess_json.get("scy").and_then(|v| v.as_str()) {
                    if security != "auto" {
                        config.insert("cipher".to_string(), serde_yaml::Value::String(security.to_string()));
                    }
                }
                
                // Extract network type and transport options
                if let Some(network) = vmess_json.get("net").and_then(|v| v.as_str()) {
                    if network != "tcp" {
                        config.insert("network".to_string(), serde_yaml::Value::String(network.to_string()));
                        
                        match network {
                            "ws" => {
                                let mut ws_opts = serde_yaml::Mapping::new();
                                if let Some(path) = vmess_json.get("path").and_then(|v| v.as_str()) {
                                    if !path.is_empty() {
                                        ws_opts.insert(
                                            serde_yaml::Value::String("path".to_string()), 
                                            serde_yaml::Value::String(path.to_string())
                                        );
                                    }
                                }
                                if let Some(host) = vmess_json.get("host").and_then(|v| v.as_str()) {
                                    if !host.is_empty() {
                                        let mut headers = serde_yaml::Mapping::new();
                                        headers.insert(
                                            serde_yaml::Value::String("Host".to_string()), 
                                            serde_yaml::Value::String(host.to_string())
                                        );
                                        ws_opts.insert(
                                            serde_yaml::Value::String("headers".to_string()), 
                                            serde_yaml::Value::Mapping(headers)
                                        );
                                    }
                                }
                                if !ws_opts.is_empty() {
                                    config.insert("ws-opts".to_string(), serde_yaml::Value::Mapping(ws_opts));
                                }
                            }
                            "grpc" => {
                                let mut grpc_opts = serde_yaml::Mapping::new();
                                if let Some(path) = vmess_json.get("path").and_then(|v| v.as_str()) {
                                    if !path.is_empty() {
                                        grpc_opts.insert(
                                            serde_yaml::Value::String("grpc-service-name".to_string()), 
                                            serde_yaml::Value::String(path.to_string())
                                        );
                                    }
                                }
                                if !grpc_opts.is_empty() {
                                    config.insert("grpc-opts".to_string(), serde_yaml::Value::Mapping(grpc_opts));
                                }
                            }
                            "h2" => {
                                let mut h2_opts = serde_yaml::Mapping::new();
                                if let Some(path) = vmess_json.get("path").and_then(|v| v.as_str()) {
                                    if !path.is_empty() {
                                        h2_opts.insert(
                                            serde_yaml::Value::String("path".to_string()), 
                                            serde_yaml::Value::String(path.to_string())
                                        );
                                    }
                                }
                                if let Some(host) = vmess_json.get("host").and_then(|v| v.as_str()) {
                                    if !host.is_empty() {
                                        h2_opts.insert(
                                            serde_yaml::Value::String("host".to_string()), 
                                            serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(host.to_string())])
                                        );
                                    }
                                }
                                if !h2_opts.is_empty() {
                                    config.insert("h2-opts".to_string(), serde_yaml::Value::Mapping(h2_opts));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                
                // Extract TLS settings
                if let Some(tls) = vmess_json.get("tls").and_then(|v| v.as_str()) {
                    if tls == "tls" {
                        config.insert("tls".to_string(), serde_yaml::Value::Bool(true));
                        
                        if let Some(sni) = vmess_json.get("sni").and_then(|v| v.as_str()) {
                            if !sni.is_empty() {
                                config.insert("servername".to_string(), serde_yaml::Value::String(sni.to_string()));
                            }
                        }
                        
                        config.insert("skip-cert-verify".to_string(), serde_yaml::Value::Bool(true));
                    }
                }
            }
        }
        _ => {
            warn!("Unsupported protocol: {}", protocol);
            return Err(anyhow::anyhow!("Unsupported protocol: {}", protocol));
        }
    }

    debug!("Parsed proxy config: {:?}", config);
    
    OutboundProxyProtocol::try_from(config)
        .map_err(|e| anyhow::anyhow!("Failed to create proxy config: {}", e))
}