pub mod config;
mod limits;
mod state;
mod ws;

use std::{net::SocketAddr, sync::Arc};

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

pub use config::{Config, ConfigError};
pub use state::AppState;

/// Builds the relay router. The caller must serve it with connection info so
/// the per-IP limiters can see the peer address.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws::ws_handler))
        .route("/healthz", get(healthz))
        .with_state(state)
}

async fn healthz(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
      "ok": true,
      "rooms": state.room_count().await,
    }))
}

/// Binds `config.bind_addr` and serves until the process is stopped.
pub async fn run(config: Config) -> std::io::Result<()> {
    let bind_addr = config.bind_addr;
    let state = AppState::new(config);
    let reaper = state.spawn_reaper();
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, "party-relay listening");
    let result = axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await;
    reaper.abort();
    result
}
