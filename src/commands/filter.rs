use anyhow::{Context, Result};
use colored::Colorize;
use regex::Regex;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::utils::tools::{ensure_capinfos, ensure_tshark};

/// VXLAN decode arguments for tshark
const VXLAN_DECODE_ARGS: &[&str] = &[
    "-d", "udp.port==65330,vxlan",
    "-d", "udp.port==65530,vxlan",
    "-d", "udp.port==10000,vxlan",
    "-d", "udp.port==20000,vxlan",
];

/// Statistics for filtering operation
#[derive(Default)]
struct FilterStats {
    processed: u32,
    with_packets: u32,
    empty: u32,
    deleted: u32,
    failed: u32,
}

/// Get packet count from a PCAP file using capinfos
fn get_packet_count(capinfos_path: &Path, pcap_path: &Path) -> Result<u64> {
    let output = Command::new(capinfos_path)
        .arg("-c")
        .arg(pcap_path)
        .output()
        .context("Failed to run capinfos")?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    
    // Parse "Number of packets: <count>" from output
    let re = Regex::new(r"Number of packets:\s+(\d+)")?;
    if let Some(caps) = re.captures(&output_str) {
        if let Some(count_str) = caps.get(1) {
            return Ok(count_str.as_str().parse().unwrap_or(0));
        }
    }

    Ok(0)
}

/// Run the filter command on a single PCAP file
pub fn run_single(
    file_path: &str,
    output_dir: Option<&str>,
    filter: &str,
    delete_empty: bool,
) -> Result<()> {
    let tshark = ensure_tshark()?;
    let capinfos = ensure_capinfos()?;

    let pcap_path = Path::new(file_path);
    if !pcap_path.exists() {
        anyhow::bail!("File not found: {}", file_path);
    }
    if !pcap_path.is_file() {
        anyhow::bail!("Not a file: {}", file_path);
    }

    let output_path = output_dir
        .map(Path::new)
        .unwrap_or_else(|| pcap_path.parent().unwrap_or(Path::new(".")));

    if !output_path.exists() {
        fs::create_dir_all(output_path)
            .context("Failed to create output directory")?;
    }

    let filename = pcap_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let output_file = output_path.join(format!("{}_filtered.pcap", filename));

    println!("{} {}", "Filter:".cyan(), filter);
    println!(
        "{} {}",
        "Filtering:".cyan(),
        pcap_path.file_name().unwrap_or_default().to_string_lossy()
    );

    let mut cmd = Command::new(&tshark);
    for arg in VXLAN_DECODE_ARGS {
        cmd.arg(arg);
    }
    cmd.arg("-r").arg(pcap_path);
    cmd.arg("-Y").arg(filter);
    cmd.arg("-w").arg(&output_file);

    match cmd.output() {
        Ok(output) => {
            if output_file.exists() {
                let packet_count = get_packet_count(&capinfos, &output_file).unwrap_or(0);
                let file_size = fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0);

                if packet_count == 0 {
                    if delete_empty {
                        if let Err(e) = fs::remove_file(&output_file) {
                            println!("  {} Could not delete: {}", "Warning:".yellow(), e);
                        } else {
                            println!("  {} 0 packets - {}", "Result:".yellow(), "DELETED".yellow());
                        }
                    } else {
                        println!("  {} 0 packets, {} bytes - {}", "Result:".yellow(), file_size, "KEPT".yellow());
                    }
                } else {
                    println!("  {} {} packets, {} bytes - {}", "Result:".green(), packet_count, file_size, "SAVED".green());
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    println!("  {} No output created: {}", "Error:".red(), stderr.trim());
                } else {
                    println!("  {} No output created", "Error:".red());
                }
            }
        }
        Err(e) => {
            println!("  {} {}", "Error:".red(), e);
        }
    }

    println!();
    println!("{}", "Filtering complete.".cyan());
    Ok(())
}

/// Run the filter command
pub fn run(
    input_dir: &str,
    output_dir: Option<&str>,
    filter: &str,
    delete_empty: bool,
) -> Result<()> {
    // Ensure required tools are available
    let tshark = ensure_tshark()?;
    let capinfos = ensure_capinfos()?;

    let input_path = Path::new(input_dir);
    if !input_path.exists() {
        anyhow::bail!("Input directory not found: {}", input_dir);
    }

    let output_path = output_dir
        .map(Path::new)
        .unwrap_or(input_path);

    // Create output directory if needed
    if !output_path.exists() {
        fs::create_dir_all(output_path)
            .context("Failed to create output directory")?;
    }

    // Find all PCAP files
    let pcap_files: Vec<_> = fs::read_dir(input_path)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|ext| ext.to_string_lossy().to_lowercase() == "pcap")
                .unwrap_or(false)
        })
        .collect();

    if pcap_files.is_empty() {
        println!("{}", "No PCAP files found in directory.".yellow());
        return Ok(());
    }

    println!(
        "{} {} PCAP files to filter",
        "Found:".green(),
        pcap_files.len()
    );
    println!("{} {}", "Filter:".cyan(), filter);
    println!();

    let mut stats = FilterStats::default();
    let total = pcap_files.len();

    for (i, pcap_file) in pcap_files.iter().enumerate() {
        let filename = pcap_file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let output_file = output_path.join(format!("{}_filtered.pcap", filename));

        println!(
            "[{}/{}] {} {}...",
            i + 1,
            total,
            "Filtering:".cyan(),
            pcap_file.file_name().unwrap_or_default().to_string_lossy()
        );

        // Build tshark command with VXLAN decode args
        let mut cmd = Command::new(&tshark);
        for arg in VXLAN_DECODE_ARGS {
            cmd.arg(arg);
        }
        cmd.arg("-r").arg(pcap_file);
        cmd.arg("-Y").arg(filter);
        cmd.arg("-w").arg(&output_file);

        // Run tshark (suppress stderr)
        let result = cmd.output();

        match result {
            Ok(output) => {
                if output_file.exists() {
                    stats.processed += 1;

                    // Get packet count
                    let packet_count = get_packet_count(&capinfos, &output_file).unwrap_or(0);
                    let file_size = fs::metadata(&output_file)
                        .map(|m| m.len())
                        .unwrap_or(0);

                    if packet_count == 0 {
                        stats.empty += 1;
                        if delete_empty {
                            if let Err(e) = fs::remove_file(&output_file) {
                                println!("  {} Could not delete: {}", "Warning:".yellow(), e);
                            } else {
                                stats.deleted += 1;
                                println!("  {} 0 packets - {}", "Result:".yellow(), "DELETED".yellow());
                            }
                        } else {
                            println!(
                                "  {} 0 packets, {} bytes - {}",
                                "Result:".yellow(),
                                file_size,
                                "KEPT".yellow()
                            );
                        }
                    } else {
                        stats.with_packets += 1;
                        println!(
                            "  {} {} packets, {} bytes - {}",
                            "Result:".green(),
                            packet_count,
                            file_size,
                            "SAVED".green()
                        );
                    }
                } else {
                    stats.failed += 1;
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stderr.is_empty() {
                        println!("  {} No output created: {}", "Error:".red(), stderr.trim());
                    } else {
                        println!("  {} No output created", "Error:".red());
                    }
                }
            }
            Err(e) => {
                stats.failed += 1;
                println!("  {} {}", "Error:".red(), e);
            }
        }
    }

    // Print summary
    println!();
    println!("{}", "=== Filtering Summary ===".cyan());
    println!("Total processed: {}", stats.processed);
    println!("{} {}", "Files with packets:".green(), stats.with_packets);
    println!("{} {}", "Empty files:".yellow(), stats.empty);
    if delete_empty {
        println!("{} {}", "Empty files deleted:".yellow(), stats.deleted);
    }
    if stats.failed > 0 {
        println!("{} {}", "Failed:".red(), stats.failed);
    }
    println!("{}", "Filtering complete.".cyan());

    Ok(())
}
