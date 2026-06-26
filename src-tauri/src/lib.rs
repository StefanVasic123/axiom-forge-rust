use serde::{Deserialize, Serialize};
use sysinfo::System;
use std::collections::HashMap;
use keyring::Entry;
use std::sync::Mutex;

// std::os::windows::process::CommandExt is not needed for tokio Command

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

#[tauri::command]
fn get_axiom_settings() -> Value {
    let file_path = get_base_dir().join("axiom-settings.json");
    if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    }
}

#[tauri::command]
fn save_axiom_settings(settings: Value) -> Result<(), String> {
    let file_path = get_base_dir().join("axiom-settings.json");
    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&file_path, content).map_err(|e| e.to_string())?;
    Ok(())
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

fn is_html_template_file(file_path: &str) -> bool {
    let path = std::path::Path::new(file_path);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        matches!(
            ext_lower.as_str(),
            "html" | "htm" | "php" | "twig" | "jinja" | "jinja2" | "njk" | "hbs" | "ejs" | "vue" | "svelte" | "jsx" | "tsx"
        )
    } else {
        false
    }
}

fn inject_axiom_attrs(content: &str, relative_path: &str) -> String {
    if content.contains("data-axiom-component=") {
        return content.to_string();
    }

    let path = std::path::Path::new(relative_path);
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Component");

    // PascalCase
    let mut component_name = String::new();
    let mut capitalize_next = true;
    for c in file_stem.chars() {
        if c == '-' || c == '_' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            component_name.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            component_name.push(c);
        }
    }
    if component_name.is_empty() {
        component_name = "Component".to_string();
    }

    let mut first_match_idx = None;
    let mut matched_tag_len = 0;

    let chars: Vec<char> = content.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '<' && i + 1 < chars.len() {
            // Preskoči ako je ispred (nakon preskakanja razmaka) alfanumerički karakter, tačka ili donja crta (indikacija TypeScript generika ili poređenja)
            let mut is_generic_or_compare = false;
            if i > 0 {
                let mut prev_idx = i - 1;
                while prev_idx > 0 && chars[prev_idx].is_ascii_whitespace() {
                    prev_idx -= 1;
                }
                let prev_c = chars[prev_idx];
                if prev_c.is_ascii_alphanumeric() || prev_c == '.' || prev_c == '_' {
                    let mut word_start = prev_idx;
                    while word_start > 0 {
                        let c = chars[word_start - 1];
                        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                            word_start -= 1;
                        } else {
                            break;
                        }
                    }
                    let word: String = chars[word_start..=prev_idx].iter().collect();
                    let keywords = ["return", "default", "yield", "await", "as", "typeof", "void", "delete"];
                    if !keywords.contains(&word.as_str()) {
                        is_generic_or_compare = true;
                    }
                }
            }
            if is_generic_or_compare {
                continue;
            }
            let next_c = chars[i + 1];
            if next_c.is_ascii_alphabetic() || next_c == '_' {
                let mut tag_name = String::new();
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '-' || chars[j] == '_') {
                    tag_name.push(chars[j]);
                    j += 1;
                }
                
                let is_valid_tag = if tag_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                    tag_name != "ReactNode" && tag_name != "ReactElement" && tag_name != "FC" && tag_name != "ComponentType" && tag_name != "any"
                } else {
                    let html_tags = vec![
                        "div", "section", "main", "header", "footer", "article", "aside", 
                        "nav", "form", "ul", "ol", "table", "body", "html", "span", "p", "a", "h1", "h2", "h3", "h4", "h5", "h6"
                    ];
                    html_tags.contains(&tag_name.as_str())
                };
                
                if is_valid_tag {
                    first_match_idx = Some(i);
                    matched_tag_len = tag_name.len() + 1;
                    break;
                }
            }
        }
    }

    if let Some(idx) = first_match_idx {
        let mut new_content = String::new();
        new_content.push_str(&content[..idx + matched_tag_len]);
        new_content.push_str(&format!(
            " data-axiom-component=\"{}\" data-axiom-file=\"{}\"",
            component_name, relative_path.replace('\\', "/")
        ));
        new_content.push_str(&content[idx + matched_tag_len..]);
        new_content
    } else {
        content.to_string()
    }
}

fn detect_project_type(manifest: &Value) -> String {
    if let Some(t) = manifest.get("projectType").and_then(|s| s.as_str()) {
        return t.to_lowercase();
    }
    if let Some(stack) = manifest.get("resolvedStack").and_then(|s| s.as_str()) {
        let stack_lower = stack.to_lowercase();
        if stack_lower.contains("next") {
            return "nextjs".to_string();
        }
        if stack_lower.contains("mern") || stack_lower.contains("express") || stack_lower.contains("mongodb") {
            return "mern".to_string();
        }
        if stack_lower.contains("electron") {
            return "electron".to_string();
        }
        if stack_lower.contains("rust") || stack_lower.contains("tauri") {
            return "rust".to_string();
        }
        if stack_lower.contains("flutter") {
            return "flutter".to_string();
        }
    }
    if let Some(p) = manifest.get("platform").and_then(|s| s.as_str()) {
        let p_lower = p.to_lowercase();
        if p_lower == "mobile" {
            return "flutter".to_string();
        }
        if p_lower == "desktop" {
            return "electron".to_string();
        }
    }
    "nextjs".to_string()
}

fn normalize_project_structure(files: &mut Vec<Value>, metamanifest: &Value, project_type: &str) {
    let root_files: Vec<String> = metamanifest
        .get("rootFiles")
        .and_then(|rf| rf.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();


    for file in files.iter_mut() {
        if let Some(path_str) = file.get("path").and_then(|p| p.as_str()) {
            let normalized = path_str.replace('\\', "/");
            let parts: Vec<&str> = normalized.split('/').collect();
            let filename = parts.last().map(|s| s.to_lowercase()).unwrap_or_default();
            let path_lower = normalized.to_lowercase();

            // 1. Move root configuration files to root
            if root_files.contains(&filename) {
                file["path"] = serde_json::json!(parts.last().unwrap());
                continue;
            }

            // 2. Normalize Prisma schema & seeds
            if filename == "schema.prisma" {
                file["path"] = serde_json::json!("prisma/schema.prisma");
                continue;
            }

            // Normalize Next.js App Router root layout path case-sensitivity
            if path_lower == "src/app/layout.tsx" || path_lower == "src/app/layout.jsx" || path_lower == "app/layout.tsx" || path_lower == "app/layout.jsx" {
                file["path"] = serde_json::json!("src/app/layout.tsx");
                continue;
            }
            if filename == "seed.ts" || filename == "seed.js" {
                if normalized.contains("/prisma/") || normalized.contains("prisma/") {
                    file["path"] = serde_json::json!(format!("prisma/{}", parts.last().unwrap()));
                    continue;
                }
            }

            // 3. Normalize common root files (gitignore, env, readme)
            if filename == ".gitignore" || filename == ".env.example" || filename == "readme.md" {
                file["path"] = serde_json::json!(parts.last().unwrap());
                continue;
            }

            // 4. Normalize Pages Router files to App Router if project is Next.js App Router
            if project_type == "nextjs" {
                let is_in_app = path_lower.starts_with("src/app/");
                let is_in_pages = path_lower.starts_with("src/app/pages/") || path_lower.starts_with("src/pages/") || path_lower.starts_with("pages/");
                
                if is_in_pages || is_in_app {
                    let last_part = parts.last().copied().unwrap_or("");
                    if !last_part.is_empty() {
                        let ext = last_part.rfind('.').map(|pos| &last_part[pos+1..]).unwrap_or("");
                        if ext == "tsx" || ext == "jsx" || ext == "js" {
                            let is_page_file = if is_in_pages {
                                true
                            } else {
                                parts.len() == 3
                            };
                            
                            if is_page_file {
                                let filename_no_ext = last_part.rfind('.').map(|pos| &last_part[..pos]).unwrap_or(last_part);
                                let filename_lower = filename_no_ext.to_lowercase();
                                
                                if filename_lower == "index" {
                                    file["path"] = serde_json::json!(format!("src/app/page.{}", ext));
                                    continue;
                                } else if filename_lower == "_app" || filename_lower == "layout" {
                                    file["path"] = serde_json::json!(format!("src/app/layout.{}", ext));
                                    continue;
                                } else if filename_lower != "page" && filename_lower != "layout" && filename_lower != "loading" && filename_lower != "error" && filename_lower != "not-found" {
                                    file["path"] = serde_json::json!(format!("src/app/{}/page.{}", filename_lower, ext));
                                    continue;
                                }
                            }
                        }
                    }
                }
            }

            // 5. Normalize API Route files to App Router
            if project_type == "nextjs" && path_lower.contains("src/app/api/") {
                let last_part = parts.last().unwrap();
                let ext = last_part.rfind('.').map(|pos| &last_part[pos+1..]).unwrap_or("");
                if ext == "ts" || ext == "js" {
                    let filename_no_ext = last_part.rfind('.').map(|pos| &last_part[..pos]).unwrap_or(last_part);
                    if filename_no_ext.to_lowercase() == "index" || filename_no_ext.to_lowercase() == "route" {
                        let dir_parts = &parts[0..parts.len()-1];
                        file["path"] = serde_json::json!(format!("{}/route.{}", dir_parts.join("/"), ext));
                    } else {
                        // It is a file-based route like api/assignments/[id].ts -> api/assignments/[id]/route.ts
                        let dir_parts = &parts[0..parts.len()-1];
                        file["path"] = serde_json::json!(format!("{}/{}/route.{}", dir_parts.join("/"), filename_no_ext, ext));
                    }
                    continue;
                }
            }
        }
    }

    let mut paths_present = std::collections::HashSet::new();
    for file in files.iter() {
        if let Some(path_str) = file.get("path").and_then(|p| p.as_str()) {
            paths_present.insert(path_str.replace('\\', "/").to_lowercase());
        }
    }

    if let Some(templates) = metamanifest.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
        for template in templates {
            if let Some(path_str) = template.get("path").and_then(|p| p.as_str()) {
                let path_lower = path_str.to_lowercase();
                if !paths_present.contains(&path_lower) {
                    files.push(serde_json::json!({
                        "path": path_str,
                        "description": template.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "language": template.get("language").and_then(|l| l.as_str()).unwrap_or("txt")
                    }));
                }
            }
        }
    }
}

fn post_process_generated_file(
    file_path: &str,
    content: &str,
    metamanifest: &Value,
    has_tailwind: bool,
) -> String {
    let normalized_path = file_path.replace('\\', "/");
    let path_lower = normalized_path.to_lowercase();

    if content.trim().is_empty() || content.trim() == "{}" {
        if let Some(templates) = metamanifest.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
            for template in templates {
                if let Some(t_path) = template.get("path").and_then(|p| p.as_str()) {
                    if t_path.to_lowercase() == path_lower {
                        if let Some(default_content) = template.get("defaultContent").and_then(|c| c.as_str()) {
                            return default_content.to_string();
                        }
                    }
                }
            }
        }
    }

    if path_lower.ends_with("tailwind.config.js") {
        if !content.contains("./src/") && !content.contains("src/") {
            let mut processed_tw = content.to_string();
            if processed_tw.contains("content: [") {
                processed_tw = processed_tw.replace(
                    "content: [",
                    "content: [\n    \"./src/**/*.{js,ts,jsx,tsx}\",\n    \"./app/**/*.{js,ts,jsx,tsx}\","
                );
                return processed_tw;
            }
        }
    }

    if path_lower.ends_with("postcss.config.js") {
        return "module.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n}".to_string();
    }

    if path_lower.ends_with("tsconfig.json") {
        if let Ok(mut ts_val) = serde_json::from_str::<Value>(content) {
            if let Some(ts_obj) = ts_val.as_object_mut() {
                let compiler_options = ts_obj.entry("compilerOptions").or_insert(serde_json::json!({}));
                if let Some(co_obj) = compiler_options.as_object_mut() {
                    co_obj.insert("esModuleInterop".to_string(), serde_json::json!(true));
                    co_obj.insert("resolveJsonModule".to_string(), serde_json::json!(true));
                    co_obj.insert("skipLibCheck".to_string(), serde_json::json!(true));
                    co_obj.insert("jsx".to_string(), serde_json::json!("preserve"));
                    
                    let paths = co_obj.entry("paths").or_insert(serde_json::json!({}));
                    if let Some(paths_obj) = paths.as_object_mut() {
                        paths_obj.insert("@/*".to_string(), serde_json::json!(["./src/*"]));
                    }

                    if let Some(types) = co_obj.get_mut("types") {
                        if let Some(types_arr) = types.as_array_mut() {
                            types_arr.retain(|val| val.as_str() != Some("next-env"));
                        }
                    }
                }
            }
            if let Ok(pretty) = serde_json::to_string_pretty(&ts_val) {
                return pretty;
            }
        }
    }

    if path_lower.ends_with("package.json") {
        let mut merge_rule = None;
        if let Some(templates) = metamanifest.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
            for template in templates {
                if let Some(t_path) = template.get("path").and_then(|p| p.as_str()) {
                    if t_path.to_lowercase() == path_lower {
                        if let Some(merge) = template.get("mergeDependencies") {
                            merge_rule = Some(merge);
                            break;
                        }
                    }
                }
            }
        }

        let mut pkg_val: Value = serde_json::from_str(content).unwrap_or_else(|_| {
            serde_json::json!({
                "name": "generated-package",
                "version": "1.0.0",
                "private": true,
                "dependencies": {},
                "devDependencies": {}
            })
        });

        if let Some(pkg_obj) = pkg_val.as_object_mut() {
            if let Some(rule) = merge_rule {
                if let Some(rule_deps) = rule.get("dependencies").and_then(|d| d.as_object()) {
                    let dependencies = pkg_obj.entry("dependencies").or_insert(serde_json::json!({}));
                    if let Some(deps_obj) = dependencies.as_object_mut() {
                        for (k, v) in rule_deps {
                            deps_obj.insert(k.clone(), v.clone());
                        }
                    }
                }

                if let Some(rule_dev_deps) = rule.get("devDependencies").and_then(|d| d.as_object()) {
                    let dev_dependencies = pkg_obj.entry("devDependencies").or_insert(serde_json::json!({}));
                    if let Some(dev_deps_obj) = dev_dependencies.as_object_mut() {
                        for (k, v) in rule_dev_deps {
                            dev_deps_obj.insert(k.clone(), v.clone());
                        }
                    }
                }

                if let Some(rule_scripts) = rule.get("scripts").and_then(|s| s.as_object()) {
                    let scripts = pkg_obj.entry("scripts").or_insert(serde_json::json!({}));
                    if let Some(scripts_obj) = scripts.as_object_mut() {
                        for (k, v) in rule_scripts {
                            scripts_obj.insert(k.clone(), v.clone());
                        }
                    }
                }
            }

            // Next.js specific cleanup (remove SPA routing)
            let is_nextjs = pkg_obj.get("dependencies")
                .and_then(|d| d.as_object())
                .map_or(false, |deps| deps.contains_key("next"));

            if is_nextjs {
                if let Some(deps) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    deps.remove("react-router-dom");
                    deps.remove("react-router");
                }
            }

            if let Ok(pretty) = serde_json::to_string_pretty(&pkg_val) {
                return pretty;
            }
        }
    }

    let mut is_target_css = false;
    if has_tailwind && (path_lower.ends_with(".css") || path_lower.ends_with(".scss")) {
        if let Some(target_files) = metamanifest.get("injectTailwindDirectives").and_then(|i| i.get("targetCssFiles")).and_then(|f| f.as_array()) {
            is_target_css = target_files.iter().any(|val| {
                val.as_str().map(|s| path_lower.contains(&s.to_lowercase())).unwrap_or(false)
            });
        } else {
            is_target_css = path_lower.contains("globals.css") || path_lower.contains("styles.css");
        }
    }

    if is_target_css {
        let trimmed = content.trim();
        if !trimmed.contains("@tailwind base") {
            let mut prepended = "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n\n".to_string();
            prepended.push_str(content);
            return prepended;
        }
    }

    let mut processed = if is_html_template_file(&normalized_path) {
        inject_axiom_attrs(content, &normalized_path)
    } else {
        content.to_string()
    };

    // Apply learned global healing rules
    let rules_path = get_base_dir().join("axiom-healing-rules.json");
    if rules_path.exists() {
        if let Ok(rules_content) = std::fs::read_to_string(&rules_path) {
            if let Ok(rules_val) = serde_json::from_str::<Value>(&rules_content) {
                if let Some(rules_arr) = rules_val.as_array() {
                    for rule in rules_arr {
                        if let (Some(search), Some(replace)) = (rule.get("search").and_then(|s| s.as_str()), rule.get("replace").and_then(|r| r.as_str())) {
                            if processed.contains(search) {
                                processed = processed.replace(search, replace);
                            }
                        }
                    }
                }
            }
        }
    }

    // Rule for schema.prisma corrections
    if path_lower.ends_with("schema.prisma") {
        // 1. Fix missing url in datasource block
        if processed.contains("datasource db {") {
            if let Some(start_idx) = processed.find("datasource db {") {
                if let Some(close_brace) = processed[start_idx..].find('}') {
                    let datasource_block = &processed[start_idx..start_idx + close_brace];
                    if !datasource_block.contains("url") {
                        let mut fixed_datasource = datasource_block.to_string();
                        if let Some(open_brace_idx) = fixed_datasource.find('{') {
                            fixed_datasource.insert_str(open_brace_idx + 1, "\n  url      = env(\"DATABASE_URL\")");
                            processed = processed.replace(datasource_block, &fixed_datasource);
                        }
                    }
                }
            }
        }

        // 2. Fix VerificationToken definition
        if processed.contains("model VerificationToken") {
            if let Some(start_idx) = processed.find("model VerificationToken") {
                if let Some(open_brace) = processed[start_idx..].find('{') {
                    let absolute_open = start_idx + open_brace;
                    if let Some(close_brace) = processed[absolute_open..].find('}') {
                        let absolute_close = absolute_open + close_brace;
                        let correct_model = "model VerificationToken {\n  identifier String\n  token      String   @unique\n  expires    DateTime\n\n  @@unique([identifier, token])\n}".to_string();
                        processed.replace_range(start_idx..=absolute_close, &correct_model);
                    }
                }
            }
        }

        // 3. Fix references like [User.id] -> [id]
        let mut i = 0;
        while let Some(idx) = processed[i..].find("references: [") {
            let absolute_idx = i + idx;
            let start_bracket = absolute_idx + 13;
            if let Some(end_bracket) = processed[start_bracket..].find(']') {
                let content_inside = processed[start_bracket..start_bracket + end_bracket].to_string();
                if content_inside.contains('.') {
                    let parts: Vec<&str> = content_inside.split('.').collect();
                    if parts.len() > 1 {
                        let correct_field = parts[1];
                        processed.replace_range(start_bracket..start_bracket + end_bracket, correct_field);
                    }
                }
            }
            i = absolute_idx + 1;
        }

        // 4. Strip @relation from scalar fields
        let mut lines: Vec<String> = processed.lines().map(|s| s.to_string()).collect();
        let primitives = ["String", "Boolean", "Int", "BigInt", "Float", "Decimal", "DateTime", "Json", "Bytes"];
        for line in lines.iter_mut() {
            let trimmed = line.trim();
            if trimmed.contains("@relation") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let clean_type = parts[1].replace('?', "").replace('[', "").replace(']', "");
                    if primitives.contains(&clean_type.as_str()) {
                        if let Some(rel_idx) = line.find("@relation") {
                            let mut paren_count = 0;
                            let mut end_idx = rel_idx;
                            let chars: Vec<char> = line.chars().collect();
                            for k in rel_idx..chars.len() {
                                if chars[k] == '(' {
                                    paren_count += 1;
                                } else if chars[k] == ')' {
                                    paren_count -= 1;
                                    if paren_count == 0 {
                                        end_idx = k + 1;
                                        break;
                                    }
                                }
                            }
                            if end_idx > rel_idx {
                                let mut new_line = line[..rel_idx].to_string();
                                new_line.push_str(&line[end_idx..]);
                                *line = new_line;
                            } else {
                                *line = line[..rel_idx].to_string();
                            }
                        }
                    }
                }
            }
        }
        processed = lines.join("\n");
    }

    let is_js_ts_file = path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx") || path_lower.ends_with(".ts") || path_lower.ends_with(".js");
    if is_js_ts_file {
        // Ensure prisma singleton config exports default if it has a named export
        if path_lower.ends_with("lib/prisma.ts") || path_lower.ends_with("lib/prisma.js") {
            if processed.contains("export const prisma") && !processed.contains("export default") {
                processed.push_str("\nexport default prisma;\n");
            }
        }
        let is_nextjs_app_file = normalized_path.contains("/app/") || normalized_path.contains("app/")
            || normalized_path.contains("/components/") || normalized_path.contains("components/");
        if is_nextjs_app_file {
            let mut has_custom_hooks = false;
            let bytes = processed.as_bytes();
            for i in 0..bytes.len() {
                if i + 3 < bytes.len() && &bytes[i..i+3] == b"use" {
                    let next_char = bytes[i+3] as char;
                    if next_char.is_ascii_uppercase() {
                        has_custom_hooks = true;
                        break;
                    }
                }
            }

            let has_client_hooks = processed.contains("useState") 
                || processed.contains("useEffect") 
                || processed.contains("useContext") 
                || processed.contains("useRef") 
                || processed.contains("useReducer") 
                || processed.contains("useMemo") 
                || processed.contains("useCallback")
                || processed.contains("useForm")
                || processed.contains("Controller")
                || processed.contains("useFormContext")
                || processed.contains("useFieldArray")
                || has_custom_hooks;
                
            let has_mui_styles = processed.contains("styled(") 
                || processed.contains("createTheme") 
                || processed.contains("@mui/material/styles") 
                || processed.contains("@mui/system") 
                || processed.contains("@mui/styles") 
                || processed.contains("@emotion/styled") 
                || processed.contains("styled-components") 
                || processed.contains("ThemeProvider")
                || processed.contains("useSession")
                || processed.contains("useRouter")
                || processed.contains("usePathname")
                || processed.contains("useParams");
                
            if (has_client_hooks || has_mui_styles) && !processed.contains("\"use client\"") && !processed.contains("'use client'") {
                processed = format!("\"use client\";\n\n{}", processed);
            }
        }

        // Convert old Material-UI (v4) to modern MUI (v5)
        if processed.contains("@material-ui/core") || processed.contains("@material-ui/icons") || processed.contains("createMuiTheme") {
            processed = processed
                .replace("@material-ui/core/styles", "@mui/material/styles")
                .replace("@material-ui/core", "@mui/material")
                .replace("@material-ui/icons", "@mui/icons-material")
                .replace("@material-ui/styles", "@mui/styles")
                .replace("createMuiTheme", "createTheme");
        }

        // Convert React Router to Next.js App Router equivalents for Next.js projects
        if processed.contains("react-router-dom") {
            processed = processed
                .replace("import { Link } from 'react-router-dom'", "import Link from 'next/link'")
                .replace("import { Link } from \"react-router-dom\"", "import Link from \"next/link\"")
                .replace("import { useNavigate } from 'react-router-dom'", "import { useRouter } from 'next/navigation'")
                .replace("import { useNavigate } from \"react-router-dom\"", "import { useRouter } from \"next/navigation\"")
                .replace("import { useParams } from 'react-router-dom'", "import { useParams } from 'next/navigation'")
                .replace("import { useParams } from \"react-router-dom\"", "import { useParams } from \"next/navigation\"")
                .replace("import { useLocation } from 'react-router-dom'", "import { usePathname } from 'next/navigation'")
                .replace("import { useLocation } from \"react-router-dom\"", "import { usePathname } from \"next/navigation\"")
                .replace("from 'react-router-dom'", "from 'next/link'")
                .replace("from \"react-router-dom\"", "from \"next/link\"")
                .replace("<Link to=", "<Link href=");

            if processed.contains("useNavigate") {
                processed = processed
                    .replace("useNavigate()", "useRouter()")
                    .replace("const navigate =", "const router =")
                    .replace("navigate(", "router.push(");
            }
            if processed.contains("useLocation") {
                processed = processed
                    .replace("useLocation()", "usePathname()")
                    .replace("const location =", "const pathname =")
                    .replace("location.pathname", "pathname")
                    .replace("location", "pathname");
            }
        }

        // Root layout HTML & Body injection + TypeScript children fix for Next.js App Router
        let is_root_layout = path_lower.ends_with("app/layout.tsx") || path_lower.ends_with("app/layout.jsx");
        if is_root_layout {
            // Fix TypeScript children typings
            let has_fc = processed.contains("React.FC") || processed.contains("FC");
            if has_fc {
                let mut result = String::new();
                let chars: Vec<char> = processed.chars().collect();
                let mut i = 0;
                while i < chars.len() {
                    // Check for React.FC
                    if i + 8 <= chars.len() && chars[i..i+8] == ['R', 'e', 'a', 'c', 't', '.', 'F', 'C'] {
                        let mut j = i + 8;
                        let mut is_followed_by_angle_bracket = false;
                        while j < chars.len() {
                            if chars[j].is_whitespace() {
                                j += 1;
                            } else if chars[j] == '<' {
                                is_followed_by_angle_bracket = true;
                                break;
                            } else {
                                break;
                            }
                        }
                        if !is_followed_by_angle_bracket {
                            result.push_str("React.FC<{ children: React.ReactNode }>");
                            i += 8;
                            continue;
                        }
                    }
                    // Check for FC (must be standing alone as a word, e.g. not in "someFC" or "FCBuilder")
                    else if i + 2 <= chars.len() && chars[i..i+2] == ['F', 'C'] {
                        // Check prefix boundary
                        let prefix_ok = i == 0 || !chars[i-1].is_alphanumeric() && chars[i-1] != '_';
                        // Check suffix boundary
                        let suffix_ok = i + 2 == chars.len() || !chars[i+2].is_alphanumeric() && chars[i+2] != '_';
                        if prefix_ok && suffix_ok {
                            let mut j = i + 2;
                            let mut is_followed_by_angle_bracket = false;
                            while j < chars.len() {
                                if chars[j].is_whitespace() {
                                    j += 1;
                                } else if chars[j] == '<' {
                                    is_followed_by_angle_bracket = true;
                                    break;
                                } else {
                                    break;
                                }
                            }
                            if !is_followed_by_angle_bracket {
                                result.push_str("FC<{ children: React.ReactNode }>");
                                i += 2;
                                continue;
                            }
                        }
                    }
                    result.push(chars[i]);
                    i += 1;
                }
                processed = result;
            }

            // Ensure globals.css is imported in the root layout to load Tailwind styles
            if !processed.contains("globals.css") {
                let trimmed = processed.trim_start();
                if trimmed.starts_with("\"use client\"") {
                    if let Some(idx) = processed.find("\"use client\"") {
                        if let Some(semicolon_idx) = processed[idx..].find(';') {
                            let insert_pos = idx + semicolon_idx + 1;
                            processed.insert_str(insert_pos, "\nimport './globals.css';");
                        } else {
                            let insert_pos = idx + "\"use client\"".len();
                            processed.insert_str(insert_pos, ";\nimport './globals.css';");
                        }
                    }
                } else if trimmed.starts_with("'use client'") {
                    if let Some(idx) = processed.find("'use client'") {
                        if let Some(semicolon_idx) = processed[idx..].find(';') {
                            let insert_pos = idx + semicolon_idx + 1;
                            processed.insert_str(insert_pos, "\nimport './globals.css';");
                        } else {
                            let insert_pos = idx + "'use client'".len();
                            processed.insert_str(insert_pos, ";\nimport './globals.css';");
                        }
                    }
                } else {
                    processed = format!("import './globals.css';\n{}", processed);
                }
            }

            if processed.contains("({ children })") {
                processed = processed.replace("({ children })", "({ children }: { children: React.ReactNode })");
            }
            
            // Inject <html> and <body> if missing
            if !processed.contains("<html") && !processed.contains("<body") {
                if let Some(return_idx) = processed.find("return (") {
                    let insert_pos = return_idx + "return (".len();
                    processed.insert_str(insert_pos, "\n    <html lang=\"en\">\n      <body>");
                    
                    if let Some(last_close) = processed.rfind(");") {
                        processed.insert_str(last_close, "      </body>\n    </html>\n");
                    }
                }
            }
            
            // Auto-wrap root layout in SessionProvider if it uses useSession directly or has next-auth dependency
            let has_nextauth_dep = metamanifest.get("requiredFilesTemplates")
                .and_then(|t| t.as_array())
                .map_or(false, |templates| {
                    templates.iter().any(|temp| {
                        temp.get("mergeDependencies")
                            .and_then(|m| m.get("dependencies"))
                            .and_then(|d| d.as_object())
                            .map_or(false, |deps| deps.contains_key("next-auth"))
                    })
                });

            if processed.contains("useSession") || has_nextauth_dep {
                let mut renamed = false;
                let mut export_name = "RootLayout".to_string();
                
                for possible_name in &["RootLayout", "Layout", "PageLayout", "AppLayout"] {
                    let pattern = format!("export default function {}", possible_name);
                    if processed.contains(&pattern) {
                        processed = processed.replace(&pattern, &format!("function {}Inner", possible_name));
                        export_name = possible_name.to_string();
                        renamed = true;
                        break;
                    }
                }
                
                if !renamed && processed.contains("export default function") {
                    processed = processed.replace("export default function", "function RootLayoutInner");
                    export_name = "RootLayout".to_string();
                    renamed = true;
                }
                
                if !renamed {
                    for possible_name in &["RootLayout", "Layout", "PageLayout", "AppLayout"] {
                        let pattern = format!("export default {}", possible_name);
                        if processed.contains(&pattern) {
                            processed = processed.replace(&format!("const {} =", possible_name), &format!("const {}Inner =", possible_name));
                            processed = processed.replace(&format!("let {} =", possible_name), &format!("let {}Inner =", possible_name));
                            processed = processed.replace(&format!("function {} (", possible_name), &format!("function {}Inner (", possible_name));
                            processed = processed.replace(&format!("function {}(", possible_name), &format!("function {}Inner(", possible_name));
                            processed = processed.replace(&pattern, ""); // Remove the default export statement
                            export_name = possible_name.to_string();
                            renamed = true;
                            break;
                        }
                    }
                }
                
                if renamed {
                    // Ensure SessionProvider is imported
                    if !processed.contains("SessionProvider") {
                        processed = format!("import {{ SessionProvider }} from 'next-auth/react';\n{}", processed);
                    }
                    
                    let is_tsx = path_lower.ends_with(".tsx");
                    let wrapper = if is_tsx {
                        format!(
                            "\nexport default function {}({{ children }}: {{ children: React.ReactNode }}) {{\n  return (\n    <SessionProvider>\n      <{}Inner>{{children}}</{}Inner>\n    </SessionProvider>\n  );\n}}\n",
                            export_name, export_name, export_name
                        )
                    } else {
                        format!(
                            "\nexport default function {}({{ children }}) {{\n  return (\n    <SessionProvider>\n      <{}Inner>{{children}}</{}Inner>\n    </SessionProvider>\n  );\n}}\n",
                            export_name, export_name, export_name
                        )
                    };
                    processed.push_str(&wrapper);
                }
            }
        }

        // NextAuth v3 vs v4 rewrite for route handlers
        if path_lower.contains("src/app/api/auth/[...nextauth]/route.ts") || path_lower.contains("src/app/api/auth/[...nextauth]/route.js") {
            processed = r#"import NextAuth from "next-auth";
import CredentialsProvider from "next-auth/providers/credentials";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";

const authOptions = {
  providers: [
    CredentialsProvider({
      name: "credentials",
      credentials: {
        email: { label: "Email", type: "email" },
        password: { label: "Password", type: "password" },
      },
      async authorize(credentials) {
        if (!credentials?.email || !credentials?.password) return null;
        try {
          const user = await prisma.user.findUnique({ where: { email: credentials.email } });
          if (!user) return null;
          const passwordMatch = await bcrypt.compare(credentials.password, user.hashedPassword || user.password || "");
          if (!passwordMatch) return null;
          return { id: String(user.id), email: user.email, name: user.name };
        } catch (error) {
          console.error("Auth error in CredentialsProvider:", error);
          return null;
        }
      },
    }),
  ],
  session: { strategy: "jwt" as const },
  callbacks: {
    async jwt({ token, user }: any) {
      if (user) {
        token.id = user.id;
      }
      return token;
    },
    async session({ session, token }: any) {
      if (session.user && token) {
        session.user.id = token.id;
      }
      return session;
    },
  },
  pages: {
    signIn: "/login",
  },
  secret: process.env.NEXTAUTH_SECRET || "axiom-dev-secret-change-in-production",
};

const handler = NextAuth(authOptions);
export { handler as GET, handler as POST };
"#.to_string();
        }

        if normalized_path.contains("src/app/") {
            if !path_lower.ends_with("route.ts") && !path_lower.ends_with("route.js") {
                let has_hooks = processed.contains("useState") || processed.contains("useEffect") || 
                                processed.contains("useContext") || processed.contains("useRef") || 
                                processed.contains("useReducer") || processed.contains("useSession") ||
                                processed.contains("useRouter") || processed.contains("usePathname") ||
                                processed.contains("useParams") || processed.contains("useSearchParams");
                if has_hooks && !processed.trim_start().starts_with("\"use client\"") && !processed.trim_start().starts_with("'use client'") {
                    processed = format!("\"use client\";\n\n{}", processed);
                }
            }

            // Prisma Client import normalization
            processed = processed
                .replace("\"../../../../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../../../../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../../../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../../../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../../../prisma\"", "\"@/lib/prisma\"")
                .replace("'../../../prisma'", "'@/lib/prisma'")
                .replace("\"../../prisma\"", "\"@/lib/prisma\"")
                .replace("'../../prisma'", "'@/lib/prisma'")
                .replace("\"../prisma\"", "\"@/lib/prisma\"")
                .replace("'../prisma'", "'@/lib/prisma'")
                .replace("\"@/prisma\"", "\"@/lib/prisma\"")
                .replace("'@/prisma'", "'@/lib/prisma'");

            // React Router Dom to Next Link/Navigation rewrite for Next.js App Router
            if processed.contains("react-router-dom") {
                processed = processed
                    .replace("from 'react-router-dom'", "from 'next/link'")
                    .replace("from \"react-router-dom\"", "from \"next/link\"")
                    .replace("import { Link } from 'next/link'", "import Link from 'next/link'")
                    .replace("import { Link } from \"next/link\"", "import Link from \"next/link\"")
                    .replace("NavLink", "Link");
            }

            // Clean invalid page level react-router configurations
            if (path_lower.ends_with("page.tsx") || path_lower.ends_with("page.jsx")) && 
               (processed.contains("<BrowserRouter") || processed.contains("<Switch") || processed.contains("<Route")) {
                processed = r#"import React from 'react';
import Link from 'next/link';

export default function Home() {
  return (
    <main className="min-h-screen bg-slate-950 text-white flex flex-col items-center justify-center p-6 font-sans">
      <div className="max-w-3xl text-center space-y-6">
        <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-indigo-500/10 border border-indigo-500/20 text-indigo-400 text-sm font-medium">
          <span className="w-2 h-2 rounded-full bg-indigo-400 animate-pulse"></span>
          Axiom Forge Dev Preview
        </div>
        <h1 className="text-5xl font-extrabold tracking-tight bg-gradient-to-r from-indigo-400 via-purple-400 to-pink-400 bg-clip-text text-transparent pb-2">
          Project Generated Successfully
        </h1>
        <p className="text-lg text-slate-400 max-w-xl mx-auto">
          Your Next.js App Router application has been successfully generated and compiled. 
          Use this preview to explore your project structures and database collections.
        </p>
        <div className="flex flex-wrap justify-center gap-4 pt-4">
          <Link href="/api/auth/signin" className="px-6 py-3 bg-indigo-600 hover:bg-indigo-700 text-white font-medium rounded-lg transition-all shadow-lg shadow-indigo-600/20 hover:scale-[1.02]">
            Sign In / Register
          </Link>
          <a href="https://nextjs.org/docs" target="_blank" rel="noopener noreferrer" className="px-6 py-3 bg-slate-900 hover:bg-slate-800 text-slate-200 font-medium rounded-lg transition-colors border border-slate-800">
            Next.js Documentation
          </a>
        </div>
      </div>
    </main>
  );
}
"#.to_string();
            }

            // Router query normalization
            if processed.contains("router.query") {
                processed = processed.replace("router.query", "useParams()");
                if processed.contains("useRouter") && !processed.contains("useParams") {
                    processed = processed.replace("useRouter", "useRouter, useParams");
                }
            }

            // Router imports normalization
            processed = processed
                .replace("from 'next/router'", "from 'next/navigation'")
                .replace("from \"next/router\"", "from \"next/navigation\"");
                
            // Resend dummy key fallback
            processed = processed
                .replace("new Resend(process.env.RESEND_API_KEY)", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY as string)", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY!)", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY || \"\")", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY || '')", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')");
        }
    }

    processed
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct PackageMeta {
    name: String,
    version: String,
    aliases: Vec<String>,
    #[serde(rename = "importExamples")]
    import_examples: Vec<String>,
    compatibility: serde_json::Map<String, Value>,
    #[serde(rename = "bestPractices")]
    best_practices: String,
}

fn ensure_packages_db_exists() -> std::path::PathBuf {
    let base_dir = get_base_dir();
    let _ = std::fs::create_dir_all(&base_dir);
    let db_path = base_dir.join("axiom-packages-db.json");
    
    let mut needs_write = !db_path.exists();
    if db_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&db_path) {
            if !content.contains("@mui/material") {
                needs_write = true;
            }
        }
    }
    
    if needs_write {
        let default_db = serde_json::json!({
            "packages": [
                {
                    "name": "resend",
                    "version": "^3.2.0",
                    "aliases": ["@resend/client", "@resend/base", "resend-node", "resend/client"],
                    "importExamples": ["import { Resend } from 'resend';"],
                    "compatibility": {
                        "node": ">=18.0.0",
                        "next": ">=13.0.0"
                    },
                    "bestPractices": "Always instantiate Resend client using: const resend = new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview');"
                },
                {
                    "name": "next-auth",
                    "version": "^4.24.7",
                    "aliases": ["@next-auth/client", "next-auth/client"],
                    "importExamples": [
                        "import { getServerSession } from 'next-auth';",
                        "import NextAuth from 'next-auth';",
                        "import { useSession } from 'next-auth/react';"
                    ],
                    "compatibility": {
                        "node": ">=16.0.0",
                        "next": ">=12.0.0"
                    },
                    "bestPractices": "Use NextAuth(authOptions) in API routes. Use useSession() hook on the client side with SessionProvider."
                },
                {
                    "name": "@next-auth/prisma-adapter",
                    "version": "^1.0.7",
                    "aliases": [],
                    "importExamples": ["import { PrismaAdapter } from '@next-auth/prisma-adapter';"],
                    "compatibility": {
                        "next-auth": ">=4.0.0",
                        "prisma": ">=4.0.0"
                    },
                    "bestPractices": "Pass the PrismaClient instance to PrismaAdapter(prisma) in NextAuth configuration."
                },
                {
                    "name": "zod",
                    "version": "^3.23.8",
                    "aliases": ["zod-validator"],
                    "importExamples": ["import { z } from 'zod';"],
                    "compatibility": {},
                    "bestPractices": "Define schemas using z.object({...}). Validate input using schema.safeParse(data)."
                },
                {
                    "name": "react-hook-form",
                    "version": "^7.51.5",
                    "aliases": [],
                    "importExamples": ["import { useForm } from 'react-hook-form';"],
                    "compatibility": {},
                    "bestPractices": "Use register function to bind inputs. Use formState.errors for validations."
                },
                {
                    "name": "@hookform/resolvers",
                    "version": "^3.4.2",
                    "aliases": [],
                    "importExamples": ["import { zodResolver } from '@hookform/resolvers/zod';"],
                    "compatibility": {
                        "react-hook-form": ">=7.0.0"
                    },
                    "bestPractices": "Combine with zod to validate react-hook-form: useForm({ resolver: zodResolver(schema) })."
                },
                {
                    "name": "axios",
                    "version": "^1.7.2",
                    "aliases": [],
                    "importExamples": ["import axios from 'axios';"],
                    "compatibility": {},
                    "bestPractices": "Create an Axios instance with base URL for modular requests: axios.create({...})."
                },
                {
                    "name": "lucide-react",
                    "version": "^0.460.0",
                    "aliases": [],
                    "importExamples": ["import { Mail, Settings } from 'lucide-react';"],
                    "compatibility": {},
                    "bestPractices": "Import only necessary icons to enable tree shaking and reduce bundle sizes."
                },
                {
                    "name": "framer-motion",
                    "version": "^11.2.10",
                    "aliases": [],
                    "importExamples": ["import { motion } from 'framer-motion';"],
                    "compatibility": {},
                    "bestPractices": "Wrap animated components with motion.div. Use AnimatePresence for exit animations."
                },
                {
                    "name": "recharts",
                    "version": "^2.12.7",
                    "aliases": [],
                    "importExamples": ["import { ResponsiveContainer, AreaChart, Area, XAxis, YAxis, Tooltip } from 'recharts';"],
                    "compatibility": {},
                    "bestPractices": "Wrap Recharts charts in ResponsiveContainer to ensure responsiveness."
                },
                {
                    "name": "react-icons",
                    "version": "^5.2.1",
                    "aliases": [],
                    "importExamples": ["import { FaMailBulk } from 'react-icons/fa';", "import { IoMdNotifications } from 'react-icons/io';"],
                    "compatibility": {},
                    "bestPractices": "Import icons individually to reduce bundle size."
                },
                {
                    "name": "clsx",
                    "version": "^2.1.1",
                    "aliases": [],
                    "importExamples": ["import clsx from 'clsx';"],
                    "compatibility": {},
                    "bestPractices": "Construct className strings conditionally."
                },
                {
                    "name": "tailwind-merge",
                    "version": "^2.3.0",
                    "aliases": [],
                    "importExamples": ["import { twMerge } from 'tailwind-merge';"],
                    "compatibility": {},
                    "bestPractices": "Merge Tailwind CSS classes without conflicts."
                },
                {
                    "name": "class-variance-authority",
                    "version": "^0.7.0",
                    "aliases": [],
                    "importExamples": ["import { cva } from 'class-variance-authority';"],
                    "compatibility": {},
                    "bestPractices": "Create UI variants easily with cva."
                },
                {
                    "name": "uuid",
                    "version": "^9.0.1",
                    "aliases": [],
                    "importExamples": ["import { v4 as uuidv4 } from 'uuid';"],
                    "compatibility": {},
                    "bestPractices": "Generate random UUID v4."
                },
                {
                    "name": "bcrypt",
                    "version": "^5.1.1",
                    "aliases": ["bcryptjs-alias"],
                    "importExamples": ["import bcrypt from 'bcrypt';"],
                    "compatibility": {},
                    "bestPractices": "Hash passwords asynchronously using bcrypt.hash(pwd, 10)."
                },
                {
                    "name": "jsonwebtoken",
                    "version": "^9.0.2",
                    "aliases": [],
                    "importExamples": ["import jwt from 'jsonwebtoken';"],
                    "compatibility": {},
                    "bestPractices": "Sign and verify JWT tokens securely."
                },
                {
                    "name": "@mui/material",
                    "version": "^5.15.0",
                    "aliases": ["@material-ui/core", "@mui/core"],
                    "importExamples": [
                        "import { Button, AppBar, Toolbar, Typography } from '@mui/material';",
                        "import { styled } from '@mui/material/styles';"
                    ],
                    "compatibility": {
                        "react": ">=17.0.0",
                        "@emotion/react": "^11.0.0",
                        "@emotion/styled": "^11.0.0"
                    },
                    "bestPractices": "Import components from @mui/material. Use custom styles with styled utility."
                },
                {
                    "name": "@mui/icons-material",
                    "version": "^5.15.0",
                    "aliases": ["@material-ui/icons"],
                    "importExamples": ["import { Mail, Settings, Notifications } from '@mui/icons-material';"],
                    "compatibility": {
                        "@mui/material": ">=5.0.0"
                    },
                    "bestPractices": "Import icons individually or destructured. Make sure @mui/material is installed."
                },
                {
                    "name": "@prisma/client",
                    "version": "^5.14.0",
                    "aliases": [],
                    "importExamples": ["import { PrismaClient } from '@prisma/client';"],
                    "compatibility": {
                        "prisma": ">=5.0.0"
                    },
                    "bestPractices": "Instantiate PrismaClient globally to avoid multiple connection pools in hot reload environments. Prisma client MUST only be imported and used inside server-side code (Next.js API route handlers), never in React hooks or client components ('use client')."
                },
                {
                    "name": "prisma",
                    "version": "^5.14.0",
                    "aliases": [],
                    "importExamples": [],
                    "compatibility": {},
                    "bestPractices": "Use prisma developer CLI commands via npx prisma."
                }
            ]
        });
        let _ = std::fs::write(&db_path, serde_json::to_string_pretty(&default_db).unwrap());
    }
    db_path
}

fn load_packages_db() -> Vec<PackageMeta> {
    let db_path = ensure_packages_db_exists();
    if let Ok(content) = std::fs::read_to_string(&db_path) {
        if let Ok(db_val) = serde_json::from_str::<Value>(&content) {
            if let Some(arr) = db_val.get("packages").and_then(|p| p.as_array()) {
                let mut pkgs = Vec::new();
                for item in arr {
                    if let Ok(pkg) = serde_json::from_value::<PackageMeta>(item.clone()) {
                        pkgs.push(pkg);
                    }
                }
                return pkgs;
            }
        }
    }
    Vec::new()
}

fn resolve_and_rewrite_import(imported_path: &str, packages_db: &[PackageMeta]) -> Option<String> {
    let imported_path_trimmed = imported_path.trim();
    if imported_path_trimmed.is_empty() {
        return None;
    }

    if imported_path_trimmed == "bcrypt" {
        return Some("bcryptjs".to_string());
    }
    if imported_path_trimmed == "node-base64" {
        return Some("js-base64".to_string());
    }
    if imported_path_trimmed == "node-sass" {
        return Some("sass".to_string());
    }
    
    // 1. Check if it's a relative import or absolute path in the project
    if imported_path_trimmed.starts_with('.') || imported_path_trimmed.starts_with('/') || imported_path_trimmed.starts_with("@/") {
        return None;
    }
    
    // We also skip common local aliases that nextjs/react uses
    let local_aliases = ["@components", "@hooks", "@utils", "@lib", "@styles", "@services", "@context", "@types", "@pages", "@app", "@assets", "@config"];
    for &la in &local_aliases {
        if imported_path_trimmed == la || imported_path_trimmed.starts_with(&format!("{}/", la)) {
            return None;
        }
    }
    
    // 2. Check for exact match with aliases first
    for pkg in packages_db {
        if pkg.aliases.contains(&imported_path_trimmed.to_string()) {
            // Special case for next-auth client side legacy alias
            if pkg.name == "next-auth" && (imported_path_trimmed == "@next-auth/client" || imported_path_trimmed == "next-auth/client") {
                return Some("next-auth/react".to_string());
            }
            return Some(pkg.name.clone());
        }
    }
    
    // 3. Check if the imported path starts with a package name or its aliases followed by "/"
    for pkg in packages_db {
        // Check package name match
        if imported_path_trimmed.starts_with(&format!("{}/", pkg.name)) {
            // Verify if this specific subpath is listed as valid in any import examples
            let mut is_valid_subpath = false;
            for example in &pkg.import_examples {
                if example.contains(imported_path_trimmed) {
                    is_valid_subpath = true;
                    break;
                }
            }
            if !is_valid_subpath {
                // It's a non-existent/hallucinated subpath of a known package (e.g. resend/client)
                // Rewrite to the package name itself
                return Some(pkg.name.clone());
            }
        }
        
        // Check aliases matches
        for alias in &pkg.aliases {
            if imported_path_trimmed.starts_with(&format!("{}/", alias)) {
                // If it starts with an alias, we check if it matches some known subpath, otherwise rewrite to pkg.name
                let mut is_valid_subpath = false;
                for example in &pkg.import_examples {
                    if example.contains(imported_path_trimmed) {
                        is_valid_subpath = true;
                        break;
                    }
                }
                if !is_valid_subpath {
                    // E.g. @resend/client/something -> resend
                    return Some(pkg.name.clone());
                }
            }
        }
    }
    
    None
}

fn get_packages_db_summary() -> String {
    let mut pkg_db_summary = "\nAvailable NPM package names, stable versions, import formats, compatibility, and integration best practices. ALWAYS use these exact package names and import syntaxes. Do NOT hallucinate other packages or subpaths:\n".to_string();
    let db_path = get_base_dir().join("axiom-packages-db.json");
    if db_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&db_path) {
            if let Ok(db_val) = serde_json::from_str::<Value>(&content) {
                if let Some(arr) = db_val.get("packages").and_then(|p| p.as_array()) {
                    for item in arr {
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let version = item.get("version").and_then(|v| v.as_str()).unwrap_or("");
                        let examples = item.get("importExamples").and_then(|e| e.as_array());
                        let best_practices = item.get("bestPractices").and_then(|b| b.as_str()).unwrap_or("");
                        let compat = item.get("compatibility").and_then(|c| c.as_object());
                        
                        if !name.is_empty() {
                            pkg_db_summary.push_str(&format!("- Package: '{}' (Version: {})\n", name, version));
                            if let Some(ex_arr) = examples {
                                for ex in ex_arr {
                                    if let Some(ex_str) = ex.as_str() {
                                        pkg_db_summary.push_str(&format!("  * Import Syntax: {}\n", ex_str));
                                    }
                                }
                            }
                            if let Some(c_obj) = compat {
                                if !c_obj.is_empty() {
                                    let mut compat_parts = Vec::new();
                                    for (k, v) in c_obj {
                                        compat_parts.push(format!("{} ({})", k, v.as_str().unwrap_or("*")));
                                    }
                                    pkg_db_summary.push_str(&format!("  * Compatibility: {}\n", compat_parts.join(", ")));
                                }
                            }
                            if !best_practices.is_empty() {
                                pkg_db_summary.push_str(&format!("  * Best Practices: {}\n", best_practices));
                            }
                        }
                    }
                }
            }
        }
    }
    pkg_db_summary
}

pub fn repair_package_json_dependencies(proj_dir: &std::path::Path) {
    let pkg_path = proj_dir.join("package.json");
    if !pkg_path.exists() {
        return;
    }
    
    let packages_db = load_packages_db();
    let mut imports_found = std::collections::HashSet::new();
    
    // Scan directory and perform alias rewriting inline
    let mut scan_dir = |dir: &std::path::Path| {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            if let Ok(entries) = std::fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name != "node_modules" && name != ".next" && name != ".git" {
                            stack.push(path);
                        }
                    } else if path.is_file() {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let mut file_changed = false;
                                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                                
                                for line in lines.iter_mut() {
                                    let trimmed = line.trim();
                                    if (trimmed.starts_with("import ") || trimmed.starts_with("import{") || trimmed.starts_with("import type") || trimmed.contains("require(")) 
                                        && (trimmed.contains('\'') || trimmed.contains('"') || trimmed.contains('`')) 
                                    {
                                        let chars: Vec<char> = line.chars().collect();
                                        let mut i = 0;
                                        while i < chars.len() {
                                            if chars[i] == '\'' || chars[i] == '"' || chars[i] == '`' {
                                                let quote = chars[i];
                                                i += 1;
                                                let start = i;
                                                while i < chars.len() && chars[i] != quote {
                                                    i += 1;
                                                }
                                                if i < chars.len() {
                                                    let literal: String = chars[start..i].iter().collect();
                                                    let literal = literal.trim();
                                                    
                                                    // Check if it's an import we should rewrite
                                                    if let Some(correct_path) = resolve_and_rewrite_import(&literal, &packages_db) {
                                                        let old_quote = format!("{}{}{}", quote, literal, quote);
                                                        let new_quote = format!("{}{}{}", quote, correct_path, quote);
                                                        *line = line.replace(&old_quote, &new_quote);
                                                        file_changed = true;
                                                    }
                                                }
                                            }
                                            i += 1;
                                        }
                                    }
                                }
                                
                                let content_to_parse = if file_changed {
                                    let joined = lines.join("\n");
                                    let _ = std::fs::write(&path, &joined);
                                    joined
                                } else {
                                    content
                                };
                                
                                // Parse import/require paths for package.json dependency gathering
                                for line in content_to_parse.lines() {
                                    let line = line.trim();
                                    if (line.starts_with("import ") || line.starts_with("import{") || line.starts_with("import type") || line.contains("require(")) && (line.contains('\'') || line.contains('"') || line.contains('`')) {
                                        let chars: Vec<char> = line.chars().collect();
                                        let mut i = 0;
                                        while i < chars.len() {
                                            if chars[i] == '\'' || chars[i] == '"' || chars[i] == '`' {
                                                let quote = chars[i];
                                                i += 1;
                                                let start = i;
                                                while i < chars.len() && chars[i] != quote {
                                                    i += 1;
                                                }
                                                if i < chars.len() {
                                                    let literal: String = chars[start..i].iter().collect();
                                                    let literal = literal.trim();
                                                    if !literal.is_empty() && !literal.starts_with('.') && !literal.starts_with('/') && !literal.starts_with("@/") {
                                                        let parts: Vec<&str> = literal.split('/').collect();
                                                        if !parts.is_empty() {
                                                            let local_aliases = ["@components", "@hooks", "@utils", "@lib", "@styles", "@services", "@context", "@types", "@pages", "@app", "@assets", "@config"];
                                                            if !local_aliases.contains(&parts[0]) {
                                                                let pkg = if parts[0].starts_with('@') && parts.len() > 1 {
                                                                    format!("{}/{}", parts[0], parts[1])
                                                                } else {
                                                                    parts[0].to_string()
                                                                };
                                                                let node_builtins = ["fs", "path", "http", "https", "os", "crypto", "stream", "util", "events", "assert", "dns", "net", "tls", "querystring", "url", "zlib", "buffer", "child_process", "cluster", "dgram", "process", "readline", "vm"];
                                                                if !node_builtins.contains(&pkg.as_str()) && pkg != "react" && pkg != "react-dom" && pkg != "next" {
                                                                    // Map to correct package if it matches an alias
                                                                    let mut mapped_pkg = pkg.clone();
                                                                    for db_pkg in &packages_db {
                                                                        if db_pkg.aliases.contains(&pkg) {
                                                                            mapped_pkg = db_pkg.name.clone();
                                                                            break;
                                                                        }
                                                                    }
                                                                    imports_found.insert(mapped_pkg);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            i += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    
    let has_tsconfig = proj_dir.join("tsconfig.json").exists();
    scan_dir(proj_dir);
    
    if let Ok(pkg_content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(mut pkg_json) = serde_json::from_str::<Value>(&pkg_content) {
            if let Some(pkg_obj) = pkg_json.as_object_mut() {
                pkg_obj.entry("dependencies").or_insert(serde_json::json!({}));
                if has_tsconfig {
                    pkg_obj.entry("devDependencies").or_insert(serde_json::json!({}));
                }
                
                let mut changed = false;
                
                if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    if deps_obj.remove("bcrypt").is_some() {
                        deps_obj.insert("bcryptjs".to_string(), serde_json::json!("^2.4.3"));
                        changed = true;
                    }
                    if deps_obj.remove("node-base64").is_some() {
                        deps_obj.insert("js-base64".to_string(), serde_json::json!("^3.7.7"));
                        changed = true;
                    }
                    if deps_obj.remove("node-sass").is_some() {
                        deps_obj.insert("sass".to_string(), serde_json::json!("^1.77.0"));
                        changed = true;
                    }
                }

                // Clean up any dependencies that are aliases of known packages in the DB
                for pkg in &packages_db {
                    for alias in &pkg.aliases {
                        if let Some(deps) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if deps.remove(alias).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(dev_deps) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                            if dev_deps.remove(alias).is_some() {
                                changed = true;
                            }
                        }
                    }
                }
                for imp in imports_found {
                    // Try to find the version in the database
                    let mut version = None;
                    for pkg in &packages_db {
                        if pkg.name == imp {
                            version = Some(pkg.version.clone());
                            break;
                        }
                    }
                    
                    if let Some(v) = version {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key(&imp) {
                                deps_obj.insert(imp.clone(), serde_json::json!(v));
                                changed = true;
                            }
                        }
                    } else if imp == "prisma" {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key("@prisma/client") {
                                deps_obj.insert("@prisma/client".to_string(), serde_json::json!("^5.14.0"));
                                changed = true;
                            }
                        }
                    } else if imp == "@mui/material" {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key("@mui/material") {
                                deps_obj.insert("@mui/material".to_string(), serde_json::json!("^5.15.0"));
                                changed = true;
                            }
                            if !deps_obj.contains_key("@emotion/react") {
                                deps_obj.insert("@emotion/react".to_string(), serde_json::json!("^11.11.0"));
                                changed = true;
                            }
                            if !deps_obj.contains_key("@emotion/styled") {
                                deps_obj.insert("@emotion/styled".to_string(), serde_json::json!("^11.11.0"));
                                changed = true;
                            }
                        }
                    } else if imp == "@mui/icons-material" {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key("@mui/icons-material") {
                                deps_obj.insert("@mui/icons-material".to_string(), serde_json::json!("^5.15.0"));
                                changed = true;
                            }
                        }
                    } else {
                        if let Some(deps_obj) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                            if !deps_obj.contains_key(&imp) {
                                deps_obj.insert(imp.clone(), serde_json::json!("*"));
                                changed = true;
                            }
                        }
                    }
                    
                    if has_tsconfig {
                        let type_pkg = match imp.as_str() {
                            "bcrypt" => Some(("@types/bcrypt", "^5.0.2")),
                            "bcryptjs" => Some(("@types/bcryptjs", "^2.4.6")),
                            "jsonwebtoken" => Some(("@types/jsonwebtoken", "^9.0.6")),
                            "uuid" => Some(("@types/uuid", "^9.0.8")),
                            "canvas-confetti" => Some(("@types/canvas-confetti", "^1.6.4")),
                            _ => None
                        };
                        if let Some((tp_name, tp_ver)) = type_pkg {
                            if let Some(dev_deps_obj) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                                if !dev_deps_obj.contains_key(tp_name) {
                                    dev_deps_obj.insert(tp_name.to_string(), serde_json::json!(tp_ver));
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                
                if changed {
                    if let Ok(pretty) = serde_json::to_string_pretty(&pkg_json) {
                        let _ = std::fs::write(&pkg_path, pretty);
                    }
                }
            }
        }
    }
}

fn _find_404_package(err_log: &str) -> Option<String> {
    if let Some(idx) = err_log.find("requested resource '") {
        let after = &err_log[idx + 20..];
        if let Some(end_idx) = after.find('\'') {
            let resource_str = &after[..end_idx];
            if let Some(at_idx) = resource_str.rfind('@') {
                if at_idx > 0 {
                    return Some(resource_str[..at_idx].to_string());
                }
            }
        }
    }
    if let Some(idx) = err_log.find("https://registry.npmjs.org/") {
        let after = &err_log[idx + 27..];
        let end_idx = after.find(' ').unwrap_or_else(|| after.find('\n').unwrap_or(after.len()));
        let url_pkg = &after[..end_idx];
        let decoded = url_pkg.replace("%2f", "/").replace("%2F", "/");
        if !decoded.is_empty() {
            return Some(decoded);
        }
    }
    None
}

fn remove_package_from_json(pkg_path: &std::path::Path, pkg_name: &str) -> bool {
    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            if let Some(obj) = json_val.as_object_mut() {
                if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    if deps.remove(pkg_name).is_some() {
                        changed = true;
                    }
                }
                if let Some(dev_deps) = obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                    if dev_deps.remove(pkg_name).is_some() {
                        changed = true;
                    }
                }
            }
            if changed {
                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                    let _ = std::fs::write(pkg_path, pretty);
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Debug, Clone)]
enum NpmFailureAction {
    Remove(String),
    SetWildcard(String),
}

fn analyze_npm_install_failure(err_log: &str) -> Option<NpmFailureAction> {
    // 1. Check for ETARGET / notarget (version mismatch)
    if let Some(idx) = err_log.find("No matching version found for ") {
        let after = &err_log[idx + 30..];
        let line = after.lines().next().unwrap_or("").trim();
        let mut cleaned = line.to_string();
        while cleaned.ends_with('.') || cleaned.ends_with('\'') || cleaned.ends_with('"') || cleaned.ends_with(';') {
            cleaned.pop();
        }
        if let Some(at_idx) = cleaned.rfind('@') {
            if at_idx > 0 {
                let pkg_name = &cleaned[..at_idx];
                return Some(NpmFailureAction::SetWildcard(pkg_name.to_string()));
            }
        }
    }

    // 2. Check for E404 (NotFound) - original/legacy format
    if let Some(idx) = err_log.find("requested resource '") {
        let after = &err_log[idx + 20..];
        if let Some(end_idx) = after.find('\'') {
            let resource_str = &after[..end_idx];
            if let Some(at_idx) = resource_str.rfind('@') {
                if at_idx > 0 {
                    return Some(NpmFailureAction::Remove(resource_str[..at_idx].to_string()));
                }
            }
            return Some(NpmFailureAction::Remove(resource_str.to_string()));
        }
    }

    // 3. Check for E404 (NotFound) - registry not found format
    if let Some(idx) = err_log.find(" is not in the npm registry") {
        let before = &err_log[..idx];
        let parts: Vec<&str> = before.split('\'').collect();
        if parts.len() >= 2 {
            let pkg_name = parts[parts.len() - 2];
            return Some(NpmFailureAction::Remove(pkg_name.to_string()));
        }
    }

    // 4. Check for E404 (NotFound) - GET url format
    if let Some(idx) = err_log.find("404 Not Found") {
        let line = err_log[idx..].lines().next().unwrap_or("");
        if let Some(get_idx) = line.find("GET ") {
            let after_get = &line[get_idx + 4..];
            let url_token = after_get.split_whitespace().next().unwrap_or("");
            if let Some(last_slash) = url_token.rfind('/') {
                let pkg_name = &url_token[last_slash + 1..];
                let pkg_name = pkg_name.split('?').next().unwrap_or("").split('#').next().unwrap_or("");
                if !pkg_name.is_empty() && pkg_name != "index" {
                    return Some(NpmFailureAction::Remove(pkg_name.to_string()));
                }
            }
        }
    }

    None
}

fn set_package_version_to_wildcard(pkg_path: &std::path::Path, pkg_name: &str) -> bool {
    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            if let Some(obj) = json_val.as_object_mut() {
                if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                    if deps.contains_key(pkg_name) {
                        deps.insert(pkg_name.to_string(), serde_json::json!("*"));
                        changed = true;
                    }
                }
                if let Some(dev_deps) = obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                    if dev_deps.contains_key(pkg_name) {
                        dev_deps.insert(pkg_name.to_string(), serde_json::json!("*"));
                        changed = true;
                    }
                }
            }
            if changed {
                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                    let _ = std::fs::write(pkg_path, pretty);
                    return true;
                }
            }
        }
    }
    false
}

fn lock_installed_dependencies(pkg_path: &std::path::Path) {
    let parent_dir = match pkg_path.parent() {
        Some(p) => p,
        None => return,
    };
    if let Ok(content) = std::fs::read_to_string(pkg_path) {
        if let Ok(mut json_val) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            if let Some(obj) = json_val.as_object_mut() {
                for deps_key in &["dependencies", "devDependencies"] {
                    if let Some(deps) = obj.get_mut(*deps_key).and_then(|d| d.as_object_mut()) {
                        for (pkg_name, version_val) in deps.iter_mut() {
                            if let Some(version_str) = version_val.as_str() {
                                if version_str == "*" || version_str == "latest" || version_str.is_empty() {
                                    let node_pkg_json_path = parent_dir.join("node_modules").join(pkg_name).join("package.json");
                                    if node_pkg_json_path.exists() {
                                        if let Ok(node_content) = std::fs::read_to_string(&node_pkg_json_path) {
                                            if let Ok(node_val) = serde_json::from_str::<Value>(&node_content) {
                                                if let Some(installed_version) = node_val.get("version").and_then(|v| v.as_str()) {
                                                    let locked_ver = if installed_version.chars().next().map_or(false, |c| c.is_numeric()) {
                                                        format!("^{}", installed_version)
                                                    } else {
                                                        installed_version.to_string()
                                                    };
                                                    debug_log_to_file(format!("[Dependency Installer] Locking {} version to {}", pkg_name, locked_ver));
                                                    *version_val = serde_json::json!(locked_ver);
                                                    changed = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if changed {
                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                    let _ = std::fs::write(pkg_path, pretty);
                }
            }
        }
    }
}

async fn run_npm_install_with_auto_healing(
    proj_dir: &std::path::Path,
    window: &tauri::Window,
    log_event: &str,
    task_id_opt: Option<&str>,
) -> bool {
    let pkg_path = proj_dir.join("package.json");
    if !pkg_path.exists() {
        return false;
    }

    use tauri::Manager;
    let mut project_id = proj_dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    if project_id == "src" {
        if let Some(parent) = proj_dir.parent() {
            project_id = parent.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        }
    }

    if let Some(state) = window.try_state::<ServerState>() {
        let child_opt = {
            let mut servers = state.0.lock().unwrap();
            servers.remove(&project_id)
        };
        if let Some(mut child) = child_opt {
            if !log_event.is_empty() {
                let _ = window.emit(log_event, serde_json::json!({
                    "text": format!("> [Self-Healing] Zaustavljanje aktivnog dev servera za projekat '{}' radi sprečavanja EPERM file lock-a...\n", project_id),
                    "type": "info"
                }));
            }
            #[cfg(target_os = "windows")]
            {
                if let Some(pid) = child.id() {
                    let mut kill_cmd = tokio::process::Command::new("taskkill");
                    kill_cmd.arg("/F").arg("/T").arg("/PID").arg(pid.to_string());
                    kill_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                    let _ = kill_cmd.spawn();
                }
            }
            let _ = child.kill().await;
            let _ = window.emit("server:status", serde_json::json!({
                "status": "stopped"
            }));
        }
    }

    let emit_log = |text: &str, is_error: bool| {
        if !log_event.is_empty() {
            let _ = window.emit(log_event, serde_json::json!({
                "text": text.to_string(),
                "type": if is_error { "error" } else { "info" }
            }));
        }
        debug_log_to_file(format!("[Dependency Installer] {}", text.trim()));
    };

    let emit_progress = |msg: &str, progress: f64| {
        if let Some(task_id) = task_id_opt {
            let _ = window.emit("task:progress", serde_json::json!({
                "taskId": task_id,
                "phase": "install-deps",
                "message": msg,
                "progress": progress
            }));
        }
    };

    emit_progress("Finalizing dependencies...", 75.0);
    emit_log("> Starting dependency installation check...\n", false);

    for attempt in 1..=4 {
        emit_progress(&format!("Installing dependencies (attempt {}/4)...", attempt), 75.0 + (attempt as f64 - 1.0) * 0.7);
        emit_log(&format!("> Running npm install, attempt {}/4...\n", attempt), false);

        #[cfg(target_os = "windows")]
        let mut install_cmd = tokio::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        install_cmd.arg("/C").arg("npm").arg("install").arg("--legacy-peer-deps").current_dir(proj_dir);

        #[cfg(not(target_os = "windows"))]
        let mut install_cmd = tokio::process::Command::new("npm");
        #[cfg(not(target_os = "windows"))]
        install_cmd.arg("install").arg("--legacy-peer-deps").current_dir(proj_dir);

        #[cfg(target_os = "windows")]
        install_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        install_cmd.stdout(std::process::Stdio::piped());
        install_cmd.stderr(std::process::Stdio::piped());

        let mut child = match install_cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                emit_log(&format!("> Failed to start npm install command: {}\n", e), true);
                break;
            }
        };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let window_log = window.clone();
        let log_event_str = log_event.to_string();
        let stdout_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stdout).lines();
            let mut collected = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                if !log_event_str.is_empty() {
                    let _ = window_log.emit(&log_event_str, serde_json::json!({
                        "text": format!("> [npm install] {}\n", line),
                        "type": "info"
                    }));
                }
                collected.push(line);
            }
            collected.join("\n")
        });

        let window_err = window.clone();
        let log_event_err_str = log_event.to_string();
        let stderr_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stderr).lines();
            let mut collected = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                if !log_event_err_str.is_empty() {
                    let is_warn = line.contains("npm warn") || line.contains("npm WARN") || line.contains("npm depr") || line.contains("npm DEPR") || line.contains("npm notice") || line.contains("npm NOTICE");
                    let prefix = if is_warn { "> [npm install warn]" } else { "> [npm install err]" };
                    let log_type = if is_warn { "info" } else { "error" };
                    let _ = window_err.emit(&log_event_err_str, serde_json::json!({
                        "text": format!("{} {}\n", prefix, line),
                        "type": log_type
                    }));
                }
                collected.push(line);
            }
            collected.join("\n")
        });

        let status = child.wait().await;
        let stdout_log = stdout_handle.await.unwrap_or_default();
        let stderr_log = stderr_handle.await.unwrap_or_default();
        let err_log = format!("{}\n{}", stdout_log, stderr_log);

        match status {
            Ok(exit_status) if exit_status.success() => {
                emit_log("> npm install completed successfully.\n", false);
                lock_installed_dependencies(&pkg_path);
                return true;
            }
            Ok(exit_status) => {
                emit_log(&format!("> npm install failed with exit status: {}\n", exit_status), true);
            }
            Err(e) => {
                emit_log(&format!("> Error waiting for npm install: {}\n", e), true);
            }
        }

        if let Some(action) = analyze_npm_install_failure(&err_log) {
            match action {
                NpmFailureAction::SetWildcard(pkg_name) => {
                    emit_log(&format!("> [Self-Healing] Package '{}' has no matching version. Updating its version to '*' in package.json and retrying...\n", pkg_name), false);
                    set_package_version_to_wildcard(&pkg_path, &pkg_name);
                }
                NpmFailureAction::Remove(pkg_name) => {
                    emit_log(&format!("> [Self-Healing] Package '{}' not found in npm registry. Removing from package.json and retrying...\n", pkg_name), false);
                    remove_package_from_json(&pkg_path, &pkg_name);
                }
            }
        } else {
            emit_log("> [Self-Healing] No specific package error parsed. Deleting package-lock.json and retrying...\n", false);
            let lock_path = proj_dir.join("package-lock.json");
            if lock_path.exists() {
                let _ = std::fs::remove_file(lock_path);
            }
        }
    }

    false
}

fn generate_axiom_features_json(proj_dir: &std::path::Path, project_name: &str, files_list: &[Value]) {
    let mut files_by_dir: HashMap<String, Vec<Value>> = HashMap::new();
    
    for file_spec in files_list {
        if let Some(path_str) = file_spec.get("path").and_then(|p| p.as_str()) {
            let normalized = path_str.replace('\\', "/");
            let parts: Vec<&str> = normalized.split('/').collect();
            let top_dir = if parts.len() > 1 { parts[0].to_string() } else { "__root__".to_string() };
            
            files_by_dir.entry(top_dir).or_insert_with(Vec::new).push(file_spec.clone());
        }
    }

    let mut features = Vec::new();
    for (dir, dir_files) in files_by_dir {
        let feature_name = match dir.to_lowercase().as_str() {
            "components" => "UI Components".to_string(),
            "pages" => "Pages & Routes".to_string(),
            "api" => "API & Backend".to_string(),
            "routes" => "Routes".to_string(),
            "controllers" => "Controllers".to_string(),
            "models" => "Data Models".to_string(),
            "views" => "Views & Templates".to_string(),
            "utils" => "Utilities".to_string(),
            "hooks" => "React Hooks".to_string(),
            "context" => "App State & Context".to_string(),
            "store" => "State Management".to_string(),
            "auth" => "Authentication".to_string(),
            "lib" => "Core Library".to_string(),
            "styles" => "Styles".to_string(),
            "__root__" => "Root Files".to_string(),
            other => other.to_string(),
        };

        let mut files_array = Vec::new();
        for df in dir_files {
            if let Some(p) = df.get("path").and_then(|path| path.as_str()) {
                let role = if p.contains("index") {
                    "entry point"
                } else if p.contains("config") || p.contains("settings") {
                    "configuration"
                } else if p.contains(".css") {
                    "stylesheet"
                } else {
                    "logic"
                };
                files_array.push(serde_json::json!({
                    "path": p,
                    "role": role
                }));
            }
        }

        features.push(serde_json::json!({
            "id": feature_name.to_lowercase().replace(' ', "-"),
            "name": feature_name,
            "description": format!("Files related to {}.", dir),
            "editableDescription": format!("Files related to {}.", dir),
            "files": files_array
        }));
    }

    let features_map = serde_json::json!({
        "version": "1.0",
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "projectName": project_name,
        "totalFiles": files_list.len(),
        "features": features,
        "_usage": "This file is used by the Axiom Forge AI editor. Edit 'editableDescription' to change what a feature does."
    });

    if let Ok(content) = serde_json::to_string_pretty(&features_map) {
        let _ = fs::write(proj_dir.join("axiom-features.json"), content);
    }
}

fn extract_code_content(raw: &str) -> String {
    let raw_trimmed = raw.trim();
    if let Some(start_idx) = raw_trimmed.find("```") {
        let after_start = &raw_trimmed[start_idx + 3..];
        if let Some(line_end_idx) = after_start.find('\n') {
            let after_line_end = &after_start[line_end_idx + 1..];
            if let Some(end_idx) = after_line_end.find("```") {
                return after_line_end[..end_idx].trim().to_string();
            } else {
                return after_line_end.trim().to_string();
            }
        }
    }
    raw_trimmed.to_string()
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
    
    // Load metadata.json to append related file context
    let proj_dir = get_projects_dir().join(&project_id);
    let metadata_path = proj_dir.join(".axiom").join("metadata.json");
    let rel_file_path = if let Ok(stripped) = std::path::Path::new(&file_path).strip_prefix(&proj_dir) {
        stripped.to_str().unwrap_or("").replace('\\', "/")
    } else {
        file_path.replace('\\', "/")
    };

    let mut related_files_context = String::new();
    if metadata_path.exists() {
        if let Ok(meta_content) = fs::read_to_string(&metadata_path) {
            if let Ok(meta_json) = serde_json::from_str::<Value>(&meta_content) {
                if let Some(deps) = meta_json.get("dependencies").and_then(|d| d.get(&rel_file_path)).and_then(|a| a.as_array()) {
                    for dep_val in deps {
                        if let Some(dep_path_str) = dep_val.as_str() {
                            let dep_full_path = proj_dir.join(dep_path_str);
                            if dep_full_path.exists() {
                                if let Ok(dep_content) = fs::read_to_string(&dep_full_path) {
                                    related_files_context.push_str(&format!(
                                        "\n--- Related File Context: {} ---\n{}\n-----------------------------------\n",
                                        dep_path_str, dep_content
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !related_files_context.is_empty() {
        user_prompt.push_str(&format!(
            "\nRelated Files Scope (for context/imports reference):\n{}",
            related_files_context
        ));
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
                        let final_content = extract_code_content(msg_content);
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

#[derive(Serialize, Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    #[serde(rename = "vramGB")]
    pub vram_gb: f64,
    pub brand: String, // "Nvidia", "AMD", "Intel", "Apple", "Unknown"
}

fn detect_gpu() -> Option<GpuInfo> {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg("Get-CimInstance Win32_VideoController | Select-Object Name, AdapterRAM | ConvertTo-Json -Compress")
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let trimmed = stdout.trim();
                
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    let parse_single_gpu = |obj: &serde_json::Value| -> Option<GpuInfo> {
                        let name = obj.get("Name")?.as_str()?.to_string();
                        let ram_bytes = if let Some(n) = obj.get("AdapterRAM").and_then(|v| v.as_u64()) {
                            n as f64
                        } else if let Some(s) = obj.get("AdapterRAM").and_then(|v| v.as_str()) {
                            s.parse::<f64>().unwrap_or(0.0)
                        } else if let Some(n) = obj.get("AdapterRAM").and_then(|v| v.as_i64()) {
                            n as f64
                        } else {
                            0.0
                        };

                        let vram_gb = ram_bytes / (1024.0 * 1024.0 * 1024.0);
                        
                        let name_lower = name.to_lowercase();
                        let brand = if name_lower.contains("nvidia") || name_lower.contains("geforce") || name_lower.contains("rtx") || name_lower.contains("gtx") {
                            "Nvidia".to_string()
                        } else if name_lower.contains("amd") || name_lower.contains("radeon") {
                            "AMD".to_string()
                        } else if name_lower.contains("intel") {
                            "Intel".to_string()
                        } else {
                            "Unknown".to_string()
                        };

                        Some(GpuInfo {
                            name,
                            vram_gb,
                            brand,
                        })
                    };

                    if val.is_array() {
                        if let Some(arr) = val.as_array() {
                            let mut best_gpu: Option<GpuInfo> = None;
                            for item in arr {
                                if let Some(gpu) = parse_single_gpu(item) {
                                    if gpu.brand == "Nvidia" || gpu.brand == "AMD" {
                                        return Some(gpu);
                                    }
                                    if best_gpu.is_none() {
                                        best_gpu = Some(gpu);
                                    }
                                }
                            }
                            return best_gpu;
                        }
                    } else {
                        return parse_single_gpu(&val);
                    }
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("system_profiler")
            .arg("SPDisplaysDataType")
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let mut chip_set = "Apple M-Series".to_string();
                let mut vram_gb = 8.0;

                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("Chipset Model:") {
                        chip_set = trimmed.replace("Chipset Model:", "").trim().to_string();
                    } else if trimmed.starts_with("VRAM (Total):") || trimmed.starts_with("VRAM (Dynamic, Max):") {
                        let vram_str = trimmed
                            .replace("VRAM (Total):", "")
                            .replace("VRAM (Dynamic, Max):", "")
                            .trim()
                            .to_string();
                        if vram_str.contains("GB") {
                            vram_gb = vram_str.replace("GB", "").trim().parse::<f64>().unwrap_or(8.0);
                        } else if vram_str.contains("MB") {
                            let mb = vram_str.replace("MB", "").trim().parse::<f64>().unwrap_or(8192.0);
                            vram_gb = mb / 1024.0;
                        }
                    }
                }

                return Some(GpuInfo {
                    name: chip_set,
                    vram_gb,
                    brand: "Apple".to_string(),
                });
            }
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("lspci | grep -i -E 'vga|3d'")
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let line = stdout.lines().next().unwrap_or("").trim().to_string();
                
                let brand = if line.to_lowercase().contains("nvidia") {
                    "Nvidia".to_string()
                } else if line.to_lowercase().contains("amd") || line.to_lowercase().contains("ati") {
                    "AMD".to_string()
                } else if line.to_lowercase().contains("intel") {
                    "Intel".to_string()
                } else {
                    "Unknown".to_string()
                };

                return Some(GpuInfo {
                    name: line,
                    vram_gb: 4.0, 
                    brand,
                });
            }
        }
        None
    }
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
    pub gpu: Option<GpuInfo>,
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
            id: "gemma4:e2b".to_string(),
            name: "Gemma 4 Edge 2B".to_string(),
            tier: "nano".to_string(),
            size_gb: 1.5,
            min_ram_gb: 3.0,
            tags: vec!["code".to_string(), "fast".to_string(), "multimodal".to_string()],
            strengths: "Very fast local generation, audio/image support".to_string(),
            weakness: "Basic code planning".to_string(),
            description: "Super lightweight, ideal for laptops and low-spec machines. Trimodal inputs (text, image, audio).".to_string(),
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
            id: "gemma4:e4b".to_string(),
            name: "Gemma 4 Edge 4B".to_string(),
            tier: "small".to_string(),
            size_gb: 2.8,
            min_ram_gb: 6.0,
            tags: vec!["code".to_string(), "fast".to_string(), "multimodal".to_string()],
            strengths: "Trimodal support, excellent speed-to-quality ratio".to_string(),
            weakness: "Limited context compared to 31B".to_string(),
            description: "Balanced edge model with native audio/image inputs. Great for consumer laptops with 8GB RAM.".to_string(),
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
            min_ram_gb: 12.0,
            tags: vec!["code".to_string(), "advanced".to_string()],
            strengths: "Complex multi-file projects, superior architecture understanding".to_string(),
            weakness: "Requires more VRAM/RAM for fast performance".to_string(),
            description: "The professional choice. Handles entire codebases, complex APIs, and database schemas with extreme accuracy. Perfect for 16GB+ RAM setups.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:12b".to_string(),
            name: "Gemma 4 12B".to_string(),
            tier: "mid".to_string(),
            size_gb: 7.2,
            min_ram_gb: 16.0,
            tags: vec!["code".to_string(), "advanced".to_string(), "multimodal".to_string()],
            strengths: "Strong reasoning, excellent structured JSON, native audio".to_string(),
            weakness: "Higher VRAM requirements".to_string(),
            description: "Highly capable model for intermediate configurations. Near-frontier reasoning on workstations with 16GB+ RAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:26b".to_string(),
            name: "Gemma 4 26B (MoE)".to_string(),
            tier: "power".to_string(),
            size_gb: 16.0,
            min_ram_gb: 24.0,
            tags: vec!["code".to_string(), "advanced".to_string(), "fast".to_string()],
            strengths: "Fast MoE inference, excellent programming logic".to_string(),
            weakness: "Large download size, high VRAM usage".to_string(),
            description: "Google's Mixture-of-Experts powerhouse. Extremely fast coding generation for systems with 24GB+ RAM / 16GB+ VRAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "gemma4:31b".to_string(),
            name: "Gemma 4 31B".to_string(),
            tier: "expert".to_string(),
            size_gb: 18.0,
            min_ram_gb: 32.0,
            tags: vec!["code".to_string(), "expert".to_string()],
            strengths: "Frontier reasoning, ranked #3 on Arena, complex database design".to_string(),
            weakness: "Requires substantial system memory, slow on entry-level GPUs".to_string(),
            description: "Google's flagship open-weight model. Outstanding logic, code layout, and architecture understanding. Requires 32GB+ RAM / 24GB+ VRAM.".to_string(),
            is_compatible: false, is_marginal: false, is_recommended: false, allow_override: true,
        },
        Model {
            id: "qwen2.5-coder:32b".to_string(),
            name: "Qwen 2.5 Coder 32B".to_string(),
            tier: "expert".to_string(),
            size_gb: 19.0,
            min_ram_gb: 32.0,
            tags: vec!["code".to_string(), "expert".to_string()],
            strengths: "Expert coder, near GPT-4 coding capabilities".to_string(),
            weakness: "Requires massive system resources".to_string(),
            description: "The absolute benchmark for local code generation. Outstanding repository understanding and bug fixing. Requires 32GB+ RAM.".to_string(),
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
    let gpu_info = detect_gpu();
    
    // Determine recommended model based on GPU first, fall back to RAM
    let mut best_model_id = "qwen2.5-coder:7b".to_string(); // default fallback
    let mut recommendation_set = false;

    if let Some(ref gpu) = gpu_info {
        if gpu.brand == "Nvidia" || gpu.brand == "Apple" || gpu.brand == "AMD" {
            if gpu.vram_gb >= 10.0 {
                best_model_id = "qwen2.5-coder:14b".to_string();
                recommendation_set = true;
            } else if gpu.vram_gb >= 3.0 {
                // Matches GTX 1060 (reported as 4GB) or other 4GB-8GB GPUs
                best_model_id = "qwen2.5-coder:7b".to_string();
                recommendation_set = true;
            }
        }
    }

    if !recommendation_set {
        let safe_ram = ram_gb * 0.80;
        let mut best_tier_idx = -1;
        let mut best_is_code = false;
        
        let tier_order = vec!["nano", "small", "mid", "power", "expert", "extreme"];
        
        for model in &raw_models {
            // Apply 0.5 GB RAM tolerance to safe RAM check
            if (safe_ram + 0.5) >= model.min_ram_gb {
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
    }
    
    let mut models = Vec::new();
    let mut by_tier: HashMap<String, Vec<Model>> = HashMap::new();
    let tolerance = 0.5;
    
    for mut model in raw_models {
        // Apply 0.5 GB RAM tolerance to compatibility checks
        model.is_compatible = (ram_gb + tolerance) >= model.min_ram_gb;
        model.is_marginal = !model.is_compatible && (ram_gb + tolerance) >= model.min_ram_gb * 0.75;
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
        gpu: gpu_info,
    }
}

#[derive(Serialize)]
pub struct LocalToolsProfile {
    pub node: Option<String>,
    pub npm: Option<String>,
    pub rust: Option<String>,
    pub cargo: Option<String>,
}

#[tauri::command]
async fn get_local_tools_profile() -> LocalToolsProfile {
    let node_ver = get_tool_version("node", &["-v"]).await;
    
    #[cfg(target_os = "windows")]
    let npm_ver = get_tool_version("cmd", &["/C", "npm", "-v"]).await;
    #[cfg(not(target_os = "windows"))]
    let npm_ver = get_tool_version("npm", &["-v"]).await;
    
    let rust_ver = get_tool_version("rustc", &["--version"]).await;
    let cargo_ver = get_tool_version("cargo", &["--version"]).await;
    
    LocalToolsProfile {
        node: node_ver,
        npm: npm_ver,
        rust: rust_ver,
        cargo: cargo_ver,
    }
}

async fn get_tool_version(cmd: &str, args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new(cmd);
    for arg in args {
        command.arg(arg);
    }
    
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    if let Ok(output) = command.output().await {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let cleaned = raw.trim()
                .replace("rustc ", "")
                .replace("cargo ", "");
            let first_word = cleaned.split_whitespace().next().unwrap_or("").to_string();
            if !first_word.is_empty() {
                return Some(first_word);
            }
            return Some(cleaned.to_string());
        }
    }
    None
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

fn kill_process_on_port(port: u16) {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg(format!("netstat -ano | findstr :{}", port))
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if line.contains("LISTENING") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(pid_str) = parts.last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            let mut kill_cmd = std::process::Command::new("taskkill");
                            kill_cmd.arg("/F").arg("/T").arg("/PID").arg(pid.to_string());
                            use std::os::windows::process::CommandExt;
                            kill_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                            let _ = kill_cmd.status();
                        }
                    }
                }
            }
        }
    }
}

fn strip_ansi_codes(input: &str) -> String {
    let mut output = String::new();
    let mut in_escape = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' || c == '\u{1b}' {
            in_escape = true;
            if let Some(&'[') = chars.peek() {
                chars.next();
            }
        } else if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            output.push(c);
        }
    }
    output
}

fn extract_file_path_from_line(line: &str) -> Option<String> {
    let cleaned = line.replace("⨯", " ");
    for word in cleaned.split_whitespace() {
        let word_clean = word.trim_matches(|c| c == '[' || c == ']' || c == ',' || c == '`' || c == '(' || c == ')');
        for ext in &[".tsx", ".ts", ".jsx", ".js", ".css"] {
            if let Some(idx) = word_clean.find(ext) {
                let end_pos = idx + ext.len();
                let mut path = word_clean[..end_pos].to_string();
                path = path.trim_start_matches(':').to_string();
                let path_clean = path.trim_start_matches(|c: char| {
                    !c.is_alphanumeric() && c != '/' && c != '\\' && c != '.'
                });
                let mut path_str = path_clean.to_string();
                if path_str.starts_with("./") {
                    path_str = path_str.split_off(2);
                }
                // Preskoči biblioteke treće strane, bild izlaz, keš i git foldere
                if path_str.contains("node_modules") 
                    || path_str.contains(".next") 
                    || path_str.contains("dist") 
                    || path_str.contains("build") 
                    || path_str.contains(".git") 
                {
                    continue;
                }
                return Some(path_str);
            }
        }
    }
    None
}

fn extract_package_name_from_error(line: &str) -> Option<String> {
    let pkg = if let Some(idx) = line.find("Cannot find module '") {
        let start = idx + "Cannot find module '".len();
        line[start..].find('\'').map(|end| line[start..start + end].to_string())
    } else if let Some(idx) = line.find("Can't resolve '") {
        let start = idx + "Can't resolve '".len();
        line[start..].find('\'').map(|end| line[start..start + end].to_string())
    } else {
        None
    };

    if let Some(p) = pkg {
        let trimmed = p.trim();
        // Filtriraj lokalne relativne i apsolutne putanje
        if trimmed.starts_with('.') || trimmed.starts_with('/') || trimmed.starts_with('\\') || trimmed.contains(":\\") || trimmed.contains(":/") {
            return None;
        }
        return Some(trimmed.to_string());
    }
    None
}

struct CompileErrorDetector {
    is_recording: bool,
    file_path: Option<String>,
    error_lines: Vec<String>,
    lines_since_start: usize,
}

impl CompileErrorDetector {
    fn new() -> Self {
        Self {
            is_recording: false,
            file_path: None,
            error_lines: Vec::new(),
            lines_since_start: 0,
        }
    }

    fn process_line(&mut self, line: &str, window: &tauri::Window, project_id: &str) {
        let clean_line = strip_ansi_codes(line);

        if clean_line.contains("Cannot find module") || clean_line.contains("Module not found") {
            if let Some(pkg_name) = extract_package_name_from_error(&clean_line) {
                let _ = window.emit("server:missing-package", serde_json::json!({
                    "projectId": project_id,
                    "packageName": pkg_name
                }));
                return;
            }
        }

        let has_error_marker = clean_line.contains('⨯') || clean_line.contains("Failed to compile") || clean_line.contains("Error:");

        if has_error_marker {
            if self.is_recording && self.file_path.is_some() {
                self.emit_current(window, project_id);
            }
            self.is_recording = true;
            self.file_path = extract_file_path_from_line(&clean_line);
            self.error_lines = vec![clean_line];
            self.lines_since_start = 0;
            
            if self.file_path.is_some() {
                self.emit_current(window, project_id);
            }
        } else if self.is_recording {
            self.error_lines.push(clean_line.clone());
            self.lines_since_start += 1;

            if self.file_path.is_none() {
                self.file_path = extract_file_path_from_line(&clean_line);
            }

            if self.file_path.is_some() {
                self.emit_current(window, project_id);
            } else if self.lines_since_start >= 10 {
                self.is_recording = false;
                self.error_lines.clear();
                self.lines_since_start = 0;
            }
        }
    }

    fn emit_current(&mut self, window: &tauri::Window, project_id: &str) {
        if let Some(ref path) = self.file_path {
            let proj_dir = get_projects_dir().join(project_id);
            let full_path = proj_dir.join(path);
            if full_path.exists() && full_path.is_file() {
                let error_message = self.error_lines.join("\n");
                let _ = window.emit("server:compile-error", serde_json::json!({
                    "projectId": project_id,
                    "filePath": path,
                    "errorMessage": error_message
                }));
            }
        }
        self.is_recording = false;
        self.file_path = None;
        self.error_lines.clear();
        self.lines_since_start = 0;
    }
}


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
    // Clean up any zombie processes on port 3000 and 3001
    kill_process_on_port(3000);
    kill_process_on_port(3001);

    let mut proj_dir = get_projects_dir().join(&options.project_id);
    
    // Fallback: If there is no package.json in the root but there is one in the src directory, use the src directory as the root.
    if !proj_dir.join("package.json").exists() && proj_dir.join("src").join("package.json").exists() {
        let _ = window.emit("server:log", serde_json::json!({
            "text": "> package.json found in src/ directory. Using src/ as project root.\n",
            "type": "info"
        }));
        proj_dir = proj_dir.join("src");
    }
    
    // Auto-install dependencies if node_modules is missing or incomplete
    let node_modules_dir = proj_dir.join("node_modules");
    let mut needs_install = !node_modules_dir.exists();
    
    if !needs_install {
        if let Ok(pkg_content) = std::fs::read_to_string(proj_dir.join("package.json")) {
            if let Ok(pkg_json) = serde_json::from_str::<Value>(&pkg_content) {
                if let Some(deps) = pkg_json.get("dependencies").and_then(|d| d.as_object()) {
                    for dep_name in deps.keys() {
                        if !node_modules_dir.join(dep_name).exists() {
                            needs_install = true;
                            let _ = window.emit("server:log", serde_json::json!({
                                "text": format!("> Detected missing package in node_modules: {}. Running npm install...\n", dep_name),
                                "type": "info"
                            }));
                            break;
                        }
                    }
                }
            }
        }
    }
    
    if needs_install {
        run_npm_install_with_auto_healing(&proj_dir, &window, "server:log", None).await;
    }
    
    // Auto-run prisma generate if Prisma schema exists in the project
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        let _ = window.emit("server:log", serde_json::json!({
            "text": "> Prisma schema detected. Running prisma generate...\n",
            "type": "info"
        }));
        
        #[cfg(target_os = "windows")]
        let mut prisma_cmd = Command::new("cmd");
        #[cfg(target_os = "windows")]
        prisma_cmd.arg("/C").arg("npx").arg("-y").arg("prisma").arg("generate").current_dir(&proj_dir);
        
        #[cfg(not(target_os = "windows"))]
        let mut prisma_cmd = Command::new("npx");
        #[cfg(not(target_os = "windows"))]
        prisma_cmd.arg("-y").arg("prisma").arg("generate").current_dir(&proj_dir);
        
        #[cfg(target_os = "windows")]
        prisma_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        
        match prisma_cmd.spawn() {
            Ok(mut child) => {
                let _ = child.wait().await;
                let _ = window.emit("server:log", serde_json::json!({
                    "text": "> Prisma client generation complete.\n",
                    "type": "info"
                }));
            }
            Err(e) => {
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> Failed to run prisma generate: {}\n", e),
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
            let p_id_stdout = options.project_id.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stdout).lines();
                let mut detector = CompileErrorDetector::new();
                while let Ok(Some(line)) = reader.next_line().await {
                    detector.process_line(&line, &window_stdout, &p_id_stdout);
                    let _ = window_stdout.emit("server:log", serde_json::json!({
                        "text": format!("{}\n", line),
                        "type": "info"
                    }));
                }
                if detector.is_recording {
                    detector.emit_current(&window_stdout, &p_id_stdout);
                }
            });

            // Spawn task to read stderr and emit logs
            let window_stderr = window.clone();
            let p_id_stderr = options.project_id.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stderr).lines();
                let mut detector = CompileErrorDetector::new();
                while let Ok(Some(line)) = reader.next_line().await {
                    detector.process_line(&line, &window_stderr, &p_id_stderr);
                    let _ = window_stderr.emit("server:log", serde_json::json!({
                        "text": format!("{}\n", line),
                        "type": "error"
                    }));
                }
                if detector.is_recording {
                    detector.emit_current(&window_stderr, &p_id_stderr);
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
        #[cfg(target_os = "windows")]
        {
            if let Some(pid) = child.id() {
                let mut kill_cmd = Command::new("taskkill");
                kill_cmd.arg("/F").arg("/T").arg("/PID").arg(pid.to_string());
                kill_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                let _ = kill_cmd.spawn();
            }
        }
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

#[tauri::command]
async fn project_heal_compile_error(
    project_id: String,
    file_path: String,
    error_msg: String,
    window: tauri::Window,
) -> Result<bool, String> {
    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": "Inicijalizacija popravke..."
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing] Započinjem popravku za fajl: {}\n", file_path),
        "type": "info"
    }));

    let proj_dir = get_projects_dir().join(&project_id);
    let full_path = proj_dir.join(&file_path);
    if !full_path.exists() {
        let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška: Fajl ne postoji" }));
        return Err(format!("Fajl {} ne postoji.", file_path));
    }

    let file_content = std::fs::read_to_string(&full_path)
        .map_err(|e| {
            let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška pri čitanju" }));
            format!("Greška pri čitanju fajla: {}", e)
        })?;

    let settings = get_axiom_settings();
    let model = settings.get("modelId")
        .and_then(|m| m.as_str())
        .unwrap_or("qwen2.5-coder:7b")
        .to_string();

    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": format!("Pozivanje AI modela ({}) u toku...", model)
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing] Pozivam model '{}' sa opisom greške...\n", model),
        "type": "info"
    }));

    let client = reqwest::Client::new();
    let mut system_prompt = "You are a Senior Next.js App Router Developer. A file in the project failed with an error. \
        Instead of rewriting the entire file, you must identify the exact lines causing the error and provide a list of target replacements in JSON format.\
        \n\n\
        CRITICAL RULES:\n\
        1. Respond ONLY with a valid JSON object matching the format described below. Do not write any explanations, introductory text, or markdown code blocks (do not wrap in ```json). Just the raw JSON.\n\
        2. In the JSON structure, under 'edits', each 'target' block must exactly match (character-for-character, including leading whitespace and indentation) a unique substring in the original file.\n\
        3. Do NOT include placeholders in 'replacement'. The code must be complete and ready to run.\n\
        4. Focus only on resolving the reported error. Do not make unrelated changes.\n\
        5. DATABASE RESILIENCY: If a database query (like a Prisma call `findMany`) throws a database connection error, wrap the query in try-catch and fall back to returning a mock array containing 10 realistic mockup items so the application remains previewable even if the database server is offline.\n\
        \n\n\
        JSON FORMAT:\n\
        {\n\
          \"explanation\": \"Brief explanation of the compile error and how to fix it\",\n\
          \"edits\": [\n\
            {\n\
              \"target\": \"exact string of lines in original file to be replaced\",\n\
              \"replacement\": \"the new code to put in its place\"\n\
            }\n\
          ]\n\
        }".to_string();
    system_prompt.push_str(&get_packages_db_summary());

    let user_prompt = format!(
        "File: {}\n\nCompilation Error Details:\n{}\n\nFile Current Content:\n{}\n\nPlease analyze the error and output the required JSON edits to fix it.",
        file_path,
        error_msg,
        file_content
    );

    let ollama_req = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "stream": false,
        "options": {
            "temperature": 0.1
        }
    });

    match client.post("http://127.0.0.1:11434/api/chat")
        .json(&ollama_req)
        .send()
        .await 
    {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(resp_json) = resp.json::<serde_json::Value>().await {
                    if let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                        let _ = window.emit("server:heal-status", serde_json::json!({
                            "status": "Ispravljanje koda i post-processing..."
                        }));
                        let _ = window.emit("server:log", serde_json::json!({
                            "text": "> [AI Auto-Healing] AI je vratio ispravku. Primjenjujem post-processing...\n".to_string(),
                            "type": "info"
                        }));

                        let clean_content = extract_code_content(ai_msg);
                        
                        // Parse as JSON
                        let parsed_json: serde_json::Value = match serde_json::from_str(&clean_content) {
                            Ok(val) => val,
                            Err(e) => {
                                // Try to extract JSON if it was wrapped in some other text
                                if let Some(start_json) = clean_content.find('{') {
                                    if let Some(end_json) = clean_content.rfind('}') {
                                        let json_str = &clean_content[start_json..=end_json];
                                        serde_json::from_str(json_str).map_err(|_| {
                                            format!("Model did not return valid JSON. Error: {}. Response: {}", e, clean_content)
                                        })?
                                    } else {
                                        return Err(format!("Model did not return valid JSON. Error: {}", e));
                                    }
                                } else {
                                    return Err(format!("Model did not return valid JSON. Error: {}", e));
                                }
                            }
                        };

                        let explanation = parsed_json.get("explanation")
                            .and_then(|e| e.as_str())
                            .unwrap_or("No explanation provided");
                        
                        let _ = window.emit("server:log", serde_json::json!({
                            "text": format!("> [AI Auto-Healing] AI analiza: {}\n", explanation),
                            "type": "info"
                        }));

                        let edits = match parsed_json.get("edits").and_then(|e| e.as_array()) {
                            Some(arr) => arr,
                            None => return Err("JSON does not contain an 'edits' array.".to_string()),
                        };

                        if edits.is_empty() {
                            return Err("AI did not propose any edits to apply.".to_string());
                        }

                        let mut new_file_content = file_content.clone();
                        for edit in edits {
                            let target = match edit.get("target").and_then(|t| t.as_str()) {
                                Some(t) => t,
                                None => return Err("An edit is missing its 'target' field.".to_string()),
                            };
                            let replacement = match edit.get("replacement").and_then(|r| r.as_str()) {
                                Some(r) => r,
                                None => return Err("An edit is missing its 'replacement' field.".to_string()),
                            };

                            if target.is_empty() {
                                return Err("Edit 'target' cannot be empty.".to_string());
                            }

                            // Check that the target substring exists exactly once in the file content
                            let occurrences: Vec<_> = new_file_content.match_indices(target).collect();
                            if occurrences.is_empty() {
                                return Err(format!(
                                    "Greška pri primeni ispravke: Target kod nije pronađen u fajlu. Target: {:?}",
                                    target
                                ));
                            } else if occurrences.len() > 1 {
                                return Err(format!(
                                    "Greška pri primeni ispravke: Target kod je dupliran ({}) u fajlu. Target mora biti jedinstven. Target: {:?}",
                                    occurrences.len(), target
                                ));
                            }

                            new_file_content = new_file_content.replace(target, replacement);
                        }

                        let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();
                        let processed = post_process_generated_file(&file_path, &new_file_content, &serde_json::json!({}), has_tailwind);
                        
                        let _ = window.emit("server:heal-status", serde_json::json!({
                            "status": "Upisivanje ispravki na disk..."
                        }));
                        
                        std::fs::write(&full_path, &processed)
                            .map_err(|e| {
                                let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška pri upisu" }));
                                format!("Greška pri upisu na disk: {}", e)
                            })?;

                        let _ = window.emit("server:heal-status", serde_json::json!({
                            "status": "Završeno!"
                        }));
                        let _ = window.emit("server:log", serde_json::json!({
                            "text": format!("> [AI Auto-Healing] Uspešno popravljen fajl: {}. Server bi trebalo da se re-kompajlira.\n", file_path),
                            "type": "info"
                        }));

                        return Ok(true);
                    }
                }
            }
            let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška: Prazan odgovor modela" }));
            Err("Model nije vratio ispravan odgovor.".to_string())
        }
        Err(e) => {
            let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška u komunikaciji sa AI" }));
            Err(format!("Greška u komunikaciji sa Ollama serverom: {}", e))
        }
    }
}

#[tauri::command]
async fn project_install_package(
    project_id: String,
    package_name: String,
    window: tauri::Window,
) -> Result<bool, String> {
    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": format!("Instalacija paketa {}...", package_name)
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing] Detektovan nedostajući paket. Pokrećem instalaciju: {}\n", package_name),
        "type": "info"
    }));

    let proj_dir = get_projects_dir().join(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }

    let _ = window.emit("server:log", serde_json::json!({
        "text": "> [AI Auto-Healing] Pokrećem npm install za paket...\n".to_string(),
        "type": "info"
    }));

    #[cfg(target_os = "windows")]
    let mut cmd = tokio::process::Command::new("cmd");
    #[cfg(target_os = "windows")]
    cmd.arg("/C").arg("npm").arg("install").arg(&package_name).current_dir(&proj_dir);

    #[cfg(not(target_os = "windows"))]
    let mut cmd = tokio::process::Command::new("npm");
    #[cfg(not(target_os = "windows"))]
    cmd.arg("install").arg(&package_name).current_dir(&proj_dir);

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    match cmd.output().await {
        Ok(output) => {
            if output.status.success() {
                let _ = window.emit("server:heal-status", serde_json::json!({
                    "status": "Instalirano!"
                }));
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [AI Auto-Healing] Uspešno instaliran paket {}.\n", package_name),
                    "type": "info"
                }));
                Ok(true)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let _ = window.emit("server:heal-status", serde_json::json!({
                    "status": "Greška pri instalaciji"
                }));
                Err(format!("npm install failed: {}", stderr))
            }
        }
        Err(e) => Err(format!("Failed to run npm install command: {}", e))
    }
}

#[tauri::command]
async fn project_run_prisma_generate(project_id: String) -> Result<String, String> {
    let proj_dir = get_projects_dir().join(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory does not exist".to_string());
    }
    
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if !has_prisma {
        return Err("No Prisma schema found in this project".to_string());
    }
    
    #[cfg(target_os = "windows")]
    let mut cmd = Command::new("cmd");
    #[cfg(target_os = "windows")]
    cmd.arg("/C").arg("npx").arg("-y").arg("prisma").arg("generate").current_dir(&proj_dir);
    
    #[cfg(not(target_os = "windows"))]
    let mut cmd = Command::new("npx");
    #[cfg(not(target_os = "windows"))]
    cmd.arg("-y").arg("prisma").arg("generate").current_dir(&proj_dir);
    
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    let output = cmd.output().await
        .map_err(|e| format!("Failed to run prisma generate command: {}", e))?;
        
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if output.status.success() {
        Ok(stdout.into_owned())
    } else {
        Err(format!("Prisma generate failed:\n{}\n{}", stdout, stderr))
    }
}

fn generate_project_metadata(proj_dir: &std::path::Path, project_id: &str, files_list: &[Value]) {
    let mut dependencies = HashMap::new();

    let mut project_files = Vec::new();
    for file_spec in files_list {
        if let Some(path_str) = file_spec.get("path").and_then(|p| p.as_str()) {
            project_files.push(path_str.replace('\\', "/"));
        }
    }

    for file_path in &project_files {
        let full_path = proj_dir.join(file_path);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let mut file_deps = Vec::new();
            
            for line in content.lines() {
                let line_trimmed = line.trim();
                if line_trimmed.starts_with("import ") || line_trimmed.contains("require(") {
                    let quotes = ['\'', '"', '`'];
                    let mut found_path = None;
                    
                    for q in &quotes {
                        let parts: Vec<&str> = line_trimmed.split(*q).collect();
                        if parts.len() >= 3 {
                            for part in parts.iter().skip(1).step_by(2) {
                                if part.starts_with('.') || part.contains('/') {
                                    found_path = Some(part.to_string());
                                    break;
                                }
                            }
                        }
                    }
                    
                    if let Some(import_path) = found_path {
                        if let Some(parent_dir) = std::path::Path::new(file_path).parent() {
                            let resolved = parent_dir.join(&import_path);
                            let mut normalized_parts = Vec::new();
                            for component in resolved.components() {
                                match component {
                                    std::path::Component::Normal(c) => {
                                        if let Some(c_str) = c.to_str() {
                                            normalized_parts.push(c_str);
                                        }
                                    }
                                    std::path::Component::ParentDir => {
                                        normalized_parts.pop();
                                    }
                                    _ => {}
                                }
                            }
                            let normalized_path_str = normalized_parts.join("/");
                            
                            for pf in &project_files {
                                let pf_no_ext = std::path::Path::new(pf).with_extension("");
                                let pf_no_ext_str = pf_no_ext.to_str().unwrap_or("").replace('\\', "/");
                                if pf_no_ext_str == normalized_path_str {
                                    file_deps.push(pf.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            dependencies.insert(file_path.clone(), file_deps);
        }
    }

    let metadata = serde_json::json!({
        "projectId": project_id,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "dependencies": dependencies
    });

    let axiom_dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&axiom_dir);
    if let Ok(meta_content) = serde_json::to_string_pretty(&metadata) {
        let _ = fs::write(axiom_dir.join("metadata.json"), meta_content);
    }
}

// ==================== ORCHESTRATOR API ====================
use tauri::Emitter;

fn detect_infinite_loop(content: &str) -> bool {
    // 1. Check character repetition (e.g. } } } } } } } } } } } } } } })
    let trimmed_chars: Vec<char> = content.trim_end().chars().rev().take(20).collect();
    if trimmed_chars.len() >= 15 {
        let first = trimmed_chars[0];
        if !first.is_alphanumeric() && trimmed_chars.iter().all(|&c| c == first) {
            return true;
        }
    }

    // 2. Check line block repetition (e.g. repeating a block of 1-5 lines 3 times)
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let n = lines.len();
    for block_size in 1..=5 {
        if n >= block_size * 3 {
            let mut is_loop = true;
            for i in 0..block_size {
                let idx_curr = n - block_size + i;
                let idx_prev1 = n - 2 * block_size + i;
                let idx_prev2 = n - 3 * block_size + i;
                if lines[idx_curr] != lines[idx_prev1] || lines[idx_curr] != lines[idx_prev2] {
                    is_loop = false;
                    break;
                }
            }
            if is_loop {
                let block_content: String = lines[n-block_size..n].join(" ");
                let trimmed = block_content.trim();
                if trimmed.len() > 8 && trimmed.chars().any(|c| c.is_alphanumeric()) {
                    return true;
                }
            }
        }
    }

    // 3. Check word repetition on the last line (e.g. repeating a word 4 times)
    if let Some(last_line) = content.lines().last() {
        let words: Vec<&str> = last_line.split_whitespace().collect();
        let nw = words.len();
        for w_size in 1..=4 {
            if nw >= w_size * 4 {
                let mut w_loop = true;
                for i in 0..w_size {
                    let w_curr = words[nw - w_size + i];
                    let w_prev1 = words[nw - 2 * w_size + i];
                    let w_prev2 = words[nw - 3 * w_size + i];
                    let w_prev3 = words[nw - 4 * w_size + i];
                    if w_curr != w_prev1 || w_curr != w_prev2 || w_curr != w_prev3 {
                        w_loop = false;
                        break;
                    }
                }
                if w_loop {
                    let word_block = words[nw - w_size..nw].join(" ");
                    if word_block.len() > 3 {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn index_modules(dir: &std::path::Path, src_root: &std::path::Path, index: &mut std::collections::HashMap<String, Vec<String>>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "node_modules" && name != ".next" && name != ".git" {
                    index_modules(&path, src_root, index);
                }
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(rel_path) = path.strip_prefix(src_root) {
                            let mut rel_str = rel_path.to_string_lossy().replace('\\', "/");
                            if let Some(dot_idx) = rel_str.rfind('.') {
                                rel_str.truncate(dot_idx);
                            }
                            index.entry(stem.to_string()).or_insert_with(Vec::new).push(rel_str);
                        }
                    }
                }
            }
        }
    }
}

fn scan_and_fix_imports(dir: &std::path::Path, src_root: &std::path::Path, index: &std::collections::HashMap<String, Vec<String>>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "node_modules" && name != ".next" && name != ".git" {
                    scan_and_fix_imports(&path, src_root, index);
                }
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let mut changed = false;
                        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                        
                        for line in lines.iter_mut() {
                            if (line.contains("import ") || line.contains("import{") || line.contains("require(")) &&
                               (line.contains('\'') || line.contains('"') || line.contains('`')) {
                                
                                let quotes = ['\'', '"', '`'];
                                let mut literal = String::new();
                                for quote in quotes {
                                    if let Some(start) = line.find(quote) {
                                        if let Some(end) = line[start+1..].find(quote) {
                                            literal = line[start+1..start+1+end].to_string();
                                            break;
                                        }
                                    }
                                }
                                
                                let literal = literal.trim();
                                if !literal.is_empty() && (literal.starts_with('.') || literal.starts_with("@/")) {
                                    let mut exists = false;
                                    
                                    let target_abs = if literal.starts_with("@/") {
                                        let sub = &literal[2..];
                                        src_root.join(sub)
                                    } else {
                                        if let Some(parent) = path.parent() {
                                            parent.join(literal)
                                        } else {
                                            src_root.join(literal)
                                        }
                                    };
                                    
                                    let variations = [
                                        target_abs.with_extension("ts"),
                                        target_abs.with_extension("tsx"),
                                        target_abs.with_extension("js"),
                                        target_abs.with_extension("jsx"),
                                        target_abs.join("index.ts"),
                                        target_abs.join("index.tsx"),
                                        target_abs.join("index.js"),
                                        target_abs.join("index.jsx"),
                                    ];
                                    
                                    for var_path in variations {
                                        if var_path.exists() {
                                            exists = true;
                                            break;
                                        }
                                    }
                                    
                                    if !exists {
                                        let parts: Vec<&str> = literal.split('/').collect();
                                        if let Some(&last_part) = parts.last() {
                                            if !last_part.is_empty() {
                                                if let Some(candidates) = index.get(last_part) {
                                                    if candidates.len() == 1 {
                                                        let corrected_import = format!("@{}/{}", "", candidates[0]);
                                                        let old_import_quote = format!("'{}'", literal);
                                                        let old_import_dquote = format!("\"{}\"", literal);
                                                        let old_import_backtick = format!("`{}`", literal);
                                                        
                                                        let new_import_quote = format!("'{}'", corrected_import);
                                                        let new_import_dquote = format!("\"{}\"", corrected_import);
                                                        let new_import_backtick = format!("`{}`", corrected_import);
                                                        
                                                        if line.contains(&old_import_quote) {
                                                            *line = line.replace(&old_import_quote, &new_import_quote);
                                                            changed = true;
                                                        } else if line.contains(&old_import_dquote) {
                                                            *line = line.replace(&old_import_dquote, &new_import_dquote);
                                                            changed = true;
                                                        } else if line.contains(&old_import_backtick) {
                                                            *line = line.replace(&old_import_backtick, &new_import_backtick);
                                                            changed = true;
                                                        }
                                                        
                                                        debug_log_to_file(format!("[Import Fixer] Fixed broken import in {:?}: {} -> {}", path, literal, corrected_import));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        if changed {
                            let _ = std::fs::write(&path, lines.join("\n"));
                        }
                    }
                }
            }
        }
    }
}

struct MissingImport {
    target_path: std::path::PathBuf,
    importing_file_path: std::path::PathBuf,
    import_line: String,
}

fn collect_missing_imports(
    dir: &std::path::Path,
    src_root: &std::path::Path,
    missing: &mut Vec<MissingImport>,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "node_modules" && name != ".next" && name != ".git" {
                    collect_missing_imports(&path, src_root, missing);
                }
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for line in content.lines() {
                            if (line.contains("import ") || line.contains("import{") || line.contains("require(")) &&
                               (line.contains('\'') || line.contains('"') || line.contains('`')) {
                                
                                let quotes = ['\'', '"', '`'];
                                let mut literal = String::new();
                                for quote in quotes {
                                    if let Some(start) = line.find(quote) {
                                        if let Some(end) = line[start+1..].find(quote) {
                                            literal = line[start+1..start+1+end].to_string();
                                            break;
                                        }
                                    }
                                }
                                
                                let literal = literal.trim();
                                if !literal.is_empty() && (literal.starts_with('.') || literal.starts_with("@/")) {
                                    let mut exists = false;
                                    
                                    let target_abs = if literal.starts_with("@/") {
                                        let sub = &literal[2..];
                                        src_root.join(sub)
                                    } else {
                                        if let Some(parent) = path.parent() {
                                            parent.join(literal)
                                        } else {
                                            src_root.join(literal)
                                        }
                                    };
                                    
                                    let variations = [
                                        target_abs.with_extension("ts"),
                                        target_abs.with_extension("tsx"),
                                        target_abs.with_extension("js"),
                                        target_abs.with_extension("jsx"),
                                        target_abs.join("index.ts"),
                                        target_abs.join("index.tsx"),
                                        target_abs.join("index.js"),
                                        target_abs.join("index.jsx"),
                                    ];
                                    
                                    for var_path in variations {
                                        if var_path.exists() {
                                            exists = true;
                                            break;
                                        }
                                    }
                                    
                                    if !exists {
                                        let target_file_path = if target_abs.extension().is_some() {
                                            target_abs.clone()
                                        } else {
                                            let ext_str = path.extension().and_then(|e| e.to_str()).unwrap_or("tsx");
                                            target_abs.with_extension(ext_str)
                                        };
                                        
                                        if !missing.iter().any(|m| m.target_path == target_file_path) {
                                            missing.push(MissingImport {
                                                target_path: target_file_path,
                                                importing_file_path: path.clone(),
                                                import_line: line.to_string(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn repair_broken_imports(proj_dir: &std::path::Path, model: &str) {
    let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();
    let src_dir = proj_dir.join("src");
    if !src_dir.exists() {
        return;
    }
    
    let mut module_index = std::collections::HashMap::new();
    index_modules(&src_dir, &src_dir, &mut module_index);
    scan_and_fix_imports(&src_dir, &src_dir, &module_index);
    
    let mut missing_imports = Vec::new();
    collect_missing_imports(&src_dir, &src_dir, &mut missing_imports);
    
    if !missing_imports.is_empty() {
        debug_log_to_file(format!("[Import Fixer] Found {} missing modules that need generation.", missing_imports.len()));
        
        let client = reqwest::Client::new();
        
        for item in missing_imports {
            let importing_content = std::fs::read_to_string(&item.importing_file_path).unwrap_or_default();
            let relative_importing = item.importing_file_path.strip_prefix(proj_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| item.importing_file_path.to_string_lossy().to_string());
                
            let relative_target = item.target_path.strip_prefix(proj_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| item.target_path.to_string_lossy().to_string());
                
            debug_log_to_file(format!(
                "[Import Fixer] Generating missing file: {} (imported in {} via: {})",
                relative_target, relative_importing, item.import_line.trim()
            ));
            
            let system_prompt = "You are a Senior Next.js/React Developer. Respond with ONLY the raw, complete file content. Do not include markdown code block formatting (like ```tsx), explanations, introduction, or outro. Respond with clean source code only.";
            let user_prompt = format!(
                "The project is missing an imported module/file.\n\
                 Missing File path to generate: {}\n\
                 Imported in file: {}\n\
                 Import line: {}\n\n\
                 Here is the contents of the importing file ({}):\n\
                 ```\n\
                 {}\n\
                 ```\n\n\
                 Generate a suitable mock/boilerplate content for the missing file ({}) that defines and exports everything expected by the importing file so that the project compiles without errors.",
                relative_target, relative_importing, item.import_line.trim(), relative_importing, importing_content, relative_target
            );
            
            let ollama_req = serde_json::json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ],
                "stream": false,
                "options": {
                    "temperature": 0.1
                }
            });
            
            if let Ok(resp) = client.post("http://127.0.0.1:11434/api/chat")
                .json(&ollama_req)
                .send()
                .await 
            {
                if resp.status().is_success() {
                    if let Ok(resp_json) = resp.json::<serde_json::Value>().await {
                        if let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                            let clean_content = extract_code_content(ai_msg);
                            let processed = post_process_generated_file(&relative_target, &clean_content, &serde_json::json!({}), has_tailwind);
                            
                            if let Some(parent) = item.target_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            
                            if let Ok(_) = std::fs::write(&item.target_path, &processed) {
                                debug_log_to_file(format!(
                                    "[Import Fixer] Successfully created missing module: {}",
                                    relative_target
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn ensure_theme_file_exists(proj_dir: &std::path::Path) {
    let src_dir = proj_dir.join("src");
    if !src_dir.exists() {
        return;
    }
    
    fn scan_and_create(dir: &std::path::Path, src_root: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_and_create(&path, src_root);
                } else if path.is_file() {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let mut needs_theme = false;
                            let mut theme_target_path = None;
                            
                            if content.contains("'./theme'") || content.contains("\"./theme\"") {
                                needs_theme = true;
                                if let Some(parent) = path.parent() {
                                    theme_target_path = Some(parent.join("theme.ts"));
                                }
                            } else if content.contains("'@/theme'") || content.contains("\"@/theme\"") {
                                needs_theme = true;
                                theme_target_path = Some(src_root.join("theme.ts"));
                            } else if content.contains("'@/styles/theme'") || content.contains("\"@/styles/theme\"") {
                                needs_theme = true;
                                theme_target_path = Some(src_root.join("styles").join("theme.ts"));
                            }
                            
                            if needs_theme {
                                if let Some(target) = theme_target_path {
                                    let base = target.with_extension("");
                                    let exists = base.with_extension("ts").exists() ||
                                                 base.with_extension("tsx").exists() ||
                                                 base.with_extension("js").exists() ||
                                                 base.with_extension("jsx").exists();
                                                 
                                    if !exists {
                                        if let Some(parent) = target.parent() {
                                            let _ = std::fs::create_dir_all(parent);
                                        }
                                        let default_theme = r#"import { createTheme } from '@mui/material/styles';

const theme = createTheme({
  palette: {
    mode: 'dark',
    primary: {
      main: '#6366f1', // indigo-500
    },
    secondary: {
      main: '#ec4899', // pink-500
    },
    background: {
      default: '#0f172a', // slate-900
      paper: '#1e293b',   // slate-800
    },
  },
});

export default theme;
"#;
                                        let _ = std::fs::write(&target, default_theme);
                                        debug_log_to_file(format!("[Theme Fixer] Created default theme file at {:?}", target));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    scan_and_create(&src_dir, &src_dir);
}

fn repair_cargo_toml_dependencies(proj_dir: &std::path::Path) {
    let cargo_paths = [
        proj_dir.join("Cargo.toml"),
        proj_dir.join("src-tauri/Cargo.toml"),
    ];

    for cargo_path in &cargo_paths {
        if cargo_path.exists() {
            if let Ok(content) = std::fs::read_to_string(cargo_path) {
                let mut changed = false;
                let mut new_lines = Vec::new();
                for line in content.lines() {
                    let mut new_line = line.to_string();
                    let trimmed = line.trim();
                    if trimmed.starts_with("tauri ") || trimmed.starts_with("tauri=") {
                        if !trimmed.contains("version = \"1.6\"") && !trimmed.contains("\"1.6\"") {
                            new_line = "tauri = { version = \"1.6\", features = [\"api-all\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("tauri-build ") || trimmed.starts_with("tauri-build=") {
                        if !trimmed.contains("\"1.6\"") {
                            new_line = "tauri-build = \"1.6\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("serde ") || trimmed.starts_with("serde=") {
                        if !trimmed.contains("version = \"1.0\"") && !trimmed.contains("\"1.0\"") {
                            new_line = "serde = { version = \"1.0\", features = [\"derive\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("serde_json ") || trimmed.starts_with("serde_json=") {
                        if !trimmed.contains("\"1.0\"") {
                            new_line = "serde_json = \"1.0\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("tokio ") || trimmed.starts_with("tokio=") {
                        if !trimmed.contains("version = \"1\"") && !trimmed.contains("\"1\"") {
                            new_line = "tokio = { version = \"1\", features = [\"full\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("reqwest ") || trimmed.starts_with("reqwest=") {
                        if !trimmed.contains("version = \"0.12\"") && !trimmed.contains("\"0.12\"") {
                            new_line = "reqwest = { version = \"0.12\", features = [\"json\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("uuid ") || trimmed.starts_with("uuid=") {
                        if !trimmed.contains("version = \"1.8\"") && !trimmed.contains("\"1.8\"") {
                            new_line = "uuid = { version = \"1.8\", features = [\"v4\", \"serde\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("chrono ") || trimmed.starts_with("chrono=") {
                        if !trimmed.contains("version = \"0.4\"") && !trimmed.contains("\"0.4\"") {
                            new_line = "chrono = { version = \"0.4\", features = [\"serde\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("bcrypt ") || trimmed.starts_with("bcrypt=") {
                        if !trimmed.contains("\"0.15\"") {
                            new_line = "bcrypt = \"0.15\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("jsonwebtoken ") || trimmed.starts_with("jsonwebtoken=") {
                        if !trimmed.contains("\"9\"") {
                            new_line = "jsonwebtoken = \"9\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("once_cell ") || trimmed.starts_with("once_cell=") {
                        if !trimmed.contains("\"1.19\"") {
                            new_line = "once_cell = \"1.19\"".to_string();
                            changed = true;
                        }
                    }
                    new_lines.push(new_line);
                }
                if changed {
                    let _ = std::fs::write(cargo_path, new_lines.join("\n"));
                }
            }
        }
    }
}

fn repair_pubspec_yaml_dependencies(proj_dir: &std::path::Path) {
    let pubspec_path = proj_dir.join("pubspec.yaml");
    if pubspec_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pubspec_path) {
            let mut changed = false;
            let mut new_lines = Vec::new();
            for line in content.lines() {
                let mut new_line = line.to_string();
                let trimmed = line.trim();
                if trimmed.starts_with("cupertino_icons:") {
                    if !trimmed.contains("1.0.6") {
                        new_line = "  cupertino_icons: ^1.0.6".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("http:") {
                    if !trimmed.contains("1.2.1") {
                        new_line = "  http: ^1.2.1".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("provider:") {
                    if !trimmed.contains("6.1.2") {
                        new_line = "  provider: ^6.1.2".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("shared_preferences:") {
                    if !trimmed.contains("2.2.3") {
                        new_line = "  shared_preferences: ^2.2.3".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("sqflite:") {
                    if !trimmed.contains("2.3.2") {
                        new_line = "  sqflite: ^2.3.2".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("path_provider:") {
                    if !trimmed.contains("2.1.2") {
                        new_line = "  path_provider: ^2.1.2".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("uuid:") {
                    if !trimmed.contains("4.3.3") {
                        new_line = "  uuid: ^4.3.3".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("intl:") {
                    if !trimmed.contains("0.19.0") {
                        new_line = "  intl: ^0.19.0".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("bloc:") {
                    if !trimmed.contains("8.1.3") {
                        new_line = "  bloc: ^8.1.3".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("flutter_bloc:") {
                    if !trimmed.contains("8.1.5") {
                        new_line = "  flutter_bloc: ^8.1.5".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("go_router:") {
                    if !trimmed.contains("13.2.0") {
                        new_line = "  go_router: ^13.2.0".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("cached_network_image:") {
                    if !trimmed.contains("3.3.1") {
                        new_line = "  cached_network_image: ^3.3.1".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("google_fonts:") {
                    if !trimmed.contains("6.1.0") {
                        new_line = "  google_fonts: ^6.1.0".to_string();
                        changed = true;
                    }
                }
                new_lines.push(new_line);
            }
            if changed {
                let _ = std::fs::write(&pubspec_path, new_lines.join("\n"));
            }
        }
    }
}

async fn generate_and_save_healing_rule(
    file_path: &str,
    original_content: &str,
    repaired_content: &str,
    model: &str,
) {
    debug_log_to_file(format!("[Self-Healing] Analyzing changes in {} to extract global healing rules...", file_path));

    let client = reqwest::Client::new();
    let system_prompt = "You are a Senior Project Architect. Analyze the original file content (with compilation errors) and the corrected content. Identify the specific lines that were changed to fix the compilation error. Respond ONLY with a valid JSON array of objects representing search-and-replace rules that can be applied globally to other files to prevent this error. Each object must have 'search' (the exact erroneous string, including leading spaces/tabs) and 'replace' (the correct replacement string). Do not include any explanations, introduction, or markdown block formatting. Respond only with clean JSON.";
    
    let user_prompt = format!(
        "Original Content (with errors):\n{}\n\nRepaired Content (fixed):\n{}",
        original_content,
        repaired_content
    );

    let ollama_req = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "stream": false,
        "options": {
            "temperature": 0.1
        }
    });

    if let Ok(resp) = client.post("http://127.0.0.1:11434/api/chat")
        .json(&ollama_req)
        .send()
        .await 
    {
        if resp.status().is_success() {
            if let Ok(resp_json) = resp.json::<serde_json::Value>().await {
                if let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                    let clean_content = extract_code_content(ai_msg);
                    if let Ok(new_rules_val) = serde_json::from_str::<serde_json::Value>(&clean_content) {
                        if let Some(new_rules_arr) = new_rules_val.as_array() {
                            let rules_path = get_base_dir().join("axiom-healing-rules.json");
                            let mut existing_rules = if rules_path.exists() {
                                let content = std::fs::read_to_string(&rules_path).unwrap_or_else(|_| "[]".to_string());
                                serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|_| serde_json::json!([]))
                            } else {
                                serde_json::json!([])
                            };

                            let mut changed = false;
                            if let Some(existing_arr) = existing_rules.as_array_mut() {
                                for new_rule in new_rules_arr {
                                    if let (Some(search_str), Some(replace_str)) = (new_rule.get("search").and_then(|s| s.as_str()), new_rule.get("replace").and_then(|r| r.as_str())) {
                                        // Ignore rules that are too short/generic to avoid accidental corruption of valid code
                                        if search_str.trim().len() >= 6 && search_str != replace_str {
                                            // Check if rule already exists
                                            let exists = existing_arr.iter().any(|r| r.get("search").and_then(|s| s.as_str()) == Some(search_str));
                                            if !exists {
                                                existing_arr.push(serde_json::json!({
                                                    "search": search_str,
                                                    "replace": replace_str,
                                                    "createdAt": chrono::Utc::now().to_rfc3339()
                                                }));
                                                changed = true;
                                                debug_log_to_file(format!("[Self-Healing] Learned new global rule: Replace \"{}\" with \"{}\"", search_str.trim(), replace_str.trim()));
                                            }
                                        }
                                    }
                                }
                            }

                            if changed {
                                if let Ok(pretty) = serde_json::to_string_pretty(&existing_rules) {
                                    let _ = std::fs::write(&rules_path, pretty);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn perform_self_healing_loop(proj_dir: &std::path::Path, model: &str, files_list: &[Value]) {
    let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();
    // Only perform self-healing if it's a Next.js project
    let is_nextjs = proj_dir.join("next.config.js").exists() || proj_dir.join("package.json").exists();
    if !is_nextjs {
        return;
    }

    debug_log_to_file("[Self-Healing] Starting compilation check loop...".to_string());
    
    // Run build and self-heal up to 3 times
    for attempt in 1..=3 {
        debug_log_to_file(format!("[Self-Healing] Build check attempt {}/3...", attempt));
        
        #[cfg(target_os = "windows")]
        let mut cmd = std::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        cmd.arg("/C").arg("npx").arg("--no-install").arg("next").arg("build").current_dir(proj_dir);
        
        #[cfg(not(target_os = "windows"))]
        let mut cmd = std::process::Command::new("npx");
        #[cfg(not(target_os = "windows"))]
        cmd.arg("--no-install").arg("next").arg("build").current_dir(proj_dir);
        
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output_res = cmd.output();
        let output = match output_res {
            Ok(o) => o,
            Err(e) => {
                debug_log_to_file(format!("[Self-Healing] Failed to execute build command: {}", e));
                break;
            }
        };

        if output.status.success() {
            debug_log_to_file("[Self-Healing] Build completed successfully! No errors.".to_string());
            break;
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        let build_output = format!("{}\n{}", stdout_str, stderr_str);
        
        debug_log_to_file(format!("[Self-Healing] Build failed. Log size: {} characters.", build_output.len()));
        
        // Find which files in the project appear in the build error log
        let mut files_with_errors = Vec::new();
        for file_spec in files_list {
            if let Some(path_str) = file_spec.get("path").and_then(|p| p.as_str()) {
                let path_lower = path_str.to_lowercase().replace('\\', "/");
                let build_lower = build_output.to_lowercase().replace('\\', "/");
                if build_lower.contains(&path_lower) {
                    let file_path = proj_dir.join(path_str);
                    if file_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&file_path) {
                            files_with_errors.push(serde_json::json!({
                                "path": path_str,
                                "content": content
                            }));
                        }
                    }
                }
            }
        }

        if files_with_errors.is_empty() {
            debug_log_to_file("[Self-Healing] Could not map build errors to any project files. Aborting loop.".to_string());
            break;
        }

        debug_log_to_file(format!("[Self-Healing] Identified {} files containing build errors. Requesting AI repairs...", files_with_errors.len()));

        // Call local AI engine to fix the errors
        let client = reqwest::Client::new();
        let mut system_prompt = "You are a Senior Next.js Developer. The project build failed with compilation errors. Review the build log and the contents of the files containing errors. Correct the files to fix the build errors. Respond ONLY with a valid JSON array of objects, where each object has 'path' (the relative file path, e.g. 'src/components/Layout.tsx') and 'content' (the complete corrected file contents). Do not include any explanations, introduction, or markdown block formatting. Your response must be clean JSON only.".to_string();
        system_prompt.push_str(&get_packages_db_summary());
        
        // Truncate build log if too long
        let truncated_log = if build_output.len() > 3000 {
            format!("...[truncated]...\n{}", &build_output[build_output.len() - 3000..])
        } else {
            build_output.clone()
        };

        let user_prompt = format!(
            "Build log with error details:\n{}\n\nFiles with errors current contents:\n{}\n\nPlease provide corrected code for these files in JSON format as specified.",
            truncated_log,
            serde_json::to_string_pretty(&files_with_errors).unwrap_or_default()
        );

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

        match client.post("http://127.0.0.1:11434/api/chat")
            .json(&ollama_req)
            .send()
            .await 
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(resp_json) = resp.json::<serde_json::Value>().await {
                        if let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                            let clean_content = extract_code_content(ai_msg);
                            if let Ok(fixed_files) = serde_json::from_str::<serde_json::Value>(&clean_content) {
                                if let Some(files_arr) = fixed_files.as_array() {
                                    for item in files_arr {
                                        if let (Some(path_str), Some(content_str)) = (item.get("path").and_then(|p| p.as_str()), item.get("content").and_then(|c| c.as_str())) {
                                            let full_path = proj_dir.join(path_str);
                                            if full_path.starts_with(proj_dir) {
                                                if let Some(parent) = full_path.parent() {
                                                    let _ = std::fs::create_dir_all(parent);
                                                }
                                                let processed = post_process_generated_file(path_str, content_str, &serde_json::json!({}), has_tailwind);
                                                if let Ok(_) = std::fs::write(&full_path, &processed) {
                                                    debug_log_to_file(format!("[Self-Healing] Successfully repaired/created file: {}", path_str));
                                                    
                                                    // Find the original content
                                                    let mut original_content = String::new();
                                                    for original_spec in &files_with_errors {
                                                        if original_spec.get("path").and_then(|p| p.as_str()) == Some(path_str) {
                                                            if let Some(c) = original_spec.get("content").and_then(|c| c.as_str()) {
                                                                original_content = c.to_string();
                                                            }
                                                        }
                                                    }
                                                    if !original_content.is_empty() {
                                                        generate_and_save_healing_rule(path_str, &original_content, &processed, model).await;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                debug_log_to_file(format!("[Self-Healing] Failed to parse AI response as JSON: {}", clean_content));
                            }
                        }
                    }
                } else {
                    debug_log_to_file(format!("[Self-Healing] AI API call returned status error: {}", resp.status()));
                }
            }
            Err(e) => {
                debug_log_to_file(format!("[Self-Healing] AI request connection error: {}", e));
            }
        }
    }
}

async fn setup_local_postgres(_proj_dir: &std::path::Path, db_name: &str) -> Result<String, String> {
    // Create docker-compose.yml in the project directory
    let docker_compose_content = format!(
        "version: '3.8'\n\
         services:\n\
         \x20\x20db:\n\
         \x20\x20\x20\x20image: postgres:15-alpine\n\
         \x20\x20\x20\x20environment:\n\
         \x20\x20\x20\x20\x20\x20POSTGRES_DB: {}\n\
         \x20\x20\x20\x20\x20\x20POSTGRES_USER: postgres\n\
         \x20\x20\x20\x20\x20\x20POSTGRES_PASSWORD: postgres_password\n\
         \x20\x20\x20\x20ports:\n\
         \x20\x20\x20\x20\x20\x20- \"54321:5432\"\n\
         \x20\x20\x20\x20volumes:\n\
         \x20\x20\x20\x20\x20\x20- pgdata:/var/lib/postgresql/data\n\
         volumes:\n\
         \x20\x20pgdata:\n",
        db_name
    );
    let docker_compose_path = _proj_dir.join("docker-compose.yml");
    let _ = std::fs::write(&docker_compose_path, docker_compose_content);

    // Create .env.example
    let env_example_content = format!(
        "DATABASE_URL=\"postgresql://postgres:postgres_password@localhost:54321/{}?schema=public\"\n\
         NEXTAUTH_SECRET=\"axiom-dev-secret-change-in-production\"\n",
        db_name
    );
    let env_example_path = _proj_dir.join(".env.example");
    let _ = std::fs::write(&env_example_path, env_example_content);

    // Try starting PostgreSQL via Docker Compose
    let mut docker_success = false;
    
    let mut compose_cmd = std::process::Command::new("docker");
    compose_cmd.arg("compose").arg("up").arg("-d").current_dir(_proj_dir);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        compose_cmd.creation_flags(0x08000000);
    }
    
    if let Ok(status) = compose_cmd.status() {
        if status.success() {
            docker_success = true;
        }
    }
    
    if !docker_success {
        let mut legacy_cmd = std::process::Command::new("docker-compose");
        legacy_cmd.arg("up").arg("-d").current_dir(_proj_dir);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            legacy_cmd.creation_flags(0x08000000);
        }
        if let Ok(status) = legacy_cmd.status() {
            if status.success() {
                docker_success = true;
            }
        }
    }

    if docker_success {
        // Wait for database initialization
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        return Ok(format!("postgresql://postgres:postgres_password@localhost:54321/{}?schema=public", db_name));
    }

    let app_data = dirs::data_local_dir()
        .ok_or_else(|| "Could not determine local app data directory".to_string())?
        .join("axiom-forge")
        .join("postgres");
    
    let pg_dir = app_data.join("pgsql");
    let pg_bin = pg_dir.join("bin");
    let pg_postgres_exe = pg_bin.join("postgres.exe");
    let pg_data_dir = app_data.join("data");
    
    if !pg_postgres_exe.exists() {
        let _ = std::fs::create_dir_all(&app_data);
        let zip_path = app_data.join("postgresql.zip");
        
        let pg_url = "https://get.enterprisedb.com/postgresql/postgresql-15.8-1-windows-x64-binaries.zip";
        let response = reqwest::get(pg_url).await
            .map_err(|e| format!("Failed to download PostgreSQL: {}", e))?;
            
        let bytes = response.bytes().await
            .map_err(|e| format!("Failed to read PostgreSQL download bytes: {}", e))?;
            
        std::fs::write(&zip_path, &bytes)
            .map_err(|e| format!("Failed to save PostgreSQL zip file: {}", e))?;
            
        let mut unzip_cmd = std::process::Command::new("powershell");
        unzip_cmd.arg("-Command")
            .arg(format!("Expand-Archive -Path '{}' -DestinationPath '{}' -Force", zip_path.to_string_lossy(), app_data.to_string_lossy()));
            
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            unzip_cmd.creation_flags(0x08000000);
        }
        
        let status = unzip_cmd.status()
            .map_err(|e| format!("Failed to run PowerShell Expand-Archive: {}", e))?;
            
        if !status.success() {
            return Err("PowerShell Expand-Archive exited with error status".to_string());
        }
        
        let _ = std::fs::remove_file(&zip_path);
    }
    
    if !pg_data_dir.exists() || std::fs::read_dir(&pg_data_dir).map(|mut d| d.next().is_none()).unwrap_or(true) {
        let _ = std::fs::create_dir_all(&pg_data_dir);
        let initdb_exe = pg_bin.join("initdb.exe");
        let mut init_cmd = std::process::Command::new(initdb_exe);
        init_cmd.arg("-D").arg(&pg_data_dir)
            .arg("-U").arg("postgres")
            .arg("--auth-local=trust")
            .arg("--auth-host=trust");
            
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            init_cmd.creation_flags(0x08000000);
        }
        
        let status = init_cmd.status()
            .map_err(|e| format!("Failed to run initdb: {}", e))?;
            
        if !status.success() {
            return Err("initdb failed to initialize database cluster".to_string());
        }
    }
    
    let pg_ctl_exe = pg_bin.join("pg_ctl.exe");
    let mut start_cmd = std::process::Command::new(pg_ctl_exe);
    start_cmd.arg("start")
        .arg("-D").arg(&pg_data_dir)
        .arg("-o").arg("-p 54321 -h 127.0.0.1 -N 20 -B 1024 -c work_mem=1MB -c maintenance_work_mem=8MB -c shared_buffers=16MB");
        
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        start_cmd.creation_flags(0x08000000);
    }
    
    let _ = start_cmd.status();
    
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    
    let psql_exe = pg_bin.join("psql.exe");
    let mut db_cmd = std::process::Command::new(psql_exe);
    db_cmd.arg("-h").arg("127.0.0.1")
        .arg("-p").arg("54321")
        .arg("-U").arg("postgres")
        .arg("-d").arg("postgres")
        .arg("-c").arg(format!("CREATE DATABASE {};", db_name));
        
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        db_cmd.creation_flags(0x08000000);
    }
    
    let _ = db_cmd.status();
    
    Ok(format!("postgresql://postgres@localhost:54321/{}?schema=public", db_name))
}

fn update_env_database_url(proj_dir: &std::path::Path, db_url: &str) {
    let env_paths = [proj_dir.join(".env"), proj_dir.join(".env.local")];
    for env_path in env_paths {
        if env_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&env_path) {
                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let mut found = false;
                for line in lines.iter_mut() {
                    if line.starts_with("DATABASE_URL=") {
                        *line = format!("DATABASE_URL=\"{}\"", db_url);
                        found = true;
                    }
                }
                if !found {
                    lines.push(format!("DATABASE_URL=\"{}\"", db_url));
                }
                let _ = std::fs::write(&env_path, lines.join("\n"));
            }
        } else {
            let _ = std::fs::write(&env_path, format!("DATABASE_URL=\"{}\"\n", db_url));
        }
    }
}

fn fallback_to_sqlite(proj_dir: &std::path::Path) -> Result<(), String> {
    let schema_paths = [proj_dir.join("prisma/schema.prisma"), proj_dir.join("schema.prisma")];
    let mut schema_path = None;
    for path in schema_paths {
        if path.exists() {
            schema_path = Some(path);
            break;
        }
    }
    
    if let Some(path) = schema_path {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let updated = content
                .replace("provider = \"postgresql\"", "provider = \"sqlite\"")
                .replace("url      = env(\"DATABASE_URL\")", "url      = \"file:./dev.db\"");
            let _ = std::fs::write(&path, updated);
        }
    }
    
    let env_paths = [proj_dir.join(".env"), proj_dir.join(".env.local")];
    for env_path in env_paths {
        if env_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&env_path) {
                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let mut found = false;
                for line in lines.iter_mut() {
                    if line.starts_with("DATABASE_URL=") {
                        *line = "DATABASE_URL=\"file:./dev.db\"".to_string();
                        found = true;
                    }
                }
                if !found {
                    lines.push("DATABASE_URL=\"file:./dev.db\"".to_string());
                }
                let _ = std::fs::write(&env_path, lines.join("\n"));
            }
        } else {
            let _ = std::fs::write(&env_path, "DATABASE_URL=\"file:./dev.db\"\n");
        }
    }
    
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum GenerationPhase {
    Config = 1,
    DatabaseOrMain = 2,
    ApiOrState = 3,
    LogicOrHooks = 4,
    ShellOrWidgets = 5,
    PagesOrUi = 6,
}

fn get_generation_phase(file_path: &str, project_type: &str) -> GenerationPhase {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    
    match project_type {
        "nextjs" | "mern" => {
            if path_lower == "package.json" || path_lower == "tsconfig.json" || 
               path_lower == "tailwind.config.js" || path_lower == "postcss.config.js" || 
               path_lower == "next.config.js" || path_lower.contains("config") || path_lower.ends_with(".json") {
                GenerationPhase::Config
            } else if path_lower.contains("schema.prisma") || path_lower.contains("prisma") || path_lower.contains("models/") || path_lower.contains("database") {
                GenerationPhase::DatabaseOrMain
            } else if path_lower.contains("hooks/") || path_lower.contains("utils/") || path_lower.contains("lib/") {
                GenerationPhase::LogicOrHooks
            } else if path_lower.contains("api/") || path_lower.contains("auth") || path_lower.contains("router") {
                GenerationPhase::ApiOrState
            } else if path_lower.contains("layout.tsx") || path_lower.contains("layout.jsx") || path_lower.contains("layout.js") || 
                      path_lower.contains("globals.css") || path_lower.contains("styles/") || path_lower.contains("navigation") {
                GenerationPhase::ShellOrWidgets
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        "electron" | "rust" => {
            if path_lower == "package.json" || path_lower == "cargo.toml" || path_lower.contains("config") {
                GenerationPhase::Config
            } else if path_lower.contains("main.js") || path_lower.contains("main.ts") || path_lower.contains("preload.js") || 
                      path_lower.contains("main.rs") || path_lower.contains("commands") {
                GenerationPhase::DatabaseOrMain
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        "flutter" => {
            if path_lower.contains("pubspec.yaml") || path_lower.contains("config") {
                GenerationPhase::Config
            } else if path_lower.contains("models/") || path_lower.contains("sqlite") || path_lower.contains("db") || 
                      path_lower.contains("api") || path_lower.contains("bloc") || path_lower.contains("provider") {
                GenerationPhase::DatabaseOrMain
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        _ => {
            if path_lower.contains("package.json") || path_lower.contains("cargo.toml") || path_lower.ends_with(".json") {
                GenerationPhase::Config
            } else if path_lower.contains("server") || path_lower.contains("db") || path_lower.contains("api") || path_lower.contains("lib") {
                GenerationPhase::DatabaseOrMain
            } else {
                GenerationPhase::PagesOrUi
            }
        }
    }
}

fn get_context_files_for(file_path: &str, proj_dir: &std::path::Path) -> String {
    let mut context = String::new();
    
    let prisma_path = proj_dir.join("prisma/schema.prisma");
    let prisma_path_alt = proj_dir.join("schema.prisma");
    let actual_prisma = if prisma_path.exists() { Some(prisma_path) } else if prisma_path_alt.exists() { Some(prisma_path_alt) } else { None };
    
    if let Some(p_path) = actual_prisma {
        if file_path != "prisma/schema.prisma" && file_path != "schema.prisma" {
            if let Ok(content) = std::fs::read_to_string(&p_path) {
                context.push_str(&format!("\nReference: Database Schema (schema.prisma):\n```prisma\n{}\n```\n", content));
            }
        }
    }
    
    let prisma_lib = proj_dir.join("src/lib/prisma.ts");
    let prisma_lib_alt = proj_dir.join("src/lib/prisma.js");
    let actual_prisma_lib = if prisma_lib.exists() { Some(prisma_lib) } else if prisma_lib_alt.exists() { Some(prisma_lib_alt) } else { None };
    if let Some(pl_path) = actual_prisma_lib {
        if !file_path.contains("prisma") {
            if let Ok(content) = std::fs::read_to_string(&pl_path) {
                context.push_str(&format!("\nReference: Prisma client export (src/lib/prisma.ts):\n```typescript\n{}\n```\n", content));
            }
        }
    }
    
    let auth_route = proj_dir.join("src/app/api/auth/[...nextauth]/route.ts");
    let auth_route_alt = proj_dir.join("src/app/api/auth/[...nextauth]/route.js");
    let actual_auth_route = if auth_route.exists() { Some(auth_route) } else if auth_route_alt.exists() { Some(auth_route_alt) } else { None };
    if let Some(ar_path) = actual_auth_route {
        if file_path.contains("hooks") || file_path.contains("components") || file_path.contains("app") {
            if let Ok(content) = std::fs::read_to_string(&ar_path) {
                context.push_str(&format!("\nReference: NextAuth configuration (src/app/api/auth/[...nextauth]/route.ts):\n```typescript\n{}\n```\n", content));
            }
        }
    }
    
    if file_path.contains("components") || (file_path.contains("app") && file_path.ends_with("page.tsx")) {
        let hooks_dir = proj_dir.join("src/hooks");
        if hooks_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(hooks_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                context.push_str(&format!("\nReference: Hook (src/hooks/{}):\n```typescript\n{}\n```\n", name, content));
                            }
                        }
                    }
                }
            }
        }
    }
    
    context
}

async fn validate_file_with_abstract_rules(
    file_path: &str,
    content: &str,
    model_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
        
    let system_prompt = "You are a strict code quality validator. Analyze the provided file path and code content for logical errors, missing imports, syntax errors, or architectural violations. If it is a Next.js App Router layout, check that globals.css is imported. If NextAuth is used, ensure there are no incorrect imports for Prisma or NextAuth. Respond ONLY with a JSON object in this format: { \"valid\": true } or { \"valid\": false, \"reason\": \"Detailed description of the issue\" }.";
    
    let user_prompt = format!("File Path: {}\n\nCode Content:\n```\n{}\n```", file_path, content);
    
    let ollama_req = serde_json::json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": &user_prompt }
        ],
        "stream": false,
        "options": {
            "temperature": 0.1
        }
    });
    
    let res = client.post("http://127.0.0.1:11434/api/chat")
        .json(&ollama_req)
        .send()
        .await;
        
    if let Ok(resp) = res {
        if resp.status().is_success() {
            if let Ok(resp_json) = resp.json::<Value>().await {
                if let Some(content_str) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                    let cleaned = extract_code_content(content_str);
                    if let Ok(parsed) = serde_json::from_str::<Value>(&cleaned) {
                        if let Some(valid) = parsed.get("valid").and_then(|v| v.as_bool()) {
                            if valid {
                                return Ok(());
                            } else {
                                let reason = parsed.get("reason").and_then(|r| r.as_str()).unwrap_or("Unknown validation issue");
                                return Err(reason.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}

fn generate_axiom_map(proj_dir: &std::path::Path) -> Result<(), String> {
    use std::fs;
    use std::path::{Path, PathBuf};
    
    let mut files_map = serde_json::Map::new();
    let mut methods_map = serde_json::Map::new();
    
    let mut source_files = Vec::new();
    
    fn scan_dir(dir: &Path, source_files: &mut Vec<PathBuf>) {
        if !dir.exists() { return; }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules" && name != ".next" && name != "build" && name != "dist" {
                        scan_dir(&path, source_files);
                    }
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                            source_files.push(path);
                        }
                    }
                }
            }
        }
    }
    
    scan_dir(proj_dir, &mut source_files);
    
    for file_path in &source_files {
        let Ok(content) = fs::read_to_string(file_path) else { continue; };
        let Ok(rel_path) = file_path.strip_prefix(proj_dir) else { continue; };
        let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");
        
        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut local_deps = Vec::new();
        
        for line in content.lines() {
            let line = line.trim();
            
            if line.starts_with("import ") || line.starts_with("import{") {
                if let Some(from_pos) = line.rfind("from") {
                    let path_part = &line[from_pos + 4..].trim();
                    let cleaned = path_part.trim_matches(|c| c == '\'' || c == '"' || c == ';').trim().to_string();
                    if !cleaned.is_empty() {
                        imports.push(cleaned.clone());
                        
                        let mut resolved_dep = None;
                        if cleaned.starts_with("@/") {
                            let rel_sub = cleaned.replacen("@/", "src/", 1);
                            resolved_dep = Some(proj_dir.join(rel_sub));
                        } else if cleaned.starts_with("./") || cleaned.starts_with("../") {
                            if let Some(parent) = file_path.parent() {
                                resolved_dep = Some(parent.join(&cleaned));
                            }
                        }
                        
                        if let Some(dep_base) = resolved_dep {
                            let extensions = vec!["ts", "tsx", "js", "jsx"];
                            for ext in extensions {
                                let dep_file = dep_base.with_extension(ext);
                                if dep_file.exists() {
                                    if let Ok(dep_rel) = dep_file.strip_prefix(proj_dir) {
                                        local_deps.push(dep_rel.to_string_lossy().replace('\\', "/"));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            if line.starts_with("export ") {
                if line.contains("default") {
                    exports.push("default".to_string());
                } else {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for (idx, &part) in parts.iter().enumerate() {
                        if (part == "const" || part == "let" || part == "var" || part == "function" || part == "type" || part == "interface" || part == "class") && idx + 1 < parts.len() {
                            let name = parts[idx + 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                            if !name.is_empty() {
                                exports.push(name.to_string());
                            }
                            break;
                        }
                    }
                }
            }
            
            if (line.starts_with("export function ") && line.contains("(")) || 
               (line.starts_with("function ") && line.contains("(")) ||
               (line.contains("const ") && line.contains("=") && line.contains("=>") && line.contains("(")) {
                
                let mut method_name = String::new();
                if line.starts_with("export function ") {
                    if let Some(rest) = line.strip_prefix("export function ") {
                        method_name = rest.split('(').next().unwrap_or("").trim().to_string();
                    }
                } else if line.starts_with("function ") {
                    if let Some(rest) = line.strip_prefix("function ") {
                        method_name = rest.split('(').next().unwrap_or("").trim().to_string();
                    }
                } else if line.contains("const ") {
                    if let Some(start_idx) = line.find("const ") {
                        let after_const = &line[start_idx + 6..];
                        if let Some(eq_idx) = after_const.find('=') {
                            method_name = after_const[..eq_idx].trim().to_string();
                        }
                    }
                }
                
                method_name = method_name.trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string();
                
                if !method_name.is_empty() && method_name != "default" {
                    let mut params = Vec::new();
                    if let Some(open_paren) = line.find('(') {
                        if let Some(close_paren) = line.find(')') {
                            let param_sub = &line[open_paren + 1..close_paren];
                            for p in param_sub.split(',') {
                                let p_trimmed = p.trim().to_string();
                                if !p_trimmed.is_empty() {
                                    params.push(p_trimmed);
                                }
                            }
                        }
                    }
                    
                    methods_map.insert(
                        method_name,
                        serde_json::json!({
                            "definedIn": rel_path_str.clone(),
                            "parameters": params,
                            "returns": "any"
                        })
                    );
                }
            }
        }
        
        files_map.insert(
            rel_path_str,
            serde_json::json!({
                "exports": exports,
                "imports": imports,
                "dependencies": local_deps
            })
        );
    }
    
    let axiom_map = serde_json::json!({
        "files": files_map,
        "methods": methods_map
    });
    
    if let Ok(pretty) = serde_json::to_string_pretty(&axiom_map) {
        let _ = fs::write(proj_dir.join("axiom-map.json"), pretty);
    }
    
    Ok(())
}

fn get_project_routes_prompt(files: &[Value]) -> String {
    let mut prompt = String::new();
    let mut routes = Vec::new();
    
    for file in files {
        if let Some(path_str) = file.get("path").and_then(|p| p.as_str()) {
            let path_lower = path_str.to_lowercase().replace('\\', "/");
            if path_lower.starts_with("src/app/") && path_lower.ends_with("/page.tsx") {
                let route = path_lower
                    .strip_prefix("src/app/")
                    .unwrap_or(&path_lower)
                    .strip_suffix("/page.tsx")
                    .unwrap_or("");
                if route.is_empty() {
                    routes.push(("/".to_string(), path_str.to_string()));
                } else {
                    routes.push((format!("/{}", route), path_str.to_string()));
                }
            } else if path_lower.starts_with("app/") && path_lower.ends_with("/page.tsx") {
                let route = path_lower
                    .strip_prefix("app/")
                    .unwrap_or(&path_lower)
                    .strip_suffix("/page.tsx")
                    .unwrap_or("");
                if route.is_empty() {
                    routes.push(("/".to_string(), path_str.to_string()));
                } else {
                    routes.push((format!("/{}", route), path_str.to_string()));
                }
            }
        }
    }
    
    if !routes.is_empty() {
        prompt.push_str("\nAvailable Pages/Routes in this Next.js project. You MUST use these exact paths for navigation links:\n");
        for (route, file_path) in routes {
            prompt.push_str(&format!("- Route '{}' -> maps to file '{}'\n", route, file_path));
        }
    }
    
    prompt
}

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
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
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
    let project_type = detect_project_type(&manifest);
    let mut metamanifest = manifest.get("metamanifest").cloned().unwrap_or(serde_json::json!({}));
    
    // Load default metamanifest definitions based on project type
    let mut default_metamanifest = serde_json::Value::Null;
    if project_type == "nextjs" {
        default_metamanifest = serde_json::json!({
            "rootFiles": ["package.json", "tsconfig.json", "next.config.js", "tailwind.config.js", "postcss.config.js", "next-env.d.ts"],
            "rootFilesMoveFromSrc": true,
            "requiredFilesTemplates": [
                {
                    "path": "package.json",
                    "description": "Standard package.json for Next.js project with Tailwind CSS.",
                    "language": "json",
                    "mergeDependencies": {
                        "dependencies": {
                            "next": "^14.2.3",
                            "react": "^18.3.1",
                            "react-dom": "^18.3.1",
                            "lucide-react": "^0.460.0",
                            "next-auth": "^4.24.7",
                            "@next-auth/prisma-adapter": "^1.0.7",
                            "@prisma/client": "^5.14.0",
                            "resend": "^3.2.0",
                            "bcryptjs": "^2.4.3"
                        },
                        "devDependencies": {
                            "tailwindcss": "^3.4.14",
                            "postcss": "^8.4.47",
                            "autoprefixer": "^10.4.20",
                            "typescript": "^5.0.0",
                            "@types/react": "^18.3.1",
                            "@types/react-dom": "^18.3.1",
                            "@types/node": "^20.0.0",
                            "prisma": "^5.14.0",
                            "@types/bcryptjs": "^2.4.6"
                        },
                        "scripts": {
                            "dev": "next dev",
                            "build": "next build",
                            "start": "next start"
                        }
                    }
                },
                {
                    "path": "tailwind.config.js",
                    "description": "Configuration file for Tailwind CSS.",
                    "language": "javascript",
                    "defaultContent": "/** @type {import('tailwindcss').Config} */\nmodule.exports = {\n  content: [\n    \"./src/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./app/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./components/**/*.{js,ts,jsx,tsx,mdx}\",\n    \"./pages/**/*.{js,ts,jsx,tsx,mdx}\"\n  ],\n  theme: {\n    extend: {},\n  },\n  plugins: [],\n}"
                },
                {
                    "path": "postcss.config.js",
                    "description": "Configuration file for PostCSS.",
                    "language": "javascript",
                    "defaultContent": "module.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n}"
                },
                {
                    "path": "tsconfig.json",
                    "description": "TypeScript compiler settings.",
                    "language": "json",
                    "defaultContent": "{\n  \"compilerOptions\": {\n    \"lib\": [\"dom\", \"dom.iterable\", \"esnext\"],\n    \"allowJs\": true,\n    \"skipLibCheck\": true,\n    \"strict\": false,\n    \"noEmit\": true,\n    \"esModuleInterop\": true,\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"resolveJsonModule\": true,\n    \"isolatedModules\": true,\n    \"jsx\": \"preserve\",\n    \"incremental\": true,\n    \"plugins\": [\n      {\n        \"name\": \"next\"\n      }\n    ],\n    \"paths\": {\n      \"@/*\": [\"./src/*\"]\n    }\n  },\n  \"include\": [\"next-env.d.ts\", \"**/*.ts\", \"**/*.tsx\", \".next/types/**/*.ts\"],\n  \"exclude\": [\"node_modules\"]\n}"
                },
                {
                    "path": "next.config.js",
                    "description": "Next.js configuration.",
                    "language": "javascript",
                    "defaultContent": "/** @type {import('next').NextConfig} */\nconst nextConfig = {\n  reactStrictMode: true,\n  swcMinify: true,\n};\nmodule.exports = nextConfig;"
                },
                {
                    "path": "src/app/globals.css",
                    "description": "Global CSS file containing Tailwind directives.",
                    "language": "css",
                    "defaultContent": "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n\n"
                },
                {
                    "path": "src/app/layout.tsx",
                    "description": "Root layout for Next.js App Router.",
                    "language": "typescript",
                    "defaultContent": "import React from 'react';\nimport './globals.css';\n\nexport const metadata = {\n  title: 'Axiom Forge App',\n  description: 'Generated by Axiom Forge',\n};\n\nexport default function RootLayout({\n  children,\n}: {\n  children: React.ReactNode;\n}) {\n  return (\n    <html lang=\"en\">\n      <body>\n        {children}\n      </body>\n    </html>\n  );\n}"
                }
            ],
            "injectTailwindDirectives": {
                "targetCssFiles": ["globals.css", "styles.css"]
            }
        });
    } else if project_type == "mern" {
        default_metamanifest = serde_json::json!({
            "rootFiles": ["package.json"],
            "rootFilesMoveFromSrc": true,
            "requiredFilesTemplates": [
                {
                    "path": "package.json",
                    "description": "Root package.json for MERN stack workspace.",
                    "language": "json",
                    "mergeDependencies": {
                        "dependencies": {
                            "express": "^4.19.2",
                            "cors": "^2.8.5",
                            "dotenv": "^16.4.5",
                            "jsonwebtoken": "^9.0.2",
                            "bcryptjs": "^2.4.3"
                        },
                        "devDependencies": {
                            "nodemon": "^3.1.0",
                            "concurrently": "^8.2.2"
                        },
                        "scripts": {
                            "dev": "concurrently \"npm run server\" \"npm run client\"",
                            "server": "nodemon src/server.js",
                            "client": "npm run dev --prefix client",
                            "start": "node src/server.js"
                        }
                    }
                },
                {
                    "path": "src/server.js",
                    "description": "Entry point for Express backend.",
                    "language": "javascript"
                },
                {
                    "path": "client/package.json",
                    "description": "Package settings for MERN frontend.",
                    "language": "json",
                    "mergeDependencies": {
                        "dependencies": {
                            "react": "^18.3.1",
                            "react-dom": "^18.3.1",
                            "react-router-dom": "^6.22.3",
                            "axios": "^1.6.8"
                        },
                        "devDependencies": {
                            "vite": "^5.2.0",
                            "tailwindcss": "^3.4.1"
                        }
                    }
                }
            ]
        });
    } else if project_type == "electron" {
        default_metamanifest = serde_json::json!({
            "rootFiles": ["package.json"],
            "rootFilesMoveFromSrc": true,
            "requiredFilesTemplates": [
                {
                    "path": "package.json",
                    "description": "Package settings for Electron desktop application.",
                    "language": "json",
                    "mergeDependencies": {
                        "devDependencies": {
                            "electron": "^30.0.0"
                        },
                        "scripts": {
                            "start": "electron ."
                        }
                    }
                },
                {
                    "path": "main.js",
                    "description": "Main process file for Electron app.",
                    "language": "javascript"
                },
                {
                    "path": "preload.js",
                    "description": "Preload script exposing secure APIs to renderer process.",
                    "language": "javascript"
                }
            ]
        });
    }

    // Deep merge default_metamanifest into metamanifest
    if !default_metamanifest.is_null() {
        if !metamanifest.is_object() {
            metamanifest = serde_json::json!({});
        }
        if let Some(mm_obj) = metamanifest.as_object_mut() {
            let dm_obj = default_metamanifest.as_object().unwrap();

            // 1. Merge rootFiles
            if let Some(dm_rf) = dm_obj.get("rootFiles").and_then(|rf| rf.as_array()) {
                if !mm_obj.contains_key("rootFiles") {
                    mm_obj.insert("rootFiles".to_string(), serde_json::json!(dm_rf));
                } else if let Some(mm_rf) = mm_obj.get_mut("rootFiles").and_then(|rf| rf.as_array_mut()) {
                    for default_file in dm_rf {
                        if let Some(df_str) = default_file.as_str() {
                            let df_lower = df_str.to_lowercase();
                            let exists = mm_rf.iter().any(|f| f.as_str().map(|s| s.to_lowercase()) == Some(df_lower.clone()));
                            if !exists {
                                mm_rf.push(default_file.clone());
                            }
                        }
                    }
                }
            }

            // 2. Merge requiredFilesTemplates
            if let Some(dm_templates) = dm_obj.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
                if !mm_obj.contains_key("requiredFilesTemplates") {
                    mm_obj.insert("requiredFilesTemplates".to_string(), serde_json::json!(dm_templates));
                } else if let Some(mm_templates) = mm_obj.get_mut("requiredFilesTemplates").and_then(|t| t.as_array_mut()) {
                    for default_temp in dm_templates {
                        if let Some(dt_path) = default_temp.get("path").and_then(|p| p.as_str()) {
                            let dt_path_lower = dt_path.to_lowercase();
                            let exists = mm_templates.iter().any(|t| t.get("path").and_then(|p| p.as_str()).map(|s| s.to_lowercase()) == Some(dt_path_lower.clone()));
                            if !exists {
                                mm_templates.push(default_temp.clone());
                            }
                        }
                    }
                }
            }

            // 3. Merge rootFilesMoveFromSrc
            if !mm_obj.contains_key("rootFilesMoveFromSrc") {
                if let Some(val) = dm_obj.get("rootFilesMoveFromSrc") {
                    mm_obj.insert("rootFilesMoveFromSrc".to_string(), val.clone());
                }
            }

            // 4. Merge injectTailwindDirectives
            if !mm_obj.contains_key("injectTailwindDirectives") {
                if let Some(val) = dm_obj.get("injectTailwindDirectives") {
                    mm_obj.insert("injectTailwindDirectives".to_string(), val.clone());
                }
            }
        }
    }

    emit_progress("fetch-manifest", "Manifest secured and ready.", 10.0);

    // Load tech version overrides from settings
    let settings_path = get_base_dir().join("axiom-settings.json");
    let settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    
    let overrides = settings.get("techOverrides");
    let mut overrides_desc = String::new();
    if let Some(ovs) = overrides.and_then(|o| o.as_object()) {
        for (k, v) in ovs {
            if let Some(v_str) = v.as_str() {
                if !v_str.is_empty() {
                    overrides_desc.push_str(&format!("- {}: {}\n", k, v_str));
                }
            }
        }
    }
    if overrides_desc.is_empty() {
        overrides_desc.push_str("- Use default stable versions\n");
    }

    // Manifest Expansion Phase
    emit_progress("manifest-expansion", "Expanding manifest structure with local AI...", 12.0);
    
    let resolved_stack = manifest.get("resolvedStack").and_then(|s| s.as_str()).unwrap_or("Next.js");
    let platform = manifest.get("platform").and_then(|p| p.as_str()).unwrap_or("web");
    
    let original_files_json = serde_json::to_string_pretty(&files).unwrap_or_else(|_| "[]".to_string());
    
    let expansion_prompt = format!(
        "You are a Senior Project Architect. We are generating a complete {} application named '{}' using {}.\n\
         The user requested a fully complete, production-ready implementation.\n\
         Version Overrides requested by user:\n\
         {}\n\n\
         Original files skeleton in the manifest:\n\
         {}\n\n\
         Identify all missing files, routes, state management, components, configuration files, or database schemas needed to make this project complete.\n\
         Output a JSON array containing the expanded, complete list of files. For each file, provide 'path', 'description', and 'language'.\n\
         Ensure all files from the original list are preserved (but you may update their descriptions to match the technology overrides).\n\
         Respond ONLY with a valid JSON array. Do not include any introductory or concluding text, or markdown code blocks.",
         platform, name, resolved_stack, overrides_desc, original_files_json
    );

    let mut files_to_generate = files.clone();
    
    let ollama_req = serde_json::json!({
        "model": &model_id,
        "messages": [
            { "role": "system", "content": "You are a Senior Project Architect. Output only raw JSON. No markdown." },
            { "role": "user", "content": expansion_prompt }
        ],
        "stream": true,
        "options": {
            "temperature": 0.2,
            "num_predict": 8192
        }
    });

    let mut expansion_success = false;
    let mut expansion_attempts = 0;
    let mut max_expansion_attempts = 3;
    let mut last_accumulated_response = String::new();

    while expansion_attempts < max_expansion_attempts && !expansion_success {
        expansion_attempts += 1;
        if expansion_attempts > 1 {
            debug_log_to_file(format!(
                "[Orchestrator] Retrying manifest expansion (Attempt {}/{})",
                expansion_attempts, max_expansion_attempts
            ));
            emit_progress("manifest-expansion", &format!("Retrying manifest expansion (attempt {}/{})...", expansion_attempts, max_expansion_attempts), 12.0);
        }

        let req_fut = client.post("http://127.0.0.1:11434/api/chat")
            .json(&ollama_req)
            .send();

        // 120-second timeout for manifest expansion request (initial connection)
        let resp_res = tokio::time::timeout(std::time::Duration::from_secs(120), req_fut).await;

        match resp_res {
            Ok(Ok(mut resp)) => {
                if resp.status().is_success() {
                    let mut accumulated_response = String::new();
                    let mut chars_received = 0;
                    let mut byte_buf = Vec::new();
                    let stream_start = std::time::Instant::now();
                    #[allow(unused_assignments)]
                    let mut last_chunk_time = stream_start;
                    let mut is_done = false;
                    let mut chunk_error = false;

                    loop {
                        // We want a chunk timeout of 30 seconds
                        let chunk_fut = resp.chunk();
                        let chunk_res = tokio::time::timeout(std::time::Duration::from_secs(30), chunk_fut).await;

                        match chunk_res {
                            Ok(Ok(Some(chunk))) => {
                                last_chunk_time = std::time::Instant::now();
                                byte_buf.extend_from_slice(&chunk);

                                // Process any complete lines in the buffer
                                while let Some(pos) = byte_buf.iter().position(|&b| b == b'\n') {
                                    let line_bytes = byte_buf.drain(..=pos).collect::<Vec<u8>>();
                                    if let Ok(line_str) = std::str::from_utf8(&line_bytes) {
                                        let line_trimmed = line_str.trim();
                                        if !line_trimmed.is_empty() {
                                            if let Ok(val) = serde_json::from_str::<Value>(line_trimmed) {
                                                if let Some(content) = val.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                                                    accumulated_response.push_str(content);
                                                    chars_received += content.len();
                                                }
                                                if let Some(done) = val.get("done").and_then(|d| d.as_bool()) {
                                                    if done {
                                                        is_done = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if is_done {
                                    break;
                                }
                            }
                            Ok(Ok(None)) => {
                                // Stream ended
                                break;
                            }
                            Ok(Err(e)) => {
                                debug_log_to_file(format!("[Orchestrator] Error reading stream chunk (Attempt {}): {}", expansion_attempts, e));
                                chunk_error = true;
                                break;
                            }
                            Err(_) => {
                                debug_log_to_file(format!("[Orchestrator] Chunk read timed out after 30s (Attempt {})", expansion_attempts));
                                chunk_error = true;
                                break;
                            }
                        }

                        // Check global timeout for this attempt
                        let elapsed = stream_start.elapsed();
                        if elapsed.as_secs() > 120 {
                            if chars_received == 0 {
                                debug_log_to_file(format!("[Orchestrator] Stream has not started generating any content after 120s. Aborting attempt {}.", expansion_attempts));
                                chunk_error = true;
                                break;
                            } else {
                                // System has started writing the extended manifest. Check if it's progressing.
                                if last_chunk_time.elapsed().as_secs() > 30 {
                                    debug_log_to_file(format!("[Orchestrator] Stream stalled (no chunks for 30s) after 120s limit. Aborting attempt {}.", expansion_attempts));
                                    chunk_error = true;
                                    break;
                                } else if elapsed.as_secs() > 360 {
                                    debug_log_to_file(format!("[Orchestrator] Stream exceeded maximum allowed time of 360s. Aborting attempt {}.", expansion_attempts));
                                    chunk_error = true;
                                    break;
                                }
                            }
                        }
                    }

                    last_accumulated_response = accumulated_response.clone();

                    if !chunk_error {
                        let cleaned_json = extract_code_content(&accumulated_response);
                        if let Ok(expanded_files) = serde_json::from_str::<Vec<Value>>(&cleaned_json) {
                            if !expanded_files.is_empty() {
                                // Merge to ensure all original manifest files are preserved
                                let mut merged = expanded_files.clone();
                                for orig in files.iter() {
                                    if let Some(orig_path) = orig.get("path").and_then(|p| p.as_str()) {
                                        let exists = merged.iter().any(|f| {
                                            f.get("path").and_then(|p| p.as_str()) == Some(orig_path)
                                        });
                                        if !exists {
                                            merged.push(orig.clone());
                                        }
                                    }
                                }
                                files_to_generate = merged;
                                debug_log_to_file(format!(
                                    "[Orchestrator] Expanded and merged file list from {} to {} files using local Ollama (Attempt {}).",
                                    files.len(), files_to_generate.len(), expansion_attempts
                                ));
                                emit_progress("manifest-expansion", &format!("Manifest expanded to {} files.", files_to_generate.len()), 14.0);
                                expansion_success = true;
                            }
                        } else {
                            debug_log_to_file(format!(
                                "[Orchestrator] Failed to parse expanded manifest JSON (Attempt {}). Content: {}",
                                expansion_attempts, cleaned_json
                            ));
                            if expansion_attempts == 3 && chars_received > 0 {
                                debug_log_to_file("[Orchestrator] Attempt 3 failed to parse but generated content. Granting attempt 4.".to_string());
                                max_expansion_attempts = 4;
                            }
                        }
                    } else {
                        if expansion_attempts == 3 && chars_received > 0 {
                            debug_log_to_file("[Orchestrator] Attempt 3 stalled/errored but generated content. Granting attempt 4.".to_string());
                            max_expansion_attempts = 4;
                        }
                    }
                } else {
                    debug_log_to_file(format!("[Orchestrator] Manifest expansion Ollama status error (Attempt {}): {}", expansion_attempts, resp.status()));
                }
            }
            Ok(Err(e)) => {
                debug_log_to_file(format!("[Orchestrator] Manifest expansion request error (Attempt {}): {}", expansion_attempts, e));
            }
            Err(_) => {
                debug_log_to_file(format!("[Orchestrator] Manifest expansion initial connection timed out after 120s (Attempt {})", expansion_attempts));
            }
        }
    }

    if !expansion_success {
        debug_log_to_file(format!(
            "[Orchestrator] CRITICAL: Manifest expansion failed after {} attempts.\nLast accumulated raw content:\n{}",
            expansion_attempts, last_accumulated_response
        ));
        emit_progress("manifest-expansion", "Manifest expansion failed. Using original manifest files.", 14.0);
    }

    normalize_project_structure(&mut files_to_generate, &metamanifest, &project_type);

    if project_type == "nextjs" {
        let has_root_page = files_to_generate.iter().any(|f| {
            if let Some(p) = f.get("path").and_then(|path| path.as_str()) {
                let p_lower = p.to_lowercase().replace('\\', "/");
                p_lower == "src/app/page.tsx" || p_lower == "src/app/page.jsx" || p_lower == "src/app/page.js" || p_lower == "src/app/page.ts" ||
                p_lower == "app/page.tsx" || p_lower == "app/page.jsx" || p_lower == "app/page.js" || p_lower == "app/page.ts"
            } else {
                false
            }
        });
        
        if !has_root_page {
            let mut has_dashboard = false;
            let mut has_auth = false;
            let mut has_public = false;
            
            for f in &files_to_generate {
                if let Some(p) = f.get("path").and_then(|path| path.as_str()) {
                    let p_lower = p.to_lowercase().replace('\\', "/");
                    if p_lower.contains("dashboard/page") || p_lower.contains("admin/page") {
                        has_dashboard = true;
                    } else if p_lower.contains("auth/login") || p_lower.contains("login/page") || p_lower.contains("signin/page") {
                        has_auth = true;
                    } else if p_lower.contains("products/page") || p_lower.contains("store/page") || p_lower.contains("landing/page") {
                        has_public = true;
                    }
                }
            }
            
            let proj_desc = manifest.get("metadata").and_then(|m| m.get("description")).and_then(|d| d.as_str()).unwrap_or("");
            let proj_name = manifest.get("metadata").and_then(|m| m.get("name")).and_then(|n| n.as_str()).unwrap_or(name);
            
            let (ui_flow, file_desc, default_content) = if has_public || proj_desc.to_lowercase().contains("landing") || proj_desc.to_lowercase().contains("marketing") {
                ("Landing Page",
                 format!("Root landing page/homepage for '{}'. Render a premium, beautifully designed start screen introducing the application features, navigation header, and visual links to core screens. Description: {}.", proj_name, proj_desc),
                 format!("import React from 'react';\nimport Link from 'next/link';\n\nexport default function HomePage() {{\n  return (\n    <div className=\"min-h-screen bg-slate-900 text-white flex flex-col justify-between\" data-axiom-component=\"HomePage\" data-axiom-file=\"src/app/page.tsx\">\n      <header className=\"border-b border-slate-800 py-4 px-6 flex justify-between items-center\">\n        <h1 className=\"text-xl font-bold tracking-tight bg-gradient-to-r from-blue-400 to-indigo-500 bg-clip-text text-transparent\">{}</h1>\n        <div className=\"space-x-4\">\n          <Link href=\"/auth/login\" className=\"text-sm font-medium hover:text-blue-400 transition\">Sign In</Link>\n          <Link href=\"/dashboard\" className=\"text-sm bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded-md font-medium transition\">Dashboard</Link>\n        </div>\n      </header>\n      <main className=\"max-w-4xl mx-auto px-6 py-20 flex-grow flex flex-col justify-center text-center\">\n        <h2 className=\"text-5xl font-extrabold tracking-tight mb-6 bg-gradient-to-r from-white via-slate-200 to-slate-400 bg-clip-text text-transparent leading-tight\">\n          Welcome to {}\n        </h2>\n        <p className=\"text-lg text-slate-400 mb-10 max-w-xl mx-auto\">\n          {}\n        </p>\n        <div className=\"flex justify-center gap-4\">\n          <Link href=\"/dashboard\" className=\"bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-500 hover:to-indigo-500 text-white px-8 py-3 rounded-lg font-semibold shadow-lg shadow-blue-500/20 transition\">\n            Go to Dashboard\n          </Link>\n          <Link href=\"/auth/login\" className=\"border border-slate-700 hover:border-slate-600 px-8 py-3 rounded-lg font-semibold transition\">\n            Sign In\n          </Link>\n        </div>\n      </main>\n      <footer className=\"border-t border-slate-800 py-6 text-center text-sm text-slate-500\">\n        &copy; 2026 {}. All rights reserved.\n      </footer>\n    </div>\n  );\n}}", proj_name, proj_name, proj_desc, proj_name)
                )
            } else if has_auth && !has_dashboard {
                ("Auth-First Flow",
                 format!("Root page of the application '{}'. UI Flow: Auth-First. The application requires authentication to access. Verify session or render a beautiful gateway card directing the user to the Sign In page (/auth/login). Description: {}.", proj_name, proj_desc),
                 format!("import React from 'react';\nimport Link from 'next/link';\n\nexport default function HomePage() {{\n  return (\n    <div className=\"min-h-screen bg-slate-900 text-white flex flex-col justify-center items-center p-6\" data-axiom-component=\"HomePage\" data-axiom-file=\"src/app/page.tsx\">\n      <div className=\"max-w-md w-full bg-slate-800 border border-slate-700 rounded-2xl p-8 text-center shadow-xl\">\n        <h1 className=\"text-2xl font-bold mb-4\">{}</h1>\n        <p className=\"text-slate-400 mb-6\">Authentication is required to access the application features.</p>\n        <Link href=\"/auth/login\" className=\"inline-block w-full bg-blue-600 hover:bg-blue-500 text-white font-semibold py-3 px-6 rounded-lg transition\">\n          Sign In to Account\n        </Link>\n      </div>\n    </div>\n  );\n}}", proj_name)
                )
            } else {
                ("Dashboard-First Flow",
                 format!("Root page of the application '{}'. UI Flow: Dashboard-First. The app is a workspace/dashboard. The homepage should perform a server-side redirect to the Dashboard (/dashboard) using Next.js 'redirect'. Description: {}.", proj_name, proj_desc),
                 "import { redirect } from 'next/navigation';\n\nexport default function HomePage() {\n  redirect('/dashboard');\n}".to_string()
                )
            };
            
            debug_log_to_file(format!("[Orchestrator] Dynamic UI Flow detected: {}. Adding custom page.tsx template.", ui_flow));
            
            // Insert it into files_to_generate
            files_to_generate.push(serde_json::json!({
                "path": "src/app/page.tsx",
                "role": "logic",
                "language": "typescript",
                "description": file_desc,
                "defaultContent": default_content
            }));
        }
    }

    let has_tailwind = files_to_generate.iter().any(|f| {
        if let Some(p) = f.get("path").and_then(|p| p.as_str()) {
            let p_lower = p.to_lowercase();
            p_lower.contains("tailwind") || p_lower.contains("globals.css")
        } else {
            false
        }
    }) || project_type == "nextjs";

    // Sort files_to_generate by generation phase
    files_to_generate.sort_by_key(|file_spec| {
        let file_path = file_spec.get("path").and_then(|p| p.as_str()).unwrap_or("");
        get_generation_phase(file_path, &project_type)
    });

    emit_progress("generate-files", "Checking local AI engine...", 15.0);

    // 2. Generation Loop
    let total_files = files_to_generate.len() as f64;
    let proj_dir = get_projects_dir().join(&project_id);

    for (i, file_spec) in files_to_generate.iter().enumerate() {
        let file_path = file_spec.get("path").and_then(|p| p.as_str()).unwrap_or("unknown.txt");
        let desc = file_spec.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let lang = file_spec.get("language").and_then(|l| l.as_str()).unwrap_or("txt");

        let msg = format!("Generating {}/{} - {}", i + 1, files_to_generate.len(), file_path);
        let current_progress = 25.0 + ((i as f64) / total_files) * 55.0;
        emit_progress("generate-files", &msg, current_progress);

        let mut system_prompt = "You are an expert software engineer. Write only the code, no explanations. Provide only the code, wrapped in markdown code blocks.\n\
                                 IMPORTANT DESIGN & CONTENT REQUIREMENTS:\n\
                                 - Do NOT write minimal skeletons, simple placeholders, or empty cards.\n\
                                 - Generate highly complete, production-ready, and visually beautiful layouts.\n\
                                 - Always write rich, detailed copy and realistic mock data (e.g. detailed statistics, complete form fields, multi-section cards, rich body text) instead of using 'Lorem Ipsum' or empty divs.\n\
                                 - Use comprehensive styles and modern layouts (e.g., responsive grids, flexboxes, borders, shadows, hover effects, nice colors) to create a premium feel.\n\
                                 - For dashboards, include functional widgets, detailed summary stats, interactive tables, and charts where applicable.\n\
                                 - Do NOT write mock authentication handlers, console.log-only form submissions, or stubbed mock responses (e.g. simulating user login with hardcoded credentials or alert boxes).\n\
                                 - Implement fully functional authentication: Login page MUST use NextAuth's `signIn` method (or the project's chosen authentication library) with proper redirection.\n\
                                 - Registration page MUST perform a real API request (e.g. `POST` to `/api/auth/register`) to save the user, and you MUST write the corresponding API route in Next.js (under `src/app/api/auth/register/route.ts` or similar) that securely hashes the password (using bcryptjs) and writes it to the database using Prisma.\n\
                                 - Ensure all forms have fully implemented submission handlers that communicate with real API endpoints, handle loading states, and show real success/error feedbacks.\n\
                                 - PRISMA SERVER-ONLY RULE: Prisma Client (`@prisma/client`) MUST ONLY be imported and used inside Next.js API Route Handlers (files under `src/app/api/`). NEVER import or use Prisma in React hooks, client components, or any file with `'use client'`. Client-side hooks that need database data MUST use `fetch('/api/...')` to call the corresponding API route.\n\
                                 - HOOK EXPORTS RULE: All custom React hooks (e.g. `useAuth`, `useProducts`, `useOrders`) MUST use `export default` (not named exports). Pages and components importing them must use default import syntax (e.g. `import useAuth from '@/hooks/useAuth'`).\n\
                                 - NEXTAUTH ARCHITECTURE: When using next-auth with credentials (email+password), always use `CredentialsProvider` from `next-auth/providers/credentials` with bcryptjs for password comparison and `strategy: 'jwt'` sessions. DO NOT use `EmailProvider` unless the project explicitly requires magic link authentication.\n\
                                 - DATABASE RESILIENCY & FALLBACKS: Database queries (such as Prisma Client calls like `prisma.product.findMany()`) must be wrapped in `try { ... } catch (error) { ... }` blocks. In the `catch` block, log a warning (e.g. `console.warn('Failed to fetch data, using mock data:', error)`) and fall back to a rich mockup dataset (e.g. 10 realistic mockup items with full properties) so that the application can still render and run in preview mode even if the database is offline or not configured.\n\
                                 - Ensure all import paths match existing files and libraries in the project.".to_string();
        if !overrides_desc.trim().is_empty() {
            system_prompt.push_str(&format!(
                "\nUse the following version/stack preferences if applicable:\n{}",
                overrides_desc
            ));
        }

        let pkg_db_summary = get_packages_db_summary();
        system_prompt.push_str(&pkg_db_summary);

        // Include context of already generated files as reference
        let context_ref = get_context_files_for(file_path, &proj_dir);
        if !context_ref.is_empty() {
            system_prompt.push_str(&context_ref);
        }

        // Include routes structure for accurate link pathing
        let routes_ref = get_project_routes_prompt(&files_to_generate);
        if !routes_ref.is_empty() {
            system_prompt.push_str(&routes_ref);
        }

        if file_path == "package.json" || file_path.ends_with("/package.json") {
            let mut pkg_extra = " IMPORTANT: When generating package.json, always use modern, stable dependency versions".to_string();
            if let Some(nextjs_v) = overrides.and_then(|o| o.get("nextjs")).and_then(|n| n.as_str()) {
                if !nextjs_v.is_empty() {
                    pkg_extra.push_str(&format!(" (specifically Next.js {} and React {})", nextjs_v, overrides.and_then(|o| o.get("react")).and_then(|r| r.as_str()).unwrap_or("18")));
                }
            }
            pkg_extra.push_str(". Do NOT use outdated versions.");
            system_prompt.push_str(&pkg_extra);
        }
        let user_prompt = format!("Generate {} code for file: {}\n\nDescription: {}", lang, file_path, desc);

        let mut processed_content = String::new();
        let mut file_gen_success = false;
        let mut attempts = 0;
        let max_attempts = 3;
        let mut current_user_prompt = user_prompt.clone();

        while attempts < max_attempts && !file_gen_success {
            attempts += 1;
            if attempts > 1 {
                debug_log_to_file(format!(
                    "[Orchestrator] Retrying file generation for {} (Attempt {}/{})",
                    file_path, attempts, max_attempts
                ));
                emit_progress(
                    "generate-files",
                    &format!("Retrying {}/{} (attempt {}/{}) - {}", i + 1, files_to_generate.len(), attempts, max_attempts, file_path),
                    current_progress
                );
            }

            let ollama_req = serde_json::json!({
                "model": model_id.clone(), 
                "messages": [
                    { "role": "system", "content": &system_prompt },
                    { "role": "user", "content": &current_user_prompt }
                ],
                "stream": true,
                "options": {
                    "temperature": 0.3,
                    "num_predict": 4096
                }
            });

            let req_fut = client.post("http://127.0.0.1:11434/api/chat")
                .json(&ollama_req)
                .send();

            // 60-second connection timeout
            let resp_res = tokio::time::timeout(std::time::Duration::from_secs(60), req_fut).await;

            let mut resp = match resp_res {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    debug_log_to_file(format!("[Orchestrator] Connection error for {} (Attempt {}): {}", file_path, attempts, e));
                    continue;
                }
                Err(_) => {
                    debug_log_to_file(format!("[Orchestrator] Connection timeout for {} (Attempt {})", file_path, attempts));
                    continue;
                }
            };

            if !resp.status().is_success() {
                debug_log_to_file(format!("[Orchestrator] Ollama status error {} for {} (Attempt {})", resp.status(), file_path, attempts));
                continue;
            }

            let mut full_content = String::new();
            let mut token_count = 0;
            let mut loop_detected = false;
            let mut stream_stalled = false;

            loop {
                let chunk_fut = resp.chunk();
                // 25-second timeout per streaming chunk
                let chunk_res = tokio::time::timeout(std::time::Duration::from_secs(25), chunk_fut).await;

                let chunk = match chunk_res {
                    Ok(Ok(Some(c))) => c,
                    Ok(Ok(None)) => break, // normal end of stream
                    Ok(Err(e)) => {
                        debug_log_to_file(format!("[Orchestrator] Chunk read error for {} (Attempt {}): {}", file_path, attempts, e));
                        stream_stalled = true;
                        break;
                    }
                    Err(_) => {
                        debug_log_to_file(format!("[Orchestrator] Streaming chunk timeout for {} (Attempt {})", file_path, attempts));
                        stream_stalled = true;
                        break;
                    }
                };

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

                                // Detect repetition loop every 20 tokens
                                if token_count % 20 == 0 && detect_infinite_loop(&full_content) {
                                    debug_log_to_file(format!(
                                        "[Orchestrator] Infinite loop detected for {} (Attempt {}/{} at token {})!",
                                        file_path, attempts, max_attempts, token_count
                                    ));
                                    loop_detected = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if loop_detected {
                    break;
                }
            }

            if loop_detected || stream_stalled {
                continue;
            }

            // Clean markdown backticks
            let cleaned = extract_code_content(&full_content);
            processed_content = post_process_generated_file(file_path, &cleaned, &metamanifest, has_tailwind);
            
            // Run abstract validation agent
            match validate_file_with_abstract_rules(file_path, &processed_content, &model_id).await {
                Ok(()) => {
                    file_gen_success = true;
                }
                Err(validation_error) => {
                    debug_log_to_file(format!(
                        "[Validator] Validation failed for {} (Attempt {}/{}): {}. Retrying with feedback.",
                        file_path, attempts, max_attempts, validation_error
                    ));
                    if attempts < max_attempts {
                        current_user_prompt = format!(
                            "Generate {} code for file: {}\n\nDescription: {}\n\nIMPORTANT: The previous attempt failed validation with the following error:\n{}\nPlease correct this error in your new implementation.",
                            lang, file_path, desc, validation_error
                        );
                        file_gen_success = false;
                    } else {
                        // Accept anyway to avoid blockages
                        file_gen_success = true;
                    }
                }
            }
        }

        if !file_gen_success {
            return Err(format!("Failed to generate file {} after 3 attempts due to infinite loops or timeouts.", file_path));
        }

        // 3. Save File
        let full_disk_path = proj_dir.join(file_path);
        if let Some(parent) = full_disk_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&full_disk_path, &processed_content);
    }

    // Setup local PostgreSQL database if project uses Prisma
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        emit_progress("install-deps", "Checking database setup...", 74.0);
        let db_name = name.replace("-", "_").replace(" ", "_").to_lowercase();
        
        #[cfg(target_os = "windows")]
        {
            emit_progress("install-deps", "Configuring local PostgreSQL database...", 74.2);
            match setup_local_postgres(&proj_dir, &db_name).await {
                Ok(db_url) => {
                    update_env_database_url(&proj_dir, &db_url);
                    emit_progress("install-deps", "Local PostgreSQL database configured successfully.", 74.8);
                }
                Err(err) => {
                    emit_progress("install-deps", &format!("PostgreSQL setup failed: {}. Falling back to SQLite...", err), 74.5);
                    let _ = fallback_to_sqlite(&proj_dir);
                }
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            emit_progress("install-deps", "Non-Windows environment detected. Using local SQLite database fallback.", 74.5);
            let _ = fallback_to_sqlite(&proj_dir);
        }
    }

    // Ensure theme file exists if imported in layout files
    ensure_theme_file_exists(&proj_dir);

    // Resolve any broken local imports in generated JS/TS code
    repair_broken_imports(&proj_dir, &model_id).await;

    // Repair package.json dependencies based on actual source code imports
    repair_package_json_dependencies(&proj_dir);

    // Repair Cargo.toml dependencies if they exist
    repair_cargo_toml_dependencies(&proj_dir);

    // Repair pubspec.yaml dependencies if they exist
    repair_pubspec_yaml_dependencies(&proj_dir);

    // Run npm install with automatic self-healing and version locking
    run_npm_install_with_auto_healing(&proj_dir, &window, "", Some(&task_id)).await;

    // Auto-run prisma generate if Prisma schema exists in the project
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        let schema_path = if proj_dir.join("prisma/schema.prisma").exists() {
            proj_dir.join("prisma/schema.prisma")
        } else {
            proj_dir.join("schema.prisma")
        };
        
        for attempt in 1..=3 {
            emit_progress("install-deps", &format!("Generating Prisma client (attempt {}/3)...", attempt), 78.0);
            debug_log_to_file(format!("[Prisma Generator] Running prisma generate, attempt {}/3...", attempt));
            
            #[cfg(target_os = "windows")]
            let mut prisma_cmd = std::process::Command::new("cmd");
            #[cfg(target_os = "windows")]
            prisma_cmd.arg("/C").arg("npx").arg("-y").arg("prisma").arg("generate").current_dir(&proj_dir);
            
            #[cfg(not(target_os = "windows"))]
            let mut prisma_cmd = std::process::Command::new("npx");
            #[cfg(not(target_os = "windows"))]
            prisma_cmd.arg("-y").arg("prisma").arg("generate").current_dir(&proj_dir);
            
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                prisma_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }
            
            let output_res = prisma_cmd.output();
            let output = match output_res {
                Ok(o) => o,
                Err(e) => {
                    debug_log_to_file(format!("[Prisma Generator] Failed to execute prisma generate: {}", e));
                    break;
                }
            };
            
            if output.status.success() {
                debug_log_to_file("[Prisma Generator] prisma generate completed successfully!".to_string());
                break;
            }
            
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            let stderr_str = String::from_utf8_lossy(&output.stderr);
            let error_log = format!("{}\n{}", stdout_str, stderr_str);
            
            debug_log_to_file(format!("[Prisma Generator] prisma generate failed. Error log:\n{}", error_log));
            
            if schema_path.exists() {
                if let Ok(schema_content) = std::fs::read_to_string(&schema_path) {
                    emit_progress("install-deps", &format!("Prisma generation failed. Self-healing schema (attempt {}/3)...", attempt), 78.2 + (attempt as f64 * 0.2));
                    
                    // Call local LLM to fix schema.prisma
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(60))
                        .build()
                        .unwrap_or_else(|_| reqwest::Client::new());
                    let system_prompt = "You are an expert database administrator and Prisma specialist. The prisma generate command failed with syntax or validation errors. Review the schema.prisma content and the error log. Correct the schema.prisma file to fix the validation errors. Respond ONLY with the complete corrected schema.prisma contents. Do not include any explanations, introduction, or markdown code blocks.";
                    
                    let user_prompt = format!(
                        "Prisma CLI error log:\n{}\n\nCurrent schema.prisma content:\n{}\n\nPlease provide the complete corrected schema.prisma code.",
                        error_log, schema_content
                    );
                    
                    let ollama_req = serde_json::json!({
                        "model": model_id.clone(),
                        "messages": [
                            { "role": "system", "content": system_prompt },
                            { "role": "user", "content": user_prompt }
                        ],
                        "stream": false,
                        "options": {
                            "temperature": 0.2
                        }
                    });
                    
                    let req_fut = client.post("http://127.0.0.1:11434/api/chat")
                        .json(&ollama_req)
                        .send();
                        
                    if let Ok(resp) = req_fut.await {
                        if resp.status().is_success() {
                            if let Ok(resp_json) = resp.json::<Value>().await {
                                if let Some(content) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                                    let cleaned = extract_code_content(content);
                                    if !cleaned.trim().is_empty() {
                                        let _ = std::fs::write(&schema_path, &cleaned);
                                        debug_log_to_file("[Prisma Generator] Wrote corrected schema.prisma to disk.".to_string());
                                        continue; // retry prisma generate
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // If we get here and didn't continue, break the loop
            break;
        }

        // Run prisma db push to synchronize the database schema
        emit_progress("install-deps", "Syncing database schema...", 79.0);
        #[cfg(target_os = "windows")]
        let mut push_cmd = std::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        push_cmd.arg("/C").arg("npx").arg("-y").arg("prisma").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
        
        #[cfg(not(target_os = "windows"))]
        let mut push_cmd = std::process::Command::new("npx");
        #[cfg(not(target_os = "windows"))]
        push_cmd.arg("-y").arg("prisma").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
        
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            push_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        
        if let Ok(mut child) = push_cmd.spawn() {
            let _ = child.wait();
        }
    }

    // Run self-healing compiler loop to automatically correct any compilation or syntax errors
    perform_self_healing_loop(&proj_dir, &model_id, &files_to_generate).await;

    // Generate Axiom Map (axiom-map.json)
    if let Err(e) = generate_axiom_map(&proj_dir) {
        debug_log_to_file(format!("[Axiom Map] Greška pri kreiranju axiom-map.json: {}", e));
    }

    // Generate project dependency graph metadata
    generate_project_metadata(&proj_dir, &project_id, &files_to_generate);
    generate_axiom_features_json(&proj_dir, name, &files_to_generate);

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
    let projects_dir = get_projects_dir();
    let _ = std::fs::create_dir_all(&projects_dir);
    let log_path = projects_dir.join("deeplink_debug.txt");
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
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            debug_log_to_file(format!("Single instance callback triggered! argv: {:?}", argv));
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            for arg in argv {
                let cleaned = arg.trim_matches('"').trim_matches('\'').to_string();
                debug_log_to_file(format!("Single instance checking arg: {}", cleaned));
                if cleaned.starts_with("axiom://") {
                    handle_axiom_url(app, &cleaned);
                }
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
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
            get_local_tools_profile,
            get_axiom_settings,
            save_axiom_settings,
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
            project_heal_compile_error,
            project_install_package,
            project_run_prisma_generate,
            task_start_generation,
            ollama_unload_model,
            get_pending_deep_link,
            clear_pending_deep_link,
            debug_log_to_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_and_rewrite_import() {
        let db = vec![
            PackageMeta {
                name: "resend".to_string(),
                version: "^3.2.0".to_string(),
                aliases: vec!["@resend/client".to_string(), "@resend/base".to_string(), "resend-node".to_string(), "resend/client".to_string()],
                import_examples: vec!["import { Resend } from 'resend';".to_string()],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
            PackageMeta {
                name: "next-auth".to_string(),
                version: "^4.24.7".to_string(),
                aliases: vec!["@next-auth/client".to_string(), "next-auth/client".to_string()],
                import_examples: vec![
                    "import { getServerSession } from 'next-auth';".to_string(),
                    "import NextAuth from 'next-auth';".to_string(),
                    "import { useSession } from 'next-auth/react';".to_string(),
                ],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
            PackageMeta {
                name: "@hookform/resolvers".to_string(),
                version: "^3.4.2".to_string(),
                aliases: vec![],
                import_examples: vec!["import { zodResolver } from '@hookform/resolvers/zod';".to_string()],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
        ];

        // 1. Should not rewrite relative imports
        assert_eq!(resolve_and_rewrite_import("./components/Header", &db), None);
        assert_eq!(resolve_and_rewrite_import("../hooks/useAuth", &db), None);
        assert_eq!(resolve_and_rewrite_import("@/lib/prisma", &db), None);

        // 2. Should rewrite exact aliases
        assert_eq!(resolve_and_rewrite_import("@resend/client", &db), Some("resend".to_string()));
        assert_eq!(resolve_and_rewrite_import("resend-node", &db), Some("resend".to_string()));

        // 3. Should rewrite next-auth client aliases specifically to next-auth/react
        assert_eq!(resolve_and_rewrite_import("@next-auth/client", &db), Some("next-auth/react".to_string()));
        assert_eq!(resolve_and_rewrite_import("next-auth/client", &db), Some("next-auth/react".to_string()));

        // 4. Should rewrite invalid subpaths of package name
        assert_eq!(resolve_and_rewrite_import("resend/client", &db), Some("resend".to_string()));
        assert_eq!(resolve_and_rewrite_import("resend/something/else", &db), Some("resend".to_string()));

        // 5. Should NOT rewrite valid subpaths of package name
        assert_eq!(resolve_and_rewrite_import("@hookform/resolvers/zod", &db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/react", &db), None);
    }

    #[test]
    fn test_post_process_use_client() {
        let content = "import { useState } from 'react';\nexport default function Counter() { const [c, setC] = useState(0); return <div />; }";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/app/components/Counter.tsx", content, &metamanifest, false);
        assert!(processed.starts_with("\"use client\";"));

        let content_with = "\"use client\";\nimport { useState } from 'react';";
        let processed_with = post_process_generated_file("src/app/components/Counter.tsx", content_with, &metamanifest, false);
        assert_eq!(processed_with, content_with.to_string());
        
        let mui_content = "import { styled } from '@mui/material/styles';\nconst MyDiv = styled('div')({});";
        let processed_mui = post_process_generated_file("src/app/layout.tsx", mui_content, &metamanifest, false);
        assert!(processed_mui.starts_with("\"use client\";"));
    }

    #[test]
    fn test_post_process_use_session_layout() {
        let content = "import { useSession } from 'next-auth/react';\nexport default function RootLayout({ children }) { const { data } = useSession(); return <html><body>{children}</body></html>; }";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/app/layout.tsx", content, &metamanifest, false);
        assert!(processed.contains("function RootLayoutInner"));
        assert!(processed.contains("export default function RootLayout"));
        assert!(processed.contains("<SessionProvider>"));
    }

    #[test]
    fn test_analyze_npm_install_failure() {
        // Test ETARGET (version mismatch)
        let etarget_log = "npm error code ETARGET\nnpm error notarget No matching version found for tw-elements-react@^4.0.0.\nnpm error notarget In most cases you or one of your dependencies...";
        let res1 = analyze_npm_install_failure(etarget_log);
        assert!(matches!(res1, Some(NpmFailureAction::SetWildcard(ref p)) if p == "tw-elements-react"));

        // Test E404 (not in registry)
        let e404_log = "npm error code E404\nnpm error 404 'some-bad-package' is not in the npm registry.";
        let res2 = analyze_npm_install_failure(e404_log);
        assert!(matches!(res2, Some(NpmFailureAction::Remove(ref p)) if p == "some-bad-package"));

        // Test E404 (legacy/requested resource)
        let legacy_e404_log = "npm ERR! 404 Not Found\nnpm ERR! 404  requested resource 'another-bad-package@^1.0.0'";
        let res3 = analyze_npm_install_failure(legacy_e404_log);
        assert!(matches!(res3, Some(NpmFailureAction::Remove(ref p)) if p == "another-bad-package"));

        // Test E404 (GET request style)
        let get_e404_log = "npm ERR! 404 Not Found - GET https://registry.npmjs.org/third-bad-package - Not found";
        let res4 = analyze_npm_install_failure(get_e404_log);
        assert!(matches!(res4, Some(NpmFailureAction::Remove(ref p)) if p == "third-bad-package"));
    }

    #[test]
    fn test_post_process_prisma_missing_url() {
        let content = "datasource db {\n  provider = \"postgresql\"\n}\n\nmodel User {\n  id            String     @id\n  userId        String\n  user          User       @relation(fields: [userId], references: [User.id])\n}";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("prisma/schema.prisma", content, &metamanifest, false);
        assert!(processed.contains("url      = env(\"DATABASE_URL\")"));
        assert!(processed.contains("references: [id]"));
        assert!(processed.contains("userId        String\n"));
    }

    #[test]
    fn test_post_process_prisma_default_export() {
        let content = "import { PrismaClient } from '@prisma/client';\nexport const prisma = new PrismaClient();";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/lib/prisma.ts", content, &metamanifest, false);
        assert!(processed.contains("export default prisma;"));
    }

    #[test]
    fn test_get_generation_phase() {
        assert_eq!(get_generation_phase("package.json", "nextjs"), GenerationPhase::Config);
        assert_eq!(get_generation_phase("tsconfig.json", "nextjs"), GenerationPhase::Config);
        assert_eq!(get_generation_phase("schema.prisma", "nextjs"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("src/app/api/auth/[...nextauth]/route.ts", "nextjs"), GenerationPhase::ApiOrState);
        assert_eq!(get_generation_phase("src/hooks/useAuth.ts", "nextjs"), GenerationPhase::LogicOrHooks);
        assert_eq!(get_generation_phase("src/app/layout.tsx", "nextjs"), GenerationPhase::ShellOrWidgets);
        assert_eq!(get_generation_phase("src/app/dashboard/page.tsx", "nextjs"), GenerationPhase::PagesOrUi);

        assert_eq!(get_generation_phase("main.js", "electron"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("preload.js", "electron"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("src/components/Counter.tsx", "electron"), GenerationPhase::PagesOrUi);
    }

    #[test]
    fn test_generate_axiom_map() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        let src_hooks = temp_dir.join("src/hooks");
        std::fs::create_dir_all(&src_hooks).unwrap();
        
        let hook_content = r#"
import axios from 'axios';
export function useAuth() {
    return "auth";
}
"#;
        std::fs::write(src_hooks.join("useAuth.ts"), hook_content).unwrap();

        let comp_dir = temp_dir.join("src/components");
        std::fs::create_dir_all(&comp_dir).unwrap();
        
        let comp_content = r#"
import { useAuth } from '@/hooks/useAuth';
export default function Dashboard() {
    const auth = useAuth();
    return "dashboard";
}
export const version = "1.0.0";
"#;
        std::fs::write(comp_dir.join("Dashboard.tsx"), comp_content).unwrap();

        let map_res = generate_axiom_map(&temp_dir);
        assert!(map_res.is_ok());

        let map_path = temp_dir.join("axiom-map.json");
        assert!(map_path.exists());

        let map_content = std::fs::read_to_string(map_path).unwrap();
        let map_val: Value = serde_json::from_str(&map_content).unwrap();

        assert!(map_val.get("files").unwrap().get("src/hooks/useAuth.ts").is_some());
        assert!(map_val.get("files").unwrap().get("src/components/Dashboard.tsx").is_some());

        let dash_entry = map_val.get("files").unwrap().get("src/components/Dashboard.tsx").unwrap();
        assert!(dash_entry.get("exports").unwrap().as_array().unwrap().contains(&serde_json::json!("default")));
        assert!(dash_entry.get("exports").unwrap().as_array().unwrap().contains(&serde_json::json!("version")));
        assert!(dash_entry.get("dependencies").unwrap().as_array().unwrap().contains(&serde_json::json!("src/hooks/useAuth.ts")));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1B[31mx\x1B[0m You're importing a component that needs \u{1b}[36museState\u{1b}[0m.";
        let cleaned = strip_ansi_codes(input);
        assert_eq!(cleaned, "x You're importing a component that needs useState.");
    }

    #[test]
    fn test_extract_file_path_from_line() {
        let input1 = " ⨯ ./src/hooks/useAuth.ts";
        assert_eq!(extract_file_path_from_line(input1), Some("src/hooks/useAuth.ts".to_string()));

        let input2 = "Error: C:\\projects\\app\\src\\components\\Button.tsx:10:15";
        assert_eq!(extract_file_path_from_line(input2), Some("C:\\projects\\app\\src\\components\\Button.tsx".to_string()));

        let input3 = "   ,-[C:\\projects\\app\\src\\hooks\\useAuth.ts:1:1]";
        assert_eq!(extract_file_path_from_line(input3), Some("C:\\projects\\app\\src\\hooks\\useAuth.ts".to_string()));

        let input4 = "  at Home (./src/app/page.tsx:15:98)";
        assert_eq!(extract_file_path_from_line(input4), Some("src/app/page.tsx".to_string()));

        let input5 = "  at Module._compile (C:\\projects\\node_modules\\next\\dist\\server\\require.js:5:10)";
        assert_eq!(extract_file_path_from_line(input5), None);

        let input6 = "  at eval (./.next/server/app/products/page.js:151:1)";
        assert_eq!(extract_file_path_from_line(input6), None);
    }

    #[test]
    fn test_extract_package_name_from_error() {
        let input1 = "Error: Cannot find module 'decimal.js'";
        assert_eq!(extract_package_name_from_error(input1), Some("decimal.js".to_string()));

        let input2 = "Module not found: Can't resolve '@emotion/react' in 'C:\\projects\\...'";
        assert_eq!(extract_package_name_from_error(input2), Some("@emotion/react".to_string()));

        let input3 = "Error: Cannot find module './extensions'";
        assert_eq!(extract_package_name_from_error(input3), None);

        let input4 = "Error: Cannot find module '../utils/helpers'";
        assert_eq!(extract_package_name_from_error(input4), None);

        let input5 = "Error: Cannot find module 'C:\\projects\\app\\extensions'";
        assert_eq!(extract_package_name_from_error(input5), None);

        let input6 = "Error: Cannot find module 'C:/projects/app/extensions'";
        assert_eq!(extract_package_name_from_error(input6), None);
    }

    #[test]
    fn test_inject_axiom_attrs_generics() {
        let input = "const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => { return <form>Hello</form>; };";
        let output = inject_axiom_attrs(input, "src/app/register/page.tsx");
        assert!(!output.contains("<HTMLFormElement data-axiom-component"));
        assert!(output.contains("<form data-axiom-component=\"Page\" data-axiom-file=\"src/app/register/page.tsx\">"));
        
        let input2 = "const [error, setError] = useState<string | null>(null); return <div>Error</div>;";
        let output2 = inject_axiom_attrs(input2, "src/app/register/page.tsx");
        assert!(!output2.contains("<string data-axiom-component"));
        assert!(output2.contains("<div data-axiom-component=\"Page\" data-axiom-file=\"src/app/register/page.tsx\">"));
    }
}

