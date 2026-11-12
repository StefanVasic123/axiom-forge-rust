use crate::*;
use std::sync::{Mutex, OnceLock};

static KATASTAR_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn katastar_lock() -> std::sync::MutexGuard<'static, ()> {
    KATASTAR_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn get_katastar_path() -> std::path::PathBuf {
    get_base_dir().join("axiom-error-katastar.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KatastarPatch {
    pub search: String,
    pub replace: String,
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KatastarHistoryEvent {
    pub timestamp: String,
    pub outcome: String,      // "passed" | "failed"
    pub source: String,       // "deterministic" | "ai" | "katastar"
    pub verification: String, // "prisma_generate" | "prisma_validate" | "next_build" | "tsc_noemit" | "npm_install" | "error_disappeared" | "error_reoccurred"
    #[serde(rename = "modelId")]
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KatastarEntry {
    pub id: String,
    pub category: String, // "prisma" | "compile" | "runtime" | "npm"
    #[serde(rename = "errorCode")]
    pub error_code: Option<String>,
    #[serde(rename = "errorSignature")]
    pub error_signature: String,
    #[serde(rename = "normalizedSample")]
    pub normalized_sample: String,
    #[serde(rename = "fileRole")]
    pub file_role: String,
    #[serde(rename = "fixSource")]
    pub fix_source: String, // "deterministic" | "ai" | "katastar"
    pub strategy: String,   // "search_replace" | "ai_edit" | "deterministic_sanitizer" | "npm_action" | "prisma_heal"
    #[serde(default)]
    pub patch: Option<KatastarPatch>,
    pub hint: String,
    pub status: String, // "pending" | "verified" | "failed" | "disabled"
    #[serde(rename = "successCount")]
    pub success_count: u32,
    #[serde(rename = "failCount")]
    pub fail_count: u32,
    #[serde(rename = "consecutiveFailures")]
    pub consecutive_failures: u32,
    #[serde(rename = "appVersion")]
    pub app_version: String,
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "firstSeenAt")]
    pub first_seen_at: String,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: String,
    #[serde(rename = "lastVerifiedAt")]
    pub last_verified_at: Option<String>,
    #[serde(rename = "lastOutcome")]
    pub last_outcome: String, // "pending" | "passed" | "failed"
    #[serde(rename = "syncedAt")]
    pub synced_at: Option<String>,
    #[serde(default)]
    pub history: Vec<KatastarHistoryEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingProjectRepair {
    #[serde(rename = "projectId")]
    pub project_id: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "errorSignature")]
    pub error_signature: String,
    #[serde(rename = "entryId")]
    pub entry_id: String,
    pub source: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KatastarStore {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "lastWeeklySyncAt")]
    pub last_weekly_sync_at: Option<String>,
    #[serde(default)]
    pub entries: Vec<KatastarEntry>,
    #[serde(rename = "pendingRepairs", default)]
    pub pending_repairs: Vec<PendingProjectRepair>,
}

impl Default for KatastarStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            updated_at: chrono::Utc::now().to_rfc3339(),
            last_weekly_sync_at: None,
            entries: Vec::new(),
            pending_repairs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ErrorSignatureInfo {
    pub category: String,
    pub error_code: Option<String>,
    pub error_signature: String,
    pub normalized_sample: String,
}

#[derive(Debug, Clone)]
pub struct KatastarPatchCandidate {
    pub entry_id: String,
    pub error_signature: String,
    pub patch: KatastarPatch,
    pub hint: String,
    pub success_count: u32,
    pub fail_count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct KatastarLookupResult {
    pub error_signature: String,
    pub error_code: Option<String>,
    pub file_role: String,
    pub applicable_patches: Vec<KatastarPatchCandidate>,
    pub hints: Vec<String>,
}

fn fnv1a_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in input.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

pub fn scrub_sensitive_text(raw: &str) -> String {
    let mut out = clean_cli_output_for_llm(raw);
    if let Ok(win_user_re) = regex::Regex::new(r#"(?i)[a-z]:\\Users\\[^\\\s"'`]+"#) {
        out = win_user_re.replace_all(&out, "<USER_DIR>").to_string();
    }
    if let Ok(unix_user_re) = regex::Regex::new(r#"(?i)(/Users/|/home/)[^/\s"'`]+"#) {
        out = unix_user_re.replace_all(&out, "<USER_DIR>").to_string();
    }
    if let Ok(proj_uuid_re) = regex::Regex::new(r#"(?i)(projects[/\\])[0-9a-f]{8}-[0-9a-f-]{27}"#) {
        out = proj_uuid_re.replace_all(&out, "${1}<PROJ>").to_string();
    }
    if let Ok(db_url_re) = regex::Regex::new(r#"(?i)(postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis)://[^\s"'`]+"#) {
        out = db_url_re.replace_all(&out, "$1://<REDACTED>").to_string();
    }
    if let Ok(secret_re) = regex::Regex::new(r#"\b(sk_live_|sk_test_|ghp_|gho_|ghu_|github_pat_|re_)[A-Za-z0-9_\-]{8,}"#) {
        out = secret_re.replace_all(&out, "<SECRET>").to_string();
    }
    if let Ok(bearer_re) = regex::Regex::new(r#"(?i)\bBearer\s+[A-Za-z0-9._\-]{10,}"#) {
        out = bearer_re.replace_all(&out, "Bearer <SECRET>").to_string();
    }
    out
}

pub fn classify_file_role(file_path: &str) -> String {
    let norm = file_path.replace('\\', "/").to_lowercase();
    let file_name = std::path::Path::new(&norm)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if norm.ends_with("schema.prisma") {
        "schema.prisma".to_string()
    } else if norm.ends_with("package.json") {
        "package.json".to_string()
    } else if file_name.starts_with("next.config.") {
        "next-config".to_string()
    } else if norm.contains("/api/") || file_name == "route.ts" || file_name == "route.js" {
        "api-route".to_string()
    } else if norm.contains("/hooks/") || file_name.starts_with("use") {
        "hook".to_string()
    } else if file_name == "layout.tsx" || file_name == "layout.jsx" || file_name == "layout.js" {
        "layout".to_string()
    } else if file_name == "page.tsx" || file_name == "page.jsx" || file_name == "page.js" {
        "page".to_string()
    } else if norm.contains("/components/") {
        "component".to_string()
    } else if norm.contains("/lib/") || norm.contains("/utils/") {
        "lib".to_string()
    } else {
        "source".to_string()
    }
}

fn to_slug(input: &str, max_len: usize) -> String {
    let mut slug = String::new();
    let mut last_dash = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= max_len {
            break;
        }
    }
    slug.trim_matches('-').to_string()
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{}...", head)
    }
}

pub fn normalize_error_signature(category_hint: &str, raw_error: &str, file_path: Option<&str>) -> ErrorSignatureInfo {
    let cleaned = scrub_sensitive_text(raw_error);
    let lower = cleaned.to_lowercase();

    let category = if category_hint == "prisma" || file_path.map_or(false, |p| p.to_lowercase().ends_with(".prisma")) {
        "prisma".to_string()
    } else if category_hint == "npm" {
        "npm".to_string()
    } else if category_hint == "runtime"
        || lower.contains("referenceerror:")
        || lower.contains("typeerror:")
        || lower.contains("syntaxerror:")
        || lower.contains("rangeerror:")
        || lower.contains("cannot read properties of null")
        || lower.contains("cannot read properties of undefined")
        || lower.contains("cannot read property ")
        || lower.contains("is not a function")
        || lower.contains("objects are not valid as a react child")
        || lower.contains("hydration failed")
        || lower.contains("unhandled runtime error")
    {
        "runtime".to_string()
    } else {
        "compile".to_string()
    };

    if category == "prisma" {
        let error_code = regex::Regex::new(r"\b(P\d{4})\b")
            .ok()
            .and_then(|re| re.captures(&cleaned))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());

        let is_prisma_boilerplate = |l: &str| -> bool {
            l.is_empty()
                || l.starts_with("Environment variables loaded")
                || l.starts_with("Prisma schema loaded")
                || l.starts_with("Datasource ")
                || l.starts_with("Error: Prisma schema validation")
                || l.starts_with("Error code:")
                || l.starts_with("Validation Error Count:")
                || l.starts_with("[Context:")
                || l.starts_with("Prisma CLI Version")
                || l.starts_with("-->")
                || l.starts_with('|')
        };

        let core_line = cleaned
            .lines()
            .map(|l| l.trim())
            .filter(|l| !is_prisma_boilerplate(l))
            .find(|l| {
                l.starts_with("error:")
                    || l.contains("Error validating:")
                    || l.contains("Error parsing")
                    || l.contains("Validation Error")
            })
            .or_else(|| {
                cleaned
                    .lines()
                    .map(|l| l.trim())
                    .find(|l| !is_prisma_boilerplate(l))
            })
            .unwrap_or("prisma validation error");

        let mut normalized = core_line.to_string();
        if let Ok(backtick_re) = regex::Regex::new(r"`[^`]+`") {
            normalized = backtick_re.replace_all(&normalized, "<IDENT>").to_string();
        }
        if let Ok(line_re) = regex::Regex::new(r"schema\.prisma:\d+(:\d+)?") {
            normalized = line_re.replace_all(&normalized, "schema.prisma:<LINE>").to_string();
        }
        let code_part = error_code.as_deref().unwrap_or("PRISMA");
        let body_slug = to_slug(&normalized.replace("error:", "").replace("Error validating:", ""), 72);
        let sig = if body_slug.is_empty() {
            format!("prisma:{}", code_part)
        } else {
            format!("prisma:{}:{}", code_part, body_slug)
        };
        return ErrorSignatureInfo {
            category,
            error_code,
            error_signature: sig,
            normalized_sample: truncate_chars(&normalized, 220),
        };
    }

    if category == "npm" {
        let error_code = [
            "ETARGET",
            "E404",
            "ERESOLVE",
            "EPERM",
            "EACCES",
            "ENOTCACHED",
            "ENOENT",
            "ENOTFOUND",
            "ETIMEDOUT",
            "ECONNREFUSED",
        ]
        .iter()
        .find(|code| cleaned.contains(**code))
        .map(|s| (*s).to_string());

        let pkg_hint = if let Some(idx) = cleaned.find("No matching version found for ") {
            let tok = cleaned[idx + 30..].split_whitespace().next().unwrap_or("");
            tok.rsplit_once('@')
                .map(|(name, _)| name)
                .unwrap_or(tok)
                .trim_matches(|c| c == '.' || c == '\'' || c == '"')
                .to_string()
        } else if let Some(idx) = cleaned.find("requested resource '") {
            let inner = cleaned[idx + 20..].split('\'').next().unwrap_or("");
            inner.rsplit_once('@').map(|(name, _)| name).unwrap_or(inner).to_string()
        } else if let Some(idx) = cleaned.find("missing package '") {
            let inner = cleaned[idx + 17..].split('\'').next().unwrap_or("");
            to_slug(inner, 48)
        } else if let Some(idx) = cleaned.find("registry.npmjs.org/") {
            let after = &cleaned[idx + 19..];
            let raw_pkg = after.split_whitespace().next().unwrap_or("");
            to_slug(raw_pkg, 48)
        } else {
            String::new()
        };

        let code_part = error_code.as_deref().unwrap_or("NPM_ERR");
        let sig = if !pkg_hint.is_empty() {
            format!("npm:{}:{}", code_part, pkg_hint)
        } else {
            format!("npm:{}", code_part)
        };
        let first_err_line = cleaned
            .lines()
            .map(|l| l.trim())
            .find(|l| {
                l.contains("npm error")
                    || l.contains("npm ERR!")
                    || l.contains("notarget")
                    || l.contains("missing package")
            })
            .or_else(|| cleaned.lines().map(|l| l.trim()).find(|l| !l.is_empty()))
            .unwrap_or("npm install error");
        return ErrorSignatureInfo {
            category,
            error_code,
            error_signature: sig,
            normalized_sample: truncate_chars(first_err_line, 220),
        };
    }

    let is_next_compile_boilerplate = |l: &str| -> bool {
        let ll = l
            .trim()
            .trim_start_matches(|c: char| matches!(c, '>' | '-' | '*' | '•' | '⨯' | '✓' | '▲' | ' '))
            .to_lowercase();
        ll.is_empty()
            || ll == "failed to compile."
            || ll == "failed to compile"
            || ll.starts_with("error: failed to compile")
            || ll.starts_with("build failed because of webpack")
            || ll.starts_with("build error occurred")
            || ll.starts_with("import trace for requested module")
            || ll.starts_with("next.js (")
    };

    // Compile & Runtime normalization
    if lower.contains("you're importing a component that needs")
        || lower.contains("this only works in a client component")
        || lower.contains("mark the file with the 'use client'")
        || lower.contains("use client\" directive")
    {
        return ErrorSignatureInfo {
            category: "compile".to_string(),
            error_code: Some("NEXT_USE_CLIENT".to_string()),
            error_signature: "compile:NEXT_USE_CLIENT:missing-client-directive".to_string(),
            normalized_sample: "Component requires \"use client\" directive at top of file".to_string(),
        };
    }

    if lower.contains("referenceerror:") && lower.contains("is not defined") {
        let ident_sample = cleaned
            .lines()
            .map(|l| l.trim())
            .find(|l| l.to_lowercase().contains("referenceerror:") && l.to_lowercase().contains("is not defined"))
            .unwrap_or("ReferenceError: <IDENT> is not defined");
        return ErrorSignatureInfo {
            category: "runtime".to_string(),
            error_code: Some("ReferenceError".to_string()),
            error_signature: "runtime:ReferenceError:identifier-not-defined".to_string(),
            normalized_sample: truncate_chars(ident_sample, 220),
        };
    }

    if lower.contains("typeerror:")
        || lower.contains("cannot read properties of null")
        || lower.contains("cannot read properties of undefined")
        || lower.contains("cannot read property ")
        || lower.contains("is not a function")
    {
        let err_line = cleaned
            .lines()
            .map(|l| l.trim())
            .find(|l| {
                let ll = l.to_lowercase();
                ll.contains("typeerror:")
                    || ll.contains("cannot read propert")
                    || ll.contains("is not a function")
            })
            .unwrap_or("TypeError: runtime null/undefined dereference");
        let slug = to_slug(err_line, 64);
        let sig = if slug.is_empty() {
            "runtime:TypeError".to_string()
        } else {
            format!("runtime:TypeError:{}", slug)
        };
        return ErrorSignatureInfo {
            category: "runtime".to_string(),
            error_code: Some("TypeError".to_string()),
            error_signature: sig,
            normalized_sample: truncate_chars(err_line, 220),
        };
    }

    if lower.contains("module not found: can't resolve") || cleaned.contains("TS2307") {
        let missing_mod = regex::Regex::new(r#"(?:Can't resolve|Cannot find module)\s+['"]([^'"]+)['"]"#)
            .ok()
            .and_then(|re| re.captures(&cleaned))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        let code = if cleaned.contains("TS2307") { "TS2307" } else { "MODULE_NOT_FOUND" };
        let sig = if is_local_import_specifier(&missing_mod) {
            format!("compile:{}:local-import", code)
        } else if !missing_mod.is_empty() {
            format!("compile:{}:{}", code, to_slug(&missing_mod, 48))
        } else {
            format!("compile:{}:cannot-find-module", code)
        };
        let sample = cleaned
            .lines()
            .map(|l| l.trim())
            .filter(|l| !is_next_compile_boilerplate(l))
            .find(|l| l.contains("Can't resolve") || l.contains("Cannot find module") || l.contains("TS2307"))
            .or_else(|| {
                cleaned
                    .lines()
                    .map(|l| l.trim())
                    .find(|l| !is_next_compile_boilerplate(l) && !l.is_empty())
            })
            .unwrap_or("Module not found");
        return ErrorSignatureInfo {
            category: "compile".to_string(),
            error_code: Some(code.to_string()),
            error_signature: sig,
            normalized_sample: truncate_chars(sample, 220),
        };
    }

    if let Some(ts_code) = regex::Regex::new(r"\b(TS\d{4,5})\b")
        .ok()
        .and_then(|re| re.captures(&cleaned))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    {
        let ts_line = cleaned
            .lines()
            .map(|l| l.trim())
            .find(|l| l.contains(&ts_code))
            .unwrap_or(&cleaned);
        let msg_part = ts_line.split(&ts_code).nth(1).unwrap_or(ts_line).trim_start_matches(':').trim();
        let mut norm_msg = msg_part.to_string();
        if let Ok(sq_re) = regex::Regex::new(r"'[^']+'") {
            norm_msg = sq_re.replace_all(&norm_msg, "'<ID>'").to_string();
        }
        let slug = to_slug(&norm_msg, 64);
        let sig = if slug.is_empty() {
            format!("compile:{}", ts_code)
        } else {
            format!("compile:{}:{}", ts_code, slug)
        };
        return ErrorSignatureInfo {
            category: "compile".to_string(),
            error_code: Some(ts_code),
            error_signature: sig,
            normalized_sample: truncate_chars(ts_line, 220),
        };
    }

    let first_err = cleaned
        .lines()
        .map(|l| l.trim())
        .filter(|l| !is_next_compile_boilerplate(l))
        .find(|l| {
            let ll = l.to_lowercase();
            (ll.contains("error") || ll.contains("failed") || ll.contains("syntaxerror") || ll.contains("typeerror"))
                && l.len() > 6
        })
        .or_else(|| {
            cleaned
                .lines()
                .map(|l| l.trim())
                .find(|l| !is_next_compile_boilerplate(l) && l.len() > 4)
        })
        .unwrap_or("unknown error");

    let mut norm_line = first_err.to_string();
    if let Ok(line_col_re) = regex::Regex::new(r":\d+:\d+") {
        norm_line = line_col_re.replace_all(&norm_line, ":<LINE>").to_string();
    }
    let slug = to_slug(&norm_line, 72);
    let sig = if slug.is_empty() {
        format!("{}:general", category)
    } else {
        format!("{}:{}", category, slug)
    };
    ErrorSignatureInfo {
        category,
        error_code: None,
        error_signature: sig,
        normalized_sample: truncate_chars(&norm_line, 220),
    }
}

pub fn is_safe_auto_patch(file_path: &str, patch: &KatastarPatch) -> bool {
    let search_trimmed = patch.search.trim();
    let replace_trimmed = patch.replace.trim();
    if search_trimmed.len() < 8 || search_trimmed == replace_trimmed {
        return false;
    }
    if patch.search.len() > 350 || patch.replace.len() > 350 {
        return false;
    }
    if patch.search.lines().count() > 6 || patch.replace.lines().count() > 6 {
        return false;
    }

    // Never allow secrets/env credentials into reusable rules
    let combined_lower = format!("{}\n{}", patch.search, patch.replace).to_lowercase();
    if combined_lower.contains("sk_live_")
        || combined_lower.contains("sk_test_")
        || combined_lower.contains("ghp_")
        || combined_lower.contains("database_url")
        || combined_lower.contains("postgresql://")
    {
        return false;
    }

    let path_lower = file_path.replace('\\', "/").to_lowercase();
    if path_lower.ends_with(".prisma") {
        // Only universally invalid Prisma syntax/keyword fixes may auto-apply across projects
        // (never mutate datasource `provider` blindly during generation; SQLite projects need sqlite)
        let is_global_prisma_fix = combined_lower.contains("ondelete:")
            || combined_lower.contains("onupdate:")
            || combined_lower.contains("previewfeatures");
        return is_global_prisma_fix && !combined_lower.contains("model ");
    }

    // Never auto-apply JSX/HTML component blocks across projects
    if patch.search.contains('<')
        || patch.search.contains("</")
        || patch.search.contains("return (")
        || patch.replace.contains('<')
        || patch.replace.contains("</")
        || patch.replace.contains("return (")
    {
        return false;
    }

    // Only allow atomic import / export / directive / config / type-level patterns
    let is_atomic_code_pattern = search_trimmed.starts_with("import ")
        || search_trimmed.starts_with("import{")
        || search_trimmed.starts_with("export ")
        || search_trimmed.contains("from '")
        || search_trimmed.contains("from \"")
        || search_trimmed.contains("require(")
        || search_trimmed.contains("require.resolve(")
        || search_trimmed.contains("use client")
        || path_lower.contains("next.config");

    is_atomic_code_pattern
}

/// Computes a safe atomic single-hunk diff between `before` and `after` in pure Rust without calling an LLM.
pub fn extract_safe_atomic_patch(file_path: &str, before: &str, after: &str) -> Option<KatastarPatch> {
    let b_norm = before.replace("\r\n", "\n");
    let a_norm = after.replace("\r\n", "\n");
    if b_norm == a_norm {
        return None;
    }

    let b_lines: Vec<&str> = b_norm.lines().collect();
    let a_lines: Vec<&str> = a_norm.lines().collect();

    let mut prefix = 0usize;
    while prefix < b_lines.len() && prefix < a_lines.len() && b_lines[prefix] == a_lines[prefix] {
        prefix += 1;
    }

    let mut b_end = b_lines.len();
    let mut a_end = a_lines.len();
    while b_end > prefix && a_end > prefix && b_lines[b_end - 1] == a_lines[a_end - 1] {
        b_end -= 1;
        a_end -= 1;
    }

    let b_hunk = &b_lines[prefix..b_end];
    let a_hunk = &a_lines[prefix..a_end];
    if b_hunk.is_empty() || b_hunk.len() > 4 || a_hunk.len() > 4 {
        return None;
    }

    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    let patch = KatastarPatch {
        search: b_hunk.join("\n"),
        replace: a_hunk.join("\n"),
        extensions: if ext.is_empty() { vec![] } else { vec![ext] },
    };

    if is_safe_auto_patch(file_path, &patch) {
        Some(patch)
    } else {
        None
    }
}

pub fn load_katastar_store_at(path: &std::path::Path) -> KatastarStore {
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(store) = serde_json::from_str::<KatastarStore>(&content) {
                return store;
            }
        }
    }
    KatastarStore::default()
}

pub fn save_katastar_store_at(path: &std::path::Path, store: &mut KatastarStore) -> Result<(), String> {
    store.updated_at = chrono::Utc::now().to_rfc3339();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let pretty = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &pretty).map_err(|e| e.to_string())?;
    if let Err(_) = std::fs::rename(&tmp_path, path) {
        std::fs::write(path, &pretty).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp_path);
    }
    Ok(())
}

fn entry_id_for(category: &str, error_signature: &str, strategy_key: &str) -> String {
    let raw = format!("{}|{}|{}", category, error_signature, strategy_key);
    let prefix = to_slug(error_signature, 28);
    if prefix.is_empty() {
        format!("kat-{}", fnv1a_hex(&raw))
    } else {
        format!("kat-{}-{}", prefix, &fnv1a_hex(&raw)[..8])
    }
}

pub fn lookup_katastar_at(
    store_path: &std::path::Path,
    category: &str,
    raw_error: &str,
    file_path: &str,
    file_content: &str,
    skip_entry_ids: &[String],
) -> KatastarLookupResult {
    let sig_info = normalize_error_signature(category, raw_error, Some(file_path));
    let file_role = classify_file_role(file_path);
    let file_ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    let _guard = katastar_lock();
    let store = load_katastar_store_at(store_path);

    let mut patch_candidates: Vec<(i32, KatastarPatchCandidate)> = Vec::new();
    let mut hint_candidates: Vec<(i32, String)> = Vec::new();

    for entry in &store.entries {
        if entry.status != "verified" || entry.success_count <= entry.fail_count {
            continue;
        }
        if skip_entry_ids.contains(&entry.id) {
            continue;
        }
        if entry.category != sig_info.category {
            continue;
        }

        let exact_sig = entry.error_signature == sig_info.error_signature;
        let same_code_and_role = sig_info.error_code.is_some()
            && entry.error_code == sig_info.error_code
            && entry.file_role == file_role;

        let score = (entry.success_count as i32) * 3 - (entry.fail_count as i32) * 5 + if exact_sig { 12 } else { 4 };

        if exact_sig || same_code_and_role {
            let trimmed_hint = entry.hint.trim();
            if !trimmed_hint.is_empty() {
                hint_candidates.push((score, trimmed_hint.to_string()));
            }
        }

        if let Some(ref patch) = entry.patch {
            if !is_safe_auto_patch(file_path, patch) {
                continue;
            }
            let ext_ok = patch.extensions.is_empty()
                || patch.extensions.iter().any(|e| e.eq_ignore_ascii_case(&file_ext));
            if !ext_ok || !file_content.contains(&patch.search) {
                continue;
            }
            if exact_sig || same_code_and_role || entry.file_role == file_role {
                patch_candidates.push((
                    score,
                    KatastarPatchCandidate {
                        entry_id: entry.id.clone(),
                        error_signature: entry.error_signature.clone(),
                        patch: patch.clone(),
                        hint: entry.hint.clone(),
                        success_count: entry.success_count,
                        fail_count: entry.fail_count,
                    },
                ));
            }
        }
    }

    patch_candidates.sort_by(|a, b| b.0.cmp(&a.0));
    hint_candidates.sort_by(|a, b| b.0.cmp(&a.0));

    let mut unique_hints = Vec::new();
    for (_, h) in hint_candidates {
        if !unique_hints.contains(&h) {
            unique_hints.push(h);
            if unique_hints.len() >= 3 {
                break;
            }
        }
    }

    KatastarLookupResult {
        error_signature: sig_info.error_signature,
        error_code: sig_info.error_code,
        file_role,
        applicable_patches: patch_candidates.into_iter().map(|(_, c)| c).collect(),
        hints: unique_hints,
    }
}

pub fn lookup_katastar(
    category: &str,
    raw_error: &str,
    file_path: &str,
    file_content: &str,
    skip_entry_ids: &[String],
) -> KatastarLookupResult {
    lookup_katastar_at(&get_katastar_path(), category, raw_error, file_path, file_content, skip_entry_ids)
}

/// Returns verified safe atomic patches applicable to `file_path` for `sanitizer.rs`.
/// Under `#[cfg(test)]`, returns empty so unit tests remain hermetic and never depend on developer AppData.
pub fn get_verified_sanitizer_patches(file_path: &str) -> Vec<(String, String)> {
    #[cfg(test)]
    {
        let _ = file_path;
        return Vec::new();
    }
    #[cfg(not(test))]
    {
        get_verified_sanitizer_patches_at(&get_katastar_path(), file_path)
    }
}

pub fn get_verified_sanitizer_patches_at(store_path: &std::path::Path, file_path: &str) -> Vec<(String, String)> {
    if !store_path.exists() {
        return Vec::new();
    }
    let file_ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    let _guard = katastar_lock();
    let store = load_katastar_store_at(store_path);
    let mut out = Vec::new();
    for entry in &store.entries {
        if entry.status == "verified" && entry.success_count > entry.fail_count {
            if let Some(ref patch) = entry.patch {
                let ext_ok = patch.extensions.is_empty()
                    || patch.extensions.iter().any(|e| e.eq_ignore_ascii_case(&file_ext));
                if ext_ok && is_safe_auto_patch(file_path, patch) {
                    out.push((patch.search.clone(), patch.replace.clone()));
                }
            }
        }
    }
    out
}

pub fn register_repair_attempt_at(
    store_path: &std::path::Path,
    category: &str,
    raw_error: &str,
    file_path: &str,
    fix_source: &str,
    strategy: &str,
    patch_opt: Option<KatastarPatch>,
    hint: &str,
    model_id: &str,
) -> String {
    let sig_info = normalize_error_signature(category, raw_error, Some(file_path));
    let file_role = classify_file_role(file_path);
    let valid_patch = patch_opt.filter(|p| is_safe_auto_patch(file_path, p));
    let strategy_key = match &valid_patch {
        Some(p) => format!("patch:{}=>{}", p.search, p.replace),
        None => format!("{}:{}", strategy, file_role),
    };
    let id = entry_id_for(&sig_info.category, &sig_info.error_signature, &strategy_key);
    let now = chrono::Utc::now().to_rfc3339();
    let scrubbed_hint = truncate_chars(&scrub_sensitive_text(hint), 280);

    let _guard = katastar_lock();
    let mut store = load_katastar_store_at(store_path);

    if let Some(existing) = store.entries.iter_mut().find(|e| e.id == id) {
        existing.last_seen_at = now;
        existing.fix_source = fix_source.to_string();
        if !model_id.is_empty() {
            existing.model_id = model_id.to_string();
        }
        if existing.patch.is_none() && valid_patch.is_some() {
            existing.patch = valid_patch;
        }
        if !scrubbed_hint.is_empty() && existing.hint.is_empty() {
            existing.hint = scrubbed_hint;
        }
        // Keep verified status untouched while the verification test runs; otherwise mark pending.
        if existing.status != "verified" && existing.status != "disabled" {
            existing.status = "pending".to_string();
            existing.last_outcome = "pending".to_string();
        }
    } else {
        store.entries.push(KatastarEntry {
            id: id.clone(),
            category: sig_info.category,
            error_code: sig_info.error_code,
            error_signature: sig_info.error_signature,
            normalized_sample: sig_info.normalized_sample,
            file_role,
            fix_source: fix_source.to_string(),
            strategy: strategy.to_string(),
            patch: valid_patch,
            hint: scrubbed_hint,
            status: "pending".to_string(),
            success_count: 0,
            fail_count: 0,
            consecutive_failures: 0,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            model_id: model_id.to_string(),
            first_seen_at: now.clone(),
            last_seen_at: now,
            last_verified_at: None,
            last_outcome: "pending".to_string(),
            synced_at: None,
            history: Vec::new(),
        });
    }

    let _ = save_katastar_store_at(store_path, &mut store);
    id
}

pub fn register_repair_attempt(
    category: &str,
    raw_error: &str,
    file_path: &str,
    fix_source: &str,
    strategy: &str,
    patch_opt: Option<KatastarPatch>,
    hint: &str,
    model_id: &str,
) -> String {
    register_repair_attempt_at(
        &get_katastar_path(),
        category,
        raw_error,
        file_path,
        fix_source,
        strategy,
        patch_opt,
        hint,
        model_id,
    )
}

pub fn finalize_katastar_outcome_at(
    store_path: &std::path::Path,
    entry_id: &str,
    passed: bool,
    source: &str,
    verification: &str,
    upgraded_patch: Option<KatastarPatch>,
    upgraded_hint: Option<&str>,
    model_id: &str,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let _guard = katastar_lock();
    let mut store = load_katastar_store_at(store_path);

    if let Some(entry) = store.entries.iter_mut().find(|e| e.id == entry_id) {
        entry.last_seen_at = now.clone();
        if !model_id.is_empty() {
            entry.model_id = model_id.to_string();
        }
        entry.history.push(KatastarHistoryEvent {
            timestamp: now.clone(),
            outcome: if passed { "passed".to_string() } else { "failed".to_string() },
            source: source.to_string(),
            verification: verification.to_string(),
            model_id: model_id.to_string(),
        });
        if entry.history.len() > 15 {
            let drain_count = entry.history.len() - 15;
            entry.history.drain(0..drain_count);
        }

        if passed {
            entry.success_count += 1;
            entry.consecutive_failures = 0;
            entry.last_outcome = "passed".to_string();
            entry.last_verified_at = Some(now);
            if entry.status != "disabled" {
                entry.status = "verified".to_string();
            }
            if entry.patch.is_none() {
                if let Some(p) = upgraded_patch {
                    if !p.search.trim().is_empty() && p.search != p.replace {
                        entry.patch = Some(p);
                    }
                }
            }
            if let Some(h) = upgraded_hint {
                let clean_h = truncate_chars(&scrub_sensitive_text(h), 280);
                if !clean_h.is_empty() {
                    entry.hint = clean_h;
                }
            }
        } else {
            entry.fail_count += 1;
            entry.consecutive_failures += 1;
            entry.last_outcome = "failed".to_string();
            if entry.status == "pending" {
                entry.status = "failed".to_string();
            } else if entry.status == "verified"
                && (entry.consecutive_failures >= 2 || entry.fail_count > entry.success_count)
            {
                entry.status = "disabled".to_string();
            }
        }
        let _ = save_katastar_store_at(store_path, &mut store);
    }
}

pub fn finalize_katastar_outcome(
    entry_id: &str,
    passed: bool,
    source: &str,
    verification: &str,
    upgraded_patch: Option<KatastarPatch>,
    upgraded_hint: Option<&str>,
    model_id: &str,
) {
    finalize_katastar_outcome_at(
        &get_katastar_path(),
        entry_id,
        passed,
        source,
        verification,
        upgraded_patch,
        upgraded_hint,
        model_id,
    );
}

pub fn track_pending_project_repair_at(
    store_path: &std::path::Path,
    project_id: &str,
    file_path: &str,
    error_signature: &str,
    entry_id: &str,
    source: &str,
) {
    let norm_file = file_path.replace('\\', "/");
    let now = chrono::Utc::now().to_rfc3339();
    let _guard = katastar_lock();
    let mut store = load_katastar_store_at(store_path);

    store.pending_repairs.retain(|p| {
        !(p.project_id == project_id && p.file_path == norm_file && p.error_signature == error_signature)
    });
    store.pending_repairs.push(PendingProjectRepair {
        project_id: project_id.to_string(),
        file_path: norm_file,
        error_signature: error_signature.to_string(),
        entry_id: entry_id.to_string(),
        source: source.to_string(),
        created_at: now,
    });
    if store.pending_repairs.len() > 60 {
        let excess = store.pending_repairs.len() - 60;
        store.pending_repairs.drain(0..excess);
    }
    let _ = save_katastar_store_at(store_path, &mut store);
}

pub fn track_pending_project_repair(
    project_id: &str,
    file_path: &str,
    error_signature: &str,
    entry_id: &str,
    source: &str,
) {
    track_pending_project_repair_at(&get_katastar_path(), project_id, file_path, error_signature, entry_id, source);
}

/// If an identical `(project_id, file_path, error_signature)` arrives again before verification,
/// the previous pending repair failed to resolve the issue -> mark it failed immediately and return true.
pub fn resolve_reoccurred_pending_repair_at(
    store_path: &std::path::Path,
    project_id: &str,
    file_path: &str,
    current_error_signature: &str,
    model_id: &str,
) -> Vec<String> {
    let norm_file = file_path.replace('\\', "/");
    let matched_to_fail: Vec<PendingProjectRepair> = {
        let _guard = katastar_lock();
        let mut store = load_katastar_store_at(store_path);
        let mut failed_items = Vec::new();
        let mut remaining = Vec::with_capacity(store.pending_repairs.len());
        for item in store.pending_repairs.drain(..) {
            if item.project_id == project_id
                && item.file_path == norm_file
                && item.error_signature == current_error_signature
            {
                failed_items.push(item);
            } else {
                remaining.push(item);
            }
        }
        store.pending_repairs = remaining;
        if !failed_items.is_empty() {
            let _ = save_katastar_store_at(store_path, &mut store);
        }
        failed_items
    };

    let mut failed_entry_ids = Vec::new();
    for item in matched_to_fail {
        finalize_katastar_outcome_at(
            store_path,
            &item.entry_id,
            false,
            &item.source,
            "error_reoccurred",
            None,
            None,
            model_id,
        );
        failed_entry_ids.push(item.entry_id);
    }
    failed_entry_ids
}

pub fn resolve_reoccurred_pending_repair(
    project_id: &str,
    file_path: &str,
    current_error_signature: &str,
    model_id: &str,
) -> Vec<String> {
    resolve_reoccurred_pending_repair_at(
        &get_katastar_path(),
        project_id,
        file_path,
        current_error_signature,
        model_id,
    )
}

pub fn complete_pending_project_repairs_at(
    store_path: &std::path::Path,
    project_id: &str,
    file_path: Option<&str>,
    passed: bool,
    verification: &str,
    model_id: &str,
) {
    let norm_file_opt = file_path.map(|f| f.replace('\\', "/"));
    let matched: Vec<PendingProjectRepair> = {
        let _guard = katastar_lock();
        let mut store = load_katastar_store_at(store_path);
        let mut to_resolve = Vec::new();
        let mut remaining = Vec::with_capacity(store.pending_repairs.len());
        for item in store.pending_repairs.drain(..) {
            let file_matches = match &norm_file_opt {
                Some(nf) => &item.file_path == nf,
                None => true,
            };
            if item.project_id == project_id && file_matches {
                to_resolve.push(item);
            } else {
                remaining.push(item);
            }
        }
        store.pending_repairs = remaining;
        if !to_resolve.is_empty() {
            let _ = save_katastar_store_at(store_path, &mut store);
        }
        to_resolve
    };

    for item in matched {
        finalize_katastar_outcome_at(
            store_path,
            &item.entry_id,
            passed,
            &item.source,
            verification,
            None,
            None,
            model_id,
        );
    }
}

pub fn complete_pending_project_repairs(
    project_id: &str,
    file_path: Option<&str>,
    passed: bool,
    verification: &str,
    model_id: &str,
) {
    complete_pending_project_repairs_at(
        &get_katastar_path(),
        project_id,
        file_path,
        passed,
        verification,
        model_id,
    );
}

#[tauri::command]
pub fn get_error_katastar() -> Result<Value, String> {
    let path = get_katastar_path();
    let _guard = katastar_lock();
    let store = load_katastar_store_at(&path);
    let verified_count = store.entries.iter().filter(|e| e.status == "verified").count();
    let failed_count = store.entries.iter().filter(|e| e.status == "failed").count();
    let disabled_count = store.entries.iter().filter(|e| e.status == "disabled").count();
    let pending_count = store.entries.iter().filter(|e| e.status == "pending").count();

    Ok(serde_json::json!({
        "path": path.to_string_lossy().to_string(),
        "schemaVersion": store.schema_version,
        "updatedAt": store.updated_at,
        "lastWeeklySyncAt": store.last_weekly_sync_at,
        "summary": {
            "totalEntries": store.entries.len(),
            "verified": verified_count,
            "failed": failed_count,
            "disabled": disabled_count,
            "pending": pending_count,
            "inFlightPendingRepairs": store.pending_repairs.len()
        },
        "entries": store.entries
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileHealAttemptRecord {
    #[serde(default)]
    pub round: usize,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub resolved: Vec<String>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileErrorLedger {
    #[serde(default, rename = "filePath")]
    pub file_path: String,
    #[serde(default)]
    pub resolved: Vec<String>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    #[serde(default)]
    pub attempts: Vec<FileHealAttemptRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectHealLedger {
    #[serde(default, rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: String,
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, FileErrorLedger>,
}

impl Default for ProjectHealLedger {
    fn default() -> Self {
        Self {
            schema_version: 1,
            updated_at: chrono::Utc::now().to_rfc3339(),
            files: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticDiff {
    pub resolved: Vec<String>,
    pub still_unresolved: Vec<String>,
    pub introduced: Vec<String>,
    pub count_reduced: bool,
}

/// Produces a line-number-independent canonical key for a compiler/runtime diagnostic
/// so line shifts from adding an `import` do not misclassify resolved vs unresolved errors.
pub fn canonical_diag_key(raw_line: &str) -> String {
    let trimmed = raw_line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(ts_re) = regex::Regex::new(r"(?i)\b(TS\d{4})\b\s*[:\]]?\s*(.+)$") {
        if let Some(cap) = ts_re.captures(trimmed) {
            let code = cap.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
            let msg = cap
                .get(2)
                .map(|m| m.as_str().split_whitespace().collect::<Vec<&str>>().join(" "))
                .unwrap_or_default();
            if !code.is_empty() && !msg.is_empty() {
                return format!("{}:{}", code, msg);
            }
        }
    }
    let stripped = if let Ok(loc_re) = regex::Regex::new(
        r#"^(?:[^\r\n()]+?\(\d+,\d+\):\s*(?:error|warning)\s*|Line\s+\d+,\s*Col\s+\d+:\s*(?:\[[^\]]+\]\s*)?|[^:\r\n]+:\d+:\d+\s*[-:]?\s*)"#,
    ) {
        loc_re.replace(trimmed, "").to_string()
    } else {
        trimmed.to_string()
    };
    stripped.split_whitespace().collect::<Vec<&str>>().join(" ")
}

pub fn compute_diagnostic_diff(baseline: &[String], post: &[String]) -> DiagnosticDiff {
    let mut base_by_key: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for line in baseline {
        let key = canonical_diag_key(line);
        if !key.is_empty() {
            base_by_key.entry(key).or_insert_with(|| line.clone());
        }
    }

    let mut post_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut still_unresolved = Vec::new();
    let mut introduced = Vec::new();
    for line in post {
        let key = canonical_diag_key(line);
        if key.is_empty() {
            continue;
        }
        post_keys.insert(key.clone());
        if base_by_key.contains_key(&key) {
            still_unresolved.push(line.clone());
        } else {
            introduced.push(line.clone());
        }
    }

    let mut resolved = Vec::new();
    for (key, orig_line) in &base_by_key {
        if !post_keys.contains(key) {
            resolved.push(orig_line.clone());
        }
    }

    DiagnosticDiff {
        resolved,
        still_unresolved,
        introduced,
        count_reduced: post.len() < baseline.len(),
    }
}

pub fn load_project_heal_ledger(proj_dir: &std::path::Path) -> ProjectHealLedger {
    let ledger_path = proj_dir.join(".axiom").join("heal-ledger.json");
    if ledger_path.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&ledger_path) {
            if let Ok(parsed) = serde_json::from_str::<ProjectHealLedger>(&raw) {
                return parsed;
            }
        }
    }
    ProjectHealLedger::default()
}

pub fn save_project_heal_ledger(proj_dir: &std::path::Path, ledger: &ProjectHealLedger) {
    let axiom_dir = proj_dir.join(".axiom");
    let _ = std::fs::create_dir_all(&axiom_dir);
    let ledger_path = axiom_dir.join("heal-ledger.json");
    if let Ok(pretty) = serde_json::to_string_pretty(ledger) {
        let _ = std::fs::write(ledger_path, pretty);
    }
}

pub fn record_project_heal_round(
    proj_dir: &std::path::Path,
    file_path: &str,
    round: usize,
    source: &str,
    outcome: &str,
    resolved_items: &[String],
    unresolved_items: &[String],
    summary: &str,
) {
    let norm_path = file_path.trim().replace('\\', "/").trim_start_matches("./").to_string();
    if norm_path.is_empty() {
        return;
    }

    let mut ledger = load_project_heal_ledger(proj_dir);
    let entry = ledger.files.entry(norm_path.clone()).or_insert_with(|| FileErrorLedger {
        file_path: norm_path.clone(),
        ..Default::default()
    });

    let unresolved_keys: std::collections::HashSet<String> = unresolved_items
        .iter()
        .map(|u| canonical_diag_key(u))
        .filter(|k| !k.is_empty())
        .collect();

    // Remove previously resolved keys only if they resurfaced as active unresolved errors
    entry.resolved.retain(|r| !unresolved_keys.contains(&canonical_diag_key(r)));

    let mut existing_res_keys: std::collections::HashSet<String> = entry
        .resolved
        .iter()
        .map(|r| canonical_diag_key(r))
        .collect();
    for r in resolved_items {
        let key = canonical_diag_key(r);
        if !key.is_empty() && !unresolved_keys.contains(&key) && existing_res_keys.insert(key.clone()) {
            entry.resolved.push(key);
        }
    }

    let mut seen_unres = std::collections::HashSet::new();
    let mut cleaned_unres = Vec::new();
    for u in unresolved_items {
        let t = u.trim();
        if !t.is_empty() && seen_unres.insert(canonical_diag_key(t)) {
            cleaned_unres.push(t.to_string());
        }
    }
    entry.unresolved = cleaned_unres;

    entry.attempts.push(FileHealAttemptRecord {
        round,
        source: source.to_string(),
        outcome: outcome.to_string(),
        resolved: resolved_items.to_vec(),
        unresolved: unresolved_items.iter().take(10).cloned().collect(),
        summary: summary.chars().take(400).collect(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    if entry.attempts.len() > 12 {
        let excess = entry.attempts.len() - 12;
        entry.attempts.drain(0..excess);
    }

    ledger.updated_at = chrono::Utc::now().to_rfc3339();
    save_project_heal_ledger(proj_dir, &ledger);
}

/// Builds the Chain-of-Information block from `.axiom/heal-ledger.json` for round 2+.
/// Strictly sends ONLY currently unresolved diagnostics (omitting already-resolved diagnostics
/// so small models do not re-edit or undo verified fixes).
pub fn format_chain_of_information_prompt(
    proj_dir: &std::path::Path,
    file_path: &str,
    current_unresolved: &[String],
    attempt: usize,
) -> String {
    let norm_path = file_path.trim().replace('\\', "/").trim_start_matches("./").to_string();
    let ledger = load_project_heal_ledger(proj_dir);
    let entry_opt = ledger.files.get(&norm_path);

    let resolved_keys: std::collections::HashSet<String> = entry_opt
        .map(|e| e.resolved.iter().map(|r| canonical_diag_key(r)).collect())
        .unwrap_or_default();

    let source_unresolved: Vec<String> = if !current_unresolved.is_empty() {
        current_unresolved.to_vec()
    } else {
        entry_opt.map(|e| e.unresolved.clone()).unwrap_or_default()
    };

    let active_unresolved: Vec<String> = source_unresolved
        .into_iter()
        .filter(|u| {
            let k = canonical_diag_key(u);
            !k.is_empty() && !resolved_keys.contains(&k)
        })
        .collect();

    if active_unresolved.is_empty() && entry_opt.map_or(true, |e| e.attempts.is_empty()) {
        return String::new();
    }

    let mut out = String::from("\n--- CHAIN OF INFORMATION (UNRESOLVED ERRORS & ATTEMPT HISTORY) ---\n");
    if !resolved_keys.is_empty() {
        out.push_str(&format!(
            "Note: {} earlier error(s) in this file are ALREADY RESOLVED & verified in ledger (omitted from this prompt — do NOT undo prior imports/fixes).\n",
            resolved_keys.len()
        ));
    }

    if !active_unresolved.is_empty() {
        out.push_str("UNRESOLVED DIAGNOSTICS REQUIRING FIX NOW:\n");
        for err in active_unresolved.iter().take(12) {
            out.push_str(&format!("  - {}\n", err));
        }
    }

    if let Some(entry) = entry_opt {
        if !entry.attempts.is_empty() {
            out.push_str("PRIOR ATTEMPTS IN THIS FILE:\n");
            for att in entry.attempts.iter().rev().take(3).rev() {
                out.push_str(&format!(
                    "  * Round {} [{} -> {}]: {}\n",
                    att.round, att.source, att.outcome, att.summary
                ));
            }
        }
    }

    if attempt >= 2 {
        out.push_str(
            "DEEP CAUSAL REFLECTION REQUIRED BEFORE PROPOSING EDITS:\n\
             1. Why did the previous attempt fail, trigger rollback, or leave remaining diagnostics?\n\
             2. Did the previous edit depend on a missing/unverified symbol, wrong npm subpath, or broken exporter file?\n\
             3. Apply the minimal verified fix (or multi-file exporter fix) targeting ONLY the unresolved diagnostics above without breaking anything already working.\n",
        );
    }
    out.push_str("-------------------------------------------------------------------\n");
    out
}

pub fn infer_app_version_from_timestamp(created_at: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(created_at.trim()) else {
        return env!("CARGO_PKG_VERSION").to_string();
    };
    let ts = dt.timestamp();
    let cutoff = |iso: &str| -> i64 {
        chrono::DateTime::parse_from_rfc3339(iso)
            .map(|d| d.timestamp())
            .unwrap_or(i64::MAX)
    };
    if ts < cutoff("2026-11-09T14:16:00+01:00") {
        "0.1.42".to_string()
    } else if ts < cutoff("2026-11-09T21:31:00+01:00") {
        "0.1.43".to_string()
    } else if ts < cutoff("2026-11-10T07:00:00+01:00") {
        "0.1.44".to_string()
    } else if ts < cutoff("2026-11-10T11:21:00+01:00") {
        "0.1.45".to_string()
    } else if ts < cutoff("2026-11-10T14:52:00+01:00") {
        "0.1.46".to_string()
    } else if ts < cutoff("2026-11-10T17:35:00+01:00") {
        "0.1.47".to_string()
    } else if ts < cutoff("2026-11-11T12:33:00+01:00") {
        "0.1.48".to_string()
    } else if ts < cutoff("2026-11-11T14:40:00+01:00") {
        "0.1.49".to_string()
    } else if ts < cutoff("2026-11-11T21:59:00+01:00") {
        "0.1.50".to_string()
    } else {
        "0.1.51".to_string()
    }
}

fn semver_sort_key(ver: &str) -> (u32, u32, u32, String) {
    let clean = ver.trim().trim_start_matches('v');
    let mut parts = clean.split('.');
    let maj = parts.next().and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
    let min = parts.next().and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
    let pat = parts
        .next()
        .and_then(|p| p.split('-').next())
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(0);
    (maj, min, pat, clean.to_string())
}

fn classify_error_domain(text: &str) -> &'static str {
    let low = text.to_lowercase();
    if low.contains("prisma") || low.contains("schema.prisma") || low.contains("p1012") || low.contains("ts2551") {
        "Prisma & DB"
    } else if low.contains("stripe") || low.contains("webhook") || low.contains("checkout") || low.contains("latestapiversion") || low.contains("constructevent") {
        "Stripe & API"
    } else if low.contains("middleware") || low.contains("path-to-regexp") || low.contains("nextrequest") || low.contains("nextresponse") {
        "Middleware & Edge"
    } else if low.contains("next-auth") || low.contains("nextauth") || low.contains("useSession".to_lowercase().as_str()) || low.contains("/api/") || low.contains("app router") || low.contains("default export") {
        "NextAuth & Routing"
    } else if low.contains("children") || low.contains("jsx") || low.contains("layout.tsx") || low.contains(".tsx") || low.contains("react") || low.contains("chunkloaderror") || low.contains("tailwind") {
        "React UI & JSX"
    } else {
        "Types & Imports"
    }
}

fn extract_error_count_from_warnings(warnings: &[String]) -> usize {
    let mut total = 0usize;
    let re_paren = regex::Regex::new(r"TypeScript greške\s*\((\d+)\)").ok();
    let re_prijavila = regex::Regex::new(r"prijavila\s+(\d+)\s+greš").ok();
    for w in warnings {
        let mut matched_num = None;
        if let Some(ref re) = re_paren {
            if let Some(c) = re.captures(w) {
                matched_num = c.get(1).and_then(|m| m.as_str().parse::<usize>().ok());
            }
        }
        if matched_num.is_none() {
            if let Some(ref re) = re_prijavila {
                if let Some(c) = re.captures(w) {
                    matched_num = c.get(1).and_then(|m| m.as_str().parse::<usize>().ok());
                }
            }
        }
        total += matched_num.unwrap_or(1);
    }
    total
}

fn count_workspace_code_files(proj_dir: &std::path::Path) -> usize {
    let mut count = 0usize;
    let mut stack = vec![proj_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if !matches!(n, "node_modules" | ".next" | ".git" | ".axiom") {
                    stack.push(p);
                }
            } else if p.is_file() {
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                if matches!(ext, "ts" | "tsx" | "js" | "jsx" | "prisma") {
                    count += 1;
                }
            }
        }
    }
    count
}

#[derive(Default)]
struct DomainStatAccum {
    total_errors: usize,
    resolved_heals: usize,
    unresolved_errors: usize,
}

pub fn compute_model_progress_telemetry_at(
    projects_file: &std::path::Path,
    projects_dir: &std::path::Path,
    katastar_path: &std::path::Path,
    settings_path: &std::path::Path,
    model_filter: Option<&str>,
) -> Result<Value, String> {
    let normalized_filter = model_filter
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"));
    let projects_raw = std::fs::read_to_string(projects_file).unwrap_or_else(|_| "[]".to_string());
    let projects_list: Vec<Value> = serde_json::from_str(&projects_raw).unwrap_or_default();

    let katastar_store: KatastarStore = if katastar_path.is_file() {
        std::fs::read_to_string(katastar_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        KatastarStore::default()
    };

    let settings_val: Value = if settings_path.is_file() {
        std::fs::read_to_string(settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let default_model = settings_val
        .get("ollamaModel")
        .or_else(|| settings_val.get("builderModel"))
        .and_then(|v| v.as_str())
        .unwrap_or("qwen2.5-coder:7b")
        .to_string();

    let all_domains = [
        "Prisma & DB",
        "Types & Imports",
        "React UI & JSX",
        "NextAuth & Routing",
        "Stripe & API",
        "Middleware & Edge",
    ];
    let mut domain_map: std::collections::BTreeMap<String, DomainStatAccum> = std::collections::BTreeMap::new();
    for d in all_domains {
        domain_map.insert(d.to_string(), DomainStatAccum::default());
    }

    let mut available_models: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    available_models.insert(default_model.clone());

    let mut project_records: Vec<Value> = Vec::new();
    #[derive(Clone)]
    struct CohortPoint {
        version: String,
        model_id: String,
        f100: f64,
        bug_scope: f64,
        accuracy: f64,
        warnings_count: usize,
        residual_errors: usize,
    }
    let mut points: Vec<CohortPoint> = Vec::new();

    for proj in &projects_list {
        let pid = proj.get("id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if pid.is_empty() {
            continue;
        }
        let name = proj.get("name").and_then(|v| v.as_str()).unwrap_or(&pid).to_string();
        let created_at = proj.get("createdAt").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let proj_dir = proj
            .get("localPath")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| projects_dir.join(&pid));

        let gen_state: Value = {
            let p = proj_dir.join(".axiom/generation_state.json");
            if p.is_file() {
                std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            }
        };

        let heal_ledger: ProjectHealLedger = {
            let p = proj_dir.join(".axiom/heal-ledger.json");
            if p.is_file() {
                std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default()
            } else {
                ProjectHealLedger::default()
            }
        };

        let features_val: Value = {
            let p = proj_dir.join("axiom-features.json");
            if p.is_file() {
                std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            }
        };

        let explicit_ver = proj
            .get("metadata")
            .and_then(|m| m.get("appVersion"))
            .and_then(|v| v.as_str())
            .or_else(|| gen_state.get("appVersion").and_then(|v| v.as_str()))
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string());
        let version = explicit_ver.unwrap_or_else(|| infer_app_version_from_timestamp(&created_at));

        let model_id = proj
            .get("metadata")
            .and_then(|m| m.get("modelId"))
            .and_then(|v| v.as_str())
            .or_else(|| gen_state.get("modelId").and_then(|v| v.as_str()))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&default_model)
            .to_string();
        available_models.insert(model_id.clone());
        if let Some(f) = normalized_filter {
            if !model_id.eq_ignore_ascii_case(f) {
                continue;
            }
        }

        let mut warnings: Vec<String> = proj
            .get("metadata")
            .and_then(|m| m.get("warnings"))
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        if warnings.is_empty() {
            warnings = gen_state
                .get("warnings")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
        }

        let completed_files_len = gen_state
            .get("completedFiles")
            .and_then(|a| a.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let disk_files_len = if proj_dir.is_dir() {
            count_workspace_code_files(&proj_dir)
        } else {
            0
        };
        let total_files = disk_files_len.max(completed_files_len).max(1);

        let mut affected_files: std::collections::HashSet<String> = std::collections::HashSet::new();
        let file_ref_re = regex::Regex::new(r"(?:src/)?[A-Za-z0-9_.\-/\[\]]+\.(?:tsx|ts|jsx|js|prisma)").ok();
        for w in &warnings {
            let dom = classify_error_domain(w);
            if let Some(acc) = domain_map.get_mut(dom) {
                acc.total_errors += 1;
                acc.unresolved_errors += 1;
            }
            if let Some(ref re) = file_ref_re {
                for m in re.find_iter(w) {
                    affected_files.insert(m.as_str().to_lowercase());
                }
            }
        }

        let mut healed_files_count = 0usize;
        let mut ledger_resolved_count = 0usize;
        for (fpath, entry) in &heal_ledger.files {
            if !entry.resolved.is_empty() {
                healed_files_count += 1;
                ledger_resolved_count += entry.resolved.len();
                for res_item in &entry.resolved {
                    let dom = classify_error_domain(&format!("{} {}", fpath, res_item));
                    if let Some(acc) = domain_map.get_mut(dom) {
                        acc.total_errors += 1;
                        acc.resolved_heals += 1;
                    }
                }
            }
            if !entry.unresolved.is_empty() {
                affected_files.insert(fpath.to_lowercase());
                for unres in &entry.unresolved {
                    let dom = classify_error_domain(&format!("{} {}", fpath, unres));
                    if let Some(acc) = domain_map.get_mut(dom) {
                        acc.total_errors += 1;
                        acc.unresolved_errors += 1;
                    }
                }
            }
        }

        let residual_errors = extract_error_count_from_warnings(&warnings);
        let total_features = features_val
            .get("features")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .filter(|&n| n > 0)
            .unwrap_or_else(|| (total_files / 3).max(1));

        let unresolved_file_count = if affected_files.is_empty() && !warnings.is_empty() {
            1
        } else {
            affected_files.len()
        };
        let bug_scope = ((unresolved_file_count as f64) / (total_files as f64)).clamp(0.0, 1.0);
        let clean_first_pass_files = total_files.saturating_sub(healed_files_count + unresolved_file_count);
        let f100 = if warnings.is_empty() && unresolved_file_count == 0 {
            if healed_files_count == 0 {
                1.0
            } else {
                // Features clean at first pass weighted with complete final healing
                (0.45 * (clean_first_pass_files as f64 / total_files as f64) + 0.55).clamp(0.0, 1.0)
            }
        } else {
            let buggy_features = (unresolved_file_count + (residual_errors / 8)).min(total_features);
            ((total_features.saturating_sub(buggy_features)) as f64 / (total_features as f64)).clamp(0.0, 1.0)
        };

        let mu = (residual_errors as f64) / (total_files as f64);
        let phi = 1.0 / (1.0 + 0.25 * mu);
        let accuracy = ((0.55 * f100 + 0.45 * (1.0 - bug_scope)) * phi).clamp(0.0, 1.0);

        points.push(CohortPoint {
            version: version.clone(),
            model_id: model_id.clone(),
            f100,
            bug_scope,
            accuracy,
            warnings_count: warnings.len(),
            residual_errors,
        });

        project_records.push(serde_json::json!({
            "id": pid,
            "name": name,
            "appVersion": version,
            "modelId": model_id,
            "totalFiles": total_files,
            "totalFeatures": total_features,
            "firstPassCleanRatePct": ((clean_first_pass_files as f64 / total_files as f64) * 1000.0).round() / 10.0,
            "f100Pct": (f100 * 1000.0).round() / 10.0,
            "bugScopePct": (bug_scope * 1000.0).round() / 10.0,
            "accuracyPct": (accuracy * 1000.0).round() / 10.0,
            "warningsCount": warnings.len(),
            "residualErrors": residual_errors,
            "healedIssuesCount": ledger_resolved_count,
            "createdAt": created_at,
        }));
    }

    let mut katastar_total_entries = 0usize;
    let mut katastar_verified = 0usize;
    for entry in &katastar_store.entries {
        let entry_model = entry.model_id.trim();
        if !entry_model.is_empty() {
            available_models.insert(entry_model.to_string());
        }
        if let Some(f) = normalized_filter {
            let effective_entry_model = if entry_model.is_empty() {
                default_model.as_str()
            } else {
                entry_model
            };
            if !effective_entry_model.eq_ignore_ascii_case(f) {
                continue;
            }
        }
        katastar_total_entries += 1;
        if entry.status == "verified" {
            katastar_verified += 1;
        }
        let dom = classify_error_domain(&format!(
            "{} {} {}",
            entry.category, entry.error_signature, entry.normalized_sample
        ));
        if let Some(acc) = domain_map.get_mut(dom) {
            acc.total_errors += (entry.success_count + entry.fail_count).max(1) as usize;
            acc.resolved_heals += entry.success_count as usize;
            acc.unresolved_errors += entry.fail_count as usize;
        }
    }

    // Group points by AppVersion and compute Exponential Accuracy Growth Equation
    let mut by_version: std::collections::BTreeMap<(u32, u32, u32, String), Vec<CohortPoint>> =
        std::collections::BTreeMap::new();
    for pt in &points {
        by_version
            .entry(semver_sort_key(&pt.version))
            .or_default()
            .push(pt.clone());
    }

    let mut versions_timeline: Vec<Value> = Vec::new();
    let mut prev_accuracy: Option<f64> = None;
    let mut best_accuracy: Option<f64> = None;
    let mut ema_accuracy: Option<f64> = None;

    for ((_, _, _, ver_str), cohort) in by_version {
        let n = cohort.len().max(1) as f64;
        let mean_acc = cohort.iter().map(|p| p.accuracy).sum::<f64>() / n;
        let mean_f100 = cohort.iter().map(|p| p.f100).sum::<f64>() / n;
        let mean_bug_scope = cohort.iter().map(|p| p.bug_scope).sum::<f64>() / n;
        let total_residual: usize = cohort.iter().map(|p| p.residual_errors).sum();
        let clean_projects = cohort.iter().filter(|p| p.warnings_count == 0).count();

        let delta_vs_prev = prev_accuracy.map(|p| mean_acc - p).unwrap_or(0.0);
        let delta_vs_best = best_accuracy.map(|b| mean_acc - b).unwrap_or(0.0);

        let alpha = 0.45_f64;
        let next_ema = match ema_accuracy {
            Some(prev_ema) => alpha * mean_acc + (1.0 - alpha) * prev_ema,
            None => mean_acc,
        };
        let momentum_boost = 0.35_f64 * delta_vs_best.max(0.0) * (1.0 - next_ema);
        let exp_index = ((next_ema + momentum_boost).clamp(0.0, 1.0)) * 100.0;

        let mut cohort_models: Vec<String> = cohort
            .iter()
            .map(|p| p.model_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        cohort_models.sort();

        versions_timeline.push(serde_json::json!({
            "version": format!("v{}", ver_str),
            "rawVersion": ver_str,
            "projectCount": cohort.len(),
            "cleanProjectsCount": clean_projects,
            "models": cohort_models,
            "rawAccuracyPct": (mean_acc * 1000.0).round() / 10.0,
            "exponentialIndexPct": (exp_index * 10.0).round() / 10.0,
            "f100Pct": (mean_f100 * 1000.0).round() / 10.0,
            "bugScopePct": (mean_bug_scope * 1000.0).round() / 10.0,
            "deltaVsPrevPct": (delta_vs_prev * 1000.0).round() / 10.0,
            "deltaVsBestPct": (delta_vs_best * 1000.0).round() / 10.0,
            "totalResidualErrors": total_residual,
        }));

        prev_accuracy = Some(mean_acc);
        best_accuracy = Some(best_accuracy.map_or(mean_acc, |b| b.max(mean_acc)));
        ema_accuracy = Some(next_ema);
    }

    let sum_domain_errors: usize = domain_map.values().map(|a| a.total_errors).sum::<usize>().max(1);
    let mut domain_breakdown: Vec<Value> = domain_map
        .into_iter()
        .map(|(domain, acc)| {
            let share_pct = (acc.total_errors as f64 / sum_domain_errors as f64) * 100.0;
            let heal_rate = if acc.resolved_heals + acc.unresolved_errors > 0 {
                (acc.resolved_heals as f64 / (acc.resolved_heals + acc.unresolved_errors) as f64) * 100.0
            } else {
                100.0
            };
            let strength_score = (100.0 - share_pct * 0.65 - (100.0 - heal_rate) * 0.35).clamp(0.0, 100.0);
            serde_json::json!({
                "domain": domain,
                "totalErrors": acc.total_errors,
                "resolvedHeals": acc.resolved_heals,
                "unresolvedErrors": acc.unresolved_errors,
                "errorSharePct": (share_pct * 10.0).round() / 10.0,
                "healSuccessPct": (heal_rate * 10.0).round() / 10.0,
                "strengthScore": (strength_score * 10.0).round() / 10.0,
            })
        })
        .collect();
    domain_breakdown.sort_by(|a, b| {
        let sa = a.get("totalErrors").and_then(|v| v.as_u64()).unwrap_or(0);
        let sb = b.get("totalErrors").and_then(|v| v.as_u64()).unwrap_or(0);
        sb.cmp(&sa)
    });

    let latest_summary = versions_timeline.last().cloned().unwrap_or_else(|| {
        serde_json::json!({
            "version": format!("v{}", env!("CARGO_PKG_VERSION")),
            "rawAccuracyPct": 0.0,
            "exponentialIndexPct": 0.0,
            "f100Pct": 0.0,
            "bugScopePct": 0.0,
            "deltaVsPrevPct": 0.0,
            "deltaVsBestPct": 0.0,
        })
    });

    Ok(serde_json::json!({
        "success": true,
        "currentAppVersion": env!("CARGO_PKG_VERSION"),
        "selectedModelFilter": normalized_filter.unwrap_or("all"),
        "defaultModel": default_model,
        "availableModels": available_models.into_iter().collect::<Vec<String>>(),
        "totalProjects": project_records.len(),
        "katastarTotalEntries": katastar_total_entries,
        "katastarVerifiedEntries": katastar_verified,
        "summary": latest_summary,
        "versionsTimeline": versions_timeline,
        "domainBreakdown": domain_breakdown,
        "projects": project_records,
    }))
}

#[tauri::command]
pub fn get_model_progress_telemetry(model_filter: Option<String>) -> Result<Value, String> {
    compute_model_progress_telemetry_at(
        &get_projects_file(),
        &get_projects_dir(),
        &get_katastar_path(),
        &get_base_dir().join("axiom-settings.json"),
        model_filter.as_deref(),
    )
}

pub fn export_mini_llm_training_dataset_at(
    projects_dir: &std::path::Path,
    katastar_path: &std::path::Path,
    output_dir: &std::path::Path,
    model_base: &str,
) -> Result<Value, String> {
    std::fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;
    let mut sft_lines: Vec<String> = Vec::new();
    let system_msg = "You are Axiom-Mini-Coder, a specialized TypeScript, Next.js App Router, Prisma, NextAuth v4, and Stripe engineer. Produce strictly compilable code with zero placeholders or broken imports.";

    let push_sample = |lines: &mut Vec<String>, user_msg: &str, assistant_msg: &str| {
        let u = scrub_sensitive_text(user_msg);
        let a = scrub_sensitive_text(assistant_msg);
        if u.trim().is_empty() || a.trim().is_empty() {
            return;
        }
        let item = serde_json::json!({
            "messages": [
                { "role": "system", "content": system_msg },
                { "role": "user", "content": u },
                { "role": "assistant", "content": a }
            ]
        });
        if let Ok(serialized) = serde_json::to_string(&item) {
            lines.push(serialized);
        }
    };

    // 1. Export Verified Katastar fixes
    if katastar_path.is_file() {
        if let Ok(raw) = std::fs::read_to_string(katastar_path) {
            if let Ok(store) = serde_json::from_str::<KatastarStore>(&raw) {
                for entry in store.entries.iter().filter(|e| e.status == "verified" || e.success_count > e.fail_count) {
                    let user_prompt = format!(
                        "Fix verified {} diagnostic in role `{}`:\nSignature: {}\nSample:\n{}",
                        entry.category, entry.file_role, entry.error_signature, entry.normalized_sample
                    );
                    let assistant_resp = if let Some(ref patch) = entry.patch {
                        format!(
                            "Apply atomic fix ({}):\nSearch:\n```ts\n{}\n```\nReplace:\n```ts\n{}\n```\nRule: {}",
                            entry.strategy, patch.search, patch.replace, entry.hint
                        )
                    } else {
                        format!("Verified remediation rule ({}): {}", entry.strategy, entry.hint)
                    };
                    push_sample(&mut sft_lines, &user_prompt, &assistant_resp);
                }
            }
        }
    }

    // 2. Export verified heal-ledger resolutions across generated workspaces
    if projects_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(projects_dir) {
            for ent in entries.flatten() {
                let p = ent.path();
                let ledger_path = p.join(".axiom/heal-ledger.json");
                if ledger_path.is_file() {
                    if let Ok(raw) = std::fs::read_to_string(&ledger_path) {
                        if let Ok(ledger) = serde_json::from_str::<ProjectHealLedger>(&raw) {
                            for (fpath, frec) in ledger.files {
                                if !frec.resolved.is_empty() {
                                    let resolved_list = frec.resolved.iter().take(6).cloned().collect::<Vec<_>>().join("\n- ");
                                    let summary_att = frec
                                        .attempts
                                        .iter()
                                        .rev()
                                        .find(|a| a.outcome == "verified" || a.outcome == "partial_progress")
                                        .map(|a| a.summary.as_str())
                                        .unwrap_or("Resolved diagnostics cleanly.");
                                    push_sample(
                                        &mut sft_lines,
                                        &format!("Repair `{}` diagnostics:\n- {}", fpath, resolved_list),
                                        &format!("Verified resolution applied on `{}`: {}", fpath, summary_att),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Seed high-signal Axiom Core Stack Rulepacks for mini-LLM distillation
    let core_lessons = [
        (
            "Generate a valid Next.js App Router RootLayout (`src/app/layout.tsx`) with Navbar, Footer, and NextAuth.",
            "Wrap `{children}` inside `<SessionProvider><Navbar /><main className=\"flex-1 flex flex-col w-full\">{children}</main><Footer /></SessionProvider>` inside `<html><body>...</body></html>`. NEVER embed dashboard tables or page KPI cards inside RootLayout.",
        ),
        (
            "Fix TS2322 under \"strict\": false when passing Zod parsed data into prisma.<model>.create({ data }).",
            "Construct an explicit typed `data: { ... }` payload assigning every required Prisma field with explicit fallbacks/defaults and `z.nativeEnum(EnumName)` for enums; never spread `validatedBody` directly into Prisma `create`.",
        ),
        (
            "Write a resource-optimized Next.js Edge Middleware (`src/middleware.ts`).",
            "Layer A: Exclude `/api`, `/_next/static`, `/_next/image`, `/_vercel`, `favicon.ico`, and `.*\\..*` inside `config.matcher` (never use `/*` wildcards). Layer B: Keep the middleware body free of duplicate static checks, bypass Stripe webhooks, check maintenance/i18n env flags before auth, and use `NextResponse.rewrite` instead of `redirect` for internal API rewrites.",
        ),
    ];
    for (u, a) in core_lessons {
        push_sample(&mut sft_lines, u, a);
    }

    let dataset_path = output_dir.join("axiom_sft_dataset.jsonl");
    let modelfile_path = output_dir.join("Modelfile");

    std::fs::write(&dataset_path, format!("{}\n", sft_lines.join("\n"))).map_err(|e| e.to_string())?;

    let modelfile_content = format!(
        "FROM {}\nPARAMETER temperature 0.15\nPARAMETER top_p 0.9\nPARAMETER num_ctx 16384\nSYSTEM \"\"\"{}\"\"\"\n",
        model_base.trim(),
        system_msg
    );
    std::fs::write(&modelfile_path, modelfile_content).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "success": true,
        "sampleCount": sft_lines.len(),
        "datasetPath": dataset_path.to_string_lossy().replace('\\', "/"),
        "modelfilePath": modelfile_path.to_string_lossy().replace('\\', "/"),
        "ollamaCommand": format!("ollama create axiom-mini-coder -f \"{}\"", modelfile_path.to_string_lossy().replace('\\', "/")),
    }))
}

#[tauri::command]
pub fn export_mini_llm_training_dataset(
    output_dir: Option<String>,
    model_base: Option<String>,
) -> Result<Value, String> {
    let dir = output_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| get_base_dir().join("training-exports"));
    let base_model = model_base
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "qwen2.5-coder:3b".to_string());
    export_mini_llm_training_dataset_at(&get_projects_dir(), &get_katastar_path(), &dir, &base_model)
}
