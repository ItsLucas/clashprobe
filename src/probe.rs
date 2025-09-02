use clash_lib::{proxy::AnyOutboundHandler, ProxyManager};
use futures::stream::{self, StreamExt};
use std::time::Duration;

/// Test proxies using Clash's proper protocol-aware connection testing
pub async fn test_proxies_with_clash(
    proxy_manager: &ProxyManager,
    handlers: &[AnyOutboundHandler],
    test_url: &str,
    timeout: Duration,
) -> Vec<std::io::Result<(Duration, Duration)>> {
    let results = stream::iter(handlers)
        .map(|handler| async {
            proxy_manager.url_test(handler.clone(), test_url, Some(timeout)).await
        })
        .buffer_unordered(10) // Limit concurrency to avoid overwhelming
        .collect::<Vec<_>>()
        .await;
    
    results
}