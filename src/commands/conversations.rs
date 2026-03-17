use crate::utils::tools::ensure_tshark;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fmt;
use std::path::Path;
use std::process::Command;

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

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} kB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

/// A parsed conversation entry
#[derive(Clone)]
pub struct Conversation {
    pub protocol: String,
    pub addr_a: String,
    pub port_a: String,
    pub addr_b: String,
    pub port_b: String,
    pub packets_a_to_b: u64,
    pub bytes_a_to_b: u64,
    pub packets_b_to_a: u64,
    pub bytes_b_to_a: u64,
    pub packets_total: u64,
    pub bytes_total: u64,
    pub duration: String,
}

impl fmt::Display for Conversation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let src = if self.port_a.is_empty() {
            self.addr_a.clone()
        } else {
            format!("{}:{}", self.addr_a, self.port_a)
        };
        let dst = if self.port_b.is_empty() {
            self.addr_b.clone()
        } else {
            format!("{}:{}", self.addr_b, self.port_b)
        };
        write!(
            f,
            "{} <-> {}  | pkts: {} | bytes: {} | dur: {}",
            src, dst, self.packets_total, self.bytes_total, self.duration
        )
    }
}

impl Conversation {
    /// Build a tshark display filter for this conversation
    pub fn to_display_filter(&self) -> String {
        match self.protocol.as_str() {
            "TCP" => format!(
                "(ip.addr=={} && ip.addr=={} && tcp.port=={} && tcp.port=={})",
                self.addr_a, self.addr_b, self.port_a, self.port_b
            ),
            "UDP" => format!(
                "(ip.addr=={} && ip.addr=={} && udp.port=={} && udp.port=={})",
                self.addr_a, self.addr_b, self.port_a, self.port_b
            ),
            _ => format!(
                "(ip.addr=={} && ip.addr=={})",
                self.addr_a, self.addr_b
            ),
        }
    }

    /// A short label for identifying this conversation
    pub fn label(&self) -> String {
        let src = if self.port_a.is_empty() {
            self.addr_a.clone()
        } else {
            format!("{}:{}", self.addr_a, self.port_a)
        };
        let dst = if self.port_b.is_empty() {
            self.addr_b.clone()
        } else {
            format!("{}:{}", self.addr_b, self.port_b)
        };
        format!("[{}] {} <-> {}", self.protocol, src, dst)
    }

    /// Average speed as a formatted string (e.g., "1.23 Mbps")
    pub fn avg_speed(&self) -> String {
        let dur: f64 = self.duration.parse().unwrap_or(0.0);
        if dur <= 0.0 || self.bytes_total == 0 {
            return "N/A".to_string();
        }
        let bits_per_sec = (self.bytes_total as f64 * 8.0) / dur;
        format_bps(bits_per_sec)
    }
}

/// Parse tshark conversation statistics output
fn parse_conversations(output: &str) -> Vec<Conversation> {
    let mut conversations = Vec::new();
    let mut current_protocol = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Detect protocol section headers (e.g., "IPv4 Conversations", "TCP Conversations")
        if trimmed.contains("Conversations") {
            if trimmed.contains("TCP") {
                current_protocol = "TCP".to_string();
            } else if trimmed.contains("UDP") {
                current_protocol = "UDP".to_string();
            } else if trimmed.contains("IPv4") {
                current_protocol = "IPv4".to_string();
            }
            continue;
        }

        // Skip header/separator/filter/empty lines
        if trimmed.is_empty()
            || trimmed.starts_with("Filter")
            || trimmed.starts_with('|')
            || trimmed.starts_with('=')
            || (trimmed.contains("Frames") && trimmed.contains("Bytes"))
        {
            continue;
        }

        if current_protocol.is_empty() || !trimmed.contains("<->") {
            continue;
        }

        // Parse conversation line
        // Format: addr_a:port_a  <-> addr_b:port_b  packets  bytes  packets  bytes  packets  bytes  rel_start  duration
        if let Some(conv) = parse_conversation_line(trimmed, &current_protocol) {
            conversations.push(conv);
        }
    }

    conversations
}

/// Parse a single conversation line from tshark output
/// Format varies: "addr_a <-> addr_b  frames bytes_val unit  frames bytes_val unit  frames bytes_val unit  rel_start  duration"
/// Units can be "bytes", "kB", "MB", "GB" etc.
fn parse_conversation_line(line: &str, protocol: &str) -> Option<Conversation> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    // Find the <-> separator
    let arrow_idx = parts.iter().position(|&s| s == "<->")?;
    if arrow_idx == 0 || arrow_idx + 1 >= parts.len() {
        return None;
    }

    let left = parts[arrow_idx - 1];
    let right = parts[arrow_idx + 1];

    let (addr_a, port_a) = split_addr_port(left);
    let (addr_b, port_b) = split_addr_port(right);

    // Parse remaining tokens as groups of (frames, byte_value, unit)
    // tshark outputs: frames bytes_val unit  frames bytes_val unit  frames bytes_val unit  rel_start  duration
    let remaining = &parts[arrow_idx + 2..];
    let mut idx = 0;
    let mut frame_counts: Vec<u64> = Vec::new();
    let mut byte_counts: Vec<u64> = Vec::new();
    let mut trailing_numbers: Vec<String> = Vec::new();

    while idx < remaining.len() {
        let token = remaining[idx];
        if let Ok(num) = token.replace(',', ".").parse::<f64>() {
            // Check if this starts a frame+bytes+unit group
            if idx + 2 < remaining.len() && is_byte_unit(remaining[idx + 2]) {
                // frames, bytes_val, unit
                frame_counts.push(num as u64);
                let byte_val: f64 = remaining[idx + 1].replace(',', ".").parse().unwrap_or(0.0);
                let unit = remaining[idx + 2];
                byte_counts.push(convert_to_bytes(byte_val, unit));
                idx += 3;
            } else if idx + 1 < remaining.len() && is_byte_unit(remaining[idx + 1]) {
                // bytes_val, unit (no preceding frame count parsed yet — this is a byte column)
                let byte_val = num;
                let unit = remaining[idx + 1];
                byte_counts.push(convert_to_bytes(byte_val, unit));
                idx += 2;
            } else {
                // standalone number (rel_start or duration)
                trailing_numbers.push(token.replace(',', "."));
                idx += 1;
            }
        } else if is_byte_unit(token) {
            idx += 1;
        } else {
            idx += 1;
        }
    }

    let packets_a_to_b = frame_counts.first().copied().unwrap_or(0);
    let bytes_a_to_b = byte_counts.first().copied().unwrap_or(0);
    let packets_b_to_a = frame_counts.get(1).copied().unwrap_or(0);
    let bytes_b_to_a = byte_counts.get(1).copied().unwrap_or(0);
    let packets_total = frame_counts.get(2).copied().unwrap_or(packets_a_to_b + packets_b_to_a);
    let bytes_total = byte_counts.get(2).copied().unwrap_or(bytes_a_to_b + bytes_b_to_a);
    // trailing_numbers: rel_start, duration
    let duration = trailing_numbers.get(1).cloned().unwrap_or_else(|| "0.0".to_string());

    Some(Conversation {
        protocol: protocol.to_string(),
        addr_a: addr_a.to_string(),
        port_a: port_a.to_string(),
        addr_b: addr_b.to_string(),
        port_b: port_b.to_string(),
        packets_a_to_b,
        bytes_a_to_b,
        packets_b_to_a,
        bytes_b_to_a,
        packets_total,
        bytes_total,
        duration,
    })
}

fn is_byte_unit(s: &str) -> bool {
    matches!(s, "bytes" | "kB" | "MB" | "GB" | "TB")
}

fn convert_to_bytes(value: f64, unit: &str) -> u64 {
    let multiplier = match unit {
        "kB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "TB" => 1_000_000_000_000.0,
        _ => 1.0, // "bytes"
    };
    (value * multiplier) as u64
}

fn split_addr_port(s: &str) -> (&str, &str) {
    // For TCP/UDP: "10.0.0.1:443" -> ("10.0.0.1", "443")
    // For IP-only: "10.0.0.1" -> ("10.0.0.1", "")
    if let Some(last_colon) = s.rfind(':') {
        let port_part = &s[last_colon + 1..];
        if port_part.chars().all(|c| c.is_ascii_digit()) {
            (&s[..last_colon], port_part)
        } else {
            (s, "")
        }
    } else {
        (s, "")
    }
}

/// Get conversations from a PCAP file
pub fn list_conversations(pcap_path: &Path) -> Result<Vec<Conversation>> {
    let tshark = ensure_tshark()?;

    let output = Command::new(&tshark)
        .arg("-r")
        .arg(pcap_path)
        .arg("-z")
        .arg("conv,tcp")
        .arg("-z")
        .arg("conv,udp")
        .arg("-z")
        .arg("conv,ip")
        .arg("-q")
        .output()
        .context("Failed to run tshark")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_conversations(&stdout))
}

/// Export a conversation to a new PCAP file
pub fn export_conversation(
    pcap_path: &Path,
    conversation: &Conversation,
    output_path: &Path,
) -> Result<()> {
    let tshark = ensure_tshark()?;
    let filter = conversation.to_display_filter();

    let output = Command::new(&tshark)
        .arg("-r")
        .arg(pcap_path)
        .arg("-Y")
        .arg(&filter)
        .arg("-w")
        .arg(output_path)
        .output()
        .context("Failed to run tshark for export")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tshark export failed: {}", stderr.lines().next().unwrap_or("Unknown error"));
    }

    Ok(())
}

/// CLI: list conversations for a PCAP file
pub fn run(file_path: &str) -> Result<Vec<Conversation>> {
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
    println!();

    let conversations = list_conversations(pcap_path)?;

    if conversations.is_empty() {
        println!("{}", "No conversations found.".yellow());
        return Ok(conversations);
    }

    println!(
        "{} {} conversation(s)",
        "Found:".green(),
        conversations.len()
    );
    println!();

    // Print header
    println!(
        "  {:<5} {:<45} {:>8} {:>12} {:>10} {:>12}",
        "#", "Conversation", "Packets", "Bytes", "Duration", "Avg Speed"
    );
    println!("  {}", "-".repeat(98));

    for (i, conv) in conversations.iter().enumerate() {
        let label = conv.label();
        println!(
            "  {:<5} {:<45} {:>8} {:>12} {:>10}s {:>12}",
            i + 1,
            label,
            conv.packets_total,
            format_bytes(conv.bytes_total),
            conv.duration,
            conv.avg_speed()
        );
    }

    println!();
    Ok(conversations)
}

/// CLI: export a specific conversation by index
pub fn run_export(
    file_path: &str,
    conv_index: usize,
    output_dir: Option<&str>,
) -> Result<()> {
    let pcap_path = Path::new(file_path);
    let conversations = list_conversations(pcap_path)?;

    if conv_index == 0 || conv_index > conversations.len() {
        anyhow::bail!(
            "Invalid conversation index {}. Valid range: 1-{}",
            conv_index,
            conversations.len()
        );
    }

    let conv = &conversations[conv_index - 1];
    let out_dir = output_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| pcap_path.parent().unwrap_or(Path::new(".")).to_path_buf());

    if !out_dir.exists() {
        std::fs::create_dir_all(&out_dir)?;
    }

    let filename = format!(
        "{}_{}_{}_flow.pcap",
        pcap_path.file_stem().unwrap_or_default().to_string_lossy(),
        conv.protocol.to_lowercase(),
        conv_index
    );
    let output_path = out_dir.join(&filename);

    println!("{} {}", "Exporting:".cyan(), conv.label());
    println!("{} {}", "Filter:".cyan(), conv.to_display_filter());

    export_conversation(pcap_path, conv, &output_path)?;

    if output_path.exists() {
        let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
        println!(
            "  {} {} ({} bytes)",
            "Saved:".green(),
            output_path.display(),
            size
        );
    } else {
        println!("  {} No output created", "Error:".red());
    }

    Ok(())
}
