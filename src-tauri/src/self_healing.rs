use crate::*;

// ==================== ORCHESTRATOR API ====================

pub fn detect_infinite_loop(content: &str) -> bool {
    // 1. Check character repetition (e.g. } } } } } } } } } } } } } } })
    // Ignore common banner, divider, and comment formatting characters that frequently occur in large blocks
    let divider_chars = ['=', '-', '*', '/', '#', '_', ' ', '.', '\t', '~', ':'];
    let trimmed_chars: Vec<char> = content.trim_end().chars().rev().take(30).collect();
    if trimmed_chars.len() >= 25 {
        let first = trimmed_chars[0];
        if !first.is_alphanumeric() && !divider_chars.contains(&first) && trimmed_chars.iter().all(|&c| c == first) {
            return true;
        }
    }

    // 2. Check line block repetition (e.g. repeating a block of 1-5 lines 5 times)
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let n = lines.len();
    for block_size in 1..=5 {
        if n >= block_size * 5 {
            let mut is_loop = true;
            for i in 0..block_size {
                let idx_curr = n - block_size + i;
                let idx_prev1 = n - 2 * block_size + i;
                let idx_prev2 = n - 3 * block_size + i;
                let idx_prev3 = n - 4 * block_size + i;
                let idx_prev4 = n - 5 * block_size + i;
                if lines[idx_curr] != lines[idx_prev1]
                    || lines[idx_curr] != lines[idx_prev2]
                    || lines[idx_curr] != lines[idx_prev3]
                    || lines[idx_curr] != lines[idx_prev4]
                {
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

    // 3. Check word repetition on the last line (require at least 8 repetitions to avoid false positives on CSS shorthands)
    if let Some(last_line) = content.lines().last() {
        let words: Vec<&str> = last_line.split_whitespace().collect();
        let nw = words.len();
        for w_size in 1..=4 {
            if nw >= w_size * 8 {
                let mut w_loop = true;
                for i in 0..w_size {
                    let w_curr = words[nw - w_size + i];
                    for repeat_idx in 1..8 {
                        if w_curr != words[nw - (repeat_idx + 1) * w_size + i] {
                            w_loop = false;
                            break;
                        }
                    }
                    if !w_loop {
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

pub fn get_safe_fallback_content(file_path: &str, _lang: &str, desc: &str) -> String {
    let lower = file_path.to_lowercase();
    if lower.ends_with(".css") {
        format!("/* {}\n * {}\n */\n\n.container {{\n  display: flex;\n  flex-direction: column;\n}}\n", file_path, desc)
    } else if lower.ends_with(".json") {
        "{\n}\n".to_string()
    } else if lower.ends_with(".tsx") || lower.ends_with(".jsx") {
        let comp_name = std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Component");
        format!("import React from 'react';\n\nexport default function {}() {{\n  return (\n    <div className=\"p-4\">\n      <h1 className=\"text-xl font-bold\">{}</h1>\n    </div>\n  );\n}}\n", comp_name, comp_name)
    } else if lower.ends_with(".ts") || lower.ends_with(".js") {
        format!("// {}\n// {}\n\nexport {{}};\n", file_path, desc)
    } else {
        format!("// {}\n", desc)
    }
}

pub fn index_modules(dir: &std::path::Path, src_root: &std::path::Path, index: &mut std::collections::HashMap<String, Vec<String>>) {
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

pub fn scan_and_fix_imports(dir: &std::path::Path, src_root: &std::path::Path, index: &std::collections::HashMap<String, Vec<String>>) {
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
                                        target_abs.clone(),
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

pub struct MissingImport {
    target_path: std::path::PathBuf,
    importing_file_path: std::path::PathBuf,
    import_line: String,
}

pub fn collect_missing_imports(
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
                                        target_abs.clone(),
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
                "options": build_ollama_options(model, 0.1, Some(4096))
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

pub fn ensure_theme_file_exists(proj_dir: &std::path::Path) {
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

pub fn repair_cargo_toml_dependencies(proj_dir: &std::path::Path) {
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

pub fn repair_pubspec_yaml_dependencies(proj_dir: &std::path::Path) {
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

pub async fn generate_and_save_healing_rule(
    file_path: &str,
    original_content: &str,
    repaired_content: &str,
    model: &str,
) {
    debug_log_to_file(format!("[Self-Healing] Analyzing changes in {} to extract global healing rules...", file_path));

    let client = reqwest::Client::new();
    let system_prompt = "You are a Senior Project Architect. Analyze the original file content (with compilation errors) and the corrected content. Identify ONLY atomic, reusable import, export, or TypeScript type changes that were made to fix the compilation error. DO NOT generate rules for UI components, JSX blocks, HTML tags, or component return statements. Each rule must be a precise single-line or small multi-line import/export/type fix. Respond ONLY with a valid JSON array of objects, each with 'search' and 'replace'. Do not include any explanations, introduction, or markdown block formatting. Respond only with clean JSON.";
    
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
        "options": build_ollama_options(model, 0.1, Some(1024))
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
                                        // Strictly reject any UI/JSX blocks or oversized multi-line replacements
                                        let contains_jsx = search_str.contains('<') || search_str.contains("</") || search_str.contains("return (")
                                            || replace_str.contains('<') || replace_str.contains("</") || replace_str.contains("return (");
                                        let s_lines = search_str.lines().count();
                                        let r_lines = replace_str.lines().count();
                                        let is_too_large = s_lines > 4 || r_lines > 4 || search_str.len() > 300 || replace_str.len() > 300;

                                        // Only learn atomic, safe import/export/type rules
                                        if !contains_jsx && !is_too_large && search_str.trim().len() >= 6 && search_str != replace_str {
                                            let exists = existing_arr.iter().any(|r| r.get("search").and_then(|s| s.as_str()) == Some(search_str));
                                            if !exists {
                                                existing_arr.push(serde_json::json!({
                                                    "search": search_str,
                                                    "replace": replace_str,
                                                    "createdAt": chrono::Utc::now().to_rfc3339()
                                                }));
                                                changed = true;
                                                debug_log_to_file(format!("[Self-Healing] Learned new atomic global rule: Replace \"{}\" with \"{}\"", search_str.trim(), replace_str.trim()));
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

pub async fn perform_self_healing_loop(proj_dir: &std::path::Path, model: &str, files_list: &[Value]) {
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

        // Extract targeted project category and stack from live_builder_guide.json if available
        let mut category_stack_info = "Next.js App Router + Tailwind".to_string();
        let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
        let mut file_specs_map = std::collections::HashMap::new();
        if lbg_path.exists() {
            if let Ok(lbg_content) = std::fs::read_to_string(&lbg_path) {
                if let Ok(lbg_json) = serde_json::from_str::<Value>(&lbg_content) {
                    let cat = lbg_json.get("projectSpecification").and_then(|s| s.get("category")).and_then(|c| c.as_str()).unwrap_or("web-app");
                    let stk = lbg_json.get("projectSpecification").and_then(|s| s.get("stack")).and_then(|c| c.as_str()).unwrap_or("Next.js");
                    category_stack_info = format!("Stack: {}, Category: {}", stk, cat);
                    
                    if let Some(req_routes) = lbg_json.get("projectSpecification").and_then(|s| s.get("categoryRequirements")).and_then(|r| r.get("requiredRoutes")).and_then(|r| r.as_array()) {
                        for rr in req_routes {
                            if let Some(rr_str) = rr.as_str() {
                                for err_file in &files_with_errors {
                                    if let Some(err_path) = err_file.get("path").and_then(|p| p.as_str()) {
                                        if rr_str.contains(err_path) {
                                            file_specs_map.insert(err_path.to_string(), rr_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback to files_list for file descriptions if not found in LBG
        for err_file in &files_with_errors {
            if let Some(err_path) = err_file.get("path").and_then(|p| p.as_str()) {
                if !file_specs_map.contains_key(err_path) {
                    if let Some(desc) = files_list.iter()
                        .find(|f| f.get("path").and_then(|p| p.as_str()) == Some(err_path))
                        .and_then(|f| f.get("description").and_then(|d| d.as_str())) {
                        file_specs_map.insert(err_path.to_string(), format!("{}: {}", err_path, desc));
                    }
                }
            }
        }

        let mut targeted_specs = String::new();
        for (_, f_desc) in &file_specs_map {
            targeted_specs.push_str(&format!("- Target File Role: {}\n", f_desc));
        }

        // Truncate build log if too long
        let truncated_log = if build_output.len() > 3000 {
            format!("...[truncated]...\n{}", &build_output[build_output.len() - 3000..])
        } else {
            build_output.clone()
        };

        let mut system_prompt = format!(
            "You are a Senior Next.js Developer fixing compilation errors.\n\
             Project Architecture: {}\n{}\n\
             CRITICAL HEALING RULES:\n\
             1. STRICT UI & DESIGN PRESERVATION: You are fixing COMPILATION errors. NEVER delete or simplify UI components, hero sections, product grids, forms, tables, or buttons to bypass an error. The original visual richness, structure, and functional purpose of the page MUST remain 100% intact.\n\
             2. MANDATORY TAILWIND CSS STYLING: All JSX elements MUST continue to use comprehensive Tailwind CSS utility classes. NEVER degrade styled elements into raw, unstyled HTML.\n\
             3. SURGICAL REPAIR: Fix the underlying issue (e.g. invalid import path, missing type declaration, mismatched props, async/await syntax, or client-side hook rule) in place.\n\
             4. CLIENT COMPONENT DIRECTIVE: If a file uses React hooks (useState, useEffect, useSession, useCart) or interactive handlers (onClick, onSubmit), the very first line MUST be \"use client\";.\n\
             5. TYPE CASTING FALLBACK: If an external property or interface is missing, declare it locally or cast safely (e.g. `(session?.user as any)?.id`) instead of deleting the feature.\n\
             Respond ONLY with a valid JSON array of objects, where each object has 'path' (the relative file path) and 'content' (the complete corrected file contents). Do not include explanations or markdown blocks.",
            category_stack_info,
            targeted_specs
        );
        system_prompt.push_str(&get_packages_db_summary());

        for err_file in &files_with_errors {
            if let (Some(path_str), Some(content_str)) = (err_file.get("path").and_then(|p| p.as_str()), err_file.get("content").and_then(|c| c.as_str())) {
                let rp = get_stack_rulepacks(path_str, content_str, Some(&truncated_log));
                if !rp.is_empty() {
                    system_prompt.push_str(&rp);
                    break;
                }
            }
        }
        
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
            "options": build_ollama_options(model, 0.2, Some(4096))
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
                                // Whole-File Code Fallback for single file error
                                if files_with_errors.len() == 1 {
                                    if let Some(single_path) = files_with_errors[0].get("path").and_then(|p| p.as_str()) {
                                        let has_code = clean_content.contains("import ") || clean_content.contains("export ") || clean_content.contains("function ") || clean_content.contains("const ");
                                        if has_code {
                                            debug_log_to_file(format!("[Self-Healing] Activating Whole-File Fallback for single errored file: {}", single_path));
                                            let full_path = proj_dir.join(single_path);
                                            let processed = post_process_generated_file(single_path, &clean_content, &serde_json::json!({}), has_tailwind);
                                            let _ = std::fs::write(&full_path, &processed);
                                        }
                                    }
                                }
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

pub async fn setup_local_postgres(_proj_dir: &std::path::Path, db_name: &str) -> Result<String, String> {
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

pub fn update_env_database_url(proj_dir: &std::path::Path, db_url: &str) {
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

pub fn fallback_to_sqlite(proj_dir: &std::path::Path) -> Result<(), String> {
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

