use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response, Sse},
    routing::{get, post},
    Form, Json, Router,
};
use futures::stream::{self, Stream};
use rust_embed::Embed;
use serde::Deserialize;
use std::{convert::Infallible, time::Duration};
use tokio_stream::StreamExt;

use super::state::{AppState, JobStatus};

#[derive(Embed)]
#[folder = "static/"]
struct StaticFiles;

/// Create all routes
pub fn create_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/static/{*file}", get(static_files))
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{id}", get(get_job))
        .route("/api/jobs/{id}/stream", get(job_stream))
        .route("/api/download", post(start_download))
        .route("/api/merge", post(start_merge))
        .route("/api/filter", post(start_filter))
        .route("/api/convert", post(start_convert))
        .route("/api/tcpping", post(start_tcpping))
        .route("/api/tcpping/{id}/stop", post(stop_tcpping))
        // htmx partials
        .route("/partials/download-form", get(download_form))
        .route("/partials/merge-form", get(merge_form))
        .route("/partials/filter-form", get(filter_form))
        .route("/partials/convert-form", get(convert_form))
        .route("/partials/tcpping-form", get(tcpping_form))
        .route("/partials/jobs", get(jobs_partial))
        .route("/partials/job/{id}", get(job_partial))
}

/// Serve static files
async fn static_files(Path(file): Path<String>) -> impl IntoResponse {
    match StaticFiles::get(&file) {
        Some(content) => {
            let mime = mime_guess::from_path(&file).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Main page
async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// List all jobs
async fn list_jobs(State(state): State<AppState>) -> Json<Vec<super::state::Job>> {
    Json(state.get_all_jobs())
}

/// Get a specific job
async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<super::state::Job>, StatusCode> {
    state.get_job(&id).map(Json).ok_or(StatusCode::NOT_FOUND)
}

/// Server-Sent Events stream for job updates
async fn job_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let stream = stream::unfold((state, id), |(state, id)| async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let job = state.get_job(&id)?;
        let data = serde_json::to_string(&job).ok()?;
        let event = axum::response::sse::Event::default().data(data);
        
        // Stop streaming if job is complete
        if job.status == JobStatus::Completed || job.status == JobStatus::Failed {
            return None;
        }
        
        Some((Ok(event), (state, id)))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}

// ============================================================================
// Form Handlers
// ============================================================================

#[derive(Deserialize)]
pub struct DownloadForm {
    urls: Option<String>,
    file_path: Option<String>,
    output: Option<String>,
    threads: Option<u32>,
}

async fn start_download(
    State(state): State<AppState>,
    Form(form): Form<DownloadForm>,
) -> Html<String> {
    let job = state.create_job("download");
    let job_id = job.id.clone();
    
    // Spawn background task
    let state_clone = state.clone();
    let urls = form.urls.clone();
    let file_path = form.file_path.clone();
    let output = form.output.clone();
    let threads = form.threads.unwrap_or(4);
    
    tokio::spawn(async move {
        run_download_job(state_clone, &job_id, urls.as_deref(), file_path.as_deref(), output.as_deref(), threads).await;
    });

    Html(job_card_html(&job))
}

#[derive(Deserialize)]
pub struct MergeForm {
    input: String,
    output: Option<String>,
}

async fn start_merge(
    State(state): State<AppState>,
    Form(form): Form<MergeForm>,
) -> Html<String> {
    let job = state.create_job("merge");
    let job_id = job.id.clone();
    
    let state_clone = state.clone();
    let input = form.input.clone();
    let output = form.output.clone();
    
    tokio::spawn(async move {
        run_merge_job(state_clone, &job_id, &input, output.as_deref()).await;
    });

    Html(job_card_html(&job))
}

#[derive(Deserialize)]
pub struct FilterForm {
    input: String,
    output: Option<String>,
    filter: String,
    delete_empty: Option<String>,
}

async fn start_filter(
    State(state): State<AppState>,
    Form(form): Form<FilterForm>,
) -> Html<String> {
    let job = state.create_job("filter");
    let job_id = job.id.clone();
    
    let state_clone = state.clone();
    let input = form.input.clone();
    let output = form.output.clone();
    let filter = form.filter.clone();
    let delete_empty = form.delete_empty.as_deref() == Some("on");
    
    tokio::spawn(async move {
        run_filter_job(state_clone, &job_id, &input, output.as_deref(), &filter, delete_empty).await;
    });

    Html(job_card_html(&job))
}

#[derive(Deserialize)]
pub struct ConvertForm {
    input: Option<String>,
    single_file: Option<String>,
    output: Option<String>,
}

async fn start_convert(
    State(state): State<AppState>,
    Form(form): Form<ConvertForm>,
) -> Html<String> {
    let job = state.create_job("convert");
    let job_id = job.id.clone();
    
    let state_clone = state.clone();
    let input = form.input.clone();
    let single_file = form.single_file.clone();
    let output = form.output.clone();
    
    tokio::spawn(async move {
        run_convert_job(state_clone, &job_id, input.as_deref(), single_file.as_deref(), output.as_deref()).await;
    });

    Html(job_card_html(&job))
}

#[derive(Deserialize)]
pub struct TcppingForm {
    target: String,
    port: u16,
    timeout: Option<u64>,
    interval: Option<u64>,
}

async fn start_tcpping(
    State(state): State<AppState>,
    Form(form): Form<TcppingForm>,
) -> Html<String> {
    let job = state.create_job("tcpping");
    let job_id = job.id.clone();
    
    let state_clone = state.clone();
    let target = form.target.clone();
    let port = form.port;
    let timeout = form.timeout.unwrap_or(2000);
    let interval = form.interval.unwrap_or(1);
    
    tokio::spawn(async move {
        run_tcpping_job(state_clone, &job_id, &target, port, timeout, interval).await;
    });

    Html(job_card_html(&job))
}

async fn stop_tcpping(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    state.update_job(&id, |job| {
        job.status = JobStatus::Completed;
        job.message = "Stopped by user".to_string();
    });
    StatusCode::OK
}

// ============================================================================
// Job Runners
// ============================================================================

async fn run_download_job(state: AppState, job_id: &str, urls: Option<&str>, file_path: Option<&str>, output: Option<&str>, threads: u32) {
    use crate::utils::tools::ensure_azcopy;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    
    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = "Initializing download...".to_string();
        job.output.push("Checking for azcopy...".to_string());
    });

    // Ensure azcopy is available
    let azcopy_path = match ensure_azcopy().await {
        Ok(path) => {
            state.update_job(job_id, |job| {
                job.output.push(format!("Found azcopy: {}", path.display()));
            });
            path
        }
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("azcopy not available: {}", e);
            });
            return;
        }
    };

    // Get URLs from file or direct input
    let url_content = if let Some(fp) = file_path {
        if !fp.trim().is_empty() {
            state.update_job(job_id, |job| {
                job.output.push(format!("Reading URLs from: {}", fp));
            });
            match std::fs::read_to_string(fp) {
                Ok(content) => content,
                Err(e) => {
                    state.update_job(job_id, |job| {
                        job.status = JobStatus::Failed;
                        job.message = format!("Failed to read URL file: {}", e);
                    });
                    return;
                }
            }
        } else {
            urls.unwrap_or("").to_string()
        }
    } else {
        urls.unwrap_or("").to_string()
    };

    // Parse URLs
    let url_list: Vec<String> = url_content.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && l.starts_with("http"))
        .collect();

    if url_list.is_empty() {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Failed;
            job.message = "No valid URLs found".to_string();
        });
        return;
    }

    let total_files = url_list.len();
    state.update_job(job_id, |job| {
        job.message = format!("Downloading {} files with {} parallel threads...", total_files, threads);
        job.output.push(format!("Found {} URLs to download", total_files));
        job.output.push(format!("Using {} parallel downloads", threads));
    });

    // Determine output directory
    // Priority: 1. Custom output path, 2. Subfolder next to txt file, 3. Exe location
    let output_dir = if let Some(out) = output {
        if !out.trim().is_empty() {
            std::path::PathBuf::from(out)
        } else if let Some(fp) = file_path {
            // Use subfolder next to txt file (e.g., C:\Downloads\test.txt -> C:\Downloads\test\)
            let fp_path = std::path::Path::new(fp);
            if let (Some(parent), Some(stem)) = (fp_path.parent(), fp_path.file_stem()) {
                parent.join(stem)
            } else {
                std::env::current_exe()
                    .map(|p| p.parent().unwrap_or(std::path::Path::new(".")).to_path_buf())
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
            }
        } else {
            std::env::current_exe()
                .map(|p| p.parent().unwrap_or(std::path::Path::new(".")).to_path_buf())
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        }
    } else if let Some(fp) = file_path {
        // Use subfolder next to txt file
        let fp_path = std::path::Path::new(fp);
        if let (Some(parent), Some(stem)) = (fp_path.parent(), fp_path.file_stem()) {
            parent.join(stem)
        } else {
            std::env::current_exe()
                .map(|p| p.parent().unwrap_or(std::path::Path::new(".")).to_path_buf())
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        }
    } else {
        std::env::current_exe()
            .map(|p| p.parent().unwrap_or(std::path::Path::new(".")).to_path_buf())
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
    };

    state.update_job(job_id, |job| {
        job.output.push(format!("Output directory: {}", output_dir.display()));
        job.output.push("─".repeat(40));
    });

    // Create output directory if needed
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Failed;
            job.message = format!("Failed to create output directory: {}", e);
        });
        return;
    }

    // Counters for parallel execution
    let success_count = Arc::new(AtomicU32::new(0));
    let skip_count = Arc::new(AtomicU32::new(0));
    let fail_count = Arc::new(AtomicU32::new(0));
    let completed_count = Arc::new(AtomicU32::new(0));

    // Process downloads in parallel using semaphore to limit concurrency
    let semaphore = Arc::new(tokio::sync::Semaphore::new(threads as usize));
    let mut handles = Vec::new();

    for (i, url) in url_list.into_iter().enumerate() {
        let sem = semaphore.clone();
        let azcopy = azcopy_path.clone();
        let out_dir = output_dir.clone();
        let state_clone = state.clone();
        let job_id_clone = job_id.to_string();
        let success = success_count.clone();
        let skip = skip_count.clone();
        let fail = fail_count.clone();
        let completed = completed_count.clone();
        let total = total_files;

        let handle = tokio::spawn(async move {
            // Acquire semaphore permit to limit concurrency
            let _permit = sem.acquire().await.unwrap();

            // Extract filename from URL
            let filename = url.split('/').last().unwrap_or("file")
                .split('?').next().unwrap_or("file");
            let output_path = out_dir.join(filename);

            // Check if file already exists
            if output_path.exists() {
                skip.fetch_add(1, Ordering::Relaxed);
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                state_clone.update_job(&job_id_clone, |job| {
                    job.progress = ((done as usize * 100) / total) as u32;
                    job.output.push(format!("[{}/{}] SKIP: {} (exists)", done, total, filename));
                });
                return;
            }

            state_clone.update_job(&job_id_clone, |job| {
                job.output.push(format!("[{}] Downloading: {}", i + 1, filename));
            });

            // Run azcopy
            let result = tokio::process::Command::new(&azcopy)
                .args(["copy", &url, output_path.to_str().unwrap()])
                .output()
                .await;

            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;

            match result {
                Ok(output) => {
                    if output.status.success() {
                        success.fetch_add(1, Ordering::Relaxed);
                        state_clone.update_job(&job_id_clone, |job| {
                            job.progress = ((done as usize * 100) / total) as u32;
                            job.output.push(format!("[{}/{}] OK: {}", done, total, filename));
                        });
                    } else {
                        fail.fetch_add(1, Ordering::Relaxed);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        state_clone.update_job(&job_id_clone, |job| {
                            job.progress = ((done as usize * 100) / total) as u32;
                            job.output.push(format!("[{}/{}] FAIL: {} - {}", done, total, filename, stderr.lines().next().unwrap_or("error")));
                        });
                    }
                }
                Err(e) => {
                    fail.fetch_add(1, Ordering::Relaxed);
                    state_clone.update_job(&job_id_clone, |job| {
                        job.progress = ((done as usize * 100) / total) as u32;
                        job.output.push(format!("[{}/{}] ERROR: {} - {}", done, total, filename, e));
                    });
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all downloads to complete
    for handle in handles {
        let _ = handle.await;
    }

    // Final summary
    let final_success = success_count.load(Ordering::Relaxed);
    let final_skip = skip_count.load(Ordering::Relaxed);
    let final_fail = fail_count.load(Ordering::Relaxed);

    state.update_job(job_id, |job| {
        job.progress = 100;
        job.output.push("─".repeat(40));
        job.output.push(format!("Summary: {} success, {} skipped, {} failed", final_success, final_skip, final_fail));
        
        if final_fail == 0 {
            job.status = JobStatus::Completed;
            job.message = format!("Download completed: {} files ({} skipped)", final_success, final_skip);
        } else {
            job.status = JobStatus::Failed;
            job.message = format!("Download finished with {} failures", final_fail);
        }
    });
}

async fn run_merge_job(state: AppState, job_id: &str, input: &str, output: Option<&str>) {
    use crate::utils::tools::ensure_mergecap;
    use regex::Regex;
    use std::collections::HashMap;
    
    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = "Initializing merge...".to_string();
        job.output.push("Checking for mergecap...".to_string());
    });

    // Ensure mergecap is available
    let mergecap = match ensure_mergecap() {
        Ok(path) => {
            state.update_job(job_id, |job| {
                job.output.push(format!("Found mergecap: {}", path.display()));
            });
            path
        }
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("mergecap not available: {}", e);
                job.output.push(format!("ERROR: {}", e));
                job.output.push("Please install Wireshark to use this feature.".to_string());
            });
            return;
        }
    };

    let input_path = std::path::Path::new(input);
    if !input_path.exists() {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Failed;
            job.message = format!("Directory not found: {}", input);
        });
        return;
    }

    // Use custom output path if provided, otherwise save to input directory
    let output_path = match output {
        Some(out) if !out.trim().is_empty() => std::path::PathBuf::from(out),
        _ => input_path.to_path_buf(),
    };

    // Create output directory if needed
    if !output_path.exists() {
        if let Err(e) = std::fs::create_dir_all(&output_path) {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("Failed to create output directory: {}", e);
            });
            return;
        }
    }

    state.update_job(job_id, |job| {
        job.output.push(format!("Input: {}", input_path.display()));
        job.output.push(format!("Output: {}", output_path.display()));
    });

    // Group files by IP address (pattern: _X.X.X.X.pcap)
    let ip_regex = Regex::new(r"_(\d{1,3}(?:\.\d{1,3}){3})\.pcap$").unwrap();
    let mut grouped: HashMap<String, Vec<std::path::PathBuf>> = HashMap::new();

    let entries = match std::fs::read_dir(input_path) {
        Ok(e) => e,
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("Failed to read directory: {}", e);
            });
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().to_lowercase() == "pcap" {
                    if let Some(filename) = path.file_name() {
                        let filename_str = filename.to_string_lossy();
                        if let Some(caps) = ip_regex.captures(&filename_str) {
                            let ip = caps[1].to_string();
                            grouped.entry(ip).or_default().push(path);
                        }
                    }
                }
            }
        }
    }

    if grouped.is_empty() {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Completed;
            job.progress = 100;
            job.message = "No matching files found".to_string();
            job.output.push("No PCAP files with IP pattern (filename_X.X.X.X.pcap) found.".to_string());
        });
        return;
    }

    let total = grouped.len();
    state.update_job(job_id, |job| {
        job.message = format!("Merging files for {} IP(s)...", total);
        job.output.push(format!("Found {} unique IP addresses", total));
        job.output.push("─".repeat(40));
    });

    let mut success = 0u32;
    let mut failed = 0u32;
    let mut ips: Vec<_> = grouped.keys().cloned().collect();
    ips.sort();

    for (i, ip) in ips.iter().enumerate() {
        let files = &grouped[ip];
        let output_file = output_path.join(format!("{}_merged.pcap", ip));

        state.update_job(job_id, |job| {
            job.progress = ((i * 100) / total) as u32;
            job.output.push(format!("[{}/{}] Merging {} file(s) for IP {}", i + 1, total, files.len(), ip));
        });

        // Build mergecap command
        let mut cmd = std::process::Command::new(&mergecap);
        cmd.arg("-w").arg(&output_file);
        for file in files {
            cmd.arg(file);
        }

        match cmd.output() {
            Ok(cmd_output) => {
                if cmd_output.status.success() && output_file.exists() {
                    let size = std::fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0);
                    success += 1;
                    state.update_job(job_id, |job| {
                        job.output.push(format!("  → {}_merged.pcap ({} bytes) - OK", ip, size));
                    });
                } else {
                    failed += 1;
                    let stderr = String::from_utf8_lossy(&cmd_output.stderr);
                    state.update_job(job_id, |job| {
                        job.output.push(format!("  → ERROR: {}", stderr.lines().next().unwrap_or("Unknown error")));
                    });
                }
            }
            Err(e) => {
                failed += 1;
                state.update_job(job_id, |job| {
                    job.output.push(format!("  → ERROR: {}", e));
                });
            }
        }
    }

    // Final summary
    state.update_job(job_id, |job| {
        job.progress = 100;
        job.output.push("─".repeat(40));
        job.output.push("=== Merge Summary ===".to_string());
        job.output.push(format!("Successful: {}", success));
        if failed > 0 {
            job.output.push(format!("Failed: {}", failed));
        }
        job.output.push(format!("Output: {}", output_path.display()));

        if failed == 0 {
            job.status = JobStatus::Completed;
            job.message = format!("Merged files for {} IP(s)", success);
        } else {
            job.status = JobStatus::Failed;
            job.message = format!("Completed with {} failure(s)", failed);
        }
    });
}

async fn run_filter_job(state: AppState, job_id: &str, input: &str, output: Option<&str>, filter: &str, delete_empty: bool) {
    use crate::utils::tools::{ensure_tshark, ensure_capinfos};
    use regex::Regex;
    
    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = "Initializing filter...".to_string();
        job.output.push("Checking for tshark...".to_string());
    });

    // Ensure required tools
    let tshark = match ensure_tshark() {
        Ok(path) => {
            state.update_job(job_id, |job| {
                job.output.push(format!("Found tshark: {}", path.display()));
            });
            path
        }
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("tshark not available: {}", e);
                job.output.push(format!("ERROR: {}", e));
            });
            return;
        }
    };

    let capinfos = match ensure_capinfos() {
        Ok(path) => {
            state.update_job(job_id, |job| {
                job.output.push(format!("Found capinfos: {}", path.display()));
            });
            path
        }
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("capinfos not available: {}", e);
                job.output.push(format!("ERROR: {}", e));
            });
            return;
        }
    };

    let input_path = std::path::Path::new(input);
    if !input_path.exists() {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Failed;
            job.message = format!("Input directory not found: {}", input);
        });
        return;
    }

    // Use custom output path if provided, otherwise save to input directory
    let output_path = match output {
        Some(out) if !out.trim().is_empty() => std::path::Path::new(out).to_path_buf(),
        _ => input_path.to_path_buf(),
    };
    
    // Create output directory if needed
    if !output_path.exists() {
        if let Err(e) = std::fs::create_dir_all(&output_path) {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("Failed to create output directory: {}", e);
            });
            return;
        }
    }

    // Find all PCAP files
    let pcap_files: Vec<_> = match std::fs::read_dir(input_path) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|ext| ext.to_string_lossy().to_lowercase() == "pcap").unwrap_or(false))
            .collect(),
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("Failed to read directory: {}", e);
            });
            return;
        }
    };

    if pcap_files.is_empty() {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Completed;
            job.progress = 100;
            job.message = "No PCAP files found".to_string();
            job.output.push("No PCAP files found in directory.".to_string());
        });
        return;
    }

    let total = pcap_files.len();
    state.update_job(job_id, |job| {
        job.message = format!("Filtering {} files...", total);
        job.output.push(format!("Found {} PCAP files to filter", total));
        job.output.push(format!("Filter: {}", filter));
        job.output.push(format!("Output: {}", output_path.display()));
        job.output.push("─".repeat(40));
    });

    // VXLAN decode args
    let vxlan_decode_args = [
        "-d", "udp.port==65330,vxlan",
        "-d", "udp.port==65530,vxlan",
        "-d", "udp.port==10000,vxlan",
        "-d", "udp.port==20000,vxlan",
    ];

    let mut processed = 0u32;
    let mut with_packets = 0u32;
    let mut empty = 0u32;
    let mut deleted = 0u32;
    let mut failed = 0u32;

    for (i, pcap_file) in pcap_files.iter().enumerate() {
        let filename = pcap_file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let display_name = pcap_file.file_name().unwrap_or_default().to_string_lossy().to_string();
        let output_file = output_path.join(format!("{}_filtered.pcap", filename));

        state.update_job(job_id, |job| {
            job.progress = ((i * 100) / total) as u32;
            job.output.push(format!("[{}/{}] Filtering: {}", i + 1, total, display_name));
        });

        // Build tshark command
        let mut cmd = std::process::Command::new(&tshark);
        for arg in &vxlan_decode_args {
            cmd.arg(arg);
        }
        cmd.arg("-r").arg(pcap_file);
        cmd.arg("-Y").arg(filter);
        cmd.arg("-w").arg(&output_file);

        let result = cmd.output();

        match result {
            Ok(cmd_output) => {
                if output_file.exists() {
                    processed += 1;

                    // Get packet count using capinfos
                    let packet_count = {
                        let capinfos_output = std::process::Command::new(&capinfos)
                            .arg("-c")
                            .arg(&output_file)
                            .output();
                        
                        if let Ok(out) = capinfos_output {
                            let out_str = String::from_utf8_lossy(&out.stdout);
                            let re = Regex::new(r"Number of packets:\s+(\d+)").unwrap();
                            re.captures(&out_str)
                                .and_then(|c| c.get(1))
                                .and_then(|m| m.as_str().parse::<u64>().ok())
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    };

                    let file_size = std::fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0);

                    if packet_count == 0 {
                        empty += 1;
                        if delete_empty {
                            if std::fs::remove_file(&output_file).is_ok() {
                                deleted += 1;
                                state.update_job(job_id, |job| {
                                    job.output.push(format!("  → 0 packets - DELETED"));
                                });
                            } else {
                                state.update_job(job_id, |job| {
                                    job.output.push(format!("  → 0 packets - DELETE FAILED"));
                                });
                            }
                        } else {
                            state.update_job(job_id, |job| {
                                job.output.push(format!("  → 0 packets, {} bytes - KEPT", file_size));
                            });
                        }
                    } else {
                        with_packets += 1;
                        state.update_job(job_id, |job| {
                            job.output.push(format!("  → {} packets, {} bytes - SAVED", packet_count, file_size));
                        });
                    }
                } else {
                    failed += 1;
                    let stderr = String::from_utf8_lossy(&cmd_output.stderr);
                    state.update_job(job_id, |job| {
                        if !stderr.is_empty() {
                            job.output.push(format!("  → ERROR: {}", stderr.lines().next().unwrap_or("No output")));
                        } else {
                            job.output.push("  → ERROR: No output created".to_string());
                        }
                    });
                }
            }
            Err(e) => {
                failed += 1;
                state.update_job(job_id, |job| {
                    job.output.push(format!("  → ERROR: {}", e));
                });
            }
        }
    }

    // Final summary
    state.update_job(job_id, |job| {
        job.progress = 100;
        job.output.push("─".repeat(40));
        job.output.push("=== Filtering Summary ===".to_string());
        job.output.push(format!("Total processed: {}", processed));
        job.output.push(format!("Files with packets: {}", with_packets));
        job.output.push(format!("Empty files: {}", empty));
        if delete_empty {
            job.output.push(format!("Empty files deleted: {}", deleted));
        }
        if failed > 0 {
            job.output.push(format!("Failed: {}", failed));
        }

        if failed == 0 {
            job.status = JobStatus::Completed;
            job.message = format!("Filtered {} files ({} with packets)", processed, with_packets);
        } else {
            job.status = JobStatus::Failed;
            job.message = format!("Completed with {} failures", failed);
        }
    });
}

async fn run_convert_job(state: AppState, job_id: &str, input_dir: Option<&str>, single_file: Option<&str>, output: Option<&str>) {
    use crate::utils::tools::ensure_etl2pcapng;
    
    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = "Initializing conversion...".to_string();
        job.output.push("Checking for etl2pcapng...".to_string());
    });

    // Ensure etl2pcapng is available
    let etl2pcapng = match ensure_etl2pcapng().await {
        Ok(path) => {
            state.update_job(job_id, |job| {
                job.output.push(format!("Found etl2pcapng: {}", path.display()));
            });
            path
        }
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("etl2pcapng not available: {}", e);
                job.output.push(format!("ERROR: {}", e));
            });
            return;
        }
    };

    // Determine if we're converting a single file or a directory
    let single_file_path = single_file.filter(|s| !s.trim().is_empty()).map(std::path::PathBuf::from);
    let input_dir_path = input_dir.filter(|s| !s.trim().is_empty()).map(std::path::PathBuf::from);

    // Collect ETL files to convert
    let etl_files: Vec<std::path::PathBuf> = if let Some(ref file_path) = single_file_path {
        // Single file mode
        if !file_path.exists() {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("File not found: {}", file_path.display());
            });
            return;
        }
        state.update_job(job_id, |job| {
            job.output.push(format!("Single file mode: {}", file_path.display()));
        });
        vec![file_path.clone()]
    } else if let Some(ref dir_path) = input_dir_path {
        // Directory mode
        if !dir_path.exists() {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("Directory not found: {}", dir_path.display());
            });
            return;
        }
        
        match std::fs::read_dir(dir_path) {
            Ok(entries) => {
                let files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().map(|ext| ext.to_string_lossy().to_lowercase() == "etl").unwrap_or(false))
                    .collect();
                
                state.update_job(job_id, |job| {
                    job.output.push(format!("Directory mode: {}", dir_path.display()));
                });
                files
            }
            Err(e) => {
                state.update_job(job_id, |job| {
                    job.status = JobStatus::Failed;
                    job.message = format!("Failed to read directory: {}", e);
                });
                return;
            }
        }
    } else {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Failed;
            job.message = "No input specified".to_string();
        });
        return;
    };

    if etl_files.is_empty() {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Completed;
            job.progress = 100;
            job.message = "No ETL files found".to_string();
            job.output.push("No ETL files found.".to_string());
        });
        return;
    }

    // Determine output directory: custom > same as input file/dir
    let output_dir = match output.filter(|s| !s.trim().is_empty()) {
        Some(out) => std::path::PathBuf::from(out),
        None => {
            // Default to same location as input
            if let Some(ref file_path) = single_file_path {
                file_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf()
            } else if let Some(ref dir_path) = input_dir_path {
                dir_path.clone()
            } else {
                std::path::PathBuf::from(".")
            }
        }
    };

    // Create output directory if needed
    if !output_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("Failed to create output directory: {}", e);
            });
            return;
        }
    }

    let total = etl_files.len();
    state.update_job(job_id, |job| {
        job.message = format!("Converting {} file(s)...", total);
        job.output.push(format!("Found {} ETL file(s) to convert", total));
        job.output.push(format!("Output: {}", output_dir.display()));
        job.output.push("─".repeat(40));
    });

    let mut success = 0u32;
    let mut failed = 0u32;

    for (i, etl_file) in etl_files.iter().enumerate() {
        let filename = etl_file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let display_name = etl_file.file_name().unwrap_or_default().to_string_lossy().to_string();
        let output_file = output_dir.join(format!("{}.pcap", filename));

        state.update_job(job_id, |job| {
            job.progress = ((i * 100) / total) as u32;
            job.output.push(format!("[{}/{}] Converting: {}", i + 1, total, display_name));
        });

        // Run etl2pcapng
        let result = std::process::Command::new(&etl2pcapng)
            .arg(etl_file)
            .arg(&output_file)
            .output();

        match result {
            Ok(cmd_output) => {
                if output_file.exists() {
                    let file_size = std::fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0);
                    if file_size > 0 {
                        success += 1;
                        state.update_job(job_id, |job| {
                            job.output.push(format!("  → {} bytes - OK", file_size));
                        });
                    } else {
                        failed += 1;
                        state.update_job(job_id, |job| {
                            job.output.push("  → ERROR: Output file is empty".to_string());
                        });
                    }
                } else {
                    failed += 1;
                    let stderr = String::from_utf8_lossy(&cmd_output.stderr);
                    let stdout = String::from_utf8_lossy(&cmd_output.stdout);
                    let err_msg = if !stderr.is_empty() {
                        stderr.lines().next().unwrap_or("Unknown error").to_string()
                    } else if !stdout.is_empty() {
                        stdout.lines().last().unwrap_or("Unknown error").to_string()
                    } else {
                        "No output file created".to_string()
                    };
                    state.update_job(job_id, |job| {
                        job.output.push(format!("  → ERROR: {}", err_msg));
                    });
                }
            }
            Err(e) => {
                failed += 1;
                state.update_job(job_id, |job| {
                    job.output.push(format!("  → ERROR: {}", e));
                });
            }
        }
    }

    // Final summary
    state.update_job(job_id, |job| {
        job.progress = 100;
        job.output.push("─".repeat(40));
        job.output.push("=== Conversion Summary ===".to_string());
        job.output.push(format!("Total files: {}", total));
        job.output.push(format!("Successful: {}", success));
        if failed > 0 {
            job.output.push(format!("Failed: {}", failed));
        }

        if failed == 0 {
            job.status = JobStatus::Completed;
            job.message = format!("Converted {} file(s) successfully", success);
        } else {
            job.status = JobStatus::Failed;
            job.message = format!("Completed with {} failure(s)", failed);
        }
    });
}

async fn run_tcpping_job(state: AppState, job_id: &str, target: &str, port: u16, timeout: u64, interval: u64) {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::{Duration, Instant};

    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = format!("Pinging {}:{}", target, port);
    });

    let addr_str = format!("{}:{}", target, port);
    let addr = match addr_str.to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(a) => a,
            None => {
                state.update_job(job_id, |job| {
                    job.status = JobStatus::Failed;
                    job.message = "Could not resolve address".to_string();
                });
                return;
            }
        },
        Err(e) => {
            state.update_job(job_id, |job| {
                job.status = JobStatus::Failed;
                job.message = format!("DNS resolution failed: {}", e);
            });
            return;
        }
    };

    let timeout_dur = Duration::from_millis(timeout);
    let interval_dur = Duration::from_secs(interval);

    loop {
        // Check if stopped
        if let Some(job) = state.get_job(job_id) {
            if job.status != JobStatus::Running {
                break;
            }
        } else {
            break;
        }

        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        let start = Instant::now();

        let result = TcpStream::connect_timeout(&addr, timeout_dur);
        let elapsed = start.elapsed().as_millis();

        let line = match result {
            Ok(_) => format!("[{}] Success: Connected in {} ms", timestamp, elapsed),
            Err(e) => {
                if elapsed >= timeout as u128 {
                    format!("[{}] Timeout after {} ms", timestamp, timeout)
                } else {
                    format!("[{}] Failed: {}", timestamp, e)
                }
            }
        };

        state.update_job(job_id, |job| {
            job.output.push(line);
            // Keep only last 50 lines
            if job.output.len() > 50 {
                job.output.remove(0);
            }
        });

        tokio::time::sleep(interval_dur).await;
    }
}

// ============================================================================
// HTML Partials
// ============================================================================

async fn download_form() -> Html<&'static str> {
    Html(DOWNLOAD_FORM_HTML)
}

async fn merge_form() -> Html<&'static str> {
    Html(MERGE_FORM_HTML)
}

async fn filter_form() -> Html<&'static str> {
    Html(FILTER_FORM_HTML)
}

async fn convert_form() -> Html<&'static str> {
    Html(CONVERT_FORM_HTML)
}

async fn tcpping_form() -> Html<&'static str> {
    Html(TCPPING_FORM_HTML)
}

async fn jobs_partial(State(state): State<AppState>) -> Html<String> {
    let jobs = state.get_all_jobs();
    let mut html = String::new();
    
    for job in jobs.iter().rev() {
        html.push_str(&job_card_html(job));
    }
    
    if jobs.is_empty() {
        html.push_str(r##"<p class="text-gray-500 dark:text-gray-400 text-center py-4">No jobs yet. Start a task from the menu above.</p>"##);
    }
    
    Html(html)
}

/// Get a single job as HTML partial (for polling updates)
async fn job_partial(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    state
        .get_job(&id)
        .map(|job| Html(job_card_html(&job)))
        .ok_or(StatusCode::NOT_FOUND)
}

fn job_card_html(job: &super::state::Job) -> String {
    let status_class = match job.status {
        JobStatus::Pending => "bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-200",
        JobStatus::Running => "bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200 animate-pulse",
        JobStatus::Completed => "bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200",
        JobStatus::Failed => "bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200",
    };
    
    let status_text = match job.status {
        JobStatus::Pending => "Pending",
        JobStatus::Running => "Running",
        JobStatus::Completed => "Completed",
        JobStatus::Failed => "Failed",
    };

    let output_html = if !job.output.is_empty() {
        let lines: String = job.output.iter()
            .map(|l| format!("<div class=\"py-0.5\">{}</div>", html_escape(l)))
            .collect();
        // Add script to auto-scroll to bottom after htmx swap
        format!(r##"<div class="mt-2 bg-gray-900 text-green-400 p-2 rounded text-xs font-mono max-h-48 overflow-y-auto job-output" id="output-{id}">{lines}</div>
        <script>document.getElementById('output-{id}').scrollTop = document.getElementById('output-{id}').scrollHeight;</script>"##, id = job.id, lines = lines)
    } else {
        String::new()
    };

    let stop_button = if job.job_type == "tcpping" && job.status == JobStatus::Running {
        format!(r##"<button hx-post="/api/tcpping/{}/stop" hx-swap="none" class="mt-2 text-red-600 dark:text-red-400 hover:text-red-800 dark:hover:text-red-300 text-sm font-medium">⏹ Stop</button>"##, job.id)
    } else {
        String::new()
    };

    // Only poll if job is still running
    let poll_attr = if job.status == JobStatus::Running || job.status == JobStatus::Pending {
        format!(r##"hx-get="/partials/job/{}" hx-trigger="every 1s" hx-swap="outerHTML settle:0s""##, job.id)
    } else {
        String::new()
    };

    format!(
        r##"<div class="job-card bg-white dark:bg-gray-800 rounded-lg shadow p-4 mb-3 border-l-4 {border_color}" id="job-{id}" {poll_attr}>
            <div class="flex flex-col sm:flex-row sm:justify-between sm:items-start gap-1">
                <div>
                    <span class="font-semibold capitalize dark:text-white">{job_type}</span>
                    <span class="ml-2 px-2 py-1 rounded text-xs {status_class}">{status_text}</span>
                </div>
                <div class="text-xs text-gray-500 dark:text-gray-400">{created_at}</div>
            </div>
            <p class="text-sm text-gray-600 dark:text-gray-300 mt-2">{message}</p>
            {output_html}
            {stop_button}
        </div>"##,
        id = job.id,
        job_type = job.job_type,
        status_class = status_class,
        status_text = status_text,
        created_at = job.created_at,
        message = html_escape(&job.message),
        output_html = output_html,
        stop_button = stop_button,
        poll_attr = poll_attr,
        border_color = match job.status {
            JobStatus::Pending => "border-gray-300 dark:border-gray-600",
            JobStatus::Running => "border-blue-500",
            JobStatus::Completed => "border-green-500",
            JobStatus::Failed => "border-red-500",
        },
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ============================================================================
// Embedded HTML Templates
// ============================================================================

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en" class="dark:bg-gray-900">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>DPE Toolbox</title>
    <script src="https://unpkg.com/htmx.org@1.9.10"></script>
    <script src="https://unpkg.com/idiomorph@0.3.0/dist/idiomorph-ext.min.js"></script>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        tailwind.config = {
            darkMode: 'media'
        }
    </script>
    <style>
        .htmx-request { opacity: 0.7; }
        .job-card { transition: all 0.2s ease-in-out; }
        .job-output { scroll-behavior: smooth; }
        /* Prevent layout shift during swap */
        [hx-swap-oob] { display: contents; }
    </style>
</head>
<body class="bg-gray-100 dark:bg-gray-900 min-h-screen transition-colors">
    <div class="container mx-auto px-2 sm:px-4 py-4 sm:py-8 max-w-4xl">
        <!-- Header (clickable to return to landing page) -->
        <div class="text-center mb-6 sm:mb-8">
            <a href="/" class="inline-block hover:opacity-80 transition-opacity">
                <h1 class="text-2xl sm:text-3xl font-bold text-cyan-600 dark:text-cyan-400">DPE Toolbox</h1>
                <p class="text-gray-600 dark:text-gray-400 text-sm sm:text-base">Network Analysis Toolbox</p>
            </a>
        </div>

        <!-- Navigation Tabs -->
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow mb-4 sm:mb-6">
            <nav class="flex flex-wrap justify-center border-b dark:border-gray-700">
                <button hx-get="/partials/download-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-cyan-600 dark:hover:text-cyan-400 hover:bg-gray-50 dark:hover:bg-gray-700 border-b-2 border-transparent hover:border-cyan-600">
                    Download
                </button>
                <button hx-get="/partials/merge-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-cyan-600 dark:hover:text-cyan-400 hover:bg-gray-50 dark:hover:bg-gray-700 border-b-2 border-transparent hover:border-cyan-600">
                    PCAP Merge
                </button>
                <button hx-get="/partials/filter-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-cyan-600 dark:hover:text-cyan-400 hover:bg-gray-50 dark:hover:bg-gray-700 border-b-2 border-transparent hover:border-cyan-600">
                    PCAP Filter
                </button>
                <button hx-get="/partials/convert-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-cyan-600 dark:hover:text-cyan-400 hover:bg-gray-50 dark:hover:bg-gray-700 border-b-2 border-transparent hover:border-cyan-600">
                    ETL → PCAP
                </button>
                <button hx-get="/partials/tcpping-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-cyan-600 dark:hover:text-cyan-400 hover:bg-gray-50 dark:hover:bg-gray-700 border-b-2 border-transparent hover:border-cyan-600">
                    TCP Ping
                </button>
            </nav>
            
            <!-- Form Container -->
            <div id="form-container" class="p-4 sm:p-6">
                <!-- Landing page content -->
                <div class="text-center space-y-4 max-w-2xl mx-auto">
                    <p class="text-gray-600 dark:text-gray-300">A tool developed to simplify common tasks in datapath analysis and diagnostics for DPEs.</p>
                    
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 mt-6">
                        <div hx-get="/partials/download-form" hx-target="#form-container" hx-swap="innerHTML"
                             class="p-3 bg-gray-50 dark:bg-gray-700 rounded-lg text-center cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-600 transition-colors">
                            <h4 class="font-semibold text-cyan-600 dark:text-cyan-400">📥 Download</h4>
                            <p class="text-sm text-gray-600 dark:text-gray-400">Multi-threaded file downloads from URLs using azcopy</p>
                        </div>
                        <div hx-get="/partials/merge-form" hx-target="#form-container" hx-swap="innerHTML"
                             class="p-3 bg-gray-50 dark:bg-gray-700 rounded-lg text-center cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-600 transition-colors">
                            <h4 class="font-semibold text-cyan-600 dark:text-cyan-400">🔗 PCAP Merge</h4>
                            <p class="text-sm text-gray-600 dark:text-gray-400">Combine PCAP files by IP address using mergecap</p>
                        </div>
                        <div hx-get="/partials/filter-form" hx-target="#form-container" hx-swap="innerHTML"
                             class="p-3 bg-gray-50 dark:bg-gray-700 rounded-lg text-center cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-600 transition-colors">
                            <h4 class="font-semibold text-cyan-600 dark:text-cyan-400">🔍 PCAP Filter</h4>
                            <p class="text-sm text-gray-600 dark:text-gray-400">Apply Wireshark display filters to extract packets</p>
                        </div>
                        <div hx-get="/partials/convert-form" hx-target="#form-container" hx-swap="innerHTML"
                             class="p-3 bg-gray-50 dark:bg-gray-700 rounded-lg text-center cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-600 transition-colors">
                            <h4 class="font-semibold text-cyan-600 dark:text-cyan-400">🔄 ETL → PCAP</h4>
                            <p class="text-sm text-gray-600 dark:text-gray-400">Convert Windows ETL traces to PCAP format</p>
                        </div>
                        <div hx-get="/partials/tcpping-form" hx-target="#form-container" hx-swap="innerHTML"
                             class="p-3 bg-gray-50 dark:bg-gray-700 rounded-lg text-center cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-600 transition-colors sm:col-span-2 sm:max-w-xs sm:mx-auto">
                            <h4 class="font-semibold text-cyan-600 dark:text-cyan-400">🌐 TCP Ping</h4>
                            <p class="text-sm text-gray-600 dark:text-gray-400">Test TCP connectivity to hosts and ports</p>
                        </div>
                    </div>
                    
                    <p class="text-sm text-gray-500 dark:text-gray-400 mt-4">Click a tool above or use the tabs to get started.</p>
                    
                    <div class="pt-4 border-t border-gray-200 dark:border-gray-700 mt-6 space-y-2">
                        <p class="text-xs text-gray-400 dark:text-gray-500">Crafted by Diogo Esteves + GitHub Copilot</p>
                        <a href="https://github.com/diesteve_microsoft/dpetoolbox" target="_blank" rel="noopener noreferrer" 
                           class="inline-flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400 hover:text-cyan-600 dark:hover:text-cyan-400 transition-colors">
                            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
                            View on GitHub
                        </a>
                    </div>
                </div>
            </div>
        </div>

        <!-- Jobs List -->
        <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 sm:p-6">
            <h2 class="text-lg font-semibold mb-4 dark:text-white">Jobs</h2>
            <div id="jobs-list" hx-get="/partials/jobs" hx-trigger="load" hx-swap="innerHTML">
                <p class="text-gray-500 dark:text-gray-400 text-center">Loading...</p>
            </div>
        </div>
    </div>
</body>
</html>"##;

const DOWNLOAD_FORM_HTML: &str = r##"<form hx-post="/api/download" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-4 dark:text-white">Download Files</h3>
    <div class="space-y-4">
        <!-- Option 1: File path -->
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">URL List File (optional)</label>
            <input type="text" name="file_path" 
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white text-sm"
                placeholder="C:\path\to\urls.txt">
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">Path to a .txt file containing URLs (one per line)</p>
        </div>
        
        <!-- Divider -->
        <div class="relative">
            <div class="absolute inset-0 flex items-center">
                <div class="w-full border-t border-gray-300 dark:border-gray-600"></div>
            </div>
            <div class="relative flex justify-center text-sm">
                <span class="px-2 bg-white dark:bg-gray-800 text-gray-500 dark:text-gray-400">OR paste URLs directly</span>
            </div>
        </div>
        
        <!-- Option 2: Direct URLs -->
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">URLs (one per line)</label>
            <textarea name="urls" rows="8"
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white text-sm font-mono"
                placeholder="https://example.com/file1.pcap&#10;https://example.com/file2.pcap&#10;https://example.com/file3.pcap"></textarea>
        </div>
        
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Output Directory (optional)</label>
                <input type="text" name="output" 
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white text-sm"
                    placeholder="C:\Downloads">
            </div>
            <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Parallel Downloads</label>
                <input type="number" name="threads" value="4" min="1" max="16"
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white">
            </div>
        </div>
        <div class="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 rounded-md p-3 text-sm text-blue-800 dark:text-blue-300">
            <strong>📁 Save Location:</strong> Custom output takes precedence. If a URL list file is provided without custom path, files save to a subfolder with the same name (e.g., <code>test.txt</code> → <code>test\</code>). Otherwise, files save to the dpetoolbox.exe location.
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Download
        </button>
    </div>
</form>"##;

const MERGE_FORM_HTML: &str = r##"<form hx-post="/api/merge" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-2 dark:text-white">Merge PCAP Files by IP</h3>
    <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">Uses <strong>mergecap</strong> (Wireshark's CLI) to combine PCAP files that share the same IP address in their filename.</p>
    <div class="space-y-4">
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Input Directory</label>
            <input type="text" name="input" required
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                placeholder="C:\PCAPs">
        </div>
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Output Directory (optional)</label>
            <input type="text" name="output"
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                placeholder="Same as input">
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">If not specified, merged files are saved to the input directory</p>
        </div>
        <div class="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 rounded-md p-3 text-sm text-blue-800 dark:text-blue-300">
            <strong>📋 Filename pattern:</strong> Files must end with <code class="bg-blue-100 dark:bg-blue-800 px-1 rounded">_X.X.X.X.pcap</code> (e.g., <code class="bg-blue-100 dark:bg-blue-800 px-1 rounded">capture_10.0.0.1.pcap</code>). Files with the same IP are merged into <code class="bg-blue-100 dark:bg-blue-800 px-1 rounded">10.0.0.1_merged.pcap</code>.
        </div>
        <div class="bg-yellow-50 dark:bg-yellow-900/30 border border-yellow-200 dark:border-yellow-800 rounded-md p-3 text-sm text-yellow-800 dark:text-yellow-300">
            ⚠️ Requires Wireshark to be installed (provides mergecap)
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Merge
        </button>
    </div>
</form>"##;

const FILTER_FORM_HTML: &str = r##"<form hx-post="/api/filter" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-2 dark:text-white">PCAP Filtering</h3>
    <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">Uses <strong>tshark</strong> (Wireshark's CLI) to apply display filters to PCAP files, extracting only matching packets.</p>
    <div class="space-y-4">
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Input Directory</label>
            <input type="text" name="input" required
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                placeholder="C:\PCAPs">
        </div>
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Wireshark Display Filter</label>
            <input type="text" name="filter" required
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                placeholder="ip.src == 10.0.0.1">
        </div>
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Output Directory (optional)</label>
            <input type="text" name="output"
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                placeholder="Same as input">
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">If not specified, filtered files (_filtered.pcap) are saved to the input directory</p>
        </div>
        <div class="flex items-center">
            <input type="checkbox" name="delete_empty" id="delete_empty" class="h-4 w-4 text-cyan-600 rounded">
            <label for="delete_empty" class="ml-2 text-sm text-gray-700 dark:text-gray-300">Delete empty files (0 packets)</label>
        </div>
        <div class="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 rounded-md p-3 text-sm text-blue-800 dark:text-blue-300">
            <strong>🔧 Auto-decode protocols:</strong> VXLAN on ports 65330, 65530, 10000, 20000
        </div>
        <div class="bg-yellow-50 dark:bg-yellow-900/30 border border-yellow-200 dark:border-yellow-800 rounded-md p-3 text-sm text-yellow-800 dark:text-yellow-300">
            ⚠️ Requires Wireshark to be installed (provides tshark and capinfos)
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Filter
        </button>
    </div>
</form>"##;

const CONVERT_FORM_HTML: &str = r##"<form hx-post="/api/convert" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-2 dark:text-white">Convert ETL to PCAP</h3>
    <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">Uses <strong>etl2pcapng</strong> to convert Windows ETL network traces to PCAP format.</p>
    <div class="space-y-4">
        <!-- Single file option -->
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Single ETL File (optional)</label>
            <input type="text" name="single_file" 
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white text-sm"
                placeholder="C:\path\to\file.etl">
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">Convert a single ETL file</p>
        </div>
        
        <!-- Divider -->
        <div class="relative">
            <div class="absolute inset-0 flex items-center">
                <div class="w-full border-t border-gray-300 dark:border-gray-600"></div>
            </div>
            <div class="relative flex justify-center text-sm">
                <span class="px-2 bg-white dark:bg-gray-800 text-gray-500 dark:text-gray-400">OR convert entire directory</span>
            </div>
        </div>
        
        <!-- Directory option -->
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Input Directory (ETL files)</label>
            <input type="text" name="input" 
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                placeholder="C:\ETLs">
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">Convert all ETL files in directory</p>
        </div>
        
        <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Output Directory (optional)</label>
            <input type="text" name="output"
                class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                placeholder="Same as input">
            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">If not specified, PCAP files are saved to the same location as source</p>
        </div>
        <div class="bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 rounded-md p-3 text-sm text-blue-800 dark:text-blue-300">
            ℹ️ etl2pcapng will be auto-downloaded if not found
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Conversion
        </button>
    </div>
</form>"##;

const TCPPING_FORM_HTML: &str = r##"<form hx-post="/api/tcpping" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-4 dark:text-white">TCP Ping</h3>
    <div class="space-y-4">
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Target Host</label>
                <input type="text" name="target" required
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white text-sm"
                    placeholder="google.com">
            </div>
            <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Port</label>
                <input type="number" name="port" required min="1" max="65535"
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white"
                    placeholder="443">
            </div>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Timeout (ms)</label>
                <input type="number" name="timeout" value="2000" min="100"
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white">
            </div>
            <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">Interval (seconds)</label>
                <input type="number" name="interval" value="1" min="1"
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:ring-cyan-500 focus:border-cyan-500 dark:bg-gray-700 dark:text-white">
            </div>
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start TCP Ping
        </button>
    </div>
</form>"##;
