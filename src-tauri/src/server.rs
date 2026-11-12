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

/// Maps Next.js browser chunk URLs/paths (e.g. `http://localhost:3000/_next/static/chunks/app/layout.js`
/// or `/_next/static/chunks/app/dashboard/page-a1b2c3d4.js`) back to workspace source paths.
pub fn map_next_chunk_url_to_source(raw: &str) -> Option<String> {
    let norm = raw.trim().replace('\\', "/");
    let no_query = norm.split(['?', '#']).next().unwrap_or(&norm);
    let idx = no_query.find("/_next/static/chunks/")?;
    let after = &no_query[idx + "/_next/static/chunks/".len()..];
    let (router_kind, rest) = if let Some(r) = after.strip_prefix("app/") {
        ("app", r)
    } else if let Some(r) = after.strip_prefix("pages/") {
        ("pages", r)
    } else {
        return None;
    };

    let stem_with_ext = rest
        .strip_suffix(".mjs")
        .or_else(|| rest.strip_suffix(".js"))?;
    let mut stem = stem_with_ext.to_string();
    if let Some(dash_idx) = stem.rfind('-') {
        let suffix = &stem[dash_idx + 1..];
        if suffix.len() >= 8 && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            stem.truncate(dash_idx);
        }
    }
    let trimmed_stem = stem.trim_matches('/');
    if trimmed_stem.is_empty()
        || matches!(
            trimmed_stem,
            "_app" | "_document" | "_error" | "_not-found" | "webpack" | "main" | "framework" | "polyfills"
        )
    {
        return None;
    }

    if router_kind == "app" {
        Some(format!("src/app/{}.tsx", trimmed_stem))
    } else {
        Some(format!("src/pages/{}.tsx", trimmed_stem))
    }
}

pub fn extract_file_path_from_line(line: &str) -> Option<String> {
    if let Some(mapped_chunk) = map_next_chunk_url_to_source(line) {
        return Some(mapped_chunk);
    }

    // Next.js route-based runtime and build/prerender errors, e.g.:
    // - 'Error: The default export is not a React Component in page: "/"'
    // - 'Error occurred prerendering page "/models". Read more: ...'
    let route_extracted = [
        ("in page: \"", "\""),
        ("in page: '", "'"),
        ("prerendering page \"", "\""),
        ("prerendering page '", "'"),
        ("not a React Component in \"", "\""),
        ("not a React Component in '", "'"),
        ("page: \"", "\""),
        ("page: '", "'"),
    ]
    .iter()
    .find_map(|(prefix, end_delim)| {
        let start = line.find(prefix)?;
        let sub = &line[start + prefix.len()..];
        let end = sub.find(end_delim)?;
        let candidate = sub[..end].trim();
        if matches!(*prefix, "page: \"" | "page: '") && !candidate.starts_with('/') {
            return None;
        }
        Some(candidate.to_string())
    });

    if let Some(route) = route_extracted {
        let route_no_query = route.split(['?', '#']).next().unwrap_or(&route);
        let clean_route = route_no_query.trim_matches('/');
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
        let word_clean = word.trim_matches(|c| c == '[' || c == ']' || c == ',' || c == '`' || c == '(' || c == ')' || c == '"' || c == '\'');
        if let Some(mapped_chunk) = map_next_chunk_url_to_source(word_clean) {
            return Some(mapped_chunk);
        }
        let word_low = word_clean.to_lowercase();
        if word_low.starts_with("http://") || word_low.starts_with("https://") {
            continue;
        }
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
                    path_str = path_str[2..].to_string();
                }

                // Map Next.js typed routes (`.next/types/app/.../*.ts`) back to App Router source files
                let norm_for_next = path_str.replace('\\', "/");
                if let Some(types_idx) = norm_for_next.find(".next/types/") {
                    let sub = norm_for_next[types_idx + ".next/types/".len()..].trim_start_matches('/');
                    if sub.starts_with("app/") {
                        let mapped = if sub.contains("/api/") && (sub.ends_with("/route.ts") || sub.ends_with("/route.js")) {
                            format!("src/{}", sub)
                        } else if let Some(stem_path) = sub.strip_suffix(".ts") {
                            let file_name = stem_path.rsplit('/').next().unwrap_or("");
                            if matches!(
                                file_name,
                                "page" | "layout" | "loading" | "error" | "not-found" | "template" | "default" | "global-error"
                            ) {
                                format!("src/{}.tsx", stem_path)
                            } else {
                                format!("src/{}", sub)
                            }
                        } else {
                            format!("src/{}", sub)
                        };
                        return Some(mapped);
                    }
                }

                // Preskoči biblioteke treće strane, bild izlaz (osim već mapiranih `.next/types/`), keš, HTTP URL-ove i git foldere
                let norm_low = norm_for_next.to_lowercase();
                if norm_low.starts_with("http://")
                    || norm_low.starts_with("https://")
                    || norm_low.contains("node_modules/")
                    || norm_low.ends_with("/node_modules")
                    || norm_low.contains("_next/")
                    || norm_low.contains(".next/")
                    || norm_low.starts_with(".next")
                    || norm_low.contains("/dist/")
                    || norm_low.starts_with("dist/")
                    || norm_low.contains("/build/")
                    || norm_low.starts_with("build/")
                    || norm_low.contains(".git/")
                    || norm_low.starts_with(".git")
                {
                    continue;
                }
                return Some(path_str);
            }
        }
    }

    // Fallback: extensionless source paths emitted in TypeScript/Next build errors (e.g. `typeof import(".../src/app/models/page")`)
    for token in cleaned.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '(' | ')' | '`' | ',' | '<' | '>')) {
        let norm = token.replace('\\', "/");
        let norm_trim = norm.trim_matches(|c: char| matches!(c, '[' | ']' | ':' | ';'));
        if norm_trim.is_empty() {
            continue;
        }
        let low = norm_trim.to_lowercase();
        if low.starts_with("http://")
            || low.starts_with("https://")
            || low.contains("node_modules/")
            || low.contains(".next/")
            || low.contains(".git/")
        {
            continue;
        }
        if let Some(src_idx) = low.find("/src/") {
            let candidate = norm_trim[src_idx + 1..].trim_end_matches('/');
            if candidate.matches('/').count() >= 1 {
                return Some(candidate.to_string());
            }
        } else if low.starts_with("src/") && norm_trim.matches('/').count() >= 1 {
            return Some(norm_trim.trim_end_matches('/').to_string());
        } else if low.starts_with("./src/") && norm_trim.matches('/').count() >= 2 {
            return Some(norm_trim[2..].trim_end_matches('/').to_string());
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
        // Filtriraj lokalne relativne, apsolutne, TS alias i gole lokalne putanje (npr. 'src/models/...')
        if is_local_import_specifier(trimmed) || trimmed.contains(' ') {
            return None;
        }
        let local_aliases = ["@components", "@hooks", "@utils", "@lib", "@styles", "@services", "@context", "@pages", "@app", "@assets", "@config"];
        for la in &local_aliases {
            if trimmed == *la || trimmed.starts_with(&format!("{}/", la)) {
                return None;
            }
        }
        // Svedi subpath import (npr. 'next-auth/providers/credentials' ili '@mui/material/Button') na koren npm paketa
        let parts: Vec<&str> = trimmed.split('/').collect();
        let root_pkg = if trimmed.starts_with('@') && parts.len() >= 2 {
            format!("{}/{}", parts[0], parts[1])
        } else {
            parts.first().copied().unwrap_or(trimmed).to_string()
        };
        let node_builtins = [
            "fs", "path", "http", "https", "os", "crypto", "stream", "util", "events", "assert",
            "dns", "net", "tls", "querystring", "url", "zlib", "buffer", "child_process",
            "cluster", "dgram", "process", "readline", "vm",
        ];
        if root_pkg.starts_with("node:")
            || node_builtins.contains(&root_pkg.as_str())
            || root_pkg == "react"
            || root_pkg == "react-dom"
            || root_pkg == "next"
            || root_pkg.is_empty()
        {
            return None;
        }
        return Some(root_pkg);
    }
    None
}

pub fn resolve_project_file_candidate(proj_dir: &Path, rel_path: &str) -> Option<String> {
    if let Some(mapped_chunk) = map_next_chunk_url_to_source(rel_path) {
        if mapped_chunk != rel_path {
            return resolve_project_file_candidate(proj_dir, &mapped_chunk);
        }
    }
    let mut norm_rel = rel_path.replace('\\', "/");
    let norm_low = norm_rel.trim().to_lowercase();
    if norm_low.starts_with("http://")
        || norm_low.starts_with("https://")
        || norm_low.starts_with("webpack-internal:")
        || norm_low.contains("node_modules/")
        || norm_low.ends_with("/node_modules")
    {
        return None;
    }
    let proj_dir_norm = proj_dir.to_string_lossy().replace('\\', "/");
    if let Some(stripped) = norm_rel.strip_prefix(&format!("{}/", proj_dir_norm)) {
        norm_rel = stripped.to_string();
    } else if let Some(src_idx) = norm_rel.rfind("/src/") {
        norm_rel = norm_rel[src_idx + 1..].to_string();
    } else if let Some(prisma_idx) = norm_rel.rfind("/prisma/") {
        norm_rel = norm_rel[prisma_idx + 1..].to_string();
    }
    let rel_path = norm_rel.as_str();

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

    // Try file extensions (.tsx, .jsx, .ts, .js) when replacing extension OR resolving extensionless specifier (e.g. `src/app/models/page` or `[...nextauth]/route`)
    let last_slash = rel_path.rfind('/');
    let ext_dot_idx = rel_path.rfind('.').filter(|&dot_idx| last_slash.map_or(true, |sl| dot_idx > sl));
    let base_no_ext = match ext_dot_idx {
        Some(dot_idx) => &rel_path[..dot_idx],
        None => rel_path,
    };
    for ext in &[".tsx", ".jsx", ".ts", ".js"] {
        let candidate = format!("{}{}", base_no_ext, ext);
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
        if ext_dot_idx.is_none() {
            let index_cand = format!("{}/index{}", base_no_ext, ext);
            if proj_dir.join(&index_cand).is_file() {
                return Some(index_cand);
            }
            if let Some(stripped) = index_cand.strip_prefix("src/") {
                if proj_dir.join(stripped).is_file() {
                    return Some(stripped.to_string());
                }
            }
            let with_src_idx = format!("src/{}", index_cand);
            if proj_dir.join(&with_src_idx).is_file() {
                return Some(with_src_idx);
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
        let clean_line = clean_cli_output_for_llm(line);

        if clean_line.contains("Cannot find module") || clean_line.contains("Module not found") || clean_line.contains("Can't resolve") {
            if let Some(pkg_name) = extract_package_name_from_error(&clean_line) {
                let _ = window.emit("server:missing-package", serde_json::json!({
                    "projectId": project_id,
                    "packageName": pkg_name
                }));
            }
            // Do NOT return early here — preserve Module not found / Can't resolve context in `error_lines` for Popravi sa AI & Katastar!
        }

        let trimmed = clean_line.trim();
        let is_boundary = trimmed.starts_with("GET /")
            || trimmed.starts_with("POST /")
            || trimmed.starts_with("PUT /")
            || trimmed.starts_with("DELETE /")
            || trimmed.contains("✓ Compiled")
            || trimmed.contains("○ Compiling")
            || trimmed.contains("✓ Ready in");

        if is_boundary {
            if self.is_recording {
                self.emit_current(window, project_id);
            }
            return;
        }

        let has_error_marker = clean_line.contains('⨯') 
            || clean_line.contains("Failed to compile") 
            || clean_line.contains("Module not found")
            || clean_line.contains("Cannot find module")
            || clean_line.contains("Error:")
            || clean_line.contains("ReferenceError:")
            || clean_line.contains("TypeError:")
            || clean_line.contains("SyntaxError:")
            || clean_line.contains("Unhandled Runtime Error")
            || clean_line.contains("Attempted import error:")
            || (clean_line.contains('⚠') && (clean_line.contains(".tsx") || clean_line.contains(".ts") || clean_line.contains(".jsx") || clean_line.contains(".js")));

        if has_error_marker {
            let line_file = extract_file_path_from_line(&clean_line);
            let is_same_error_continuation = self.is_recording
                && self.lines_since_start <= 4
                && (line_file.is_none() || self.file_path.is_none() || line_file == self.file_path);

            if is_same_error_continuation {
                self.error_lines.push(clean_line);
                self.lines_since_start += 1;
                if self.file_path.is_none() {
                    self.file_path = line_file;
                }
                if self.file_path.is_some() {
                    self.emit_snapshot(window, project_id);
                }
            } else {
                if self.is_recording && self.file_path.is_some() {
                    self.emit_current(window, project_id);
                }
                self.is_recording = true;
                self.file_path = line_file;
                self.error_lines = vec![clean_line];
                self.lines_since_start = 0;
                
                if self.file_path.is_some() {
                    self.emit_snapshot(window, project_id);
                }
            }
        } else if self.is_recording {
            self.error_lines.push(clean_line.clone());
            self.lines_since_start += 1;

            if self.file_path.is_none() {
                self.file_path = extract_file_path_from_line(&clean_line);
            }

            if self.file_path.is_some() {
                self.emit_snapshot(window, project_id);
                if self.lines_since_start >= 16 {
                    self.reset();
                }
            } else if self.lines_since_start >= 12 {
                self.reset();
            }
        }
    }

    fn emit_snapshot(&self, window: &tauri::Window, project_id: &str) {
        if let Some(ref raw_path) = self.file_path {
            let proj_dir = get_project_path(project_id);
            let resolved_path = resolve_project_file_candidate(&proj_dir, raw_path)
                .unwrap_or_else(|| raw_path.replace('\\', "/"));
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
    }

    fn emit_current(&mut self, window: &tauri::Window, project_id: &str) {
        if let Some(ref raw_path) = self.file_path {
            let proj_dir = get_project_path(project_id);
            let resolved_path = resolve_project_file_candidate(&proj_dir, raw_path)
                .unwrap_or_else(|| raw_path.replace('\\', "/"));
            let error_message = self.error_lines.join(" | ");
            debug_log_to_file(format!(
                "[Dev Server CompileError] project={} file={} details={}",
                project_id, resolved_path, error_message
            ));
        }
        self.emit_snapshot(window, project_id);
        self.reset();
    }

    fn reset(&mut self) {
        self.is_recording = false;
        self.file_path = None;
        self.error_lines.clear();
        self.lines_since_start = 0;
    }
}

pub struct LogThrottler {
    last_line: String,
    repeat_count: usize,
    window_start: std::time::Instant,
    lines_in_window: usize,
    storm_reported: bool,
}

impl LogThrottler {
    pub fn new() -> Self {
        Self {
            last_line: String::new(),
            repeat_count: 0,
            window_start: std::time::Instant::now(),
            lines_in_window: 0,
            storm_reported: false,
        }
    }

    pub fn filter_line(&mut self, text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        let now = std::time::Instant::now();
        if now.duration_since(self.window_start).as_millis() >= 1000 {
            self.window_start = now;
            self.lines_in_window = 0;
            self.storm_reported = false;
        }

        self.lines_in_window += 1;

        // If more than 60 lines arrive within 1 second, suppress the deluge
        if self.lines_in_window > 60 {
            if !self.storm_reported {
                self.storm_reported = true;
                return Some("... [Prekomeran broj logova sa servera u sekundi - privremeno sažeto] ...".to_string());
            }
            return None;
        }

        // Duplicate suppression
        if trimmed == self.last_line {
            self.repeat_count += 1;
            if self.repeat_count == 2 {
                return Some(format!("{} (ponavlja se...)", trimmed));
            } else if self.repeat_count % 30 == 0 {
                return Some(format!("... [Linija ponovljena {} puta]: {}", self.repeat_count, trimmed));
            }
            return None;
        }

        // Line changed
        let prefix = if self.repeat_count > 2 && (self.repeat_count % 30 != 0) {
            format!("... [Prethodna poruka ponovljena ukupno {} puta]\n", self.repeat_count)
        } else {
            String::new()
        };

        self.last_line = trimmed.to_string();
        self.repeat_count = 1;

        Some(format!("{}{}", prefix, text))
    }
}



pub fn ensure_react_icons_sanitized(proj_dir: &std::path::Path) {
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
                            if content.contains("react-icons") || content.contains("-icons-react") {
                                let fixed = sanitize_react_icons_imports(&content);
                                if fixed != content {
                                    let _ = std::fs::write(&path, fixed);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    walk_and_fix(proj_dir);
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

    // Ensure NextAuth projects have NEXTAUTH_URL & NEXTAUTH_SECRET configured before server boot
    crate::self_healing::ensure_nextauth_env_vars(&proj_dir);

    // Auto-resolve route collisions between Pages Router and App Router
    if proj_dir.join("src/app/api").exists() && proj_dir.join("src/pages/api").exists() {
        let _ = std::fs::remove_dir_all(proj_dir.join("src/pages/api"));
    }

    // Pre-flight: Auto-sanitize any bare 'react-icons' imports into specific subpaths (e.g. react-icons/hi)
    ensure_react_icons_sanitized(&proj_dir);

    // Pre-flight: Ensure barrel index.ts files exist in src/hooks, src/types, src/utils, src/config
    ensure_directory_barrel_files(&proj_dir);

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
        let install_outcome = run_npm_install_with_auto_healing(&proj_dir, &window, "server:log", None).await;
        if install_outcome == NpmInstallOutcome::OfflinePaused {
            let err_msg = "Nema internet konekcije ili je npm registar nedostupan. Nastavljamo sa instalacijom paketa kada budemo ponovo online.".to_string();
            let _ = window.emit("server:log", serde_json::json!({
                "text": format!("> [Offline Pause] Pokretanje servera pauzirano: {}\n", err_msg),
                "type": "error"
            }));
            return Err(err_msg);
        }
        if install_outcome == NpmInstallOutcome::Failed {
            let pkg_path = proj_dir.join("package.json");
            let mut missing_runtime_pkgs: Vec<String> = Vec::new();
            if !node_modules_dir.exists() {
                missing_runtime_pkgs.push("node_modules".to_string());
            } else if let Ok(pkg_content) = std::fs::read_to_string(&pkg_path) {
                if let Ok(pkg_json) = serde_json::from_str::<Value>(&pkg_content) {
                    if let Some(deps) = pkg_json.get("dependencies").and_then(|d| d.as_object()) {
                        for dep_name in deps.keys() {
                            if !node_modules_dir.join(dep_name).exists() {
                                missing_runtime_pkgs.push(dep_name.clone());
                            }
                        }
                    }
                }
            }

            if !missing_runtime_pkgs.is_empty() {
                let err_msg = format!(
                    "Automatska instalacija npm zavisnosti nije uspela. Nedostaju kritični paketi: {}",
                    missing_runtime_pkgs.join(", ")
                );
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [NPM Critical Error] Pokretanje servera otkazano: {}\n", err_msg),
                    "type": "error"
                }));
                return Err(err_msg);
            }
        }
    }
    
    let settings = get_axiom_settings();
    let model_id = settings.get("modelId")
        .and_then(|m| m.as_str())
        .unwrap_or("qwen2.5-coder:7b")
        .to_string();

    // Auto-run, validate and heal prisma generate if Prisma schema exists in the project
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        if let Err(e) = heal_prisma_schema_loop(&proj_dir, &window, &model_id, "server:log").await {
            let _ = window.emit("server:log", serde_json::json!({
                "text": format!("> [Prisma Critical Error] Pokretanje servera otkazano: {}\n", e),
                "type": "error"
            }));
            return Err(e);
        }

        // Obavezan prisma db push pre starta dev servera
        let _ = window.emit("server:log", serde_json::json!({
            "text": "> [Prisma Sync] Sinhronizujem bazu podataka sa semom (db push)...\n",
            "type": "info"
        }));
        let local_prisma_js = proj_dir.join("node_modules").join("prisma").join("build").join("index.js");
        let mut push_cmd = if local_prisma_js.exists() {
            let mut c = tokio::process::Command::new("node");
            c.arg(&local_prisma_js).arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            c
        } else {
            #[cfg(target_os = "windows")]
            let mut c = tokio::process::Command::new("cmd");
            #[cfg(target_os = "windows")]
            c.arg("/C").arg("npx").arg("-y").arg("prisma@5.22.0").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            #[cfg(not(target_os = "windows"))]
            let mut c = tokio::process::Command::new("npx");
            #[cfg(not(target_os = "windows"))]
            c.arg("-y").arg("prisma@5.22.0").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            c
        };
        #[cfg(target_os = "windows")]
        {
            push_cmd.creation_flags(0x08000000);
        }
        if let Ok(po) = push_cmd.output().await {
            if po.status.success() {
                let _ = window.emit("server:log", serde_json::json!({
                    "text": "> [Prisma Sync] Baza podataka uspesno sinhronizovana!\n",
                    "type": "info"
                }));
            } else {
                let err_text = crate::healer::clean_cli_output_for_llm(&String::from_utf8_lossy(&po.stderr));
                let first_err = err_text.lines().find(|l| !l.trim().is_empty()).unwrap_or("db push upozorenje");
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [Prisma Sync Warning] {}\n", first_err),
                    "type": "warning"
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
            let shared_detector = std::sync::Arc::new(std::sync::Mutex::new(CompileErrorDetector::new()));
            
            // Spawn task to read stdout and emit logs
            let window_stdout = window.clone();
            let p_id_stdout = options.project_id.clone();
            let detector_stdout = shared_detector.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stdout).lines();
                let mut throttler_stdout = LogThrottler::new();
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Ok(mut det) = detector_stdout.lock() {
                        det.process_line(&line, &window_stdout, &p_id_stdout);
                    }
                    let clean_text = clean_cli_output_for_llm(&line);
                    if let Some(filtered) = throttler_stdout.filter_line(&clean_text) {
                        let is_important = filtered.contains('⨯')
                            || filtered.contains("Error")
                            || filtered.contains("Failed to compile")
                            || filtered.contains("Ready in")
                            || filtered.contains("Local:")
                            || filtered.contains(" 500 in ")
                            || filtered.contains("Cannot find module");
                        if is_important {
                            debug_log_to_file(format!("[Dev Server stdout] {}", filtered.trim()));
                        }
                        let _ = window_stdout.emit("server:log", serde_json::json!({
                            "text": format!("{}\n", filtered),
                            "type": "info"
                        }));
                    }
                }
                if let Ok(mut det) = detector_stdout.lock() {
                    if det.is_recording {
                        det.emit_current(&window_stdout, &p_id_stdout);
                    }
                }
            });

            // Spawn task to read stderr and emit logs
            let window_stderr = window.clone();
            let p_id_stderr = options.project_id.clone();
            let detector_stderr = shared_detector.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stderr).lines();
                let mut throttler_stderr = LogThrottler::new();
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Ok(mut det) = detector_stderr.lock() {
                        det.process_line(&line, &window_stderr, &p_id_stderr);
                    }
                    let clean_text = clean_cli_output_for_llm(&line);
                    if let Some(filtered) = throttler_stderr.filter_line(&clean_text) {
                        debug_log_to_file(format!("[Dev Server stderr] {}", filtered.trim()));
                        let _ = window_stderr.emit("server:log", serde_json::json!({
                            "text": format!("{}\n", filtered),
                            "type": "error"
                        }));
                    }
                }
                if let Ok(mut det) = detector_stderr.lock() {
                    if det.is_recording {
                        det.emit_current(&window_stderr, &p_id_stderr);
                    }
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
    }

    // Always aggressively liberate port 3000 to prevent any lingering/zombie processes
    kill_process_on_port(3000);

    let _ = window.emit("server:status", serde_json::json!({
        "status": "stopped"
    }));
    debug_log_to_file(format!("[Server] Development server stopped for project: {}", project_id));
    Ok(true)
}

#[tauri::command]
pub async fn server_status(project_id: String, state: State<'_, ServerState>) -> Result<bool, String> {
    let servers = state.0.lock().unwrap();
    Ok(servers.contains_key(&project_id))
}

