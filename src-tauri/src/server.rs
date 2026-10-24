use crate::*;

// ==================== SERVER API ====================
use tauri::State;
use tokio::process::Command;

pub struct ServerState(pub Mutex<HashMap<String, tokio::process::Child>>);

pub fn kill_process_on_port(port: u16) {
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

pub fn strip_ansi_codes(input: &str) -> String {
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

pub fn extract_file_path_from_line(line: &str) -> Option<String> {
    // Next.js route-based runtime errors, e.g. 'Error: The default export is not a React Component in page: "/"'
    let route_extracted = if let Some(start) = line.find("in page: \"") {
        let sub = &line[start + "in page: \"".len()..];
        sub.find('\"').map(|end| sub[..end].to_string())
    } else if let Some(start) = line.find("in page: '") {
        let sub = &line[start + "in page: '".len()..];
        sub.find('\'').map(|end| sub[..end].to_string())
    } else {
        None
    };

    if let Some(route) = route_extracted {
        let clean_route = route.trim_matches('/');
        if clean_route.is_empty() {
            return Some("src/app/page.tsx".to_string());
        } else if clean_route.starts_with("api/") || clean_route == "api" {
            // In Next.js App Router, API routes are located in route.ts/route.js, NOT page.tsx
            return Some(format!("src/app/{}/route.ts", clean_route));
        } else {
            return Some(format!("src/app/{}/page.tsx", clean_route));
        }
    }

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
                
                // Handle webpack internal paths commonly found in Next.js stack traces
                if let Some(idx) = path_str.find("webpack-internal:///") {
                    path_str = path_str[idx + "webpack-internal:///".len()..].to_string();
                }
                
                // Strip common webpack wrapper prefixes like (rsc)/, (app-pages)/, etc.
                if let Some(idx) = path_str.find("./") {
                    path_str = path_str[idx + 2..].to_string();
                } else if path_str.starts_with("./") {
                    path_str = path_str[idx + 2..].to_string();
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

pub fn extract_package_name_from_error(line: &str) -> Option<String> {
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

pub fn resolve_project_file_candidate(proj_dir: &Path, rel_path: &str) -> Option<String> {
    if proj_dir.join(rel_path).is_file() {
        return Some(rel_path.to_string());
    }

    // Try without src/ if path starts with src/
    if let Some(stripped) = rel_path.strip_prefix("src/") {
        if proj_dir.join(stripped).is_file() {
            return Some(stripped.to_string());
        }
    }

    // Try with src/ if path doesn't start with src/
    let with_src = format!("src/{}", rel_path);
    if proj_dir.join(&with_src).is_file() {
        return Some(with_src);
    }

    // Try alternative file extensions (.tsx, .jsx, .ts, .js)
    for ext in &[".tsx", ".jsx", ".ts", ".js"] {
        if let Some(dot_idx) = rel_path.rfind('.') {
            let base = &rel_path[..dot_idx];
            let candidate = format!("{}{}", base, ext);
            if proj_dir.join(&candidate).is_file() {
                return Some(candidate);
            }
            if let Some(stripped) = candidate.strip_prefix("src/") {
                if proj_dir.join(stripped).is_file() {
                    return Some(stripped.to_string());
                }
            }
            let with_src = format!("src/{}", candidate);
            if proj_dir.join(&with_src).is_file() {
                return Some(with_src);
            }
        }
    }

    // If it's src/app/api/.../route.ts, also check Pages router candidates: src/pages/api/....ts
    if rel_path.contains("app/api/") && rel_path.ends_with("/route.ts") {
        let api_name = rel_path
            .trim_start_matches("src/")
            .trim_start_matches("app/api/")
            .trim_end_matches("/route.ts");
        for ext in &[".ts", ".js", ".tsx", ".jsx"] {
            let pages_candidate = format!("src/pages/api/{}{}", api_name, ext);
            if proj_dir.join(&pages_candidate).is_file() {
                return Some(pages_candidate);
            }
            let pages_no_src = format!("pages/api/{}{}", api_name, ext);
            if proj_dir.join(&pages_no_src).is_file() {
                return Some(pages_no_src);
            }
        }
    }

    // If it's src/app/.../page.tsx, also check Pages router candidates: src/pages/....tsx
    if rel_path.contains("app/") && rel_path.ends_with("/page.tsx") {
        let page_name = rel_path
            .trim_start_matches("src/")
            .trim_start_matches("app/")
            .trim_end_matches("/page.tsx");
        for ext in &[".tsx", ".jsx", ".js", ".ts"] {
            let pages_candidate = if page_name.is_empty() {
                format!("src/pages/index{}", ext)
            } else {
                format!("src/pages/{}{}", page_name, ext)
            };
            if proj_dir.join(&pages_candidate).is_file() {
                return Some(pages_candidate);
            }
            let pages_no_src = if page_name.is_empty() {
                format!("pages/index{}", ext)
            } else {
                format!("pages/{}{}", page_name, ext)
            };
            if proj_dir.join(&pages_no_src).is_file() {
                return Some(pages_no_src);
            }
        }
    }

    None
}

pub struct CompileErrorDetector {
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

        let has_error_marker = clean_line.contains('⨯') 
            || clean_line.contains("Failed to compile") 
            || clean_line.contains("Error:")
            || clean_line.contains("Attempted import error:")
            || (clean_line.contains('⚠') && (clean_line.contains(".tsx") || clean_line.contains(".ts") || clean_line.contains(".jsx") || clean_line.contains(".js")));

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
        if let Some(ref raw_path) = self.file_path {
            let proj_dir = get_project_path(project_id);
            let resolved_path = resolve_project_file_candidate(&proj_dir, raw_path)
                .unwrap_or_else(|| raw_path.clone());
            let full_path = proj_dir.join(&resolved_path);
            if full_path.exists() && full_path.is_file() {
                let error_message = self.error_lines.join("\n");
                let _ = window.emit("server:compile-error", serde_json::json!({
                    "projectId": project_id,
                    "filePath": resolved_path,
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



pub fn ensure_project_files_sanitized(proj_dir: &std::path::Path) {
    fn walk_and_fix(dir: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules" && name != ".next" && name != "build" && name != "dist" && name != ".git" {
                        walk_and_fix(&path);
                    }
                } else if path.is_file() {
                    let path_str = path.to_string_lossy();
                    if path_str.ends_with(".tsx") || path_str.ends_with(".jsx") || path_str.ends_with(".ts") || path_str.ends_with(".js") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let mut fixed = content.clone();
                            if fixed.contains("from 'react-icons'") || fixed.contains("from \"react-icons\"") {
                                fixed = sanitize_react_icons_imports(&fixed);
                            }
                            if fixed.contains("useSession") && fixed.contains("setLoading") {
                                fixed = sanitize_unhandled_session_loading(&fixed);
                            }
                            if fixed.contains("useQuery") || fixed.contains("useMutation") {
                                fixed = sanitize_tanstack_query_v5(&fixed);
                            }
                            if fixed.contains("<Link") || fixed.contains("import { Link } from") {
                                fixed = sanitize_next_link_imports(&fixed);
                            }
                            if fixed != content {
                                let _ = std::fs::write(&path, fixed);
                            }
                        }
                    }
                }
            }
        }
    }

    walk_and_fix(proj_dir);
}

pub fn ensure_react_icons_sanitized(proj_dir: &std::path::Path) {
    ensure_project_files_sanitized(proj_dir);
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
pub async fn server_start(
    options: ServerStartOptions,
    window: tauri::Window,
    state: State<'_, ServerState>,
) -> Result<bool, String> {
    // Clean up any zombie processes on port 3000 and 3001
    kill_process_on_port(3000);
    kill_process_on_port(3001);

    let mut proj_dir = get_project_path(&options.project_id);
    
    // Fallback: If there is no package.json in the root but there is one in the src directory, use the src directory as the root.
    if !proj_dir.join("package.json").exists() && proj_dir.join("src").join("package.json").exists() {
        let _ = window.emit("server:log", serde_json::json!({
            "text": "> package.json found in src/ directory. Using src/ as project root.\n",
            "type": "info"
        }));
        proj_dir = proj_dir.join("src");
    }
    
    // Ensure Axiom Live Visual Inspector is present in layout files for visual editing mode
    ensure_axiom_inspector_injected(&proj_dir);

    // Ensure tsconfig.json is valid and not corrupted
    ensure_valid_tsconfig(&proj_dir);

    // Ensure package.json has all required Next.js, Prisma, and Tailwind dependencies and scripts
    repair_package_json_dependencies(&proj_dir);

    // Auto-resolve route collisions between Pages Router and App Router
    if proj_dir.join("src/app/api").exists() && proj_dir.join("src/pages/api").exists() {
        let _ = std::fs::remove_dir_all(proj_dir.join("src/pages/api"));
    }

    // Pre-flight: Auto-sanitize project files (react-icons, unhandled session loading, TanStack v5, Link imports)
    ensure_project_files_sanitized(&proj_dir);

    // Pre-flight: Normalize any App Router route links across the project
    let project_routes = get_project_routes_from_dir(&proj_dir);
    let routes_list: Vec<String> = project_routes.into_iter().map(|(r, _)| r).collect();
    if !routes_list.is_empty() {
        normalize_project_route_links(&proj_dir, &routes_list);
    }

    
    // Auto-install dependencies if node_modules is missing or incomplete
    let node_modules_dir = proj_dir.join("node_modules");
    let mut needs_install = !node_modules_dir.exists();
    
    if !needs_install {
        if let Ok(pkg_content) = std::fs::read_to_string(proj_dir.join("package.json")) {
            if let Ok(pkg_json) = serde_json::from_str::<Value>(&pkg_content) {
                for section in ["dependencies", "devDependencies"] {
                    if let Some(deps) = pkg_json.get(section).and_then(|d| d.as_object()) {
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
                    if needs_install {
                        break;
                    }
                }
            }
        }
    }
    
    let enriched_path = ensure_node_toolchain_ready(Some(&window), "server:log").await;

    if needs_install {
        run_npm_install_with_auto_healing(&proj_dir, &window, "server:log", None).await;
    }
    
    let settings = get_axiom_settings();
    let model_id = settings.get("modelId")
        .and_then(|m| m.as_str())
        .unwrap_or("qwen2.5-coder:7b")
        .to_string();

    // Auto-run, validate and heal prisma generate if Prisma schema exists in the project
    if let Err(e) = heal_prisma_schema_loop(&proj_dir, &window, &model_id, "server:log").await {
        let _ = window.emit("server:log", serde_json::json!({
            "text": format!("> [Prisma Critical Error] Pokretanje servera otkazano: {}\n", e),
            "type": "error"
        }));
        return Err(e);
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
            .env("PATH", &enriched_path)
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
            .env("PATH", &enriched_path)
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
pub async fn server_stop(
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
pub async fn server_status(project_id: String, state: State<'_, ServerState>) -> Result<bool, String> {
    let servers = state.0.lock().unwrap();
    Ok(servers.contains_key(&project_id))
}

