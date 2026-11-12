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
    let is_root_json = !path_lower.contains('/') && path_lower.ends_with(".json");
    let is_config_location = path_lower.starts_with("config/")
        || path_lower.contains("/config/")
        || path_lower.contains(".config.");

    let is_app_route = crate::sanitizer::is_app_router_route_file(file_path);
    let is_api_path = path_lower.starts_with("api/") || path_lower.contains("/api/");

    match project_type {
        "nextjs" | "mern" => {
            if path_lower == "package.json"
                || path_lower == "tsconfig.json"
                || path_lower == "components.json"
                || path_lower == "tailwind.config.js"
                || path_lower == "postcss.config.js"
                || path_lower == "next.config.js"
                || is_config_location
                || is_root_json
            {
                GenerationPhase::Config
            } else if path_lower.contains("schema.prisma")
                || path_lower.contains("prisma")
                || path_lower.contains("database")
                || (!is_app_route && !is_api_path && (path_lower.contains("models/") || path_lower.contains("types/")))
            {
                GenerationPhase::DatabaseOrMain
            } else if !is_app_route && !is_api_path && (path_lower.contains("hooks/") || path_lower.contains("utils/") || path_lower.contains("lib/")) {
                GenerationPhase::LogicOrHooks
            } else if is_api_path || path_lower.contains("api/") || (!is_app_route && (path_lower.contains("auth") || path_lower.contains("router"))) {
                GenerationPhase::ApiOrState
            } else if path_lower.contains("layout.tsx") || path_lower.contains("layout.jsx") || path_lower.contains("layout.js") || 
                      path_lower.contains("globals.css") || path_lower.contains("styles/") || path_lower.contains("navigation") {
                GenerationPhase::ShellOrWidgets
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        "electron" | "rust" => {
            if path_lower == "package.json" || path_lower == "cargo.toml" || is_config_location || is_root_json {
                GenerationPhase::Config
            } else if path_lower.contains("main.js") || path_lower.contains("main.ts") || path_lower.contains("preload.js") || 
                      path_lower.contains("main.rs") || path_lower.contains("commands") {
                GenerationPhase::DatabaseOrMain
            } else {
                GenerationPhase::PagesOrUi
            }
        }
        "flutter" => {
            if path_lower.contains("pubspec.yaml") || is_config_location {
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
    let is_app_route = crate::sanitizer::is_app_router_route_file(file_path);

    let is_backend_or_db = path_lower.contains("/api/")
        || path_lower.ends_with("route.ts")
        || path_lower.ends_with("route.js")
        || path_lower.contains("prisma")
        || path_lower.contains("server.")
        || (!is_app_route && (path_lower.contains("/models/") || path_lower.starts_with("models/")))
        || path_lower.contains("seed.");

    let is_auth_file = path_lower.contains("/login")
        || path_lower.contains("/register")
        || path_lower.contains("/signup")
        || path_lower.contains("useauth")
        || path_lower.contains("/api/auth")
        || (!is_app_route && path_lower.contains("auth"));

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
            if !path_lower.contains("prisma") && !path_lower.contains("/models/") && !path_lower.starts_with("models/") {
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
                    "\nReference ONLY (NEVER import /api/**/route or authOptions directly in 'use client' files or hooks — use next-auth/react or fetch('/api/auth/...') instead): NextAuth configuration (src/app/api/auth/[...nextauth]/route.ts):\n```typescript\n{}\n```\n",
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

fn strip_ts_js_comments_preserving_strings(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_tick = false;
    let mut escaped = false;

    while i < chars.len() {
        let ch = chars[i];
        if escaped {
            out.push(ch);
            escaped = false;
            i += 1;
            continue;
        }
        if (in_single || in_double || in_tick) && ch == '\\' {
            out.push(ch);
            escaped = true;
            i += 1;
            continue;
        }
        if ch == '\'' && !in_double && !in_tick {
            in_single = !in_single;
            out.push(ch);
            i += 1;
            continue;
        }
        if ch == '"' && !in_single && !in_tick {
            in_double = !in_double;
            out.push(ch);
            i += 1;
            continue;
        }
        if ch == '`' && !in_single && !in_double {
            in_tick = !in_tick;
            out.push(ch);
            i += 1;
            continue;
        }
        if !in_single && !in_double && !in_tick && ch == '/' {
            if chars.get(i + 1) == Some(&'/') {
                i += 2;
                while i < chars.len() && chars[i] != '\n' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }
            if chars.get(i + 1) == Some(&'*') {
                i += 2;
                while i < chars.len() {
                    if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        i += 2;
                        out.push(' ');
                        break;
                    }
                    out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                    i += 1;
                }
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn find_matching_brace_end(chars: &[char], open_idx: usize) -> Option<usize> {
    if chars.get(open_idx) != Some(&'{') {
        return None;
    }
    let mut depth = 0isize;
    let mut i = open_idx;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_tick = false;
    let mut escaped = false;
    while i < chars.len() {
        let ch = chars[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if (in_single || in_double || in_tick) && ch == '\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if ch == '\'' && !in_double && !in_tick {
            in_single = !in_single;
        } else if ch == '"' && !in_single && !in_tick {
            in_double = !in_double;
        } else if ch == '`' && !in_single && !in_double {
            in_tick = !in_tick;
        } else if !in_single && !in_double && !in_tick {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

fn model_to_prisma_delegate(model_name: &str) -> String {
    let mut chars = model_name.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_lowercase(), chars.as_str()),
        None => String::new(),
    }
}

pub fn extract_planned_api_routes_summary(files: &[Value]) -> String {
    let mut routes: Vec<String> = Vec::new();
    for f in files {
        let Some(raw_path) = f.get("path").and_then(|p| p.as_str()) else { continue };
        let norm = raw_path.replace('\\', "/");
        let lower = norm.to_lowercase();
        if (lower.contains("/api/") || lower.starts_with("api/"))
            && (lower.ends_with("/route.ts") || lower.ends_with("/route.js"))
        {
            let ep = if let Some(idx) = lower.find("/api/") {
                norm[idx..].trim_end_matches("/route.ts").trim_end_matches("/route.js")
            } else {
                norm.trim_end_matches("/route.ts").trim_end_matches("/route.js")
            };
            let desc = f
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .split_whitespace()
                .collect::<Vec<&str>>()
                .join(" ");
            let desc_short: String = desc.chars().take(140).collect();
            if desc_short.is_empty() {
                routes.push(format!("- {}", ep));
            } else {
                routes.push(format!("- {}: {}", ep, desc_short));
            }
        }
    }
    routes.sort();
    routes.dedup();
    if routes.is_empty() {
        String::new()
    } else {
        format!(
            "\nPLANNED API ROUTES REQUIRING DATABASE SCHEMA SUPPORT:\n{}\nEnsure EVERY model, enum, and scalar field referenced by these planned API routes is declared in schema.prisma.\n",
            routes.join("\n")
        )
    }
}

pub async fn validate_file_with_project_context(
    proj_dir: &std::path::Path,
    file_path: &str,
    content: &str,
    model_id: &str,
) -> Result<(), String> {
    validate_file_with_abstract_rules(file_path, content, model_id).await?;

    let path_lower = file_path.replace('\\', "/").to_lowercase();
    let is_ts_js = path_lower.ends_with(".ts")
        || path_lower.ends_with(".tsx")
        || path_lower.ends_with(".js")
        || path_lower.ends_with(".jsx");
    if !is_ts_js {
        return Ok(());
    }

    let is_api_route = (path_lower.contains("/api/") || path_lower.starts_with("api/"))
        && (path_lower.ends_with("/route.ts")
            || path_lower.ends_with("/route.js")
            || path_lower == "route.ts"
            || path_lower == "route.js");

    let trimmed = content.trim_start();
    let is_client_file = trimmed.starts_with("\"use client\"")
        || trimmed.starts_with("'use client'")
        || trimmed.lines().take(5).any(|l| {
            let t = l.trim();
            t == "\"use client\";" || t == "\"use client\"" || t == "'use client';" || t == "'use client'"
        });
    let is_hook_file = (path_lower.contains("/hooks/") || path_lower.starts_with("hooks/")) && !path_lower.contains("/api/");
    let uses_server_prisma = !is_client_file
        && !is_hook_file
        && (content.contains("prisma.")
            || content.contains("@prisma/client")
            || content.contains("/lib/prisma"));

    if !is_api_route && !uses_server_prisma {
        return Ok(());
    }

    let clean_code = strip_ts_js_comments_preserving_strings(content);

    // 1. Validate Prisma delegates and Enum references against actual prisma/schema.prisma on disk
    let schema_path = [
        proj_dir.join("prisma/schema.prisma"),
        proj_dir.join("schema.prisma"),
    ]
    .into_iter()
    .find(|p| p.is_file());

    if let Some(sp) = schema_path {
        if let Ok(prisma_src) = std::fs::read_to_string(&sp) {
            let contract = extract_prisma_schema_contract(&prisma_src);
            if let Some(models_obj) = contract.get("models").and_then(|m| m.as_object()) {
                if !models_obj.is_empty() {
                    let valid_delegates: Vec<String> = models_obj
                        .keys()
                        .map(|m| model_to_prisma_delegate(m))
                        .collect();

                    if let Ok(del_re) = regex::Regex::new(
                        r"\bprisma\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)\s*\.\s*(findMany|findUnique|findFirst|findUniqueOrThrow|findFirstOrThrow|create|createMany|update|updateMany|upsert|delete|deleteMany|count|aggregate|groupBy)\s*\(",
                    ) {
                        for cap in del_re.captures_iter(&clean_code) {
                            if let Some(del_m) = cap.get(1) {
                                let del_name = del_m.as_str();
                                if !del_name.starts_with('$') && !valid_delegates.iter().any(|vd| vd == del_name) {
                                    return Err(format!(
                                        "Invalid Prisma delegate `prisma.{}` in {}: model does not exist in prisma/schema.prisma (declared delegates: {}).",
                                        del_name,
                                        file_path,
                                        valid_delegates.join(", ")
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            if let Some(enums_obj) = contract.get("enums").and_then(|e| e.as_object()) {
                for enum_name in enums_obj.keys() {
                    let prisma_ns_pat = format!(r"\bPrisma\s*\.\s*{}\b", regex::escape(enum_name));
                    if regex::Regex::new(&prisma_ns_pat).ok().map_or(false, |re| re.is_match(&clean_code)) {
                        return Err(format!(
                            "Invalid Prisma v5 enum reference `Prisma.{}` in {}: import `{}` directly from '@prisma/client' instead of `Prisma.{}`.",
                            enum_name, file_path, enum_name, enum_name
                        ));
                    }

                    let usage_pat = format!(r"\b{}\.[A-Z][A-Z0-9_]*\b", regex::escape(enum_name));
                    let used = regex::Regex::new(&usage_pat)
                        .ok()
                        .map_or(false, |re| re.is_match(&clean_code));
                    if used {
                        let decl_pat = format!(
                            r#"(?s)(?:\bimport\s+(?:type\s+)?(?:\{{[^}}]*\b{}\b[^}}]*\}}|{}\b)|\b(?:enum|const|let|var|type|interface)\s+{}\b)"#,
                            regex::escape(enum_name),
                            regex::escape(enum_name),
                            regex::escape(enum_name)
                        );
                        let declared_or_imported = regex::Regex::new(&decl_pat)
                            .ok()
                            .map_or(false, |re| re.is_match(&clean_code));
                        if !declared_or_imported {
                            return Err(format!(
                                "Prisma enum `{}` is referenced in {} without importing it from '@prisma/client' or declaring it.",
                                enum_name, file_path
                            ));
                        }
                    }
                }

                // Reject fabricated `Prisma.<UnknownEnum>.<UPPER_MEMBER>` namespace lookups
                if let Ok(phantom_ns_re) = regex::Regex::new(r"\bPrisma\s*\.\s*([A-Z][A-Za-z0-9_]*)\s*\.\s*([A-Z][A-Z0-9_]+)\b") {
                    for cap in phantom_ns_re.captures_iter(&clean_code) {
                        if let (Some(ns_m), Some(mem_m)) = (cap.get(1), cap.get(2)) {
                            let ns_name = ns_m.as_str();
                            let mem_name = mem_m.as_str();
                            let declared_list: Vec<&str> = enums_obj.keys().map(|s| s.as_str()).collect();
                            return Err(format!(
                                "Invalid Prisma namespace enum `Prisma.{}.{}` in {}. In Prisma v5+, enums are imported directly from '@prisma/client' (declared enums: {}).",
                                ns_name,
                                mem_name,
                                file_path,
                                if declared_list.is_empty() { "none".to_string() } else { declared_list.join(", ") }
                            ));
                        }
                    }
                }
            }
        }
    }

    if !is_api_route {
        return Ok(());
    }

    // 2. Validate URLSearchParams property access (`searchParams.id` instead of `searchParams.get('id')`)
    let uses_url_search_params = (clean_code.contains("new URL(")
        || clean_code.contains(".searchParams")
        || clean_code.contains("URLSearchParams"))
        && !clean_code.contains("searchParams: {")
        && !clean_code.contains("searchParams:{");
    if uses_url_search_params {
        const VALID_SEARCH_PARAMS_PROPS: &[&str] = &[
            "get",
            "getAll",
            "has",
            "set",
            "append",
            "delete",
            "keys",
            "values",
            "entries",
            "forEach",
            "sort",
            "toString",
            "size",
            "valueOf",
            "constructor",
            "hasOwnProperty",
        ];
        if let Ok(sp_re) = regex::Regex::new(r"\bsearchParams\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)") {
            for cap in sp_re.captures_iter(&clean_code) {
                if let Some(prop_m) = cap.get(1) {
                    let prop = prop_m.as_str();
                    if !VALID_SEARCH_PARAMS_PROPS.contains(&prop) {
                        return Err(format!(
                            "Invalid URLSearchParams access `searchParams.{}` in {}: use `searchParams.get('{}')` instead.",
                            prop, file_path, prop
                        ));
                    }
                }
            }
        }
    }

    // 3. Detect double req.json() read or `try`-scoped `body` referenced in paired `catch`
    if let Ok(try_re) = regex::Regex::new(r"\btry\s*\{") {
        let code_chars: Vec<char> = clean_code.chars().collect();
        let req_json_re = regex::Regex::new(r"\b(?:req|request)\s*\.\s*json\s*\(").unwrap();
        let body_decl_re = regex::Regex::new(
            r"\b(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=\n]+)?=\s*(?:await\s+)?(?:req|request)\s*\.\s*json\s*\(",
        )
        .unwrap();

        for m in try_re.find_iter(&clean_code) {
            let try_open_char_idx = clean_code[..m.end() - 1].chars().count();
            let Some(try_close_char_idx) = find_matching_brace_end(&code_chars, try_open_char_idx) else {
                continue;
            };
            let try_body: String = code_chars[try_open_char_idx + 1..try_close_char_idx].iter().collect();

            // Check immediately following `catch (...) { ... }`
            let mut k = try_close_char_idx + 1;
            while k < code_chars.len() && code_chars[k].is_whitespace() {
                k += 1;
            }
            let after_try: String = code_chars[k..].iter().collect();
            let catch_re = regex::Regex::new(r"^catch\s*(?:\([^)]*\))?\s*\{").unwrap();
            let Some(catch_m) = catch_re.find(&after_try) else {
                continue;
            };
            let catch_open_char_idx = k + after_try[..catch_m.end() - 1].chars().count();
            let Some(catch_close_char_idx) = find_matching_brace_end(&code_chars, catch_open_char_idx) else {
                continue;
            };
            let catch_body: String = code_chars[catch_open_char_idx + 1..catch_close_char_idx].iter().collect();

            if req_json_re.is_match(&try_body) && req_json_re.is_match(&catch_body) {
                return Err(format!(
                    "Invalid request body handling in {}: `request.json()` is called in both `try` and `catch` (stream already consumed). Parse body before `try` if needed in fallback.",
                    file_path
                ));
            }

            for cap in body_decl_re.captures_iter(&try_body) {
                if let Some(v_m) = cap.get(1) {
                    let var_name = v_m.as_str();
                    // Check if catch redeclares it
                    let redecl_pat = format!(r"\b(?:const|let|var)\s+{}\b", regex::escape(var_name));
                    let redeclared_in_catch = regex::Regex::new(&redecl_pat)
                        .ok()
                        .map_or(false, |re| re.is_match(&catch_body));
                    if redeclared_in_catch {
                        continue;
                    }
                    // Check standalone reference in catch_body (not preceded by `.` or `:` property key)
                    let ref_pat = format!(r"\b{}\b", regex::escape(var_name));
                    if let Ok(ref_re) = regex::Regex::new(&ref_pat) {
                        for r_m in ref_re.find_iter(&catch_body) {
                            let prev_ch = catch_body[..r_m.start()].trim_end().chars().next_back();
                            if prev_ch != Some('.') {
                                return Err(format!(
                                    "Out-of-scope variable `{}` in {}: declared inside `try` and referenced inside `catch` (causes TS2304). Hoist declaration before `try`.",
                                    var_name, file_path
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn validate_file_with_abstract_rules(
    file_path: &str,
    content: &str,
    _model_id: &str,
) -> Result<(), String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("Generated file content is empty.".to_string());
    }

    let path_lower = file_path.replace('\\', "/").to_lowercase();
    let is_code_file = path_lower.ends_with(".ts")
        || path_lower.ends_with(".tsx")
        || path_lower.ends_with(".js")
        || path_lower.ends_with(".jsx");
    let is_css_file = path_lower.ends_with(".css") || path_lower.ends_with(".scss");
    let is_prisma_file = path_lower.ends_with(".prisma");

    // 1. Reject residual markdown fences or prose explanations outside JSON/Markdown
    if (is_code_file || is_css_file || is_prisma_file) && trimmed.starts_with("```") {
        return Err("File contains raw markdown code fences instead of pure source code.".to_string());
    }

    if is_code_file || is_css_file {
        let lower_sample = trimmed.chars().take(1500).collect::<String>().to_lowercase();
        let has_prose_leak = trimmed.contains("The error indicates")
            || trimmed.contains("Here is the corrected")
            || trimmed.contains("To fix this")
            || (lower_sample.contains("1. ensure") && lower_sample.contains("2. "));
        if has_prose_leak {
            return Err("Output contains explanatory prose or numbered steps mixed into code.".to_string());
        }
    }

    // 2. Repeating CSS block loop check
    if is_css_file {
        let deduped = crate::sanitizer::deduplicate_repeated_css_blocks(trimmed);
        if trimmed.len() > 256 && deduped.len() * 2 < trimmed.len() {
            return Err("CSS file contains repeating duplicate rule blocks (generation loop).".to_string());
        }
        return Ok(());
    }

    if !is_code_file {
        return Ok(());
    }

    let is_app_route = crate::sanitizer::is_app_router_route_file(file_path);

    // 3. Domain model isolation: never instantiate PrismaClient or import @/lib/prisma in src/models/*
    if !is_app_route && (path_lower.contains("/models/") || path_lower.starts_with("models/")) && !path_lower.contains("/api/") {
        if trimmed.contains("new PrismaClient") || trimmed.contains("@/lib/prisma") {
            return Err("Files under src/models/ must define pure TypeScript types/interfaces ONLY. Do NOT import @/lib/prisma or instantiate new PrismaClient().".to_string());
        }
        if (trimmed.contains("'@prisma/client'") || trimmed.contains("\"@prisma/client\""))
            && !trimmed.contains("import type")
        {
            return Err("Files under src/models/ must NOT import runtime symbols from '@prisma/client'. Use pure TypeScript interfaces.".to_string());
        }
    }

    // 4. Next.js Edge Middleware validation
    if path_lower.ends_with("middleware.ts") || path_lower.ends_with("middleware.js") {
        if trimmed.contains("'express'") || trimmed.contains("\"express\"") || trimmed.contains("NextFunction") {
            return Err("Next.js middleware must use NextRequest/NextResponse from 'next/server', never Express.".to_string());
        }
        let has_valid_export = trimmed.contains("export function middleware")
            || trimmed.contains("export async function middleware")
            || trimmed.contains("export default")
            || trimmed.contains("withAuth");
        if !has_valid_export {
            return Err("Next.js middleware must export function `middleware` or default middleware.".to_string());
        }
        // Reject unnamed glob wildcards like '/dashboard/*' or '/**' (path-to-regexp crashes at runtime)
        if let Ok(bad_glob_re) = regex::Regex::new(r#"['"][^'"]*?/(\*\*|\*)['"]"#) {
            if let Some(m) = bad_glob_re.find(trimmed) {
                let lit = m.as_str();
                if !lit.contains("((") && !lit.contains(":path*") {
                    return Err(format!(
                        "Next.js middleware `config.matcher` entry {} uses invalid glob wildcard syntax (`/*`). Use named catch-all `/:path*` or regex negative lookahead `/((?!api|_next|_vercel|.*\\..*).*)'`.",
                        lit
                    ));
                }
            }
        }
    }

    // 5. Client component & hook boundary validation
    let is_client_file = trimmed.starts_with("\"use client\"")
        || trimmed.starts_with("'use client'")
        || trimmed.lines().take(5).any(|l| {
            let t = l.trim();
            t == "\"use client\";" || t == "\"use client\"" || t == "'use client';" || t == "'use client'"
        });
    let is_hook_file = !is_app_route && (path_lower.contains("/hooks/") || path_lower.starts_with("hooks/")) && !path_lower.contains("/api/");
    let is_route_handler = path_lower.ends_with("route.ts") || path_lower.ends_with("route.js");
    if (is_client_file || is_hook_file) && !is_route_handler {
        if is_client_file && trimmed.contains("export default async function") {
            return Err("Client Component ('use client') cannot be declared as `export default async function`.".to_string());
        }
        if trimmed.contains("new PrismaClient") {
            return Err("Client Component / hook must NOT instantiate PrismaClient.".to_string());
        }
        let clean_client_code = strip_ts_js_comments_preserving_strings(trimmed);
        if let Ok(client_prisma_re) = regex::Regex::new(
            r"\bprisma\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)\s*\.\s*(findMany|findUnique|findFirst|findUniqueOrThrow|findFirstOrThrow|create|createMany|update|updateMany|upsert|delete|deleteMany|count|aggregate|groupBy)\s*\(",
        ) {
            if let Some(cap) = client_prisma_re.captures(&clean_client_code) {
                let del_name = cap.get(1).map_or("model", |m| m.as_str());
                return Err(format!(
                    "Client Component / hook must NOT call `prisma.{}` directly; fetch from '/api/...' endpoints instead.",
                    del_name
                ));
            }
        }
        let raw_lines: Vec<&str> = trimmed.lines().collect();
        let mut idx_l = 0usize;
        while idx_l < raw_lines.len() {
            let t = raw_lines[idx_l].trim();
            if t.starts_with("import ") || t.starts_with("import{") {
                let mut end_l = idx_l;
                let mut stmt = raw_lines[idx_l].to_string();
                let is_side_effect = t.starts_with("import '") || t.starts_with("import \"");
                if !is_side_effect {
                    while !stmt.contains(" from ") && !stmt.contains("from'") && !stmt.contains("from\"") && !stmt.trim_end().ends_with(';') && end_l + 1 < raw_lines.len() {
                        end_l += 1;
                        stmt.push('\n');
                        stmt.push_str(raw_lines[end_l]);
                    }
                }
                let flat = stmt.replace('\n', " ");
                let flat_trim = flat.trim();
                let is_explicit_type = flat_trim.starts_with("import type ") || flat_trim.starts_with("import type{");
                let is_inline_all_types = if !is_explicit_type && flat_trim.contains('{') && flat_trim.contains('}') {
                    if let (Some(ob), Some(cb)) = (flat_trim.find('{'), flat_trim.find('}')) {
                        if ob < cb {
                            let inner = &flat_trim[ob + 1..cb];
                            let items: Vec<&str> = inner.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                            !items.is_empty() && items.iter().all(|it| it.starts_with("type "))
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                let is_type_only = is_explicit_type || is_inline_all_types;
                if !is_type_only {
                    if let Some(spec) = crate::sanitizer::extract_import_specifier(flat_trim) {
                        let s_low = spec.to_lowercase();
                        let is_server_mod = s_low == "@prisma/client"
                            || s_low == "@/lib/prisma"
                            || s_low == "~/lib/prisma"
                            || s_low.ends_with("/lib/prisma");
                        let is_api_route_import = s_low.starts_with("@/app/api/")
                            || s_low.starts_with("~/app/api/")
                            || s_low.starts_with("src/app/api/")
                            || s_low.starts_with("app/api/")
                            || (s_low.contains("/api/")
                                && (s_low.ends_with("/route")
                                    || s_low.ends_with("/route.ts")
                                    || s_low.ends_with("/route.js")));
                        if is_server_mod {
                            return Err("Client Component / hook must NOT import @/lib/prisma or runtime symbols from @prisma/client.".to_string());
                        }
                        if is_api_route_import {
                            return Err("Client Component / hook must NOT import Next.js API route handlers (/api/**/route); call fetch('/api/...') instead.".to_string());
                        }
                    }
                }
                idx_l = end_l + 1;
                continue;
            }
            idx_l += 1;
        }
    }

    // 5b. App Router route boundary: forbid importing App Router page/layout route modules from any non-test TS/JS file
    let is_test_file = path_lower.contains(".test.")
        || path_lower.contains(".spec.")
        || path_lower.contains("__tests__");
    if !is_test_file {
        let is_app_router_route_import_spec = |spec: &str| -> bool {
            let raw = spec.split(['?', '#']).next().unwrap_or(spec).trim().replace('\\', "/");
            let s_low = raw.to_lowercase();
            let resolved_opt = if let Some(sub) = s_low.strip_prefix("@/app/") {
                Some(format!("src/app/{}", sub))
            } else if let Some(sub) = s_low.strip_prefix("~/app/") {
                Some(format!("src/app/{}", sub))
            } else if s_low.starts_with("src/app/") || s_low.starts_with("app/") {
                Some(s_low.clone())
            } else if s_low.starts_with("./") || s_low.starts_with("../") {
                let norm_cur = path_lower.trim_start_matches("./");
                let parent = std::path::Path::new(norm_cur).parent().unwrap_or_else(|| std::path::Path::new(""));
                let joined = parent.join(&s_low);
                let mut parts: Vec<String> = Vec::new();
                for comp in joined.components() {
                    match comp {
                        std::path::Component::Normal(c) => parts.push(c.to_string_lossy().to_string()),
                        std::path::Component::ParentDir => {
                            parts.pop();
                        }
                        _ => {}
                    }
                }
                let joined_str = parts.join("/");
                if joined_str.starts_with("src/app/") || joined_str.starts_with("app/") {
                    Some(joined_str)
                } else {
                    None
                }
            } else {
                None
            };
            let Some(resolved) = resolved_opt else {
                return false;
            };
            if resolved.starts_with("src/app/api/")
                || resolved.starts_with("app/api/")
                || resolved.contains("/api/")
            {
                return false;
            }
            let last_seg = resolved.rsplit('/').next().unwrap_or("");
            let stem = last_seg
                .strip_suffix(".tsx")
                .or_else(|| last_seg.strip_suffix(".ts"))
                .or_else(|| last_seg.strip_suffix(".jsx"))
                .or_else(|| last_seg.strip_suffix(".js"))
                .unwrap_or(last_seg);
            matches!(
                stem,
                "page" | "layout" | "loading" | "error" | "not-found" | "template" | "default" | "global-error"
            )
        };

        let raw_lines: Vec<&str> = trimmed.lines().collect();
        let mut idx_l = 0usize;
        while idx_l < raw_lines.len() {
            let t = raw_lines[idx_l].trim();
            if t.starts_with("import ") || t.starts_with("import{") || (t.starts_with("export ") && t.contains(" from ")) {
                let mut end_l = idx_l;
                let mut stmt = raw_lines[idx_l].to_string();
                let is_side_effect = t.starts_with("import '") || t.starts_with("import \"");
                if !is_side_effect {
                    while !stmt.contains(" from ") && !stmt.contains("from'") && !stmt.contains("from\"") && !stmt.trim_end().ends_with(';') && end_l + 1 < raw_lines.len() {
                        end_l += 1;
                        stmt.push('\n');
                        stmt.push_str(raw_lines[end_l]);
                    }
                }
                let flat = stmt.replace('\n', " ");
                if let Some(spec) = crate::sanitizer::extract_import_specifier(flat.trim()) {
                    if is_app_router_route_import_spec(&spec) {
                        return Err(format!(
                            "Forbidden import from Next.js App Router route module `{}`. Move shared types/components into `src/types/` or `src/components/` and import from there.",
                            spec
                        ));
                    }
                }
                idx_l = end_l + 1;
                continue;
            }
            idx_l += 1;
        }
    }

    // 5c. App Router page/layout/error/loading files MUST export a default React Component
    if is_app_route && !is_route_handler {
        let uncommented = strip_ts_js_comments_preserving_strings(trimmed);
        let has_default_export = regex::Regex::new(r"(?m)(?:^\s*|\b)export\s+default\b")
            .map(|re| re.is_match(&uncommented))
            .unwrap_or_else(|_| uncommented.contains("export default"))
            || regex::Regex::new(r"(?m)^\s*export\s*\{[^}]*\bdefault\b")
                .map(|re| re.is_match(&uncommented))
                .unwrap_or(false);
        if !has_default_export {
            return Err(format!(
                "Next.js App Router route `{}` MUST export a default React Component (`export default function ...`). Never put standalone interfaces/types/logic without a default component export; move shared types into `src/types/`.",
                file_path
            ));
        }
        if let Ok(re_non_comp_default) = regex::Regex::new(
            r#"(?m)^\s*export\s+default\s*(\{|\[|null\b|undefined\b|true\b|false\b|['"`\d])"#,
        ) {
            if re_non_comp_default.is_match(&uncommented) {
                return Err(format!(
                    "Next.js App Router route `{}` default export must be a React Component function/class, not a plain object, array, or literal.",
                    file_path
                ));
            }
        }
        let is_layout_route = path_lower == "layout.tsx"
            || path_lower == "layout.jsx"
            || path_lower.ends_with("/layout.tsx")
            || path_lower.ends_with("/layout.jsx");
        if is_layout_route {
            let has_children = if let Ok(r) = regex::Regex::new(r"\{\s*children\s*\}") {
                r.find_iter(&uncommented).any(|m| {
                    let next_ch = uncommented[m.end()..].chars().find(|c| !c.is_whitespace());
                    !matches!(next_ch, Some(':') | Some(',') | Some('=') | Some(')'))
                })
            } else {
                uncommented.contains("{children}")
            };
            if !has_children {
                return Err(format!(
                    "Next.js App Router layout `{}` MUST render `{{children}}` inside its JSX layout wrapper (`<main>{{children}}</main>`).",
                    file_path
                ));
            }
            if (path_lower == "src/app/layout.tsx"
                || path_lower == "src/app/layout.jsx"
                || path_lower == "app/layout.tsx"
                || path_lower == "app/layout.jsx"
                || path_lower == "layout.tsx"
                || path_lower == "layout.jsx")
                && (uncommented.to_lowercase().contains("<table")
                    || uncommented.contains("from '@/hooks/")
                    || uncommented.contains("from \"@/hooks/"))
            {
                return Err(format!(
                    "RootLayout `{}` must contain ONLY global shell (Navbar/Footer/Providers/`{{children}}`) — NEVER embed `<table>`, page dashboards, or domain data hooks (`@/hooks/*`) inside RootLayout.",
                    file_path
                ));
            }
        }
    }

    // 6. globals.css restriction & bare imports check
    let is_layout = path_lower.ends_with("layout.tsx") || path_lower.ends_with("layout.jsx") || path_lower.ends_with("_app.tsx") || path_lower.ends_with("_app.jsx");
    if !is_layout && (trimmed.contains("globals.css'") || trimmed.contains("globals.css\"")) {
        return Err("Only root layout files may import globals.css.".to_string());
    }
    if trimmed.contains("from 'react-icons'") || trimmed.contains("from \"react-icons\"") {
        return Err("Never import from bare 'react-icons'; use a family subpath like 'react-icons/hi'.".to_string());
    }
    if trimmed.contains("from 'src/") || trimmed.contains("from \"src/") {
        return Err("Never use bare 'src/...' module specifiers; always use '@/...' alias.".to_string());
    }
    if trimmed.contains("from 'next-auth/providers'")
        || trimmed.contains("from \"next-auth/providers\"")
        || trimmed.contains("from 'next-auth/client'")
        || trimmed.contains("from \"next-auth/client\"")
    {
        return Err("Deprecated NextAuth v3 import (`next-auth/providers` or `next-auth/client`). Use NextAuth v4 subpaths (e.g. `CredentialsProvider` from `'next-auth/providers/credentials'`, `useSession` from `'next-auth/react'`).".to_string());
    }

    // Reject foreign frontend frameworks and invalid Next.js/NextAuth named imports in TS/JS files
    let foreign_import_re = regex::Regex::new(
        r#"(?m)^\s*import\s+[\s\S]*?\s+from\s+['"](vue|vue-i18n|vue-router|pinia|vuex|nuxt|svelte|@sveltejs/kit|@angular/core|@angular/common)(?:/[^'"]*)?['"]"#,
    );
    if let Ok(re_foreign) = foreign_import_re {
        if let Some(cap) = re_foreign.captures(trimmed) {
            let pkg = cap.get(1).map_or("unknown", |m| m.as_str());
            return Err(format!(
                "Foreign framework package `{}` cannot be imported in a Next.js / React project. Implement with React/Next.js primitives only.",
                pkg
            ));
        }
    }
    if let Ok(re_named_link) = regex::Regex::new(r#"(?m)^\s*import\s+\{[^}]*\bLink\b[^}]*\}\s+from\s+['"]next/link['"]"#) {
        if re_named_link.is_match(trimmed) {
            return Err("`next/link` has NO named export `{ Link }`; use default import `import Link from 'next/link';`.".to_string());
        }
    }
    if let Ok(re_na_prov_named) = regex::Regex::new(
        r#"(?m)^\s*import\s+(?:[A-Za-z_$][A-Za-z0-9_$]*\s*,\s*)?\{[^}]*\b(CredentialsProvider|GoogleProvider|GitHubProvider|GithubProvider|EmailProvider|DiscordProvider)\b[^}]*\}\s+from\s+['"]next-auth['"]"#,
    ) {
        if let Some(cap) = re_na_prov_named.captures(trimmed) {
            let prov = cap.get(1).map_or("CredentialsProvider", |m| m.as_str());
            return Err(format!(
                "`{}` is NOT a named export of `'next-auth'`; import it via its provider subpath (e.g. `import CredentialsProvider from 'next-auth/providers/credentials';`).",
                prov
            ));
        }
    }

    // 7. Basic bracket balance check to catch mid-stream truncation
    let mut open_curly = 0isize;
    let mut close_curly = 0isize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_tick = false;
    let mut escape = false;
    for ch in trimmed.chars() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && (in_single || in_double || in_tick) {
            escape = true;
            continue;
        }
        if ch == '\'' && !in_double && !in_tick {
            in_single = !in_single;
        } else if ch == '"' && !in_single && !in_tick {
            in_double = !in_double;
        } else if ch == '`' && !in_single && !in_double {
            in_tick = !in_tick;
        } else if !in_single && !in_double && !in_tick {
            if ch == '{' {
                open_curly += 1;
            } else if ch == '}' {
                close_curly += 1;
            }
        }
    }
    if open_curly > close_curly + 2 {
        return Err(format!("Generated code appears truncated: {} unclosed '{{' block(s).", open_curly - close_curly));
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
        if !dir.exists() {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules"
                        && name != ".next"
                        && name != "build"
                        && name != "dist"
                        && name != ".axiom"
                        && name != ".git"
                        && name != "target"
                    {
                        scan_dir(&path, source_files);
                    }
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" || ext == "mjs" || ext == "mts" {
                            source_files.push(path);
                        }
                    }
                }
            }
        }
    }

    scan_dir(proj_dir, &mut source_files);
    source_files.sort();

    let import_from_re = regex::Regex::new(
        r#"(?ms)(?:^|;|\n)\s*import\s+(?:type\s+)?(?:[A-Za-z0-9_$\*]+(?:\s+as\s+[A-Za-z0-9_$]+)?(?:\s*,\s*\{[^}]*\})?|\{[^}]*\})\s+from\s+['"]([^'"]+)['"]"#,
    )
    .ok();
    let side_effect_import_re = regex::Regex::new(r#"(?m)^\s*import\s+['"]([^'"]+)['"]"#).ok();
    let reexport_re = regex::Regex::new(
        r#"(?ms)(?:^|;|\n)\s*export\s+(?:\*(?:\s+as\s+[A-Za-z0-9_$]+)?|\{[^}]*\})\s+from\s+['"]([^'"]+)['"]"#,
    )
    .ok();
    let require_or_dyn_re = regex::Regex::new(r#"(?:\brequire|import)\s*\(\s*['"]([^'"]+)['"]\s*\)"#).ok();

    let export_decl_re = regex::Regex::new(
        r"(?m)^\s*export\s+(?:async\s+)?(?:function|const|let|var|type|interface|class|enum)\s+([A-Za-z0-9_$]+)",
    )
    .ok();
    let export_list_re = regex::Regex::new(r"(?ms)^\s*export\s*\{([^}]+)\}").ok();
    let method_fn_re = regex::Regex::new(
        r"(?m)^(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>\n]*>)?\s*\(([^)\n]*)\)",
    )
    .ok();
    let method_arrow_re = regex::Regex::new(
        r"(?m)^(?:export\s+)?const\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=\n]+)?=\s*(?:async\s+)?(?:\(([^)\n]*)\)|([A-Za-z_][A-Za-z0-9_]*))\s*=>",
    )
    .ok();

    for file_path in &source_files {
        let Ok(content) = fs::read_to_string(file_path) else {
            continue;
        };
        let Ok(rel_path) = file_path.strip_prefix(proj_dir) else {
            continue;
        };
        let rel_path_str = rel_path.to_string_lossy().replace('\\', "/");

        if let Some(route) = resolve_app_router_route(&rel_path_str) {
            routes_map.insert(
                route,
                serde_json::json!({
                    "file": rel_path_str.clone()
                }),
            );
        }

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut local_deps = Vec::new();

        let mut push_import = |spec: &str| {
            let cleaned = spec.trim();
            if cleaned.is_empty() {
                return;
            }
            if !imports.contains(&cleaned.to_string()) {
                imports.push(cleaned.to_string());
            }

            let dep_base_opt: Option<PathBuf> = if let Some(rest) = cleaned.strip_prefix("@/").or_else(|| cleaned.strip_prefix("~/")) {
                Some(proj_dir.join("src").join(rest))
            } else if cleaned.starts_with("./") || cleaned.starts_with("../") {
                file_path.parent().map(|p| p.join(cleaned))
            } else {
                None
            };

            if let Some(dep_base) = dep_base_opt {
                let mut candidates = Vec::new();
                if dep_base.extension().is_some() {
                    candidates.push(dep_base.clone());
                } else {
                    for ext in ["ts", "tsx", "js", "jsx", "mjs", "mts"] {
                        candidates.push(dep_base.with_extension(ext));
                    }
                    for ext in ["ts", "tsx", "js", "jsx"] {
                        candidates.push(dep_base.join(format!("index.{}", ext)));
                    }
                }
                for cand in candidates {
                    if cand.is_file() {
                        if let Ok(canon) = std::fs::canonicalize(&cand) {
                            if let Ok(canon_root) = std::fs::canonicalize(proj_dir) {
                                if let Ok(dep_rel) = canon.strip_prefix(&canon_root) {
                                    let rel_s = dep_rel.to_string_lossy().replace('\\', "/");
                                    if !local_deps.contains(&rel_s) {
                                        local_deps.push(rel_s);
                                    }
                                    break;
                                }
                            }
                        }
                        if let Ok(dep_rel) = cand.strip_prefix(proj_dir) {
                            let rel_s = dep_rel.to_string_lossy().replace('\\', "/");
                            if !local_deps.contains(&rel_s) {
                                local_deps.push(rel_s);
                            }
                            break;
                        }
                    }
                }
            }
        };

        for re_opt in [&import_from_re, &side_effect_import_re, &reexport_re, &require_or_dyn_re] {
            if let Some(re) = re_opt {
                for cap in re.captures_iter(&content) {
                    if let Some(m) = cap.get(1) {
                        push_import(m.as_str());
                    }
                }
            }
        }

        if content.contains("export default") {
            exports.push("default".to_string());
        }
        if let Some(ref re) = export_decl_re {
            for cap in re.captures_iter(&content) {
                if let Some(m) = cap.get(1) {
                    let name = m.as_str().trim();
                    if !name.is_empty() && name != "default" && !exports.contains(&name.to_string()) {
                        exports.push(name.to_string());
                    }
                }
            }
        }
        if let Some(ref re) = export_list_re {
            for cap in re.captures_iter(&content) {
                if let Some(list_m) = cap.get(1) {
                    for raw_item in list_m.as_str().split(',') {
                        let cleaned = raw_item
                            .split_whitespace()
                            .last()
                            .unwrap_or("")
                            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '$');
                        if !cleaned.is_empty() && cleaned != "type" && !exports.contains(&cleaned.to_string()) {
                            exports.push(cleaned.to_string());
                        }
                    }
                }
            }
        }

        let mut push_method = |name: &str, raw_params: &str| {
            let clean_name = name.trim();
            if clean_name.is_empty()
                || matches!(
                    clean_name,
                    "default" | "if" | "for" | "while" | "switch" | "catch" | "return" | "typeof" | "await"
                )
            {
                return;
            }
            let params: Vec<String> = raw_params
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            methods_map.insert(
                clean_name.to_string(),
                serde_json::json!({
                    "definedIn": rel_path_str.clone(),
                    "parameters": params,
                    "returns": "any"
                }),
            );
        };

        if let Some(ref re) = method_fn_re {
            for cap in re.captures_iter(&content) {
                if let Some(n) = cap.get(1) {
                    let params = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                    push_method(n.as_str(), params);
                }
            }
        }
        if let Some(ref re) = method_arrow_re {
            for cap in re.captures_iter(&content) {
                if let Some(n) = cap.get(1) {
                    let params = cap
                        .get(2)
                        .or_else(|| cap.get(3))
                        .map(|m| m.as_str())
                        .unwrap_or("");
                    push_method(n.as_str(), params);
                }
            }
        }

        files_map.insert(
            rel_path_str,
            serde_json::json!({
                "exports": exports,
                "imports": imports,
                "dependencies": local_deps
            }),
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

pub const NEXTJS_MIDDLEWARE_RULEPACK: &str = r#"NEXT.JS MIDDLEWARE & EDGE ROUTING RULES (TWO-LAYER METACOGNITIVE HANDBOOK):
LAYER A — ALWAYS IGNORE IN `config.matcher` (ZERO BODY DUPLICATION):
1. PATH-TO-REGEXP MATCHERS: `config.matcher` strictly parses paths using `path-to-regexp`. Wildcards MUST use named catch-alls (`'/dashboard/:path*'`) or negative lookahead regex (`'/((?!api|_next/static|_next/image|_vercel|favicon\\.ico|robots\\.txt|sitemap\\.xml|.*\\..*).*)'`). NEVER write unnamed globs (`'/dashboard/*'`, `'/**'`) which crash at startup.
2. STATIC ASSETS AT MATCHER LEVEL ONLY: Exclude `/_next/static`, `/_next/image`, `/_vercel`, `favicon.ico`, `robots.txt`, `sitemap.xml`, and dotted static files (`.*\\..*`) strictly inside `config.matcher`. Once excluded in `config.matcher`, do NOT repeat redundant `pathname.includes('/_next/')` or `pathname.includes('.')` branches in the `middleware()` body — keeping Edge CPU/latency minimal.
3. SINGLE ROOT/SRC MIDDLEWARE FILE & EDGE RUNTIME BOUNDARY: Next.js allows strictly ONE middleware file at `src/middleware.ts` (or root `middleware.ts`). NEVER import Node core packages (`fs`, `path`, `child_process`, `crypto`) or PrismaClient inside middleware.

LAYER B — SERVICE-SPECIFIC {metakognitivan_spisak_pravila}:
4. NEXT-AUTH SESSIONS: Inspect JWT via `withAuth` or `getToken({ req: request, secret: process.env.NEXTAUTH_SECRET })`. Run auth ONLY on protected prefixes (`/dashboard`, `/admin`, `/profile`, `/settings`) and explicitly allow public/auth routes (`/login`, `/register`, `/api/auth`).
5. PAYMENT WEBHOOK BYPASS (STRIPE / PADDLE): When webhooks are present, NEVER redirect, rewrite, or inject auth walls on `/api/stripe/webhook` or `/api/webhooks` — webhooks require unauthenticated POSTs with untouched raw request bodies for HMAC signature verification.
6. MULTI-LANGUAGE & MULTI-TENANT OPTIMIZATION: Check fast env gates (`MAINTENANCE_MODE`, `WAITLIST_MODE`) BEFORE expensive auth checks; strip or resolve locale/tenant prefixes once; if internal `/api/` translation/rewrite is needed across locale or tenant subdomains, use `NextResponse.rewrite(...)` — NEVER `NextResponse.redirect(...)` (redirects break API methods & CORS preflights). Set only lightweight routing headers (`x-locale` / `x-tenant`) without gratuitous cookies on non-HTML fetches."#;

/// Builds dynamic Layer-B middleware rules (`{metakognitivan_spisak_pravila}`) tailored to
/// services and files actually detected in the current workspace or prompt context.
pub fn get_middleware_metacognitive_rules(proj_dir: Option<&std::path::Path>, context_text: &str) -> String {
    let mut pkg_json = String::new();
    let mut has_translations_dir = false;
    let mut has_stripe_webhook = false;
    if let Some(dir) = proj_dir {
        if let Ok(raw) = std::fs::read_to_string(dir.join("package.json")) {
            pkg_json = raw.to_lowercase();
        }
        has_translations_dir = dir.join("src/i18n").is_dir()
            || dir.join("src/messages").is_dir()
            || dir.join("src/translations").is_dir()
            || dir.join("messages").is_dir();
        has_stripe_webhook = dir.join("src/app/api/stripe").is_dir()
            || dir.join("src/app/api/webhooks").is_dir()
            || dir.join("app/api/stripe").is_dir();
    }

    let ctx_low = context_text.to_lowercase();
    let has_nextauth = pkg_json.contains("\"next-auth\"") || ctx_low.contains("next-auth") || ctx_low.contains("withauth") || ctx_low.contains("gettoken");
    let has_stripe = has_stripe_webhook || pkg_json.contains("\"stripe\"") || ctx_low.contains("stripe") || ctx_low.contains("webhook");
    let has_i18n = has_translations_dir
        || pkg_json.contains("\"next-intl\"")
        || pkg_json.contains("\"next-i18next\"")
        || ctx_low.contains("next-intl")
        || ctx_low.contains("locale")
        || ctx_low.contains("tenant");

    let mut rules = Vec::new();
    rules.push("- Layer A (Always Ignore via `config.matcher`): exclude `/api` (unless scoped webhook/i18n API routing is needed), `/_next/static`, `/_next/image`, `/_vercel`, `favicon.ico`, `robots.txt`, `sitemap.xml`, and `.*\\..*` directly in `export const config = { matcher: [...] }`. Keep the `middleware()` function body free of duplicate static-extension checks.".to_string());

    if has_nextauth {
        rules.push("- Service Detected (`next-auth`): Protect dashboard/admin/profile prefixes while keeping `/login`, `/register`, `/api/auth`, and public marketing routes accessible without redirect loops.".to_string());
    }
    if has_stripe {
        rules.push("- Service Detected (`stripe` / webhooks): Ensure `/api/stripe/webhook` and `/api/webhooks` bypass auth/locale middleware completely so Stripe signatures verify cleanly.".to_string());
    }
    if has_i18n {
        rules.push("- Service Detected (`next-intl` / i18n / multi-tenant): Evaluate `process.env.MAINTENANCE_MODE` / `WAITLIST_MODE` env flags first, resolve locale or tenant host in O(1), and rewrite internal API calls with `NextResponse.rewrite` (never `NextResponse.redirect`).".to_string());
    }

    format!(
        "\nMETACOGNITIVE MIDDLEWARE CHECKLIST ({{metakognitivan_spisak_pravila}}):\n{}\n",
        rules.join("\n")
    )
}

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
3. RELATION FOREIGN-KEY PLACEMENT: In `schema.prisma`, the scalar foreign key (`authorId String`) and the `@relation(fields: [authorId], references: [id])` attribute MUST be defined on the model that stores the foreign key. For one-to-many relations, the parent defines only the back-relation array (`posts Post[]`). For one-to-one relations (where parent defines singular `profile Profile?`), the foreign key on the child MUST be marked `@unique` (`userId String @unique`). Between any two models, define exactly one clean relation pair unless multiple distinct foreign keys exist. Never define self-relations unless building an explicit hierarchical tree (`parent Model?` and `children Model[]`).
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

pub const METACOGNITIVE_UI_STATE_RULEPACK: &str = r#"METACOGNITIVE UI COMPONENT STATE & INTERACTION PROTOCOL (MANDATORY FOR ALL UI FILES):
You are writing a React UI component. Before emitting the JSX markup, you MUST follow this strict 3-phase cognitive sequence:

PHASE 1 — INTERACTIVE STATE & USER ACTION IDENTIFICATION:
Before writing the `return (...)` JSX block, systematically evaluate all interactive elements required by this component's specific domain logic (such as drawer or dialog toggles, navigation tabs, filter selections, text or numeric inputs, collection lists, and loading indicators). For every interactive element, determine its semantic category, exact type, and logical initial value:
- Binary toggles (visibility flags, open/closed controls, boolean switches): initialize with an explicit boolean (`false` or `true`).
- Discrete selectors (active tabs, category filters, text queries): initialize with an empty string or the first valid option.
- Ordered collections (data rows, selected identifier arrays, tag lists): initialize with a domain-realistic mock array or an empty array `[]`.
- Single records or nullable entities: initialize with `null` or a structured default object.

PHASE 2 — MANDATORY STATE DECLARATION (BEFORE ANY JSX):
Every single piece of reactive state MUST be explicitly declared via `useState(...)` at the very top of the component function body, immediately following external routing or session hooks, and strictly BEFORE any helper functions, event handlers, or JSX return statements.
Always ensure `import { useState } from 'react';` is present at the top of the file whenever local state is declared.

PHASE 3 — STRICT JSX USAGE ENFORCEMENT:
In the `return (...)` JSX block, every identifier referenced in an `onClick`, `onChange`, conditional expression, ternary condition, or property binding MUST strictly originate from one of these three explicit sources:
1. Declared in the component's props interface and destructured in the function signature.
2. Explicitly declared via a React hook (`useState`, `useMemo`, `useCallback`) in Phase 2.
3. Imported at the top of the file (components, icons, utilities).
NEVER emit JSX that references an undeclared identifier or an undeclared state setter. Every variable used in JSX conditions or handlers MUST exist in the component's local scope before the JSX return begins."#;

pub const UI_NAVBAR_RULEPACK: &str = r#"SCOPED NAVBAR, BRAND LOGO & HAMBURGER MENU RULES:
1. STICKY GLASSMORPHISM HEADER: Structure the navbar as `<header className="sticky top-0 z-50 w-full backdrop-blur-md bg-white/90 border-b border-slate-200/80 shadow-sm">` with an inner container `<div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center justify-between gap-4">`.
2. INLINE SVG BRAND LOGO LOCKUP (NO BROKEN IMAGE FILES & NO HAMBURGER ICON IN BRAND BADGE):
   - NEVER use `<img src="/logo.png" />` or `<Image src="/logo.svg" />` (which cause 404 broken image icons), NEVER render plain unstyled text alone, and NEVER place a hamburger menu icon (`<HiMenu />`, `<FiMenu />`, `<HiOutlineMenu />`) inside the Brand Logo Mark Badge!
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
1. BAN TALL VERTICAL LINK STACKS & BAN BUTTON-STYLED LINKS: NEVER stack all footer links vertically in a single centered column (`flex-col items-center space-y-4` is STRICTLY FORBIDDEN), and NEVER style footer `<Link>` items as solid primary buttons (`bg-indigo-600`, `bg-teal-600`, `px-4 py-2.5`, `rounded-xl` are STRICTLY FORBIDDEN on footer links!).
2. COMPACT HORIZONTAL / MULTI-COLUMN FOOTER:
   - Use `<footer className="mt-auto w-full bg-slate-900 text-slate-400 border-t border-slate-800 py-8">`.
   - Inside `<div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col md:flex-row items-center justify-between gap-6">`:
     a) Left side: Compact Brand Lockup (small gradient rounded-lg icon + bold white brand name) and `<p className="text-xs text-slate-500 mt-1">&copy; {new Date().getFullYear()} Brand. All rights reserved.</p>`.
     b) Right side: `<nav className="flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm">` so all footer links sit horizontally in a single clean, compact row (`className="text-sm text-slate-400 hover:text-white transition-colors"`)."#;

pub const UI_PRIMITIVES_RULEPACK: &str = r#"SCOPED UI PRIMITIVES RULES (src/components/ui/*):
1. EXTEND NATIVE HTML ATTRIBUTES: Always extend standard React HTML interfaces (`export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>`, `export interface CardProps extends React.HTMLAttributes<HTMLDivElement>`, `export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement>`) and spread `{...props}` onto the underlying DOM element.
2. SAFE CLASSNAME DEFAULT & ASCHILD DESTRUCTURING: Always default `className = ''` in props destructuring (`({ className = '', children, ...props })`) so template literals never output `"undefined"`. In `button.tsx`, accept `asChild?: boolean` in `ButtonProps` and destructure it out (`{ asChild, variant = 'default', size = 'md', className = '', children, ...props }`) so `asChild` is never passed as an invalid DOM attribute to `<button>`.
3. POLISHED BASE CLASSES:
   - `Button`: `inline-flex items-center justify-center gap-2 rounded-xl font-semibold text-sm px-4 py-2.5 transition-all focus:outline-none disabled:opacity-50` with variants (`default`: `bg-indigo-600 text-white hover:bg-indigo-700 shadow-sm`, `outline`: `border border-slate-200 bg-white text-slate-700 hover:bg-slate-50`, `ghost`: `text-slate-600 hover:bg-slate-100`, `destructive`: `bg-rose-600 text-white hover:bg-rose-700`).
   - `Card`: `bg-white rounded-2xl border border-slate-200/80 shadow-sm` (`CardHeader`: `p-6 pb-3`, `CardTitle`: `text-lg font-bold text-slate-900`, `CardContent`: `p-6 pt-0`).
   - `Skeleton`: `animate-pulse rounded-xl bg-slate-200/80`.
4. MANDATORY DUAL EXPORTS: Export BOTH named and default exports (`export { Button }; export default Button;`)."#;

pub const UI_PAGE_AND_DASHBOARD_RULEPACK: &str = r#"SCOPED PAGE & DASHBOARD PREVIEW RESILIENCE RULES:
1. ZERO AUTH-WALL / ZERO INFINITE LOADING / ZERO LOGIN REDIRECT IN PREVIEW (CRITICAL):
   - NEVER block page rendering with `if (!session) return <div>Loading...</div>;` or `if (status === 'unauthenticated') return <Link href="/login">Log in</Link>;` or `if (loading) return <div>Loading...</div>;`, and NEVER redirect unauthenticated users away in `useEffect` (`if (!user) router.push('/login')` is STRICTLY FORBIDDEN)!
   - Users inspect and test all pages in Live Preview mode before logging in. Every page (`/dashboard`, `/courses`, `/mentorship`, `/employer`, `/pricing`, `/subscription`, etc.) MUST immediately render its complete, interactive UI with rich domain data even when `session` or `user` is null!
2. MANDATORY RICH INITIAL MOCK STATE (`INITIAL_MOCK_DATA`) IN PAGES AND HOOKS:
   - NEVER initialize domain lists, metrics, or cards with an empty array `useState([])` or `useState(null)` in either pages (`page.tsx`) or custom hooks (`src/hooks/use*.ts`)!
   - Always define a module-level constant (e.g. `const INITIAL_MOCK_ITEMS = [...]`) containing 4 to 6 realistic, domain-specific items (with realistic titles, descriptions, status badges, dates, and metrics tailored to the project's business domain) and initialize state with it: `const [items, setItems] = useState(INITIAL_MOCK_ITEMS);`.
3. UNCONDITIONAL FETCH WITH NON-EMPTY GUARD:
   - Inside `useEffect`, do NOT wrap `fetch('/api/...')` inside `if (session)`! Fetch unconditionally on mount with an empty dependency array `[]` (NEVER pass a non-memoized function or object into the `useEffect` dependency array, which causes an infinite re-render loop!), and only overwrite the initial mock state if `res.ok` and the API returns a non-empty array or valid object:
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
4. INTERACTIVE OPTIMISTIC CRUD WORKFLOWS & BOOLEAN MODAL TOGGLES:
   - When the user clicks "New / Add / Create / Request", toggle a dedicated boolean state (`const [isFormOpen, setIsFormOpen] = useState(false);`) to open the modal/form (NEVER guard form visibility on non-empty input fields like `{newRequest.mentorId && (...)}` which prevents the form from ever opening!).
   - On form submit, perform an optimistic state update (`setItems((prev) => [newItem, ...prev])`) alongside the `fetch` call so the UI responds instantaneously in Live Preview!
5. DIRECT HOOK IMPORTS:
   - Import custom hooks directly from their module file (e.g. `import { useCourse } from '@/hooks/useCourse';`), NOT from a barrel `'@/hooks'`."#;

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

pub fn resolve_database_archetype_prompt(project_name: &str, project_description: &str, category: &str) -> String {
    let combined = format!("{} {} {}", project_name, project_description, category).to_lowercase();

    let (archetype_name, modeling_priority, relational_guidance, architectural_hints) = if combined.contains("crypto")
        || combined.contains("blockchain")
        || combined.contains("cyber")
        || combined.contains("security")
        || combined.contains("trading")
        || combined.contains("terminal")
        || combined.contains("wallet")
    {
        (
            "FinTech / Trading & Ledger Domain",
            "Derive core account, holding/balance, transaction, and audit-log entities strictly from the project's specification and contract.",
            "Link sub-accounts and ledger transactions to their owning account/identity via explicit foreign keys, using named `@relation` tags whenever a single child references multiple parent roles (e.g. source vs. destination).",
            "Use high-precision numeric types (`Float` or `Decimal @db.Decimal(18, 4)`), immutable timestamps (`createdAt`, `updatedAt`), and status/type enums matching the workflow."
        )
    } else if combined.contains("ecommerce")
        || combined.contains("e-commerce")
        || combined.contains("marketplace")
        || combined.contains("shop")
        || combined.contains("store")
        || combined.contains("booking")
        || combined.contains("hotel")
        || combined.contains("restaurant")
    {
        (
            "E-Commerce & Digital Marketplace Domain",
            "Derive catalog item, category, order/booking, line-item, and review entities strictly from the project's specification and contract.",
            "Normalize 1-to-many ownership from parent collections to line items/bookings using explicit scalar foreign keys on child records and matching opposite arrays on parent records.",
            "Include pricing, currency, stock/capacity scalars, and clean status/payment enums instead of hardcoding unrelated SaaS tables."
        )
    } else if combined.contains("health")
        || combined.contains("medical")
        || combined.contains("clinic")
        || combined.contains("patient")
        || combined.contains("wellness")
        || combined.contains("edtech")
        || combined.contains("education")
        || combined.contains("learning")
        || combined.contains("course")
        || combined.contains("school")
    {
        (
            "EdTech / HealthTech Care & Progression Domain",
            "Derive domain curriculum/care program, unit/session, participant/enrollment, and progression/record entities strictly from the project's specification and contract.",
            "Connect hierarchical structural levels downward via 1-to-many foreign keys and attach participant tracking records via composite or unique relations where needed.",
            "Include progress metrics, timestamps, optional role/status enums, and nullable personal credentials so local auth/seeding works reliably."
        )
    } else {
        (
            "Domain Operations & Workspace Platform",
            "Model ONLY the exact domain entities required by the project description and REQUIRED DATABASE MODELS CONTRACT (never inject generic placeholder entities that do not belong to the domain).",
            "Represent every relationship strictly with symmetric opposite fields and explicit foreign keys on the child side; mark 1-to-1 foreign keys `@unique`.",
            "Define role, status, and priority enums at the top of the schema when fixed choices apply, and include `createdAt` / `updatedAt` timestamps on persisted entities."
        )
    };

    format!(
        "APPLICATION DATABASE DOMAIN ARCHETYPE ({}):\n\
         - Domain Modeling Priority: {}\n\
         - Relational Topography Guidance: {}\n\
         - Production Architectural Hints: {}\n\n",
        archetype_name, modeling_priority, relational_guidance, architectural_hints
    )
}

pub fn get_project_database_archetype_from_dir(proj_dir: &std::path::Path) -> String {
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

    resolve_database_archetype_prompt(&proj_name, &proj_desc, &category)
}

pub fn resolve_api_domain_archetype(category: &str) -> &'static str {
    let cat_lower = category.to_lowercase();
    if cat_lower.contains("fintech") || cat_lower.contains("trading") || cat_lower.contains("crypto") || cat_lower.contains("banking") || cat_lower.contains("finance") {
        "API DOMAIN ARCHETYPE: FinTech, Ledger & Transaction Management\n\
         - Target ONLY the financial/ledger models declared in ACTIVE DATABASE MODELS.\n\
         - Handlers deal with balances, currency amounts, status transitions, and audit trails.\n\
         - In GET handlers, return realistic domain records with formatted amounts and ISO timestamps.\n\
         - In POST mutations, validate positive numeric amounts and persist mutations safely."
    } else if cat_lower.contains("ecommerce") || cat_lower.contains("marketplace") || cat_lower.contains("store") || cat_lower.contains("retail") || cat_lower.contains("shop") {
        "API DOMAIN ARCHETYPE: E-Commerce, Product Catalog & Order Management\n\
         - Target ONLY the catalog, order, and customer models declared in ACTIVE DATABASE MODELS.\n\
         - Handlers deal with catalog attributes, prices, stock/availability, and fulfillment state.\n\
         - In GET handlers, return rich catalog/order records matching schema fields.\n\
         - In POST mutations, validate payloads with Zod and link created records cleanly."
    } else if cat_lower.contains("edtech") || cat_lower.contains("education") || cat_lower.contains("course") || cat_lower.contains("health") || cat_lower.contains("care") || cat_lower.contains("learning") {
        "API DOMAIN ARCHETYPE: EdTech, HealthCare & Progression Tracking\n\
         - Target ONLY the domain models declared in ACTIVE DATABASE MODELS.\n\
         - Handlers deal with participant registrations, progression metrics (0-100), schedules, and structured content.\n\
         - In GET handlers, return entities with related counts/metadata matching exact schema relation names.\n\
         - In POST mutations, record timestamps and update status/progression cleanly."
    } else {
        "API DOMAIN ARCHETYPE: Domain Operations, Resource & Collaborative Workflows\n\
         - Target ONLY the domain models and field names declared in ACTIVE DATABASE MODELS (never invent missing model delegates).\n\
         - Handlers deal with status transitions, priorities, assignments, and domain metrics.\n\
         - In GET handlers, return domain entities with exact relation inclusions and fallback sample data.\n\
         - In POST mutations, validate inputs with Zod using sensible `.optional().default(...)` fallbacks."
    }
}

pub const API_ROUTE_CHAIN_OF_THOUGHT: &str = r#"BACKEND API GENERATION CHAIN-OF-THOUGHT (Execute in strict sequence):
Step 1 [Imports & Core Bindings]:
       - Import { NextResponse, NextRequest } from 'next/server'.
       - Import { prisma } from '@/lib/prisma'.
       - Import Prisma Enum types/values actually used by the model from '@prisma/client' (when schema defines enums).
       - If validating request bodies, ALWAYS import { z } from 'zod'.
Step 2 [Schema Validation & Safe Defaults]:
       - Define Zod schema matching the target model's scalar fields (use ONLY fields present in ACTIVE DATABASE MODELS).
       - For Prisma enum fields, validate with `z.nativeEnum(EnumName)` (NEVER bare `z.string()`, which causes TS2322 against Prisma enum types).
       - For Prisma `DateTime` fields, use `z.coerce.date().optional().default(() => new Date())` so JSON ISO strings and missing dates both coerce cleanly.
       - Make optional/secondary fields `.optional().default(...)` so quick-create UI requests succeed without unnecessary 400 errors.
Step 3 [Typed Relational Queries]:
       - Target ONLY models declared in ACTIVE DATABASE MODELS.
       - Under tsconfig `"strict": false`, `z.infer<typeof schema>` produces optional properties (`{ field?: T }`), so passing `validatedBody` directly into `prisma.<model>.create({ data: validatedBody })` causes `TS2322` errors.
       - ALWAYS construct an EXPLICIT typed `data: { requiredField: parsed.requiredField, ... }` object in `.create(...)` / `.update(...)` (explicitly checking required inputs or providing safe defaults / `new Date(...)` / `parsed.enumField as EnumName`).
       - In GET handlers, call `prisma.<model>.findMany({ include: { ... } })` matching the exact relation names from the contract.
Step 4 [Local Resilience & Status Gate]:
       - Always wrap Prisma queries in try/catch blocks.
       - In GET catch (or if findMany returns [] in local preview), return a fallback array of 4-6 realistic domain mockup items with status 200 so the UI immediately renders with data.
       - In POST/PUT catch, return a 201/200 fallback response with { id: 'mock-' + Date.now(), ...body } so UI creation flows always succeed in preview mode."#;

pub const HOOK_CHAIN_OF_THOUGHT: &str = r#"CUSTOM REACT HOOK GENERATION CHAIN-OF-THOUGHT (Execute in strict sequence):
Step 1 [Signature & Parameter Safety]:
       - Accept an optional options object parameter with a safe default: `({ ...options }: HookOptions = {} as any)`.
       - NEVER require mandatory destructured parameters without defaults, to prevent fatal runtime TypeError crashes.
Step 2 [State Management]:
       - Declare state for data, loading, and error: `const [data, setData] = useState<T>(fallbackData);`, `const [isLoading, setIsLoading] = useState<boolean>(false);`, `const [error, setError] = useState<string | null>(null);`.
Step 3 [Data Fetching & Resilience]:
       - Fetch data from verified API endpoints using async/await inside useCallback or useEffect.
       - Wrap fetch in try/catch/finally: in finally, ALWAYS set `setIsLoading(false);`.
       - In catch, log error and maintain realistic fallback mock data so preview UI is never broken or trapped in endless loading.
Step 4 [Dual Export Compatibility]:
       - Export the hook as BOTH named export (`export { useHookName }`) and default export (`export default useHookName`).
       - Do NOT render JSX markup or HTML tags inside hook files."#;

pub const UI_GENERATION_CHAIN_OF_THOUGHT: &str = r#"FRONTEND UI GENERATION CHAIN-OF-THOUGHT (Execute in strict sequence):
Step 1 [Client Boundary & Imports]:
       - Place 'use client'; at line 1 if the component uses hooks or interactivity.
       - Import icons ONLY from subpaths ('react-icons/hi', 'react-icons/fa', 'lucide-react'), NEVER from bare 'react-icons'.
       - Import verified hooks from '@/hooks' and UI primitives from '@/components/ui/*'.
Step 2 [Domain Mock State]:
       - Pre-populate state with 4-6 realistic domain mockup items matching the project archetype so the page renders rich visual content immediately.
Step 3 [Visual Hierarchy & Layout]:
       - Header: Prominent page title, descriptive subtitle, and primary CTA action button.
       - Metric Cards: Responsive grid (grid-cols-1 sm:grid-cols-2 lg:grid-cols-4) with KPI numbers, labels, trend badges.
       - Work Surface: Main interactive area with search input, status filter tabs, and responsive data cards or table.
Step 4 [Navigation & Dual Export]:
       - Every <Link> MUST point to a verified route from VERIFIED APPLICATION ROUTES.
       - Export component as BOTH named (`export { ComponentName }`) and default (`export default ComponentName`)."#;

pub const UI_ROUTE_PAGE_CHAIN_OF_THOUGHT: &str = r#"FRONTEND UI GENERATION CHAIN-OF-THOUGHT (Execute in strict sequence):
Step 1 [Client Boundary & Imports]:
       - Place 'use client'; at line 1 if the page/layout uses hooks or interactivity.
       - Import icons ONLY from subpaths ('react-icons/hi', 'react-icons/fa', 'lucide-react'), NEVER from bare 'react-icons'.
       - Import verified hooks from '@/hooks' and UI primitives from '@/components/ui/*'.
Step 2 [Domain Mock State]:
       - Pre-populate state with 4-6 realistic domain mockup items matching the project archetype so the page renders rich visual content immediately.
Step 3 [Visual Hierarchy & Layout]:
       - Header: Prominent page title, descriptive subtitle, and primary CTA action button.
       - Metric Cards: Responsive grid (grid-cols-1 sm:grid-cols-2 lg:grid-cols-4) with KPI numbers, labels, trend badges.
       - Work Surface: Main interactive area with search input, status filter tabs, and responsive data cards or table.
Step 4 [Navigation & Default Export Only]:
       - Every <Link> MUST point to a verified route from VERIFIED APPLICATION ROUTES.
       - Next.js App Router route files (`page.tsx`, `layout.tsx`, `loading.tsx`, `error.tsx`, `not-found.tsx`, `template.tsx`, `default.tsx`, `global-error.tsx`) MUST use ONLY `export default ComponentName` (and reserved Next.js exports such as `metadata` / `dynamic`); NEVER emit named component, function, or const exports."#;

pub fn get_scoped_ui_rules(file_path: &str) -> String {
    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let mut ui_rules = Vec::new();

    ui_rules.push(UI_DESIGN_FOUNDATION_RULEPACK);
    ui_rules.push(METACOGNITIVE_UI_STATE_RULEPACK);

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
        || path_lower.contains("hooks/")
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
            rules.push(METACOGNITIVE_UI_STATE_RULEPACK);

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
    let is_nextjs_signal = (path_lower.starts_with("src/") && (path_lower.ends_with(".ts") || path_lower.ends_with(".tsx")))
        || path_lower.starts_with("app/")
        || path_lower.contains("/app/")
        || content_lower.contains("from 'next/")
        || content_lower.contains("from \"next/")
        || content_lower.contains("@prisma/client")
        || content_lower.contains("@/lib/prisma")
        || content_lower.contains("'use client'")
        || content_lower.contains("\"use client\"")
        || content_lower.contains("export interface ");

    let has_express_or_mongoose = content_lower.contains("express()")
        || content_lower.contains("from 'express'")
        || content_lower.contains("from \"express\"")
        || content_lower.contains("require('express')")
        || content_lower.contains("require(\"express\")")
        || content_lower.contains("from 'mongoose'")
        || content_lower.contains("require('mongoose')")
        || content_lower.contains("mongoose.schema")
        || content_lower.contains("from 'react-router-dom'")
        || content_lower.contains("from \"react-router-dom\"")
        || err_lower.contains("cannot set headers");

    let is_app_route_file = crate::sanitizer::is_app_router_route_file(file_path);
    let is_mern = has_express_or_mongoose
        || (!is_nextjs_signal
            && !is_app_route_file
            && (path_lower.ends_with("server.js")
                || ((path_lower.contains("/routes/") || path_lower.contains("/models/"))
                    && (path_lower.ends_with(".js") || path_lower.ends_with(".jsx")))));

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
    let config_basename = path_lower.rsplit('/').next().unwrap_or(path_lower.as_str());
    let is_config = path_lower.ends_with(".json")
        || matches!(
            config_basename,
            "tsconfig.json"
                | "next.config.js"
                | "next.config.mjs"
                | "next.config.ts"
                | "tailwind.config.js"
                | "tailwind.config.ts"
                | "postcss.config.js"
                | "postcss.config.mjs"
                | "package.json"
        )
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

/// Extracts domain contracts (models, fields, relations, enums) from `schema.prisma` content.
pub fn extract_prisma_schema_contract(content: &str) -> Value {
    let mut models = serde_json::Map::new();
    let mut enums = serde_json::Map::new();

    // 1. Extract domain enums
    if let Ok(enum_re) = regex::Regex::new(r"(?ms)^enum\s+(\w+)\s*\{([^}]+)\}") {
        for cap in enum_re.captures_iter(content) {
            if let (Some(name_match), Some(body_match)) = (cap.get(1), cap.get(2)) {
                let enum_name = name_match.as_str().trim();
                let values: Vec<String> = body_match
                    .as_str()
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with("//"))
                    .map(|s| s.to_string())
                    .collect();
                if !values.is_empty() {
                    enums.insert(enum_name.to_string(), serde_json::json!(values));
                }
            }
        }
    }

    // 2. Extract model names first to distinguish relations from scalar columns
    let mut model_names = Vec::new();
    if let Ok(model_re) = regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{([^}]+)\}") {
        for cap in model_re.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                model_names.push(name_match.as_str().trim().to_string());
            }
        }
    }

    // 3. Extract models with scalar fields and relation fields
    if let Ok(model_re) = regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{([^}]+)\}") {
        for cap in model_re.captures_iter(content) {
            if let (Some(name_match), Some(body_match)) = (cap.get(1), cap.get(2)) {
                let model_name = name_match.as_str().trim().to_string();
                let mut scalar_fields = Vec::new();
                let mut typed_fields = Vec::new();
                let mut required_fields = Vec::new();
                let mut relation_fields = Vec::new();

                for line in body_match.as_str().lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("@@") {
                        continue;
                    }
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let field_name = parts[0];
                        let field_type = parts[1];
                        let clean_type = field_type.replace(['?', '[', ']'], "");

                        if model_names.contains(&clean_type) || trimmed.contains("@relation") {
                            relation_fields.push(format!("{}: {}", field_name, field_type));
                        } else {
                            scalar_fields.push(field_name.to_string());
                            typed_fields.push(format!("{}: {}", field_name, field_type));
                            let is_optional_or_list = field_type.ends_with('?') || field_type.ends_with("[]");
                            let has_auto_default = trimmed.contains("@default(") || trimmed.contains("@updatedAt");
                            if !is_optional_or_list && !has_auto_default {
                                required_fields.push(field_name.to_string());
                            }
                        }
                    }
                }

                let delegate = model_to_prisma_delegate(&model_name);
                models.insert(model_name, serde_json::json!({
                    "delegate": delegate,
                    "fields": scalar_fields,
                    "typedFields": typed_fields,
                    "requiredFields": required_fields,
                    "relations": relation_fields
                }));
            }
        }
    }

    serde_json::json!({
        "models": models,
        "enums": enums
    })
}

/// Extracts API endpoint path and exported HTTP methods from route handler content.
pub fn extract_api_route_contract(file_path: &str, content: &str) -> Option<(String, Vec<String>)> {
    let norm_path = file_path.replace('\\', "/");
    let api_idx = norm_path.find("/api/")?;
    let without_file = norm_path[api_idx..].trim_end_matches("/route.ts").trim_end_matches("/route.js");
    let endpoint = if without_file.is_empty() { "/api".to_string() } else { without_file.to_string() };

    let mut methods = Vec::new();
    let method_candidates = ["GET", "POST", "PUT", "DELETE", "PATCH"];
    for method in &method_candidates {
        let pattern_func = format!(r"\bexport\s+(?:async\s+)?function\s+{}\b", method);
        let pattern_const = format!(r"\bexport\s+const\s+{}\b", method);
        let pattern_export = format!(r"\bhandler\s+as\s+{}\b", method);

        let matches = regex::Regex::new(&pattern_func).map(|r| r.is_match(content)).unwrap_or(false)
            || regex::Regex::new(&pattern_const).map(|r| r.is_match(content)).unwrap_or(false)
            || regex::Regex::new(&pattern_export).map(|r| r.is_match(content)).unwrap_or(false);

        if matches {
            methods.push(method.to_string());
        }
    }

    if methods.is_empty() {
        methods.push("GET".to_string());
    }

    Some((endpoint, methods))
}

/// Extracts custom hook names from a hook file.
pub fn extract_hook_contract(file_path: &str, content: &str) -> Vec<String> {
    let norm_path = file_path.replace('\\', "/");
    let mut hooks = Vec::new();

    if let Ok(hook_re) = regex::Regex::new(r"\bexport\s+(?:(?:async\s+)?function|const)\s+(use[A-Za-z0-9_]+)\b") {
        for cap in hook_re.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let name = m.as_str().to_string();
                if !hooks.contains(&name) {
                    hooks.push(name);
                }
            }
        }
    }

    if hooks.is_empty() {
        let path = std::path::Path::new(&norm_path);
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.starts_with("use") && stem.len() > 3 {
                hooks.push(stem.to_string());
            }
        }
    }

    hooks
}

/// Harvests metadata from newly generated files into `.axiom/live_builder_guide.json`.
pub fn harvest_file_contract_to_lbg(proj_dir: &std::path::Path, file_path: &str, content: &str) {
    let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
    let mut lbg: Value = if lbg_path.exists() {
        if let Ok(c) = std::fs::read_to_string(&lbg_path) {
            serde_json::from_str(&c).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        }
    } else {
        serde_json::json!({})
    };

    if !lbg.is_object() {
        lbg = serde_json::json!({});
    }

    if lbg.get("sharedContracts").is_none() {
        lbg["sharedContracts"] = serde_json::json!({
            "database": { "models": {}, "enums": {} },
            "api": { "endpoints": {} },
            "hooks": {}
        });
    }

    let path_lower = file_path.to_lowercase().replace('\\', "/");
    let is_app_route = crate::sanitizer::is_app_router_route_file(file_path);

    // 1. Prisma schema
    if path_lower.ends_with("schema.prisma") {
        let db_contract = extract_prisma_schema_contract(content);
        if let Some(shared) = lbg.get_mut("sharedContracts") {
            shared["database"] = db_contract;
        }
    }
    // 2. API Route
    else if (path_lower.contains("/api/") || path_lower.starts_with("api/")) && (path_lower.ends_with("route.ts") || path_lower.ends_with("route.js")) {
        if let Some((endpoint, methods)) = extract_api_route_contract(file_path, content) {
            if let Some(shared) = lbg.get_mut("sharedContracts") {
                if let Some(api) = shared.get_mut("api") {
                    if let Some(endpoints) = api.get_mut("endpoints") {
                        if let Some(map) = endpoints.as_object_mut() {
                            map.insert(endpoint, serde_json::json!(methods));
                        }
                    } else {
                        api["endpoints"] = serde_json::json!({ endpoint: methods });
                    }
                }
            }
        }
    }
    // 3. Custom Hooks
    else if !is_app_route && (path_lower.contains("/hooks/") || path_lower.starts_with("hooks/")) && !path_lower.contains("/api/") && (path_lower.ends_with(".ts") || path_lower.ends_with(".tsx")) && !path_lower.ends_with("index.ts") {
        let hooks = extract_hook_contract(file_path, content);
        if let Some(shared) = lbg.get_mut("sharedContracts") {
            if let Some(hooks_obj) = shared.get_mut("hooks") {
                if let Some(map) = hooks_obj.as_object_mut() {
                    for h in hooks {
                        map.insert(h.clone(), serde_json::json!(format!("@/hooks/{}", h)));
                    }
                }
            }
        }
    }

    if let Ok(pretty) = serde_json::to_string_pretty(&lbg) {
        if let Some(parent) = lbg_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&lbg_path, pretty);
    }
}

/// Harvests and maintains the unified `.axiom/axiom-contracts.json` and `axiom-contracts.json` in real time.
/// Dynamically extracts:
/// 1. Internal dependencies (imported project files with @/ or relative paths) and external npm packages.
/// 2. Role (ui-component, page-route, client-hook, api-route, database-schema, layout, config).
/// 3. Declared capabilities (exported components/functions/hooks/types, Prisma models, API methods).
/// 4. Declared UI states (useState hooks with inferred/declared types and default initializers).
/// 5. Consumed capabilities (API routes fetched, custom hooks called, Prisma models queried).
/// 6. Connected UI routes (<Link href="..."> or router.push(...)).
fn extract_use_state_contracts(content: &str) -> serde_json::Map<String, Value> {
    let mut state_contracts = serde_json::Map::new();
    let Ok(header_re) = regex::Regex::new(
        r#"const\s*\[\s*([a-zA-Z0-9_]+)\s*,\s*([a-zA-Z0-9_]+)\s*\]\s*=\s*useState(?:<([\s\S]*?)>)?\s*\("#,
    ) else {
        return state_contracts;
    };

    let bytes = content.as_bytes();
    for cap in header_re.captures_iter(content) {
        let Some(full_m) = cap.get(0) else { continue };
        let (Some(state_name), Some(setter_name)) = (cap.get(1), cap.get(2)) else { continue };
        let open_paren_end = full_m.end();

        // Walk balanced parentheses from the opening `(` of useState(...)
        let mut depth: i32 = 1;
        let mut i = open_paren_end;
        let mut in_single = false;
        let mut in_double = false;
        let mut in_tick = false;
        let mut close_idx = open_paren_end;
        while i < bytes.len() && depth > 0 {
            let ch = bytes[i] as char;
            let prev = if i > 0 { bytes[i - 1] as char } else { '\0' };
            if prev != '\\' {
                if ch == '\'' && !in_double && !in_tick {
                    in_single = !in_single;
                } else if ch == '"' && !in_single && !in_tick {
                    in_double = !in_double;
                } else if ch == '`' && !in_single && !in_double {
                    in_tick = !in_tick;
                }
            }
            if !in_single && !in_double && !in_tick {
                if ch == '(' {
                    depth += 1;
                } else if ch == ')' {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = i;
                        break;
                    }
                }
            }
            i += 1;
        }

        let default_raw = if close_idx >= open_paren_end && close_idx <= content.len() {
            content[open_paren_end..close_idx].trim()
        } else {
            ""
        };
        let default_val = if default_raw.chars().count() > 160 {
            format!("{}...", default_raw.chars().take(157).collect::<String>())
        } else {
            default_raw.to_string()
        };

        let type_param = cap
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());

        let inferred_type = if let Some(tp) = type_param {
            tp
        } else if default_raw == "false" || default_raw == "true" {
            "boolean".to_string()
        } else if default_raw.starts_with('[') || default_raw.ends_with(']') {
            "array".to_string()
        } else if default_raw.starts_with('{') && default_raw.ends_with('}') {
            "object".to_string()
        } else if default_raw.starts_with('"') || default_raw.starts_with('\'') || default_raw.starts_with('`') {
            "string".to_string()
        } else if default_raw.parse::<f64>().is_ok() {
            "number".to_string()
        } else if default_raw == "null" {
            "nullable-object".to_string()
        } else {
            "any".to_string()
        };

        state_contracts.insert(
            state_name.as_str().to_string(),
            serde_json::json!({
                "setter": setter_name.as_str(),
                "type": inferred_type,
                "defaultValue": default_val
            }),
        );
    }

    state_contracts
}

fn ensure_axiom_contracts_root(contracts_root: &mut Value, project_id: &str) {
    if !contracts_root.is_object() {
        *contracts_root = serde_json::json!({});
    }
    if let Some(root_obj) = contracts_root.as_object_mut() {
        root_obj
            .entry("version".to_string())
            .or_insert_with(|| serde_json::json!("1.0"));
        root_obj
            .entry("projectId".to_string())
            .or_insert_with(|| serde_json::json!(project_id));
        if !root_obj.get("files").map_or(false, |f| f.is_array()) {
            root_obj.insert("files".to_string(), serde_json::json!([]));
        }
    }
}

fn upsert_file_contract_into_root(
    contracts_root: &mut Value,
    project_id: &str,
    file_path: &str,
    content: &str,
    files_list: &[Value],
) {
    ensure_axiom_contracts_root(contracts_root, project_id);

    let norm_path = file_path.replace('\\', "/");
    let path_lower = norm_path.to_lowercase();
    let is_app_route = crate::sanitizer::is_app_router_route_file(&norm_path);

    // Determine role
    let role = if path_lower.ends_with("schema.prisma") {
        "database-schema"
    } else if (path_lower.contains("/api/") || path_lower.starts_with("api/"))
        && (path_lower.ends_with("route.ts") || path_lower.ends_with("route.js"))
    {
        "api-route"
    } else if !is_app_route
        && !path_lower.contains("/components/")
        && !path_lower.starts_with("components/")
        && (path_lower.contains("/hooks/") || path_lower.starts_with("hooks/") || path_lower.ends_with("hook.ts") || path_lower.ends_with("hook.tsx") || path_lower.ends_with("hooks.ts") || path_lower.ends_with("hooks.tsx"))
    {
        "client-hook"
    } else if path_lower.ends_with("layout.tsx") || path_lower.ends_with("layout.jsx") {
        "layout"
    } else if is_app_route || path_lower.ends_with("page.tsx") || path_lower.ends_with("page.jsx") || path_lower.ends_with("page.js") {
        "page-route"
    } else if path_lower.contains("/components/") || path_lower.starts_with("components/") {
        "ui-component"
    } else if path_lower.contains("/models/") || path_lower.starts_with("models/") || path_lower.contains("/types/") || path_lower.starts_with("types/") {
        "domain-model"
    } else if path_lower.ends_with(".css") || path_lower.ends_with(".scss") {
        "stylesheet"
    } else if path_lower.ends_with(".config.js")
        || path_lower.ends_with(".config.mjs")
        || path_lower.ends_with(".config.ts")
        || path_lower == "tsconfig.json"
        || path_lower == "package.json"
    {
        "configuration"
    } else {
        "source-module"
    };

    // Extract dependencies: internal vs external
    let mut internal_deps = Vec::new();
    let mut external_deps = Vec::new();

    let node_builtins = [
        "fs", "path", "http", "https", "os", "crypto", "stream", "util", "events", "assert",
        "dns", "net", "tls", "querystring", "url", "zlib", "buffer", "child_process",
    ];

    if path_lower == "package.json" {
        if let Ok(pkg_val) = serde_json::from_str::<Value>(content) {
            for sec in ["dependencies", "devDependencies"] {
                if let Some(deps_obj) = pkg_val.get(sec).and_then(|d| d.as_object()) {
                    for dep_name in deps_obj.keys() {
                        if !crate::packages::is_local_import_specifier(dep_name) && !external_deps.contains(dep_name) {
                            external_deps.push(dep_name.clone());
                        }
                    }
                }
            }
        }
    } else if let Ok(import_re) = regex::Regex::new(
        r#"(?:import\s+(?:[^"';]+\s+from\s+)?|require\s*\(\s*|require\.resolve\s*\(\s*)['"]([^'"]+)['"]"#,
    ) {
        for cap in import_re.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let specifier = m.as_str().trim();
                if crate::packages::is_local_import_specifier(specifier) {
                    let resolved = if let Some(stripped) = specifier.strip_prefix("@/") {
                        format!("src/{}", stripped)
                    } else if let Some(stripped) = specifier.strip_prefix("~/") {
                        format!("src/{}", stripped)
                    } else if specifier.starts_with('.') {
                        if let Some(parent) = std::path::Path::new(&norm_path).parent() {
                            let res = parent.join(specifier);
                            let mut normalized_parts = Vec::new();
                            for comp in res.components() {
                                match comp {
                                    std::path::Component::Normal(c) => {
                                        if let Some(cs) = c.to_str() {
                                            normalized_parts.push(cs);
                                        }
                                    }
                                    std::path::Component::ParentDir => {
                                        normalized_parts.pop();
                                    }
                                    _ => {}
                                }
                            }
                            normalized_parts.join("/")
                        } else {
                            specifier.to_string()
                        }
                    } else {
                        specifier.trim_start_matches('/').to_string()
                    };

                    let mut matched_target = resolved.clone();
                    for f in files_list {
                        if let Some(p) = f.get("path").and_then(|p| p.as_str()) {
                            let p_norm = p.replace('\\', "/");
                            let p_no_ext = std::path::Path::new(&p_norm).with_extension("");
                            let p_no_ext_str = p_no_ext.to_str().unwrap_or("");
                            if p_norm == resolved
                                || p_no_ext_str == resolved
                                || p_norm == format!("{}.tsx", resolved)
                                || p_norm == format!("{}.ts", resolved)
                                || p_norm == format!("src/{}.tsx", resolved)
                                || p_norm == format!("src/{}.ts", resolved)
                            {
                                matched_target = p_norm;
                                break;
                            }
                        }
                    }
                    if !internal_deps.contains(&matched_target) {
                        internal_deps.push(matched_target);
                    }
                } else {
                    let clean_spec = specifier.strip_prefix("node:").unwrap_or(specifier);
                    let pkg_name = clean_spec.split('/').next().unwrap_or(clean_spec).to_string();
                    let full_pkg = if clean_spec.starts_with('@') {
                        let parts: Vec<&str> = clean_spec.split('/').collect();
                        if parts.len() > 1 {
                            format!("{}/{}", parts[0], parts[1])
                        } else {
                            pkg_name
                        }
                    } else {
                        pkg_name
                    };
                    if !node_builtins.contains(&full_pkg.as_str())
                        && !specifier.starts_with("node:")
                        && !external_deps.contains(&full_pkg)
                    {
                        external_deps.push(full_pkg);
                    }
                }
            }
        }
    }

    // Extract client/UI states (useState hooks) using balanced parentheses scanner
    let state_contracts = if role == "ui-component" || role == "page-route" || role == "layout" || role == "client-hook" {
        extract_use_state_contracts(content)
    } else {
        serde_json::Map::new()
    };

    // Extract called hooks (useXxx(...))
    let mut hook_calls = Vec::new();
    if let Ok(hook_call_re) = regex::Regex::new(r"\b(use[A-Z][a-zA-Z0-9_]*)\s*(?:<[^>]+>)?\s*\(") {
        for cap in hook_call_re.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let h = m.as_str().to_string();
                if !hook_calls.contains(&h) {
                    hook_calls.push(h);
                }
            }
        }
    }

    // Extract API endpoints (for api-route)
    let mut endpoints = Vec::new();
    if role == "api-route" {
        if let Some((_, detected_methods)) = extract_api_route_contract(&norm_path, content) {
            endpoints = detected_methods;
        }
    }

    // Extract Prisma models
    let mut prisma_models = Vec::new();
    let mut prisma_enums = Vec::new();
    if role == "database-schema" {
        if let Ok(model_re) = regex::Regex::new(r"(?m)^model\s+([a-zA-Z0-9_]+)\s*\{") {
            for cap in model_re.captures_iter(content) {
                if let Some(m) = cap.get(1) {
                    prisma_models.push(m.as_str().to_string());
                }
            }
        }
        if let Ok(enum_re) = regex::Regex::new(r"(?m)^enum\s+([a-zA-Z0-9_]+)\s*\{") {
            for cap in enum_re.captures_iter(content) {
                if let Some(e) = cap.get(1) {
                    prisma_enums.push(e.as_str().to_string());
                }
            }
        }
    }

    // Extract exported symbols
    let mut exports = Vec::new();
    if let Ok(export_re) = regex::Regex::new(
        r#"(?:export\s+(?:default\s+)?(?:async\s+)?(?:function|const|class|type|interface)\s+([a-zA-Z0-9_]+)|export\s*\{\s*([^}]+)\s*\}|export\s+default\s+([A-Za-z0-9_]+)\s*;)"#,
    ) {
        for cap in export_re.captures_iter(content) {
            if let Some(name) = cap.get(1).or_else(|| cap.get(3)) {
                let n = name.as_str().trim();
                if !n.is_empty() && n != "default" && n != "function" && n != "class" && !exports.contains(&n.to_string()) {
                    exports.push(n.to_string());
                }
            } else if let Some(list) = cap.get(2) {
                for item in list.as_str().split(',') {
                    let cleaned = item.trim().split_whitespace().next().unwrap_or("").trim();
                    if !cleaned.is_empty() && !exports.contains(&cleaned.to_string()) {
                        exports.push(cleaned.to_string());
                    }
                }
            }
        }
    }

    // Extract consumed API routes via fetch
    let mut api_calls = Vec::new();
    if let Ok(fetch_re) = regex::Regex::new(r#"fetch\s*\(\s*['"`](/api/[^'"`]+)['"`]"#) {
        for cap in fetch_re.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let url = m.as_str().to_string();
                if !api_calls.contains(&url) {
                    api_calls.push(url);
                }
            }
        }
    }

    // Extract connected routes via <Link href="..."> or router.push(...)
    let mut connected_routes = Vec::new();
    if let Ok(link_re) = regex::Regex::new(r#"href\s*=\s*['"](/[^'"]*)['"]"#) {
        for cap in link_re.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let r = m.as_str().to_string();
                if !connected_routes.contains(&r) && !r.starts_with("/api") && !r.starts_with("/_") {
                    connected_routes.push(r);
                }
            }
        }
    }
    if let Ok(push_re) = regex::Regex::new(r#"router\.push\s*\(\s*['"](/[^'"]*)['"]"#) {
        for cap in push_re.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                let r = m.as_str().to_string();
                if !connected_routes.contains(&r) && !r.starts_with("/api") && !r.starts_with("/_") {
                    connected_routes.push(r);
                }
            }
        }
    }

    // Extract return keys for hooks & exported interface/type properties into capabilities.members
    let mut members = Vec::new();
    if role == "client-hook" {
        if let Ok(ret_re) = regex::Regex::new(r"(?s)return\s*\{([^}]+)\}\s*;?") {
            for cap in ret_re.captures_iter(content) {
                if let Some(body) = cap.get(1) {
                    for raw_line in body.as_str().split(',') {
                        let clean = raw_line
                            .lines()
                            .map(|l| l.split("//").next().unwrap_or("").trim())
                            .collect::<Vec<&str>>()
                            .join(" ");
                        let key = clean
                            .split(':')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                        if !key.is_empty() && !members.contains(&key.to_string()) {
                            members.push(key.to_string());
                        }
                    }
                }
            }
        }
    }
    if let Ok(iface_re) = regex::Regex::new(r"(?s)export\s+(?:interface|type)\s+[A-Za-z0-9_]+\s*(?:=\s*)?\{([^}]+)\}") {
        for cap in iface_re.captures_iter(content) {
            if let Some(body) = cap.get(1) {
                for line in body.as_str().lines() {
                    let trimmed = line.split("//").next().unwrap_or("").trim();
                    if trimmed.is_empty() || trimmed.starts_with('[') || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                        continue;
                    }
                    let prop = trimmed
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_end_matches('?')
                        .trim();
                    if !prop.is_empty()
                        && prop.chars().all(|c| c.is_alphanumeric() || c == '_')
                        && !members.contains(&prop.to_string())
                    {
                        members.push(prop.to_string());
                    }
                }
            }
        }
    }

    let file_entry = serde_json::json!({
        "path": norm_path,
        "role": role,
        "dependencies": {
            "internal": internal_deps,
            "external": external_deps
        },
        "capabilities": {
            "exports": exports,
            "members": members,
            "states": state_contracts,
            "endpoints": endpoints,
            "models": prisma_models,
            "enums": prisma_enums
        },
        "consumedContracts": {
            "apiCalls": api_calls,
            "hookCalls": hook_calls
        },
        "connectedRoutes": connected_routes
    });

    if let Some(files_arr) = contracts_root.get_mut("files").and_then(|f| f.as_array_mut()) {
        if let Some(pos) = files_arr
            .iter()
            .position(|f| f.get("path").and_then(|p| p.as_str()) == Some(&norm_path))
        {
            files_arr[pos] = file_entry;
        } else {
            files_arr.push(file_entry);
        }
    }
}

pub fn harvest_file_to_axiom_contracts(
    proj_dir: &std::path::Path,
    project_id: &str,
    file_path: &str,
    content: &str,
    files_list: &[Value],
) {
    let axiom_dir = proj_dir.join(".axiom");
    let _ = std::fs::create_dir_all(&axiom_dir);
    let contracts_path = axiom_dir.join("axiom-contracts.json");

    let mut contracts_root: Value = if contracts_path.exists() {
        if let Ok(c) = std::fs::read_to_string(&contracts_path) {
            serde_json::from_str(&c).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        }
    } else {
        serde_json::json!({})
    };

    upsert_file_contract_into_root(&mut contracts_root, project_id, file_path, content, files_list);

    contracts_root["lastUpdated"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    contracts_root["totalFilesTracked"] = serde_json::json!(
        contracts_root
            .get("files")
            .and_then(|f| f.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    );

    if let Ok(pretty) = serde_json::to_string_pretty(&contracts_root) {
        let _ = std::fs::write(&contracts_path, &pretty);
        let _ = std::fs::write(proj_dir.join("axiom-contracts.json"), &pretty);
    }
}

/// Scans all generated/healed project files on disk and writes the up-to-date
/// `.axiom/axiom-contracts.json` and root `axiom-contracts.json` in a single pass.
pub fn refresh_axiom_contracts_for_project(
    proj_dir: &std::path::Path,
    project_id: &str,
    files_list: &[Value],
) {
    let axiom_dir = proj_dir.join(".axiom");
    let _ = std::fs::create_dir_all(&axiom_dir);
    let contracts_path = axiom_dir.join("axiom-contracts.json");

    let mut contracts_root = serde_json::json!({
        "version": "1.0",
        "projectId": project_id,
        "lastUpdated": chrono::Utc::now().to_rfc3339(),
        "files": []
    });

    fn collect_project_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if path.is_dir() {
                    if name != "node_modules"
                        && name != ".next"
                        && name != ".git"
                        && name != ".axiom"
                        && name != "dist"
                        && name != "build"
                        && name != "target"
                    {
                        collect_project_files(&path, out);
                    }
                } else if path.is_file() {
                    let lower = name.to_lowercase();
                    if lower.ends_with(".ts")
                        || lower.ends_with(".tsx")
                        || lower.ends_with(".js")
                        || lower.ends_with(".jsx")
                        || lower.ends_with(".css")
                        || lower.ends_with(".scss")
                        || lower.ends_with(".prisma")
                        || lower == "package.json"
                        || lower == "tsconfig.json"
                        || lower.starts_with("next.config.")
                        || lower.starts_with("tailwind.config.")
                    {
                        out.push(path);
                    }
                }
            }
        }
    }

    let mut discovered_paths = Vec::new();
    collect_project_files(proj_dir, &mut discovered_paths);
    discovered_paths.sort();

    let mut all_file_specs: Vec<Value> = files_list.to_vec();
    for p in &discovered_paths {
        if let Ok(rel) = p.strip_prefix(proj_dir) {
            let rel_norm = rel.to_string_lossy().replace('\\', "/");
            if !all_file_specs.iter().any(|v| {
                v.get("path")
                    .and_then(|x| x.as_str())
                    .map_or(false, |s| s.replace('\\', "/") == rel_norm)
            }) {
                all_file_specs.push(serde_json::json!({ "path": rel_norm }));
            }
        }
    }

    for p in &discovered_paths {
        if let Ok(rel) = p.strip_prefix(proj_dir) {
            let rel_norm = rel.to_string_lossy().replace('\\', "/");
            if let Ok(content) = std::fs::read_to_string(p) {
                upsert_file_contract_into_root(&mut contracts_root, project_id, &rel_norm, &content, &all_file_specs);
            }
        }
    }

    contracts_root["lastUpdated"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    contracts_root["totalFilesTracked"] = serde_json::json!(
        contracts_root
            .get("files")
            .and_then(|f| f.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    );

    if let Ok(pretty) = serde_json::to_string_pretty(&contracts_root) {
        let _ = std::fs::write(&contracts_path, &pretty);
        let _ = std::fs::write(proj_dir.join("axiom-contracts.json"), &pretty);
    }
}

/// Formats cross-file dependency and capability contracts from `.axiom/axiom-contracts.json`
/// into a compact, dependency-prioritized context block for subsequent LLM generations.
pub fn format_axiom_contracts_prompt(proj_dir: &std::path::Path, current_file_path: &str) -> String {
    let contracts_path = proj_dir.join(".axiom").join("axiom-contracts.json");
    if !contracts_path.exists() {
        return String::new();
    }
    let Ok(content) = std::fs::read_to_string(&contracts_path) else {
        return String::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return String::new();
    };
    let Some(files) = root.get("files").and_then(|f| f.as_array()) else {
        return String::new();
    };
    if files.is_empty() {
        return String::new();
    }

    let cur_norm = current_file_path.replace('\\', "/");
    let direct_deps: std::collections::HashSet<String> = files
        .iter()
        .find(|entry| {
            entry
                .get("path")
                .and_then(|p| p.as_str())
                .map(|p| p.replace('\\', "/").eq_ignore_ascii_case(&cur_norm))
                .unwrap_or(false)
        })
        .and_then(|entry| entry.get("dependencies"))
        .and_then(|d| d.get("internal"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.replace('\\', "/").to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    let mut ranked: Vec<((u8, String), String)> = Vec::new();
    for entry in files {
        let raw_path = entry.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let path = raw_path.replace('\\', "/");
        if path.is_empty() || path.eq_ignore_ascii_case(&cur_norm) {
            continue;
        }
        let role = entry.get("role").and_then(|r| r.as_str()).unwrap_or("source-module");
        if role == "configuration" {
            continue;
        }
        let is_app_route_role = role == "page-route" || role == "layout";
        let exports: Vec<&str> = if is_app_route_role {
            Vec::new()
        } else {
            entry
                .get("capabilities")
                .and_then(|c| c.get("exports"))
                .and_then(|e| e.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default()
        };
        let members: Vec<&str> = if is_app_route_role {
            Vec::new()
        } else {
            entry
                .get("capabilities")
                .and_then(|c| c.get("members"))
                .and_then(|e| e.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default()
        };
        let endpoints: Vec<&str> = entry
            .get("capabilities")
            .and_then(|c| c.get("endpoints"))
            .and_then(|e| e.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let models: Vec<&str> = entry
            .get("capabilities")
            .and_then(|c| c.get("models"))
            .and_then(|e| e.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let enums: Vec<&str> = entry
            .get("capabilities")
            .and_then(|c| c.get("enums"))
            .and_then(|e| e.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let api_calls: Vec<&str> = entry
            .get("consumedContracts")
            .and_then(|c| c.get("apiCalls"))
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let routes: Vec<&str> = entry
            .get("connectedRoutes")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        if exports.is_empty()
            && members.is_empty()
            && endpoints.is_empty()
            && models.is_empty()
            && enums.is_empty()
            && api_calls.is_empty()
            && routes.is_empty()
        {
            continue;
        }

        let priority: u8 = if direct_deps.contains(&path.to_lowercase()) {
            0
        } else {
            match role {
                "database-schema" | "domain-model" => 1,
                "client-hook" => 2,
                "source-module" | "layout" => 3,
                "api-route" => 4,
                "ui-component" => 5,
                "page-route" => 6,
                _ => 7,
            }
        };

        let mut summary = format!("- {} [{}]", path, role);
        if !exports.is_empty() {
            summary.push_str(&format!(" exports:[{}]", exports.join(", ")));
        }
        if !members.is_empty() {
            let cap_members: Vec<&str> = members.iter().take(12).copied().collect();
            summary.push_str(&format!(" members:[{}]", cap_members.join(", ")));
        }
        if !models.is_empty() {
            summary.push_str(&format!(" models:[{}]", models.join(", ")));
        }
        if !enums.is_empty() {
            summary.push_str(&format!(" enums:[{}]", enums.join(", ")));
        }
        if !endpoints.is_empty() {
            summary.push_str(&format!(" endpoints:[{}]", endpoints.join(", ")));
        }
        if !api_calls.is_empty() {
            summary.push_str(&format!(" calls:[{}]", api_calls.join(", ")));
        }
        if !routes.is_empty() {
            summary.push_str(&format!(" links:[{}]", routes.join(", ")));
        }
        ranked.push(((priority, path), summary));
    }

    ranked.sort_by(|a, b| a.0.cmp(&b.0));

    let mut lines: Vec<String> = Vec::new();
    let mut total_chars: usize = 0;
    for (_, summary) in ranked {
        if lines.len() >= 24 {
            break;
        }
        if !lines.is_empty() && total_chars + summary.len() + 1 > 3500 {
            break;
        }
        total_chars += summary.len() + 1;
        lines.push(summary);
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("CROSS-FILE CONTRACTS REGISTRY (.axiom/axiom-contracts.json):\n{}\n", lines.join("\n"))
    }
}

fn parse_lbg_relation_entry(raw: &str) -> (String, String, bool) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), String::new(), false);
    }
    let (rel_field, type_part) = if let Some((lhs, rhs)) = trimmed.split_once(':') {
        let f = lhs.split_whitespace().next().unwrap_or("").trim().to_string();
        let t = rhs.split_whitespace().next().unwrap_or("").trim();
        (f, t)
    } else {
        let toks: Vec<&str> = trimmed.split_whitespace().collect();
        if toks.len() >= 2 && toks[1].chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
            (toks[0].to_string(), toks[1])
        } else {
            (String::new(), toks.first().copied().unwrap_or(""))
        }
    };
    let clean_tok = type_part.split('(').next().unwrap_or(type_part).trim();
    let is_array = clean_tok.contains('[');
    let target_model = clean_tok
        .replace(['?', '[', ']', ',', ';'], "")
        .trim()
        .to_string();
    (rel_field, target_model, is_array)
}

fn is_valid_lbg_api_endpoint_key(key: &str) -> bool {
    let kt = key.trim();
    if kt.is_empty() {
        return false;
    }
    if kt.starts_with('/') || kt.contains("/api/") {
        return true;
    }
    let mut parts = kt.split_whitespace();
    if let (Some(method), Some(path), None) = (parts.next(), parts.next(), parts.next()) {
        let m_up = method.to_uppercase();
        if matches!(m_up.as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS")
            && path.starts_with('/')
        {
            return true;
        }
    }
    false
}

fn is_prisma_primitive_or_scalar_type(base_ty: &str) -> bool {
    matches!(
        base_ty,
        "String"
            | "Int"
            | "Float"
            | "Boolean"
            | "DateTime"
            | "Json"
            | "Bytes"
            | "BigInt"
            | "Decimal"
            | "string"
            | "number"
            | "boolean"
            | "Date"
            | "any"
            | "unknown"
            | "object"
    )
}

fn parse_lbg_field_name_and_type(raw: &str) -> (String, Option<(String, bool)>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }
    if let Some((lhs, rhs)) = trimmed.split_once(':') {
        let fname = lhs.split_whitespace().next().unwrap_or("").trim().to_string();
        let type_tok = rhs
            .split_whitespace()
            .next()
            .unwrap_or("")
            .split('(')
            .next()
            .unwrap_or("")
            .trim();
        let is_arr = type_tok.contains('[');
        let base_ty = type_tok.replace(['?', '[', ']', ',', ';'], "").trim().to_string();
        let ty_opt = if base_ty.is_empty() { None } else { Some((base_ty, is_arr)) };
        (fname, ty_opt)
    } else {
        let toks: Vec<&str> = trimmed.split_whitespace().collect();
        let fname = toks.first().copied().unwrap_or("").trim().to_string();
        if toks.len() >= 2 {
            let type_tok = toks[1].split('(').next().unwrap_or(toks[1]).trim();
            let is_arr = type_tok.contains('[');
            let base_ty = type_tok.replace(['?', '[', ']', ',', ';'], "").trim().to_string();
            let ty_opt = if base_ty.is_empty() { None } else { Some((base_ty, is_arr)) };
            (fname, ty_opt)
        } else {
            (fname, None)
        }
    }
}

/// Validates and cleans relational graph contracts inside `live_builder_guide.json` using
/// model-agnostic Prisma graph invariants:
/// - Prunes non-path prose headings accidentally placed in `sharedContracts.api.endpoints`,
/// - Rejects relations referencing undeclared models (`target_model` missing from `models`),
/// - Rejects unsupervised self-list relations (`M[]`) that lack a singular `M?` partner (or pair),
/// - Rejects one-sided broken relations whose target model has neither an opposite relation nor FK,
/// - Keeps `typedFields` and `fields` consistent by pruning removed relation/undeclared model refs.
/// Preserves single-model (`models.len() < 2`) partial database contracts unchanged.
pub fn sanitize_live_builder_guide_contracts(lbg: &Value) -> Value {
    let mut out = lbg.clone();

    if let Some(ep_obj) = out
        .get_mut("sharedContracts")
        .and_then(|c| c.get_mut("api"))
        .and_then(|a| a.get_mut("endpoints"))
        .and_then(|e| e.as_object_mut())
    {
        ep_obj.retain(|k, _| is_valid_lbg_api_endpoint_key(k));
    }

    let (declared_enums, has_declared_enums): (std::collections::HashSet<String>, bool) = out
        .get("sharedContracts")
        .and_then(|c| c.get("database"))
        .and_then(|d| d.get("enums"))
        .and_then(|e| e.as_object())
        .map(|obj| (obj.keys().cloned().collect(), !obj.is_empty()))
        .unwrap_or_default();

    let Some(models_obj) = out
        .get_mut("sharedContracts")
        .and_then(|c| c.get_mut("database"))
        .and_then(|d| d.get_mut("models"))
        .and_then(|m| m.as_object_mut())
    else {
        return out;
    };

    let declared_models: std::collections::HashSet<String> = models_obj.keys().cloned().collect();
    if declared_models.len() < 2 {
        return out;
    }

    let fields_map: std::collections::HashMap<String, std::collections::HashSet<String>> = models_obj
        .iter()
        .map(|(m_name, m_val)| {
            let fset: std::collections::HashSet<String> = m_val
                .get("fields")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .filter_map(|s| s.split([':', ' ', '\t']).next())
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            (m_name.clone(), fset)
        })
        .collect();

    let mut removed_rel_fields: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();
    let mut removed_rel_targets: std::collections::HashMap<String, std::collections::HashSet<String>> = std::collections::HashMap::new();

    let max_iters = declared_models.len() + 1;
    for _ in 0..max_iters {
        let mut rels_snapshot: std::collections::HashMap<String, Vec<(String, String, bool)>> = std::collections::HashMap::new();
        for (m_name, m_val) in models_obj.iter() {
            let parsed_list: Vec<(String, String, bool)> = m_val
                .get("relations")
                .and_then(|r| r.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(parse_lbg_relation_entry)
                        .collect()
                })
                .unwrap_or_default();
            rels_snapshot.insert(m_name.clone(), parsed_list);
        }

        let mut changed_any = false;
        let model_keys: Vec<String> = models_obj.keys().cloned().collect();
        for m_name in model_keys {
            let Some(m_obj) = models_obj.get_mut(&m_name).and_then(|v| v.as_object_mut()) else {
                continue;
            };
            let Some(rels_arr) = m_obj.get("relations").and_then(|r| r.as_array()).cloned() else {
                continue;
            };

            let self_rels = rels_snapshot.get(&m_name).map(|v| v.as_slice()).unwrap_or(&[]);
            let self_singular_count = self_rels
                .iter()
                .filter(|(_, tgt, is_arr)| *tgt == m_name && !*is_arr)
                .count();
            let self_list_count = self_rels
                .iter()
                .filter(|(_, tgt, is_arr)| *tgt == m_name && *is_arr)
                .count();
            let self_fields = fields_map.get(&m_name);

            let mut filtered_rels: Vec<Value> = Vec::with_capacity(rels_arr.len());
            for rel_val in &rels_arr {
                let Some(rel_str) = rel_val.as_str() else {
                    filtered_rels.push(rel_val.clone());
                    continue;
                };
                let (rel_field, target_model, is_array) = parse_lbg_relation_entry(rel_str);
                let mut record_removed = || {
                    if !rel_field.is_empty() {
                        removed_rel_fields
                            .entry(m_name.clone())
                            .or_default()
                            .insert(rel_field.to_lowercase());
                    }
                    if !target_model.is_empty() {
                        removed_rel_targets
                            .entry(m_name.clone())
                            .or_default()
                            .insert(target_model.clone());
                    }
                };
                if target_model.is_empty() || !declared_models.contains(&target_model) {
                    record_removed();
                    changed_any = true;
                    continue;
                }

                if target_model == m_name {
                    if is_array && self_singular_count == 0 && self_list_count < 2 {
                        record_removed();
                        changed_any = true;
                        continue;
                    }
                    filtered_rels.push(rel_val.clone());
                    continue;
                }

                let target_has_rel_back = rels_snapshot
                    .get(&target_model)
                    .map(|list| list.iter().any(|(_, t, _)| *t == m_name))
                    .unwrap_or(false);
                let m_fk_camel = format!("{}id", m_name.to_lowercase());
                let m_fk_snake = format!("{}_id", m_name.to_lowercase());
                let target_has_fk_to_m = fields_map
                    .get(&target_model)
                    .map(|fs| fs.contains(&m_fk_camel) || fs.contains(&m_fk_snake))
                    .unwrap_or(false);
                let self_has_fk_to_target = self_fields
                    .map(|fs| {
                        let t_camel = format!("{}id", target_model.to_lowercase());
                        let t_snake = format!("{}_id", target_model.to_lowercase());
                        let f_camel = if rel_field.is_empty() { String::new() } else { format!("{}id", rel_field.to_lowercase()) };
                        let f_snake = if rel_field.is_empty() { String::new() } else { format!("{}_id", rel_field.to_lowercase()) };
                        fs.contains(&t_camel)
                            || fs.contains(&t_snake)
                            || (!f_camel.is_empty() && fs.contains(&f_camel))
                            || (!f_snake.is_empty() && fs.contains(&f_snake))
                    })
                    .unwrap_or(false);

                let keep = target_has_rel_back
                    || (is_array && target_has_fk_to_m)
                    || (!is_array && self_has_fk_to_target);
                if keep {
                    filtered_rels.push(rel_val.clone());
                } else {
                    record_removed();
                    changed_any = true;
                }
            }

            if filtered_rels.len() != rels_arr.len() {
                m_obj.insert("relations".to_string(), Value::Array(filtered_rels));
            }
        }

        if !changed_any {
            break;
        }
    }

    // Propagate pruned relations & undeclared non-scalar types to `typedFields` and `fields`
    let model_keys: Vec<String> = models_obj.keys().cloned().collect();
    for m_name in model_keys {
        let Some(m_obj) = models_obj.get_mut(&m_name).and_then(|v| v.as_object_mut()) else {
            continue;
        };
        let mut pruned_names: std::collections::HashSet<String> = removed_rel_fields
            .get(&m_name)
            .cloned()
            .unwrap_or_default();
        let bad_targets: std::collections::HashSet<String> = removed_rel_targets
            .get(&m_name)
            .cloned()
            .unwrap_or_default();

        let should_prune_entry = |raw_entry: &str, extra_pruned: &std::collections::HashSet<String>| -> (bool, String) {
            let (fname, ty_opt) = parse_lbg_field_name_and_type(raw_entry);
            let fname_low = fname.to_lowercase();
            if !fname_low.is_empty() && extra_pruned.contains(&fname_low) {
                return (true, fname_low);
            }
            if let Some((base_ty, is_arr)) = ty_opt {
                if bad_targets.contains(&base_ty) {
                    return (true, fname_low);
                }
                if !is_prisma_primitive_or_scalar_type(&base_ty)
                    && !declared_models.contains(&base_ty)
                    && !base_ty.starts_with("Unsupported")
                    && (is_arr || (has_declared_enums && !declared_enums.contains(&base_ty)))
                {
                    return (true, fname_low);
                }
            }
            (false, fname_low)
        };

        if let Some(tf_arr) = m_obj.get("typedFields").and_then(|f| f.as_array()).cloned() {
            let mut filtered_tf: Vec<Value> = Vec::with_capacity(tf_arr.len());
            for val in &tf_arr {
                if let Some(s) = val.as_str() {
                    let (prune, fname_low) = should_prune_entry(s, &pruned_names);
                    if prune {
                        if !fname_low.is_empty() {
                            pruned_names.insert(fname_low);
                        }
                        continue;
                    }
                }
                filtered_tf.push(val.clone());
            }
            if filtered_tf.len() != tf_arr.len() {
                m_obj.insert("typedFields".to_string(), Value::Array(filtered_tf));
            }
        }

        if let Some(f_arr) = m_obj.get("fields").and_then(|f| f.as_array()).cloned() {
            let mut filtered_f: Vec<Value> = Vec::with_capacity(f_arr.len());
            for val in &f_arr {
                if let Some(s) = val.as_str() {
                    let (prune, _) = should_prune_entry(s, &pruned_names);
                    if prune {
                        continue;
                    }
                }
                filtered_f.push(val.clone());
            }
            if filtered_f.len() != f_arr.len() {
                m_obj.insert("fields".to_string(), Value::Array(filtered_f));
            }
        }
    }

    out
}

/// Formats database contract JSON into a compact, high-signal prompt block.
pub fn format_database_contract_prompt(db_val: &Value) -> String {
    let mut out = String::new();
    if let Some(models) = db_val.get("models").and_then(|m| m.as_object()) {
        if !models.is_empty() {
            out.push_str("ACTIVE DATABASE MODELS:\n");
            for (m_name, m_data) in models {
                let fields = m_data.get("typedFields")
                    .or_else(|| m_data.get("fields"))
                    .and_then(|f| f.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(", "))
                    .unwrap_or_default();
                let scalars = m_data.get("fields")
                    .and_then(|f| f.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(", "))
                    .unwrap_or_default();
                let relations = m_data.get("relations")
                    .and_then(|r| r.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(", "))
                    .unwrap_or_default();
                let delegate = m_data.get("delegate")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| model_to_prisma_delegate(m_name));
                let required = m_data.get("requiredFields")
                    .and_then(|r| r.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(", "))
                    .unwrap_or_default();

                let scalars_line = if !scalars.is_empty() { scalars } else { fields.clone() };
                out.push_str(&format!("- model {} {{\n    scalars: [{}]\n    relations: [{}]\n", m_name, scalars_line, relations));
                if m_data.get("typedFields").is_some() && !fields.is_empty() {
                    out.push_str(&format!("    types: [{}]\n", fields));
                }
                if !delegate.is_empty() {
                    out.push_str(&format!("    delegate: prisma.{}\n", delegate));
                }
                if !required.is_empty() {
                    out.push_str(&format!("    requiredOnCreate: [{}]\n", required));
                }
                out.push_str("  }\n");
            }
        }
    }

    if let Some(enums) = db_val.get("enums").and_then(|e| e.as_object()) {
        if !enums.is_empty() {
            out.push_str("DATABASE ENUMS:\n");
            for (e_name, e_vals) in enums {
                let vals = e_vals.as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(", "))
                    .unwrap_or_default();
                out.push_str(&format!("- enum {} {{ {} }}\n", e_name, vals));
            }
        }
    }

    out
}

/// Formats API endpoints contract JSON into a compact prompt block.
pub fn format_api_endpoints_prompt(api_val: &Value) -> String {
    let mut out = String::new();
    let endpoints = api_val.get("endpoints").and_then(|e| e.as_object()).or_else(|| api_val.as_object());
    if let Some(eps) = endpoints {
        let mut valid_lines: Vec<String> = Vec::new();
        for (ep_path, methods_val) in eps {
            if !is_valid_lbg_api_endpoint_key(ep_path) {
                continue;
            }
            let methods = methods_val.as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(", "))
                .unwrap_or_else(|| "GET".to_string());
            valid_lines.push(format!("- [{}] {}\n", methods, ep_path));
        }
        if !valid_lines.is_empty() {
            out.push_str("VERIFIED API ENDPOINTS TO CALL:\n");
            for line in valid_lines {
                out.push_str(&line);
            }
        }
    }
    out
}

/// Formats custom hooks contract JSON into a compact prompt block.
pub fn format_hooks_prompt(hooks_val: &Value) -> String {
    let mut out = String::new();
    if let Some(hooks) = hooks_val.as_object() {
        if !hooks.is_empty() {
            out.push_str("VERIFIED CUSTOM HOOKS (Import from '@/hooks'):\n");
            for (h_name, _) in hooks {
                out.push_str(&format!("- {}\n", h_name));
            }
        }
    }
    out
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
        let db_archetype = get_project_database_archetype_from_dir(proj_dir);
        let mut prompt = format!(
            "You are a Senior Database Architect. Write only the complete, valid schema.prisma code wrapped in markdown code blocks. No explanations.\n\n             {}             DATABASE GENERATION CHAIN-OF-THOUGHT (Execute in strict sequence):\n             Step 1 [Domain Enums]: Define all domain enums (e.g. Role, Status, Priority) at the top of the schema when fixed choice sets exist.\n             Step 2 [Core Entities & Attributes]: Define models with IDs (@id @default(cuid())), scalar columns, and timestamps (createdAt, updatedAt). Make password fields optional (hashedPassword String?).\n             Step 3 [Relational Topography & Foreign Keys]:\n                    - One-to-Many: Put scalar foreign key (e.g. `<parentId> String`) and `@relation(fields: [<parentId>], references: [id])` on `<ChildModel>`. Put the symmetric list field (`<children> <ChildModel>[]`) on `<ParentModel>`.\n                    - One-to-One: Put scalar foreign key WITH `@unique` on `<ChildModel>` (`<parentId> String @unique` and `@relation(fields: [<parentId>], references: [id])`) and optional singular inverse (`<child> <ChildModel>?`) on `<ParentModel>`.\n                    - Multiple Relations Between Same Models: If `<ModelA>` defines two or more relations pointing to the SAME `<ModelB>` (`<roleA> <ModelB>` and `<roleB> <ModelB>?`), you MUST assign distinct `@relation(\"RoleA\", fields: [<roleAId>], references: [id])` / `@relation(\"RoleB\", ...)` tags and matching `@relation(\"RoleA\")` / `@relation(\"RoleB\")` opposite fields on `<ModelB>`.\n                    - Implicit Many-to-Many: Define array fields on both `<ModelA>` and `<ModelB>` ONLY when BOTH sides declare matching opposite arrays.\n             Step 4 [Verification Gate]: Verify before finalizing:\n                    1. Every relation field has an opposite field on a declared model in this schema (zero undefined model types, zero one-sided dangling array fields).\n                    2. Zero duplicate relation fields exist between the same two models.\n                    3. If multiple relations exist between the same two models, each pair has a distinct `@relation(\"Name\")`.\n                    4. Zero self-relations (`Model[]`) exist unless defining a tree with both `parent <Model>? @relation(\"<Model>Hierarchy\", fields: [parentId], references: [id])` and `children <Model>[] @relation(\"<Model>Hierarchy\")`.\n                    5. All foreign key scalars in 1-to-1 relations have `@unique`.\n\n             PRISMA SCHEMA RULES:\n             - The `@relation` attribute MUST ONLY be placed on relational object fields, never on scalar fields.\n             - Referential actions in `@relation` (`onDelete`, `onUpdate`) MUST use exact Prisma PascalCase keywords: `SetNull`, `Cascade`, `Restrict`, `NoAction`, `SetDefault` (NEVER uppercase SQL syntax like `SET_NULL` or `CASCADE`).\n             - For NextAuth schema, the VerificationToken model does not have a direct relation back to the User model.\n             - Fixed sets of values MUST be defined using Prisma `enum Name {{ VAL1 VAL2 }}`, NEVER as a relational `model`.\n",
            db_archetype
        );

        if let Some(ref lbg) = lbg_val {
            let clean_lbg = sanitize_live_builder_guide_contracts(lbg);
            if let Some(db_models) = clean_lbg.get("sharedContracts").and_then(|c| c.get("database")).and_then(|d| d.get("models")) {
                prompt.push_str(&format!("\nREQUIRED DATABASE MODELS CONTRACT:\n{}\n", db_models));
            }
        }
        return prompt;
    }

    // 6b. FILE-SCOPED PROMPT: Prisma Client Singleton (src/lib/prisma.ts)
    if path_lower.ends_with("lib/prisma.ts") || path_lower.ends_with("lib/prisma.js") {
        return "You are an expert Prisma and Next.js Database Architect. Output only the complete, valid production-ready src/lib/prisma.ts singleton file wrapped in markdown code blocks. No explanations.\n\n\
                PRISMA SINGLETON RULES:\n\
                - Export a cached PrismaClient on globalThis to prevent connection pool exhaustion during Next.js App Router hot reloading.\n\
                - Export BOTH named (`export const prisma`) and default (`export default prisma`) exports.\n\n\
                ```typescript\n\
                import { PrismaClient } from '@prisma/client';\n\n\
                const globalForPrisma = globalThis as unknown as {\n\
                  prisma: PrismaClient | undefined;\n\
                };\n\n\
                export const prisma =\n\
                  globalForPrisma.prisma ??\n\
                  new PrismaClient({\n\
                    log: process.env.NODE_ENV === 'development' ? ['query', 'error', 'warn'] : ['error'],\n\
                  });\n\n\
                if (process.env.NODE_ENV !== 'production') globalForPrisma.prisma = prisma;\n\n\
                export default prisma;\n\
                ```\n".to_string();
    }

    // 7. FILE-SCOPED PROMPT: Backend API Routes & NextAuth
    let is_api_route = path_lower.contains("/api/") || path_lower.contains("route.ts") || path_lower.contains("route.js");
    if is_api_route {
        let mut category = "saas".to_string();
        if let Some(ref lbg) = lbg_val {
            if let Some(cat) = lbg.get("projectSpecification").and_then(|s| s.get("category")).and_then(|c| c.as_str()) {
                category = cat.to_string();
            }
        }
        let api_archetype = resolve_api_domain_archetype(&category);

        let mut prompt = format!(
            "You are a Senior Backend Developer. Write only clean, production-ready Next.js Route Handler code wrapped in markdown code blocks. No explanations.\n\n\
             {}\n\n\
             {}\n\n\
             BACKEND API DEVELOPMENT RULES:\n\
             - Implement clean Next.js App Router Route Handlers (exporting GET, POST, PUT, DELETE functions).\n\
             - STRICT IMPORT RESOLUTION: Every external dependency ('zod', 'bcryptjs', '@/lib/prisma', 'next/server') MUST be explicitly imported at the top.\n",
            api_archetype,
            API_ROUTE_CHAIN_OF_THOUGHT
        );

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
            if let Some(db_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("database")) {
                let fmt_db = format_database_contract_prompt(db_contracts);
                if !fmt_db.is_empty() {
                    prompt.push_str(&format!("\n{}\n", fmt_db));
                }
            }
            if let Some(api_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("api")) {
                let fmt_api = format_api_endpoints_prompt(api_contracts);
                if !fmt_api.is_empty() {
                    prompt.push_str(&format!("\n{}\n", fmt_api));
                }
            }
        }
        let contracts_prompt = format_axiom_contracts_prompt(proj_dir, file_path);
        if !contracts_prompt.is_empty() {
            prompt.push_str(&format!("\n{}\n", contracts_prompt));
        }
        if !context_ref.is_empty() {
            prompt.push_str(context_ref);
        }
        return prompt;
    }

    // 7b. FILE-SCOPED PROMPT: Next.js Edge Middleware (middleware.ts / middleware.js)
    if path_lower.ends_with("middleware.ts") || path_lower.ends_with("middleware.js") {
        let meta_rules = get_middleware_metacognitive_rules(Some(proj_dir), context_ref);
        return format!(
            "You are a Senior Next.js Edge Architecture Engineer. Write only clean, production-ready Next.js Middleware code wrapped in markdown code blocks. No explanations.\n\n{}\n{}\n\
             - NEVER import from 'express' (no Request, Response, NextFunction from 'express'). Always import `{{ NextResponse }}` and `type {{ NextRequest }}` from `'next/server'`.\n\
             - Always export `function middleware(request: NextRequest)` and `export const config = {{ matcher: [...] }}`.\n",
            NEXTJS_MIDDLEWARE_RULEPACK,
            meta_rules
        );
    }

    // 7c. FILE-SCOPED PROMPT: Custom React Hooks (src/hooks/**)
    let is_app_route_prompt = crate::sanitizer::is_app_router_route_file(file_path);
    let is_custom_hook = !is_app_route_prompt
        && (path_lower.contains("/hooks/") || path_lower.starts_with("hooks/"))
        && !path_lower.contains("/api/")
        && (path_lower.ends_with(".ts") || path_lower.ends_with(".tsx"));
    if is_custom_hook {
        let mut prompt = format!(
            "You are a Senior React Hooks & State Management Architect. Write only clean, production-ready React hook TypeScript code wrapped in markdown code blocks. No explanations.\n\n\
             {}\n\n\
             HOOK ARCHITECTURE RULES:\n\
             - This is a headless React hook. Do NOT output JSX elements (no <div>, <button>, etc.).\n\
             - Hook name MUST start with 'use' (camelCase: useProjects, useAuth, useMetrics).\n\
             - Provide clear TypeScript interfaces for state and hook return values.\n",
            HOOK_CHAIN_OF_THOUGHT
        );

        if let Some(ref lbg) = lbg_val {
            if let Some(api_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("api")) {
                let fmt_api = format_api_endpoints_prompt(api_contracts);
                if !fmt_api.is_empty() {
                    prompt.push_str(&format!("\n{}\n", fmt_api));
                }
            }
        }
        let contracts_prompt = format_axiom_contracts_prompt(proj_dir, file_path);
        if !contracts_prompt.is_empty() {
            prompt.push_str(&format!("\n{}\n", contracts_prompt));
        }
        if !context_ref.is_empty() {
            prompt.push_str(context_ref);
        }
        return prompt;
    }

    // 7d. FILE-SCOPED PROMPT: Domain Models & Types (src/models/**, src/types/**)
    let is_model_or_type_file = !is_app_route_prompt
        && (path_lower.contains("/models/") || path_lower.starts_with("models/") || path_lower.contains("/types/") || path_lower.starts_with("types/"))
        && (path_lower.ends_with(".ts") || path_lower.ends_with(".tsx"))
        && !path_lower.contains("/api/");
    if is_model_or_type_file {
        let mut prompt = format!(
            "You are a Senior TypeScript Domain Architect. Write only clean, pure TypeScript domain types, interfaces, enums, and lightweight helper types wrapped in markdown code blocks. No explanations.\n\n\
             DOMAIN MODEL & TYPE RULES:\n\
             - Define clean, exported TypeScript interfaces, types, and enums matching the project's database entities.\n\
             - STRICTLY FORBIDDEN: NEVER import `'@prisma/client'`, NEVER import `'@/lib/prisma'`, and NEVER instantiate `new PrismaClient()` inside `src/models/` or `src/types/`. Database access belongs strictly inside API route handlers (`src/app/api/**/route.ts`).\n\
             - PATH ALIASES: Always import other local project modules using `'@/...'` (e.g. `import ... from '@/types/...'`), NEVER bare `'src/...'`.\n\
             - Export symbols both cleanly as named exports (`export interface ...`, `export type ...`, `export function ...`).\n\
             FILE: {}\n",
            file_path
        );
        if let Some(ref lbg) = lbg_val {
            if let Some(db_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("database")) {
                let fmt_db = format_database_contract_prompt(db_contracts);
                if !fmt_db.is_empty() {
                    prompt.push_str(&format!("\n{}\n", fmt_db));
                }
            }
        }
        let contracts_prompt = format_axiom_contracts_prompt(proj_dir, file_path);
        if !contracts_prompt.is_empty() {
            prompt.push_str(&format!("\n{}\n", contracts_prompt));
        }
        if !context_ref.is_empty() {
            prompt.push_str(context_ref);
        }
        return prompt;
    }

    // 8. FILE-SCOPED PROMPT: Web Client UI Components, Pages & Layouts (.tsx, .jsx)
    // Strictly isolated from backend Prisma schemas and NextAuth server internals!
    let is_react_frontend = (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx")) && !is_custom_hook;
    if is_react_frontend {
        let is_route_file = crate::sanitizer::is_app_router_route_file(file_path);
        let ui_cot = if is_route_file {
            UI_ROUTE_PAGE_CHAIN_OF_THOUGHT
        } else {
            UI_GENERATION_CHAIN_OF_THOUGHT
        };
        let mut prompt = format!(
            "You are a Principal React & Tailwind CSS Product Designer and Frontend Engineer. Write only clean, complete, production-ready React code wrapped in markdown code blocks. No explanations.\n\n\
             {}\n\n",
            ui_cot
        );

        // Inject tailored Design Archetype tokens based on the project's domain & category
        let design_archetype = get_project_design_archetype_from_dir(proj_dir);
        prompt.push_str(&design_archetype);

        if path_lower.ends_with("layout.tsx") || path_lower.ends_with("layout.jsx") {
            prompt.push_str(
                "ROOT LAYOUT ARCHITECTURE RULES:\n\
                 - DOM HIERARCHY: Outermost elements returned MUST be `<html lang=\"en\"><body className=\"min-h-screen flex flex-col bg-slate-50 text-slate-900 antialiased selection:bg-indigo-600 selection:text-white\">...</body></html>`.\n\
                 - MANDATORY `{children}` SLOT: RootLayout MUST render `{children}` inside `<main className=\"flex-1 flex flex-col w-full\">{children}</main>` inside `<body>` (wrapped by `<SessionProvider>` if NextAuth is configured).\n\
                 - STRICT SHELL-ONLY DISCIPLINE: NEVER put KPI cards, `<table>` elements, page forms, or domain hooks (`useAlerts`, `useDroneFeed`, etc.) inside `layout.tsx`! Page content belongs inside `src/app/page.tsx` and child route pages, NEVER inside RootLayout.\n\
                 - PERSISTENT NAVIGATION & STICKY FOOTER: Always import and mount `<Navbar />` (from `@/components/Navbar`) above `<main>`, and `<Footer />` (from `@/components/Footer`) below `<main>`.\n\
                 - MANDATORY DEFAULT EXPORT: RootLayout MUST have `export default function RootLayout({ children }: { children: React.ReactNode })` and MUST NOT emit arbitrary named value exports.\n\
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
            let export_compat_rule = if is_route_file {
                "- DEFAULT EXPORT ONLY: Next.js App Router route files MUST export ONLY `export default ComponentName` (and reserved Next.js exports such as `metadata`); NEVER emit named component/function/const exports."
            } else {
                "- DUAL EXPORT COMPATIBILITY: Export the component as both a named export (`export { ComponentName }`) and default export (`export default ComponentName`)."
            };
            prompt.push_str(&format!(
                "CORE FRONTEND COMPONENT RULES:\n\
                 - CLIENT-SIDE BOUNDARY: NEVER import `@prisma/client` or `/api/**/route` in React components or hooks. Fetch data from `/api/...` endpoints.\n\
                 - STRICT TYPE DEFINITIONS & IMPORTS: Declare TypeScript interfaces cleanly inside the file. Import icons ONLY from valid subpaths (`'react-icons/hi'`, `'react-icons/fa'`, `'react-icons/fi'`, `'react-icons/io5'`, or `'lucide-react'`), NEVER from bare `'react-icons'`.\n\
                 - NEXT.JS LINK RULE: Do NOT place `<a>` tags inside `<Link>`. Put `className` and `prefetch={{false}}` directly on `<Link>`.\n\
                 - NO DEAD IMPORTS: Do not import external local modules unless they exist in the project; prefer clean self-contained components or verified `@/components/ui/*` primitives.\n\
                 {}\n\
                 - NO MANUAL AXIOM ATTRS: Never declare or use 'data-axiom-component' or 'data-axiom-file' props.\n\n",
                export_compat_rule
            ));
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
            if let Some(api_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("api")) {
                let fmt_api = format_api_endpoints_prompt(api_contracts);
                if !fmt_api.is_empty() {
                    prompt.push_str(&format!("\n{}\n", fmt_api));
                }
            }
            if let Some(hooks_contracts) = lbg.get("sharedContracts").and_then(|c| c.get("hooks")) {
                let fmt_hooks = format_hooks_prompt(hooks_contracts);
                if !fmt_hooks.is_empty() {
                    prompt.push_str(&format!("\n{}\n", fmt_hooks));
                }
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

        let contracts_prompt = format_axiom_contracts_prompt(proj_dir, file_path);
        if !contracts_prompt.is_empty() {
            prompt.push_str(&format!("\n{}\n", contracts_prompt));
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
        prompt.push_str(&format!("ROLE: Expert TypeScript / JavaScript Developer\nFILE: {}\n- Always import local modules using the '@/...' alias (never bare 'src/...').\n- Never clone new PrismaClient() outside src/lib/prisma.ts.\n", file_path));
        let path_low_fallback = file_path.to_lowercase().replace('\\', "/");
        if path_low_fallback.contains("nextauth") || path_low_fallback.ends_with("/auth.ts") || path_low_fallback.ends_with("/auth.js") || path_low_fallback == "auth.ts" {
            prompt.push_str("- NEXTAUTH V4 ONLY: Import `type { NextAuthOptions }` from `'next-auth'` and `CredentialsProvider` from `'next-auth/providers/credentials'` (NEVER deprecated v3 `'next-auth/providers'`). Use `PrismaAdapter` from `'@auth/prisma-adapter'` with `prisma` from `'@/lib/prisma'`.\n- AUTH UTILITY BOUNDARY: Export `authOptions: NextAuthOptions` and auth helpers ONLY from utility files (`src/utils/auth.ts`, `src/lib/auth.ts`); NEVER export NextAuth App Router `{ handler as GET, handler as POST }` outside `src/app/api/auth/[...nextauth]/route.ts`.\n- OPTIONAL USER FIELDS: Access optional credential hashes via safe cast `(user as any)?.hashedPassword || (user as any)?.password || ''` so `tsc` passes even when `User` schema uses either field name.\n");
        }
    }
    if !context_ref.is_empty() {
        prompt.push_str(context_ref);
    }
    prompt
}

fn extract_live_code_preview(raw: &str) -> String {
    let trimmed = raw.trim_start();
    let without_fence = if trimmed.starts_with("```") {
        if let Some(newline_pos) = trimmed.find('\n') {
            &trimmed[newline_pos + 1..]
        } else {
            ""
        }
    } else {
        trimmed
    };
    let without_closing = without_fence.trim_end().strip_suffix("```").unwrap_or(without_fence);
    let lines: Vec<&str> = without_closing.lines().collect();
    let tail_lines = if lines.len() > 22 {
        &lines[lines.len() - 22..]
    } else {
        &lines[..]
    };
    let joined = tail_lines.join("\n");
    if joined.chars().count() > 1400 {
        let count = joined.chars().count();
        joined.chars().skip(count - 1400).collect()
    } else {
        joined
    }
}

/// Replaces upstream manifest sentinel/failure titles (e.g. "Business Analysis Failed",
/// "Unknown Project", empty strings) with a clean domain name derived from `description`
/// or a deterministic ID fallback.
pub fn resolve_manifest_project_name(raw_name: Option<&str>, description: &str, project_id: &str) -> String {
    let raw = raw_name.unwrap_or("").trim();
    let raw_low = raw.to_lowercase();
    let is_sentinel = raw.is_empty()
        || matches!(
            raw_low.as_str(),
            "business analysis failed"
                | "analysis failed"
                | "project analysis failed"
                | "generation failed"
                | "error"
                | "failed"
                | "unknown project"
                | "unknown"
                | "untitled"
                | "untitled project"
                | "n/a"
                | "null"
                | "undefined"
        )
        || raw_low.contains("analysis failed")
        || raw_low.starts_with("error:")
        || raw_low.starts_with("failed:");

    if !is_sentinel {
        return raw.to_string();
    }

    let desc_trimmed = description.trim();
    if !desc_trimmed.is_empty() {
        for sep in [" is a ", " is an ", " is the ", ": ", " - ", " — ", " – "] {
            if let Some(pos) = desc_trimmed.find(sep) {
                let candidate = desc_trimmed[..pos]
                    .trim()
                    .trim_matches(|c: char| matches!(c, '"' | '\'' | '`'));
                let wc = candidate.split_whitespace().count();
                if !candidate.is_empty() && wc >= 1 && wc <= 6 && candidate.len() <= 60 {
                    return candidate.to_string();
                }
            }
        }
        let first_clause = desc_trimmed
            .split(['.', '!', '?', '\n'])
            .next()
            .unwrap_or(desc_trimmed)
            .trim();
        let words: Vec<&str> = first_clause.split_whitespace().take(4).collect();
        if !words.is_empty() {
            let candidate = words
                .join(" ")
                .trim_matches(|c: char| matches!(c, ',' | ':' | ';' | '-' | '"' | '\''))
                .to_string();
            if !candidate.is_empty() {
                return candidate;
            }
        }
    }

    let short_id: String = project_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect();
    if short_id.is_empty() {
        "Axiom Project".to_string()
    } else {
        format!("Axiom Project {}", short_id)
    }
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
        let status = match phase {
            "paused" => "paused",
            "stopped" => "stopped",
            "complete" | "success" => "completed",
            "error" => "error",
            _ => "generating",
        };
        debug_log_to_file(format!(
            "[Orchestrator Progress] Emit event - phase: {}, message: {}, progress: {}%",
            phase, msg, progress
        ));
        let _ = window.app_handle().emit("task:progress", serde_json::json!({
            "taskId": task_id,
            "projectId": project_id,
            "phase": phase,
            "status": status,
            "offlineWaiting": false,
            "message": msg,
            "progress": progress
        }));
    };
    let emit_live_code = |phase: &str, msg: &str, progress: f64, current_file: &str, file_desc: &str, file_idx: usize, total_files: usize, token_count: usize, live_code: &str| {
        let status = match phase {
            "paused" => "paused",
            "stopped" => "stopped",
            "complete" | "success" => "completed",
            "error" => "error",
            _ => "generating",
        };
        let _ = window.app_handle().emit("task:progress", serde_json::json!({
            "taskId": task_id,
            "projectId": project_id,
            "phase": phase,
            "status": status,
            "offlineWaiting": false,
            "message": msg,
            "progress": progress,
            "currentFile": current_file,
            "fileDescription": file_desc,
            "fileIndex": file_idx,
            "totalFiles": total_files,
            "tokenCount": token_count,
            "liveCode": live_code
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

    let resuming_install_deps = existing_checkpoint
        .as_ref()
        .and_then(|cp| cp.get("phase").and_then(|v| v.as_str()))
        .map_or(false, |p| p == "install-deps");

    let resume_base_pct: Option<f64> = files_to_generate_from_cp.as_ref().and_then(|ftg| {
        if resuming_install_deps {
            Some(75.0)
        } else if !completed_files.is_empty() && !ftg.is_empty() {
            Some((25.0 + (completed_files.len() as f64 / ftg.len() as f64) * 55.0).min(99.0))
        } else {
            None
        }
    });

    emit_progress(
        if resuming_install_deps {
            "install-deps"
        } else if resume_base_pct.is_some() {
            "generate-files"
        } else {
            "fetch-manifest"
        },
        if resuming_install_deps {
            "Nastavljam instalaciju zavisnosti i završne provere..."
        } else if files_to_generate_from_cp.is_some() {
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
    let desc_val = manifest.get("description")
        .or_else(|| manifest.get("projectDescription"))
        .or_else(|| manifest.get("idea"))
        .or_else(|| manifest.get("content"))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("description")))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("idea")));
    let project_description = desc_val.and_then(|d| d.as_str()).unwrap_or("");
    let name_val = manifest.get("name")
        .or_else(|| manifest.get("projectName"))
        .or_else(|| manifest.get("title"))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("name")))
        .or_else(|| manifest.get("metadata").and_then(|m| m.get("title")));
    let name_owned = resolve_manifest_project_name(
        name_val.and_then(|n| n.as_str()),
        project_description,
        &project_id,
    );
    let name = name_owned.as_str();
    let project_type = detect_project_type(&manifest);

    // Immediately register/update the project in projects.json with "generating" status
    // so it appears on the Dashboard right away with its real name and description.
    let initial_proj = serde_json::json!({
        "id": project_id,
        "name": name,
        "description": project_description,
        "status": "generating",
        "metadata": {
            "appVersion": env!("CARGO_PKG_VERSION"),
            "modelId": model_id
        },
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
                    "description": "Resource-optimized middleware that bypasses static asset queries at the matcher layer.",
                    "language": "typescript",
                    "defaultContent": r#"import { withAuth } from "next-auth/middleware";
import { NextRequest, NextResponse } from "next/server";

const authMiddleware = withAuth(
  function middleware(_req: NextRequest) {
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

  // Protected user & admin routes (static assets & /api are already excluded in config.matcher)
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
  matcher: [
    "/((?!api|_next/static|_next/image|_vercel|favicon\\.ico|robots\\.txt|sitemap\\.xml|.*\\..*).*)",
  ],
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
         STRICT SCOPE & STACK GUARDRAILS:\n\
         - Stick strictly to Next.js + React + TypeScript. NEVER add Vue, Svelte, or Angular files (`.vue`, `.svelte`, `vue-i18n`, `vue-router`).\n\
         - NEVER expand a matrix of dozens of regional locale/translation JSON files (`translations/*.json`, `locales/*.json`); keep at most 1-2 locale files if localization is needed.\n\
         - NEVER duplicate shared folders inside `src/app/` (`src/app/components/...`, `src/app/utils/...`, `src/app/config/...`); keep shared code in `src/components/`, `src/lib/`, `src/hooks/`, `src/utils/`.\n\
         - API ROUTE COMPLETENESS: Ensure EVERY core domain entity in `prisma/schema.prisma` and every `/api/...` endpoint called by hooks or pages has a real matching App Router handler (`src/app/api/<resource>/route.ts`).\n\
         - SHARED TYPES PLACEMENT: Keep shared TypeScript interfaces, types, and enums in `src/types/...` or `src/models/...`, NEVER exported from `src/app/**/page.tsx`.\n\
         - Keep the expanded file list cohesive and lean (~15-30 essential files total).\n\
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
                    let mut manifest_token_count = 0usize;
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
                                                    manifest_token_count += 1;
                                                    if manifest_token_count == 1 || manifest_token_count % 10 == 0 {
                                                        let preview = extract_live_code_preview(&accumulated_response);
                                                        emit_live_code(
                                                            "manifest-expansion",
                                                            &format!("AI projektuje arhitekturu manifesta... ({} tokena)", manifest_token_count),
                                                            13.0,
                                                            ".axiom/manifest.json",
                                                            "Proširivanje arhitekture projekta, ruta, modela baze i UI komponenti",
                                                            0,
                                                            0,
                                                            manifest_token_count,
                                                            &preview,
                                                        );
                                                    }
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
                        files_to_generate = crate::sanitizer::filter_runaway_manifest_expansion(&files, merged, &project_type);
                        crate::sanitizer::deduplicate_manifest_files(&mut files_to_generate);
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
            
            let proj_desc = manifest.get("metadata").and_then(|m| m.get("description")).and_then(|d| d.as_str()).unwrap_or(project_description);
            let proj_name = resolve_manifest_project_name(
                manifest.get("metadata").and_then(|m| m.get("name")).and_then(|n| n.as_str()).or(Some(name)),
                proj_desc,
                &project_id,
            );
            
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

    // Sort files_to_generate by generation phase (with deterministic sub-ordering inside Phase 1)
    files_to_generate.sort_by_key(|file_spec| {
        let file_path = file_spec.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let p_lower = file_path.replace('\\', "/").to_lowercase();
        let is_app_route = crate::sanitizer::is_app_router_route_file(file_path);
        let is_api_path = p_lower.starts_with("api/") || p_lower.contains("/api/");
        let db_sub_priority = if p_lower.contains("schema.prisma") {
            0u8
        } else if p_lower.ends_with("lib/prisma.ts") || p_lower.ends_with("lib/prisma.js") {
            1u8
        } else if !is_app_route && !is_api_path && (p_lower.contains("/models/") || p_lower.starts_with("models/") || p_lower.contains("/types/") || p_lower.starts_with("types/")) {
            2u8
        } else {
            3u8
        };
        (get_generation_phase(file_path, &project_type), db_sub_priority, file_path.to_string())
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
        "appVersion": env!("CARGO_PKG_VERSION"),
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
        let sanitized_lbg = sanitize_live_builder_guide_contracts(lbg_val);
        if let Ok(lbg_content) = serde_json::to_string_pretty(&sanitized_lbg) {
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
            if let Ok(c) = fs::read_to_string(&full_disk_path) {
                harvest_file_contract_to_lbg(&proj_dir, file_path, &c);
                harvest_file_to_axiom_contracts(&proj_dir, &project_id, file_path, &c, &files_to_generate);
            }
            continue;
        }

        let msg = format!("Generating {}/{} - {}", i + 1, files_to_generate.len(), file_path);
        emit_progress("generate-files", &msg, current_progress);
        emit_live_code(
            "generate-files",
            &msg,
            current_progress,
            file_path,
            desc,
            i + 1,
            files_to_generate.len(),
            0,
            "// AI analizira kontekst i započinje pisanje koda...",
        );

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

        let has_auth_routes = files_to_generate.iter().any(|f| {
            f.get("path").and_then(|p| p.as_str()).map_or(false, |p| {
                let low = p.replace('\\', "/").to_lowercase();
                low.contains("api/auth/") || low.contains("api/register") || low.contains("nextauth")
            })
        });
        let is_schema_prisma = file_path.replace('\\', "/").to_lowercase().ends_with("schema.prisma");
        if is_schema_prisma {
            let api_routes_summary = extract_planned_api_routes_summary(&files_to_generate);
            if !api_routes_summary.is_empty() {
                system_prompt.push_str(&api_routes_summary);
            }
            if has_auth_routes {
                system_prompt.push_str(
                    "\nCRITICAL AUTH SCHEMA CONTRACT:\n- This project includes NextAuth / user authentication routes. You MUST declare `generator client { provider = \"prisma-client-js\" }`, `datasource db { provider = \"postgresql\"\n  url = env(\"DATABASE_URL\") }`, and `model User` (with id, email @unique, name?, emailVerified?, image?, hashedPassword?, role?, createdAt, updatedAt) inside `prisma/schema.prisma`.\n"
                );
            }
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
                while task_control.paused.load(Ordering::SeqCst) {
                    if task_control.stopped.load(Ordering::SeqCst) {
                        loop_detected = true;
                        break;
                    }
                    let preview = extract_live_code_preview(&full_content);
                    let sub_progress = current_progress + ((token_count as f64 / 600.0).min(0.9) * (55.0 / total_files.max(1.0)));
                    emit_live_code(
                        "paused",
                        &format!("Generisanje pauzirano na fajlu {}/{} ({} tokena)", i + 1, files_to_generate.len(), token_count),
                        sub_progress,
                        file_path,
                        desc,
                        i + 1,
                        files_to_generate.len(),
                        token_count,
                        &preview,
                    );
                    tokio::select! {
                        _ = task_control.notify.notified() => {},
                        _ = tokio::time::sleep(std::time::Duration::from_millis(400)) => {}
                    }
                }
                if task_control.stopped.load(Ordering::SeqCst) {
                    debug_log_to_file(format!("[Orchestrator] Task {} stopped during chunk generation for {}", task_id, file_path));
                    loop_detected = true;
                    break;
                }

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

                                if token_count == 1 || token_count % 8 == 0 {
                                    let preview = extract_live_code_preview(&full_content);
                                    let sub_progress = current_progress + ((token_count as f64 / 600.0).min(0.9) * (55.0 / total_files.max(1.0)));
                                    emit_live_code(
                                        "generate-files",
                                        &format!("AI typing {}... ({} tokens)", file_path, token_count),
                                        sub_progress,
                                        file_path,
                                        desc,
                                        i + 1,
                                        files_to_generate.len(),
                                        token_count,
                                        &preview,
                                    );
                                }
                                if token_count % 100 == 0 {
                                    debug_log_to_file(format!(
                                        "[Orchestrator Progress] AI typing {} ({}/{}): {} tokens",
                                        file_path, i + 1, files_to_generate.len(), token_count
                                    ));
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
            let pre_processed = if is_schema_prisma && has_auth_routes {
                crate::sanitizer::ensure_prisma_user_model_if_missing(&cleaned)
            } else {
                cleaned
            };
            processed_content = post_process_generated_file(file_path, &pre_processed, &metamanifest, has_tailwind);
            processed_content = crate::sanitizer::reconcile_server_prisma_enum_usage(&proj_dir, file_path, &processed_content);
            
            // Run project-aware validation agent
            match validate_file_with_project_context(&proj_dir, file_path, &processed_content, &model_id).await {
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
        harvest_file_contract_to_lbg(&proj_dir, file_path, &processed_content);
        harvest_file_to_axiom_contracts(&proj_dir, &project_id, file_path, &processed_content, &files_to_generate);

        // Emit final live code preview with completed, fully processed code
        let preview = extract_live_code_preview(&processed_content);
        emit_live_code(
            "generate-files",
            &format!("Completed {}/{} - {}", i + 1, files_to_generate.len(), file_path),
            current_progress,
            file_path,
            desc,
            i + 1,
            files_to_generate.len(),
            processed_content.len() / 4,
            &preview,
        );

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
        let mut db_name: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        while db_name.contains("__") {
            db_name = db_name.replace("__", "_");
        }
        db_name = db_name.trim_matches('_').chars().take(48).collect();
        if db_name.is_empty() || !db_name.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
            db_name = format!("axiom_{}", db_name.trim_start_matches('_'));
            if db_name == "axiom_" {
                db_name = "axiom_db".to_string();
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            emit_progress("install-deps", "Configuring local PostgreSQL database...", 74.2);
            match setup_local_postgres(&proj_dir, &db_name).await {
                Ok(db_url) => {
                    update_env_database_url(&proj_dir, &db_url);
                    emit_progress("install-deps", "Local PostgreSQL database configured successfully.", 74.8);
                }
                Err(err) => {
                    match fallback_to_sqlite(&proj_dir) {
                        Ok(()) => {
                            emit_progress("install-deps", &format!("PostgreSQL setup failed: {}. Falling back to SQLite...", err), 74.5);
                        }
                        Err(fb_err) => {
                            emit_progress("install-deps", &format!("PostgreSQL setup warning: {}. SQLite fallback skipped ({})", err, fb_err), 74.5);
                        }
                    }
                }
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            emit_progress("install-deps", "Non-Windows environment detected. Using local SQLite database fallback.", 74.5);
            let _ = fallback_to_sqlite(&proj_dir);
        }
    }

    // Ensure theme file, UI primitives & directory barrels exist BEFORE local import repair scans missing modules
    ensure_theme_file_exists(&proj_dir);
    crate::sanitizer::ensure_ui_primitives_reconciled(&proj_dir);
    ensure_directory_barrel_files(&proj_dir);

    // Resolve any broken local imports in generated JS/TS code
    repair_broken_imports(&proj_dir, &model_id).await;

    // Repair package.json dependencies based on actual source code imports
    repair_package_json_dependencies(&proj_dir);

    // Repair Cargo.toml dependencies if they exist
    repair_cargo_toml_dependencies(&proj_dir);

    // Repair pubspec.yaml dependencies if they exist
    repair_pubspec_yaml_dependencies(&proj_dir);

    // Persist install-deps phase before starting npm resolution
    checkpoint_state["phase"] = serde_json::json!("install-deps");
    checkpoint_state["offlineWaiting"] = serde_json::json!(false);
    if let Some(obj) = checkpoint_state.as_object_mut() {
        obj.remove("pausedReason");
    }
    checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    save_generation_checkpoint(&proj_dir, &checkpoint_state);

    // Run npm install with automatic self-healing and version locking
    let npm_install_outcome = run_npm_install_with_auto_healing(&proj_dir, &window, "", Some(&task_id)).await;
    if npm_install_outcome == NpmInstallOutcome::OfflinePaused {
        debug_log_to_file(format!(
            "[Orchestrator] Pausing project '{}' ({}) at install-deps due to offline/network error.",
            name, project_id
        ));
        checkpoint_state["status"] = serde_json::json!("paused");
        checkpoint_state["phase"] = serde_json::json!("install-deps");
        checkpoint_state["offlineWaiting"] = serde_json::json!(true);
        checkpoint_state["pausedReason"] = serde_json::json!("offline_npm_install");
        checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
        save_generation_checkpoint(&proj_dir, &checkpoint_state);

        let _ = project_update_status(
            project_id.clone(),
            "paused".to_string(),
            serde_json::json!({
                "offlineWaiting": true,
                "pausedReason": "offline_npm_install"
            }),
        );

        let _ = window.app_handle().emit("task:progress", serde_json::json!({
            "taskId": task_id,
            "projectId": project_id,
            "phase": "paused",
            "status": "paused",
            "offlineWaiting": true,
            "pausedReason": "offline_npm_install",
            "message": "Nema internet konekcije — nastavljamo sa instalacijom paketa kada budemo ponovo online.",
            "progress": 75.0
        }));

        unregister_task_control(&task_id, &project_id);
        return Ok(());
    }
    let npm_install_ok = npm_install_outcome == NpmInstallOutcome::Installed;

    // AST Sanity Pass: Clean invalid React imports (e.g. useRouter in 'react'), hoist client directives across src/
    apply_ast_sanity_pass_to_project(&proj_dir);

    // Auto-run prisma generate & db push if Prisma schema exists in the project
    let mut prisma_heal_err: Option<String> = None;
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        if let Err(e) = heal_prisma_schema_loop(&proj_dir, &window, &model_id, "install-deps").await {
            debug_log_to_file(format!("[Prisma Critical Error] Orchestrator schema healing failed: {}", e));
            prisma_heal_err = Some(e);
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

    // Ensure barrel index.ts files exist and normalize route links BEFORE pre-build hygiene & compiler healing
    ensure_directory_barrel_files(&proj_dir);
    let pre_routes = get_project_routes_from_dir(&proj_dir);
    let pre_routes_list: Vec<String> = pre_routes.into_iter().map(|(r, _)| r).collect();
    if !pre_routes_list.is_empty() {
        normalize_project_route_links(&proj_dir, &pre_routes_list);
    }

    // Run pre-build hygiene pass (tsconfig, package.json, bare imports, CSS dedup, UI primitives, App Router route exports)
    crate::self_healing::run_hygiene_pass_on_project(&proj_dir);

    // Refresh structural graphs & cross-file contracts BEFORE compiler self-healing so healer gets complete disk state
    if let Err(e) = generate_axiom_map(&proj_dir) {
        debug_log_to_file(format!("[Axiom Map] Greška pri pre-heal kreiranju axiom-map.json: {}", e));
    }
    generate_project_metadata(&proj_dir, &project_id, &files_to_generate);
    generate_axiom_features_json(&proj_dir, name, &files_to_generate);
    refresh_axiom_contracts_for_project(&proj_dir, &project_id, &files_to_generate);

    // Run self-healing compiler loop to automatically correct any compilation or syntax errors
    let heal_warnings = perform_self_healing_loop(&proj_dir, &model_id, &files_to_generate).await;

    // Run automated post-generation static, import, hook, and Prisma checks
    let auto_test_warnings = crate::self_healing::run_generation_auto_test_suite(&proj_dir, &window, &model_id).await;

    // Refresh route links and write final up-to-date project graphs & contracts after healing
    let post_routes = get_project_routes_from_dir(&proj_dir);
    let post_routes_list: Vec<String> = post_routes.into_iter().map(|(r, _)| r).collect();
    if !post_routes_list.is_empty() {
        normalize_project_route_links(&proj_dir, &post_routes_list);
    }

    if let Err(e) = generate_axiom_map(&proj_dir) {
        debug_log_to_file(format!("[Axiom Map] Greška pri kreiranju axiom-map.json: {}", e));
    }
    generate_project_metadata(&proj_dir, &project_id, &files_to_generate);
    generate_axiom_features_json(&proj_dir, name, &files_to_generate);
    refresh_axiom_contracts_for_project(&proj_dir, &project_id, &files_to_generate);

    // Assemble honest completion warnings BEFORE persisting final state and emitting completion event
    let mut completion_warnings: Vec<String> = Vec::new();
    if !npm_install_ok && proj_dir.join("package.json").exists() {
        completion_warnings.push("npm install failed (check logs)".to_string());
    }
    if let Some(ref p_err) = prisma_heal_err {
        completion_warnings.push(format!("Prisma validation/generation failed ({})", p_err));
    }
    for w in &heal_warnings {
        if !completion_warnings.iter().any(|cw| cw == w || cw.contains(w)) {
            completion_warnings.push(w.clone());
        }
    }
    for w in &auto_test_warnings {
        if !completion_warnings.iter().any(|cw| cw.contains(w)) {
            completion_warnings.push(w.clone());
        }
    }
    let has_warnings = !completion_warnings.is_empty();

    // 4. Save Project State
    emit_progress("save-project", "Saving project files...", 80.0);

    let new_proj = serde_json::json!({
        "id": project_id,
        "name": name,
        "description": project_description,
        "status": "ready",
        "metadata": {
            "appVersion": env!("CARGO_PKG_VERSION"),
            "modelId": model_id,
            "offlineWaiting": false,
            "hasWarnings": has_warnings,
            "warnings": completion_warnings
        },
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    });
    let _ = project_save(new_proj);

    checkpoint_state["status"] = serde_json::json!("completed");
    checkpoint_state["phase"] = serde_json::json!("complete");
    checkpoint_state["offlineWaiting"] = serde_json::json!(false);
    checkpoint_state["hasWarnings"] = serde_json::json!(has_warnings);
    checkpoint_state["warnings"] = serde_json::json!(completion_warnings);
    if let Some(obj) = checkpoint_state.as_object_mut() {
        obj.remove("pausedReason");
    }
    checkpoint_state["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    save_generation_checkpoint(&proj_dir, &checkpoint_state);
    unregister_task_control(&task_id, &project_id);

    let final_completion_msg = if completion_warnings.is_empty() {
        "Generation completed successfully!".to_string()
    } else {
        format!(
            "Project generated with warnings: {}.",
            completion_warnings.join("; ")
        )
    };
    debug_log_to_file(format!(
        "[Orchestrator Progress] Emit final complete - hasWarnings={}, message={}",
        has_warnings, final_completion_msg
    ));
    let _ = window.app_handle().emit("task:progress", serde_json::json!({
        "taskId": task_id,
        "projectId": project_id,
        "phase": "complete",
        "status": "completed",
        "offlineWaiting": false,
        "hasWarnings": has_warnings,
        "warnings": completion_warnings,
        "message": final_completion_msg,
        "progress": 100.0
    }));

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
            let prev_phase = cp.get("phase").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let completed_len = cp.get("completedFiles").and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
            let total_len = cp.get("totalFiles").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
            let resume_phase = if prev_phase == "install-deps" {
                "install-deps"
            } else {
                "generate-files"
            };
            let pct = if prev_phase == "install-deps" {
                75.0
            } else {
                (25.0 + (completed_len as f64 / total_len as f64) * 55.0).min(99.0)
            };
            let resume_msg = if prev_phase == "install-deps" {
                "Internet konekcija aktivna — nastavljam instalaciju zavisnosti i završne provere...".to_string()
            } else {
                format!("Nastavljam generisanje ({}/{} fajlova)...", completed_len, total_len)
            };
            cp["taskId"] = serde_json::json!(task_id);
            cp["status"] = serde_json::json!("generating");
            cp["phase"] = serde_json::json!(resume_phase);
            cp["offlineWaiting"] = serde_json::json!(false);
            if let Some(obj) = cp.as_object_mut() {
                obj.remove("pausedReason");
            }
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
            let _ = project_update_status(
                p_id.clone(),
                "generating".to_string(),
                serde_json::json!({
                    "offlineWaiting": false,
                    "pausedReason": serde_json::Value::Null
                }),
            );
            let _ = window.app_handle().emit("task:progress", serde_json::json!({
                "taskId": task_id,
                "projectId": p_id,
                "phase": resume_phase,
                "status": "generating",
                "offlineWaiting": false,
                "message": resume_msg,
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
        let proj_dir = get_project_path(&task_id);
        if let Some(mut cp) = load_generation_checkpoint(&proj_dir) {
            cp["status"] = serde_json::json!("paused");
            cp["phase"] = serde_json::json!("paused");
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
            let _ = project_update_status(task_id, "paused".to_string(), serde_json::json!({}));
        }
        Ok(())
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
            cp["offlineWaiting"] = serde_json::json!(false);
            if let Some(obj) = cp.as_object_mut() {
                obj.remove("pausedReason");
            }
            cp["updatedAt"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
            save_generation_checkpoint(&proj_dir, &cp);
            let _ = project_update_status(
                task_id,
                "stopped".to_string(),
                serde_json::json!({
                    "offlineWaiting": false,
                    "pausedReason": serde_json::Value::Null
                }),
            );
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

