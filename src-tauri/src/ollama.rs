use crate::*;

// ==================== SECURITY API ====================

pub fn get_entry(key: &str) -> Result<Entry, String> {
    Entry::new("axiom_forge", key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn store_token(key: String, value: String) -> Result<(), String> {
    let entry = get_entry(&key)?;
    entry.set_password(&value).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct TokenResult {
    pub success: bool,
    pub value: Option<String>,
}

#[tauri::command]
pub fn get_token(key: String) -> TokenResult {
    if let Ok(entry) = get_entry(&key) {
        if let Ok(password) = entry.get_password() {
            return TokenResult { success: true, value: Some(password) };
        }
    }
    TokenResult { success: false, value: None }
}

#[tauri::command]
pub fn delete_token(key: String) -> Result<(), String> {
    let entry = get_entry(&key)?;
    let _ = entry.delete_password(); // Ignore if doesn't exist
    Ok(())
}

#[derive(Serialize)]
pub struct HasTokenResult {
    pub exists: bool,
}

#[tauri::command]
pub fn has_token(key: String) -> HasTokenResult {
    if let Ok(entry) = get_entry(&key) {
        HasTokenResult { exists: entry.get_password().is_ok() }
    } else {
        HasTokenResult { exists: false }
    }
}

#[tauri::command]
pub fn clear_all_tokens() -> Result<(), String> {
    let keys = vec!["github-token", "vercel-token", "netlify-token", "render-token", "hostinger-token"];
    for key in keys {
        if let Ok(entry) = get_entry(key) {
            let _ = entry.delete_password();
        }
    }
    Ok(())
}

// ==================== OLLAMA API ====================
use std::time::Duration;

#[derive(Serialize)]
pub struct OllamaHealthResult {
    pub available: bool,
    #[serde(rename = "hasModel")]
    pub has_model: bool,
    pub models: Vec<String>,
    pub message: String,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ollama_check_health(host: Option<String>, target_model: Option<String>) -> Result<OllamaHealthResult, String> {
    let base_url = host.unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
    let model_to_check = target_model.unwrap_or_else(|| "llama3.2:1b".to_string());
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;
        
    let url = format!("{}/api/tags", base_url);
    let res = client.get(&url).send().await;
    
    match res {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                if let Ok(data) = response.json::<serde_json::Value>().await {
                    let mut model_names = Vec::new();
                    let mut has_model = false;
                    
                    if let Some(models) = data.get("models").and_then(|m| m.as_array()) {
                        for m in models {
                            if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                                model_names.push(name.to_string());
                                if name == model_to_check || name.starts_with(&model_to_check) {
                                    has_model = true;
                                }
                            }
                        }
                    }
                    
                    return Ok(OllamaHealthResult {
                        available: true,
                        has_model,
                        models: model_names,
                        message: if has_model { "Ready".to_string() } else { format!("Model '{}' not found.", model_to_check) },
                        error: None,
                    });
                }
            }
            Ok(OllamaHealthResult {
                available: false,
                has_model: false,
                models: vec![],
                message: "Ollama not responding correctly".to_string(),
                error: Some(format!("HTTP {}", status)),
            })
        },
        Err(e) => {
            let is_timeout = e.is_timeout();
            let msg = if is_timeout { "Ollama connection timed out (3s)" } else { "Ollama not running" };
            Ok(OllamaHealthResult {
                available: false,
                has_model: false,
                models: vec![],
                message: msg.to_string(),
                error: Some(e.to_string()),
            })
        }
    }
}

#[derive(Serialize)]
pub struct OllamaInstalledResult {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

pub fn find_ollama_executable() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(local_data) = dirs::data_local_dir() {
            let win_default = local_data.join("Programs").join("Ollama").join("ollama.exe");
            if win_default.exists() {
                return win_default;
            }
        }
    }
    PathBuf::from("ollama")
}

#[tauri::command]
pub async fn ollama_check_installed() -> Result<OllamaInstalledResult, String> {
    let exe = find_ollama_executable();
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("--version");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    match cmd.output().await {
        Ok(output) if output.status.success() => {
            let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(OllamaInstalledResult {
                installed: true,
                version: Some(ver),
                path: Some(exe.to_string_lossy().to_string()),
            })
        }
        _ => {
            // Also check if Ollama HTTP API is already responding on 11434
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap_or_default();
            if let Ok(resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
                if resp.status().is_success() {
                    return Ok(OllamaInstalledResult {
                        installed: true,
                        version: Some("running".to_string()),
                        path: None,
                    });
                }
            }
            Ok(OllamaInstalledResult {
                installed: false,
                version: None,
                path: None,
            })
        }
    }
}

#[derive(Serialize)]
pub struct OllamaStartResult {
    pub success: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn ollama_start_server() -> Result<OllamaStartResult, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_default();

    // Check if already running
    if let Ok(resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
        if resp.status().is_success() {
            return Ok(OllamaStartResult { success: true, error: None });
        }
    }

    let exe = find_ollama_executable();
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("serve");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    if let Err(e) = cmd.spawn() {
        return Ok(OllamaStartResult {
            success: false,
            error: Some(format!("Failed to spawn ollama serve: {}", e)),
        });
    }

    // Wait up to 8 seconds for server to become responsive
    for _ in 0..16 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Ok(resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
            if resp.status().is_success() {
                return Ok(OllamaStartResult { success: true, error: None });
            }
        }
    }

    Ok(OllamaStartResult {
        success: false,
        error: Some("Timed out waiting for Ollama server to start on port 11434".to_string()),
    })
}

#[tauri::command]
pub async fn ollama_pull_model(window: tauri::Window, model: String) -> Result<(), String> {
    let client = reqwest::Client::new();
    let req_body = serde_json::json!({ "name": model });
    
    let mut resp = client.post("http://127.0.0.1:11434/api/pull")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    if !resp.status().is_success() {
        return Err(format!("HTTP Error: {}", resp.status()));
    }
    
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if let Ok(str_chunk) = std::str::from_utf8(&chunk) {
            for line in str_chunk.lines() {
                if line.is_empty() { continue; }
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                    let status = parsed.get("status").and_then(|s| s.as_str()).unwrap_or("");
                    let completed = parsed.get("completed").and_then(|c| c.as_f64()).unwrap_or(0.0);
                    let total = parsed.get("total").and_then(|t| t.as_f64()).unwrap_or(1.0);
                    
                    let percent = if total > 0.0 { (completed / total) * 100.0 } else { 0.0 };
                    
                    let _ = window.emit("ollama:pull-progress", serde_json::json!({
                        "status": status,
                        "percent": percent.round(),
                        "completed": completed,
                        "total": total
                    }));
                }
            }
        }
    }
    
    let _ = window.emit("ollama:pull-progress", serde_json::json!({
        "status": "success",
        "percent": 100
    }));
    
    Ok(())
}

