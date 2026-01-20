use anyhow::{Context, Result};
use colored::Colorize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const AZCOPY_DOWNLOAD_URL: &str = "https://aka.ms/downloadazcopy-v10-windows";

/// Get the application data directory for storing tools
pub fn get_app_data_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir()
        .context("Could not find local app data directory")?
        .join("dpetoolbox");
    
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .context("Failed to create app data directory")?;
    }
    
    Ok(data_dir)
}

/// Find an executable by checking PATH and common locations
pub fn find_executable(name: &str, default_paths: &[&str]) -> Option<PathBuf> {
    // Check if command is in PATH
    if let Ok(output) = Command::new("where.exe").arg(name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = path.lines().next() {
                let path = PathBuf::from(first_line.trim());
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }
    
    // Check default installation paths
    for path_str in default_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            return Some(path);
        }
    }
    
    None
}

/// Find mergecap executable (part of Wireshark)
pub fn find_mergecap() -> Option<PathBuf> {
    find_executable("mergecap", &[
        r"C:\Program Files\Wireshark\mergecap.exe",
        r"C:\Program Files (x86)\Wireshark\mergecap.exe",
    ])
}

/// Ensure mergecap is available
pub fn ensure_mergecap() -> Result<PathBuf> {
    if let Some(path) = find_mergecap() {
        println!("{} mergecap ({})", "Found:".green(), path.display());
        return Ok(path);
    }
    
    anyhow::bail!(
        "mergecap not found. Please install Wireshark from https://www.wireshark.org/download.html"
    )
}

/// Find tshark executable (part of Wireshark)
pub fn find_tshark() -> Option<PathBuf> {
    find_executable("tshark", &[
        r"C:\Program Files\Wireshark\tshark.exe",
        r"C:\Program Files (x86)\Wireshark\tshark.exe",
    ])
}

/// Ensure tshark is available
pub fn ensure_tshark() -> Result<PathBuf> {
    if let Some(path) = find_tshark() {
        println!("{} tshark ({})", "Found:".green(), path.display());
        return Ok(path);
    }
    
    anyhow::bail!(
        "tshark not found. Please install Wireshark from https://www.wireshark.org/download.html"
    )
}

/// Find capinfos executable (part of Wireshark)
pub fn find_capinfos() -> Option<PathBuf> {
    find_executable("capinfos", &[
        r"C:\Program Files\Wireshark\capinfos.exe",
        r"C:\Program Files (x86)\Wireshark\capinfos.exe",
    ])
}

/// Ensure capinfos is available
pub fn ensure_capinfos() -> Result<PathBuf> {
    if let Some(path) = find_capinfos() {
        println!("{} capinfos ({})", "Found:".green(), path.display());
        return Ok(path);
    }
    
    anyhow::bail!(
        "capinfos not found. Please install Wireshark from https://www.wireshark.org/download.html"
    )
}

/// Find azcopy executable - checks PATH first, then app data directory
pub fn find_azcopy() -> Option<PathBuf> {
    // Check if azcopy is in PATH
    if let Ok(output) = Command::new("where.exe").arg("azcopy").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = path.lines().next() {
                let path = PathBuf::from(first_line.trim());
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }
    
    // Check common installation paths
    let common_paths = [
        PathBuf::from(r"C:\ProgramData\chocolatey\lib\azcopy10\tools\azcopy\azcopy.exe"),
        PathBuf::from(env::var("TEMP").unwrap_or_default()).join("azcopy").join("azcopy.exe"),
    ];
    
    for path in &common_paths {
        if path.exists() {
            return Some(path.clone());
        }
    }
    
    // Check app data directory
    if let Ok(app_dir) = get_app_data_dir() {
        let azcopy_path = app_dir.join("azcopy").join("azcopy.exe");
        if azcopy_path.exists() {
            return Some(azcopy_path);
        }
    }
    
    None
}

/// Download and extract azcopy to app data directory
pub async fn download_azcopy() -> Result<PathBuf> {
    let app_dir = get_app_data_dir()?;
    let azcopy_dir = app_dir.join("azcopy");
    let azcopy_exe = azcopy_dir.join("azcopy.exe");
    
    if azcopy_exe.exists() {
        return Ok(azcopy_exe);
    }
    
    println!("{}", "Downloading azcopy...".yellow());
    
    // Download the zip file
    let zip_path = app_dir.join("azcopy.zip");
    
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;
    
    let response = client.get(AZCOPY_DOWNLOAD_URL)
        .send()
        .await
        .context("Failed to download azcopy")?;
    
    let bytes = response.bytes().await?;
    fs::write(&zip_path, &bytes).context("Failed to save azcopy zip")?;
    
    println!("{}", "Extracting azcopy...".yellow());
    
    // Extract the zip file
    let file = fs::File::open(&zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    
    // Create temp extraction directory
    let extract_dir = app_dir.join("azcopy_temp");
    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir)?;
    }
    fs::create_dir_all(&extract_dir)?;
    
    // Extract all files
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = extract_dir.join(file.name());
        
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            let mut outfile = fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }
    
    // Find azcopy.exe in extracted files
    if azcopy_dir.exists() {
        fs::remove_dir_all(&azcopy_dir)?;
    }
    fs::create_dir_all(&azcopy_dir)?;
    
    // Look for azcopy.exe recursively
    fn find_exe_recursive(dir: &PathBuf) -> Option<PathBuf> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.file_name().map(|n| n.to_string_lossy().to_lowercase()) == Some("azcopy.exe".to_string()) {
                    return Some(path);
                } else if path.is_dir() {
                    if let Some(found) = find_exe_recursive(&path) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    
    if let Some(found_exe) = find_exe_recursive(&extract_dir) {
        fs::copy(&found_exe, &azcopy_exe)?;
    } else {
        anyhow::bail!("Could not find azcopy.exe in downloaded archive");
    }
    
    // Cleanup
    fs::remove_file(&zip_path).ok();
    fs::remove_dir_all(&extract_dir).ok();
    
    // Verify installation
    let output = Command::new(&azcopy_exe).arg("--version").output()?;
    let version = String::from_utf8_lossy(&output.stdout);
    println!("{} {}", "Installed:".green(), version.trim());
    
    Ok(azcopy_exe)
}

/// Ensure azcopy is available, downloading if necessary
pub async fn ensure_azcopy() -> Result<PathBuf> {
    if let Some(path) = find_azcopy() {
        // Verify it works
        if let Ok(output) = Command::new(&path).arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout);
                println!("{} {} ({})", "Found:".green(), version.trim(), path.display());
                return Ok(path);
            }
        }
    }
    
    println!("{}", "azcopy not found on system.".yellow());
    download_azcopy().await
}
