use crate::utils::tools::{ensure_capinfos, ensure_tshark};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;
use std::process::Command;

/// A parsed endpoint/talker entry
pub struct Talker {
    pub address: String,
    pub packets: u64,
    pub bytes_raw: u64,
    pub bytes_display: String,
    pub tx_packets: u64,
    pub tx_bytes_display: String,
    pub rx_packets: u64,
    pub rx_bytes_display: String,
}

/// Parse tshark endpoints output into a list of talkers
fn parse_endpoints(output: &str) -> Vec<Talker> {
    let mut talkers = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Skip headers, separators, filter lines, empty lines
        if trimmed.is_empty()
            || trimmed.starts_with('=')
            || trimmed.starts_with('|')
            || trimmed.contains("Endpoints")
            || trimmed.starts_with("Filter")
        {
            continue;
        }

        if let Some(talker) = parse_endpoint_line(trimmed) {
            talkers.push(talker);
        }
    }

    talkers
}

/// Parse a single endpoint line
/// Format: "address  packets  bytes_val unit  tx_packets  tx_bytes_val unit  rx_packets  rx_bytes_val unit"
fn parse_endpoint_line(line: &str) -> Option<Talker> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let address = parts[0];
    if !address.contains('.') {
        return None;
    }

    // Parse groups of (count, byte_value, unit) from remaining tokens
    let remaining = &parts[1..];
    let mut frame_counts: Vec<u64> = Vec::new();
    let mut byte_values: Vec<u64> = Vec::new();
    let mut byte_displays: Vec<String> = Vec::new();
    let mut i = 0;

    while i < remaining.len() {
        if let Ok(num) = remaining[i].parse::<u64>() {
            // Check if next two tokens are byte_value + unit
            if i + 2 < remaining.len() && is_unit(remaining[i + 2]) {
                frame_counts.push(num);
                let byte_val: f64 = remaining[i + 1].parse().unwrap_or(0.0);
                let unit = remaining[i + 2];
                byte_values.push(convert_to_bytes(byte_val, unit));
                byte_displays.push(format!("{} {}", remaining[i + 1], unit));
                i += 3;
            } else if i + 1 < remaining.len() && is_unit(remaining[i + 1]) {
                let byte_val = num as f64;
                let unit = remaining[i + 1];
                byte_values.push(convert_to_bytes(byte_val, unit));
                byte_displays.push(format!("{} {}", num, unit));
                i += 2;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    let packets = frame_counts.first().copied().unwrap_or(0);
    let tx_packets = frame_counts.get(1).copied().unwrap_or(0);
    let rx_packets = frame_counts.get(2).copied().unwrap_or(0);

    Some(Talker {
        address: address.to_string(),
        packets,
        bytes_raw: byte_values.first().copied().unwrap_or(0),
        bytes_display: byte_displays.first().cloned().unwrap_or_default(),
        tx_packets,
        tx_bytes_display: byte_displays.get(1).cloned().unwrap_or_default(),
        rx_packets,
        rx_bytes_display: byte_displays.get(2).cloned().unwrap_or_default(),
    })
}

fn is_unit(s: &str) -> bool {
    matches!(s, "bytes" | "kB" | "MB" | "GB" | "TB")
}

fn convert_to_bytes(value: f64, unit: &str) -> u64 {
    let multiplier = match unit {
        "kB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "TB" => 1_000_000_000_000.0,
        _ => 1.0,
    };
    (value * multiplier) as u64
}

fn format_bps(bps: f64) -> String {
    if bps >= 1_000_000_000.0 {
        format!("{:.2} Gbps", bps / 1_000_000_000.0)
    } else if bps >= 1_000_000.0 {
        format!("{:.2} Mbps", bps / 1_000_000.0)
    } else if bps >= 1_000.0 {
        format!("{:.2} Kbps", bps / 1_000.0)
    } else {
        format!("{:.0} bps", bps)
    }
}

/// Get capture duration in seconds using capinfos
pub fn get_capture_duration(pcap_path: &Path) -> Result<f64> {
    let capinfos = ensure_capinfos()?;
    let output = Command::new(&capinfos)
        .arg("-u")
        .arg(pcap_path)
        .output()
        .context("Failed to run capinfos")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("Capture duration:") {
            if let Some(val) = line.split(':').nth(1) {
                let cleaned = val.trim().replace("seconds", "").replace(',', ".").trim().to_string();
                if let Ok(dur) = cleaned.parse::<f64>() {
                    return Ok(dur);
                }
            }
        }
    }
    Ok(0.0)
}

/// Calculate average speed string for a talker given capture duration
pub fn talker_avg_speed(bytes: u64, duration: f64) -> String {
    if duration <= 0.0 || bytes == 0 {
        return "N/A".to_string();
    }
    let bps = (bytes as f64 * 8.0) / duration;
    format_bps(bps)
}

/// Get top talkers from a PCAP file
pub fn list_top_talkers(pcap_path: &Path, limit: usize) -> Result<Vec<Talker>> {
    let tshark = ensure_tshark()?;

    let output = Command::new(&tshark)
        .arg("-r")
        .arg(pcap_path)
        .arg("-z")
        .arg("endpoints,ip")
        .arg("-q")
        .output()
        .context("Failed to run tshark")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut talkers = parse_endpoints(&stdout);
    talkers.truncate(limit);
    Ok(talkers)
}

/// CLI: list top talkers
pub fn run(file_path: &str, limit: usize) -> Result<()> {
    let pcap_path = Path::new(file_path);
    if !pcap_path.exists() {
        anyhow::bail!("File not found: {}", file_path);
    }
    if !pcap_path.is_file() {
        anyhow::bail!("Not a file: {}", file_path);
    }

    println!(
        "{} {}",
        "Analyzing:".cyan(),
        pcap_path.file_name().unwrap_or_default().to_string_lossy()
    );
    println!("{} {}", "Limit:".cyan(), limit);

    let duration = get_capture_duration(pcap_path).unwrap_or(0.0);
    if duration > 0.0 {
        println!("{} {:.2}s", "Duration:".cyan(), duration);
    }
    println!();

    let talkers = list_top_talkers(pcap_path, limit)?;

    if talkers.is_empty() {
        println!("{}", "No endpoints found.".yellow());
        return Ok(());
    }

    println!(
        "{} {} top talker(s)",
        "Found:".green(),
        talkers.len()
    );
    println!();

    println!(
        "  {:<5} {:<20} {:>10} {:>14} {:>10} {:>14} {:>10} {:>14} {:>12}",
        "#", "Address", "Packets", "Bytes", "Tx Pkts", "Tx Bytes", "Rx Pkts", "Rx Bytes", "Avg Speed"
    );
    println!("  {}", "-".repeat(115));

    for (i, t) in talkers.iter().enumerate() {
        println!(
            "  {:<5} {:<20} {:>10} {:>14} {:>10} {:>14} {:>10} {:>14} {:>12}",
            i + 1,
            t.address,
            t.packets,
            t.bytes_display,
            t.tx_packets,
            t.tx_bytes_display,
            t.rx_packets,
            t.rx_bytes_display,
            talker_avg_speed(t.bytes_raw, duration)
        );
    }

    println!();
    Ok(())
}
