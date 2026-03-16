use crate::utils::tools::ensure_tshark;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fmt;
use std::path::Path;
use std::process::Command;

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

    // Extract numeric values from the remaining tokens, skipping unit strings
    let remaining = &parts[arrow_idx + 2..];
    let numbers: Vec<&str> = remaining
        .iter()
        .filter(|s| {
            // Keep only tokens that look like numbers (digits, dots, commas)
            s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        })
        .copied()
        .collect();

    // Expected numeric order: frames_a_b, bytes_a_b, frames_b_a, bytes_b_a, frames_total, bytes_total, rel_start, duration
    let parse_num = |idx: usize| -> u64 {
        numbers
            .get(idx)
            .and_then(|s| s.replace(',', ".").parse::<f64>().ok())
            .map(|v| v as u64)
            .unwrap_or(0)
    };

    let packets_a_to_b = parse_num(0);
    let bytes_a_to_b = parse_num(1);
    let packets_b_to_a = parse_num(2);
    let bytes_b_to_a = parse_num(3);
    let packets_total = parse_num(4);
    let bytes_total = parse_num(5);
    // index 6 = rel_start
    let duration = numbers.get(7).unwrap_or(&"0.0").replace(',', ".").to_string();

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
        "  {:<5} {:<45} {:>8} {:>12} {:>10}",
        "#", "Conversation", "Packets", "Bytes", "Duration"
    );
    println!("  {}", "-".repeat(85));

    for (i, conv) in conversations.iter().enumerate() {
        let label = conv.label();
        println!(
            "  {:<5} {:<45} {:>8} {:>12} {:>10}s",
            i + 1,
            label,
            conv.packets_total,
            conv.bytes_total,
            conv.duration
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
