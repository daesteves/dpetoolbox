use crate::utils::tools;
use anyhow::{Context, Result};
use colored::Colorize;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Extract IP address from filename (pattern: _X.X.X.X.pcap)
fn extract_ip_from_filename(filename: &str) -> Option<String> {
    let re = Regex::new(r"_(\d{1,3}(?:\.\d{1,3}){3})\.pcap$").ok()?;
    re.captures(filename).map(|cap| cap[1].to_string())
}

/// Group PCAP files by IP address
fn group_files_by_ip(source_dir: &Path) -> Result<HashMap<String, Vec<PathBuf>>> {
    let mut grouped: HashMap<String, Vec<PathBuf>> = HashMap::new();
    
    let entries = fs::read_dir(source_dir)
        .context(format!("Failed to read directory: {}", source_dir.display()))?;
    
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().to_lowercase() == "pcap" {
                    if let Some(filename) = path.file_name() {
                        if let Some(ip) = extract_ip_from_filename(&filename.to_string_lossy()) {
                            grouped.entry(ip).or_default().push(path);
                        }
                    }
                }
            }
        }
    }
    
    Ok(grouped)
}

/// Main merge function
pub fn run(source_dir: &str, output_dir: Option<&str>) -> Result<()> {
    let source_path = PathBuf::from(source_dir);
    
    // Validate source directory exists
    if !source_path.exists() {
        anyhow::bail!("Directory not found: {}", source_path.display());
    }
    
    if !source_path.is_dir() {
        anyhow::bail!("Not a directory: {}", source_path.display());
    }
    
    // Determine output directory
    let output_path = match output_dir {
        Some(dir) => PathBuf::from(dir),
        None => source_path.clone(),
    };
    
    // Create output directory if needed
    if !output_path.exists() {
        fs::create_dir_all(&output_path)
            .context(format!("Failed to create output directory: {}", output_path.display()))?;
        println!("{} {}", "Created output directory:".green(), output_path.display());
    }
    
    // Ensure mergecap is available
    let mergecap_path = tools::ensure_mergecap()?;
    
    // Group files by IP
    let grouped = group_files_by_ip(&source_path)?;
    
    if grouped.is_empty() {
        println!("{}", "No PCAP files with IP pattern (filename_X.X.X.X.pcap) found.".yellow());
        return Ok(());
    }
    
    println!("{} {} unique IP addresses", "Found:".cyan(), grouped.len());
    println!();
    
    let mut success_count = 0;
    let mut fail_count = 0;
    
    // Sort IPs for consistent output
    let mut ips: Vec<_> = grouped.keys().collect();
    ips.sort();
    
    for ip in ips {
        let files = &grouped[ip];
        let output_file = output_path.join(format!("{}_merged.pcap", ip));
        
        println!(
            "{} {} file(s) for {} -> {}",
            "Merging:".green(),
            files.len(),
            ip.cyan(),
            output_file.file_name().unwrap_or_default().to_string_lossy()
        );
        
        // Build mergecap command
        let mut cmd = Command::new(&mergecap_path);
        cmd.arg("-w").arg(&output_file);
        
        for file in files {
            cmd.arg(file);
        }
        
        // Execute mergecap
        match cmd.output() {
            Ok(output) => {
                if output.status.success() && output_file.exists() {
                    let size = fs::metadata(&output_file)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    println!(
                        "  {} {} ({} bytes)",
                        "✓".green(),
                        output_file.file_name().unwrap_or_default().to_string_lossy(),
                        size
                    );
                    success_count += 1;
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("  {} Failed: {}", "✗".red(), stderr.lines().next().unwrap_or("Unknown error"));
                    fail_count += 1;
                }
            }
            Err(e) => {
                println!("  {} Error: {}", "✗".red(), e);
                fail_count += 1;
            }
        }
    }
    
    println!();
    println!("{}", "=== Merge Summary ===".cyan());
    println!("{} {}", "Successful:".green(), success_count);
    if fail_count > 0 {
        println!("{} {}", "Failed:".red(), fail_count);
    }
    println!("{} {}", "Output:".cyan(), output_path.display());
    
    Ok(())
}
