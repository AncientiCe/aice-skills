use hotel_mcp::{router, state, DEFAULT_BIND};
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let bind = std::env::var("HOTEL_MCP_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let addr: SocketAddr = bind
        .parse()
        .map_err(|error| format!("invalid HOTEL_MCP_BIND '{bind}': {error}"))?;
    let state = state::new_shared();
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "hotel-mcp listening (POST /mcp, GET /mcp SSE, DELETE /mcp)");
    axum::serve(listener, app).await?;
    Ok(())
}
