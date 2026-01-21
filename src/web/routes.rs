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
    urls: String,
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
    let output = form.output.clone();
    let threads = form.threads.unwrap_or(4);
    
    tokio::spawn(async move {
        run_download_job(state_clone, &job_id, &urls, output.as_deref(), threads).await;
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
    input: String,
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
    let output = form.output.clone();
    
    tokio::spawn(async move {
        run_convert_job(state_clone, &job_id, &input, output.as_deref()).await;
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

async fn run_download_job(state: AppState, job_id: &str, urls: &str, output: Option<&str>, threads: u32) {
    use crate::utils::tools::ensure_azcopy;
    
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

    // Parse URLs
    let url_list: Vec<&str> = urls.lines()
        .map(|l| l.trim())
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
        job.message = format!("Downloading {} files...", total_files);
        job.output.push(format!("Found {} URLs to download", total_files));
    });

    // Determine output directory
    let output_dir = if let Some(out) = output {
        if !out.trim().is_empty() {
            std::path::PathBuf::from(out)
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
    });

    // Create output directory if needed
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        state.update_job(job_id, |job| {
            job.status = JobStatus::Failed;
            job.message = format!("Failed to create output directory: {}", e);
        });
        return;
    }

    let mut success_count = 0;
    let mut skip_count = 0;
    let mut fail_count = 0;

    for (i, url) in url_list.iter().enumerate() {
        // Extract filename from URL
        let filename = url.split('/').last().unwrap_or("file")
            .split('?').next().unwrap_or("file");
        let output_path = output_dir.join(filename);

        state.update_job(job_id, |job| {
            job.progress = ((i * 100) / total_files) as u32;
            job.message = format!("Downloading {}/{}: {}", i + 1, total_files, filename);
        });

        // Check if file already exists
        if output_path.exists() {
            state.update_job(job_id, |job| {
                job.output.push(format!("[{}/{}] SKIP: {} (already exists)", i + 1, total_files, filename));
            });
            skip_count += 1;
            continue;
        }

        // Run azcopy
        let output = tokio::process::Command::new(&azcopy_path)
            .args(["copy", url, output_path.to_str().unwrap()])
            .output()
            .await;

        match output {
            Ok(result) => {
                if result.status.success() {
                    state.update_job(job_id, |job| {
                        job.output.push(format!("[{}/{}] OK: {}", i + 1, total_files, filename));
                    });
                    success_count += 1;
                } else {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    state.update_job(job_id, |job| {
                        job.output.push(format!("[{}/{}] FAIL: {} - {}", i + 1, total_files, filename, stderr.lines().next().unwrap_or("unknown error")));
                    });
                    fail_count += 1;
                }
            }
            Err(e) => {
                state.update_job(job_id, |job| {
                    job.output.push(format!("[{}/{}] ERROR: {} - {}", i + 1, total_files, filename, e));
                });
                fail_count += 1;
            }
        }
    }

    // Final summary
    state.update_job(job_id, |job| {
        job.progress = 100;
        job.output.push("─".repeat(40));
        job.output.push(format!("Summary: {} success, {} skipped, {} failed", success_count, skip_count, fail_count));
        
        if fail_count == 0 {
            job.status = JobStatus::Completed;
            job.message = format!("Download completed: {} files ({} skipped)", success_count, skip_count);
        } else {
            job.status = JobStatus::Failed;
            job.message = format!("Download finished with {} failures", fail_count);
        }
    });
}

async fn run_merge_job(state: AppState, job_id: &str, input: &str, output: Option<&str>) {
    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = "Merging PCAP files...".to_string();
    });

    let result = crate::commands::merge::run(input, output);

    state.update_job(job_id, |job| {
        match result {
            Ok(_) => {
                job.status = JobStatus::Completed;
                job.progress = 100;
                job.message = "Merge completed successfully".to_string();
            }
            Err(e) => {
                job.status = JobStatus::Failed;
                job.message = format!("Merge failed: {}", e);
            }
        }
    });
}

async fn run_filter_job(state: AppState, job_id: &str, input: &str, output: Option<&str>, filter: &str, delete_empty: bool) {
    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = "Filtering PCAP files...".to_string();
    });

    let result = crate::commands::filter::run(input, output, filter, delete_empty);

    state.update_job(job_id, |job| {
        match result {
            Ok(_) => {
                job.status = JobStatus::Completed;
                job.progress = 100;
                job.message = "Filter completed successfully".to_string();
            }
            Err(e) => {
                job.status = JobStatus::Failed;
                job.message = format!("Filter failed: {}", e);
            }
        }
    });
}

async fn run_convert_job(state: AppState, job_id: &str, input: &str, output: Option<&str>) {
    state.update_job(job_id, |job| {
        job.status = JobStatus::Running;
        job.message = "Converting ETL files...".to_string();
    });

    let result = crate::commands::convert::run(input, output).await;

    state.update_job(job_id, |job| {
        match result {
            Ok(_) => {
                job.status = JobStatus::Completed;
                job.progress = 100;
                job.message = "Conversion completed successfully".to_string();
            }
            Err(e) => {
                job.status = JobStatus::Failed;
                job.message = format!("Conversion failed: {}", e);
            }
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
        html.push_str(r##"<p class="text-gray-500 text-center py-4">No jobs yet. Start a task from the menu above.</p>"##);
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
        JobStatus::Pending => "bg-gray-100 text-gray-800",
        JobStatus::Running => "bg-blue-100 text-blue-800 animate-pulse",
        JobStatus::Completed => "bg-green-100 text-green-800",
        JobStatus::Failed => "bg-red-100 text-red-800",
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
        format!(r##"<div class="mt-2 bg-gray-900 text-green-400 p-2 rounded text-xs font-mono max-h-48 overflow-y-auto job-output" id="output-{}">{}</div>"##, job.id, lines)
    } else {
        String::new()
    };

    let stop_button = if job.job_type == "tcpping" && job.status == JobStatus::Running {
        format!(r##"<button hx-post="/api/tcpping/{}/stop" hx-swap="none" class="mt-2 text-red-600 hover:text-red-800 text-sm font-medium">⏹ Stop</button>"##, job.id)
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
        r##"<div class="job-card bg-white rounded-lg shadow p-4 mb-3 border-l-4 {border_color}" id="job-{id}" {poll_attr}>
            <div class="flex flex-col sm:flex-row sm:justify-between sm:items-start gap-1">
                <div>
                    <span class="font-semibold capitalize">{job_type}</span>
                    <span class="ml-2 px-2 py-1 rounded text-xs {status_class}">{status_text}</span>
                </div>
                <div class="text-xs text-gray-500">{created_at}</div>
            </div>
            <p class="text-sm text-gray-600 mt-2">{message}</p>
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
            JobStatus::Pending => "border-gray-300",
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
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>DPE Toolbox</title>
    <script src="https://unpkg.com/htmx.org@1.9.10"></script>
    <script src="https://unpkg.com/idiomorph@0.3.0/dist/idiomorph-ext.min.js"></script>
    <script src="https://cdn.tailwindcss.com"></script>
    <style>
        .htmx-request { opacity: 0.7; }
        .job-card { transition: all 0.2s ease-in-out; }
        .job-output { scroll-behavior: smooth; }
        /* Prevent layout shift during swap */
        [hx-swap-oob] { display: contents; }
    </style>
</head>
<body class="bg-gray-100 min-h-screen">
    <div class="container mx-auto px-2 sm:px-4 py-4 sm:py-8 max-w-4xl">
        <!-- Header -->
        <div class="text-center mb-6 sm:mb-8">
            <h1 class="text-2xl sm:text-3xl font-bold text-cyan-600">DPE Toolbox</h1>
            <p class="text-gray-600 text-sm sm:text-base">Network Analysis Toolbox</p>
        </div>

        <!-- Navigation Tabs -->
        <div class="bg-white rounded-lg shadow mb-4 sm:mb-6">
            <nav class="flex flex-wrap border-b">
                <button hx-get="/partials/download-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="flex-1 sm:flex-none px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 hover:text-cyan-600 hover:bg-gray-50 border-b-2 border-transparent hover:border-cyan-600">
                    Download
                </button>
                <button hx-get="/partials/merge-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="flex-1 sm:flex-none px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 hover:text-cyan-600 hover:bg-gray-50 border-b-2 border-transparent hover:border-cyan-600">
                    Merge
                </button>
                <button hx-get="/partials/filter-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="flex-1 sm:flex-none px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 hover:text-cyan-600 hover:bg-gray-50 border-b-2 border-transparent hover:border-cyan-600">
                    Filter
                </button>
                <button hx-get="/partials/convert-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="flex-1 sm:flex-none px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 hover:text-cyan-600 hover:bg-gray-50 border-b-2 border-transparent hover:border-cyan-600">
                    Convert
                </button>
                <button hx-get="/partials/tcpping-form" hx-target="#form-container" hx-swap="innerHTML"
                        class="flex-1 sm:flex-none px-3 sm:px-4 py-2 sm:py-3 text-xs sm:text-sm font-medium text-gray-700 hover:text-cyan-600 hover:bg-gray-50 border-b-2 border-transparent hover:border-cyan-600">
                    TCP Ping
                </button>
            </nav>
            
            <!-- Form Container -->
            <div id="form-container" class="p-4 sm:p-6">
                <p class="text-gray-500 text-center">Select a tool from the menu above to get started.</p>
            </div>
        </div>

        <!-- Jobs List -->
        <div class="bg-white rounded-lg shadow p-4 sm:p-6">
            <h2 class="text-lg font-semibold mb-4">Jobs</h2>
            <div id="jobs-list" hx-get="/partials/jobs" hx-trigger="load" hx-swap="innerHTML">
                <p class="text-gray-500 text-center">Loading...</p>
            </div>
        </div>
    </div>
</body>
</html>"##;

const DOWNLOAD_FORM_HTML: &str = r##"<form hx-post="/api/download" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-4">Download Files</h3>
    <div class="space-y-4">
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">URLs (one per line)</label>
            <textarea name="urls" rows="5" required
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500 text-sm"
                placeholder="https://example.com/file1.pcap&#10;https://example.com/file2.pcap"></textarea>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
                <label class="block text-sm font-medium text-gray-700 mb-1">Output Directory (optional)</label>
                <input type="text" name="output" 
                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500 text-sm"
                    placeholder="C:\Downloads">
            </div>
            <div>
                <label class="block text-sm font-medium text-gray-700 mb-1">Parallel Downloads</label>
                <input type="number" name="threads" value="4" min="1" max="16"
                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500">
            </div>
        </div>
        <div class="bg-blue-50 border border-blue-200 rounded-md p-3 text-sm text-blue-800">
            <strong>📁 Save Location:</strong> If no output directory is specified, files will be saved to the same folder where dpetoolbox.exe is located.
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Download
        </button>
    </div>
</form>"##;

const MERGE_FORM_HTML: &str = r##"<form hx-post="/api/merge" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-4">Merge PCAP Files by IP</h3>
    <div class="space-y-4">
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Input Directory</label>
            <input type="text" name="input" required
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                placeholder="C:\PCAPs">
        </div>
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Output Directory (optional)</label>
            <input type="text" name="output"
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                placeholder="Same as input">
        </div>
        <div class="bg-yellow-50 border border-yellow-200 rounded-md p-3 text-sm text-yellow-800">
            ⚠️ Requires Wireshark to be installed
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Merge
        </button>
    </div>
</form>"##;

const FILTER_FORM_HTML: &str = r##"<form hx-post="/api/filter" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-4">Filter PCAP Files</h3>
    <div class="space-y-4">
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Input Directory</label>
            <input type="text" name="input" required
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                placeholder="C:\PCAPs">
        </div>
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Wireshark Display Filter</label>
            <input type="text" name="filter" required
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                placeholder="ip.src == 10.0.0.1">
        </div>
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Output Directory (optional)</label>
            <input type="text" name="output"
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                placeholder="Same as input">
        </div>
        <div class="flex items-center">
            <input type="checkbox" name="delete_empty" id="delete_empty" class="h-4 w-4 text-cyan-600 rounded">
            <label for="delete_empty" class="ml-2 text-sm text-gray-700">Delete empty files (0 packets)</label>
        </div>
        <div class="bg-yellow-50 border border-yellow-200 rounded-md p-3 text-sm text-yellow-800">
            ⚠️ Requires Wireshark to be installed
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Filter
        </button>
    </div>
</form>"##;

const CONVERT_FORM_HTML: &str = r##"<form hx-post="/api/convert" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-4">Convert ETL to PCAP</h3>
    <div class="space-y-4">
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Input Directory (ETL files)</label>
            <input type="text" name="input" required
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                placeholder="C:\ETLs">
        </div>
        <div>
            <label class="block text-sm font-medium text-gray-700 mb-1">Output Directory (optional)</label>
            <input type="text" name="output"
                class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                placeholder="Same as input">
        </div>
        <div class="bg-blue-50 border border-blue-200 rounded-md p-3 text-sm text-blue-800">
            ℹ️ etl2pcapng will be auto-downloaded if not found
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start Conversion
        </button>
    </div>
</form>"##;

const TCPPING_FORM_HTML: &str = r##"<form hx-post="/api/tcpping" hx-target="#jobs-list" hx-swap="afterbegin">
    <h3 class="text-lg font-semibold mb-4">TCP Ping</h3>
    <div class="space-y-4">
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
                <label class="block text-sm font-medium text-gray-700 mb-1">Target Host</label>
                <input type="text" name="target" required
                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500 text-sm"
                    placeholder="google.com">
            </div>
            <div>
                <label class="block text-sm font-medium text-gray-700 mb-1">Port</label>
                <input type="number" name="port" required min="1" max="65535"
                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500"
                    placeholder="443">
            </div>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div>
                <label class="block text-sm font-medium text-gray-700 mb-1">Timeout (ms)</label>
                <input type="number" name="timeout" value="2000" min="100"
                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500">
            </div>
            <div>
                <label class="block text-sm font-medium text-gray-700 mb-1">Interval (seconds)</label>
                <input type="number" name="interval" value="1" min="1"
                    class="w-full px-3 py-2 border border-gray-300 rounded-md focus:ring-cyan-500 focus:border-cyan-500">
            </div>
        </div>
        <button type="submit" class="w-full bg-cyan-600 text-white py-2 px-4 rounded-md hover:bg-cyan-700 transition">
            Start TCP Ping
        </button>
    </div>
</form>"##;
