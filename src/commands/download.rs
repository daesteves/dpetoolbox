use crate::utils::tools;
use anyhow::{Context, Result};
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::Semaphore;

/// Parse URLs from a file (lines containing "http")
fn parse_urls_from_file(file_path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(file_path)
        .context(format!("Failed to read file: {}", file_path.display()))?;
    
    let urls: Vec<String> = content
        .lines()
        .filter(|line| line.contains("http"))
        .map(|s| s.trim().to_string())
        .collect();
    
    Ok(urls)
}

/// Extract filename from URL (handles Azure blob URLs)
fn extract_filename(url: &str) -> String {
    // URL format: .../container/filename?sas_token
    // Extract the filename part (4th segment after splitting by /)
    let url_without_query = url.split('?').next().unwrap_or(url);
    let segments: Vec<&str> = url_without_query.split('/').collect();
    
    // Try to get the last meaningful segment
    let filename = segments
        .iter()
        .rev()
        .find(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown_file".to_string());
    
    // Replace %2F with _ (URL-encoded forward slash)
    filename.replace("%2F", "_")
}

/// Check if file already exists
fn file_exists(output_dir: &Path, filename: &str) -> bool {
    output_dir.join(filename).exists()
}

/// Download a single file using azcopy
async fn download_file(
    azcopy_path: &Path,
    url: &str,
    output_path: &Path,
    progress: ProgressBar,
) -> Result<()> {
    let filename = output_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    
    progress.set_message(format!("Downloading {}", filename));
    
    let output = Command::new(azcopy_path)
        .arg("copy")
        .arg(url)
        .arg(output_path)
        .arg("--check-md5")
        .arg("NoCheck")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to execute azcopy")?;
    
    if output.status.success() {
        progress.finish_with_message(format!("{} {}", "✓".green(), filename));
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        progress.finish_with_message(format!("{} {} - {}", "✗".red(), filename, stderr.lines().next().unwrap_or("Unknown error")));
        anyhow::bail!("azcopy failed for {}: {}", filename, stderr)
    }
}

/// Main download function
pub async fn run(file_path: &str, output_dir: Option<&str>, threads: u32) -> Result<()> {
    let file_path = PathBuf::from(file_path);
    
    // Validate input file exists
    if !file_path.exists() {
        anyhow::bail!("File not found: {}", file_path.display());
    }
    
    // Determine output directory
    let output_dir = match output_dir {
        Some(dir) => PathBuf::from(dir),
        None => {
            // Use directory of input file + filename (without extension) as subfolder
            let parent = file_path.parent().unwrap_or(Path::new("."));
            let stem = file_path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "downloads".to_string());
            parent.join(stem)
        }
    };
    
    // Create output directory
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)
            .context(format!("Failed to create output directory: {}", output_dir.display()))?;
        println!("{} {}", "Created output directory:".green(), output_dir.display());
    } else {
        println!("{} {} (existing files will be skipped)", "Output directory:".yellow(), output_dir.display());
    }
    
    // Ensure azcopy is available
    let azcopy_path = tools::ensure_azcopy().await?;
    
    // Parse URLs from file
    let urls = parse_urls_from_file(&file_path)?;
    
    if urls.is_empty() {
        println!("{}", "No URLs found in the file.".yellow());
        return Ok(());
    }
    
    println!("{} {} URLs", "Found:".cyan(), urls.len());
    println!("{} {} parallel downloads", "Threads:".cyan(), threads);
    println!();
    
    // Track already existing files
    let mut skipped = 0;
    let mut to_download: Vec<(String, PathBuf)> = Vec::new();
    let mut seen_filenames: HashSet<String> = HashSet::new();
    
    for url in &urls {
        let filename = extract_filename(url);
        
        // Handle duplicate filenames
        let unique_filename = if seen_filenames.contains(&filename) {
            let mut counter = 1;
            let stem = Path::new(&filename).file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = Path::new(&filename).extension()
                .map(|s| format!(".{}", s.to_string_lossy()))
                .unwrap_or_default();
            loop {
                let candidate = format!("{}_{}{}", stem, counter, ext);
                if !seen_filenames.contains(&candidate) {
                    break candidate;
                }
                counter += 1;
            }
        } else {
            filename.clone()
        };
        
        seen_filenames.insert(unique_filename.clone());
        let output_path = output_dir.join(&unique_filename);
        
        if output_path.exists() {
            skipped += 1;
            println!("{} {} (already exists)", "Skipping:".yellow(), unique_filename);
        } else {
            to_download.push((url.clone(), output_path));
        }
    }
    
    if skipped > 0 {
        println!();
        println!("{} {} files (already downloaded)", "Skipped:".yellow(), skipped);
    }
    
    if to_download.is_empty() {
        println!("{}", "All files already downloaded!".green());
        return Ok(());
    }
    
    println!();
    println!("{} {} files", "Downloading:".green(), to_download.len());
    println!();
    
    let start_time = Instant::now();
    
    // Set up progress tracking
    let multi_progress = MultiProgress::new();
    let style = ProgressStyle::default_spinner()
        .template("{spinner:.cyan} {msg}")
        .unwrap();
    
    // Create semaphore for limiting concurrent downloads
    let semaphore = Arc::new(Semaphore::new(threads as usize));
    let azcopy_path = Arc::new(azcopy_path);
    
    // Spawn download tasks
    let mut handles = Vec::new();
    
    for (url, output_path) in to_download {
        let permit = semaphore.clone().acquire_owned().await?;
        let azcopy = azcopy_path.clone();
        let progress = multi_progress.add(ProgressBar::new_spinner());
        progress.set_style(style.clone());
        progress.enable_steady_tick(std::time::Duration::from_millis(100));
        
        let handle = tokio::spawn(async move {
            let result = download_file(&azcopy, &url, &output_path, progress).await;
            drop(permit);
            result
        });
        
        handles.push(handle);
    }
    
    // Wait for all downloads to complete
    let mut success_count = 0;
    let mut fail_count = 0;
    
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => success_count += 1,
            Ok(Err(_)) => fail_count += 1,
            Err(_) => fail_count += 1,
        }
    }
    
    let elapsed = start_time.elapsed();
    
    println!();
    println!("{}", "=== Download Summary ===".cyan());
    println!("{} {}", "Successful:".green(), success_count);
    if fail_count > 0 {
        println!("{} {}", "Failed:".red(), fail_count);
    }
    if skipped > 0 {
        println!("{} {}", "Skipped:".yellow(), skipped);
    }
    println!("{} {:.1}s", "Time:".cyan(), elapsed.as_secs_f64());
    println!("{} {}", "Output:".cyan(), output_dir.display());
    
    Ok(())
}
