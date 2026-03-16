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

/// Collect all PCAP files from a directory
fn collect_pcap_files(source_dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(source_dir)
        .context(format!("Failed to read directory: {}", source_dir.display()))?;
    
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().to_lowercase() == "pcap" {
                    files.push(path);
                }
            }
        }
    }
    Ok(files)
}

/// Group PCAP files by IP address. Returns None if no files match the IP pattern.
fn group_files_by_ip(pcap_files: &[PathBuf]) -> Option<HashMap<String, Vec<PathBuf>>> {
    let mut grouped: HashMap<String, Vec<PathBuf>> = HashMap::new();
    
    for path in pcap_files {
        if let Some(filename) = path.file_name() {
            if let Some(ip) = extract_ip_from_filename(&filename.to_string_lossy()) {
                grouped.entry(ip).or_default().push(path.clone());
            }
        }
    }
    
    if grouped.is_empty() { None } else { Some(grouped) }
}

/// Merge files grouped by IP address
fn merge_by_ip(
    grouped: &HashMap<String, Vec<PathBuf>>,
    mergecap_path: &Path,
    output_path: &Path,
) -> (u32, u32) {
    let mut success_count = 0u32;
    let mut fail_count = 0u32;

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

        let mut cmd = Command::new(mergecap_path);
        cmd.arg("-w").arg(&output_file);
        for file in files {
            cmd.arg(file);
        }

        match cmd.output() {
            Ok(output) => {
                if output.status.success() && output_file.exists() {
                    let size = fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0);
                    println!("  {} {} ({} bytes)", "✓".green(), output_file.file_name().unwrap_or_default().to_string_lossy(), size);
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

    (success_count, fail_count)
}

/// Merge all PCAP files into a single output file
fn merge_all(
    pcap_files: &[PathBuf],
    mergecap_path: &Path,
    output_path: &Path,
) -> (u32, u32) {
    let output_file = output_path.join("merged.pcap");

    println!(
        "{} {} file(s) -> {}",
        "Merging:".green(),
        pcap_files.len(),
        output_file.file_name().unwrap_or_default().to_string_lossy()
    );

    let mut cmd = Command::new(mergecap_path);
    cmd.arg("-w").arg(&output_file);
    for file in pcap_files {
        cmd.arg(file);
    }

    match cmd.output() {
        Ok(output) => {
            if output.status.success() && output_file.exists() {
                let size = fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0);
                println!("  {} {} ({} bytes)", "✓".green(), output_file.file_name().unwrap_or_default().to_string_lossy(), size);
                (1, 0)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("  {} Failed: {}", "✗".red(), stderr.lines().next().unwrap_or("Unknown error"));
                (0, 1)
            }
        }
        Err(e) => {
            println!("  {} Error: {}", "✗".red(), e);
            (0, 1)
        }
    }
}

/// Main merge function
pub fn run(source_dir: &str, output_dir: Option<&str>) -> Result<()> {
    let source_path = PathBuf::from(source_dir);
    
    if !source_path.exists() {
        anyhow::bail!("Directory not found: {}", source_path.display());
    }
    
    if !source_path.is_dir() {
        anyhow::bail!("Not a directory: {}", source_path.display());
    }
    
    let output_path = match output_dir {
        Some(dir) => PathBuf::from(dir),
        None => source_path.clone(),
    };
    
    if !output_path.exists() {
        fs::create_dir_all(&output_path)
            .context(format!("Failed to create output directory: {}", output_path.display()))?;
        println!("{} {}", "Created output directory:".green(), output_path.display());
    }
    
    let mergecap_path = tools::ensure_mergecap()?;
    
    let pcap_files = collect_pcap_files(&source_path)?;
    
    if pcap_files.is_empty() {
        println!("{}", "No PCAP files found in directory.".yellow());
        return Ok(());
    }
    
    let (success_count, fail_count) = if let Some(grouped) = group_files_by_ip(&pcap_files) {
        println!("{} {} unique IP addresses found - merging by IP", "Mode:".cyan(), grouped.len());
        println!();
        merge_by_ip(&grouped, &mergecap_path, &output_path)
    } else {
        println!("{} No IP pattern detected - merging all {} files into one", "Mode:".cyan(), pcap_files.len());
        println!();
        merge_all(&pcap_files, &mergecap_path, &output_path)
    };
    
    println!();
    println!("{}", "=== Merge Summary ===".cyan());
    println!("{} {}", "Successful:".green(), success_count);
    if fail_count > 0 {
        println!("{} {}", "Failed:".red(), fail_count);
    }
    println!("{} {}", "Output:".cyan(), output_path.display());
    
    Ok(())
}
