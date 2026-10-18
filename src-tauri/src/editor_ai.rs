use crate::*;

pub fn generate_axiom_features_json(proj_dir: &std::path::Path, project_name: &str, files_list: &[Value]) {
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

pub fn extract_code_content(raw: &str) -> String {
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

pub async fn resolve_valid_ollama_model(client: &reqwest::Client, requested: &str) -> String {
    if let Ok(tags_resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
        if let Ok(tags_json) = tags_resp.json::<Value>().await {
            if let Some(models_arr) = tags_json.get("models").and_then(|m| m.as_array()) {
                let installed_names: Vec<String> = models_arr
                    .iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect();

                // 1. If exact or prefix match exists, use it
                for name in &installed_names {
                    if name == requested || name.starts_with(requested) || requested.starts_with(name) {
                        return name.clone();
                    }
                }

                // 2. If requested is not found, fallback to best installed model
                if !installed_names.is_empty() {
                    if let Some(coder) = installed_names.iter().find(|n| n.contains("coder") || n.contains("qwen")) {
                        return coder.clone();
                    }
                    return installed_names[0].clone();
                }
            }
        }
    }
    requested.to_string()
}

pub fn apply_targeted_edit(content: &str, target: &str, replacement: &str) -> Result<String, String> {
    let target_trimmed = target.trim();
    if target_trimmed.is_empty() {
        return Err("Target block is empty".to_string());
    }

    // 1. Try exact match
    let exact_matches: Vec<_> = content.match_indices(target).collect();
    if exact_matches.len() == 1 {
        return Ok(content.replace(target, replacement));
    } else if exact_matches.len() > 1 {
        return Err(format!("Target block is duplicated ({}) in file. It must be unique.", exact_matches.len()));
    }

    // 2. Try trimmed match
    let trimmed_matches: Vec<_> = content.match_indices(target_trimmed).collect();
    if trimmed_matches.len() == 1 {
        return Ok(content.replace(target_trimmed, replacement.trim()));
    } else if trimmed_matches.len() > 1 {
        return Err(format!("Trimmed target block is duplicated ({}) in file.", trimmed_matches.len()));
    }

    // 3. Whitespace-collapsed / normalized search
    // Extract non-whitespace characters from target and match against content character positions
    let target_chars: Vec<char> = target.chars().filter(|c| !c.is_whitespace()).collect();
    if !target_chars.is_empty() {
        let content_chars: Vec<(usize, char)> = content.char_indices().collect();
        let n = content_chars.len();
        let m = target_chars.len();
        let mut matches_found = Vec::new();

        for i in 0..n {
            if content_chars[i].1.is_whitespace() {
                continue;
            }
            let mut target_idx = 0;
            let mut j = i;
            while j < n && target_idx < m {
                let ch = content_chars[j].1;
                if ch.is_whitespace() {
                    j += 1;
                    continue;
                }
                if ch == target_chars[target_idx] {
                    target_idx += 1;
                    j += 1;
                } else {
                    break;
                }
            }
            if target_idx == m && j > 0 {
                let start_byte = content_chars[i].0;
                let end_byte = content_chars[j - 1].0 + content_chars[j - 1].1.len_utf8();
                matches_found.push((start_byte, end_byte));
            }
        }

        if matches_found.len() == 1 {
            let (start, end) = matches_found[0];
            let mut result = String::with_capacity(content.len() + replacement.len());
            result.push_str(&content[..start]);
            result.push_str(replacement);
            result.push_str(&content[end..]);
            return Ok(result);
        } else if matches_found.len() > 1 {
            return Err(format!("Normalized target matches multiple ({}) places in file.", matches_found.len()));
        }
    }

    // 4. Line-trimmed sliding window match
    let target_lines: Vec<&str> = target.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if !target_lines.is_empty() {
        let content_lines: Vec<&str> = content.lines().collect();
        let mut line_matches = Vec::new();

        for i in 0..=content_lines.len().saturating_sub(target_lines.len()) {
            let mut matched = true;
            for (k, t_line) in target_lines.iter().enumerate() {
                if content_lines[i + k].trim() != *t_line {
                    matched = false;
                    break;
                }
            }
            if matched {
                line_matches.push(i);
            }
        }

        if line_matches.len() == 1 {
            let match_line_idx = line_matches[0];
            let lines_before: String = content_lines[..match_line_idx].join("\n");
            let prefix = if lines_before.is_empty() { String::new() } else { format!("{}\n", lines_before) };
            let lines_after: String = content_lines[match_line_idx + target_lines.len()..].join("\n");
            let suffix = if lines_after.is_empty() { String::new() } else { format!("\n{}", lines_after) };
            return Ok(format!("{}{}{}", prefix, replacement, suffix));
        }
    }

    Err(format!("Target code block not found in file: {:?}", target))
}

#[tauri::command]
pub async fn editor_ai_edit(
    file_path: String,
    content: String,
    prompt: String,
    project_id: String,
    model_id: Option<String>,
    visual_context: Option<Value>,
) -> Result<AIEditResult, String> {
    let client = reqwest::Client::new();
    let initial_model = model_id.unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    let model = resolve_valid_ollama_model(&client, &initial_model).await;
    
    let mut system_prompt = "You are an expert developer assistant helping the user edit a source code file. \
        Instead of rewriting the entire file, you must identify the exact lines that need to change and provide a list of target replacements in JSON format. \
        \n\n\
        CRITICAL RULES:\n\
        1. Respond ONLY with a valid JSON object matching the format described below. Do not write any explanations, introductory text, or markdown code blocks (do not wrap in ```json). Just the raw JSON.\n\
        2. In the JSON structure, under 'edits', each 'target' block must exactly match (character-for-character, including leading whitespace and indentation) a unique substring in the original file.\n\
        3. Do NOT include placeholders in 'replacement'. The code must be complete and ready to run.\n\
        4. Focus only on resolving the user's request. Do not make unrelated changes.\n\
        \n\n\
        JSON FORMAT:\n\
        {\n\
          \"explanation\": \"Brief explanation of the changes made\",\n\
          \"edits\": [\n\
            {\n\
              \"target\": \"exact string of lines in original file to be replaced\",\n\
              \"replacement\": \"the new code to put in its place\"\n\
            }\n\
          ]\n\
        }".to_string();

    let rulepacks = get_stack_rulepacks(&file_path, &content, None);
    if !rulepacks.is_empty() {
        system_prompt.push_str(&rulepacks);
    }
    
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
    let proj_dir = get_project_path(&project_id);
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
                        debug_log_to_file(format!("[AI Edit] Raw AI response:\n{}", msg_content));
                        let clean_content = extract_code_content(msg_content);
                        
                        let mut parsed_ok = false;
                        let mut final_content = content.clone();
                        let mut fail_reason = String::new();
                        
                        let is_json_response = clean_content.trim().starts_with('{') 
                            && (clean_content.contains("\"edits\"") || clean_content.contains("\"explanation\""));

                        if let Ok(parsed_json) = serde_json::from_str::<Value>(&clean_content) {
                            if let Some(edits) = parsed_json.get("edits").and_then(|e| e.as_array()) {
                                if !edits.is_empty() {
                                    let mut temp_content = content.clone();
                                    let mut apply_failed = false;
                                    
                                    for edit in edits {
                                        let target = edit.get("target").and_then(|t| t.as_str()).unwrap_or("");
                                        let replacement = edit.get("replacement").and_then(|r| r.as_str()).unwrap_or("");
                                        
                                        match apply_targeted_edit(&temp_content, target, replacement) {
                                            Ok(modified) => {
                                                temp_content = modified;
                                            }
                                            Err(err) => {
                                                apply_failed = true;
                                                fail_reason = err;
                                                break;
                                            }
                                        }
                                    }
                                    
                                    if !apply_failed {
                                        final_content = temp_content;
                                        parsed_ok = true;
                                    } else {
                                        debug_log_to_file(format!("[AI Edit] Targeted edit apply failed: {}", fail_reason));
                                    }
                                }
                            }
                        }
                        
                        if !parsed_ok {
                            if is_json_response {
                                // CRITICAL SAFETY GUARD: Never dump raw JSON edit structures onto a source code file!
                                debug_log_to_file(format!("[AI Edit] Model returned JSON edits but target code could not be matched: {}", fail_reason));
                                return Ok(AIEditResult {
                                    success: false,
                                    new_content: None,
                                    error: Some(format!("AI model generated targeted edit JSON, but the target code was not matched in the file: {}", fail_reason)),
                                });
                            } else {
                                debug_log_to_file("[AI Edit] Falling back to treating model output as full file content.".to_string());
                                final_content = clean_content;
                            }
                        }
                        
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
                let status_code = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                let clean_msg = if let Ok(err_json) = serde_json::from_str::<Value>(&err_body) {
                    err_json.get("error").and_then(|e| e.as_str()).unwrap_or(&err_body).to_string()
                } else {
                    err_body
                };
                Ok(AIEditResult {
                    success: false,
                    new_content: None,
                    error: Some(format!("Ollama Error ({}): {}", status_code, clean_msg)),
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
pub async fn editor_ai_project_edit(
    window: tauri::Window,
    project_id: String,
    prompt: String,
    model_id: Option<String>,
) -> Result<ProjectEditResult, String> {
    let client = reqwest::Client::new();
    let initial_model = model_id.unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    let model = resolve_valid_ollama_model(&client, &initial_model).await;
    let proj_dir = get_project_path(&project_id);
    
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

