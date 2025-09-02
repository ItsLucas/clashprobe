use crate::output::ProbeResult;
use axum::{
    extract::State,
    response::{Html, Sse},
    routing::get,
    Json, Router,
};
use serde_json::json;
use std::{
    convert::Infallible,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{broadcast, RwLock};
use tokio_stream::{wrappers::BroadcastStream, StreamExt as _};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info};

pub type ProbeResults = Arc<RwLock<Vec<ProbeResult>>>;
pub type ProbeUpdateSender = broadcast::Sender<Vec<ProbeResult>>;

#[derive(Clone)]
pub struct AppState {
    pub results: ProbeResults,
    pub update_sender: ProbeUpdateSender,
}

impl AppState {
    pub fn new() -> Self {
        let (update_sender, _) = broadcast::channel(100);
        Self {
            results: Arc::new(RwLock::new(Vec::new())),
            update_sender,
        }
    }

    pub async fn update_results(&self, new_results: Vec<ProbeResult>) {
        {
            let mut results = self.results.write().await;
            *results = new_results.clone();
        }
        
        if let Err(e) = self.update_sender.send(new_results) {
            error!("Failed to broadcast update: {}", e);
        }
    }
}

pub async fn start_web_server(port: u16) -> AppState {
    let app_state = AppState::new();
    
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(status_handler))
        .route("/events", get(sse_handler))
        .nest_service("/static", ServeDir::new("static"))
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
        )
        .with_state(app_state.clone());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind to port");
        
    info!("Web server starting on http://localhost:{}", port);
    
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("Web server error: {}", e);
        }
    });

    app_state
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("static/index.html"))
}

async fn status_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let results = state.results.read().await;
    let alive_count = results.iter().filter(|r| r.alive).count();
    
    Json(json!({
        "timestamp": chrono::Utc::now(),
        "total": results.len(),
        "alive": alive_count,
        "dead": results.len() - alive_count,
        "success_rate": if results.is_empty() { 0.0 } else { (alive_count as f64 / results.len() as f64) * 100.0 },
        "proxies": *results
    }))
}

async fn sse_handler(State(state): State<AppState>) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let receiver = state.update_sender.subscribe();
    let stream = BroadcastStream::new(receiver)
        .filter_map(|result| match result {
            Ok(results) => {
                let alive_count = results.iter().filter(|r| r.alive).count();
                let data = json!({
                    "timestamp": chrono::Utc::now(),
                    "total": results.len(),
                    "alive": alive_count, 
                    "dead": results.len() - alive_count,
                    "success_rate": if results.is_empty() { 0.0 } else { (alive_count as f64 / results.len() as f64) * 100.0 },
                    "proxies": results
                });
                
                Some(Ok(axum::response::sse::Event::default()
                    .event("update")
                    .data(data.to_string())))
            }
            Err(e) => {
                error!("SSE broadcast error: {}", e);
                None
            }
        });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keep-alive-text"),
    )
}