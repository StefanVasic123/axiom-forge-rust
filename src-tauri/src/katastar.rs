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

        let core_line = cleaned
            .lines()
            .map(|l| l.trim())
            .find(|l| {
                l.starts_with("error:")
                    || l.contains("Error validating:")
                    || l.contains("Error parsing")
                    || l.contains("Validation Error")
            })
            .or_else(|| cleaned.lines().map(|l| l.trim()).find(|l| !l.is_empty()))
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
        let error_code = ["ETARGET", "E404", "ERESOLVE", "EPERM", "EACCES", "ENOTCACHED"]
            .iter()
            .find(|code| cleaned.contains(**code))
            .map(|s| (*s).to_string());

        let pkg_hint = if let Some(idx) = cleaned.find("No matching version found for ") {
            let tok = cleaned[idx + 30..].split_whitespace().next().unwrap_or("");
            tok.rsplit_once('@').map(|(name, _)| name).unwrap_or(tok).trim_matches(|c| c == '.' || c == '\'' || c == '"').to_string()
        } else if let Some(idx) = cleaned.find("requested resource '") {
            let inner = cleaned[idx + 20..].split('\'').next().unwrap_or("");
            inner.rsplit_once('@').map(|(name, _)| name).unwrap_or(inner).to_string()
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
            .find(|l| l.contains("npm error") || l.contains("npm ERR!") || l.contains("notarget") || !l.is_empty())
            .unwrap_or("npm install error");
        return ErrorSignatureInfo {
            category,
            error_code,
            error_signature: sig,
            normalized_sample: truncate_chars(first_err_line, 220),
        };
    }

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

    if lower.contains("module not found: can't resolve") || cleaned.contains("TS2307") {
        let missing_mod = regex::Regex::new(r#"(?:Can't resolve|Cannot find module)\s+['"]([^'"]+)['"]"#)
            .ok()
            .and_then(|re| re.captures(&cleaned))
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        let code = if cleaned.contains("TS2307") { "TS2307" } else { "MODULE_NOT_FOUND" };
        let sig = if missing_mod.starts_with('.') || missing_mod.starts_with("@/") {
            format!("compile:{}:local-import", code)
        } else if !missing_mod.is_empty() {
            format!("compile:{}:{}", code, to_slug(&missing_mod, 48))
        } else {
            format!("compile:{}:cannot-find-module", code)
        };
        let sample = cleaned
            .lines()
            .map(|l| l.trim())
            .find(|l| l.contains("Can't resolve") || l.contains("Cannot find module") || !l.is_empty())
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
        .find(|l| {
            let ll = l.to_lowercase();
            (ll.contains("error") || ll.contains("failed") || ll.contains("syntaxerror") || ll.contains("typeerror"))
                && l.len() > 6
        })
        .or_else(|| cleaned.lines().map(|l| l.trim()).find(|l| l.len() > 4))
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
