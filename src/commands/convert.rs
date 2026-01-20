use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::utils::tools::ensure_etl2pcapng;

/// Statistics for conversion operation
#[derive(Default)]
struct ConvertStats {
    total: u32,
    success: u32,
    failed: u32,
}

/// Run the convert command
pub async fn run(input_dir: &str, output_dir: Option<&str>) -> Result<()> {
    // Ensure etl2pcapng is available (will download if needed)
    let etl2pcapng = ensure_etl2pcapng().await?;

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

    // Find all ETL files
    let etl_files: Vec<_> = fs::read_dir(input_path)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|ext| ext.to_string_lossy().to_lowercase() == "etl")
                .unwrap_or(false)
        })
        .collect();

    if etl_files.is_empty() {
        println!("{}", "No ETL files found in directory.".yellow());
        return Ok(());
    }

    println!(
        "{} {} ETL files to convert",
        "Found:".green(),
        etl_files.len()
    );
    println!();

    let mut stats = ConvertStats {
        total: etl_files.len() as u32,
        ..Default::default()
    };

    for (i, etl_file) in etl_files.iter().enumerate() {
        let filename = etl_file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let output_file = output_path.join(format!("{}.pcap", filename));

        println!(
            "[{}/{}] {} {} -> {}",
            i + 1,
            stats.total,
            "Converting:".cyan(),
            etl_file.file_name().unwrap_or_default().to_string_lossy(),
            output_file.file_name().unwrap_or_default().to_string_lossy()
        );

        // Run etl2pcapng
        let result = Command::new(&etl2pcapng)
            .arg(etl_file)
            .arg(&output_file)
            .output();

        match result {
            Ok(output) => {
                if output_file.exists() {
                    let file_size = fs::metadata(&output_file)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    
                    if file_size > 0 {
                        stats.success += 1;
                        println!(
                            "  {} {} bytes",
                            "Success:".green(),
                            file_size
                        );
                    } else {
                        stats.failed += 1;
                        println!("  {} Output file is empty", "Failed:".red());
                    }
                } else {
                    stats.failed += 1;
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if !stderr.is_empty() {
                        println!("  {} {}", "Failed:".red(), stderr.trim());
                    } else if !stdout.is_empty() {
                        // etl2pcapng outputs to stdout, not stderr
                        println!("  {} {}", "Failed:".red(), stdout.lines().last().unwrap_or("Unknown error"));
                    } else {
                        println!("  {} No output file created", "Failed:".red());
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
    println!("{}", "=== Conversion Summary ===".cyan());
    println!("Total files: {}", stats.total);
    println!("{} {}", "Successful:".green(), stats.success);
    if stats.failed > 0 {
        println!("{} {}", "Failed:".red(), stats.failed);
    }
    println!("{}", "Conversion complete.".cyan());

    Ok(())
}
