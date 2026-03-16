use crate::utils::tools::{ensure_capinfos, ensure_tshark};
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Run capinfos on a single file and return the raw output
fn get_capinfos_output(capinfos_path: &Path, pcap_path: &Path) -> Result<String> {
    let output = Command::new(capinfos_path)
        .arg(pcap_path)
        .output()
        .context("Failed to run capinfos")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run tshark protocol hierarchy on a single file
fn get_protocol_hierarchy(tshark_path: &Path, pcap_path: &Path) -> Result<String> {
    let output = Command::new(tshark_path)
        .arg("-r")
        .arg(pcap_path)
        .arg("-z")
        .arg("io,phs")
        .arg("-q")
        .output()
        .context("Failed to run tshark")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Summarize a single PCAP file
fn summarize_file(capinfos_path: &Path, tshark_path: &Path, pcap_path: &Path) -> Result<()> {
    let filename = pcap_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    println!("{}", format!("=== {} ===", filename).cyan());
    println!();

    // capinfos output
    let capinfos_out = get_capinfos_output(capinfos_path, pcap_path)?;
    for line in capinfos_out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Format key-value lines nicely
        if let Some((key, value)) = line.split_once(':') {
            println!("  {}: {}", key.trim().green(), value.trim());
        } else {
            println!("  {}", line);
        }
    }

    println!();

    // Protocol hierarchy
    let phs_out = get_protocol_hierarchy(tshark_path, pcap_path)?;
    let mut in_table = false;
    for line in phs_out.lines() {
        if line.contains("Protocol Hierarchy Statistics") {
            println!("  {}", "Protocol Hierarchy:".green());
            in_table = true;
            continue;
        }
        if in_table {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('=') {
                continue;
            }
            println!("    {}", trimmed);
        }
    }

    println!();
    Ok(())
}

/// Run summary on a single PCAP file
pub fn run_single(file_path: &str) -> Result<()> {
    let capinfos = ensure_capinfos()?;
    let tshark = ensure_tshark()?;

    let pcap_path = Path::new(file_path);
    if !pcap_path.exists() {
        anyhow::bail!("File not found: {}", file_path);
    }
    if !pcap_path.is_file() {
        anyhow::bail!("Not a file: {}", file_path);
    }

    summarize_file(&capinfos, &tshark, pcap_path)
}

/// Run summary on all PCAP files in a directory
pub fn run(input_dir: &str) -> Result<()> {
    let capinfos = ensure_capinfos()?;
    let tshark = ensure_tshark()?;

    let input_path = Path::new(input_dir);
    if !input_path.exists() {
        anyhow::bail!("Directory not found: {}", input_dir);
    }
    if !input_path.is_dir() {
        anyhow::bail!("Not a directory: {}", input_dir);
    }

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
        "{} {} PCAP file(s) to summarize",
        "Found:".cyan(),
        pcap_files.len()
    );
    println!();

    for pcap_file in &pcap_files {
        if let Err(e) = summarize_file(&capinfos, &tshark, pcap_file) {
            println!(
                "  {} {}: {}",
                "Error:".red(),
                pcap_file.file_name().unwrap_or_default().to_string_lossy(),
                e
            );
        }
    }

    Ok(())
}

/// Get summary output as structured lines (for web UI)
pub fn get_summary_lines(
    capinfos_path: &Path,
    tshark_path: &Path,
    pcap_path: &Path,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let filename = pcap_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    lines.push(format!("=== {} ===", filename));

    let capinfos_out = get_capinfos_output(capinfos_path, pcap_path)?;
    for line in capinfos_out.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(format!("  {}", trimmed));
        }
    }

    lines.push(String::new());

    let phs_out = get_protocol_hierarchy(tshark_path, pcap_path)?;
    let mut in_table = false;
    for line in phs_out.lines() {
        if line.contains("Protocol Hierarchy Statistics") {
            lines.push("  Protocol Hierarchy:".to_string());
            in_table = true;
            continue;
        }
        if in_table {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('=') {
                continue;
            }
            lines.push(format!("    {}", trimmed));
        }
    }

    Ok(lines)
}
