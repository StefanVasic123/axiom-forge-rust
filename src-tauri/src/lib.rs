use serde::{Deserialize, Serialize};
use sysinfo::System;
use std::collections::HashMap;
use keyring::Entry;
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub struct PendingDeepLink(pub Mutex<Option<String>>);

// ==================== SECURITY API ====================

fn get_entry(key: &str) -> Result<Entry, String> {
    Entry::new("axiom_forge", key).map_err(|e| e.to_string())
}

#[tauri::command]
fn store_token(key: String, value: String) -> Result<(), String> {
    let entry = get_entry(&key)?;
    entry.set_password(&value).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct TokenResult {
    success: bool,
    value: Option<String>,
}

#[tauri::command]
fn get_token(key: String) -> TokenResult {
    if let Ok(entry) = get_entry(&key) {
        if let Ok(password) = entry.get_password() {
            return TokenResult { success: true, value: Some(password) };
        }
    }
    TokenResult { success: false, value: None }
}

#[tauri::command]
fn delete_token(key: String) -> Result<(), String> {
    let entry = get_entry(&key)?;
    let _ = entry.delete_password(); // Ignore if doesn't exist
    Ok(())
}

#[derive(Serialize)]
pub struct HasTokenResult {
    exists: bool,
}

#[tauri::command]
fn has_token(key: String) -> HasTokenResult {
    if let Ok(entry) = get_entry(&key) {
        HasTokenResult { exists: entry.get_password().is_ok() }
    } else {
        HasTokenResult { exists: false }
    }
}

#[tauri::command]
fn clear_all_tokens() -> Result<(), String> {
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
async fn ollama_check_health(host: Option<String>, target_model: Option<String>) -> Result<OllamaHealthResult, String> {
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

#[tauri::command]
async fn ollama_check_installed() -> Result<bool, String> {
    Ok(true) // Stub: real implementation runs `ollama --version` in terminal
}

#[tauri::command]
async fn ollama_start_server() -> Result<bool, String> {
    Ok(true) // Stub: runs `ollama serve` process
}

#[tauri::command]
async fn ollama_pull_model(window: tauri::Window, model: String) -> Result<(), String> {
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

// ==================== PROJECT API ====================
use std::fs;
use std::path::{Path, PathBuf};
use serde_json::Value;

fn get_base_dir() -> PathBuf {
    // Use standard OS data directory instead of hardcoded ~/.axiom-forge
    let local_data = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    local_data.join("axiom-forge")
}

fn get_projects_file() -> PathBuf {
    get_base_dir().join("projects.json")
}

fn get_projects_dir() -> PathBuf {
    get_base_dir().join("projects")
}

#[tauri::command]
fn project_get_all() -> Result<Vec<Value>, String> {
    let file_path = get_projects_file();
    let mut projects: Vec<Value> = if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| vec![])
    } else {
        vec![]
    };

    let dir_path = get_projects_dir();
    if dir_path.exists() {
        if let Ok(entries) = fs::read_dir(&dir_path) {
            let mut registered_ids: Vec<String> = projects.iter()
                .filter_map(|p| p.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
                .collect();

            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() {
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        if !registered_ids.contains(&folder_name) {
                            let new_proj = serde_json::json!({
                                "id": folder_name,
                                "name": folder_name,
                                "status": "imported",
                                "createdAt": chrono::Utc::now().to_rfc3339(),
                                "updatedAt": chrono::Utc::now().to_rfc3339(),
                            });
                            projects.push(new_proj);
                            registered_ids.push(folder_name);
                        }
                    }
                }
            }
        }
    }
    
    let _ = fs::create_dir_all(get_base_dir());
    let _ = fs::write(&file_path, serde_json::to_string_pretty(&projects).unwrap());

    Ok(projects)
}

#[derive(Serialize)]
pub struct ProjectSaveResult {
    success: bool,
    project: Option<Value>,
    error: Option<String>,
}

#[tauri::command]
fn project_save(mut project_data: Value) -> ProjectSaveResult {
    let file_path = get_projects_file();
    let mut projects: Vec<Value> = if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| vec![])
    } else {
        vec![]
    };

    let id = project_data.get("id").and_then(|id| id.as_str()).map(|s| s.to_string());
    
    if let Some(id_str) = &id {
        if let Some(pos) = projects.iter().position(|p| p.get("id").and_then(|i| i.as_str()) == Some(id_str)) {
            if let Some(obj) = project_data.as_object_mut() {
                obj.insert("updatedAt".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
            }
            projects[pos] = project_data.clone();
        } else {
            projects.push(project_data.clone());
        }
    } else {
        let new_id = uuid::Uuid::new_v4().to_string();
        if let Some(obj) = project_data.as_object_mut() {
            obj.insert("id".to_string(), serde_json::json!(new_id));
            obj.insert("createdAt".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
            obj.insert("updatedAt".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
        }
        projects.push(project_data.clone());
    }

    let _ = fs::write(&file_path, serde_json::to_string_pretty(&projects).unwrap());
    ProjectSaveResult { success: true, project: Some(project_data), error: None }
}#[tauri::command]
fn project_delete(id: String) -> Result<bool, String> {
    let file_path = get_projects_file();
    if !file_path.exists() { return Ok(true); }

    let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "[]".to_string());
    let mut projects: Vec<Value> = serde_json::from_str(&content).unwrap_or_else(|_| vec![]);
    
    projects.retain(|p| p.get("id").and_then(|i| i.as_str()) != Some(&id));
    
    let _ = fs::write(&file_path, serde_json::to_string_pretty(&projects).unwrap());
    
    // Also delete the folder if it exists
    let proj_dir = get_projects_dir().join(&id);
    if proj_dir.exists() {
        let _ = fs::remove_dir_all(proj_dir);
    }
    
    Ok(true)
}

#[tauri::command]
fn project_update_status(id: String, status: String, metadata: Value) -> Result<bool, String> {
    let file_path = get_projects_file();
    if !file_path.exists() { return Ok(false); }

    let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "[]".to_string());
    let mut projects: Vec<Value> = serde_json::from_str(&content).unwrap_or_else(|_| vec![]);
    
    if let Some(pos) = projects.iter().position(|p| p.get("id").and_then(|i| i.as_str()) == Some(&id)) {
        if let Some(obj) = projects[pos].as_object_mut() {
            obj.insert("status".to_string(), serde_json::json!(status));
            obj.insert("updatedAt".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
            
            // Merge metadata
            if let Some(existing_meta) = obj.get_mut("metadata").and_then(|m| m.as_object_mut()) {
                if let Some(new_meta) = metadata.as_object() {
                    for (k, v) in new_meta {
                        existing_meta.insert(k.clone(), v.clone());
                    }
                }
            } else {
                obj.insert("metadata".to_string(), metadata);
            }
        }
        let _ = fs::write(&file_path, serde_json::to_string_pretty(&projects).unwrap());
        return Ok(true);
    }
    Ok(false)
}

#[derive(Serialize)]
pub struct ProjectFile {
    pub path: String,
    pub language: String,
}

#[derive(Serialize)]
pub struct GetFilesResult {
    success: bool,
    files: Vec<ProjectFile>,
    error: Option<String>,
}

#[tauri::command]
fn project_get_files(project_id: String) -> GetFilesResult {
    let proj_dir = get_projects_dir().join(&project_id);
    let mut files = Vec::new();
    
    fn scan_dir(dir: &Path, base_dir: &Path, files: &mut Vec<ProjectFile>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let path = entry.path();
                    
                    if file_type.is_dir() {
                        if !name.starts_with('.') && name != "node_modules" {
                            scan_dir(&path, base_dir, files);
                        }
                    } else {
                        if let Ok(rel_path) = path.strip_prefix(base_dir) {
                            let rel_str = rel_path.to_string_lossy().replace("\\", "/");
                            let ext = path.extension().unwrap_or_default().to_string_lossy().to_string();
                            let lang = if ext.is_empty() { "txt".to_string() } else { ext };
                            files.push(ProjectFile { path: rel_str, language: lang });
                        }
                    }
                }
            }
        }
    }
    
    scan_dir(&proj_dir, &proj_dir, &mut files);
    
    GetFilesResult { success: true, files, error: None }
}

#[tauri::command]
async fn project_commit(project_id: String, message: Option<String>) -> Result<Value, String> {
    let proj_dir = get_projects_dir().join(&project_id);
    
    let mut git_add = tokio::process::Command::new("git");
    git_add.arg("add").arg(".").current_dir(&proj_dir);
    #[cfg(target_os = "windows")]
    git_add.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let _ = git_add.output().await;
    
    let msg = message.unwrap_or_else(|| format!("Axiom Forge checkpoint - {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")));
    
    let mut git_commit = tokio::process::Command::new("git");
    git_commit.arg("commit").arg("-m").arg(&msg).current_dir(&proj_dir);
    #[cfg(target_os = "windows")]
    git_commit.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    match git_commit.output().await {
        Ok(_) => Ok(serde_json::json!({ "success": true, "message": msg })),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn project_push_to_github(project_id: String) -> Result<Value, String> {
    let _ = project_id; // Silence unused warning
    Ok(serde_json::json!({ "success": false, "error": "Not implemented in Rust yet" })) // Stub
}

#[derive(Serialize)]
pub struct FileReadResult {
    success: bool,
    content: String,
    error: Option<String>,
}

#[tauri::command]
fn project_read_file(project_id: String, file_path: String) -> FileReadResult {
    let proj_dir = get_projects_dir().join(&project_id);
    let full_path = proj_dir.join(&file_path);
    
    if !full_path.starts_with(&proj_dir) {
        return FileReadResult { success: false, content: "".into(), error: Some("Access denied".into()) };
    }
    
    match fs::read_to_string(&full_path) {
        Ok(content) => FileReadResult { success: true, content, error: None },
        Err(e) => FileReadResult { success: false, content: "".into(), error: Some(e.to_string()) },
    }
}

#[derive(Serialize)]
pub struct FileWriteResult {
    success: bool,
    error: Option<String>,
}

#[tauri::command]
fn editor_write_file(project_id: String, file_path: String, content: String) -> FileWriteResult {
    let proj_dir = get_projects_dir().join(&project_id);
    let full_path = proj_dir.join(&file_path);
    
    if !full_path.starts_with(&proj_dir) {
        return FileWriteResult { success: false, error: Some("Access denied".into()) };
    }
    
    if let Some(parent) = full_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    
    match fs::write(&full_path, content) {
        Ok(_) => FileWriteResult { success: true, error: None },
        Err(e) => FileWriteResult { success: false, error: Some(e.to_string()) },
    }
}

#[derive(Serialize)]
pub struct AIEditResult {
    pub success: bool,
    #[serde(rename = "newContent")]
    pub new_content: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
async fn editor_ai_edit(
    file_path: String,
    content: String,
    prompt: String,
    project_id: String,
    model_id: Option<String>,
    visual_context: Option<Value>,
) -> Result<AIEditResult, String> {
    let client = reqwest::Client::new();
    let model = model_id.unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    
    let system_prompt = "You are an expert assistant. You are helping the user edit a source code file. \
                         Provide ONLY the modified full content of the file. Do not include any introductory or concluding text. \
                         Do not wrap the code in markdown code blocks unless the file itself is a markdown file. Just return the raw code.";
    
    let mut user_prompt = format!(
        "Original File Path: {}\n\
         Original File Content:\n\
         ```\n\
         {}\n\
         ```\n\n\
         User Prompt / Instructed Edit: {}\n",
         file_path, content, prompt
    );
    
    if let Some(ctx) = visual_context {
        if let Some(tag_name) = ctx.get("tagName").and_then(|t| t.as_str()) {
            user_prompt.push_str(&format!(
                "\nVisual Context (user clicked/inspected this element in the browser preview):\n\
                 - HTML Tag: {}\n",
                 tag_name
            ));
        }
        if let Some(text) = ctx.get("text").and_then(|t| t.as_str()) {
            user_prompt.push_str(&format!(
                "- Element Text: {}\n",
                 text
            ));
        }
        if let Some(component) = ctx.get("component").and_then(|c| c.as_str()) {
            user_prompt.push_str(&format!(
                "- Component Name: {}\n",
                 component
            ));
        }
    }
    
    user_prompt.push_str("\nReturn only the complete updated file content.");

    let ollama_req = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "stream": false,
        "options": {
            "temperature": 0.2
        }
    });

    let res = client.post("http://127.0.0.1:11434/api/chat")
        .json(&ollama_req)
        .send()
        .await;

    match res {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(data) = resp.json::<Value>().await {
                    if let Some(msg_content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                        let mut cleaned = msg_content.trim();
                        let ext = Path::new(&file_path).extension().unwrap_or_default().to_string_lossy().to_string();
                        let prefix1 = format!("```{}\n", ext);
                        let prefix2 = format!("```{}", ext);
                        
                        if cleaned.starts_with(&prefix1) {
                            cleaned = &cleaned[prefix1.len()..];
                        } else if cleaned.starts_with(&prefix2) {
                            cleaned = &cleaned[prefix2.len()..];
                        } else if cleaned.starts_with("```\n") {
                            cleaned = &cleaned[4..];
                        } else if cleaned.starts_with("```") {
                            if let Some(idx) = cleaned.find('\n') {
                                if idx < 20 {
                                    cleaned = &cleaned[idx+1..];
                                }
                            }
                        }
                        
                        if cleaned.ends_with("```") {
                            cleaned = &cleaned[..cleaned.len()-3];
                        }
                        
                        let final_content = cleaned.trim().to_string();
                        return Ok(AIEditResult {
                            success: true,
                            new_content: Some(final_content),
                            error: None,
                        });
                    }
                }
                Ok(AIEditResult {
                    success: false,
                    new_content: None,
                    error: Some("Ollama returned an empty response".to_string()),
                })
            } else {
                Ok(AIEditResult {
                    success: false,
                    new_content: None,
                    error: Some(format!("Ollama HTTP Error: {}", resp.status())),
                })
            }
        }
        Err(e) => {
            Ok(AIEditResult {
                success: false,
                new_content: None,
                error: Some(format!("Failed to connect to Ollama: {}", e)),
            })
        }
    }
}

#[derive(Serialize)]
pub struct ProjectEditResult {
    pub success: bool,
    #[serde(rename = "modifiedFiles")]
    pub modified_files: Option<HashMap<String, String>>,
    pub error: Option<String>,
}

#[tauri::command]
async fn editor_ai_project_edit(
    window: tauri::Window,
    project_id: String,
    prompt: String,
    model_id: Option<String>,
) -> Result<ProjectEditResult, String> {
    let client = reqwest::Client::new();
    let model = model_id.unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    let proj_dir = get_projects_dir().join(&project_id);
    
    if !proj_dir.exists() {
        return Ok(ProjectEditResult {
            success: false,
            modified_files: None,
            error: Some("Project directory does not exist".into()),
        });
    }

    let _ = window.emit("editor:project-edit-progress", serde_json::json!({
        "status": "Scanning project files...",
        "percent": 5.0
    }));

    // Step 1: Scan project files recursively
    let mut files = Vec::new();
    fn scan_dir(dir: &Path, base_dir: &Path, files: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let path = entry.path();
                    
                    if file_type.is_dir() {
                        if !name.starts_with('.') && name != "node_modules" && name != "build" && name != "dist" {
                            scan_dir(&path, base_dir, files);
                        }
                    } else {
                        if let Ok(rel_path) = path.strip_prefix(base_dir) {
                            let rel_str = rel_path.to_string_lossy().replace("\\", "/");
                            files.push(rel_str);
                        }
                    }
                }
            }
        }
    }
    scan_dir(&proj_dir, &proj_dir, &mut files);

    let _ = window.emit("editor:project-edit-progress", serde_json::json!({
        "status": "Routing request to identify affected files...",
        "percent": 10.0
    }));

    // Step 2: Routing Agent (Layer 1)
    let files_list_str = files.join("\n");
    let routing_system_prompt = "You are a Project Router Agent. Determine which files should be created or modified to satisfy the request. Respond ONLY with a JSON array of strings containing relative file paths. Do not write markdown blocks or any other explanation. Example response: [\"package.json\", \"src/App.jsx\"]";
    let routing_user_prompt = format!(
        "Project Files:\n{}\n\nUser Request: {}\n\nJSON array of files to modify or create:",
        files_list_str, prompt
    );

    let ollama_req = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": routing_system_prompt },
            { "role": "user", "content": routing_user_prompt }
        ],
        "stream": false,
        "options": { "temperature": 0.1 }
    });

    let res = client.post("http://127.0.0.1:11434/api/chat")
        .json(&ollama_req)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut target_files = Vec::new();
    if res.status().is_success() {
        if let Ok(data) = res.json::<Value>().await {
            if let Some(msg_content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                let cleaned = msg_content.trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();
                
                if let Ok(parsed_files) = serde_json::from_str::<Vec<String>>(cleaned) {
                    target_files = parsed_files;
                } else {
                    // Try regex/fallback extraction of JSON array
                    if let Some(start_idx) = cleaned.find('[') {
                        if let Some(end_idx) = cleaned.rfind(']') {
                            let array_part = &cleaned[start_idx..=end_idx];
                            if let Ok(parsed_files) = serde_json::from_str::<Vec<String>>(array_part) {
                                target_files = parsed_files;
                            }
                        }
                    }
                }
            }
        }
    }

    if target_files.is_empty() {
        return Ok(ProjectEditResult {
            success: false,
            modified_files: None,
            error: Some("Router agent failed to identify target files, or returned empty list".into()),
        });
    }

    let files_found_msg = format!("Routing completed. Identified files: {}", target_files.join(", "));
    let _ = window.emit("editor:project-edit-progress", serde_json::json!({
        "status": files_found_msg,
        "percent": 20.0
    }));

    // Step 3: Planner & Executor (Layers 2 & 3)
    let mut modified_files_map = HashMap::new();
    let num_targets = target_files.len() as f64;
    
    for (idx, file_path) in target_files.iter().enumerate() {
        let file_idx = idx as f64;
        let base_percent = 20.0 + (file_idx / num_targets) * 75.0;
        let step_size = 75.0 / num_targets;

        // Progress: Planning
        let plan_status = format!("Planning changes for {}...", file_path);
        let _ = window.emit("editor:project-edit-progress", serde_json::json!({
            "status": plan_status,
            "percent": base_percent + step_size * 0.3
        }));

        // Layer 2: Planning Agent
        let other_files: Vec<String> = target_files.iter()
            .filter(|f| *f != file_path)
            .cloned()
            .collect();
        let other_files_str = if other_files.is_empty() { "None".to_string() } else { other_files.join(", ") };

        let planning_system_prompt = "You are a Project Planner Agent. Write a concise, action-oriented edit directive for the specified target file to achieve the user request.";
        let planning_user_prompt = format!(
            "Target File: {}\nOther affected files: {}\nUser Request: {}\n\nCreate a clear, concise instruction of what needs to be changed in this file.",
            file_path, other_files_str, prompt
        );

        let plan_req = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": planning_system_prompt },
                { "role": "user", "content": planning_user_prompt }
            ],
            "stream": false,
            "options": { "temperature": 0.2 }
        });

        let plan_res = client.post("http://127.0.0.1:11434/api/chat")
            .json(&plan_req)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let mut directive = format!("Modify the file to fulfill: {}", prompt);
        if plan_res.status().is_success() {
            if let Ok(data) = plan_res.json::<Value>().await {
                if let Some(content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                    directive = content.to_string();
                }
            }
        }

        // Progress: Coding/Executing
        let code_status = format!("Generating code for {}...", file_path);
        let _ = window.emit("editor:project-edit-progress", serde_json::json!({
            "status": code_status,
            "percent": base_percent + step_size * 0.6
        }));

        // Read current content (if file exists)
        let full_path = proj_dir.join(file_path);
        let current_content = if full_path.exists() {
            fs::read_to_string(&full_path).unwrap_or_default()
        } else {
            "".to_string()
        };

        // Layer 3: Executor/Coder Agent
        let coder_system_prompt = "You are an expert assistant. You are helping the user edit a source code file. \
                                   Provide ONLY the modified full content of the file. Do not include any introductory or concluding text. \
                                   Do not wrap the code in markdown code blocks unless the file itself is a markdown file. Just return the raw code.";
        let coder_user_prompt = format!(
            "Original File Path: {}\nOriginal File Content:\n```\n{}\n```\n\nDirective / Instructions: {}\n\nReturn only the complete updated file content.",
            file_path, current_content, directive
        );

        let coder_req = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": coder_system_prompt },
                { "role": "user", "content": coder_user_prompt }
            ],
            "stream": false,
            "options": { "temperature": 0.2 }
        });

        let coder_res = client.post("http://127.0.0.1:11434/api/chat")
            .json(&coder_req)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if coder_res.status().is_success() {
            if let Ok(data) = coder_res.json::<Value>().await {
                if let Some(msg_content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                    let mut cleaned = msg_content.trim();
                    let ext = Path::new(file_path).extension().unwrap_or_default().to_string_lossy().to_string();
                    let prefix1 = format!("```{}\n", ext);
                    let prefix2 = format!("```{}", ext);
                    
                    if cleaned.starts_with(&prefix1) {
                        cleaned = &cleaned[prefix1.len()..];
                    } else if cleaned.starts_with(&prefix2) {
                        cleaned = &cleaned[prefix2.len()..];
                    } else if cleaned.starts_with("```\n") {
                        cleaned = &cleaned[4..];
                    } else if cleaned.starts_with("```") {
                        if let Some(idx) = cleaned.find('\n') {
                            if idx < 20 {
                                cleaned = &cleaned[idx+1..];
                            }
                        }
                    }
                    
                    if cleaned.ends_with("```") {
                        cleaned = &cleaned[..cleaned.len()-3];
                    }
                    
                    modified_files_map.insert(file_path.clone(), cleaned.trim().to_string());
                }
            }
        }
    }

    let _ = window.emit("editor:project-edit-progress", serde_json::json!({
        "status": "Done generating project changes!",
        "percent": 100.0
    }));

    Ok(ProjectEditResult {
        success: true,
        modified_files: Some(modified_files_map),
        error: None,
    })
}


// ==================== HARDWARE API ====================

#[derive(Serialize, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub tier: String,
    #[serde(rename = "sizeGB")]
    pub size_gb: f64,
    #[serde(rename = "minRamGB")]
    pub min_ram_gb: f64,
    pub tags: Vec<String>,
    pub strengths: String,
    pub weakness: String,
    pub description: String,
    #[serde(rename = "isCompatible")]
    pub is_compatible: bool,
    #[serde(rename = "isMarginal")]
    pub is_marginal: bool,
    #[serde(rename = "isRecommended")]
    pub is_recommended: bool,
    #[serde(rename = "allowOverride")]
    pub allow_override: bool,
}

#[derive(Serialize)]
pub struct HardwareProfile {
    pub platform: String,
    #[serde(rename = "cpuModel")]
    pub cpu_model: String,
    pub cpus: usize,
    #[serde(rename = "ramGB")]
    pub ram_gb: f64,
    pub models: Vec<Model>,
    #[serde(rename = "byTier")]
    pub by_tier: HashMap<String, Vec<Model>>,
    #[serde(rename = "recommendedModelId")]
    pub recommended_model_id: String,
}

fn get_all_models() -> Vec<Model> {
    vec![
        Model {
            id: "qwen2.5-coder:1.5b".to_string(),
            name: "Qwen 2.5 Coder 1.5B".to_string(),
            tier: "nano".to_string(),
            size_gb: 0.98,
            min_ram_gb: 2.0,
            tags: vec!["code".to_string(), "fast".to_string()],
            strengths: "Ultra-lightweight, minimal resource usage".to_string(),
            weakness: "Limited reasoning for highly complex apps".to_string(),
            description: "Perfect for legacy hardware or systems with limited RAM. Extremely fast response times.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:3b".to_string(),
            name: "Qwen 2.5 Coder 3B".to_string(),
            tier: "nano".to_string(),
            size_gb: 2.0,
            min_ram_gb: 4.0,
            tags: vec!["code".to_string(), "fast".to_string(), "balanced".to_string()],
            strengths: "Excellent coding for its size, strong TypeScript".to_string(),
            weakness: "Smaller context capacity than 7B".to_string(),
            description: "Outstanding code quality for entry-level machines. Recommended for 4GB-8GB RAM setups.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "deepseek-coder:6.7b".to_string(),
            name: "DeepSeek Coder 6.7B".to_string(),
            tier: "small".to_string(),
            size_gb: 3.8,
            min_ram_gb: 8.0,
            tags: vec!["code".to_string(), "balanced".to_string()],
            strengths: "Highly reliable, excellent repository understanding".to_string(),
            weakness: "Slightly older architecture".to_string(),
            description: "Highly robust and proven coder model. Excellent for balanced performance.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:7b".to_string(),
            name: "Qwen 2.5 Coder 7B".to_string(),
            tier: "small".to_string(),
            size_gb: 4.7,
            min_ram_gb: 8.0,
            tags: vec!["code".to_string(), "recommended".to_string(), "balanced".to_string()],
            strengths: "Near GPT-4 code quality, 128k context, fast".to_string(),
            weakness: "General non-code reasoning is average".to_string(),
            description: "Top pick for most developers. Best code generation for the price/performance ratio. Handles full Next.js apps with ease.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:14b".to_string(),
            name: "Qwen 2.5 Coder 14B".to_string(),
            tier: "mid".to_string(),
            size_gb: 9.0,
            min_ram_gb: 16.0,
            tags: vec!["code".to_string(), "advanced".to_string()],
            strengths: "Complex multi-file projects, superior architecture understanding".to_string(),
            weakness: "Requires more VRAM/RAM for fast performance".to_string(),
            description: "The professional choice. Handles entire codebases, complex APIs, and database schemas with extreme accuracy. Perfect for 16GB+ RAM setups.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        }
    ]
}

#[tauri::command]
fn get_hardware_profile() -> HardwareProfile {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let total_ram_bytes = sys.total_memory();
    let ram_gb = total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let cpus = sys.cpus().len();
    let cpu_model = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown CPU".to_string());
    
    let raw_models = get_all_models();
    let safe_ram = ram_gb * 0.80;
    
    // Determine recommended model based on safe RAM
    let mut best_model_id = "qwen2.5-coder:7b".to_string();
    let mut best_tier_idx = -1;
    let mut best_is_code = false;
    
    let tier_order = vec!["nano", "small", "mid", "power", "expert", "extreme"];
    
    for model in &raw_models {
        if safe_ram >= model.min_ram_gb {
            let tier_idx = tier_order.iter().position(|&t| t == model.tier).unwrap_or(0) as i32;
            let is_code = model.tags.contains(&"code".to_string());
            
            // Higher tier preferred. If same tier, prefer 'code'
            if tier_idx > best_tier_idx || (tier_idx == best_tier_idx && is_code && !best_is_code) {
                best_model_id = model.id.clone();
                best_tier_idx = tier_idx;
                best_is_code = is_code;
            }
        }
    }
    
    let mut models = Vec::new();
    let mut by_tier: HashMap<String, Vec<Model>> = HashMap::new();
    
    for mut model in raw_models {
        model.is_compatible = ram_gb >= model.min_ram_gb;
        model.is_marginal = !model.is_compatible && ram_gb >= model.min_ram_gb * 0.75;
        model.is_recommended = model.id == best_model_id;
        
        models.push(model.clone());
        by_tier.entry(model.tier.clone()).or_insert_with(Vec::new).push(model);
    }
    
    HardwareProfile {
        platform: std::env::consts::OS.to_string(),
        cpu_model,
        cpus,
        ram_gb,
        models,
        by_tier,
        recommended_model_id: best_model_id,
    }
}

#[tauri::command]
async fn ollama_unload_model(model: String) -> Result<(), String> {
    let client = reqwest::Client::new();
    let req_body = serde_json::json!({
        "model": model,
        "prompt": "",
        "keep_alive": 0
    });
    
    let res = client.post("http://127.0.0.1:11434/api/generate")
        .json(&req_body)
        .send()
        .await;
        
    match res {
        Ok(resp) => {
            if resp.status().is_success() {
                println!("[Ollama] Model '{}' unloaded successfully", model);
                Ok(())
            } else {
                Err(format!("Failed to unload model, HTTP status: {}", resp.status()))
            }
        }
        Err(e) => Err(format!("Failed to connect to Ollama: {}", e.to_string()))
    }
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// ==================== SERVER API ====================
use tauri::State;
use tokio::process::Command;

pub struct ServerState(pub Mutex<HashMap<String, tokio::process::Child>>);

#[derive(Serialize, Deserialize)]
pub struct ServerStartOptions {
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub platform: Option<String>,
    #[serde(rename = "techStack")]
    pub tech_stack: Option<String>,
}

#[tauri::command]
async fn server_start(
    options: ServerStartOptions,
    window: tauri::Window,
    state: State<'_, ServerState>,
) -> Result<bool, String> {
    let proj_dir = get_projects_dir().join(&options.project_id);
    
    // Auto-install dependencies if node_modules is missing
    let node_modules_dir = proj_dir.join("node_modules");
    if !node_modules_dir.exists() {
        let _ = window.emit("server:log", serde_json::json!({
            "text": "> node_modules not found. Running npm install first...\n",
            "type": "info"
        }));
        
        #[cfg(target_os = "windows")]
        let mut install_cmd = Command::new("cmd");
        #[cfg(target_os = "windows")]
        install_cmd.arg("/C").arg("npm").arg("install").current_dir(&proj_dir);
        
        #[cfg(not(target_os = "windows"))]
        let mut install_cmd = Command::new("npm");
        #[cfg(not(target_os = "windows"))]
        install_cmd.arg("install").current_dir(&proj_dir);
        
        #[cfg(target_os = "windows")]
        install_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        
        match install_cmd.spawn() {
            Ok(mut child) => {
                let _ = child.wait().await;
                let _ = window.emit("server:log", serde_json::json!({
                    "text": "> npm install complete.\n",
                    "type": "info"
                }));
            }
            Err(e) => {
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> Failed to run npm install: {}\n", e),
                    "type": "error"
                }));
            }
        }
    }
    
    // Dynamically select the npm script based on platform and package.json scripts
    let mut run_args = vec!["run".to_string(), "dev".to_string()];
    if let Ok(pkg_content) = std::fs::read_to_string(proj_dir.join("package.json")) {
        if let Ok(pkg_json) = serde_json::from_str::<Value>(&pkg_content) {
            if let Some(scripts) = pkg_json.get("scripts").and_then(|s| s.as_object()) {
                let platform_str = options.platform.as_deref().unwrap_or("web");
                if platform_str == "mobile" {
                    if scripts.contains_key("android") {
                        run_args = vec!["run".to_string(), "android".to_string()];
                    } else if scripts.contains_key("ios") {
                        run_args = vec!["run".to_string(), "ios".to_string()];
                    } else if scripts.contains_key("start") {
                        run_args = vec!["run".to_string(), "start".to_string()];
                    }
                } else if platform_str == "desktop" {
                    if scripts.contains_key("tauri") {
                        run_args = vec!["run".to_string(), "tauri".to_string(), "dev".to_string()];
                    } else if scripts.contains_key("desktop") {
                        run_args = vec!["run".to_string(), "desktop".to_string()];
                    }
                } else {
                    if !scripts.contains_key("dev") && scripts.contains_key("start") {
                        run_args = vec!["run".to_string(), "start".to_string()];
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    let mut cmd = Command::new("cmd");
    #[cfg(target_os = "windows")]
    {
        cmd.arg("/C").arg("npm");
        for arg in &run_args {
            cmd.arg(arg);
        }
        cmd.current_dir(&proj_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
    }
        
    #[cfg(not(target_os = "windows"))]
    let mut cmd = Command::new("npm");
    #[cfg(not(target_os = "windows"))]
    {
        for arg in &run_args {
            cmd.arg(arg);
        }
        cmd.current_dir(&proj_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
    }
        
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    match cmd.spawn() {
        Ok(mut child) => {
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            
            // Spawn task to read stdout and emit logs
            let window_stdout = window.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = window_stdout.emit("server:log", serde_json::json!({
                        "text": format!("{}\n", line),
                        "type": "info"
                    }));
                }
            });

            // Spawn task to read stderr and emit logs
            let window_stderr = window.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = window_stderr.emit("server:log", serde_json::json!({
                        "text": format!("{}\n", line),
                        "type": "error"
                    }));
                }
            });
            
            let mut servers = state.0.lock().unwrap();
            servers.insert(options.project_id.clone(), child);
            
            // Emit initial running status
            let _ = window.emit("server:status", serde_json::json!({
                "status": "running"
            }));
            
            Ok(true)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn server_stop(
    project_id: String,
    window: tauri::Window,
    state: State<'_, ServerState>,
) -> Result<bool, String> {
    let child_opt = {
        let mut servers = state.0.lock().unwrap();
        servers.remove(&project_id)
    };
    if let Some(mut child) = child_opt {
        let _ = child.kill().await;
        let _ = window.emit("server:status", serde_json::json!({
            "status": "stopped"
        }));
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
async fn server_status(project_id: String, state: State<'_, ServerState>) -> Result<bool, String> {
    let servers = state.0.lock().unwrap();
    Ok(servers.contains_key(&project_id))
}

// ==================== ORCHESTRATOR API ====================
use tauri::Emitter;

async fn run_orchestrator(
    window: tauri::Window,
    task_id: String,
    manifest_id: String,
    project_id: String,
    token: String,
    model_id: String,
    host: Option<String>,
) -> Result<(), String> {
    debug_log_to_file(format!(
        "[Orchestrator] Starting run_orchestrator. task_id: {}, manifest_id: {}, project_id: {}, model: {}, host: {:?}",
        task_id, manifest_id, project_id, model_id, host
    ));
    let emit_progress = |phase: &str, msg: &str, progress: f64| {
        debug_log_to_file(format!(
            "[Orchestrator Progress] Emit event - phase: {}, message: {}, progress: {}%",
            phase, msg, progress
        ));
        let _ = window.emit("task:progress", serde_json::json!({
            "taskId": task_id,
            "phase": phase,
            "message": msg,
            "progress": progress
        }));
    };

    emit_progress("fetch-manifest", "Initializing generation pipeline...", 1.0);

    // 1. Fetch Manifest
    let client = reqwest::Client::new();
    let manifest: Value;
    if manifest_id == "TEST_LOCAL" {
        manifest = serde_json::json!({
            "manifestId": "TEST_LOCAL",
            "platform": "web",
            "projectType": "nextjs",
            "resolvedStack": "Next.js + Tailwind",
            "metadata": {
                "name": "CamperConnect",
                "description": "Web app for campers to connect by location and trade gear"
            },
            "files": [
                {
                    "path": "package.json",
                    "description": "Setup package.json with next, react, react-dom, tailwindcss, and lucide-react.",
                    "language": "json"
                },
                {
                    "path": "src/app/globals.css",
                    "description": "Standard Tailwind CSS directives and a camping-themed color palette.",
                    "language": "css"
                },
                {
                    "path": "src/app/layout.jsx",
                    "description": "Root layout for Next.js App Router. Include a navigation bar with 'Map' and 'Trade Gear' links.",
                    "language": "javascript"
                },
                {
                    "path": "src/app/page.jsx",
                    "description": "Landing page for CamperConnect. Show a hero section encouraging campers to connect locally.",
                    "language": "javascript"
                },
                {
                    "path": "src/app/map/page.jsx",
                    "description": "Page to display campers on a map by location. Mock the map UI using a placeholder div and list 3 nearby campers.",
                    "language": "javascript"
                },
                {
                    "path": "src/app/trading/page.jsx",
                    "description": "Trading post page where campers can list gear for trade. Display a grid of 4 mock items (e.g., tent, stove).",
                    "language": "javascript"
                }
            ]
        });
    } else {
        let api_base = host.as_deref().unwrap_or("https://idea-analyzer-ten.vercel.app");
        let url = format!("{}/api/axiom/manifest/{}", api_base, manifest_id);
        
        let res = client.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            return Err(format!("Server response: {}", res.status()));
        }

        let json: Value = res.json().await.map_err(|e| e.to_string())?;
        manifest = json.get("data").cloned().ok_or("No data in manifest response")?;
    }

    let files = manifest.get("files").and_then(|f| f.as_array()).ok_or("No files in manifest")?;
    let name = manifest.get("name").and_then(|n| n.as_str()).unwrap_or("Unknown Project");

    emit_progress("fetch-manifest", "Manifest secured and ready.", 10.0);
    emit_progress("generate-files", "Checking local AI engine...", 15.0);

    // 2. Generation Loop
    let total_files = files.len() as f64;
    let proj_dir = get_projects_dir().join(&project_id);

    for (i, file_spec) in files.iter().enumerate() {
        let file_path = file_spec.get("path").and_then(|p| p.as_str()).unwrap_or("unknown.txt");
        let desc = file_spec.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let lang = file_spec.get("language").and_then(|l| l.as_str()).unwrap_or("txt");

        let msg = format!("Generating {}/{} - {}", i + 1, files.len(), file_path);
        let current_progress = 25.0 + ((i as f64) / total_files) * 55.0;
        emit_progress("generate-files", &msg, current_progress);

        let system_prompt = "You are an expert software engineer. Write only the code, no explanations. Provide only the code, wrapped in markdown code blocks.";
        let user_prompt = format!("Generate {} code for file: {}\n\nDescription: {}", lang, file_path, desc);

        let ollama_req = serde_json::json!({
            "model": model_id, 
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": true,
            "options": {
                "temperature": 0.3
            }
        });

        let mut resp = client.post("http://127.0.0.1:11434/api/chat")
            .json(&ollama_req)
            .send()
            .await
            .map_err(|e| format!("Ollama error: {}", e))?;

        let mut full_content = String::new();
        let mut token_count = 0;

        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            if let Ok(str_chunk) = std::str::from_utf8(&chunk) {
                for line in str_chunk.lines() {
                    if line.is_empty() { continue; }
                    if let Ok(parsed) = serde_json::from_str::<Value>(line) {
                        if let Some(content) = parsed.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                            full_content.push_str(content);
                            token_count += 1;
                            
                            if token_count % 50 == 0 {
                                emit_progress("generate-files", &format!("AI typing {}... ({} tokens)", file_path, token_count), current_progress);
                            }
                        }
                    }
                }
            }
        }

        // Clean markdown backticks
        let mut cleaned = full_content.trim();
        let prefix1 = format!("```{}\n", lang);
        let prefix2 = format!("```{}", lang);
        
        if cleaned.starts_with(&prefix1) {
            cleaned = &cleaned[prefix1.len()..];
        } else if cleaned.starts_with(&prefix2) {
            cleaned = &cleaned[prefix2.len()..];
        } else if cleaned.starts_with("```\n") {
            cleaned = &cleaned[4..];
        }
        
        if cleaned.ends_with("```") {
            cleaned = &cleaned[..cleaned.len()-3];
        }
        
        cleaned = cleaned.trim();

        // 3. Save File
        let full_disk_path = proj_dir.join(file_path);
        if let Some(parent) = full_disk_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&full_disk_path, cleaned);
    }

    // 4. Save Project State
    emit_progress("save-project", "Saving project files...", 80.0);
    
    let new_proj = serde_json::json!({
        "id": project_id,
        "name": name,
        "status": "generated",
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });
    let _ = project_save(new_proj);

    emit_progress("complete", "Generation completed successfully!", 100.0);

    Ok(())
}

#[tauri::command]
async fn task_start_generation(
    window: tauri::Window,
    manifest_id: String,
    project_id: Option<String>,
    token: String,
    model_id: Option<String>,
    host: Option<String>,
) -> Result<String, String> {
    let p_id = project_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let task_id = format!("task-{}", uuid::Uuid::new_v4());
    let model = model_id.clone().unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    
    debug_log_to_file(format!(
        "[IPC Rust Command] task_start_generation called. manifest_id: {}, project_id: {:?}, model_id: {:?}, host: {:?}",
        manifest_id, project_id, model_id, host
    ));

    let t_id = task_id.clone();
    let p_id_clone = p_id.clone();
    let host_clone = host.clone();
    tokio::spawn(async move {
        debug_log_to_file(format!("[IPC Rust Command] Spawned tokio worker task for task_id: {}", t_id));
        if let Err(e) = run_orchestrator(window.clone(), t_id.clone(), manifest_id, p_id_clone, token, model, host_clone).await {
            debug_log_to_file(format!("[IPC Rust Command] run_orchestrator failed for task_id {}: {}", t_id, e));
            let _ = window.emit("task:progress", serde_json::json!({
                "taskId": t_id,
                "phase": "error",
                "message": e,
                "progress": 0.0
            }));
        } else {
            debug_log_to_file(format!("[IPC Rust Command] run_orchestrator succeeded for task_id: {}", t_id));
        }
    });

    Ok(task_id)
}
use tauri::Manager;
use std::io::Write;

#[tauri::command]
fn debug_log_to_file(message: String) {
    let log_path = get_projects_dir().join("deeplink_debug.txt");
    println!("[JS/Rust Debug Log] {}", message);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path) {
            let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", time, message);
        }
}

#[tauri::command]
fn get_pending_deep_link(state: tauri::State<'_, PendingDeepLink>) -> Option<String> {
    if let Ok(pending) = state.0.lock() {
        let val = pending.clone();
        debug_log_to_file(format!("get_pending_deep_link called. Returning: {:?}", val));
        val
    } else {
        None
    }
}

#[tauri::command]
fn clear_pending_deep_link(state: tauri::State<'_, PendingDeepLink>) {
    if let Ok(mut pending) = state.0.lock() {
        debug_log_to_file("clear_pending_deep_link called. Resetting to None.".to_string());
        *pending = None;
    }
}

fn handle_axiom_url<R: tauri::Runtime>(app: &tauri::AppHandle<R>, url_str: &str) {
    debug_log_to_file(format!("handle_axiom_url intercepted deep link URL: {}", url_str));

    // Also save in global state for JIT pulling on cold starts
    if let Some(state) = app.try_state::<PendingDeepLink>() {
        if let Ok(mut pending) = state.0.lock() {
            *pending = Some(url_str.to_string());
            debug_log_to_file(format!("Saved pending URL in Rust state: {}", url_str));
        }
    }

    if let Some(rest) = url_str.strip_prefix("axiom://") {
        let mut parts = rest.splitn(2, '?');
        let action = parts.next().unwrap_or("").trim_end_matches('/');
        let query = parts.next().unwrap_or("");
        
        let mut params = std::collections::HashMap::new();
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                params.insert(k, v);
            }
        }
        
        match action {
            "build" | "generate" => {
                let manifest_id = params.get("id").copied().unwrap_or("");
                let project_id = params.get("projectId").copied().unwrap_or(manifest_id);
                let token = params.get("token").copied().unwrap_or("");
                let host = params.get("host").copied().unwrap_or("");
                
                debug_log_to_file(format!(
                    "Emitting deep-link:build event with manifestId={} token={} host={}",
                    manifest_id, token, host
                ));
                
                if !manifest_id.is_empty() {
                    let _ = app.emit("deep-link:build", serde_json::json!({
                        "manifestId": manifest_id,
                        "projectId": project_id,
                        "token": token,
                        "host": host
                    }));
                }
            },
            "config" => {
                let project_id = params.get("projectId").copied().unwrap_or("");
                debug_log_to_file(format!("Emitting deep-link:config event with projectId={}", project_id));
                if !project_id.is_empty() {
                    let _ = app.emit("deep-link:config", serde_json::json!({
                        "projectId": project_id
                    }));
                }
            },
            "deploy" => {
                let project_id = params.get("projectId").copied().unwrap_or("");
                debug_log_to_file(format!("Emitting deep-link:deploy event with projectId={}", project_id));
                if !project_id.is_empty() {
                    let _ = app.emit("deep-link:deploy", serde_json::json!({
                        "projectId": project_id
                    }));
                }
            },
            other => {
                debug_log_to_file(format!("Warning: Unknown deep link action received: {}", other));
            }
        }
    } else {
        debug_log_to_file(format!("Warning: Deep link URL lacks axiom:// prefix: {}", url_str));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            for arg in argv {
                if arg.starts_with("axiom://") {
                    handle_axiom_url(app, &arg);
                }
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let app_handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        handle_axiom_url(&app_handle, url.as_str());
                    }
                });
                
                // 1. Prioritize scanning CLI arguments for cold start deep-links
                let mut found_cli_url = false;
                for arg in std::env::args() {
                    let arg_lower = arg.to_lowercase();
                    if arg_lower.starts_with("axiom://") || arg_lower.contains("axiom://") {
                        let cleaned_url = arg.trim_matches('"').trim_matches('\'').to_string();
                        debug_log_to_file(format!("Cold start CLI argument match found: {}", cleaned_url));
                        handle_axiom_url(app.handle(), &cleaned_url);
                        found_cli_url = true;
                    }
                }
                
                // 2. Fall back to plugin retrieval if CLI scan produced no deep link
                if !found_cli_url {
                    if let Ok(Some(urls)) = app.deep_link().get_current() {
                        for url in urls {
                            debug_log_to_file(format!("Cold start plugin-registered match found: {}", url.as_str()));
                            handle_axiom_url(app.handle(), url.as_str());
                        }
                    }
                }
            }
            Ok(())
        })
        .manage(PendingDeepLink(Mutex::new(None)))
        .manage(ServerState(Mutex::new(HashMap::new())))
        .invoke_handler(tauri::generate_handler![
            greet, 
            get_hardware_profile,
            store_token,
            get_token,
            delete_token,
            has_token,
            clear_all_tokens,
            ollama_check_health,
            ollama_check_installed,
            ollama_start_server,
            ollama_pull_model,
            project_get_all,
            project_save,
            project_delete,
            project_update_status,
            project_get_files,
            project_commit,
            project_push_to_github,
            project_read_file,
            editor_write_file,
            editor_ai_edit,
            editor_ai_project_edit,
            server_start,
            server_stop,
            server_status,
            task_start_generation,
            ollama_unload_model,
            get_pending_deep_link,
            clear_pending_deep_link,
            debug_log_to_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
