use anyhow::Result;
use colored::Colorize;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Run the tcpping command
pub fn run(target: &str, port: u16, timeout_ms: u64, interval_secs: u64) -> Result<()> {
    // Resolve the address
    let addr_str = format!("{}:{}", target, port);
    let addr: SocketAddr = addr_str
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve address: {}", target))?;

    println!(
        "{} TCP ping to {} on port {}. Press Ctrl+C to stop.",
        "Starting:".cyan(),
        target,
        port
    );
    println!();

    let timeout = Duration::from_millis(timeout_ms);
    let interval = Duration::from_secs(interval_secs);

    loop {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        let start = Instant::now();

        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(_stream) => {
                let elapsed = start.elapsed().as_millis();
                println!(
                    "[{}] {} Connected in {} ms",
                    timestamp,
                    "Success:".green(),
                    elapsed
                );
            }
            Err(e) => {
                let elapsed = start.elapsed().as_millis();
                if elapsed >= timeout_ms as u128 {
                    println!(
                        "[{}] {} after {} ms",
                        timestamp,
                        "Timeout".yellow(),
                        timeout_ms
                    );
                } else {
                    println!(
                        "[{}] {} {}",
                        timestamp,
                        "Failed:".red(),
                        e
                    );
                }
            }
        }

        std::thread::sleep(interval);
    }
}
