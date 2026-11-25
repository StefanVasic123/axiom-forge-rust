use crate::hardware::{estimate_model_size_gb, get_all_models, get_cached_hardware_specs};
use crate::project::get_project_path;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, State};

const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
const MIN_COPILOT_PARAMS_B: f64 = 22.0;
const MAX_READ_LINES: usize = 250;
const MAX_LIST_ENTRIES: usize = 300;
const MAX_SEARCH_MATCHES: usize = 50;
const MAX_CHECK_LINES: usize = 80;
const MAX_FILE_READ_BYTES: u64 = 512 * 1024;
const MAX_STEPS_ASK: usize = 8;
const MAX_STEPS_AGENT: usize = 16;

// ==================== PUBLIC TYPES & STATE ====================

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CopilotCapability {
    pub eligible: bool,
    pub model_id: String,
    pub detected_params_b: Option<f64>,
    pub is_large_model: bool,
    pub supports_native_tools: bool,
    pub supports_thinking: bool,
    pub recommended_num_ctx: u32,
    pub status_code: String, // "eligible" | "too_small" | "missing_tools" | "ollama_offline"
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StagedChange {
    pub path: String,
    pub action: String, // "modify" | "create" | "delete"
    pub original_content: Option<String>,
    pub proposed_content: Option<String>,
    pub summary: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CopilotTurnResult {
    pub success: bool,
    pub final_message: String,
    pub thinking: String,
    pub steps_used: usize,
    pub cancelled: bool,
    pub staged_changes: Vec<StagedChange>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CopilotSession {
    pub messages: Vec<Value>,
    pub staged_files: HashMap<String, StagedChange>,
}

#[derive(Default)]
pub struct CopilotState {
    pub sessions: Mutex<HashMap<String, CopilotSession>>,
    pub cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl CopilotState {
    fn get_or_create_cancel_flag(&self, project_id: &str) -> Arc<AtomicBool> {
        let mut map = self.cancels.lock().unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        map.insert(project_id.to_string(), Arc::clone(&flag));
        flag
    }

    pub fn cancel_project(&self, project_id: &str) {
        let map = self.cancels.lock().unwrap();
        if let Some(flag) = map.get(project_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    pub fn get_staged_list(&self, project_id: &str) -> Vec<StagedChange> {
        let sessions = self.sessions.lock().unwrap();
        let mut list: Vec<StagedChange> = sessions
            .get(project_id)
            .map(|s| s.staged_files.values().cloned().collect())
            .unwrap_or_default();
        list.sort_by(|a, b| a.path.cmp(&b.path));
        list
    }
}

// ==================== MODEL ELIGIBILITY & RUNTIME PROFILE ====================

pub fn parse_param_size_billions(raw: &str) -> Option<f64> {
    let lower = raw.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    // Match patterns like "32.8b", ":32b", "e4b", "26b", "70b", "671b"
    let re = Regex::new(r"(?:^|[:\-_\s/e])(\d+(?:\.\d+)?)\s*b(?:$|[^a-z0-9])").ok()?;
    let mut best: Option<f64> = None;
    for cap in re.captures_iter(&lower) {
        if let Some(m) = cap.get(1) {
            if let Ok(val) = m.as_str().parse::<f64>() {
                best = Some(best.map_or(val, |prev| prev.max(val)));
            }
        }
    }
    best
}

pub fn is_catalog_large_model(model_id: &str) -> bool {
    let lower = model_id.trim().to_lowercase();
    for m in get_all_models() {
        if m.id.to_lowercase() == lower {
            return matches!(m.tier.as_str(), "power" | "expert" | "extreme");
        }
    }
    false
}

pub fn compute_copilot_num_ctx(model_id: &str) -> u32 {
    let specs = get_cached_hardware_specs();
    let model_size_gb = estimate_model_size_gb(model_id);
    let (has_gpu, vram_gb) = match &specs.gpu {
        Some(gpu) if matches!(gpu.brand.as_str(), "Nvidia" | "AMD" | "Apple") => (true, gpu.vram_gb),
        _ => (false, 0.0),
    };
    if !has_gpu {
        return 12288;
    }
    let headroom = vram_gb - model_size_gb;
    if headroom >= 16.0 {
        65536
    } else if headroom >= 6.0 {
        32768
    } else if headroom >= 1.0 {
        16384
    } else {
        12288
    }
}

fn evaluate_show_response(model_id: &str, show_json: &Value) -> CopilotCapability {
    let details_param = show_json
        .get("details")
        .and_then(|d| d.get("parameter_size"))
        .and_then(|p| p.as_str())
        .and_then(parse_param_size_billions);

    let detected_params_b = details_param.or_else(|| parse_param_size_billions(model_id));
    let is_large_model = is_catalog_large_model(model_id)
        || detected_params_b
            .map(|b| b >= MIN_COPILOT_PARAMS_B)
            .unwrap_or(false);

    let caps: Vec<String> = show_json
        .get("capabilities")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    let template_str = show_json
        .get("template")
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let supports_native_tools = if !caps.is_empty() {
        caps.iter().any(|c| c == "tools")
    } else {
        template_str.contains(".Tools") || template_str.contains("tool_call")
    };

    let supports_thinking = caps.iter().any(|c| c == "thinking");
    let recommended_num_ctx = compute_copilot_num_ctx(model_id);
    let eligible = is_large_model && supports_native_tools;

    let (status_code, message) = if eligible {
        let param_label = detected_params_b
            .map(|b| format!("{:.0}B", b))
            .unwrap_or_else(|| "Large".to_string());
        (
            "eligible".to_string(),
            format!(
                "AI Copilot unlocked ({} · native tools · {}K ctx)",
                param_label,
                recommended_num_ctx / 1024
            ),
        )
    } else if is_large_model && !supports_native_tools {
        (
            "missing_tools".to_string(),
            format!(
                "AI Copilot is hidden: '{}' does not advertise native Ollama tool-calling support.",
                model_id
            ),
        )
    } else {
        (
            "too_small".to_string(),
            format!(
                "AI Copilot appears automatically for large models (>= {}B) with native tool-calling; '{}' uses the optimized small-model Edit panel.",
                MIN_COPILOT_PARAMS_B as u32, model_id
            ),
        )
    };

    CopilotCapability {
        eligible,
        model_id: model_id.to_string(),
        detected_params_b,
        is_large_model,
        supports_native_tools,
        supports_thinking,
        recommended_num_ctx,
        status_code,
        message,
    }
}

pub async fn probe_copilot_capability_at(ollama_base_url: &str, model_id: &str) -> CopilotCapability {
    let trimmed = model_id.trim();
    let fallback_params = parse_param_size_billions(trimmed);
    let fallback_large = is_catalog_large_model(trimmed)
        || fallback_params
            .map(|b| b >= MIN_COPILOT_PARAMS_B)
            .unwrap_or(false);
    let recommended_num_ctx = compute_copilot_num_ctx(trimmed);

    // Fast path: clearly small models never unlock Copilot even if Ollama is offline.
    if !fallback_large && fallback_params.is_some() {
        return CopilotCapability {
            eligible: false,
            model_id: trimmed.to_string(),
            detected_params_b: fallback_params,
            is_large_model: false,
            supports_native_tools: false,
            supports_thinking: false,
            recommended_num_ctx,
            status_code: "too_small".to_string(),
            message: format!(
                "AI Copilot appears automatically for large models (>= {}B) with native tool-calling; '{}' uses the optimized small-model Edit panel.",
                MIN_COPILOT_PARAMS_B as u32, trimmed
            ),
        };
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return CopilotCapability {
                eligible: false,
                model_id: trimmed.to_string(),
                detected_params_b: fallback_params,
                is_large_model: fallback_large,
                supports_native_tools: false,
                supports_thinking: false,
                recommended_num_ctx,
                status_code: "ollama_offline".to_string(),
                message: "AI Copilot is hidden: could not initialize HTTP client for Ollama."
                    .to_string(),
            };
        }
    };

    let url = format!("{}/api/show", ollama_base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&json!({ "name": trimmed }))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            if let Ok(body) = r.json::<Value>().await {
                return evaluate_show_response(trimmed, &body);
            }
            CopilotCapability {
                eligible: false,
                model_id: trimmed.to_string(),
                detected_params_b: fallback_params,
                is_large_model: fallback_large,
                supports_native_tools: false,
                supports_thinking: false,
                recommended_num_ctx,
                status_code: "missing_tools".to_string(),
                message: format!(
                    "AI Copilot is hidden: could not inspect capabilities for '{}'.",
                    trimmed
                ),
            }
        }
        _ => CopilotCapability {
            eligible: false,
            model_id: trimmed.to_string(),
            detected_params_b: fallback_params,
            is_large_model: fallback_large,
            supports_native_tools: false,
            supports_thinking: false,
            recommended_num_ctx,
            status_code: if fallback_large {
                "ollama_offline".to_string()
            } else {
                "too_small".to_string()
            },
            message: if fallback_large {
                format!(
                    "AI Copilot is hidden: Ollama is offline or '{}' is not pulled yet, so native tool-calling could not be verified.",
                    trimmed
                )
            } else {
                format!(
                    "AI Copilot appears automatically for large models (>= {}B) with native tool-calling.",
                    MIN_COPILOT_PARAMS_B as u32
                )
            },
        },
    }
}

// ==================== SANDBOXED PATH & OVERLAY HELPERS ====================

pub fn normalize_rel_path(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("Path cannot be empty".to_string());
    }
    if trimmed.contains('\0') {
        return Err("Invalid null byte in path".to_string());
    }
    let stripped = trimmed.trim_start_matches("./");
    let path = Path::new(stripped);
    if path.is_absolute() {
        return Err(format!("Absolute paths are forbidden: {}", raw));
    }
    let mut parts: Vec<String> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Normal(os) => {
                let s = os.to_string_lossy().to_string();
                if !s.is_empty() {
                    parts.push(s);
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Path traversal outside project is forbidden: {}", raw));
            }
        }
    }
    if parts.is_empty() {
        return Err("Path resolves to project root".to_string());
    }
    Ok(parts.join("/"))
}

pub fn is_ignored_dir_name(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | ".next"
            | ".git"
            | "dist"
            | "build"
            | "target"
            | ".turbo"
            | ".expo"
            | ".vercel"
            | "coverage"
    )
}

pub fn is_forbidden_write_path(rel: &str) -> bool {
    let lower = rel.to_lowercase();
    let segments: Vec<&str> = lower.split('/').collect();
    if segments
        .iter()
        .any(|s| matches!(*s, ".git" | "node_modules" | ".next" | "target" | "dist"))
    {
        return true;
    }
    if let Some(fname) = segments.last() {
        if *fname == ".env" || fname.starts_with(".env.") {
            return true;
        }
    }
    false
}

fn is_lock_or_binary_name(rel: &str) -> bool {
    let lower = rel.to_lowercase();
    let fname = lower.rsplit('/').next().unwrap_or(&lower);
    if matches!(
        fname,
        "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "cargo.lock" | "bun.lockb"
    ) {
        return true;
    }
    let ext = fname.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "ico"
            | "pdf"
            | "zip"
            | "tar"
            | "gz"
            | "wasm"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "ttf"
            | "woff"
            | "woff2"
            | "mp4"
            | "mp3"
            | "sqlite"
            | "db"
    )
}

fn resolve_safe_disk_path(proj_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let clean = normalize_rel_path(rel)?;
    let full = proj_dir.join(&clean);
    if full.exists() {
        let canon_root = fs::canonicalize(proj_dir).map_err(|e| e.to_string())?;
        let canon_full = fs::canonicalize(&full).map_err(|e| e.to_string())?;
        if !canon_full.starts_with(&canon_root) {
            return Err(format!("Symlink/path escape blocked: {}", rel));
        }
    }
    Ok(full)
}

fn read_disk_file_checked(proj_dir: &Path, rel: &str) -> Result<Option<String>, String> {
    if is_lock_or_binary_name(rel) {
        return Err(format!("Binary or lockfile '{}' cannot be read as source code", rel));
    }
    let full = resolve_safe_disk_path(proj_dir, rel)?;
    if !full.exists() {
        return Ok(None);
    }
    if full.is_dir() {
        return Err(format!("'{}' is a directory, not a file", rel));
    }
    let meta = fs::metadata(&full).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_READ_BYTES {
        return Err(format!(
            "File '{}' is too large ({} bytes, max {} bytes)",
            rel,
            meta.len(),
            MAX_FILE_READ_BYTES
        ));
    }
    let bytes = fs::read(&full).map_err(|e| e.to_string())?;
    if bytes.iter().take(4096).any(|b| *b == 0) {
        return Err(format!("File '{}' appears to be binary", rel));
    }
    Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
}

pub fn read_effective_file(
    proj_dir: &Path,
    staged: &HashMap<String, StagedChange>,
    rel: &str,
) -> Result<Option<String>, String> {
    let clean = normalize_rel_path(rel)?;
    if let Some(entry) = staged.get(&clean) {
        if entry.action == "delete" {
            return Ok(None);
        }
        return Ok(entry.proposed_content.clone());
    }
    read_disk_file_checked(proj_dir, &clean)
}

fn collect_project_files(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    if depth > max_depth || out.len() >= MAX_LIST_ENTRIES {
        return;
    }
    let mut entries: Vec<fs::DirEntry> = match fs::read_dir(current) {
        Ok(rd) => rd.flatten().collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if out.len() >= MAX_LIST_ENTRIES {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if is_ignored_dir_name(&name) {
                continue;
            }
            collect_project_files(root, &path, depth + 1, max_depth, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

// ==================== TOOL SCHEMAS & EXECUTION ====================

pub fn build_copilot_tools(mode: &str) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List project files (respecting staged overlay changes). Ignores node_modules, .next, .git, dist, target.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "dir": { "type": "string", "description": "Optional relative subdirectory to list, e.g. 'src/app'" },
                        "max_depth": { "type": "integer", "description": "Optional max recursion depth (default 6)" }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read numbered lines of a project file (reads staged virtual overlay if modified in this session). Max 250 lines per call.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path inside project, e.g. 'src/app/page.tsx'" },
                        "start_line": { "type": "integer", "description": "1-based start line (default 1)" },
                        "end_line": { "type": "integer", "description": "1-based inclusive end line (default start_line + 249)" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "search_code",
                "description": "Search for a text query across project source files (including staged virtual overlay changes). Returns up to 50 matches with path and line number.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Substring to search for" },
                        "path_prefix": { "type": "string", "description": "Optional relative directory/file prefix to restrict search" },
                        "case_sensitive": { "type": "boolean", "description": "Default false" }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "get_project_overview",
                "description": "Return a concise overview of package.json, prisma/schema.prisma (if present), and axiom-features.json index (if present).",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_check",
                "description": "Run an allowlisted project verification command against current disk files: 'tsc', 'prisma_validate', 'lint', or 'build'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["tsc", "prisma_validate", "lint", "build"],
                            "description": "Which check to run"
                        }
                    },
                    "required": ["kind"]
                }
            }
        }),
    ];

    if mode == "agent" {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Stage a targeted edit to an existing file in the virtual overlay (does NOT write to disk until the user reviews Diff and clicks Apply). Requires old_text to match exactly once.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative file path" },
                        "old_text": { "type": "string", "description": "Exact existing snippet to replace (must match uniquely)" },
                        "new_text": { "type": "string", "description": "Replacement snippet" },
                        "summary": { "type": "string", "description": "Brief 1-line summary of the change" }
                    },
                    "required": ["path", "old_text", "new_text"]
                }
            }
        }));
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "create_file",
                "description": "Stage creation of a new file in the virtual overlay (does NOT write to disk until user clicks Apply).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path for the new file" },
                        "content": { "type": "string", "description": "Full file contents" },
                        "overwrite": { "type": "boolean", "description": "Set true to overwrite an existing file (default false)" },
                        "summary": { "type": "string", "description": "Brief 1-line summary" }
                    },
                    "required": ["path", "content"]
                }
            }
        }));
        tools.push(json!({
            "type": "function",
            "function": {
                "name": "delete_file",
                "description": "Stage deletion of a file in the virtual overlay (does NOT delete from disk until user confirms Apply).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path to delete" },
                        "summary": { "type": "string", "description": "Brief reason for deletion" }
                    },
                    "required": ["path"]
                }
            }
        }));
    }

    tools
}

fn truncate_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    let head = lines[..max_lines].join("\n");
    format!(
        "{}\n... [truncated {} additional lines]",
        head,
        lines.len() - max_lines
    )
}

async fn run_allowlisted_check(proj_dir: &Path, kind: &str, staged_count: usize) -> String {
    let (program, args): (&str, Vec<&str>) = match kind {
        "tsc" => {
            #[cfg(target_os = "windows")]
            {
                ("cmd", vec!["/C", "npx", "--no-install", "tsc", "--noEmit"])
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("npx", vec!["--no-install", "tsc", "--noEmit"])
            }
        }
        "prisma_validate" => {
            #[cfg(target_os = "windows")]
            {
                ("cmd", vec!["/C", "npx", "--no-install", "prisma", "validate"])
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("npx", vec!["--no-install", "prisma", "validate"])
            }
        }
        "lint" => {
            #[cfg(target_os = "windows")]
            {
                ("cmd", vec!["/C", "npm", "run", "lint"])
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("npm", vec!["run", "lint"])
            }
        }
        "build" => {
            #[cfg(target_os = "windows")]
            {
                ("cmd", vec!["/C", "npm", "run", "build"])
            }
            #[cfg(not(target_os = "windows"))]
            {
                ("npm", vec!["run", "build"])
            }
        }
        other => {
            return format!(
                "Error: unsupported check kind '{}'. Allowed: tsc, prisma_validate, lint, build.",
                other
            );
        }
    };

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(&args).current_dir(proj_dir);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    let output = match tokio::time::timeout(Duration::from_secs(90), cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return format!("Error running '{}': {}", kind, e),
        Err(_) => return format!("Error: check '{}' timed out after 90s.", kind),
    };

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout.trim(), stderr.trim()).trim().to_string();
    let snippet = if combined.is_empty() {
        "(no output)".to_string()
    } else {
        truncate_lines(&combined, MAX_CHECK_LINES)
    };
    let overlay_note = if staged_count > 0 {
        format!(
            "\n[Note: {} staged edit(s) are currently only in the virtual overlay and not yet applied to disk; this check inspected current disk files.]",
            staged_count
        )
    } else {
        String::new()
    };

    format!("check '{}' exit_code={}\n{}{}", kind, code, snippet, overlay_note)
}

pub async fn execute_copilot_tool(
    proj_dir: &Path,
    mode: &str,
    staged: &mut HashMap<String, StagedChange>,
    name: &str,
    args: &Value,
) -> (String, bool) {
    // Returns (tool_output_text, staged_changed)
    match name {
        "list_files" => {
            let max_depth = args
                .get("max_depth")
                .and_then(|v| v.as_u64())
                .map(|d| (d as usize).clamp(1, 10))
                .unwrap_or(6);
            let sub_dir = args
                .get("dir")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && *s != "." && *s != "./");

            let start_dir = if let Some(rel) = sub_dir {
                match resolve_safe_disk_path(proj_dir, rel) {
                    Ok(p) if p.is_dir() => p,
                    Ok(_) => return (format!("Error: '{}' is not an existing directory", rel), false),
                    Err(e) => return (format!("Error: {}", e), false),
                }
            } else {
                proj_dir.to_path_buf()
            };

            let mut files = Vec::new();
            collect_project_files(proj_dir, &start_dir, 0, max_depth, &mut files);

            // Overlay adjustments: remove deleted, add created
            let prefix_norm = sub_dir
                .and_then(|d| normalize_rel_path(d).ok())
                .map(|d| format!("{}/", d));
            files.retain(|f| {
                !matches!(staged.get(f), Some(sc) if sc.action == "delete")
            });
            for (path, sc) in staged.iter() {
                if sc.action == "create" && !files.contains(path) {
                    let matches_prefix = prefix_norm
                        .as_ref()
                        .map(|p| path.starts_with(p))
                        .unwrap_or(true);
                    if matches_prefix && files.len() < MAX_LIST_ENTRIES {
                        files.push(path.clone());
                    }
                }
            }
            files.sort();

            let formatted: Vec<String> = files
                .iter()
                .map(|f| {
                    if let Some(sc) = staged.get(f) {
                        format!("{} [staged:{}]", f, sc.action)
                    } else {
                        f.clone()
                    }
                })
                .collect();

            if formatted.is_empty() {
                ("(no files found)".to_string(), false)
            } else {
                let trunc = if formatted.len() >= MAX_LIST_ENTRIES {
                    format!("\n... [capped at {} entries]", MAX_LIST_ENTRIES)
                } else {
                    String::new()
                };
                (format!("{}{}", formatted.join("\n"), trunc), false)
            }
        }

        "read_file" => {
            let rel_raw = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ("Error: missing required parameter 'path'".to_string(), false),
            };
            let clean = match normalize_rel_path(rel_raw) {
                Ok(p) => p,
                Err(e) => return (format!("Error: {}", e), false),
            };
            let content = match read_effective_file(proj_dir, staged, &clean) {
                Ok(Some(c)) => c,
                Ok(None) => return (format!("Error: file '{}' does not exist", clean), false),
                Err(e) => return (format!("Error: {}", e), false),
            };
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let start = args
                .get("start_line")
                .and_then(|v| v.as_u64())
                .map(|v| (v as usize).max(1))
                .unwrap_or(1);
            let requested_end = args
                .get("end_line")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(start + MAX_READ_LINES - 1);
            let end = requested_end.min(start + MAX_READ_LINES - 1).min(total);

            if total == 0 {
                return (format!("File '{}' is empty (0 lines).", clean), false);
            }
            if start > total {
                return (
                    format!(
                        "Error: start_line {} exceeds total line count {} of '{}'.",
                        start, total, clean
                    ),
                    false,
                );
            }
            let staged_tag = staged
                .get(&clean)
                .map(|s| format!(" [staged:{}]", s.action))
                .unwrap_or_default();
            let mut out = format!(
                "File: {}{} (lines {}-{} of {})\n",
                clean, staged_tag, start, end, total
            );
            for idx in (start - 1)..end {
                out.push_str(&format!("L{}: {}\n", idx + 1, lines[idx]));
            }
            if end < total {
                out.push_str(&format!(
                    "... [{} more lines; call read_file with start_line={} to continue]",
                    total - end,
                    end + 1
                ));
            }
            (out, false)
        }

        "search_code" => {
            let query = match args.get("query").and_then(|v| v.as_str()) {
                Some(q) if !q.trim().is_empty() => q,
                _ => return ("Error: 'query' must be a non-empty string".to_string(), false),
            };
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let prefix = args
                .get("path_prefix")
                .and_then(|v| v.as_str())
                .and_then(|p| {
                    let t = p.trim();
                    if t.is_empty() || t == "." {
                        None
                    } else {
                        normalize_rel_path(t).ok()
                    }
                });

            let mut files = Vec::new();
            collect_project_files(proj_dir, proj_dir, 0, 8, &mut files);
            for (path, sc) in staged.iter() {
                if sc.action == "create" && !files.contains(path) {
                    files.push(path.clone());
                }
            }
            files.sort();

            let needle = if case_sensitive {
                query.to_string()
            } else {
                query.to_lowercase()
            };

            let mut matches = Vec::new();
            for rel in files {
                if matches.len() >= MAX_SEARCH_MATCHES {
                    break;
                }
                if let Some(ref p) = prefix {
                    if !rel.starts_with(p) {
                        continue;
                    }
                }
                if is_lock_or_binary_name(&rel) {
                    continue;
                }
                let content = match read_effective_file(proj_dir, staged, &rel) {
                    Ok(Some(c)) => c,
                    _ => continue,
                };
                for (idx, line) in content.lines().enumerate() {
                    let hay = if case_sensitive {
                        line.to_string()
                    } else {
                        line.to_lowercase()
                    };
                    if hay.contains(&needle) {
                        let clipped: String = line.trim().chars().take(160).collect();
                        matches.push(format!("{}:{}: {}", rel, idx + 1, clipped));
                        if matches.len() >= MAX_SEARCH_MATCHES {
                            break;
                        }
                    }
                }
            }

            if matches.is_empty() {
                (format!("No matches found for '{}'.", query), false)
            } else {
                let cap_note = if matches.len() >= MAX_SEARCH_MATCHES {
                    format!("\n... [capped at {} matches]", MAX_SEARCH_MATCHES)
                } else {
                    String::new()
                };
                (format!("{}{}", matches.join("\n"), cap_note), false)
            }
        }

        "get_project_overview" => {
            let mut parts = Vec::new();
            if let Ok(Some(pkg)) = read_effective_file(proj_dir, staged, "package.json") {
                parts.push(format!("=== package.json ===\n{}", truncate_lines(&pkg, 80)));
            }
            if let Ok(Some(schema)) = read_effective_file(proj_dir, staged, "prisma/schema.prisma") {
                parts.push(format!(
                    "=== prisma/schema.prisma ===\n{}",
                    truncate_lines(&schema, 120)
                ));
            }
            if let Ok(Some(features)) = read_effective_file(proj_dir, staged, "axiom-features.json") {
                let clipped: String = features.chars().take(3000).collect();
                parts.push(format!("=== axiom-features.json (excerpt) ===\n{}", clipped));
            }
            if parts.is_empty() {
                ("No package.json, prisma/schema.prisma, or axiom-features.json found.".to_string(), false)
            } else {
                (parts.join("\n\n"), false)
            }
        }

        "run_check" => {
            let kind = args
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("tsc");
            let res = run_allowlisted_check(proj_dir, kind, staged.len()).await;
            (res, false)
        }

        "edit_file" => {
            if mode != "agent" {
                return ("Error: edit_file is only available in Agent mode (currently Ask/Read-Only).".to_string(), false);
            }
            let rel_raw = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ("Error: missing 'path'".to_string(), false),
            };
            let old_text = match args.get("old_text").and_then(|v| v.as_str()) {
                Some(t) if !t.is_empty() => t,
                _ => return ("Error: 'old_text' must be a non-empty string".to_string(), false),
            };
            let new_text = match args.get("new_text").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => return ("Error: missing 'new_text'".to_string(), false),
            };
            let summary = args
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("Staged edit")
                .to_string();

            let clean = match normalize_rel_path(rel_raw) {
                Ok(p) => p,
                Err(e) => return (format!("Error: {}", e), false),
            };
            if is_forbidden_write_path(&clean) {
                return (
                    format!("Error: editing protected path '{}' is forbidden", clean),
                    false,
                );
            }

            let current = match read_effective_file(proj_dir, staged, &clean) {
                Ok(Some(c)) => c,
                Ok(None) => return (format!("Error: file '{}' does not exist (use create_file for new files)", clean), false),
                Err(e) => return (format!("Error: {}", e), false),
            };

            let count = current.matches(old_text).count();
            if count == 0 {
                return (
                    format!(
                        "Error: 'old_text' was not found in '{}'. Call read_file to inspect the exact current text before retrying.",
                        clean
                    ),
                    false,
                );
            }
            if count > 1 {
                return (
                    format!(
                        "Error: 'old_text' matched {} times in '{}'. Include more surrounding context lines so it matches uniquely.",
                        count, clean
                    ),
                    false,
                );
            }

            let updated = current.replacen(old_text, new_text, 1);
            if updated == current {
                return (
                    format!("No-op: 'new_text' is identical to 'old_text' in '{}'.", clean),
                    false,
                );
            }

            let disk_original = read_disk_file_checked(proj_dir, &clean).ok().flatten();
            let action = if disk_original.is_some() { "modify" } else { "create" };
            staged.insert(
                clean.clone(),
                StagedChange {
                    path: clean.clone(),
                    action: action.to_string(),
                    original_content: disk_original,
                    proposed_content: Some(updated),
                    summary,
                },
            );
            (
                format!(
                    "Staged edit for '{}' in virtual overlay (not written to disk yet; user will review Diff and click Apply).",
                    clean
                ),
                true,
            )
        }

        "create_file" => {
            if mode != "agent" {
                return ("Error: create_file is only available in Agent mode (currently Ask/Read-Only).".to_string(), false);
            }
            let rel_raw = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ("Error: missing 'path'".to_string(), false),
            };
            let content = match args.get("content").and_then(|v| v.as_str()) {
                Some(c) => c.to_string(),
                None => return ("Error: missing 'content'".to_string(), false),
            };
            let overwrite = args
                .get("overwrite")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let summary = args
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("Staged new file")
                .to_string();

            let clean = match normalize_rel_path(rel_raw) {
                Ok(p) => p,
                Err(e) => return (format!("Error: {}", e), false),
            };
            if is_forbidden_write_path(&clean) {
                return (
                    format!("Error: creating protected path '{}' is forbidden", clean),
                    false,
                );
            }

            let existing = read_effective_file(proj_dir, staged, &clean).ok().flatten();
            if existing.is_some() && !overwrite {
                return (
                    format!(
                        "Error: '{}' already exists. Use edit_file for targeted edits or pass overwrite=true.",
                        clean
                    ),
                    false,
                );
            }
            let disk_original = read_disk_file_checked(proj_dir, &clean).ok().flatten();
            let action = if disk_original.is_some() { "modify" } else { "create" };
            staged.insert(
                clean.clone(),
                StagedChange {
                    path: clean.clone(),
                    action: action.to_string(),
                    original_content: disk_original,
                    proposed_content: Some(content),
                    summary,
                },
            );
            (
                format!(
                    "Staged {} for '{}' in virtual overlay (pending user Apply).",
                    action, clean
                ),
                true,
            )
        }

        "delete_file" => {
            if mode != "agent" {
                return ("Error: delete_file is only available in Agent mode (currently Ask/Read-Only).".to_string(), false);
            }
            let rel_raw = match args.get("path").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return ("Error: missing 'path'".to_string(), false),
            };
            let summary = args
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("Staged deletion")
                .to_string();

            let clean = match normalize_rel_path(rel_raw) {
                Ok(p) => p,
                Err(e) => return (format!("Error: {}", e), false),
            };
            if is_forbidden_write_path(&clean) {
                return (
                    format!("Error: deleting protected path '{}' is forbidden", clean),
                    false,
                );
            }

            let disk_original = match read_disk_file_checked(proj_dir, &clean) {
                Ok(opt) => opt,
                Err(e) => return (format!("Error: {}", e), false),
            };
            if disk_original.is_none() {
                if staged.remove(&clean).is_some() {
                    return (
                        format!("Removed previously staged new file '{}' from overlay.", clean),
                        true,
                    );
                }
                return (format!("Error: file '{}' does not exist", clean), false);
            }

            staged.insert(
                clean.clone(),
                StagedChange {
                    path: clean.clone(),
                    action: "delete".to_string(),
                    original_content: disk_original,
                    proposed_content: None,
                    summary,
                },
            );
            (
                format!(
                    "Staged deletion for '{}' in virtual overlay (pending user confirmation).",
                    clean
                ),
                true,
            )
        }

        other => (format!("Error: unknown tool '{}'", other), false),
    }
}

// ==================== CONTEXT PRUNING & SYSTEM PROMPT ====================

fn estimate_messages_tokens(messages: &[Value]) -> usize {
    let chars: usize = messages
        .iter()
        .map(|m| m.to_string().len())
        .sum();
    chars / 4
}

pub fn prune_old_tool_messages(messages: &mut Vec<Value>, num_ctx: u32) {
    let budget_tokens = ((num_ctx as f64) * 0.75) as usize;
    if estimate_messages_tokens(messages) <= budget_tokens {
        return;
    }

    // Preserve the last 6 messages untouched (recent turn + tool results)
    let protected_tail = 6usize;
    if messages.len() <= protected_tail + 1 {
        return;
    }
    let cutoff = messages.len() - protected_tail;

    for msg in messages.iter_mut().take(cutoff).skip(1) {
        if estimate_messages_tokens(&[]) >= budget_tokens {
            break;
        }
        let is_tool = msg.get("role").and_then(|r| r.as_str()) == Some("tool");
        if !is_tool {
            continue;
        }
        let content_len = msg
            .get("content")
            .and_then(|c| c.as_str())
            .map(|s| s.len())
            .unwrap_or(0);
        if content_len > 300 {
            let tool_name = msg
                .get("tool_name")
                .and_then(|n| n.as_str())
                .unwrap_or("tool");
            let first_line = msg
                .get("content")
                .and_then(|c| c.as_str())
                .and_then(|s| s.lines().next())
                .unwrap_or("")
                .chars()
                .take(100)
                .collect::<String>();
            msg["content"] = json!(format!(
                "[Pruned earlier {} output ({} chars): '{}' — call tool again if needed]",
                tool_name, content_len, first_line
            ));
        }
    }
}

fn build_copilot_system_prompt(mode: &str, active_file: Option<&str>) -> String {
    let mode_desc = if mode == "agent" {
        "Mode: AGENT (Virtual Overlay). You have read-only inspection tools plus edit_file, create_file, and delete_file. All edits are staged in a safe virtual overlay and will NOT touch disk until the user reviews the diff and clicks Apply."
    } else {
        "Mode: ASK (Read-Only Analysis). You have read-only inspection tools only. Diagnose issues, explain architecture, and answer questions accurately without modifying files."
    };

    let active_hint = active_file
        .map(|f| format!("\nUser's currently open editor file: `{}` (call read_file if you need its contents).", f))
        .unwrap_or_default();

    format!(
        "You are Axiom Forge AI Copilot, an independent senior software engineer working directly on this codebase.\n\
         {}\n\
         Working rules:\n\
         - Always inspect actual files with your tools before making claims about bugs, types, APIs, or architecture.\n\
         - Cite exact relative file paths and line numbers when reporting findings.\n\
         - Never invent file paths, symbols, or command outputs you have not observed.\n\
         - When staging edits in Agent mode, keep edits minimal, clean, and faithful to the existing codebase style.{}\n",
        mode_desc, active_hint
    )
}

// ==================== MULTI-TURN AGENT LOOP ====================

pub async fn run_copilot_turn_at<F>(
    ollama_base_url: &str,
    proj_dir: &Path,
    model_id: &str,
    mode: &str,
    num_ctx: u32,
    user_prompt: &str,
    active_file: Option<&str>,
    session: &mut CopilotSession,
    cancel_flag: &Arc<AtomicBool>,
    emit: F,
) -> CopilotTurnResult
where
    F: Fn(Value),
{
    let normalized_mode = if mode == "agent" { "agent" } else { "ask" };
    let max_steps = if normalized_mode == "agent" {
        MAX_STEPS_AGENT
    } else {
        MAX_STEPS_ASK
    };

    // Always keep system prompt fresh for current mode & active file
    let sys_prompt = build_copilot_system_prompt(normalized_mode, active_file);
    if session.messages.is_empty() {
        session.messages.push(json!({
            "role": "system",
            "content": sys_prompt
        }));
    } else if session.messages[0].get("role").and_then(|r| r.as_str()) == Some("system") {
        session.messages[0] = json!({
            "role": "system",
            "content": sys_prompt
        });
    }

    session.messages.push(json!({
        "role": "user",
        "content": user_prompt
    }));

    let tools = build_copilot_tools(normalized_mode);
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CopilotTurnResult {
                success: false,
                final_message: String::new(),
                thinking: String::new(),
                steps_used: 0,
                cancelled: false,
                staged_changes: sorted_staged(&session.staged_files),
                error: Some(format!("HTTP client error: {}", e)),
            };
        }
    };

    let chat_url = format!("{}/api/chat", ollama_base_url.trim_end_matches('/'));
    let mut final_message = String::new();
    let mut all_thinking = String::new();
    let mut steps_used = 0usize;

    for step in 1..=max_steps {
        if cancel_flag.load(Ordering::SeqCst) {
            emit(json!({ "type": "cancelled" }));
            return CopilotTurnResult {
                success: false,
                final_message,
                thinking: all_thinking,
                steps_used,
                cancelled: true,
                staged_changes: sorted_staged(&session.staged_files),
                error: Some("Cancelled by user".to_string()),
            };
        }

        steps_used = step;
        prune_old_tool_messages(&mut session.messages, num_ctx);

        let payload = json!({
            "model": model_id,
            "stream": true,
            "messages": session.messages,
            "tools": tools,
            "options": {
                "temperature": 0.2,
                "num_ctx": num_ctx,
                "num_predict": 4096
            }
        });

        let mut resp = match client.post(&chat_url).json(&payload).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                let err_msg = format!("Ollama HTTP {}: {}", status, body.chars().take(300).collect::<String>());
                emit(json!({ "type": "error", "error": err_msg }));
                return CopilotTurnResult {
                    success: false,
                    final_message,
                    thinking: all_thinking,
                    steps_used,
                    cancelled: false,
                    staged_changes: sorted_staged(&session.staged_files),
                    error: Some(err_msg),
                };
            }
            Err(e) => {
                let err_msg = format!("Ollama connection failed: {}", e);
                emit(json!({ "type": "error", "error": err_msg }));
                return CopilotTurnResult {
                    success: false,
                    final_message,
                    thinking: all_thinking,
                    steps_used,
                    cancelled: false,
                    staged_changes: sorted_staged(&session.staged_files),
                    error: Some(err_msg),
                };
            }
        };

        let mut step_content = String::new();
        let mut step_thinking = String::new();
        let mut step_tool_calls: Vec<Value> = Vec::new();
        let mut line_buf = String::new();

        loop {
            if cancel_flag.load(Ordering::SeqCst) {
                emit(json!({ "type": "cancelled" }));
                return CopilotTurnResult {
                    success: false,
                    final_message: if step_content.is_empty() { final_message } else { step_content },
                    thinking: all_thinking,
                    steps_used,
                    cancelled: true,
                    staged_changes: sorted_staged(&session.staged_files),
                    error: Some("Cancelled by user".to_string()),
                };
            }

            let next_chunk = match resp.chunk().await {
                Ok(Some(c)) => c,
                Ok(None) => break,
                Err(e) => {
                    let err_msg = format!("Stream interrupted: {}", e);
                    emit(json!({ "type": "error", "error": err_msg }));
                    return CopilotTurnResult {
                        success: false,
                        final_message,
                        thinking: all_thinking,
                        steps_used,
                        cancelled: false,
                        staged_changes: sorted_staged(&session.staged_files),
                        error: Some(err_msg),
                    };
                }
            };

            line_buf.push_str(&String::from_utf8_lossy(&next_chunk));
            while let Some(nl_pos) = line_buf.find('\n') {
                let line = line_buf[..nl_pos].trim().to_string();
                line_buf.drain(..=nl_pos);
                if line.is_empty() {
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<Value>(&line) {
                    if let Some(msg) = parsed.get("message") {
                        if let Some(th) = msg.get("thinking").and_then(|v| v.as_str()) {
                            if !th.is_empty() {
                                step_thinking.push_str(th);
                                all_thinking.push_str(th);
                                emit(json!({ "type": "thinking", "content": th }));
                            }
                        }
                        if let Some(tok) = msg.get("content").and_then(|v| v.as_str()) {
                            if !tok.is_empty() {
                                step_content.push_str(tok);
                                emit(json!({ "type": "token", "content": tok }));
                            }
                        }
                        if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                            for tc in tcs {
                                step_tool_calls.push(tc.clone());
                            }
                        }
                    }
                }
            }
        }

        // Flush trailing NDJSON line if no final newline
        let tail = line_buf.trim();
        if !tail.is_empty() {
            if let Ok(parsed) = serde_json::from_str::<Value>(tail) {
                if let Some(msg) = parsed.get("message") {
                    if let Some(th) = msg.get("thinking").and_then(|v| v.as_str()) {
                        if !th.is_empty() {
                            step_thinking.push_str(th);
                            all_thinking.push_str(th);
                            emit(json!({ "type": "thinking", "content": th }));
                        }
                    }
                    if let Some(tok) = msg.get("content").and_then(|v| v.as_str()) {
                        if !tok.is_empty() {
                            step_content.push_str(tok);
                            emit(json!({ "type": "token", "content": tok }));
                        }
                    }
                    if let Some(tcs) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tcs {
                            step_tool_calls.push(tc.clone());
                        }
                    }
                }
            }
        }

        // Record assistant turn
        let mut assistant_msg = json!({
            "role": "assistant",
            "content": step_content
        });
        if !step_tool_calls.is_empty() {
            assistant_msg["tool_calls"] = Value::Array(step_tool_calls.clone());
        }
        session.messages.push(assistant_msg);

        if step_tool_calls.is_empty() {
            final_message = step_content;
            let staged_list = sorted_staged(&session.staged_files);
            emit(json!({
                "type": "done",
                "stepsUsed": steps_used,
                "stagedChanges": staged_list
            }));
            return CopilotTurnResult {
                success: true,
                final_message,
                thinking: all_thinking,
                steps_used,
                cancelled: false,
                staged_changes: staged_list,
                error: None,
            };
        }

        // Execute each requested tool call
        for tc in &step_tool_calls {
            if cancel_flag.load(Ordering::SeqCst) {
                emit(json!({ "type": "cancelled" }));
                return CopilotTurnResult {
                    success: false,
                    final_message,
                    thinking: all_thinking,
                    steps_used,
                    cancelled: true,
                    staged_changes: sorted_staged(&session.staged_files),
                    error: Some("Cancelled by user".to_string()),
                };
            }

            let func = tc.get("function").unwrap_or(&Value::Null);
            let tool_name = func
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");
            let raw_args = func.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let tool_args = if let Some(s) = raw_args.as_str() {
                serde_json::from_str::<Value>(s).unwrap_or_else(|_| json!({}))
            } else {
                raw_args
            };

            emit(json!({
                "type": "tool_start",
                "step": step,
                "name": tool_name,
                "arguments": tool_args
            }));

            let (result_text, staged_changed) = execute_copilot_tool(
                proj_dir,
                normalized_mode,
                &mut session.staged_files,
                tool_name,
                &tool_args,
            )
            .await;

            let preview: String = result_text.chars().take(240).collect();
            emit(json!({
                "type": "tool_result",
                "step": step,
                "name": tool_name,
                "preview": preview,
                "isError": result_text.starts_with("Error:")
            }));

            if staged_changed {
                emit(json!({
                    "type": "staged_updated",
                    "stagedChanges": sorted_staged(&session.staged_files)
                }));
            }

            session.messages.push(json!({
                "role": "tool",
                "tool_name": tool_name,
                "content": result_text
            }));
        }
    }

    let staged_list = sorted_staged(&session.staged_files);
    let limit_msg = format!(
        "Reached maximum tool iteration limit ({} steps). You can ask me to continue.",
        max_steps
    );
    emit(json!({
        "type": "done",
        "stepsUsed": steps_used,
        "stagedChanges": staged_list,
        "note": limit_msg
    }));

    CopilotTurnResult {
        success: true,
        final_message: if final_message.is_empty() { limit_msg } else { final_message },
        thinking: all_thinking,
        steps_used,
        cancelled: false,
        staged_changes: staged_list,
        error: None,
    }
}

fn sorted_staged(map: &HashMap<String, StagedChange>) -> Vec<StagedChange> {
    let mut v: Vec<StagedChange> = map.values().cloned().collect();
    v.sort_by(|a, b| a.path.cmp(&b.path));
    v
}

// ==================== TAURI COMMANDS ====================

#[tauri::command]
pub async fn copilot_get_capability(model_id: String) -> CopilotCapability {
    probe_copilot_capability_at(DEFAULT_OLLAMA_BASE_URL, &model_id).await
}

#[tauri::command]
pub async fn copilot_send_message(
    window: tauri::Window,
    state: State<'_, CopilotState>,
    project_id: String,
    model_id: String,
    mode: String,
    prompt: String,
    active_file: Option<String>,
    num_ctx_override: Option<u32>,
) -> Result<CopilotTurnResult, String> {
    let cap = probe_copilot_capability_at(DEFAULT_OLLAMA_BASE_URL, &model_id).await;
    if !cap.eligible {
        return Err(cap.message);
    }

    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err(format!("Project directory not found for '{}'", project_id));
    }

    let num_ctx = num_ctx_override
        .map(|v| v.clamp(8192, 131072))
        .unwrap_or(cap.recommended_num_ctx);

    let cancel_flag = state.get_or_create_cancel_flag(&project_id);

    // Clone session out of mutex so we never hold a lock across async I/O
    let mut session = {
        let mut sessions = state.sessions.lock().unwrap();
        sessions
            .entry(project_id.clone())
            .or_default()
            .clone()
    };

    let pid_for_emit = project_id.clone();
    let result = run_copilot_turn_at(
        DEFAULT_OLLAMA_BASE_URL,
        &proj_dir,
        &model_id,
        &mode,
        num_ctx,
        &prompt,
        active_file.as_deref(),
        &mut session,
        &cancel_flag,
        |mut evt| {
            if let Some(obj) = evt.as_object_mut() {
                obj.insert("projectId".to_string(), json!(pid_for_emit));
            }
            let _ = window.emit("copilot:event", evt);
        },
    )
    .await;

    // Persist updated session back to state
    {
        let mut sessions = state.sessions.lock().unwrap();
        sessions.insert(project_id, session);
    }

    Ok(result)
}

#[tauri::command]
pub fn copilot_cancel(state: State<'_, CopilotState>, project_id: String) -> Result<(), String> {
    state.cancel_project(&project_id);
    Ok(())
}

#[tauri::command]
pub fn copilot_reset_session(
    state: State<'_, CopilotState>,
    project_id: String,
    keep_staged: Option<bool>,
) -> Result<(), String> {
    state.cancel_project(&project_id);
    let mut sessions = state.sessions.lock().unwrap();
    if keep_staged.unwrap_or(false) {
        if let Some(sess) = sessions.get_mut(&project_id) {
            sess.messages.clear();
        }
    } else {
        sessions.remove(&project_id);
    }
    Ok(())
}

#[tauri::command]
pub fn copilot_get_staged_changes(
    state: State<'_, CopilotState>,
    project_id: String,
) -> Vec<StagedChange> {
    state.get_staged_list(&project_id)
}

#[tauri::command]
pub fn copilot_discard_staged_file(
    state: State<'_, CopilotState>,
    project_id: String,
    file_path: String,
) -> Vec<StagedChange> {
    let clean = normalize_rel_path(&file_path).unwrap_or(file_path);
    {
        let mut sessions = state.sessions.lock().unwrap();
        if let Some(sess) = sessions.get_mut(&project_id) {
            sess.staged_files.remove(&clean);
        }
    }
    state.get_staged_list(&project_id)
}

#[tauri::command]
pub fn copilot_clear_staged_changes(
    state: State<'_, CopilotState>,
    project_id: String,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().unwrap();
    if let Some(sess) = sessions.get_mut(&project_id) {
        sess.staged_files.clear();
    }
    Ok(())
}

#[tauri::command]
pub fn copilot_delete_file_on_disk(
    state: State<'_, CopilotState>,
    project_id: String,
    file_path: String,
) -> Result<(), String> {
    let clean = normalize_rel_path(&file_path)?;
    if is_forbidden_write_path(&clean) {
        return Err(format!("Deleting protected path '{}' is forbidden", clean));
    }
    let proj_dir = get_project_path(&project_id);
    let full = resolve_safe_disk_path(&proj_dir, &clean)?;
    if full.exists() && full.is_file() {
        fs::remove_file(&full).map_err(|e| format!("Failed to delete '{}': {}", clean, e))?;
    }
    let mut sessions = state.sessions.lock().unwrap();
    if let Some(sess) = sessions.get_mut(&project_id) {
        sess.staged_files.remove(&clean);
    }
    Ok(())
}

// ==================== UNIT TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_mock_ollama(responses: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(responses)));
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let body = {
                    let mut q = queue.lock().unwrap();
                    q.pop_front()
                };
                let Some(resp_body) = body else { break };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let _ = socket.read(&mut buf).await;
                    let http = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp_body.len(),
                        resp_body
                    );
                    let _ = socket.write_all(http.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        format!("http://{}", addr)
    }

    #[test]
    fn test_parse_param_size_and_path_guards() {
        assert_eq!(parse_param_size_billions("qwen2.5-coder:32b"), Some(32.0));
        assert_eq!(parse_param_size_billions("32.8B"), Some(32.8));
        assert_eq!(parse_param_size_billions("gemma4:26b"), Some(26.0));
        assert_eq!(parse_param_size_billions("qwen2.5-coder:7b"), Some(7.0));
        assert_eq!(parse_param_size_billions("gemma4:e4b"), Some(4.0));

        assert!(normalize_rel_path("../secret.txt").is_err());
        assert!(normalize_rel_path("src/../../etc/passwd").is_err());
        assert!(normalize_rel_path("/etc/passwd").is_err());
        assert_eq!(normalize_rel_path("./src\\app\\page.tsx").unwrap(), "src/app/page.tsx");

        assert!(is_forbidden_write_path(".env"));
        assert!(is_forbidden_write_path(".env.local"));
        assert!(is_forbidden_write_path(".git/config"));
        assert!(is_forbidden_write_path("node_modules/foo/index.js"));
        assert!(!is_forbidden_write_path("src/app/page.tsx"));
    }

    #[tokio::test]
    async fn test_capability_gate_enforces_size_and_native_tools() {
        let base = spawn_mock_ollama(vec![
            // 1. Large model WITH native tools -> eligible
            json!({
                "details": { "parameter_size": "32.8B" },
                "capabilities": ["completion", "tools", "thinking"]
            })
            .to_string(),
            // 2. Large model WITHOUT native tools -> missing_tools (hidden)
            json!({
                "details": { "parameter_size": "33B" },
                "capabilities": ["completion"]
            })
            .to_string(),
        ])
        .await;

        let ok = probe_copilot_capability_at(&base, "qwen2.5-coder:32b").await;
        assert!(ok.eligible);
        assert_eq!(ok.status_code, "eligible");
        assert!(ok.supports_native_tools);
        assert!(ok.supports_thinking);

        let no_tools = probe_copilot_capability_at(&base, "deepseek-coder:33b").await;
        assert!(!no_tools.eligible);
        assert!(no_tools.is_large_model);
        assert!(!no_tools.supports_native_tools);
        assert_eq!(no_tools.status_code, "missing_tools");

        // 3. Small model -> fast-path too_small without hitting Ollama
        let small = probe_copilot_capability_at(&base, "qwen2.5-coder:7b").await;
        assert!(!small.eligible);
        assert!(!small.is_large_model);
        assert_eq!(small.status_code, "too_small");
    }

    #[tokio::test]
    async fn test_agent_loop_stages_virtual_overlay_without_touching_disk() {
        let tmp = std::env::temp_dir().join(format!("axiom-copilot-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("src")).unwrap();
        let original = "export function greet() {\n  return 'hello';\n}\n";
        fs::write(tmp.join("src/greet.ts"), original).unwrap();

        // Mock 3-step Ollama conversation:
        // Step 1: read_file("src/greet.ts")
        // Step 2: edit_file("src/greet.ts", "'hello'", "'hello world'")
        // Step 3: final summary text
        let step1 = format!(
            "{}\n",
            json!({
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "function": {
                            "name": "read_file",
                            "arguments": { "path": "src/greet.ts" }
                        }
                    }]
                },
                "done": true
            })
        );
        let step2 = format!(
            "{}\n",
            json!({
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "function": {
                            "name": "edit_file",
                            "arguments": {
                                "path": "src/greet.ts",
                                "old_text": "return 'hello';",
                                "new_text": "return 'hello world';",
                                "summary": "Update greeting text"
                            }
                        }
                    }]
                },
                "done": true
            })
        );
        let step3 = format!(
            "{}\n",
            json!({
                "message": {
                    "role": "assistant",
                    "content": "Updated src/greet.ts in the virtual overlay. Review and click Apply."
                },
                "done": true
            })
        );

        let base = spawn_mock_ollama(vec![step1, step2, step3]).await;
        let mut session = CopilotSession::default();
        let cancel = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(Vec::<Value>::new()));
        let events_clone = Arc::clone(&events);

        let res = run_copilot_turn_at(
            &base,
            &tmp,
            "qwen2.5-coder:32b",
            "agent",
            32768,
            "Change greet to return hello world",
            Some("src/greet.ts"),
            &mut session,
            &cancel,
            move |e| events_clone.lock().unwrap().push(e),
        )
        .await;

        assert!(res.success, "expected success, got error: {:?}", res.error);
        assert_eq!(res.steps_used, 3);
        assert_eq!(res.staged_changes.len(), 1);
        assert_eq!(res.staged_changes[0].path, "src/greet.ts");
        assert_eq!(res.staged_changes[0].action, "modify");
        assert!(res.staged_changes[0]
            .proposed_content
            .as_deref()
            .unwrap()
            .contains("return 'hello world';"));

        // CRITICAL INVARIANT: disk file MUST remain completely untouched until Apply!
        let disk_after = fs::read_to_string(tmp.join("src/greet.ts")).unwrap();
        assert_eq!(disk_after, original);

        // Overlay read_file sees staged change!
        let eff = read_effective_file(&tmp, &session.staged_files, "src/greet.ts")
            .unwrap()
            .unwrap();
        assert!(eff.contains("return 'hello world';"));

        // Ask mode MUST block edit_file
        let (ask_err, changed) = execute_copilot_tool(
            &tmp,
            "ask",
            &mut session.staged_files,
            "edit_file",
            &json!({ "path": "src/greet.ts", "old_text": "hello", "new_text": "bye" }),
        )
        .await;
        assert!(!changed);
        assert!(ask_err.contains("only available in Agent mode"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_context_pruning_compacts_old_tool_outputs() {
        let mut msgs = vec![json!({ "role": "system", "content": "sys" })];
        for i in 0..10 {
            msgs.push(json!({
                "role": "tool",
                "tool_name": "read_file",
                "content": format!("File: src/f{}.ts\n{}", i, "x".repeat(2000))
            }));
        }
        prune_old_tool_messages(&mut msgs, 2048);
        assert!(msgs[1]["content"]
            .as_str()
            .unwrap()
            .starts_with("[Pruned earlier read_file output"));
        // Tail messages stay intact
        assert!(msgs.last().unwrap()["content"].as_str().unwrap().len() > 1500);
    }
}
