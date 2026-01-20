use anyhow::Result;
use colored::Colorize;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Check if Escape key was pressed (non-blocking)
fn escape_pressed() -> bool {
    if event::poll(Duration::from_millis(0)).unwrap_or(false) {
        if let Ok(Event::Key(KeyEvent { code: KeyCode::Esc, .. })) = event::read() {
            return true;
        }
    }
    false
}

/// Run the tcpping command
pub fn run(target: &str, port: u16, timeout_ms: u64, interval_secs: u64) -> Result<()> {
    // Resolve the address
    let addr_str = format!("{}:{}", target, port);
    let addr: SocketAddr = addr_str
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve address: {}", target))?;

    println!(
        "{} TCP ping to {} on port {}. Press {} to stop.",
        "Starting:".cyan(),
        target,
        port,
        "Esc".yellow()
    );
    println!();

    let timeout = Duration::from_millis(timeout_ms);
    let interval = Duration::from_secs(interval_secs);

    // Enable raw mode for keyboard detection
    crossterm::terminal::enable_raw_mode()?;

    let result = run_ping_loop(&addr, timeout, interval, timeout_ms);

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    
    println!();
    println!("{}", "TCP ping stopped.".cyan());

    result
}

fn run_ping_loop(addr: &SocketAddr, timeout: Duration, interval: Duration, timeout_ms: u64) -> Result<()> {
    loop {
        // Check for Escape key
        if escape_pressed() {
            break;
        }

        let timestamp = chrono::Local::now().format("%H:%M:%S");
        let start = Instant::now();

        match TcpStream::connect_timeout(addr, timeout) {
            Ok(_stream) => {
                let elapsed = start.elapsed().as_millis();
                println!(
                    "[{}] {} Connected in {} ms\r",
                    timestamp,
                    "Success:".green(),
                    elapsed
                );
            }
            Err(e) => {
                let elapsed = start.elapsed().as_millis();
                if elapsed >= timeout_ms as u128 {
                    println!(
                        "[{}] {} after {} ms\r",
                        timestamp,
                        "Timeout".yellow(),
                        timeout_ms
                    );
                } else {
                    println!(
                        "[{}] {} {}\r",
                        timestamp,
                        "Failed:".red(),
                        e
                    );
                }
            }
        }

        // Sleep in small increments to check for Escape
        let sleep_start = Instant::now();
        while sleep_start.elapsed() < interval {
            if escape_pressed() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    Ok(())
}
