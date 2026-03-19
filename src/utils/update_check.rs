use serde::Deserialize;
use std::sync::OnceLock;

const CURRENT_VERSION: &str = "2.2.0";
const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/daesteves/dpetoolbox/releases";

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    prerelease: bool,
    html_url: String,
}

#[derive(Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub download_url: String,
    pub is_prerelease: bool,
}

static UPDATE_INFO: OnceLock<Option<UpdateInfo>> = OnceLock::new();

/// Check if current version is a dev/pre-release build
fn is_prerelease_version(version: &str) -> bool {
    version.contains("-dev") || version.contains("-rc") || version.contains("-beta") || version.contains("-alpha")
}

/// Compare version strings (strips leading 'v' and any pre-release suffix for ordering)
fn version_is_newer(latest: &str, current: &str) -> bool {
    let clean = |v: &str| -> Vec<u32> {
        v.trim_start_matches('v')
            .split('-')
            .next()
            .unwrap_or(v)
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };

    let latest_parts = clean(latest);
    let current_parts = clean(current);

    for (l, c) in latest_parts.iter().zip(current_parts.iter()) {
        if l > c { return true; }
        if l < c { return false; }
    }

    // Same base version — if current is pre-release and latest is stable, latest is newer
    if is_prerelease_version(current) && !is_prerelease_version(latest) {
        return true;
    }

    // Same base version, both pre-release — compare suffixes
    if is_prerelease_version(current) && is_prerelease_version(latest) {
        return latest.trim_start_matches('v') > current.trim_start_matches('v');
    }

    latest_parts.len() > current_parts.len()
}

/// Fetch releases from GitHub and determine if an update is available.
/// Returns None if no update or on any error.
async fn fetch_update() -> Option<UpdateInfo> {
    let client = reqwest::Client::builder()
        .user_agent("dpetoolbox-update-check")
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let releases: Vec<GitHubRelease> = client
        .get(GITHUB_RELEASES_URL)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let current_is_prerelease = is_prerelease_version(CURRENT_VERSION);

    // Find the latest applicable release
    let candidate = if current_is_prerelease {
        // Pre-release users: show any newer release (stable or pre-release)
        releases.iter().find(|r| version_is_newer(&r.tag_name, CURRENT_VERSION))
    } else {
        // Stable users: only show newer stable releases
        releases.iter().find(|r| !r.prerelease && version_is_newer(&r.tag_name, CURRENT_VERSION))
    };

    candidate.map(|r| UpdateInfo {
        latest_version: r.tag_name.clone(),
        download_url: r.html_url.clone(),
        is_prerelease: r.prerelease,
    })
}

/// Spawn a background update check. Non-blocking, fails silently.
pub fn spawn_update_check() {
    tokio::spawn(async {
        let result = fetch_update().await;
        UPDATE_INFO.set(result).ok();
    });
}

/// Get the cached update info (if available and check completed)
pub fn get_update_info() -> Option<&'static UpdateInfo> {
    UPDATE_INFO.get().and_then(|o| o.as_ref())
}

/// Print update notification to CLI (if available)
pub fn print_cli_update_notice() {
    if let Some(info) = get_update_info() {
        let label = if info.is_prerelease { "pre-release" } else { "stable" };
        eprintln!();
        eprintln!(
            "  ** New {} available: {} (current: {}) **",
            label, info.latest_version, CURRENT_VERSION
        );
        eprintln!("     Download: {}", info.download_url);
        eprintln!();
    }
}

/// Get update info as an HTML banner for the Web UI (or empty string)
pub fn get_web_update_banner() -> String {
    match get_update_info() {
        Some(info) => {
            let label = if info.is_prerelease { "pre-release" } else { "stable release" };
            format!(
                r##"<div style="background:#ecfdf5; border:1px solid #6ee7b7; border-radius:8px; padding:12px 16px; margin-bottom:16px; display:flex; align-items:center; justify-content:space-between; font-size:14px; color:#065f46;">
                    <span>New {label} available: <strong>{version}</strong> (current: {current})</span>
                    <a href="{url}" target="_blank" rel="noopener noreferrer"
                       style="margin-left:12px; padding:6px 14px; background:#059669; color:#fff; border-radius:6px; text-decoration:none; font-size:12px; font-weight:500; white-space:nowrap;">
                        Download
                    </a>
                </div>"##,
                label = label,
                version = info.latest_version,
                current = CURRENT_VERSION,
                url = info.download_url,
            )
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(version_is_newer("v2.1.0", "2.0.2"));
        assert!(version_is_newer("v3.0.0", "2.2.0-dev1"));
        assert!(version_is_newer("2.2.0", "2.2.0-dev1"));
        assert!(!version_is_newer("2.0.0", "2.0.2"));
        assert!(!version_is_newer("2.2.0-dev1", "2.2.0-dev1"));
        assert!(version_is_newer("2.2.0-dev2", "2.2.0-dev1"));
    }

    #[test]
    fn test_is_prerelease() {
        assert!(is_prerelease_version("2.2.0-dev1"));
        assert!(is_prerelease_version("2.2.0-beta1"));
        assert!(!is_prerelease_version("2.1.0"));
    }
}
