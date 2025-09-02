use anyhow::Result;
use reqwest;

/// Fetch subscription content from URL or file
pub async fn fetch_subscription(url: &str) -> Result<String> {
    // Handle file:// URLs for local testing
    if url.starts_with("file://") {
        let file_path = url.strip_prefix("file://").unwrap();
        let content = tokio::fs::read_to_string(file_path).await?;
        return Ok(content);
    }
    
    let client = reqwest::Client::new();
    let response = client.get(url).send().await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to fetch subscription: {}", response.status()));
    }
    
    let content = response.text().await?;
    Ok(content)
}

pub fn is_base64(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        && s.len() % 4 == 0
}