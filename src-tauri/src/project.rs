use crate::*;

// ==================== PROJECT API ====================
use std::fs;
use std::path::{Path, PathBuf};
use serde_json::Value;

pub fn get_base_dir() -> PathBuf {
    // Use standard OS data directory instead of hardcoded ~/.axiom-forge
    let local_data = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    local_data.join("axiom-forge")
}

pub fn get_projects_file() -> PathBuf {
    get_base_dir().join("projects.json")
}

#[tauri::command]
pub fn get_axiom_settings() -> Value {
    let file_path = get_base_dir().join("axiom-settings.json");
    if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    }
}

#[tauri::command]
pub fn save_axiom_settings(settings: Value) -> Result<(), String> {
    let base_dir = get_base_dir();
    let _ = fs::create_dir_all(&base_dir);
    let file_path = base_dir.join("axiom-settings.json");
    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&file_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_projects_dir() -> PathBuf {
    get_base_dir().join("projects")
}

pub fn get_project_path(project_id: &str) -> PathBuf {
    let file_path = get_projects_file();
    if file_path.exists() {
        if let Ok(content) = fs::read_to_string(&file_path) {
            if let Ok(projects) = serde_json::from_str::<Vec<Value>>(&content) {
                if let Some(proj) = projects.iter().find(|p| p.get("id").and_then(|i| i.as_str()) == Some(project_id)) {
                    if let Some(local_path) = proj.get("localPath").and_then(|lp| lp.as_str()) {
                        let path = PathBuf::from(local_path);
                        if path.exists() {
                            return path;
                        }
                    }
                }
            }
        }
    }
    get_projects_dir().join(project_id)
}

#[tauri::command]
pub fn project_get_all() -> Result<Vec<Value>, String> {
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
pub fn project_save(mut project_data: Value) -> ProjectSaveResult {
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
pub fn project_delete(id: String) -> Result<bool, String> {
    let file_path = get_projects_file();
    if !file_path.exists() { return Ok(true); }

    let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "[]".to_string());
    let mut projects: Vec<Value> = serde_json::from_str(&content).unwrap_or_else(|_| vec![]);
    
    projects.retain(|p| p.get("id").and_then(|i| i.as_str()) != Some(&id));
    
    let _ = fs::write(&file_path, serde_json::to_string_pretty(&projects).unwrap());
    
    // Also delete the folder if it exists and is internal
    let proj_dir = get_project_path(&id);
    let is_internal = proj_dir.starts_with(get_projects_dir());
    if proj_dir.exists() && is_internal {
        let _ = fs::remove_dir_all(proj_dir);
    }
    
    Ok(true)
}

#[tauri::command]
pub fn project_import_local_folder(path: String) -> Result<Value, String> {
    let folder_path = PathBuf::from(&path);
    if !folder_path.exists() || !folder_path.is_dir() {
        return Err("Selected path does not exist or is not a directory.".to_string());
    }

    // Try to determine project name
    let mut project_name = folder_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported-project".to_string());

    // If package.json exists, read the name from it
    let pkg_json_path = folder_path.join("package.json");
    if pkg_json_path.exists() {
        if let Ok(content) = fs::read_to_string(&pkg_json_path) {
            if let Ok(pkg_val) = serde_json::from_str::<Value>(&content) {
                if let Some(name) = pkg_val.get("name").and_then(|n| n.as_str()) {
                    project_name = name.to_string();
                }
            }
        }
    }

    let project_id = format!("imported-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
    
    let project_data = serde_json::json!({
        "id": project_id,
        "name": project_name,
        "status": "imported",
        "localPath": path.replace('\\', "/"),
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });

    let file_path = get_projects_file();
    let mut projects: Vec<Value> = if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| vec![])
    } else {
        vec![]
    };

    projects.push(project_data.clone());
    let _ = fs::write(&file_path, serde_json::to_string_pretty(&projects).unwrap());

    Ok(project_data)
}

#[tauri::command]
pub async fn project_import_github(
    repo_url: String,
    local_path: Option<String>,
    token: Option<String>,
) -> Result<Value, String> {
    let repo_name = repo_url.split('/')
        .last()
        .unwrap_or("cloned-repo")
        .trim_end_matches(".git")
        .to_string();

    let project_id = format!("github-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
    
    let target_dir = if let Some(ref path) = local_path {
        PathBuf::from(path)
    } else {
        get_projects_dir().join(&project_id)
    };

    let _ = fs::create_dir_all(target_dir.parent().unwrap());

    // Construct clone URL with token if provided
    let clone_url = if let Some(t) = token {
        if !t.trim().is_empty() {
            repo_url.replace("https://", &format!("https://{}@", t))
        } else {
            repo_url.clone()
        }
    } else {
        repo_url.clone()
    };

    // Run git clone command
    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("clone").arg(&clone_url).arg(target_dir.to_str().unwrap());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd.output().await.map_err(|e| format!("Failed to run git clone: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git clone failed: {}", stderr));
    }

    let project_data = serde_json::json!({
        "id": project_id,
        "name": repo_name,
        "status": "imported",
        "localPath": target_dir.to_str().unwrap().replace('\\', "/"),
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });

    let file_path = get_projects_file();
    let mut projects: Vec<Value> = if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| vec![])
    } else {
        vec![]
    };

    projects.push(project_data.clone());
    let _ = fs::write(&file_path, serde_json::to_string_pretty(&projects).unwrap());

    Ok(project_data)
}

#[tauri::command]
pub fn project_detect_missing_env_keys(project_id: String) -> Result<Vec<String>, String> {
    use std::collections::HashSet;
    use regex::Regex;

    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }

    let mut required_keys = HashSet::new();

    // 1. Try to read .env.example
    let env_example_path = proj_dir.join(".env.example");
    if env_example_path.exists() {
        if let Ok(content) = fs::read_to_string(&env_example_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    if let Some(pos) = trimmed.find('=') {
                        let key = trimmed[..pos].trim().to_string();
                        if !key.is_empty() {
                            required_keys.insert(key);
                        }
                    }
                }
            }
        }
    } else {
        // Fallback: scan source files with regex
        let env_regex = Regex::new(r"process\.env\.([A-Z0-9_]+)").unwrap();
        
        fn scan_for_env(dir: &Path, re: &Regex, keys: &mut HashSet<String>) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        let path = entry.path();
                        
                        if file_type.is_dir() {
                            if !name.starts_with('.') && name != "node_modules" && name != "dist" && name != ".next" {
                                scan_for_env(&path, re, keys);
                            }
                        } else {
                            let ext = path.extension().unwrap_or_default().to_string_lossy().to_string();
                            if ext == "js" || ext == "ts" || ext == "jsx" || ext == "tsx" {
                                if let Ok(content) = fs::read_to_string(&path) {
                                    for cap in re.captures_iter(&content) {
                                        if let Some(m) = cap.get(1) {
                                            let key = m.as_str().to_string();
                                            if key != "NODE_ENV" && key != "PORT" {
                                                keys.insert(key);
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
        scan_for_env(&proj_dir, &env_regex, &mut required_keys);
    }

    // 2. Read existing .env (or .env.local) to see what is already configured
    let mut configured_keys = HashSet::new();
    let env_files = vec![".env", ".env.local", ".env.development"];
    for file in env_files {
        let env_path = proj_dir.join(file);
        if env_path.exists() {
            if let Ok(content) = fs::read_to_string(&env_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        if let Some(pos) = trimmed.find('=') {
                            let key = trimmed[..pos].trim().to_string();
                            let val = trimmed[pos+1..].trim();
                            if !key.is_empty() && !val.is_empty() {
                                configured_keys.insert(key);
                            }
                        }
                    }
                }
            }
        }
    }

    // Find missing keys
    let mut missing_keys: Vec<String> = required_keys.into_iter()
        .filter(|k| !configured_keys.contains(k))
        .collect();
        
    missing_keys.sort();
    Ok(missing_keys)
}

#[tauri::command]
pub fn project_save_env_keys(project_id: String, keys: HashMap<String, String>) -> Result<(), String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }

    let env_path = if proj_dir.join(".env.local").exists() || !proj_dir.join(".env").exists() {
        proj_dir.join(".env.local")
    } else {
        proj_dir.join(".env")
    };

    let mut env_content = if env_path.exists() {
        fs::read_to_string(&env_path).unwrap_or_default()
    } else {
        "".to_string()
    };

    if !env_content.is_empty() && !env_content.ends_with('\n') {
        env_content.push('\n');
    }

    for (k, v) in keys {
        let mut key_replaced = false;
        let mut new_lines = Vec::new();
        
        for line in env_content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(&format!("{}=", k)) {
                new_lines.push(format!("{}={}", k, v));
                key_replaced = true;
            } else {
                new_lines.push(line.to_string());
            }
        }

        if key_replaced {
            env_content = new_lines.join("\n") + "\n";
        } else {
            env_content.push_str(&format!("{}={}\n", k, v));
        }
    }

    fs::write(&env_path, env_content).map_err(|e| format!("Failed to write env file: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn project_update_status(id: String, status: String, metadata: Value) -> Result<bool, String> {
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
pub fn project_get_files(project_id: String) -> GetFilesResult {
    let proj_dir = get_project_path(&project_id);
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
pub async fn project_commit(project_id: String, message: Option<String>) -> Result<Value, String> {
    let proj_dir = get_project_path(&project_id);
    
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
pub async fn project_push_to_github(project_id: String) -> Result<Value, String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }

    // 1. Get token from security key-ring
    let token_res = get_token("github-token".to_string());
    let token = token_res.value;

    // 2. Detect current branch
    let mut git_branch = tokio::process::Command::new("git");
    git_branch.arg("rev-parse").arg("--abbrev-ref").arg("HEAD").current_dir(&proj_dir);
    #[cfg(target_os = "windows")]
    git_branch.creation_flags(0x08000000);
    
    let branch_output = git_branch.output().await.map_err(|e| format!("Failed to detect git branch: {}", e))?;
    let branch_name = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
    if branch_name.is_empty() {
        return Err("Failed to detect active git branch. Make sure the project is initialized with git.".to_string());
    }

    // 3. Get current origin URL
    let mut git_url = tokio::process::Command::new("git");
    git_url.arg("remote").arg("get-url").arg("origin").current_dir(&proj_dir);
    #[cfg(target_os = "windows")]
    git_url.creation_flags(0x08000000);
    
    let url_output = git_url.output().await;
    let original_url = match url_output {
        Ok(out) => {
            if out.status.success() {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            } else {
                "".to_string()
            }
        }
        Err(_) => "".to_string(),
    };

    if original_url.is_empty() {
        return Err("No remote origin detected. Please configure a GitHub remote first.".to_string());
    }

    // 4. Temporarily set remote URL with token if token is available
    let has_token = token.is_some() && !token.as_ref().unwrap().trim().is_empty();
    if has_token {
        let t = token.as_ref().unwrap();
        if original_url.starts_with("https://github.com/") {
            let auth_url = original_url.replace("https://github.com/", &format!("https://{}@github.com/", t));
            let mut git_set_url = tokio::process::Command::new("git");
            git_set_url.arg("remote").arg("set-url").arg("origin").arg(&auth_url).current_dir(&proj_dir);
            #[cfg(target_os = "windows")]
            git_set_url.creation_flags(0x08000000);
            let _ = git_set_url.output().await;
        }
    }

    // 5. Run git push
    let mut git_push = tokio::process::Command::new("git");
    git_push.arg("push").arg("origin").arg(&branch_name).current_dir(&proj_dir);
    #[cfg(target_os = "windows")]
    git_push.creation_flags(0x08000000);

    let push_result = git_push.output().await;

    // 6. Restore original URL (clean token from config)
    if has_token && original_url.starts_with("https://github.com/") {
        let mut git_restore_url = tokio::process::Command::new("git");
        git_restore_url.arg("remote").arg("set-url").arg("origin").arg(&original_url).current_dir(&proj_dir);
        #[cfg(target_os = "windows")]
        git_restore_url.creation_flags(0x08000000);
        let _ = git_restore_url.output().await;
    }

    match push_result {
        Ok(out) => {
            if out.status.success() {
                Ok(serde_json::json!({ "success": true, "message": format!("Successfully pushed branch '{}' to GitHub", branch_name) }))
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                Err(format!("Git push failed: {}", stderr))
            }
        }
        Err(e) => Err(format!("Failed to run git push: {}", e)),
    }
}

#[derive(Serialize)]
pub struct FileReadResult {
    success: bool,
    content: String,
    error: Option<String>,
}

#[tauri::command]
pub fn project_read_file(project_id: String, file_path: String) -> FileReadResult {
    let proj_dir = get_project_path(&project_id);
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
pub fn editor_write_file(project_id: String, file_path: String, content: String) -> FileWriteResult {
    let proj_dir = get_project_path(&project_id);
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

