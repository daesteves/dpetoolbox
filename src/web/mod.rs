pub mod routes;
pub mod state;

use anyhow::Result;
use axum::Router;
use colored::Colorize;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

pub use state::AppState;

/// Open URL in default browser
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(url)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(url)
            .spawn();
    }
}

/// Start the web server
pub async fn serve(port: u16) -> Result<()> {
    let state = AppState::new();
    
    let app = Router::new()
        .merge(routes::create_routes())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let url = format!("http://localhost:{}", port);
    
    println!();
    println!("{}", "╔══════════════════════════════════════════════╗".cyan());
    println!("{}", "║          DPE TOOLBOX - WEB UI                ║".cyan());
    println!("{}", "╠══════════════════════════════════════════════╣".cyan());
    println!("{}", format!("║  {:<44}║", url).cyan());
    println!("{}", "║                                              ║".cyan());
    println!("{}", "║  Press Ctrl+C to stop the server             ║".cyan());
    println!("{}", "╚══════════════════════════════════════════════╝".cyan());
    println!();

    // Auto-open browser
    open_browser(&url);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

