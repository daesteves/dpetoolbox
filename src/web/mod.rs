pub mod routes;
pub mod state;

use anyhow::Result;
use axum::Router;
use colored::Colorize;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

pub use state::AppState;

/// Start the web server
pub async fn serve(port: u16) -> Result<()> {
    let state = AppState::new();
    
    let app = Router::new()
        .merge(routes::create_routes())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    
    println!();
    println!("{}", "╔════════════════════════════════════════════════════════════════╗".cyan());
    println!("{}", "║                    DPE TOOLBOX WEB UI                          ║".cyan());
    println!("{}", "╠════════════════════════════════════════════════════════════════╣".cyan());
    println!("{}  http://localhost:{}                                     {}", "║".cyan(), port, "║".cyan());
    println!("{}", "║                                                                ║".cyan());
    println!("{}", "║  Press Ctrl+C to stop the server                              ║".cyan());
    println!("{}", "╚════════════════════════════════════════════════════════════════╝".cyan());
    println!();

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
