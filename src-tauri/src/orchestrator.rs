use crate::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

pub struct TaskControl {
    pub task_id: String,
    pub project_id: String,
    pub paused: Arc<AtomicBool>,
    pub stopped: Arc<AtomicBool>,
    pub notify: Arc<Notify>,
}

static ACTIVE_TASKS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Arc<TaskControl>>>> = std::sync::OnceLock::new();

fn get_active_tasks() -> &'static std::sync::Mutex<std::collections::HashMap<String, Arc<TaskControl>>> {
    ACTIVE_TASKS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub fn register_task_control(task_id: &str, project_id: &str) -> Arc<TaskControl> {
    let control = Arc::new(TaskControl {
        task_id: task_id.to_string(),
        project_id: project_id.to_string(),
        paused: Arc::new(AtomicBool::new(false)),
        stopped: Arc::new(AtomicBool::new(false)),
        notify: Arc::new(Notify::new()),
    });
    let mut map = get_active_tasks().lock().unwrap();
    map.insert(task_id.to_string(), Arc::clone(&control));
    map.insert(format!("proj:{}", project_id), Arc::clone(&control));
    control
}

pub fn unregister_task_control(task_id: &str, project_id: &str) {
    let mut map = get_active_tasks().lock().unwrap();
    map.remove(task_id);
    map.remove(&format!("proj:{}", project_id));
}

pub fn find_task_control(id: &str) -> Option<Arc<TaskControl>> {
    let map = get_active_tasks().lock().unwrap();
    if let Some(c) = map.get(id) {
        return Some(Arc::clone(c));
    }
    if let Some(c) = map.get(&format!("proj:{}", id)) {
        return Some(Arc::clone(c));
    }
    None
}

pub fn get_checkpoint_path(proj_dir: &std::path::Path) -> std::path::PathBuf {
    proj_dir.join(".axiom").join("generation_state.json")
}

pub fn load_generation_checkpoint(proj_dir: &std::path::Path) -> Option<Value> {
    let path = get_checkpoint_path(proj_dir);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                return Some(val);
            }
        }
    }
    None
}

pub fn save_generation_checkpoint(proj_dir: &std::path::Path, checkpoint: &Value) {
    let path = get_checkpoint_path(proj_dir);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(checkpoint) {
        let _ = fs::write(&path, content);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenerationPhase {
    Config = 1,
    DatabaseOrMain = 2,
    ApiOrState = 3,
    LogicOrHooks = 4,
    ShellOrWidgets = 5,
    PagesOrUi = 6,
}

pub fn get_generation_phase(file_path: &str, project_type: &str) -> GenerationPhase {
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

pub fn get_context_files_for(file_path: &str, proj_dir: &std::path::Path) -> String {
    let mut context = String::new();
    let path_lower = file_path.to_lowercase().replace('\\', "/");

    let is_backend_or_db = path_lower.contains("/api/")
        || path_lower.ends_with("route.ts")
        || path_lower.ends_with("route.js")
        || path_lower.contains("prisma")
        || path_lower.contains("server.")
        || path_lower.contains("/models/")
        || path_lower.contains("seed.");

    let is_auth_file = path_lower.contains("/login")
        || path_lower.contains("/register")
        || path_lower.contains("/signup")
        || path_lower.contains("auth");

    // 1. Database Schema & Prisma Singleton: strictly isolated to backend/API/database files.
    // Never leak raw Prisma schemas into UI components or client pages!
    if is_backend_or_db {
        let prisma_path = proj_dir.join("prisma/schema.prisma");
        let prisma_path_alt = proj_dir.join("schema.prisma");
        let actual_prisma = if prisma_path.exists() {
            Some(prisma_path)
        } else if prisma_path_alt.exists() {
            Some(prisma_path_alt)
        } else {
            None
        };

        if let Some(p_path) = actual_prisma {
            if path_lower != "prisma/schema.prisma" && path_lower != "schema.prisma" {
                if let Ok(content) = std::fs::read_to_string(&p_path) {
                    context.push_str(&format!(
                        "\nReference: Database Schema (schema.prisma):\n```prisma\n{}\n```\n",
                        content
                    ));
                }
            }
        }

        let prisma_lib = proj_dir.join("src/lib/prisma.ts");
        let prisma_lib_alt = proj_dir.join("src/lib/prisma.js");
        let actual_prisma_lib = if prisma_lib.exists() {
            Some(prisma_lib)
        } else if prisma_lib_alt.exists() {
            Some(prisma_lib_alt)
        } else {
            None
        };
        if let Some(pl_path) = actual_prisma_lib {
            if !path_lower.contains("prisma") {
                if let Ok(content) = std::fs::read_to_string(&pl_path) {
                    context.push_str(&format!(
                        "\nReference: Prisma client export (src/lib/prisma.ts):\n```typescript\n{}\n```\n",
                        content
                    ));
                }
            }
        }
    }

    // 2. NextAuth configuration: strictly isolated to authentication files (/login, /register, useAuth).
    // Never inject NextAuth server config into general UI components or dashboard/analytics pages!
    if is_auth_file && !path_lower.contains("[...nextauth]") {
        let auth_route = proj_dir.join("src/app/api/auth/[...nextauth]/route.ts");
        let auth_route_alt = proj_dir.join("src/app/api/auth/[...nextauth]/route.js");
        let actual_auth_route = if auth_route.exists() {
            Some(auth_route)
        } else if auth_route_alt.exists() {
            Some(auth_route_alt)
        } else {
            None
        };
        if let Some(ar_path) = actual_auth_route {
            if let Ok(content) = std::fs::read_to_string(&ar_path) {
                context.push_str(&format!(
                    "\nReference: NextAuth configuration (src/app/api/auth/[...nextauth]/route.ts):\n```typescript\n{}\n```\n",
                    content
                ));
            }
        }
    }

    // 3. Custom hooks reference: only for feature pages/components, excluding UI primitives and Footer/Navbar
    let is_feature_ui = (path_lower.contains("components") || (path_lower.contains("app") && path_lower.ends_with("page.tsx")))
        && !path_lower.contains("components/ui/")
        && !path_lower.contains("footer")
        && !path_lower.contains("navbar");

    if is_feature_ui {
        let hooks_dir = proj_dir.join("src/hooks");
        if hooks_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(hooks_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                context.push_str(&format!(
                                    "\nReference: Hook (src/hooks/{}):\n```typescript\n{}\n```\n",
                                    name, content
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    context
}

pub async fn validate_file_with_abstract_rules(
    file_path: &str,
    content: &str,
    model_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
        
    let system_prompt = "You are a strict code quality validator. Analyze the provided file path and code content for critical syntax errors, truncated code, or severe architectural violations. Rules:\n\
1. ONLY root layout files (`layout.tsx` / `layout.jsx`) import `globals.css`. Regular pages (`page.tsx`) and components do NOT need to import `globals.css`.\n\
2. In Next.js projects, `src/middleware.ts` or `middleware.ts` MUST use `NextRequest` and `NextResponse` from `'next/server'` and export `function middleware(request: NextRequest)`. It must NEVER use Express (`from 'express'`, `NextFunction`, or `(req, res, next)`).\n\
3. If NextAuth is used, ensure there are no invalid imports for Prisma or NextAuth.\n\
Respond ONLY with a JSON object in this format: { \"valid\": true } or { \"valid\": false, \"reason\": \"Detailed description of the issue\" }.";
    
    let user_prompt = format!("File Path: {}\n\nCode Content:\n```\n{}\n```", file_path, content);
    
    let ollama_req = serde_json::json!({
        "model": model_id,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": &user_prompt }
        ],
        "stream": false,
        "options": build_ollama_options(model_id, 0.1, Some(512))
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

pub fn resolve_app_router_route(rel_path: &str) -> Option<String> {
    let normalized = rel_path.replace('\\', "/");
    let lower = normalized.to_lowercase();
    
    let subpath = if lower.starts_with("src/app/") {
        &normalized["src/app/".len()..]
    } else if lower.starts_with("app/") {
        &normalized["app/".len()..]
    } else {
        return None;
    };

    let sub_lower = subpath.to_lowercase();
    let is_page = sub_lower == "page.tsx" 
        || sub_lower == "page.jsx" 
        || sub_lower == "page.js" 
        || sub_lower == "page.ts"
        || sub_lower.ends_with("/page.tsx")
        || sub_lower.ends_with("/page.jsx")
        || sub_lower.ends_with("/page.js")
        || sub_lower.ends_with("/page.ts");

    if !is_page {
        return None;
    }

    let parts: Vec<&str> = subpath.split('/').collect();
    let dir_parts = if parts.len() > 1 {
        &parts[..parts.len() - 1]
    } else {
        &[] as &[&str]
    };

    let mut route_segments = Vec::new();
    for seg in dir_parts {
        let seg_trimmed = seg.trim();
        if seg_trimmed.is_empty() {
            continue;
        }
        // Next.js Route groups e.g. (marketing), (auth), (dashboard) do not affect URL path
        if seg_trimmed.starts_with('(') && seg_trimmed.ends_with(')') {
            continue;
        }
        // Parallel routes e.g. @modal or private folders _components
        if seg_trimmed.starts_with('@') || seg_trimmed.starts_with('_') {
            continue;
        }
        route_segments.push(seg_trimmed);
    }

    if route_segments.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", route_segments.join("/")))
    }
}

pub fn get_project_routes_from_dir(proj_dir: &std::path::Path) -> Vec<(String, String)> {
    let mut routes = Vec::new();
    
    // 1. Try reading axiom-map.json first if it exists
    let axiom_map_path = proj_dir.join("axiom-map.json");
    if axiom_map_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&axiom_map_path) {
            if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
                if let Some(routes_obj) = parsed.get("routes").and_then(|r| r.as_object()) {
                    for (route, meta) in routes_obj {
                        let file = meta.get("file").and_then(|f| f.as_str()).unwrap_or("");
                        routes.push((route.clone(), file.to_string()));
                    }
                    if !routes.is_empty() {
                        routes.sort_by(|a, b| a.0.cmp(&b.0));
                        return routes;
                    }
                }
            }
        }
    }

    // 2. Scan directory directly
    fn scan_pages(dir: &std::path::Path, base_dir: &std::path::Path, routes: &mut Vec<(String, String)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules" && name != ".next" && name != "build" && name != "dist" {
                        scan_pages(&path, base_dir, routes);
                    }
                } else if path.is_file() {
                    if let Ok(rel_path) = path.strip_prefix(base_dir) {
                        let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                        if let Some(route) = resolve_app_router_route(&rel_str) {
                            routes.push((route, rel_str));
                        }
                    }
                }
            }
        }
    }

    scan_pages(proj_dir, proj_dir, &mut routes);
    routes.sort_by(|a, b| a.0.cmp(&b.0));
    routes.dedup_by(|a, b| a.0 == b.0);
    routes
}

pub fn format_routes_prompt(routes: &[(String, String)]) -> String {
    if routes.is_empty() {
        return String::new();
    }
    let mut prompt = String::new();
    prompt.push_str("\n=== VERIFIED APPLICATION ROUTES (SINGLE SOURCE OF TRUTH) ===\n");
    prompt.push_str("The following routes are strictly defined and implemented in this Next.js App Router application.\n");
    prompt.push_str("All navigation components (<Navbar>, <Sidebar>, <Footer>, <Header>, links, buttons) MUST link exclusively to these exact routes:\n");
    for (route, file_path) in routes {
        prompt.push_str(&format!("  - Route '{}' -> maps to file '{}'\n", route, file_path));
    }
    prompt.push_str("\nROUTING RULES:\n");
    prompt.push_str("1. LINK TARGETS: Every `<Link href=\"...\">` MUST use one of the exact route paths listed above.\n");
    prompt.push_str("2. NEVER FLATTEN ROUTES: Never invent flat root URLs like `/data-grid` or `/settings` if the page is located in a nested folder (e.g. `src/app/dashboard/data-grid/page.tsx` -> route is `/dashboard/data-grid`). Flat URLs for nested routes cause fatal 404 Not Found errors!\n");
    prompt.push_str("3. CLEAN NEXT.JS LINKS: Always use `<Link href=\"...\" prefetch={false} className=\"...\">` with no nested <a> tag.\n");
    prompt.push_str("============================================================\n");
    prompt
}

pub fn generate_axiom_map(proj_dir: &std::path::Path) -> Result<(), String> {
    use std::fs;
    use std::path::{Path, PathBuf};
    
    let mut files_map = serde_json::Map::new();
    let mut methods_map = serde_json::Map::new();
    let mut routes_map = serde_json::Map::new();
    
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
        
        if let Some(route) = resolve_app_router_route(&rel_path_str) {
            routes_map.insert(
                route,
                serde_json::json!({
                    "file": rel_path_str.clone()
                })
            );
        }
        
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
        "routes": routes_map,
        "files": files_map,
        "methods": methods_map
    });
    
    if let Ok(pretty) = serde_json::to_string_pretty(&axiom_map) {
        let _ = fs::write(proj_dir.join("axiom-map.json"), pretty);
    }
    
    Ok(())
}

pub fn get_project_routes_prompt(files: &[Value]) -> String {
    let mut routes = Vec::new();
    for file in files {
        if let Some(path_str) = file.get("path").and_then(|p| p.as_str()) {
            if let Some(route) = resolve_app_router_route(path_str) {
                routes.push((route, path_str.to_string()));
            }
        }
    }
    routes.sort_by(|a, b| a.0.cmp(&b.0));
    routes.dedup_by(|a, b| a.0 == b.0);
    format_routes_prompt(&routes)
}

pub const NEXTJS_REACT_RULEPACK: &str = r#"REACT JSX SYNTAX, HOOKS & STYLING RULES:
1. NEVER USE STRING STYLE ATTRIBUTES: In React/JSX, the 'style' attribute MUST NEVER be a string like style="color: yellow;". Passing a string throws a fatal runtime crash ('The `style` prop expects a mapping from style properties to values, not a string'). ALWAYS use Tailwind CSS utility classes via `className="..."` (e.g., `className="text-yellow-400 font-bold"`) or, if strictly necessary, a JS style object (e.g., `style={{ color: 'yellow' }}`).
2. PREFER TAILWIND CSS CLASSES: When fulfilling styling requests (colors, typography, spacing, padding, margins, flex, grid, hover states), ALWAYS apply Tailwind CSS classes to `className`. Never write inline style strings.
3. TAILWIND DYNAMIC CLASSES: NEVER construct Tailwind classes using runtime string interpolation like className={"text-" + color + "-500"}. Tailwind's compiler statically scans source files; dynamic fragments are stripped in production builds. Always use complete class names or a static mapping object.
4. JSX ATTRIBUTE CONVENTIONS: In JSX, never use pure HTML attribute names: use `className` instead of `class`, `htmlFor` instead of `for`, and camelCase for event handlers (`onClick`, `onChange`, `onSubmit`). Void tags (<img />, <input />, <br />, <hr />, <meta />, <link />) MUST self-close with '/>'.
5. NEVER CALL HOOKS AT MODULE SCOPE: React hooks (e.g., useTranslations, useState, useEffect, useContext, useRef, useSession, useRouter, useParams, etc.) MUST ONLY be called directly inside the body of a React Function Component or a custom hook (a function starting with 'use'). NEVER invoke any hook outside of a component body or in global/module scope.
6. NO CONDITIONAL HOOKS: Hooks must be placed unconditionally at the top level of the component function. Never invoke hooks inside 'if' branches, loops, nested callbacks, or after early returns.
7. 'use client' DIRECTIVE: Any component that uses React hooks, event listeners (e.g. onClick, onChange, onSubmit), or browser APIs (window, localStorage, document) MUST have 'use client' as the very first line of the file. Server Components (default, no directive) may be async and fetch data directly.
8. SERVER-ONLY IMPORT LEAKAGE: Never import @prisma/client, fs, path, crypto, or server secrets into files containing 'use client'. Pass serialized data as props from Server Components or fetch via Route Handlers.
9. NO FAKE CONTEXT BYPASS: Never attempt to bypass a missing React Context by defining mock objects or passing illegal second arguments to context-consuming hooks in the component file. If context is missing, the component must be wrapped by its parent Provider in a layout or parent component.
10. HYDRATION INTEGRITY: NEVER generate different initial markup on server vs client using Date.now(), Math.random(), or unguarded browser-only state. Perform browser-dependent updates after mount inside useEffect to prevent hydration mismatch crashes.
11. IMAGE OPTIMIZATION: Always use `import Image from 'next/image';` with required `width`, `height`, and `alt` props. Never use unoptimized `<img>` tags for static assets.
12. DYNAMIC ROUTE PARAMS: In Next.js 14 App Router, dynamic route params are synchronous objects: type them as `{ params }: { params: { id: string } }`. Do not await params.
13. ROUTE HANDLER SIGNATURES: In `app/api/**/route.ts`, export named async functions per HTTP method (`export async function GET(request: Request) { return NextResponse.json({...}); }`). Never use Pages Router default export `handler(req, res)`. Always validate incoming bodies with Zod before database operations.
14. ENVIRONMENT VARIABLE EXPOSURE: Client components can only read variables prefixed with `NEXT_PUBLIC_` (e.g. `process.env.NEXT_PUBLIC_API_URL`). Server secrets (DATABASE_URL, JWT_SECRET) must remain unprefixed and server-only.
15. SERVER ACTIONS DIRECTIVE: Mutating server actions must have `'use server'` as the first line of their file or function body. Pass actions to `<form action={myAction}>` or invoke via import.
16. CLIENT-ONLY DYNAMIC IMPORTS: `dynamic(() => import('./Chart'), { ssr: false })` is only valid inside `'use client'` files. Never use `ssr: false` directly in Server Components.
17. NEXT.JS APP ROUTER NAVIGATION IMPORTS: In Next.js App Router (`src/app/` or `app/`), NEVER import `useRouter`, `usePathname`, or `useSearchParams` from `'next/router'`. Importing from `'next/router'` throws a fatal runtime crash: "NextRouter was not mounted". ALWAYS import from `'next/navigation'`.
18. NEXTAUTH CREDENTIALS PROVIDER IMPORT: In NextAuth configuration files, `CredentialsProvider` MUST be imported from `'next-auth/providers/credentials'`. NEVER import from `'next-auth'` directly (which imports the NextAuth initialization function instead of the credentials provider factory).
19. APP ROUTER API ISOLATION: In App Router projects, all API routes MUST live under `src/app/api/**/route.ts`. NEVER create parallel duplicate endpoints in `src/pages/api/**` (e.g. `pages/api/register.ts` alongside `app/api/register/route.ts`). Having both causes a fatal Next.js build error: "Conflicting app and page files were found".
20. REACT-ICONS ROOT BAN & SUBPATH SPECIFICATION: In `react-icons`, NOTHING is exported from the root package `'react-icons'`. NEVER write `import { ... } from 'react-icons'`. This causes a fatal compilation error: "Attempted import error: '...' is not exported from 'react-icons'". Icons MUST ALWAYS be imported from their specific family subpath: 'Hi...' from `'react-icons/hi'` (or `'react-icons/hi2'`), 'Fa...' from `'react-icons/fa'`, 'Fi...' from `'react-icons/fi'`, 'Md...' from `'react-icons/md'`, 'Io...' from `'react-icons/io5'`, 'Ai...' from `'react-icons/ai'`, 'Bs...' from `'react-icons/bs'`, 'Ri...' from `'react-icons/ri'`, 'Bi...' from `'react-icons/bi'`. Alternatively, use `'lucide-react'` for unified imports.
21. PRISMA SINGLETON PATTERN: In `src/lib/prisma.ts`, `prisma` MUST be an instantiated singleton of `PrismaClient`, NOT a function or type. Always export a ready-to-use instance: `import { PrismaClient } from '@prisma/client'; const globalForPrisma = globalThis as unknown as { prisma: PrismaClient | undefined }; export const prisma = globalForPrisma.prisma ?? new PrismaClient(); if (process.env.NODE_ENV !== 'production') globalForPrisma.prisma = prisma; export default prisma;`.
22. MANDATORY DEFAULT EXPORT IN APP ROUTER (LAYOUTS & PAGES): Every file in `src/app/**/layout.tsx` (or `.jsx`) and `src/app/**/page.tsx` (or `.jsx`) MUST have an `export default function` (e.g. `export default function RootLayout({ children }: { children: React.ReactNode })`). NEVER omit `export default` or define only a local `const Layout = ...` without default export. If a file marked with 'use client' lacks a default export, the React Server Components runtime fails to resolve `module.default` through its client proxy and crashes fatally with: "Error: Cannot access default.then on the server. You cannot dot into a client module from a server component."
23. ROOT LAYOUT DOM HIERARCHY & PROVIDER PLACEMENT: In `src/app/layout.tsx`, `<html lang="en">` and `<body>` MUST be the outermost tags returned by `RootLayout`. Global Context Providers (such as `<SessionProvider>`, `<ThemeProvider>`, `<QueryClientProvider>`) MUST ALWAYS be placed INSIDE the `<body>` tag wrapping `{children}`: `return (<html lang="en"><body><SessionProvider>{children}</SessionProvider></body></html>);`. NEVER wrap any Provider outside the `<html>` tag! Wrapping `<html>` inside a Provider or component produces an invalid DOM hierarchy and causes fatal Next.js hydration and server-rendering crashes.
24. CLIENT COMPONENTS CANNOT BE ASYNC: Any component marked with 'use client' MUST NEVER be declared as an async function (e.g. `export default async function Component()`). In React 18 / Next.js 14, only Server Components can be async. Declaring an async client component causes it to return a Promise, prompting the RSC compiler to access `.then` on the client module proxy and throwing "Error: Cannot access default.then on the server". For asynchronous data fetching in client components, always use `useEffect` or standard state hooks with synchronous component signatures.
25. NEXT.JS MIDDLEWARE MATCHER SYNTAX: In `src/middleware.ts` or `src/middleware.js`, route matchers use `path-to-regexp` syntax. Wildcards MUST use named parameters (e.g., `'/dashboard/:path*'`, `'/profile/:path*'`, `'/admin/:path*'`) or regex filters (e.g. `'/((?!api|_next|_vercel|.*\\..*).*)'`). NEVER use glob wildcards like `'/dashboard/*'` or `'/profile/*'`. Glob syntax (`/*`) throws a fatal route parsing crash: "Error parsing `/.../*` Reason: Unexpected MODIFIER, expected END".
26. AUTH-GATED CLIENT COMPONENTS & PREVIEW RESILIENCE: In client components that check session state (e.g. `useSession()` from 'next-auth/react') or fetch from internal API routes (e.g. `/api/kpi`, `/api/stats`), NEVER leave a `loading` state permanently stuck at `true` when `session` is null, unauthenticated, or when API fetch fails. Always initialize state with realistic mock data items (e.g. 5-10 realistic KPI or table records) and ALWAYS ensure `setLoading(false)` is reached unconditionally (e.g. in a `finally` block or `else` branch). If `!session`, render the page using the initial mock data or redirect to `/login`—NEVER leave the user stuck on an infinite pulsing icon or blank screen in development preview mode.
27. TANSTACK REACT QUERY V5 OBJECT SIGNATURE: When using `@tanstack/react-query` (v5+), NEVER pass positional arguments to `useQuery`, `useMutation`, or query functions. The old v4 positional signature `useQuery('key', fetchFn)` or `useQuery(['key'], fetchFn)` is STRICTLY FORBIDDEN and throws fatal runtime error: "Bad argument type. Starting with v5, only the Object form is allowed". ALWAYS pass a single configuration object:
- FORBIDDEN (v4): `useQuery('metrics', fetchMetrics)`
- MANDATORY (v5): `useQuery({ queryKey: ['metrics'], queryFn: fetchMetrics })`
- FORBIDDEN (v4): `useMutation(postTodo)`
- MANDATORY (v5): `useMutation({ mutationFn: postTodo })`"#;

pub const CONFIG_FILES_RULEPACK: &str = r#"CONFIGURATION & TSCONFIG RULES:
1. TSCONFIG.JSON CONCISENESS & VALIDITY: `tsconfig.json` MUST be a valid, compact, non-redundant JSON object (maximum 30 lines). NEVER repeat compilerOptions flags or output duplicate keys (`strictNullChecks`, `noImplicitAny`). Redundant repetition loops corrupt the file and crash compilers.
2. TSCONFIG COMPILER OPTIONS SPECIFICATION: Always include: "lib": ["dom", "dom.iterable", "esnext"], "allowJs": true, "skipLibCheck": true, "strict": false, "noEmit": true, "esModuleInterop": true, "module": "esnext", "moduleResolution": "bundler", "resolveJsonModule": true, "isolatedModules": true, "jsx": "preserve", "paths": { "@/*": ["./src/*"] }.
3. TSCONFIG INCLUDES & EXCLUDES: Include array MUST strictly be `["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"]` and exclude MUST be `["node_modules"]`.
4. NEXT.CONFIG.JS FORMAT: Always export `module.exports = nextConfig;` where `nextConfig` is a plain JavaScript object. Never use ES6 `export default` in `.js` config files unless `type: "module"` is set.
5. TAILWIND.CONFIG.JS CONTENT ARRAY: The `content` array must explicitly include `./src/**/*.{js,ts,jsx,tsx,mdx}`."#;

pub const NEXT_INTL_RULEPACK: &str = r#"NEXT-INTL (INTERNATIONALIZATION) RULES:
1. useTranslations SIGNATURE ENFORCEMENT: 'useTranslations' takes only an optional namespace string, e.g. useTranslations('Header') or useTranslations(). NEVER pass an object, dictionary, or message map as a second argument (e.g., `useTranslations('Header', messages)` is STRICTLY INVALID and will crash).
2. TRANSLATIONS STORAGE: Translation key-value pairs must be defined inside JSON files under the messages directory (e.g., src/messages/en.json). Do not declare inline translation message maps inside component code.
3. CLIENT PROVIDER REQUIREMENT: In client components, 'useTranslations' requires NextIntlClientProvider to be mounted in the component tree."#;

pub const NEXT_LINK_RULEPACK: &str = r#"NEXT.JS LINK COMPONENT RULES:
1. NO NESTED ANCHOR TAGS: In Next.js 13+, do NOT place <a> tags inside <Link href="...">. Place className and styling directly on the <Link> component (e.g. `<Link href="/about" className="...">About</Link>`).
2. PREFETCH OPTIMIZATION: Always add `prefetch={false}` to <Link> components (e.g. `<Link href="/dashboard" prefetch={false}>`) to prevent aggressive edge request amplification on Vercel.
3. ROUTE INTEGRITY & NO PATH FLATTENING: In Next.js App Router, the `href` attribute in `<Link>` MUST match the exact folder structure under `src/app/` (or `app/`). If a page file is located at `src/app/dashboard/data-grid/page.tsx`, the link MUST be `<Link href="/dashboard/data-grid">`. NEVER flatten nested routes to the root (e.g. NEVER write `<Link href="/data-grid">` when the page is in `src/app/dashboard/data-grid/`). Root-flattened links result in fatal 404 (Not Found) runtime errors. Always include the full parent folder prefix (e.g. `/dashboard/...`) for all nested subpages. Route groups in parentheses (e.g. `(auth)` or `(marketing)`) do not appear in the URL path, but all regular directory folders MUST be preserved."#;

pub const NEXTJS_MIDDLEWARE_RULEPACK: &str = r#"NEXT.JS MIDDLEWARE & EDGE ROUTING RULES:
1. PATH-TO-REGEXP MATCHERS: In `src/middleware.ts` or root `middleware.ts`, `config.matcher` strictly parses paths using `path-to-regexp`. Wildcards for subpaths MUST use named parameters (e.g. `'/dashboard/:path*'`, `'/profile/:path*'`, `'/admin/:path*'`) or regex filters (e.g. `'/((?!api|_next|_vercel|.*\\..*).*)'`). NEVER write glob wildcards like `'/dashboard/*'` or `'/profile/*'`. Unnamed wildcards (`/*` or `/**`) throw fatal syntax crashes: "Error parsing /.../* Reason: Unexpected MODIFIER, expected END: Invalid middlewares found".
2. SINGLE ROOT/SRC MIDDLEWARE FILE: Next.js allows only ONE middleware file per project located strictly at `src/middleware.ts` (or `middleware.ts` at the root). Never create nested middleware files inside app subfolders.
3. EDGE RUNTIME BOUNDARY: Next.js middleware executes exclusively in the Edge Runtime. NEVER import Node.js native packages (`fs`, `path`, `child_process`, `crypto`) or database drivers/Prisma Client in middleware files. Database operations belong in Server Components, Route Handlers, or Server Actions.
4. WITHAUTH & NEXTAUTH SESSIONS: For NextAuth route protection in middleware, either wrap the handler with `withAuth` from `'next-auth/middleware'` or inspect the JWT via `await getToken({ req: request, secret: process.env.NEXTAUTH_SECRET })`. For unauthenticated access to protected routes, redirect using `NextResponse.redirect(new URL('/login', request.url))`.
5. ASSET BYPASS OPTIMIZATION: Always filter out or bypass static resources (`/_next/static`, `/_next/image`, `/favicon.ico`, and files with extensions like `.png`, `.svg`, `.css`) in the matcher configuration to avoid redundant middleware execution on static assets."#;

pub const MERN_STACK_RULEPACK: &str = r#"MERN STACK (EXPRESS + MONGOOSE + VITE/REACT) RULES:
1. EXPRESS ASYNC ROUTE SAFETY: Wrap every async route handler body in try/catch. Always return explicit status codes: `res.status(200).json(data)` for success, `res.status(201).json(data)` for created, `res.status(400)` for invalid input, `res.status(404)` for missing resources, `res.status(500).json({ error: err.message })` in catch. Immediately return after sending response to prevent 'Cannot set headers after they are sent'.
2. MIDDLEWARE ORDER: Always register `app.use(cors())` and `app.use(express.json())` immediately after `const app = express();`, BEFORE registering any API routes. Never register body parsers after route endpoints.
3. RUNTIME SEPARATION: Never import React components into `server.js`, and never import Mongoose models, `fs`, or Node core modules into `client/src/`. Client communicates strictly via HTTP (`axios.get('/api/...')`).
4. REACT ROUTER DOM V6 STANDARDS: Never use React Router v5 syntax (<Switch>, `<Route component={Comp}>`, `useHistory()`). Use React Router v6: `<Routes><Route path="/..." element={<Component />} /></Routes>` and `const navigate = useNavigate(); navigate('/path')`.
5. DOTENV LOAD ORDER: `require('dotenv').config();` must be the absolute first line executed in `server.js`, before any other local module reads `process.env`.
6. MONGOOSE CONNECTION SINGLETON: Call `mongoose.connect(process.env.MONGO_URI)` once in `server.js` at startup before `app.listen(...)`. Never connect inside individual route files or per request.
7. PASSWORD HASHING BEFORE SAVE: Never store plaintext passwords or hash passwords inside route handlers. Always hash in a Mongoose pre-save hook: `userSchema.pre('save', async function(next) { if (!this.isModified('password')) return next(); this.password = await bcrypt.hash(this.password, 10); next(); });`.
8. JWT AUTH MIDDLEWARE: Centralize authorization in a `protect` middleware that verifies `req.headers.authorization?.split(' ')[1]` with `jwt.verify()`. Never duplicate inline token verification in every route.
9. CONTROLLED INPUT STATE BINDING: In React form controls, `<input value={name} />` MUST always be paired with `onChange={(e) => setName(e.target.value)}`. Never leave a value-bound input without an onChange handler.
10. FILE UPLOAD MIDDLEWARE: For `multipart/form-data` uploads, always use `multer` middleware (e.g. `upload.single('file')`). `express.json()` cannot parse multipart form payloads.
11. POPULATE VS MANUAL JOINS: Use Mongoose `.populate('author', 'name email')` on schema fields with `ref` types. Never perform manual joins using loops with secondary `.findById()` queries.
12. ENVIRONMENT VARIABLE ISOLATION: Client code uses `import.meta.env.VITE_API_URL` (Vite-injected); backend uses `process.env.MONGO_URI` (Node/dotenv). Never cross the two. Handle duplicate-key MongoDB errors (code 11000) with HTTP 409 Conflict."#;

pub const REACT_NATIVE_EXPO_RULEPACK: &str = r#"REACT NATIVE & EXPO MOBILE RULES:
1. ABSOLUTE WEB DOM ELEMENT BAN: NEVER use HTML/DOM tags (<div>, <span>, <p>, <a>, <button>, <h1>-<h6>, <input>, <form>, <ul>, <li>, <img>). Use React Native primitives exclusively: <View>, <Text>, <TouchableOpacity>, <Pressable>, <TextInput>, <ScrollView>, <FlatList>, <SafeAreaView>, <Image>.
2. MANDATORY TEXT WRAPPING: ALL textual content and string variables MUST be wrapped inside a `<Text>` component. Never place raw text strings directly inside `<View>` or other containers.
3. BAN OF BROWSER GLOBALS & STORAGE: Never use window, document, localStorage, sessionStorage, or window.location. Use `@react-native-async-storage/async-storage` for key-value persistence. Use Expo Router or `@react-navigation` for screen navigation.
4. STYLING ARCHITECTURE: Never pass CSS strings (style="display:flex; margin: 10px;") or use web units (px, rem, em, vh, vw). Use `StyleSheet.create({ container: { flex: 1, padding: 16 } })` with unitless numeric dimensions, or NativeWind `className` if configured.
5. NATIVE TOUCH & INPUT EVENTS: Never use onClick, onChange, or onMouseEnter. Use `onPress` for touchables/buttons and `onChangeText` for `<TextInput>` (which receives string value directly).
6. GESTURE HANDLER ROOT REQUIREMENT: The root of the application MUST be wrapped in `<GestureHandlerRootView style={{ flex: 1 }}>` from `react-native-gesture-handler` when using swipeable lists, bottom sheets, or Reanimated.
7. KEYBOARD-AWARE LAYOUT: For screens with form inputs, wrap content in `<KeyboardAvoidingView behavior={Platform.OS === 'ios' ? 'padding' : 'height'}>` to prevent the software keyboard from covering input fields.
8. IMAGE SOURCE SYNTAX: Use `source={require('./assets/logo.png')}` for local images and `source={{ uri: 'https://...' }}` for remote images. The web `src` prop is invalid in React Native.
9. PLATFORM-SPECIFIC BRANCHING: Use `Platform.OS === 'ios'` or `Platform.select({...})` for OS-specific behavior. Do not assume web navigator APIs exist.
10. ASYNC PERMISSIONS PATTERN: Hardware APIs (camera, location, media library) must always explicitly await permissions first: `const { status } = await Camera.requestCameraPermissionsAsync(); if (status !== 'granted') return;`.
11. FONT & SPLASH SCREEN LOADING: Call `SplashScreen.preventAutoHideAsync()` at module load and gate UI rendering on font load completion: `const [loaded] = useFonts({...}); if (!loaded) return null;`.
12. ICONS IMPORT SOURCE: Always import icons from `@expo/vector-icons` (e.g. `Ionicons`, `MaterialIcons`). Never import web icon packages (e.g. `react-icons`) which depend on the DOM.
13. LIST VIRTUALIZATION: Never render large arrays using ScrollView + .map(). Always use `<FlatList data={items} keyExtractor={(item) => item.id} renderItem={({item}) => <Row item={item} />} />` with a stable unique keyExtractor.
14. SAFE AREA AWARENESS: Wrap top-level screens in `<SafeAreaView style={{ flex: 1 }}>` (from `react-native-safe-area-context`) to avoid rendering behind the device notch, status bar, or home indicator.
15. NO COMPOSITE BORDER OR BOXSHADOW STRINGS: In React Native, `border: "1px solid #ccc"` and `boxShadow: "..."` are strictly INVALID and will crash the JavaScript runtime. Always split borders into individual numeric properties: `{ borderWidth: 1, borderColor: '#ccc', borderStyle: 'solid' }`. For shadows, use platform-specific styling: Android uses `{ elevation: 4 }`; iOS uses `{ shadowColor: '#000', shadowOffset: { width: 0, height: 2 }, shadowOpacity: 0.15, shadowRadius: 4 }`.
16. MANDATORY CONTENTCONTAINERSTYLE FOR SCROLLABLE CONTAINERS: Inside `<ScrollView>` and `<FlatList>`, internal padding, bottom margins, and item alignment MUST be applied via `contentContainerStyle={{ padding: 16, paddingBottom: 80 }}`. Applying padding to `style` clips content and prevents scrolling to the bottom.
17. ICONS IMPORT RESTRICTION (NO WEB LUCIDE OR REACT-ICONS): Never import from 'lucide-react' or 'react-icons'. They depend on browser SVG elements and crash React Native. Always import icons from '@expo/vector-icons' (e.g. `import { Ionicons } from '@expo/vector-icons'`) or 'lucide-react-native'.
18. EXPO ROUTER (V3/V4) NAVIGATION STANDARD: Modern Expo projects use file-based Expo Router under `app/`. Every route file must export a default component (`export default function Screen()`). Navigate using `import { useRouter } from 'expo-router'; const router = useRouter(); router.push('/details');`. Never wrap screens in `<NavigationContainer>` when using Expo Router.
19. OFFLINE-FIRST RESILIENCE & ERROR BOUNDARIES: All network requests (e.g. `fetch` or `axios`) must be wrapped in `try/catch`. On network failure, populate state with cached data from `@react-native-async-storage/async-storage` or render an informative offline fallback view with a retry action. Never leave screens blank on network failure."#;

pub const DESKTOP_RULEPACK: &str = r#"DESKTOP (ELECTRON & TAURI) RULES:
1. ELECTRON CONTEXT ISOLATION: Never call require('electron'), require('fs'), or use ipcRenderer directly inside renderer/React components. In `main.js`, configure `webPreferences: { contextIsolation: true, nodeIntegration: false, preload: path.join(__dirname, 'preload.js') }`.
2. PRELOAD BRIDGE CONTRACT: In `preload.js`, expose narrow allowlisted functions via `contextBridge.exposeInMainWorld('electronAPI', { getData: () => ipcRenderer.invoke('get-data') })`. The frontend invokes via `window.electronAPI.getData()`. Never expose raw Node modules or the entire ipcRenderer object.
3. IPC INVOCATION PAIRING: Use `ipcRenderer.invoke('channel', args)` paired with `ipcMain.handle('channel', async (event, args) => { ... })` for request/response operations. Never leave IPC listeners dangling on destroyed windows.
4. WINDOW LIFECYCLE MANAGEMENT: Always guard window operations with `if (mainWindow && !mainWindow.isDestroyed())`. Clean up IPC event listeners when windows close. Handle `window-all-closed` and `activate` lifecycle events appropriately.
5. FILESYSTEM PATH RESOLUTION: Always resolve OS-safe writable locations using `app.getPath('userData')`, `app.getPath('temp')`, or `app.getPath('documents')`. Never hardcode absolute paths like `C:\\...` or cwd-relative paths that fail in packaged installers.
6. NATIVE MENU CONSTRUCTION: Build application menus in the main process using `Menu.buildFromTemplate([{ label: 'File', submenu: [{ role: 'quit' }] }])` and `Menu.setApplicationMenu(menu)`. Never build HTML `<nav>` menus expecting native OS shortcuts.
7. TAURI COMMAND SIGNATURES: In Rust Tauri commands, never use `.unwrap()` or `panic!()`. Always annotate with `#[tauri::command]` and return `Result<T, String>` where errors are mapped with `.map_err(|e| e.to_string())`.
8. TAURI COMMAND REGISTRATION: Every `#[tauri::command]` function MUST be registered inside `tauri::generate_handler![cmd1, cmd2, ...]` during app builder setup.
9. TAURI STATE MANAGEMENT: Access shared state via `tauri::State<Mutex<AppState>>` registered with `.manage(Mutex::new(AppState::default()))`. Never block the UI thread with long-running synchronous operations.
10. TAURI V2 CAPABILITIES & PERMISSIONS: In Tauri v2, declare all plugin permissions (e.g. `"fs:allow-read-file"`) in `src-tauri/capabilities/*.json`. Calls to unlisted permissions will be rejected by ACL.
11. EVENT SYSTEM VS COMMAND INVOKE: Use `window.emit("channel", payload)` from backend and `listen("channel", callback)` from frontend for backend-pushed notifications (progress, file watchers). Use commands for frontend-initiated calls.
12. NO CROSS-PLATFORM POLLUTION: Never reference `window.electronAPI` inside Tauri applications, and never use Tauri `invoke` inside Electron.
13. NO MUTEXGUARD ACROSS .AWAIT (SEND TRAIT RULE): In async Rust code, NEVER hold a standard `std::sync::MutexGuard` across an `.await` boundary. Standard `MutexGuard` does not implement `Send`, causing fatal compiler error: "future cannot be sent between threads safely". Either use `tokio::sync::Mutex`, or acquire the lock, clone the needed value into a local variable, drop the guard (`drop(guard)`), and only then execute `.await`.
14. TAURI V2 EMITTER & EVENT SYSTEM: In Tauri v2, emitting events from backend requires the `tauri::Emitter` trait: always add `use tauri::Emitter;` at the top of the file when calling `window.emit("channel", payload)` or `app_handle.emit("channel", payload)`. Listening requires `use tauri::Listener;`.
15. SERDE DERIVE ON ALL IPC DATA STRUCTURES: Any struct, enum, or DTO sent to or received from frontend commands MUST implement `serde::Serialize` and `serde::Deserialize`: always annotate with `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]`. Omitting these causes fatal trait bound errors in `#[tauri::command]`.
16. LIFETIME-SAFE ERROR CONVERSION: Never return static string slices `Result<T, &'static str>` or dangling references from Tauri commands. Always map errors into owned `String` instances using `.map_err(|e| e.to_string())`.
17. NON-BLOCKING TOKIO WORKERS: CPU-intensive operations (heavy computation, cryptographic operations like `bcrypt`) and blocking filesystem calls must be offloaded using `tokio::task::spawn_blocking(move || { ... }).await` to prevent starving the Tokio async worker pool and freezing the application window."#;

pub const PRISMA_DATABASE_RULEPACK: &str = r#"PRISMA ORM & DATABASE RULES:
1. SERVER-ONLY BOUNDARY: Prisma Client (`@prisma/client`) must NEVER be imported or instantiated in client components or files marked with `'use client'`. Database operations belong strictly in Server Components, Route Handlers (`app/api/**/route.ts`), or Server Actions (`'use server'`).
2. CONNECTION POOLING SINGLETON: In development and serverless environments, always cache the PrismaClient instance on `globalThis` to prevent connection pool exhaustion during hot reload: `const globalForPrisma = globalThis as unknown as { prisma: PrismaClient }; export const prisma = globalForPrisma.prisma ?? new PrismaClient(); if (process.env.NODE_ENV !== 'production') globalForPrisma.prisma = prisma;`.
3. RELATION FOREIGN-KEY PLACEMENT: In `schema.prisma`, the scalar foreign key (`authorId String`) and the `@relation(fields: [authorId], references: [id])` attribute MUST be defined on the model that stores the foreign key (the child/many side). The parent model defines only the back-relation array (`posts Post[]`).
4. CASCADING DELETE BEHAVIOR: Always specify referential action on relations when child records should not outlive the parent: `@relation(fields: [authorId], references: [id], onDelete: Cascade)`.
5. OFFLINE & PREVIEW DEV RESILIENCE: Wrap database calls in Route Handlers in try/catch. In catch blocks, return realistic deterministic mock data matching the TypeScript interface so the application remains fully interactive in preview mode even if the database is offline or unmigrated.
6. SCHEMA SYNTAX VS TYPESCRIPT SYNTAX: In `schema.prisma`, optional fields use `name String?` (type first, `?` suffix). In TypeScript interfaces, optional fields use `name?: string` (`?` before colon, lowercase primitive). Never mix the two syntaxes.
7. QUERY KEY CONSTRAINTS: Fields used in `where` clauses of `findUnique`, `update`, or `delete` MUST be marked with `@id`, `@unique`, or part of `@@unique([...])` in `schema.prisma`.
8. ZOD VALIDATION & TYPE SYNC: Parse incoming request bodies with `schema.safeParse(body)` before invoking Prisma mutations. Keep Zod field optionality strictly synchronized with schema nullable fields.
9. ENUM DEFINITION SYNTAX: Define enums as top-level blocks without commas or quotes: `enum Role { ADMIN USER }` and use on fields as `role Role @default(USER)`. Never write TypeScript union strings (`role: "ADMIN" | "USER"`) in `.prisma` files.
10. SEED SCRIPT CONFIGURATION: Declare seed script in `package.json` under `"prisma": { "seed": "ts-node prisma/seed.ts" }` so `npx prisma db seed` locates it reliably.
11. TRANSACTIONAL MULTI-WRITE OPERATIONS: When performing multiple dependent mutations, always use `prisma.$transaction([prisma.order.create({...}), prisma.inventory.update({...})])` to guarantee atomic all-or-nothing execution.
12. INDEX DECLARATION FOR QUERY PERFORMANCE: Always add `@@index([status, createdAt])` at the model level for field combinations frequently used in `where` or `orderBy` clauses.
13. ROUTE HANDLER HTTP METHODS: App Router route handlers in `route.ts` must export named async functions per HTTP verb (`export async function GET(request: Request) { return NextResponse.json({...}); }`). Never use default export `handler(req, res)`."#;

pub const UI_DESIGN_FOUNDATION_RULEPACK: &str = r#"MOBILE-FIRST & RESPONSIVE UI DESIGN FOUNDATION RULES:
1. CONTAINER & SPACING DISCIPLINE: Wrap page content inside `<div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8 w-full space-y-8">`. Never render raw unpadded content flush against the viewport edges.
2. PAGE HEADER FLEX ALIGNMENT: Every page header (containing a title/subtitle on the left and primary action button(s) on the right) MUST use `<div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-slate-200/80 pb-6">`.
3. BUTTON & ICON INLINE-FLEX RULE: Every `<button>` or `<Link>` that contains an icon alongside text MUST include `inline-flex items-center justify-center gap-2` (e.g. `<button className="inline-flex items-center justify-center gap-2 bg-indigo-600 hover:bg-indigo-700 text-white font-semibold px-4 py-2.5 rounded-xl shadow-sm transition-colors">`). Without `inline-flex items-center`, SVG icons render as block elements and stack vertically above button text!
4. MOBILE-FIRST GRID CASCADE: Never use fixed multi-column grids (like bare `grid-cols-3`) without mobile breakpoints. Always cascade from 1 column on mobile: `grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6` (or `grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6` for KPI stat cards).
5. RESPONSIVE TABLE WRAPPER: Always wrap `<table>` elements inside `<div className="w-full overflow-x-auto rounded-2xl border border-slate-200 bg-white shadow-sm">` so wide tables scroll horizontally on mobile devices without breaking page layout.
6. SAFE DATE FORMATTING: Dates in JSON responses or state may be ISO strings or Date objects. NEVER call `.toLocaleDateString()` directly on a property (e.g. `item.startDate.toLocaleDateString()` throws a TypeError when `startDate` is a string!). Always wrap with `new Date(item.startDate).toLocaleDateString()`."#;

pub const UI_NAVBAR_RULEPACK: &str = r#"SCOPED NAVBAR, BRAND LOGO & HAMBURGER MENU RULES:
1. STICKY GLASSMORPHISM HEADER: Structure the navbar as `<header className="sticky top-0 z-50 w-full backdrop-blur-md bg-white/90 border-b border-slate-200/80 shadow-sm">` with an inner container `<div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center justify-between gap-4">`.
2. INLINE SVG BRAND LOGO LOCKUP (NO BROKEN IMAGE FILES):
   - NEVER use `<img src="/logo.png" />` or `<Image src="/logo.svg" />` (which cause 404 broken image icons), and NEVER render plain unstyled text alone.
   - Always build a modern Brand Lockup inside `<Link href="/" prefetch={false} className="flex items-center gap-2.5 group shrink-0">`:
     a) Logo Mark Badge: `<div className="w-9 h-9 rounded-xl bg-gradient-to-br from-indigo-600 to-blue-600 flex items-center justify-center text-white shadow-md shadow-indigo-500/20">` containing a crisp inline `<svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2"><path strokeLinecap="round" strokeLinejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>`.
     b) Brand Wordmark: `<span className="text-lg font-extrabold tracking-tight text-slate-900">AppName</span>`.
3. DESKTOP NAVIGATION PILLS: Place desktop links inside `<div className="hidden md:flex items-center gap-1">`. Highlight the active route (`pathname === href` or `pathname?.startsWith(href)`) using pill styling `bg-indigo-50 text-indigo-600 font-semibold px-3.5 py-2 rounded-lg text-sm transition-colors`, and style inactive links with `text-slate-600 hover:text-slate-900 hover:bg-slate-100 px-3.5 py-2 rounded-lg text-sm font-medium transition-colors`.
4. STYLED AUTH / CTA ACTIONS: Place right-side actions inside `<div className="hidden md:flex items-center gap-3 shrink-0">`. Style Sign In as `<Link href="/login" prefetch={false} className="text-sm font-semibold text-slate-700 hover:text-indigo-600 px-3.5 py-2 rounded-lg transition-colors">` and Register / Primary CTA as `<Link href="/register" prefetch={false} className="inline-flex items-center justify-center bg-indigo-600 hover:bg-indigo-700 text-white text-sm font-semibold px-4 py-2 rounded-xl shadow-sm transition-colors">`.
5. FUNCTIONAL HAMBURGER MENU & MOBILE DRAWER:
   - Declare `const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);`.
   - Render the mobile toggle button inside `<div className="flex md:hidden">`: `<button type="button" onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)} className="inline-flex items-center justify-center p-2 rounded-lg text-slate-600 hover:text-slate-900 hover:bg-slate-100 focus:outline-none" aria-label="Toggle Menu">` toggling between Hamburger and Close icons.
   - Immediately below the main header row, render the collapsible mobile drawer: `{isMobileMenuOpen && (<div className="md:hidden border-t border-slate-200 bg-white px-4 pt-3 pb-4 space-y-1.5 shadow-lg">...all navigation & auth <Link onClick={() => setIsMobileMenuOpen(false)} className="block px-3 py-2 rounded-lg text-base font-medium ...">...</div>)}`.
6. OPTIONAL PROPS FOR STANDALONE MOUNTING: `Navbar` is mounted in `RootLayout` as `<Navbar />` with no arguments. All props in `NavbarProps` MUST be optional (`user?: ... | null`) with safe default parameters (`const Navbar: React.FC<NavbarProps> = ({ user = null } = {}) =>`)."#;

pub const UI_FOOTER_RULEPACK: &str = r#"SCOPED FOOTER LAYOUT & COMPACTNESS RULES:
1. BAN TALL VERTICAL LINK STACKS: NEVER stack all footer links vertically in a single centered column (`flex-col items-center space-y-4` is STRICTLY FORBIDDEN because it creates a giant dark block occupying half the viewport!).
2. COMPACT HORIZONTAL / MULTI-COLUMN FOOTER:
   - Use `<footer className="mt-auto w-full bg-slate-900 text-slate-400 border-t border-slate-800 py-8">`.
   - Inside `<div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col md:flex-row items-center justify-between gap-6">`:
     a) Left side: Compact Brand Lockup (small gradient rounded-lg icon + bold white brand name) and `<p className="text-xs text-slate-500 mt-1">&copy; {new Date().getFullYear()} Brand. All rights reserved.</p>`.
     b) Right side: `<nav className="flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm">` so all footer links sit horizontally in a single clean, compact row (`text-slate-400 hover:text-white transition-colors`)."#;

pub const UI_PRIMITIVES_RULEPACK: &str = r#"SCOPED UI PRIMITIVES RULES (src/components/ui/*):
1. EXTEND NATIVE HTML ATTRIBUTES: Always extend standard React HTML interfaces (`export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>`, `export interface CardProps extends React.HTMLAttributes<HTMLDivElement>`, `export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement>`) and spread `{...props}` onto the underlying DOM element.
2. SAFE CLASSNAME DEFAULT & ASCHILD DESTRUCTURING: Always default `className = ''` in props destructuring (`({ className = '', children, ...props })`) so template literals never output `"undefined"`. In `button.tsx`, accept `asChild?: boolean` in `ButtonProps` and destructure it out (`{ asChild, variant = 'default', size = 'md', className = '', children, ...props }`) so `asChild` is never passed as an invalid DOM attribute to `<button>`.
3. POLISHED BASE CLASSES:
   - `Button`: `inline-flex items-center justify-center gap-2 rounded-xl font-semibold text-sm px-4 py-2.5 transition-all focus:outline-none disabled:opacity-50` with variants (`default`: `bg-indigo-600 text-white hover:bg-indigo-700 shadow-sm`, `outline`: `border border-slate-200 bg-white text-slate-700 hover:bg-slate-50`, `ghost`: `text-slate-600 hover:bg-slate-100`, `destructive`: `bg-rose-600 text-white hover:bg-rose-700`).
   - `Card`: `bg-white rounded-2xl border border-slate-200/80 shadow-sm` (`CardHeader`: `p-6 pb-3`, `CardTitle`: `text-lg font-bold text-slate-900`, `CardContent`: `p-6 pt-0`).
   - `Skeleton`: `animate-pulse rounded-xl bg-slate-200/80`.
4. MANDATORY DUAL EXPORTS: Export BOTH named and default exports (`export { Button }; export default Button;`)."#;

pub const UI_PAGE_AND_DASHBOARD_RULEPACK: &str = r#"SCOPED PAGE & DASHBOARD PREVIEW RESILIENCE RULES:
1. ZERO AUTH-WALL / ZERO INFINITE LOADING IN PREVIEW (CRITICAL):
   - NEVER block page rendering with `if (!session) return <div>Loading...</div>;` or `if (status === 'unauthenticated') return <Link href="/login">Log in</Link>;` or `if (loading) return <div>Loading...</div>;`!
   - Users inspect and test all pages in Live Preview mode before logging in. Every page (`/dashboard`, `/dashboard/projects`, `/dashboard/analytics`, `/dashboard/integrations`, `/dashboard/pricing`, `/dashboard/subscription`, etc.) MUST immediately render its complete, interactive UI with rich domain data even when `session` is null or unauthenticated!
2. MANDATORY RICH INITIAL MOCK STATE (`INITIAL_MOCK_DATA`):
   - NEVER initialize domain lists, metrics, or cards with an empty array `useState([])` or `useState(null)`!
   - Always define a module-level constant (e.g. `const INITIAL_MOCK_PROJECTS: Project[] = [...]`) containing 4 to 6 realistic, domain-specific items (with realistic titles, descriptions, status badges, dates, and metrics tailored to the project's business domain) and initialize state with it: `const [projects, setProjects] = useState<Project[]>(INITIAL_MOCK_PROJECTS);`.
3. UNCONDITIONAL FETCH WITH NON-EMPTY GUARD:
   - Inside `useEffect`, do NOT wrap `fetch('/api/...')` inside `if (session)`! Fetch unconditionally on mount, and only overwrite the initial mock state if the API returns a non-empty array or valid object:
     ```tsx
     useEffect(() => {
       fetch('/api/projects')
         .then((res) => (res.ok ? res.json() : null))
         .then((data) => {
           if (Array.isArray(data) && data.length > 0) setProjects(data);
         })
         .catch(() => {});
     }, []);
     ```
4. INTERACTIVE OPTIMISTIC CRUD WORKFLOWS:
   - When the user clicks "New / Add / Create" or "Delete" or toggles a status on the page, provide an interactive modal/form or optimistic state update (`setItems((prev) => [newItem, ...prev])` and `setItems((prev) => prev.filter((i) => i.id !== id))`) alongside the `fetch` call so the UI responds instantaneously in Live Preview!"#;

pub const UI_ANALYTICS_AND_CHARTS_RULEPACK: &str = r#"SCOPED ANALYTICS, KPI & DATA VISUALIZATION RULES:
1. PRE-POPULATED CHART STATE (NEVER EMPTY):
   - NEVER initialize chart state with empty `labels: []` or `datasets: []`!
   - Always initialize `useState` with 6 realistic time-series labels (e.g. `['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun']`) and 1-2 complete datasets with realistic domain numbers and polished colors (`borderColor: '#4f46e5'`, `backgroundColor: 'rgba(79, 70, 229, 0.15)'`, `borderWidth: 2`, `tension: 0.35`, `fill: true`).
2. EXECUTIVE ANALYTICS DASHBOARD STRUCTURE:
   - Top Section: A 4-card KPI Summary Grid (`grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6`) displaying key domain metrics (large bold value `text-3xl font-extrabold text-slate-900`, positive/negative trend badge `+18.4%` in `bg-emerald-50 text-emerald-700 px-2.5 py-0.5 rounded-full text-xs font-semibold`, and a domain icon inside a soft tinted badge `w-11 h-11 rounded-xl bg-indigo-50 text-indigo-600 flex items-center justify-center`).
   - Main Visualization Section: Wrap charts inside a clean card (`bg-white rounded-2xl border border-slate-200/80 shadow-sm p-6`) with a section title, subtitle, and interactive time-range filter pills (`7D`, `30D`, `6M`, `1Y`)."#;

pub fn resolve_design_archetype_prompt(project_name: &str, project_description: &str, category: &str) -> String {
    let combined = format!("{} {} {}", project_name, project_description, category).to_lowercase();

    let (archetype_name, bg_surface, card_surface, primary_cta, badge_style, aesthetic_notes) = if combined.contains("crypto")
        || combined.contains("blockchain")
        || combined.contains("cyber")
        || combined.contains("security")
        || combined.contains("trading")
        || combined.contains("terminal")
        || combined.contains("devops")
    {
        (
            "FinTech / Cyber / High-Tech Dark Glassmorphism",
            "bg-slate-950 text-slate-100",
            "bg-slate-900/80 backdrop-blur-md border border-slate-800 rounded-2xl p-6 shadow-xl hover:border-indigo-500/40 transition-all",
            "inline-flex items-center justify-center gap-2 bg-gradient-to-r from-indigo-600 to-violet-600 hover:from-indigo-500 hover:to-violet-500 text-white font-semibold px-4 py-2.5 rounded-xl shadow-lg shadow-indigo-500/20 transition-all",
            "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 px-2.5 py-0.5 rounded-full text-xs font-semibold",
            "Use sleek dark surfaces, subtle glowing borders, high-contrast metric typography, and cyan/indigo/emerald accents."
        )
    } else if combined.contains("ecommerce")
        || combined.contains("e-commerce")
        || combined.contains("marketplace")
        || combined.contains("shop")
        || combined.contains("store")
        || combined.contains("booking")
        || combined.contains("hotel")
        || combined.contains("real estate")
        || combined.contains("restaurant")
    {
        (
            "Conversion Marketplace & Retail (Airbnb / Shopify Aesthetic)",
            "bg-stone-50 text-slate-900",
            "bg-white rounded-2xl border border-stone-200/90 shadow-sm hover:shadow-lg transition-all duration-200 overflow-hidden p-6",
            "inline-flex items-center justify-center gap-2 bg-rose-600 hover:bg-rose-700 text-white font-semibold px-4 py-2.5 rounded-xl shadow-sm transition-all",
            "bg-amber-50 text-amber-800 border border-amber-200 px-2.5 py-0.5 rounded-full text-xs font-semibold",
            "Emphasize visual product/listing cards, clear pricing badges, category filter pills, and warm inviting accents."
        )
    } else if combined.contains("health")
        || combined.contains("medical")
        || combined.contains("clinic")
        || combined.contains("patient")
        || combined.contains("wellness")
        || combined.contains("edtech")
        || combined.contains("education")
        || combined.contains("learning")
        || combined.contains("school")
    {
        (
            "Calm & Trustworthy HealthTech / EdTech System",
            "bg-slate-50 text-slate-900",
            "bg-white rounded-2xl border border-teal-100/90 shadow-sm hover:shadow-md transition-all p-6",
            "inline-flex items-center justify-center gap-2 bg-teal-600 hover:bg-teal-700 text-white font-semibold px-4 py-2.5 rounded-xl shadow-sm transition-all",
            "bg-teal-50 text-teal-700 border border-teal-200 px-2.5 py-0.5 rounded-full text-xs font-semibold",
            "Use clean, calm teal/sky/emerald palettes, clear progress bars, accessible contrast, and structured schedule/course cards."
        )
    } else {
        (
            "Executive B2B SaaS & Collaborative Workspace (Linear / Stripe / Vercel Aesthetic)",
            "bg-slate-50 text-slate-900",
            "bg-white rounded-2xl border border-slate-200/80 shadow-sm hover:shadow-md transition-all duration-200 p-6",
            "inline-flex items-center justify-center gap-2 bg-indigo-600 hover:bg-indigo-700 text-white font-semibold px-4 py-2.5 rounded-xl shadow-sm transition-colors",
            "bg-indigo-50 text-indigo-700 border border-indigo-200/60 px-2.5 py-0.5 rounded-full text-xs font-semibold",
            "Use crisp white cards over a soft slate-50 canvas, subtle borders, status pills (emerald for active/completed, amber for in-progress, indigo for primary tags), and clean typographic hierarchy."
        )
    };

    format!(
        "APPLICATION DESIGN SYSTEM ARCHETYPE ({}):\n\
         - Page Canvas / Background: `{}`\n\
         - Card & Surface Style: `{}`\n\
         - Primary Action Button Style: `{}`\n\
         - Status / Tag Badge Style: `{}`\n\
         - Visual Direction: {}\n\n",
        archetype_name, bg_surface, card_surface, primary_cta, badge_style, aesthetic_notes
    )
}

pub fn get_project_design_archetype_from_dir(proj_dir: &std::path::Path) -> String {
    let mut proj_name = String::new();
    let mut proj_desc = String::new();
    let mut category = String::new();

    let cp_path = proj_dir.join(".axiom").join("generation_state.json");
    if cp_path.exists() {
        if let Ok(content) = fs::read_to_string(&cp_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                proj_name = val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                proj_desc = val.get("projectDescription").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
        }
    }

    let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
    if lbg_path.exists() {
        if let Ok(content) = fs::read_to_string(&lbg_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                if let Some(cat) = val.get("projectSpecification").and_then(|s| s.get("category")).and_then(|c| c.as_str()) {
                    category = cat.to_string();
                }
            }
        }
    }

    resolve_design_archetype_prompt(&proj_name, &proj_desc, &category)
}

pub fn get_scoped_ui_rules(file_path: &str) -> String {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let mut ui_rules = Vec::new();

    ui_rules.push(UI_DESIGN_FOUNDATION_RULEPACK);

    if path_lower.contains("navbar") || path_lower.contains("header") || path_lower.contains("sidebar") || path_lower.contains("/nav") {
        ui_rules.push(UI_NAVBAR_RULEPACK);
    }

    if path_lower.contains("footer") {
        ui_rules.push(UI_FOOTER_RULEPACK);
    }

    if path_lower.contains("components/ui/") {
        ui_rules.push(UI_PRIMITIVES_RULEPACK);
    }

    let is_page_or_domain_view = path_lower.ends_with("page.tsx")
        || path_lower.ends_with("page.jsx")
        || path_lower.contains("dashboard")
        || path_lower.contains("project")
        || path_lower.contains("task")
        || path_lower.contains("integration")
        || path_lower.contains("team")
        || path_lower.contains("subscription")
        || path_lower.contains("pricing");

    if is_page_or_domain_view && !path_lower.contains("/login") && !path_lower.contains("/register") {
        ui_rules.push(UI_PAGE_AND_DASHBOARD_RULEPACK);
    }

    if path_lower.contains("analytics") || path_lower.contains("chart") || path_lower.contains("metric") {
        ui_rules.push(UI_ANALYTICS_AND_CHARTS_RULEPACK);
    }

    ui_rules.join("\n\n")
}

pub fn get_stack_rulepacks(file_path: &str, file_content: &str, error_msg: Option<&str>) -> String {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let err_lower = error_msg.unwrap_or("").to_lowercase();
    let content_lower = file_content.to_lowercase();

    let mut rules = Vec::new();

    // 1. Mobile (React Native / Expo) - check first to avoid web DOM confusion
    let is_react_native = path_lower.contains("react-native")
        || path_lower.contains("expo")
        || content_lower.contains("from 'react-native'")
        || content_lower.contains("from \"react-native\"")
        || content_lower.contains("from 'expo")
        || content_lower.contains("stylesheet.create")
        || err_lower.contains("view config not found");

    if is_react_native {
        rules.push(REACT_NATIVE_EXPO_RULEPACK);
    } else {
        // 2. React & Next.js Web Stack
        let is_react_file = path_lower.ends_with(".tsx")
            || path_lower.ends_with(".jsx")
            || path_lower.contains("/app/")
            || path_lower.contains("/pages/")
            || path_lower.contains("/components/")
            || content_lower.contains("'use client'")
            || content_lower.contains("\"use client\"")
            || content_lower.contains("from 'react'")
            || content_lower.contains("from \"react\"")
            || content_lower.contains("usestate")
            || content_lower.contains("useeffect")
            || content_lower.contains("usecontext")
            || content_lower.contains("usetranslations")
            || content_lower.contains("usesession")
            || err_lower.contains("hook")
            || err_lower.contains("react context")
            || err_lower.contains("the `style` prop expects a mapping")
            || err_lower.contains("rendered more hooks");

        let is_api_or_route_handler = path_lower.contains("/api/")
            || path_lower.ends_with("route.ts")
            || path_lower.ends_with("route.js");

        if is_react_file && !is_api_or_route_handler {
            rules.push(NEXTJS_REACT_RULEPACK);
            rules.push(UI_DESIGN_FOUNDATION_RULEPACK);

            if path_lower.contains("navbar") || path_lower.contains("header") || path_lower.contains("sidebar") || path_lower.contains("/nav") {
                rules.push(UI_NAVBAR_RULEPACK);
            }
            if path_lower.contains("footer") {
                rules.push(UI_FOOTER_RULEPACK);
            }
            if path_lower.contains("components/ui/") {
                rules.push(UI_PRIMITIVES_RULEPACK);
            }
            if path_lower.ends_with("page.tsx") || path_lower.ends_with("page.jsx") || path_lower.contains("dashboard") {
                rules.push(UI_PAGE_AND_DASHBOARD_RULEPACK);
            }
            if path_lower.contains("analytics") || path_lower.contains("chart") || path_lower.contains("metric") {
                rules.push(UI_ANALYTICS_AND_CHARTS_RULEPACK);
            }
        }

        // 3. Next-Intl (Internationalization)
        let is_next_intl = path_lower.contains("i18n")
            || path_lower.contains("messages/")
            || content_lower.contains("next-intl")
            || content_lower.contains("usetranslations")
            || err_lower.contains("nextintl")
            || err_lower.contains("usetranslations");

        if is_next_intl {
            rules.push(NEXT_INTL_RULEPACK);
        }

        // 4. Next.js Link
        let is_next_link = content_lower.contains("from 'next/link'")
            || content_lower.contains("from \"next/link\"")
            || content_lower.contains("<link")
            || err_lower.contains("invalid <link>")
            || err_lower.contains("legacybehavior");

        if is_next_link {
            rules.push(NEXT_LINK_RULEPACK);
        }
    }

    // 5. MERN Stack (Express Backend + Vite / React Router)
    let is_mern = path_lower.contains("server.js")
        || path_lower.contains("/routes/")
        || path_lower.contains("/models/")
        || content_lower.contains("express()")
        || content_lower.contains("from 'express'")
        || content_lower.contains("require('express')")
        || content_lower.contains("from 'react-router-dom'")
        || content_lower.contains("from \"react-router-dom\"")
        || err_lower.contains("cannot set headers");

    if is_mern {
        rules.push(MERN_STACK_RULEPACK);
    }

    // 6. Desktop (Electron & Tauri)
    let is_desktop = path_lower.contains("preload.js")
        || path_lower.contains("main.js")
        || path_lower.contains("electron")
        || path_lower.contains("src-tauri")
        || content_lower.contains("contextbridge")
        || content_lower.contains("ipcmain")
        || content_lower.contains("ipcrenderer")
        || content_lower.contains("#[tauri::command]")
        || err_lower.contains("contextisolation");

    if is_desktop {
        rules.push(DESKTOP_RULEPACK);
    }

    // 7. Prisma & Database Resiliency (strictly scoped to database/backend files or explicit prisma errors)
    let is_ui_component_or_page = (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx")) && !path_lower.contains("/api/");
    let is_prisma = path_lower.ends_with(".prisma")
        || path_lower.contains("/api/")
        || path_lower.contains("route.ts")
        || path_lower.contains("route.js")
        || (!is_ui_component_or_page && (content_lower.contains("@prisma/client") || content_lower.contains("prisma.")))
        || err_lower.contains("prisma");

    if is_prisma {
        rules.push(PRISMA_DATABASE_RULEPACK);
    }

    // 8. Configuration & Project Config Rules
    let is_config = path_lower.ends_with(".json")
        || path_lower.contains("config")
        || path_lower.ends_with("tsconfig.json")
        || err_lower.contains("tsconfig")
        || err_lower.contains("debug failure");

    if is_config {
        rules.push(CONFIG_FILES_RULEPACK);
    }

    // 9. Next.js Middleware & Edge Routing Rules
    let is_middleware = path_lower.contains("middleware.ts")
        || path_lower.contains("middleware.js")
        || content_lower.contains("nextrequest")
        || content_lower.contains("nextresponse")
        || content_lower.contains("withauth")
        || err_lower.contains("invalid middlewares found")
        || err_lower.contains("invalid-route-source")
        || err_lower.contains("unexpected modifier")
        || err_lower.contains("middleware");

    if is_middleware {
        rules.push(NEXTJS_MIDDLEWARE_RULEPACK);
    }

    if rules.is_empty() {
        String::new()
    } else {
        format!("\n\n=== STRICT STACK & LIBRARY CONSTRAINTS ===\n{}\n==========================================\n", rules.join("\n\n"))
    }
}

pub fn get_expert_system_prompt(
    file_path: &str,
    project_type: &str,
    _overrides_desc: &str,
    _pkg_db_summary: &str,
    context_ref: &str,
    routes_ref: &str,
    proj_dir: &std::path::Path,
) -> String {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let proj_type_lower = project_type.to_lowercase();

    // 1. RUST & TAURI SYSTEM SCOPE (.rs, Cargo.toml)
    if path_lower.ends_with(".rs") {
        return format!(
            "You are a Senior Rust Systems Engineer. Write only clean, production-ready Rust code wrapped in markdown code blocks. No explanations.\n\n\
             RUST IMPLEMENTATION RULES:\n\
             - Write idiomatic, memory-safe Rust adhering to ownership, borrowing, and lifetime rules.\n\
             - Implement robust error handling with Result<T, E> and Option<T>, avoiding bare unwrap() in production paths.\n\
             - Use serde::{{Serialize, Deserialize}} for data structures requiring serialization: annotate structs with #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)].\n\
             - If this file defines Tauri commands, annotate handlers with #[tauri::command] and return clean Result<T, String> types mapped via .map_err(|e| e.to_string()).\n\
             - NO MUTEXGUARD ACROSS .AWAIT: Never hold a std::sync::MutexGuard across an .await boundary. Use tokio::sync::Mutex or drop the guard before awaiting.\n\
             - TAURI V2 EVENTS: When emitting events via window.emit() or app_handle.emit(), always import `use tauri::Emitter;` at the top of the file.\n\
             - NON-BLOCKING WORKERS: Offload CPU-heavy or blocking filesystem operations using tokio::task::spawn_blocking.\n\
             - STRICTLY NO JavaScript, TypeScript, HTML, CSS, or Node.js imports in Rust code.\n\
             FILE: {}\n",
            file_path
        );
    }

    if path_lower.ends_with("cargo.toml") {
        return "You are an expert Rust systems architect. Write only valid, production-ready Cargo.toml code wrapped in markdown code blocks. No explanations.\n\n\
                CARGO.TOML RULES:\n\
                - Define valid [package] metadata with edition = \"2021\".\n\
                - Specify verified, stable crate versions under [dependencies] (e.g. serde, tokio, tauri, etc.).\n\
                - NEVER output npm, package.json, or JavaScript syntax in Cargo.toml.\n".to_string();
    }

    // 2. FLUTTER & DART MOBILE SCOPE (.dart, pubspec.yaml)
    if path_lower.ends_with(".dart") {
        return format!(
            "You are a Senior Flutter & Dart Mobile Developer. Write only clean, production-ready Dart code wrapped in markdown code blocks. No explanations.\n\n\
             FLUTTER RULES:\n\
             - Write clean StatelessWidget or StatefulWidget implementations using Material 3 or Cupertino widgets.\n\
             - Structure mobile layouts using Column, Row, ListView, Container, Padding, and SizedBox.\n\
             - Strictly NO HTML DOM tags (no <div>, <span>, <p>, <a>), no React hooks, and no web CSS.\n\
             - Adhere to Dart sound null-safety standards.\n\
             FILE: {}\n",
            file_path
        );
    }

    if path_lower.ends_with("pubspec.yaml") {
        return "You are an expert Flutter architect. Write only valid pubspec.yaml code wrapped in markdown code blocks. No explanations.\n\n\
                PUBSPEC RULES:\n\
                - Specify valid name, description, version, and environment sdk: \">=3.0.0 <4.0.0\".\n\
                - Include flutter sdk dependency under dependencies and flutter: uses-material-design: true.\n".to_string();
    }

    // 3. REACT NATIVE / EXPO MOBILE SCOPE
    let is_mobile_rn = proj_type_lower == "react-native" || proj_type_lower == "expo" || path_lower.contains("app.json");
    if is_mobile_rn && (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx")) {
        return format!(
            "You are a Senior React Native Mobile Engineer. Write only clean, production-ready React Native / Expo code wrapped in markdown code blocks. No explanations.\n\n\
             REACT NATIVE RULES:\n\
             - MOBILE UI ONLY: NEVER use HTML DOM tags (no <div>, <p>, <span>, <a>, <button>, or <html>). ALWAYS use React Native core components (<View>, <Text>, <TouchableOpacity>, <ScrollView>, <FlatList>, <TextInput>, <SafeAreaView>).\n\
             - NO CSS STRINGS OR BOXSHADOW: Never write `border: \"1px solid ...\"` or `boxShadow`. Use individual border properties (`borderWidth: 1, borderColor: '#...'`) and elevation (Android) / shadow* (iOS).\n\
             - SCROLL CONTAINER STYLING: Always use `contentContainerStyle={{ padding: 16 }}` on ScrollView and FlatList for inner padding, not `style`.\n\
             - ICONS: NEVER import from 'lucide-react' or 'react-icons'. Import from '@expo/vector-icons' (e.g. Ionicons) or 'lucide-react-native'.\n\
             - NAVIGATION: Use Expo Router (`import {{ useRouter }} from 'expo-router'`) or React Navigation. Never use Next.js Link or router.\n\
             - OFFLINE RESILIENCE: Wrap fetch calls in try/catch and provide fallback data or cached AsyncStorage state.\n\
             - CLIENT-SIDE ONLY: Never import @prisma/client or Node.js server modules.\n\
             FILE: {}\n",
            file_path
        );
    }

    // 4. FILE-SCOPED PROMPT: Styles & CSS (.css, .module.css, .scss)
    if path_lower.ends_with(".css") || path_lower.ends_with(".scss") {
        return format!(
            "You are an expert CSS / Styling Engineer. Write only valid, production-ready CSS code wrapped in markdown code blocks. No explanations.\n\n\
             ROLE: Senior UI/UX & CSS Specialist\n\
             STYLING RULES:\n\
             - Write clean, modern, responsive CSS rules with flexbox, grid, smooth transitions, and accessible colors.\n\
             - If this is a CSS module (e.g. *.module.css), define clean class selectors (camelCase or kebab-case) matching component usage.\n\
             - Do NOT import JavaScript, TypeScript, Prisma, or backend libraries.\n\
             - Do NOT generate empty placeholders; provide full, polished styling rules.\n"
        );
    }

    // 5. FILE-SCOPED PROMPT: Web Configuration files
    if path_lower.ends_with("postcss.config.js") || path_lower.ends_with("postcss.config.mjs") {
        return "You are an expert build configuration engineer. Write only valid postcss.config.js code wrapped in markdown code blocks. No explanations.\n\nmodule.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n};\n".to_string();
    }

    if path_lower.ends_with("tailwind.config.js") || path_lower.ends_with("tailwind.config.ts") {
        return "You are an expert Tailwind CSS configuration engineer. Write only valid tailwind.config.js code wrapped in markdown code blocks. No explanations.\n\n/** @type {import('tailwindcss').Config} */\nmodule.exports = {\n  content: [\n    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',\n    './src/components/**/*.{js,ts,jsx,tsx,mdx}',\n    './src/app/**/*.{js,ts,jsx,tsx,mdx}',\n  ],\n  theme: {\n    extend: {},\n  },\n  plugins: [],\n};\n".to_string();
    }

    if path_lower.ends_with("tsconfig.json") {
        return "You are a TypeScript Configuration Specialist. Output only a valid, concise Next.js tsconfig.json JSON object wrapped in markdown code blocks. Maximum 30 lines. No explanations.\n\n{\n  \"compilerOptions\": {\n    \"lib\": [\"dom\", \"dom.iterable\", \"esnext\"],\n    \"allowJs\": true,\n    \"skipLibCheck\": true,\n    \"strict\": false,\n    \"noEmit\": true,\n    \"esModuleInterop\": true,\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\",\n    \"resolveJsonModule\": true,\n    \"isolatedModules\": true,\n    \"jsx\": \"preserve\",\n    \"incremental\": true,\n    \"paths\": {\n      \"@/*\": [\"./src/*\"]\n    }\n  },\n  \"include\": [\"next-env.d.ts\", \"**/*.ts\", \"**/*.tsx\", \".next/types/**/*.ts\"],\n  \"exclude\": [\"node_modules\"]\n}\n".to_string();
    }

    if path_lower.contains("next.config") {
        return "You are a Next.js Configuration Expert. Write only valid next.config.js code wrapped in markdown code blocks. No explanations.\n\n/** @type {import('next').NextConfig} */\nconst nextConfig = {\n  reactStrictMode: true,\n};\n\nmodule.exports = nextConfig;\n".to_string();
    }

    // Read Live Builder Guide contracts if available
    let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
    let lbg_val: Option<Value> = if lbg_path.exists() {
        fs::read_to_string(&lbg_path).ok().and_then(|c| serde_json::from_str::<Value>(&c).ok())
    } else {
        None
    };

    // 6. FILE-SCOPED PROMPT: Database & Prisma Schema
    if path_lower.contains("schema.prisma") {
        let mut prompt = "You are a Senior Database Architect. Write only the complete, valid schema.prisma code wrapped in markdown code blocks. No explanations.\n\n\
             DATABASE DESIGN RULES:\n\
             - Focus on relational integrity, clean schema definitions, and proper Prisma model annotations.\n\
             - When writing tables, ensure IDs use `@id @default(cuid())` or similar suitable auto-generators.\n\
             - Make sure passwords or security token fields (e.g. `hashedPassword`) are defined as optional `String?` so that OAuth/Magic Link users can be saved without password errors.\n\
             - Always include necessary relation annotations cleanly and explicitly.\n\
             - PRISMA RELATION RULES:\n\
               1. In a one-to-many relation, the `@relation` attribute (specifying `fields` and `references`) MUST ONLY be defined on the model that contains the actual foreign key column (the \"many\" side). The parent model (the \"one\" side, returning an array `Model[]`) MUST NOT specify any `fields` or `references` in its `@relation` attribute.\n\
                2. Every relation must have an opposite relation field defined on both models. For example, if model `Message` has `sender User @relation(fields: [senderId], references: [id])`, then model `User` MUST have `messages Message[]`. If `@relation` has a name (e.g. `name: \"TeamMember\"`), the opposite field MUST specify the same name: `@relation(\"TeamMember\")`.\n\
               3. For implicit many-to-many relations, do NOT specify any `fields` or `references` arguments.\n\
               4. The `@relation` attribute MUST ONLY be placed on relational object fields, never on scalar fields.\n\
               5. For NextAuth schema, the VerificationToken model does not have a direct relation back to the User model.\n\
                6. CONSTANTS & ENUMS: Fixed sets of values (such as roles, statuses, priorities, tiers) MUST be defined using Prisma `enum Name { VAL1 VAL2 }`, NEVER as a relational `model`. Never place `@default(...)` on a relation field pointing to another model.\n".to_string();

        if let Some(ref lbg) = lbg_val {
            if let Some(db_models) = lbg.get("sharedContracts").and_then(|c| c.get("database")).and_then(|d| d.get("models")) {
                prompt.push_str(&format!("\nREQUIRED DATABASE MODELS CONTRACT:\n{}\n", db_models));
            }
        }
        return prompt;
    }

    // 7. FILE-SCOPED PROMPT: Backend API Routes & NextAuth
    let is_api_route = path_lower.contains("/api/") || path_lower.contains("route.ts") || path_lower.contains("route.js");
    if is_api_route {
        let mut prompt = "You are a Senior Backend Developer. Write only clean, production-ready Next.js Route Handler code wrapped in markdown code blocks. No explanations.\n\n\
             BACKEND API DEVELOPMENT RULES:\n\
             - Implement clean Next.js App Router Route Handlers (exporting GET, POST, PUT, DELETE functions).\n\
             - Wrap database calls in robust try-catch blocks. In GET handlers, if the database query returns an empty array `[]` or throws an error in catch, return a rich fallback array of 4-6 realistic domain mockup objects so the frontend preview is immediately populated with realistic data.\n\
             - GRACEFUL POST/PUT VALIDATION & DEFAULTS: If using Zod (`z.object`), ALWAYS include `import { z } from 'zod';` at the top of the file! Make optional or secondary fields (such as `timeline`, `status`, `startDate`, `endDate`, `description`) `.optional().default(...)` so quick-create requests from the UI succeed without throwing 500/400 validation errors. In catch blocks of POST/PUT handlers, return a 201/200 fallback response with `{ id: 'mock-' + Date.now(), ...body }` so UI creation flows always work in local preview mode.\n\
             - STRICT IMPORT RESOLUTION: Every external dependency ('zod', 'bcryptjs', '@/lib/prisma', 'next/server') MUST be explicitly imported at the top.\n".to_string();

        if path_lower.contains("/api/auth/") || path_lower.contains("nextauth") {
            prompt.push_str(
                "\nAUTHENTICATION SECURITY & HYBRID PROVIDER RULES (SCOPED TO AUTH ROUTE):\n\
                 - Export GET and POST handlers: `const handler = NextAuth(authOptions); export { handler as GET, handler as POST };`.\n\
                 - Use `strategy: 'jwt'` sessions for NextAuth.\n\
                 - HYBRID DEV/PROD PROVIDERS: Dynamically use `CredentialsProvider` (email+password verified with `bcryptjs`) during development (`process.env.NODE_ENV !== 'production'`), and support `EmailProvider` (Resend SMTP) in production (`process.env.NODE_ENV === 'production'`) ONLY when `process.env.EMAIL_SERVER_HOST` or `process.env.RESEND_API_KEY` is configured, falling back to `CredentialsProvider` if SMTP keys are missing.\n\
                 - CredentialsProvider MUST be imported from 'next-auth/providers/credentials', NEVER from 'next-auth' directly.\n\
                 - Never hardcode environment secrets (use `process.env.NEXTAUTH_SECRET || 'dev-secret'`).\n"
            );
        }

        if path_lower.contains("stripe") || path_lower.contains("webhook") || path_lower.contains("checkout") {
            prompt.push_str(
                "\nPAYMENTS & STRIPE DOMAIN RULES (SCOPED TO BILLING API):\n\
                 - Guard Stripe initialization if `process.env.STRIPE_SECRET_KEY` is missing in local preview, returning a graceful mock/preview JSON response instead of crashing.\n\
                 - Verify webhook signatures cleanly when `STRIPE_WEBHOOK_SECRET` is present.\n"
            );
        }

        if let Some(ref lbg) = lbg_val {
            if let Some(api_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("api")) {
                prompt.push_str(&format!("\nAPI ENDPOINTS CONTRACT:\n{}\n", api_contracts));
            }
            if let Some(db_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("database")) {
                prompt.push_str(&format!("\nDATABASE MODELS REFERENCE:\n{}\n", db_contracts));
            }
        }
        if !context_ref.is_empty() {
            prompt.push_str(context_ref);
        }
        return prompt;
    }

    // 7b. FILE-SCOPED PROMPT: Next.js Edge Middleware (middleware.ts / middleware.js)
    if path_lower.ends_with("middleware.ts") || path_lower.ends_with("middleware.js") {
        return format!(
            "You are a Senior Next.js Edge Architecture Engineer. Write only clean, production-ready Next.js Middleware code wrapped in markdown code blocks. No explanations.\n\n{}\n\
             - NEVER import from 'express' (no Request, Response, NextFunction from 'express'). Always import `{{ NextResponse }}` and `type {{ NextRequest }}` from `'next/server'`.\n\
             - Always export `function middleware(request: NextRequest)` and `export const config = {{ matcher: [...] }}`.\n",
            NEXTJS_MIDDLEWARE_RULEPACK
        );
    }

    // 8. FILE-SCOPED PROMPT: Web Client UI Components, Pages & Layouts (.tsx, .jsx)
    // Strictly isolated from backend Prisma schemas and NextAuth server internals!
    let is_react_frontend = path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx");
    if is_react_frontend {
        let mut prompt = "You are a Principal React & Tailwind CSS Product Designer and Frontend Engineer. Write only clean, complete, production-ready React code wrapped in markdown code blocks. No explanations.\n\n".to_string();

        // Inject tailored Design Archetype tokens based on the project's domain & category
        let design_archetype = get_project_design_archetype_from_dir(proj_dir);
        prompt.push_str(&design_archetype);

        if path_lower.ends_with("layout.tsx") || path_lower.ends_with("layout.jsx") {
            prompt.push_str(
                "ROOT LAYOUT ARCHITECTURE RULES:\n\
                 - DOM HIERARCHY: Outermost elements returned MUST be `<html lang=\"en\"><body className=\"min-h-screen flex flex-col bg-slate-50 text-slate-900 antialiased selection:bg-indigo-600 selection:text-white\">...</body></html>`.\n\
                 - GLOBAL PROVIDERS: Wrap all global providers inside `<body>` around `{children}`. If NextAuth is configured, wrap with `<SessionProvider>`.\n\
                 - PERSISTENT NAVIGATION & STICKY FOOTER: Always import and mount `<Navbar />` (from `@/components/Navbar`) at the top, wrap `{children}` in `<main className=\"flex-1 flex flex-col w-full\">{children}</main>`, and mount `<Footer />` (from `@/components/Footer`) at the bottom.\n\
                 - MANDATORY DEFAULT EXPORT: RootLayout MUST have `export default function RootLayout({ children }: { children: React.ReactNode })`.\n\
                 - Mark the layout file with 'use client'; at line 1.\n\n"
            );
        } else if path_lower == "src/app/page.tsx" || path_lower == "src/app/page.jsx" || path_lower == "app/page.tsx" || path_lower == "app/page.jsx" {
            prompt.push_str(
                "ROOT LANDING PAGE DESIGN RULES:\n\
                 - This is the HOME/LANDING PAGE (`/`). It must NEVER be a blank screen or bare login form.\n\
                 - HERO SECTION: Include a high-impact Hero banner with a badge pill, bold headline (`text-4xl sm:text-5xl lg:text-6xl font-extrabold tracking-tight`), engaging value proposition, and clear CTA buttons (`inline-flex items-center justify-center gap-2`) linking to `/dashboard` and key features.\n\
                 - CORE FEATURE SHOWCASE: Display a responsive `grid grid-cols-1 md:grid-cols-3 gap-6` of interactive domain cards, KPI highlights, and workflow previews.\n\
                 - DATA RESILIENCY: Pre-populate state with 4-6 realistic domain mockup items so the page is rich and interactive out-of-the-box.\n\n"
            );
        } else {
            prompt.push_str(
                "CORE FRONTEND COMPONENT RULES:\n\
                 - CLIENT-SIDE BOUNDARY: NEVER import `@prisma/client` in React components. Fetch data from `/api/...` endpoints.\n\
                 - STRICT TYPE DEFINITIONS & IMPORTS: Declare TypeScript interfaces cleanly inside the file. Import icons ONLY from valid subpaths (`'react-icons/hi'`, `'react-icons/fa'`, `'react-icons/fi'`, `'react-icons/io5'`, or `'lucide-react'`), NEVER from bare `'react-icons'`.\n\
                 - NEXT.JS LINK RULE: Do NOT place `<a>` tags inside `<Link>`. Put `className` and `prefetch={false}` directly on `<Link>`.\n\
                 - NO DEAD IMPORTS: Do not import external local modules unless they exist in the project; prefer clean self-contained components or verified `@/components/ui/*` primitives.\n\
                 - DUAL EXPORT COMPATIBILITY: Export the component as both a named export (`export { ComponentName }`) and default export (`export default ComponentName`).\n\
                 - NO MANUAL AXIOM ATTRS: Never declare or use 'data-axiom-component' or 'data-axiom-file' props.\n\n"
            );
        }

        // Inject surgical, role-specific UI rulepacks for this exact file
        let scoped_ui = get_scoped_ui_rules(file_path);
        if !scoped_ui.is_empty() {
            prompt.push_str(&scoped_ui);
            prompt.push_str("\n\n");
        }

        if path_lower.contains("navbar") || path_lower.contains("sidebar") || path_lower.contains("footer") || path_lower.contains("header") || path_lower.contains("/nav") {
            prompt.push_str(
                "SCOPED NAVIGATION ROUTE INTEGRITY RULES:\n\
                 - ROUTE CONSISTENCY: Every `<Link>` and navigation item in this component MUST strictly point to a verified route from the VERIFIED APPLICATION ROUTES list below.\n\
                 - ABSOLUTE SUBPATHS: Never strip parent folders from nested routes (e.g. use `/dashboard/projects`, NOT `/projects`). Unmapped or flattened routes cause fatal 404 errors!\n\
                 - ACTIVE STATE HIGHLIGHT: Use `usePathname()` from 'next/navigation' to detect active route and visually highlight current link.\n\n"
            );
        }

        // Scoped Auth UI Rules (ONLY for /login, /register, /signup, or auth hooks — never for general UI!)
        if path_lower.contains("/login") || path_lower.contains("/register") || path_lower.contains("/signup") || path_lower.contains("useauth") {
            prompt.push_str(
                "SCOPED AUTH UI RULES (ACTIVE ONLY FOR AUTH FILES):\n\
                 - Ensure the Login/Register UI works out-of-the-box locally without any SMTP server configuration.\n\
                 - Render a polished card (`max-w-md w-full mx-auto bg-white rounded-2xl border border-slate-200 shadow-lg p-8`) with the inline SVG Brand Lockup at the top, email + password inputs (`signIn('credentials', { email, password, redirect: false })`), clear validation feedback, and a link to `/dashboard`.\n\
                 - If defining an auth hook or provider (e.g. `useAuth`), export BOTH named (`export { useAuth }`) and default (`export default useAuth`) exports.\n\n"
            );
        }

        if path_lower.contains("pricing") || path_lower.contains("checkout") || path_lower.contains("billing") || path_lower.contains("subscription") {
            prompt.push_str(
                "SCOPED BILLING & PRICING UI RULES (ACTIVE ONLY FOR PRICING/SUBSCRIPTION FILES):\n\
                 - Display a responsive 3-tier pricing comparison grid (`Starter`, `Pro / Business` highlighted with a `Most Popular` badge and `border-2 border-indigo-600 shadow-xl`, and `Enterprise`), a Monthly/Annual billing cycle toggle with discount badge, feature checklists with check icons, and interactive plan selection buttons with immediate visual feedback.\n\n"
            );
        }

        if let Some(ref lbg) = lbg_val {
            if let Some(spec) = lbg.get("projectSpecification") {
                if let Some(style) = spec.get("importExportStyle").and_then(|s| s.as_str()) {
                    prompt.push_str(&format!("- IMPORT/EXPORT RULE: Use {} exports.\n", style));
                }
                if let Some(reqs) = spec.get("categoryRequirements") {
                    if let Some(principles) = reqs.get("designPrinciples").and_then(|p| p.as_array()) {
                        prompt.push_str("- Design Principles:\n");
                        for p in principles {
                            if let Some(ps) = p.as_str() { prompt.push_str(&format!("  * {}\n", ps)); }
                        }
                    }
                }
            }
            if let Some(api_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("api")).and_then(|a| a.get("endpoints")) {
                prompt.push_str(&format!("- Available API Endpoints to Fetch:\n{}\n", api_contracts));
            }
        }

        if !routes_ref.trim().is_empty() {
            prompt.push_str(routes_ref);
            prompt.push_str("\n");
        } else {
            let disk_routes = get_project_routes_from_dir(proj_dir);
            let formatted = format_routes_prompt(&disk_routes);
            if !formatted.is_empty() {
                prompt.push_str(&formatted);
                prompt.push_str("\n");
            }
        }

        if !context_ref.is_empty() {
            prompt.push_str(context_ref);
        }
        return prompt;
    }

    // 9. GENERAL UTILITY / FALLBACK SCOPE (e.g. src/utils/..., src/lib/...)
    let mut prompt = "You are an expert software engineer. Write only clean, production-ready code wrapped in markdown code blocks. No explanations.\n\n".to_string();
    if proj_type_lower == "rust" || proj_type_lower == "tauri" {
        prompt.push_str(&format!("ROLE: Senior Rust Systems Engineer\nFILE: {}\n", file_path));
    } else if proj_type_lower == "flutter" {
        prompt.push_str(&format!("ROLE: Senior Flutter & Dart Mobile Developer\nFILE: {}\n", file_path));
    } else {
        prompt.push_str(&format!("ROLE: Expert TypeScript / JavaScript Developer\nFILE: {}\n", file_path));
    }
    if !context_ref.is_empty() {
        prompt.push_str(context_ref);
    }
    prompt
}

pub async fn run_orchestrator(
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
        let _ = window.app_handle().emit("task:progress", serde_json::json!({
            "taskId": task_id,
            "projectId": project_id,
            "phase": phase,
            "message": msg,
            "progress": progress
        }));
    };

    let proj_dir = get_project_path(&project_id);
    let _ = fs::create_dir_all(&proj_dir);
    let axiom_dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&axiom_dir);

    let task_control = find_task_control(&task_id)
        .unwrap_or_else(|| register_task_control(&task_id, &project_id));

    let existing_checkpoint = load_generation_checkpoint(&proj_dir);
    let mut completed_files: Vec<String> = Vec::new();
    let mut files_to_generate_from_cp: Option<Vec<Value>> = None;

    if let Some(ref cp) = existing_checkpoint {
        let cp_status = cp.get("status").and_then(|s| s.as_str()).unwrap_or("");
        if cp_status != "completed" {
            if let Some(arr) = cp.get("completedFiles").and_then(|a| a.as_array()) {
                completed_files = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            }
            if let Some(ftg) = cp.get("filesToGenerate").and_then(|f| f.as_array()) {
                if !ftg.is_empty() {
                    files_to_generate_from_cp = Some(ftg.clone());
                    debug_log_to_file(format!(
                        "[Orchestrator] Resuming from checkpoint on disk: {} completed files, {} total files in manifest.",
                        completed_files.len(), ftg.len()
                    ));
                }
            }
        }
    }

    let resume_base_pct: Option<f64> = files_to_generate_from_cp.as_ref().and_then(|ftg| {
        if !completed_files.is_empty() && !ftg.is_empty() {
            Some((25.0 + (completed_files.len() as f64 / ftg.len() as f64) * 55.0).min(99.0))
        } else {
            None
        }
    });

    emit_progress(
        if resume_base_pct.is_some() { "generate-files" } else { "fetch-manifest" },
        if files_to_generate_from_cp.is_some() {
            "Nastavljam generisanje sa sačuvanog checkpoint-a..."
        } else {
            "Initializing generation pipeline..."
        },
        resume_base_pct.unwrap_or(1.0),
    );

    // 1. Fetch Manifest
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(60))
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
    } else if let (Some(ref cp), Some(ref saved_files)) = (&existing_checkpoint, &files_to_generate_from_cp) {
        let cp_name = cp.get("name").and_then(|v| v.as_str()).unwrap_or("Resumed Project");
        let cp_desc = cp.get("projectDescription").and_then(|v| v.as_str()).unwrap_or("");
        let cp_type = cp.get("projectType").and_then(|v| v.as_str()).unwrap_or("nextjs");
        debug_log_to_file(format!(
            "[Orchestrator] Reconstructing manifest directly from local checkpoint for '{}' ({} files).",
            cp_name, saved_files.len()
        ));
        manifest = serde_json::json!({
            "manifestId": manifest_id,
            "platform": "web",
            "projectType": cp_type,
            "resolvedStack": if cp_type == "nextjs" { "Next.js" } else { cp_type },
            "name": cp_name,
            "description": cp_desc,
            "metadata": {
                "name": cp_name,
                "description": cp_desc
            },
            "files": saved_files
        });
    } else {
        let api_base = host.as_deref().unwrap_or("https://idea-analyzer-ten.vercel.app");
        let url = format!("{}/api/axiom/manifest/{}", api_base, manifest_id);
        
        let res = client.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .timeout(std::time::Duration::from_secs(30))
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
    let name_val = manifest.get("name")
        .or_else(|| manifest.get("projectName"))
        .or_else(|| manifest.get("title"))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("name")))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("title")));
    let name = name_val.and_then(|n| n.as_str()).unwrap_or("Unknown Project");
    let desc_val = manifest.get("description")
        .or_else(|| manifest.get("projectDescription"))
        .or_else(|| manifest.get("idea"))
        .or_else(|| manifest.get("content"))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("description")))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("idea")));
    let project_description = desc_val.and_then(|d| d.as_str()).unwrap_or("");
    let project_type = detect_project_type(&manifest);

    // Immediately register/update the project in projects.json with "generating" status
    // so it appears on the Dashboard right away with its real name and description.
    let initial_proj = serde_json::json!({
        "id": project_id,
        "name": name,
        "description": project_description,
        "status": "generating",
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });
    let _ = project_save(initial_proj);

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
                            "next": "14.2.35",
                            "react": "^18.2.0",
                            "react-dom": "^18.2.0",
                            "next-auth": "^4.24.7",
                            "@auth/prisma-adapter": "^1.0.12",
                            "@prisma/client": "5.22.0",
                            "lucide-react": "^0.460.0",
                            "clsx": "^2.1.1",
                            "tailwind-merge": "^2.3.0",
                            "class-variance-authority": "^0.7.0",
                            "tailwindcss-animate": "^1.0.7",
                            "zod": "^3.23.8",
                            "react-hook-form": "^7.51.5",
                            "bcryptjs": "^2.4.3",
                            "jsonwebtoken": "^9.0.2"
                        },
                        "devDependencies": {
                            "prisma": "5.22.0",
                            "tailwindcss": "^3.4.0",
                            "postcss": "^8.4.32",
                            "autoprefixer": "^10.4.16",
                            "typescript": "^5.3.3",
                            "@types/react": "^18.2.45",
                            "@types/react-dom": "^18.2.18",
                            "@types/node": "^20.10.5",
                            "@types/bcryptjs": "^2.4.6",
                            "@types/jsonwebtoken": "^9.0.6"
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
                    "path": "src/middleware.ts",
                    "description": "Resource-optimized middleware that bypasses static asset queries.",
                    "language": "typescript",
                    "defaultContent": r#"import { withAuth } from "next-auth/middleware";
import { NextRequest, NextResponse } from "next/server";

const authMiddleware = withAuth(
  function middleware(req: NextRequest) {
    return NextResponse.next();
  },
  {
    callbacks: {
      authorized: ({ token }) => !!token,
    },
    pages: {
      signIn: "/login",
    },
  }
);

export default function middleware(req: NextRequest) {
  const { pathname } = req.nextUrl;

  // 1. Skip middleware for API, internal assets, and static files with extensions
  if (
    pathname.includes("/api/") ||
    pathname.includes("/_next/") ||
    pathname.includes("/_vercel/") ||
    pathname.includes(".")
  ) {
    return NextResponse.next();
  }

  // 2. Protect dashboard, admin and profile routes
  const isProtectedPage = 
    pathname.startsWith("/dashboard") || 
    pathname.startsWith("/admin") || 
    pathname.startsWith("/profile");

  if (isProtectedPage) {
    return (authMiddleware as any)(req);
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/((?!api|_next|_vercel|.*\\..*).*)"]
};"#
                },
                {
                    "path": "prisma/schema.prisma",
                    "description": "Pre-configured Prisma schema for SQLite database.",
                    "language": "prisma",
                    "defaultContent": r#"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "sqlite"
  url      = "file:./dev.db"
}

model User {
  id             String    @id @default(cuid())
  name           String?
  email          String    @unique
  emailVerified  DateTime?
  image          String?
  hashedPassword String?
  role           String    @default("USER")
  createdAt      DateTime  @default(now())
  updatedAt      DateTime  @updatedAt
  accounts       Account[]
  sessions       Session[]
}

model Account {
  id                String  @id @default(cuid())
  userId            String
  type              String
  provider          String
  providerAccountId String
  refresh_token     String?
  access_token      String?
  expires_at        Int?
  token_type        String?
  scope             String?
  id_token          String?
  session_state     String?
  user              User    @relation(fields: [userId], references: [id], onDelete: Cascade)

  @@unique([provider, providerAccountId])
}

model Session {
  id           String   @id @default(cuid())
  sessionToken String   @unique
  userId       String
  expires      DateTime
  user         User     @relation(fields: [userId], references: [id], onDelete: Cascade)
}

model VerificationToken {
  identifier String
  token      String   @unique
  expires    DateTime

  @@unique([identifier, token])
}"#
                },
                {
                    "path": "src/lib/prisma.ts",
                    "description": "PrismaClient singleton generator.",
                    "language": "typescript",
                    "defaultContent": r#"import { PrismaClient } from "@prisma/client";

const globalForPrisma = globalThis as unknown as { prisma: PrismaClient | undefined };

export const prisma =
  globalForPrisma.prisma ??
  new PrismaClient({
    log: ["query"],
  });

if (process.env.NODE_ENV !== "production") globalForPrisma.prisma = prisma;

export default prisma;"#
                },
                {
                    "path": "src/app/api/auth/[...nextauth]/route.ts",
                    "description": "NextAuth App Router route handler.",
                    "language": "typescript",
                    "defaultContent": r#"import NextAuth from "next-auth";
import CredentialsProvider from "next-auth/providers/credentials";
import { PrismaAdapter } from "@auth/prisma-adapter";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";

export const authOptions = {
  adapter: PrismaAdapter(prisma) as any,
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
          const passwordToCompare = user.hashedPassword || (user as any).password || "";
          const passwordMatch = await bcrypt.compare(credentials.password, passwordToCompare);
          if (!passwordMatch) return null;
          return { id: String(user.id), email: user.email, name: user.name, role: user.role };
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
        token.role = user.role;
      }
      return token;
    },
    async session({ session, token }: any) {
      if (session.user && token) {
        session.user.id = token.id;
        session.user.role = token.role;
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
export { handler as GET, handler as POST };"#
                },
                {
                    "path": "src/app/api/register/route.ts",
                    "description": "User registration App Router API endpoint.",
                    "language": "typescript",
                    "defaultContent": r#"import { NextResponse } from "next/server";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";
import { z } from "zod";

const registerSchema = z.object({
  name: z.string().min(2).max(50),
  email: z.string().email(),
  password: z.string().min(8)
});

export async function POST(request: Request) {
  try {
    const body = await request.json();
    const { name, email, password } = registerSchema.parse(body);

    const existingUser = await prisma.user.findUnique({ where: { email } });
    if (existingUser) {
      return NextResponse.json({ message: "Email already in use" }, { status: 400 });
    }

    const hashedPassword = await bcrypt.hash(password, 10);

    const newUser = await prisma.user.create({
      data: {
        name,
        email,
        hashedPassword,
        role: "USER"
      }
    });

    return NextResponse.json(
      { message: "User registered successfully", user: { id: newUser.id, name: newUser.name, email: newUser.email } },
      { status: 201 }
    );
  } catch (error: any) {
    console.error("Registration error:", error);
    return NextResponse.json({ message: error.message || "Internal server error" }, { status: 500 });
  }
}"#
                },
                {
                    "path": "src/components/Navbar.tsx",
                    "description": "Modern responsive Navbar with inline SVG brand lockup and mobile hamburger drawer.",
                    "language": "typescript",
                    "defaultContent": r#""use client";

import React, { useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useSession, signOut } from "next-auth/react";

const Navbar: React.FC = () => {
  const pathname = usePathname();
  const { data: session } = useSession();
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  return (
    <header className="sticky top-0 z-50 w-full backdrop-blur-md bg-white/90 border-b border-slate-200/80 shadow-sm">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center justify-between gap-4">
        <Link href="/" prefetch={false} className="flex items-center gap-2.5 group shrink-0">
          <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-indigo-600 to-blue-600 flex items-center justify-center text-white shadow-md shadow-indigo-500/20">
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
              <path strokeLinecap="round" strokeLinejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
          </div>
          <span className="text-lg font-extrabold tracking-tight text-slate-900">AxiomApp</span>
        </Link>

        <div className="hidden md:flex items-center gap-1">
          <Link
            href="/dashboard"
            prefetch={false}
            className={`px-3.5 py-2 rounded-lg text-sm transition-colors ${
              pathname === "/dashboard"
                ? "bg-indigo-50 text-indigo-600 font-semibold"
                : "text-slate-600 hover:text-slate-900 hover:bg-slate-100 font-medium"
            }`}
          >
            Dashboard
          </Link>
        </div>

        <div className="hidden md:flex items-center gap-3 shrink-0">
          {session ? (
            <>
              <span className="text-sm font-medium text-slate-600">{session.user?.name || session.user?.email}</span>
              <button
                onClick={() => signOut({ callbackUrl: "/" })}
                className="inline-flex items-center justify-center bg-slate-100 hover:bg-slate-200 text-slate-800 px-4 py-2 rounded-xl text-sm font-semibold transition-colors"
              >
                Logout
              </button>
            </>
          ) : (
            <>
              <Link href="/login" prefetch={false} className="text-sm font-semibold text-slate-700 hover:text-indigo-600 px-3.5 py-2 rounded-lg transition-colors">
                Sign In
              </Link>
              <Link href="/register" prefetch={false} className="inline-flex items-center justify-center bg-indigo-600 hover:bg-indigo-700 text-white text-sm font-semibold px-4 py-2 rounded-xl shadow-sm transition-colors">
                Register
              </Link>
            </>
          )}
        </div>

        <div className="flex md:hidden">
          <button
            type="button"
            onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
            className="inline-flex items-center justify-center p-2 rounded-lg text-slate-600 hover:text-slate-900 hover:bg-slate-100 focus:outline-none"
            aria-label="Toggle Menu"
          >
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              {isMobileMenuOpen ? (
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              ) : (
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
              )}
            </svg>
          </button>
        </div>
      </div>

      {isMobileMenuOpen && (
        <div className="md:hidden border-t border-slate-200 bg-white px-4 pt-3 pb-4 space-y-1.5 shadow-lg">
          <Link
            href="/dashboard"
            prefetch={false}
            onClick={() => setIsMobileMenuOpen(false)}
            className="block px-3 py-2 rounded-lg text-base font-medium text-slate-700 hover:bg-slate-100"
          >
            Dashboard
          </Link>
          <Link
            href="/login"
            prefetch={false}
            onClick={() => setIsMobileMenuOpen(false)}
            className="block px-3 py-2 rounded-lg text-base font-medium text-slate-700 hover:bg-slate-100"
          >
            Sign In
          </Link>
          <Link
            href="/register"
            prefetch={false}
            onClick={() => setIsMobileMenuOpen(false)}
            className="block px-3 py-2 rounded-lg text-base font-semibold text-indigo-600 hover:bg-indigo-50"
          >
            Register
          </Link>
        </div>
      )}
    </header>
  );
};

export { Navbar };
export default Navbar;"#
                },
                {
                    "path": "src/components/Footer.tsx",
                    "description": "Compact horizontal footer utilizing prefetch-disabled links.",
                    "language": "typescript",
                    "defaultContent": r#"import React from "react";
import Link from "next/link";

const Footer: React.FC = () => {
  return (
    <footer className="bg-slate-900 text-slate-400 border-t border-slate-800 py-8 mt-auto w-full">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col md:flex-row items-center justify-between gap-6">
        <div className="flex items-center gap-2.5">
          <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-indigo-600 to-blue-600 flex items-center justify-center text-white">
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
              <path strokeLinecap="round" strokeLinejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
          </div>
          <p className="text-sm text-slate-400">&copy; {new Date().getFullYear()} AxiomApp. All rights reserved.</p>
        </div>
        <nav className="flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm">
          <Link href="/" prefetch={false} className="text-slate-400 hover:text-white transition-colors">
            Home
          </Link>
          <Link href="/dashboard" prefetch={false} className="text-slate-400 hover:text-white transition-colors">
            Dashboard
          </Link>
        </nav>
      </div>
    </footer>
  );
};

export { Footer };
export default Footer;"#
                },
                {
                    "path": "src/app/layout.tsx",
                    "description": "Root layout wrapped in SessionProvider.",
                    "language": "typescript",
                    "defaultContent": r#""use client";

import React from "react";
import { SessionProvider } from "next-auth/react";
import Navbar from "@/components/Navbar";
import Footer from "@/components/Footer";
import "./globals.css";

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <head>
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
      </head>
      <body className="flex flex-col min-h-screen bg-slate-50 text-slate-900 antialiased">
        <SessionProvider>
          <Navbar />
          <main className="flex-1 flex flex-col w-full">{children}</main>
          <Footer />
        </SessionProvider>
      </body>
    </html>
  );
}"#
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

    if resume_base_pct.is_none() {
        emit_progress("fetch-manifest", "Manifest secured and ready.", 10.0);
    }

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

    // Ensure local Ollama server is running before manifest expansion (especially important on cold-start deep links)
    let _ = ollama_start_server().await;

    // Manifest Expansion Phase
    if resume_base_pct.is_none() {
        emit_progress("manifest-expansion", "Expanding manifest structure with local AI...", 12.0);
    }
    
    let resolved_stack = manifest.get("resolvedStack").and_then(|s| s.as_str()).unwrap_or("Next.js");
    let platform = manifest.get("platform").and_then(|p| p.as_str()).unwrap_or("web");
    
    // Create a compact list of original files so prompt stays concise and fast
    let original_files_summary: Vec<Value> = files.iter().map(|f| {
        let path = f.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let lang = f.get("language").and_then(|l| l.as_str()).unwrap_or("");
        let desc = f.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let short_desc = if desc.chars().count() > 100 {
            let truncated: String = desc.chars().take(100).collect();
            format!("{}...", truncated)
        } else {
            desc.to_string()
        };
        serde_json::json!({
            "path": path,
            "description": short_desc,
            "language": lang
        })
    }).collect();
    let original_files_json = serde_json::to_string(&original_files_summary).unwrap_or_else(|_| "[]".to_string());

    let expansion_prompt = format!(
        "You are a Senior Project Architect. We are generating a complete {} application named '{}' using {}.\n\
         Project Idea & Core Domain Description: \"{}\"\n\
         The user requested a fully complete, production-ready implementation tailored specifically to this business domain.\n\
         Version Overrides requested by user:\n\
         {}\n\n\
         Original files skeleton in the manifest:\n\
         {}\n\n\
         Identify all missing files, routes, state management, components, configuration files, or database schemas needed to make this project complete.\n\
         CRITICAL DOMAIN RULE: Ensure the pages, dashboard, API routes, and Prisma schema models directly implement the core business features described in the Project Idea (not just generic user authentication).\n\
         IMPORTANT ARCHITECTURAL RULE: If the project uses next-auth (NextAuth) for authentication, you MUST always include 'src/app/api/register/route.ts' in the expanded files list to handle new user registration (creating user record using bcryptjs), since NextAuth does not natively provide a user registration route.\n\
         Output a JSON array containing the expanded, complete list of files. For each file, provide 'path', 'description' (concise, 1-2 sentences), and 'language'.\n\
         Keep each file description concise (1-2 sentences) so the manifest generates quickly.\n\
         Ensure all files from the original list are preserved.\n\
         Respond ONLY with a valid JSON array. Do not include any introductory or concluding text, or markdown code blocks.",
         platform, name, resolved_stack, project_description, overrides_desc, original_files_json
    );

    let mut files_to_generate = if let Some(ref saved_files) = files_to_generate_from_cp {
        debug_log_to_file(format!("[Orchestrator] Reusing {} expanded files from checkpoint.", saved_files.len()));
        emit_progress(
            "generate-files",
            &format!("Učitan checkpoint: {}/{} fajlova već sačuvano.", completed_files.len(), saved_files.len()),
            resume_base_pct.unwrap_or(14.0),
        );
        saved_files.clone()
    } else {
        files.clone()
    };
    
    let manifest_runtime_cfg = get_ollama_runtime_config(&model_id);
    let ollama_req = serde_json::json!({
        "model": &model_id,
        "messages": [
            { "role": "system", "content": "You are a Senior Project Architect. Output only raw JSON. No markdown." },
            { "role": "user", "content": expansion_prompt }
        ],
        "stream": true,
        "options": build_ollama_options(&model_id, 0.2, Some(manifest_runtime_cfg.manifest_num_predict))
    });

    let mut expansion_success = files_to_generate_from_cp.is_some();
    let mut expansion_attempts = 0;
    let max_expansion_attempts = 2;
    let mut last_accumulated_response = String::new();

    while expansion_attempts < max_expansion_attempts && !expansion_success {
        expansion_attempts += 1;
        if task_control.stopped.load(Ordering::SeqCst) {
            unregister_task_control(&task_id, &project_id);
            let _ = project_update_status(project_id.clone(), "stopped".to_string(), serde_json::json!({}));
            emit_progress("stopped", "Generation stopped by user.", 10.0);
            return Ok(());
        }
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

                    loop {
                        // 45-second chunk timeout to allow for model thinking pauses
                        let chunk_fut = resp.chunk();
                        let chunk_res = tokio::time::timeout(std::time::Duration::from_secs(45), chunk_fut).await;

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
                                break;
                            }
                            Err(_) => {
                                debug_log_to_file(format!("[Orchestrator] Chunk read timed out after 45s (Attempt {})", expansion_attempts));
                                break;
                            }
                        }

                        // Check global timeout for this attempt
                        let elapsed = stream_start.elapsed();
                        if elapsed.as_secs() > 120 {
                            if chars_received == 0 {
                                debug_log_to_file(format!("[Orchestrator] Stream has not started generating any content after 120s. Aborting attempt {}.", expansion_attempts));
                                break;
                            } else {
                                // System has started writing the extended manifest. Check if it's progressing.
                                if last_chunk_time.elapsed().as_secs() > 45 {
                                    debug_log_to_file(format!("[Orchestrator] Stream stalled (no chunks for 45s) after 120s limit. Aborting attempt {}.", expansion_attempts));
                                    break;
                                } else if elapsed.as_secs() > 480 {
                                    debug_log_to_file(format!("[Orchestrator] Stream exceeded maximum allowed time of 480s. Aborting attempt {}.", expansion_attempts));
                                    break;
                                }
                            }
                        }
                    }

                    last_accumulated_response = accumulated_response.clone();

                    let cleaned_json = extract_code_content(&accumulated_response);
                    let mut parsed_files_opt: Option<Vec<Value>> = None;

                    // 1. Try direct array between [ and ]
                    if let Some(start) = cleaned_json.find('[') {
                        if let Some(end) = cleaned_json.rfind(']') {
                            if end > start {
                                if let Ok(files) = serde_json::from_str::<Vec<Value>>(&cleaned_json[start..=end]) {
                                    if !files.is_empty() {
                                        parsed_files_opt = Some(files);
                                    }
                                }
                            }
                        }
                        // 2. If incomplete or truncated, salvage all complete objects up to the last valid '}'
                        if parsed_files_opt.is_none() {
                            if let Some(last_brace) = cleaned_json.rfind('}') {
                                if last_brace > start {
                                    let salvaged_str = format!("{}]", &cleaned_json[start..=last_brace]);
                                    if let Ok(files) = serde_json::from_str::<Vec<Value>>(&salvaged_str) {
                                        if !files.is_empty() {
                                            debug_log_to_file(format!(
                                                "[Orchestrator] Salvaged {} complete file objects from partial manifest expansion stream.",
                                                files.len()
                                            ));
                                            parsed_files_opt = Some(files);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(expanded_files) = parsed_files_opt {
                        // Merge to ensure all original manifest files are preserved
                        let mut merged = expanded_files;
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
                        break;
                    } else {
                        debug_log_to_file(format!(
                            "[Orchestrator] Failed to parse expanded manifest JSON (Attempt {}). Content: {}",
                            expansion_attempts, cleaned_json
                        ));
                    }
                } else {
                    debug_log_to_file(format!("[Orchestrator] Manifest expansion Ollama status error (Attempt {}): {}", expansion_attempts, resp.status()));
                }
            }
            Ok(Err(e)) => {
                debug_log_to_file(format!("[Orchestrator] Manifest expansion request error (Attempt {}): {}", expansion_attempts, e));
                let _ = ollama_start_server().await;
            }
            Err(_) => {
                debug_log_to_file(format!("[Orchestrator] Manifest expansion initial connection timed out after 120s (Attempt {})", expansion_attempts));
            }
        }
    }

    if !expansion_success {
        debug_log_to_file(format!(
            "[Orchestrator] Manifest expansion could not be completed after {} attempts. Using original manifest files.\nLast accumulated raw content:\n{}",
            expansion_attempts, last_accumulated_response
        ));
        emit_progress("manifest-expansion", "Manifest ready. Using original manifest files.", 14.0);
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

    if let Some(pct) = resume_base_pct {
        emit_progress(
            "generate-files",
            &format!("Nastavljam generisanje od fajla {}/{}...", completed_files.len() + 1, files_to_generate.len()),
            pct,
        );
    } else {
        emit_progress("generate-files", "Checking local AI engine...", 15.0);
    }

    // 2. Generation Loop
    let total_files = files_to_generate.len() as f64;

    let mut checkpoint_state = serde_json::json!({
        "taskId": task_id,
        "manifestId": manifest_id,
        "projectId": project_id,
        "name": name,
        "projectDescription": project_description,
        "projectType": project_type,
        "modelId": model_id,
        "token": token,
        "host": host,
        "status": "generating",
        "phase": "generate-files",
        "currentFileIndex": completed_files.len(),
        "totalFiles": files_to_generate.len(),
        "filesToGenerate": files_to_generate,
        "completedFiles": completed_files,
        "updatedAt": chrono::Utc::now().to_rfc3339()
    });
    save_generation_checkpoint(&proj_dir, &checkpoint_state);

    // Initialize and write the Live Builder Guide (LBG) to disk
    let axiom_dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&axiom_dir);
    let lbg_path = axiom_dir.join("live_builder_guide.json");
    
    if let Some(lbg_val) = manifest.get("liveBuilderGuide") {
        if let Ok(lbg_content) = serde_json::to_string_pretty(lbg_val) {
            let _ = fs::write(&lbg_path, lbg_content);
        }
    } else if !lbg_path.exists() {
        // Fallback: Generate a default LBG guide locally if not sent by the Web App
        let default_lbg = serde_json::json!({
            "projectSpecification": {
                "stack": resolved_stack,
                "importExportStyle": "named",
                "importPathStyle": "absolute-alias",
                "typescriptStrictness": "strict-false",
                "namingConventions": {
                    "components": "PascalCase",
                    "hooks": "camelCase",
                    "routes": "kebab-case",
                    "models": "PascalCase"
                }
            },
            "sharedContracts": {
                "database": {
                    "models": {}
                },
                "api": {
                    "endpoints": {}
                },
                "hooks": {}
            },
            "generationPipeline": []
        });
        if let Ok(lbg_content) = serde_json::to_string_pretty(&default_lbg) {
            let _ = fs::write(&lbg_path, lbg_content);
        }
    }

    for (i, file_spec) in files_to_generate.iter().enumerate() {
        let file_path = file_spec.get("path").and_then(|p| p.as_str()).unwrap_or("unknown.txt");
        let desc = file_spec.get("description").and_then(|d| d.as_str()).unwrap_or("");
        let lang = file_spec.get("language").and_then(|l| l.as_str()).unwrap_or("txt");

        let current_progress = 25.0 + ((i as f64) / total_files) * 55.0;

        // 1. Check if STOPPED
        if task_control.stopped.load(Ordering::SeqCst) {
            debug_log_to_file(format!("[Orchestrator] Task {} stopped before file {}", task_id, file_path));
            checkpoint_state["status"] = serde_json::json!("stopped");
            checkpoint_state["phase"] = serde_json::json!("stopped");
            checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &checkpoint_state);
            let _ = project_update_status(project_id.clone(), "stopped".to_string(), serde_json::json!({}));
            emit_progress("stopped", "Generation stopped by user.", current_progress);
            unregister_task_control(&task_id, &project_id);
            return Ok(());
        }

        // 2. Check if PAUSED
        while task_control.paused.load(Ordering::SeqCst) {
            if task_control.stopped.load(Ordering::SeqCst) {
                checkpoint_state["status"] = serde_json::json!("stopped");
                checkpoint_state["phase"] = serde_json::json!("stopped");
                checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
                save_generation_checkpoint(&proj_dir, &checkpoint_state);
                let _ = project_update_status(project_id.clone(), "stopped".to_string(), serde_json::json!({}));
                emit_progress("stopped", "Generation stopped by user.", current_progress);
                unregister_task_control(&task_id, &project_id);
                return Ok(());
            }
            emit_progress("paused", &format!("Paused at file {}/{} ({})", i + 1, files_to_generate.len(), file_path), current_progress);
            tokio::select! {
                _ = task_control.notify.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
            }
        }

        // 3. Check if ALREADY GENERATED (Resume / Recovery from checkpoint on disk)
        let full_disk_path = proj_dir.join(file_path);
        let already_on_disk = completed_files.contains(&file_path.to_string())
            && full_disk_path.exists()
            && fs::metadata(&full_disk_path).map(|m| m.len() > 0).unwrap_or(false);

        if already_on_disk {
            continue;
        }

        let msg = format!("Generating {}/{} - {}", i + 1, files_to_generate.len(), file_path);
        emit_progress("generate-files", &msg, current_progress);

        let overrides_desc_str = if !overrides_desc.trim().is_empty() {
            format!("\nUse the following version/stack preferences if applicable:\n{}", overrides_desc)
        } else {
            "".to_string()
        };

        let pkg_db_summary = get_packages_db_summary();
        let context_ref = get_context_files_for(file_path, &proj_dir);
        let routes_ref = get_project_routes_prompt(&files_to_generate);

        let mut system_prompt = get_expert_system_prompt(
            file_path,
            &project_type,
            &overrides_desc_str,
            &pkg_db_summary,
            &context_ref,
            &routes_ref,
            &proj_dir,
        );

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
        let user_prompt = if project_description.is_empty() {
            format!("Project: {}\nGenerate {} code for file: {}\n\nDescription: {}", name, lang, file_path, desc)
        } else {
            format!(
                "Project: {} ({})\nGenerate {} code for file: {}\n\nFile Specification: {}\n\nIMPORTANT: Ensure all UI text, domain entities, metrics, and business logic reflect the Project description above rather than generic placeholders.",
                name, project_description, lang, file_path, desc
            )
        };

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
                "options": build_ollama_options(&model_id, 0.3, Some(4096))
            });

            let req_fut = client.post("http://127.0.0.1:11434/api/chat")
                .json(&ollama_req)
                .send();

            // 180-second connection timeout for local LLMs with CPU offload
            let resp_res = tokio::time::timeout(std::time::Duration::from_secs(180), req_fut).await;

            let mut resp = match resp_res {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    debug_log_to_file(format!("[Orchestrator] Connection error for {} (Attempt {}): {}", file_path, attempts, e));
                    let _ = ollama_start_server().await;
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
                // 60-second timeout per streaming chunk
                let chunk_res = tokio::time::timeout(std::time::Duration::from_secs(60), chunk_fut).await;

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

                                // Detect repetition loop only after 100 tokens every 30 tokens to avoid early false positives
                                if token_count >= 100 && token_count % 30 == 0 && detect_infinite_loop(&full_content) {
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

                if task_control.stopped.load(Ordering::SeqCst) {
                    debug_log_to_file(format!("[Orchestrator] Task {} stopped during chunk generation for {}", task_id, file_path));
                    loop_detected = true;
                    break;
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
                            "{}\n\nIMPORTANT: The previous attempt failed validation with the following error:\n{}\nPlease correct this error in your new implementation.",
                            user_prompt, validation_error
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
            if !processed_content.trim().is_empty() {
                debug_log_to_file(format!(
                    "[Orchestrator] Warning: Max attempts reached for {}. Using partially processed content to avoid blocking pipeline.",
                    file_path
                ));
            } else {
                debug_log_to_file(format!(
                    "[Orchestrator] Warning: Max attempts reached for {}. Generating safe fallback stub to avoid blocking pipeline.",
                    file_path
                ));
                let fallback = get_safe_fallback_content(file_path, lang, desc);
                processed_content = post_process_generated_file(file_path, &fallback, &metamanifest, has_tailwind);
            }
        }

        // 3. Save File
        let full_disk_path = proj_dir.join(file_path);
        if let Some(parent) = full_disk_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&full_disk_path, &processed_content);

        // Update completed files and persist checkpoint immediately
        if !completed_files.contains(&file_path.to_string()) {
            completed_files.push(file_path.to_string());
        }
        checkpoint_state["completedFiles"] = serde_json::json!(completed_files);
        checkpoint_state["currentFileIndex"] = serde_json::json!(i + 1);
        checkpoint_state["currentFilePath"] = serde_json::json!(file_path);
        checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
        save_generation_checkpoint(&proj_dir, &checkpoint_state);
    }

    // Check if stopped before post-processing
    if task_control.stopped.load(Ordering::SeqCst) {
        checkpoint_state["status"] = serde_json::json!("stopped");
        checkpoint_state["phase"] = serde_json::json!("stopped");
        checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
        save_generation_checkpoint(&proj_dir, &checkpoint_state);
        let _ = project_update_status(project_id.clone(), "stopped".to_string(), serde_json::json!({}));
        emit_progress("stopped", "Generation stopped by user.", 75.0);
        unregister_task_control(&task_id, &project_id);
        return Ok(());
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

    // Auto-run prisma generate & db push if Prisma schema exists in the project
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        if let Err(e) = heal_prisma_schema_loop(&proj_dir, &window, &model_id, "install-deps").await {
            debug_log_to_file(format!("[Prisma Critical Error] Orchestrator schema healing failed: {}", e));
        }

        // Run prisma db push to synchronize the database schema
        emit_progress("install-deps", "Syncing database schema...", 79.0);
        let local_prisma_js = proj_dir.join("node_modules").join("prisma").join("build").join("index.js");
        let mut push_cmd = if local_prisma_js.exists() {
            let mut c = std::process::Command::new("node");
            c.arg(&local_prisma_js).arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            c
        } else {
            #[cfg(target_os = "windows")]
            let mut c = std::process::Command::new("cmd");
            #[cfg(target_os = "windows")]
            c.arg("/C").arg("npx").arg("-y").arg("prisma@5.22.0").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            
            #[cfg(not(target_os = "windows"))]
            let mut c = std::process::Command::new("npx");
            #[cfg(not(target_os = "windows"))]
            c.arg("-y").arg("prisma@5.22.0").arg("db").arg("push").arg("--accept-data-loss").current_dir(&proj_dir);
            c
        };
        
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
    let project_routes = get_project_routes_from_dir(&proj_dir);
    let routes_list: Vec<String> = project_routes.into_iter().map(|(r, _)| r).collect();
    if !routes_list.is_empty() {
        normalize_project_route_links(&proj_dir, &routes_list);
    }
    generate_axiom_features_json(&proj_dir, name, &files_to_generate);

    // 4. Save Project State
    emit_progress("save-project", "Saving project files...", 80.0);
    
    let new_proj = serde_json::json!({
        "id": project_id,
        "name": name,
        "description": project_description,
        "status": "ready",
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });
    let _ = project_save(new_proj);

    checkpoint_state["status"] = serde_json::json!("completed");
    checkpoint_state["phase"] = serde_json::json!("complete");
    checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    save_generation_checkpoint(&proj_dir, &checkpoint_state);
    unregister_task_control(&task_id, &project_id);

    emit_progress("complete", "Generation completed successfully!", 100.0);

    Ok(())
}

#[tauri::command]
pub async fn task_start_generation(
    window: tauri::Window,
    manifest_id: String,
    project_id: Option<String>,
    token: String,
    model_id: Option<String>,
    host: Option<String>,
) -> Result<String, String> {
    let p_id = project_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let proj_dir = get_project_path(&p_id);
    let existing_cp = load_generation_checkpoint(&proj_dir);

    let task_id = existing_cp
        .as_ref()
        .and_then(|cp| {
            let status = cp.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if status != "completed" {
                cp.get("taskId").and_then(|v| v.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| format!("task-{}", uuid::Uuid::new_v4()));

    let model = model_id.clone().unwrap_or_else(|| "qwen2.5-coder:7b".to_string());

    // Register task control synchronously BEFORE spawning the worker so any immediate status/checkpoint queries see an active worker
    let _ = register_task_control(&task_id, &p_id);

    if let Some(mut cp) = existing_cp {
        let status = cp.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if status != "completed" {
            let completed_len = cp.get("completedFiles").and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
            let total_len = cp.get("totalFiles").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
            let pct = (25.0 + (completed_len as f64 / total_len as f64) * 55.0).min(99.0);
            cp["taskId"] = serde_json::json!(task_id);
            cp["status"] = serde_json::json!("generating");
            cp["phase"] = serde_json::json!("generate-files");
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
            let _ = project_update_status(p_id.clone(), "generating".to_string(), serde_json::json!({}));
            let _ = window.app_handle().emit("task:progress", serde_json::json!({
                "taskId": task_id,
                "projectId": p_id,
                "phase": "generate-files",
                "status": "generating",
                "message": format!("Nastavljam generisanje ({}/{} fajlova)...", completed_len, total_len),
                "progress": pct
            }));
        }
    }
    
    debug_log_to_file(format!(
        "[IPC Rust Command] task_start_generation called. manifest_id: {}, project_id: {:?}, model_id: {:?}, host: {:?}",
        manifest_id, project_id, model_id, host
    ));

    let t_id = task_id.clone();
    let p_id_clone = p_id.clone();
    let host_clone = host.clone();
    tokio::spawn(async move {
        debug_log_to_file(format!("[IPC Rust Command] Spawned tokio worker task for task_id: {}", t_id));
        let err_proj_id = p_id_clone.clone();
        if let Err(e) = run_orchestrator(window.clone(), t_id.clone(), manifest_id, p_id_clone, token, model, host_clone).await {
            debug_log_to_file(format!("[IPC Rust Command] run_orchestrator failed for task_id {}: {}", t_id, e));
            unregister_task_control(&t_id, &err_proj_id);
            let _ = window.app_handle().emit("task:progress", serde_json::json!({
                "taskId": t_id,
                "projectId": err_proj_id,
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

#[tauri::command]
pub fn task_pause_generation(task_id: String) -> Result<(), String> {
    debug_log_to_file(format!("[IPC Rust Command] task_pause_generation called for {}", task_id));
    if let Some(control) = find_task_control(&task_id) {
        control.paused.store(true, Ordering::SeqCst);
        let proj_dir = get_project_path(&control.project_id);
        if let Some(mut cp) = load_generation_checkpoint(&proj_dir) {
            cp["status"] = serde_json::json!("paused");
            cp["phase"] = serde_json::json!("paused");
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
        }
        let _ = project_update_status(control.project_id.clone(), "paused".to_string(), serde_json::json!({}));
        Ok(())
    } else {
        Err(format!("Active task '{}' not found", task_id))
    }
}

#[tauri::command]
pub fn task_resume_generation(task_id: String) -> Result<(), String> {
    debug_log_to_file(format!("[IPC Rust Command] task_resume_generation called for {}", task_id));
    if let Some(control) = find_task_control(&task_id) {
        control.paused.store(false, Ordering::SeqCst);
        control.notify.notify_waiters();
        let proj_dir = get_project_path(&control.project_id);
        if let Some(mut cp) = load_generation_checkpoint(&proj_dir) {
            cp["status"] = serde_json::json!("generating");
            cp["phase"] = serde_json::json!("generate-files");
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
        }
        let _ = project_update_status(control.project_id.clone(), "generating".to_string(), serde_json::json!({}));
        Ok(())
    } else {
        Err(format!("Active task '{}' not found", task_id))
    }
}

#[tauri::command]
pub fn task_stop_generation(task_id: String) -> Result<(), String> {
    debug_log_to_file(format!("[IPC Rust Command] task_stop_generation called for {}", task_id));
    if let Some(control) = find_task_control(&task_id) {
        control.stopped.store(true, Ordering::SeqCst);
        control.paused.store(false, Ordering::SeqCst);
        control.notify.notify_waiters();
        let proj_dir = get_project_path(&control.project_id);
        if let Some(mut cp) = load_generation_checkpoint(&proj_dir) {
            cp["status"] = serde_json::json!("stopped");
            cp["phase"] = serde_json::json!("stopped");
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
        }
        let _ = project_update_status(control.project_id.clone(), "stopped".to_string(), serde_json::json!({}));
        Ok(())
    } else {
        let proj_dir = get_project_path(&task_id);
        if let Some(mut cp) = load_generation_checkpoint(&proj_dir) {
            cp["status"] = serde_json::json!("stopped");
            cp["phase"] = serde_json::json!("stopped");
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
            let _ = project_update_status(task_id, "stopped".to_string(), serde_json::json!({}));
        }
        Ok(())
    }
}

#[tauri::command]
pub fn task_get_project_checkpoint(project_id: String) -> Result<Option<Value>, String> {
    let proj_dir = get_project_path(&project_id);
    if let Some(mut cp) = load_generation_checkpoint(&proj_dir) {
        let cp_status = cp.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if let Some(ctrl) = find_task_control(&project_id) {
            if ctrl.paused.load(Ordering::SeqCst) {
                cp["status"] = serde_json::json!("paused");
                cp["phase"] = serde_json::json!("paused");
            } else if ctrl.stopped.load(Ordering::SeqCst) {
                cp["status"] = serde_json::json!("stopped");
                cp["phase"] = serde_json::json!("stopped");
            }
        } else if cp_status == "generating" {
            // Reconcile stale "generating" checkpoint when no worker thread is running
            cp["status"] = serde_json::json!("stopped");
            cp["phase"] = serde_json::json!("stopped");
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
            let _ = project_update_status(project_id, "stopped".to_string(), serde_json::json!({}));
        }
        Ok(Some(cp))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn task_resume_project_generation(
    window: tauri::Window,
    project_id: String,
) -> Result<String, String> {
    debug_log_to_file(format!("[IPC Rust Command] task_resume_project_generation called for {}", project_id));
    if let Some(ctrl) = find_task_control(&project_id) {
        if ctrl.paused.load(Ordering::SeqCst) {
            let _ = task_resume_generation(project_id);
        }
        return Ok(ctrl.task_id.clone());
    }

    let proj_dir = get_project_path(&project_id);
    let cp = load_generation_checkpoint(&proj_dir)
        .ok_or_else(|| format!("No checkpoint found for project {}", project_id))?;

    let manifest_id = cp.get("manifestId").and_then(|v| v.as_str()).unwrap_or("TEST_LOCAL").to_string();
    let token = cp.get("token").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let model_id = cp.get("modelId").and_then(|v| v.as_str()).map(|s| s.to_string());
    let host = cp.get("host").and_then(|v| v.as_str()).map(|s| s.to_string());

    task_start_generation(window, manifest_id, Some(project_id), token, model_id, host).await
}

