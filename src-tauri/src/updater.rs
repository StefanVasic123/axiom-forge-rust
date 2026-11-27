use serde_json::{json, Value};

pub const MANDATORY_BEHIND_THRESHOLD: u64 = 10;
pub const GITHUB_RELEASES_API_URL: &str =
    "https://api.github.com/repos/axon-forge/axon-forge/releases?per_page=30";
pub const RELEASES_PAGE_URL: &str =
    "https://github.com/axon-forge/axon-forge/releases/latest";

pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let core = stripped.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn normalize_version(raw: &str) -> Option<String> {
    let (maj, min, pat) = parse_semver(raw)?;
    Some(format!("{}.{}.{}", maj, min, pat))
}

pub fn compute_semver_gap(installed: (u64, u64, u64), latest: (u64, u64, u64)) -> u64 {
    if installed >= latest {
        return 0;
    }
    if latest.0 == installed.0 && latest.1 == installed.1 {
        return latest.2.saturating_sub(installed.2);
    }
    let major_diff = latest.0.saturating_sub(installed.0);
    let minor_diff = latest.1.saturating_sub(installed.1);
    let patch_diff = latest.2.saturating_sub(installed.2);
    let weighted = major_diff
        .saturating_mul(100)
        .saturating_add(minor_diff.saturating_mul(10))
        .saturating_add(patch_diff);
    weighted.max(MANDATORY_BEHIND_THRESHOLD)
}

pub fn evaluate_desktop_version_status(
    installed_raw: &str,
    latest_raw: &str,
    recent_versions_raw: &[String],
    min_version_raw: Option<&str>,
    download_url: &str,
    release_name: Option<&str>,
    published_at: Option<&str>,
) -> Value {
    let Some(installed_tuple) = parse_semver(installed_raw) else {
        return json!({
            "checked": false,
            "status": "unknown",
            "installedVersion": installed_raw,
            "mandatoryThreshold": MANDATORY_BEHIND_THRESHOLD,
            "downloadUrl": download_url,
        });
    };
    let Some(latest_tuple) = parse_semver(latest_raw) else {
        return json!({
            "checked": false,
            "status": "unknown",
            "installedVersion": format!("{}.{}.{}", installed_tuple.0, installed_tuple.1, installed_tuple.2),
            "mandatoryThreshold": MANDATORY_BEHIND_THRESHOLD,
            "downloadUrl": download_url,
        });
    };

    let installed_norm = format!("{}.{}.{}", installed_tuple.0, installed_tuple.1, installed_tuple.2);
    let latest_norm = format!("{}.{}.{}", latest_tuple.0, latest_tuple.1, latest_tuple.2);

    let mut recent_parsed: Vec<(u64, u64, u64)> = recent_versions_raw
        .iter()
        .filter_map(|v| parse_semver(v))
        .collect();
    if !recent_parsed.contains(&latest_tuple) {
        recent_parsed.push(latest_tuple);
    }
    recent_parsed.sort_by(|a, b| b.cmp(a));
    recent_parsed.dedup();

    let explicit_min_tuple = min_version_raw.and_then(parse_semver);

    let window_min_tuple = if recent_parsed.len() as u64 >= MANDATORY_BEHIND_THRESHOLD {
        Some(recent_parsed[(MANDATORY_BEHIND_THRESHOLD - 1) as usize])
    } else if latest_tuple.2 >= MANDATORY_BEHIND_THRESHOLD - 1 {
        Some((
            latest_tuple.0,
            latest_tuple.1,
            latest_tuple.2 - (MANDATORY_BEHIND_THRESHOLD - 1),
        ))
    } else {
        None
    };

    let effective_min_tuple = match (window_min_tuple, explicit_min_tuple) {
        (Some(w), Some(e)) => Some(if e > w { e } else { w }),
        (Some(w), None) => Some(w),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    };
    let min_required_str = effective_min_tuple.map(|(a, b, c)| format!("{}.{}.{}", a, b, c));

    if installed_tuple >= latest_tuple {
        return json!({
            "checked": true,
            "status": "up_to_date",
            "installedVersion": installed_norm,
            "latestVersion": latest_norm,
            "minRequiredVersion": min_required_str,
            "versionsBehind": 0,
            "mandatoryThreshold": MANDATORY_BEHIND_THRESHOLD,
            "downloadUrl": download_url,
            "releaseName": release_name,
            "publishedAt": published_at,
        });
    }

    let versions_behind: u64 = if let Some(idx) = recent_parsed.iter().position(|v| *v == installed_tuple) {
        idx as u64
    } else if let Some(oldest) = recent_parsed.last() {
        if installed_tuple < *oldest {
            (recent_parsed.len() as u64).max(compute_semver_gap(installed_tuple, latest_tuple))
        } else {
            compute_semver_gap(installed_tuple, latest_tuple).max(1)
        }
    } else {
        compute_semver_gap(installed_tuple, latest_tuple).max(1)
    };

    let below_explicit_min = explicit_min_tuple.map(|m| installed_tuple < m).unwrap_or(false);
    let is_mandatory = versions_behind >= MANDATORY_BEHIND_THRESHOLD || below_explicit_min;
    let status = if is_mandatory {
        "update_required"
    } else {
        "update_recommended"
    };

    json!({
        "checked": true,
        "status": status,
        "installedVersion": installed_norm,
        "latestVersion": latest_norm,
        "minRequiredVersion": min_required_str,
        "versionsBehind": versions_behind,
        "mandatoryThreshold": MANDATORY_BEHIND_THRESHOLD,
        "downloadUrl": download_url,
        "releaseName": release_name,
        "publishedAt": published_at,
    })
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

async fn fetch_latest_desktop_update_inner(
    installed: &str,
    api_host: Option<String>,
) -> Value {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(2500))
        .user_agent(format!("Axiom-Forge-Desktop/{}", installed))
        .build()
    else {
        return json!({
            "checked": false,
            "status": "offline",
            "installedVersion": installed,
            "mandatoryThreshold": MANDATORY_BEHIND_THRESHOLD,
            "downloadUrl": RELEASES_PAGE_URL,
        });
    };

    // 1. Prefer the originating web host's cached /api/axiom/latest-version endpoint when provided
    if let Some(host_raw) = api_host {
        let host = host_raw.trim().trim_end_matches('/');
        if host.starts_with("http://") || host.starts_with("https://") {
            let endpoint = format!(
                "{}/api/axiom/latest-version?installed={}",
                host, installed
            );
            if let Ok(res) = client
                .get(&endpoint)
                .header("X-Axiom-Version", installed)
                .send()
                .await
            {
                if res.status().is_success() {
                    if let Ok(mut payload) = res.json::<Value>().await {
                        if let Some(eval) = payload.get_mut("evaluation").and_then(|v| v.as_object_mut()) {
                            eval.insert("checked".to_string(), json!(true));
                            return Value::Object(eval.clone());
                        }
                        if let Some(latest_v) = payload.get("latestVersion").and_then(|v| v.as_str()) {
                            let recent: Vec<String> = payload
                                .get("recentVersions")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();
                            let min_v = payload.get("minVersion").and_then(|v| v.as_str());
                            let dl = payload
                                .get("downloadUrl")
                                .and_then(|v| v.as_str())
                                .unwrap_or(RELEASES_PAGE_URL);
                            let r_name = payload.get("releaseName").and_then(|v| v.as_str());
                            let pub_at = payload.get("publishedAt").and_then(|v| v.as_str());
                            return evaluate_desktop_version_status(
                                installed, latest_v, &recent, min_v, dl, r_name, pub_at,
                            );
                        }
                    }
                }
            }
        }
    }

    // 2. Direct fallback to GitHub Releases API (works even without web host)
    if let Ok(res) = client
        .get(GITHUB_RELEASES_API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        if res.status().is_success() {
            if let Ok(items) = res.json::<Vec<Value>>().await {
                let mut valid: Vec<((u64, u64, u64), String, String, Option<String>, Option<String>)> = items
                    .into_iter()
                    .filter(|r| {
                        !r.get("draft").and_then(|v| v.as_bool()).unwrap_or(false)
                            && !r.get("prerelease").and_then(|v| v.as_bool()).unwrap_or(false)
                    })
                    .filter_map(|r| {
                        let tag = r.get("tag_name").and_then(|v| v.as_str())?;
                        let sem = parse_semver(tag)?;
                        let norm = format!("{}.{}.{}", sem.0, sem.1, sem.2);
                        let html_url = r
                            .get("html_url")
                            .and_then(|v| v.as_str())
                            .unwrap_or(RELEASES_PAGE_URL)
                            .to_string();
                        let name = r.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let pub_at = r.get("published_at").and_then(|v| v.as_str()).map(|s| s.to_string());
                        Some((sem, norm, html_url, name, pub_at))
                    })
                    .collect();

                valid.sort_by(|a, b| b.0.cmp(&a.0));
                if let Some(newest) = valid.first() {
                    let recent: Vec<String> = valid.iter().map(|v| v.1.clone()).collect();
                    return evaluate_desktop_version_status(
                        installed,
                        &newest.1,
                        &recent,
                        None,
                        &newest.2,
                        newest.3.as_deref(),
                        newest.4.as_deref(),
                    );
                }
            }
        }
    }

    // 3. Rate-limit-free fallback via GitHub releases/latest redirect Location header
    if let Ok(no_redir_client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(2000))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("Axiom-Forge-Desktop/{}", installed))
        .build()
    {
        if let Ok(res) = no_redir_client.head(RELEASES_PAGE_URL).send().await {
            if let Some(loc) = res.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()) {
                if let Some(idx) = loc.find("/releases/tag/") {
                    let raw_tag = loc[idx + "/releases/tag/".len()..].split(['?', '#']).next().unwrap_or("");
                    if let Some(norm) = normalize_version(raw_tag) {
                        let full_url = if loc.starts_with("http") {
                            loc.to_string()
                        } else {
                            format!("https://github.com{}", loc)
                        };
                        return evaluate_desktop_version_status(
                            installed,
                            &norm,
                            &[norm.clone()],
                            None,
                            &full_url,
                            None,
                            None,
                        );
                    }
                }
            }
        }
    }

    // 4. Offline / unreachable: fail open so users can still work offline
    json!({
        "checked": false,
        "status": "offline",
        "installedVersion": installed,
        "mandatoryThreshold": MANDATORY_BEHIND_THRESHOLD,
        "downloadUrl": RELEASES_PAGE_URL,
    })
}

#[tauri::command]
pub async fn check_desktop_update(api_host: Option<String>) -> Result<Value, String> {
    let installed = env!("CARGO_PKG_VERSION");
    match tokio::time::timeout(
        std::time::Duration::from_secs(3),
        fetch_latest_desktop_update_inner(installed, api_host),
    )
    .await
    {
        Ok(val) => Ok(val),
        Err(_) => Ok(json!({
            "checked": false,
            "status": "offline",
            "installedVersion": installed,
            "mandatoryThreshold": MANDATORY_BEHIND_THRESHOLD,
            "downloadUrl": RELEASES_PAGE_URL,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent_patch_list() -> Vec<String> {
        (50..=62).rev().map(|p| format!("0.1.{}", p)).collect()
    }

    #[test]
    fn test_desktop_version_up_to_date_and_newer() {
        let recent = recent_patch_list();
        let exact = evaluate_desktop_version_status("0.1.62", "0.1.62", &recent, None, RELEASES_PAGE_URL, None, None);
        assert_eq!(exact["status"], "up_to_date");
        assert_eq!(exact["versionsBehind"], 0);

        let dev_newer = evaluate_desktop_version_status("0.1.63", "0.1.62", &recent, None, RELEASES_PAGE_URL, None, None);
        assert_eq!(dev_newer["status"], "up_to_date");
        assert_eq!(dev_newer["versionsBehind"], 0);
    }

    #[test]
    fn test_desktop_version_soft_recommendation_1_to_9_behind() {
        let recent = recent_patch_list();
        let one_behind = evaluate_desktop_version_status("0.1.61", "0.1.62", &recent, None, RELEASES_PAGE_URL, None, None);
        assert_eq!(one_behind["status"], "update_recommended");
        assert_eq!(one_behind["versionsBehind"], 1);

        let nine_behind = evaluate_desktop_version_status("0.1.53", "0.1.62", &recent, None, RELEASES_PAGE_URL, None, None);
        assert_eq!(nine_behind["status"], "update_recommended");
        assert_eq!(nine_behind["versionsBehind"], 9);
    }

    #[test]
    fn test_desktop_version_mandatory_10_or_more_behind() {
        let recent = recent_patch_list();
        let ten_behind = evaluate_desktop_version_status("0.1.52", "0.1.62", &recent, None, RELEASES_PAGE_URL, None, None);
        assert_eq!(ten_behind["status"], "update_required");
        assert_eq!(ten_behind["versionsBehind"], 10);

        let older_outside_window = evaluate_desktop_version_status("0.1.40", "0.1.62", &recent, None, RELEASES_PAGE_URL, None, None);
        assert_eq!(older_outside_window["status"], "update_required");
        assert!(older_outside_window["versionsBehind"].as_u64().unwrap() >= 10);
    }

    #[test]
    fn test_desktop_version_minor_bump_and_explicit_min_override() {
        let recent = vec!["0.2.0".to_string(), "0.1.62".to_string(), "0.1.61".to_string()];
        let minor_one_behind = evaluate_desktop_version_status("0.1.62", "0.2.0", &recent, None, RELEASES_PAGE_URL, None, None);
        assert_eq!(minor_one_behind["status"], "update_recommended");
        assert_eq!(minor_one_behind["versionsBehind"], 1);

        let explicit_min = evaluate_desktop_version_status("0.1.60", "0.1.62", &recent_patch_list(), Some("0.1.61"), RELEASES_PAGE_URL, None, None);
        assert_eq!(explicit_min["status"], "update_required");
        assert_eq!(explicit_min["minRequiredVersion"], "0.1.61");
    }
}
