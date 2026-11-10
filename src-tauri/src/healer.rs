use crate::*;

fn normalize_rel_path_str(raw: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for seg in raw.replace('\\', "/").split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s.to_string()),
        }
    }
    parts.join("/")
}

pub fn resolve_import_path(proj_dir: &std::path::Path, import_path: &str, current_file: &str) -> Option<String> {
    let clean_path = import_path.trim().replace('\'', "").replace('"', "");
    let mut resolved = None;

    if let Some(suffix) = clean_path.strip_prefix("@/").or_else(|| clean_path.strip_prefix("~/")) {
        let test_src = proj_dir.join("src").join(suffix);
        if test_src.exists()
            || proj_dir.join("src").join(format!("{}.tsx", suffix)).exists()
            || proj_dir.join("src").join(format!("{}.ts", suffix)).exists()
            || proj_dir.join("src").join(format!("{}.jsx", suffix)).exists()
            || proj_dir.join("src").join(format!("{}.js", suffix)).exists()
        {
            resolved = Some(format!("src/{}", suffix));
        } else {
            resolved = Some(suffix.to_string());
        }
    } else if clean_path.starts_with("src/") {
        resolved = Some(clean_path.clone());
    } else if clean_path.starts_with('.') {
        if let Some(parent) = std::path::Path::new(current_file).parent() {
            let path = parent.join(&clean_path);
            resolved = path.to_str().map(normalize_rel_path_str);
        }
    }

    if let Some(path_str) = resolved {
        let normalized = normalize_rel_path_str(&path_str);
        if proj_dir.join(&normalized).is_file() {
            return Some(normalized);
        }
        for ext in &["tsx", "ts", "jsx", "js"] {
            let test_file = format!("{}.{}", normalized, ext);
            if proj_dir.join(&test_file).is_file() {
                return Some(test_file);
            }
            let index_file = format!("{}/index.{}", normalized, ext);
            if proj_dir.join(&index_file).is_file() {
                return Some(index_file);
            }
        }
        return Some(normalized);
    }
    None
}

pub fn extract_related_project_files_for_error(
    proj_dir: &std::path::Path,
    file_content: &str,
    current_file: &str,
    error_msg: &str,
) -> Vec<(String, String)> {
    use std::collections::HashSet;
    let mut resolved_paths = HashSet::new();
    let mut results = Vec::new();

    // 1. Scan for all quoted tokens in error_msg ('...', "...", `...`)
    let mut quoted_tokens = Vec::new();
    let chars: Vec<char> = error_msg.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' || c == '`' {
            let quote = c;
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != quote {
                end += 1;
            }
            if end < chars.len() {
                let token: String = chars[start..end].iter().collect();
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    quoted_tokens.push(trimmed.to_string());
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    // Check each quoted token as a direct or relative import path
    for token in &quoted_tokens {
        if token.starts_with("@/") || token.starts_with("./") || token.starts_with("../") || token.contains('/') {
            if let Some(resolved) = resolve_import_path(proj_dir, token, current_file) {
                if resolved != current_file && proj_dir.join(&resolved).is_file() {
                    resolved_paths.insert(resolved);
                }
            }
        }
    }

    // 2. Parse all imports declared in the failing file
    // Matches: import ... from '...'; import '...'; const ... = require('...');
    if let Ok(re) = regex::Regex::new(r#"(?m)(?:import\s+(?:(?:(\{[^}]+\})|([a-zA-Z0-9_$]+)|(?:\*\s+as\s+[a-zA-Z0-9_$]+))\s+from\s+)?['"]([^'"]+)['"]"#) {
        for cap in re.captures_iter(file_content) {
            let import_specifier = cap.get(3).map(|m| m.as_str()).unwrap_or("");
            if import_specifier.is_empty() {
                continue;
            }

            // Extract imported symbols (e.g. named imports { A, B } or default import Navbar)
            let mut imported_symbols = Vec::new();
            if let Some(named) = cap.get(1) {
                for sym in named.as_str().trim_matches(|c| c == '{' || c == '}').split(',') {
                    let sym_clean = sym.trim().split_whitespace().last().unwrap_or("").trim();
                    if !sym_clean.is_empty() {
                        imported_symbols.push(sym_clean.to_string());
                    }
                }
            }
            if let Some(default_sym) = cap.get(2) {
                let sym_clean = default_sym.as_str().trim();
                if !sym_clean.is_empty() {
                    imported_symbols.push(sym_clean.to_string());
                }
            }

            // Check if error_msg mentions the import specifier OR any of its imported symbols
            let mentions_specifier = !import_specifier.is_empty() && error_msg.contains(import_specifier);
            let mentions_symbol = imported_symbols.iter().any(|sym| error_msg.contains(sym));

            if mentions_specifier || mentions_symbol {
                if let Some(resolved) = resolve_import_path(proj_dir, import_specifier, current_file) {
                    if resolved != current_file && proj_dir.join(&resolved).is_file() {
                        resolved_paths.insert(resolved);
                    }
                }
            }
        }
    }

    // 3. For any identifier in quoted_tokens, check if a file named <token>.tsx, <token>.ts, etc. exists in the project
    for token in &quoted_tokens {
        if !token.contains('/') && !token.contains(' ') && token.len() >= 3 {
            for sub in &[
                "src/components", "components",
                "src/hooks", "hooks",
                "src/lib", "lib",
                "src/utils", "utils",
                "src/models", "models",
                "src/types", "types",
                "src/services", "services",
            ] {
                for ext in &["tsx", "ts", "jsx", "js"] {
                    let candidate = format!("{}/{}.{}", sub, token, ext);
                    if proj_dir.join(&candidate).is_file() && candidate != current_file {
                        resolved_paths.insert(candidate);
                    }
                }
            }
        }
    }

    // 4. Read contents of all resolved related files
    for path_str in resolved_paths {
        let full_path = proj_dir.join(&path_str);
        if let Ok(content) = std::fs::read_to_string(&full_path) {
            results.push((path_str, content));
        }
    }

    results
}

/// Cleans CLI output for the LLM: removes real ANSI escape sequences (via the existing
/// `strip_ansi_codes`) and also bare leftovers whose ESC byte was lost (e.g. `[0m`).
pub fn clean_cli_output_for_llm(input: &str) -> String {
    let without_esc = strip_ansi_codes(input);
    match regex::Regex::new(r"\[[0-9;]+m") {
        Ok(re) => re.replace_all(&without_esc, "").to_string(),
        Err(_) => without_esc,
    }
}

/// Extracts the exact schema lines referenced by Prisma CLI errors (`schema.prisma:31`)
/// and returns them as `Line N: <content>` so a small model gets a focused target.
pub fn extract_prisma_error_lines(error_log: &str, schema_content: &str) -> String {
    let re = match regex::Regex::new(r"schema\.prisma:(\d+)") {
        Ok(r) => r,
        Err(_) => return String::new(),
    };
    let schema_lines: Vec<&str> = schema_content.lines().collect();
    let mut seen: Vec<usize> = Vec::new();
    let mut out = String::new();
    for cap in re.captures_iter(error_log) {
        if let Some(n) = cap.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) {
            if n == 0 || seen.contains(&n) {
                continue;
            }
            seen.push(n);
            if let Some(line) = schema_lines.get(n - 1) {
                out.push_str(&format!("Line {}: {}\n", n, line.trim_end()));
            }
        }
    }
    out
}

/// Finds a top-level Prisma block (`model X { ... }`, `enum X { ... }`) and returns its
/// byte range `[start, end)` including the closing brace.
pub fn find_prisma_block(schema: &str, kind: &str, name: &str) -> Option<(usize, usize)> {
    let pattern = format!(r"(?m)^[ \t]*{}[ \t]+{}[ \t]*\{{", kind, regex::escape(name));
    let re = regex::Regex::new(&pattern).ok()?;
    let m = re.find(schema)?;
    let rest = &schema[m.end()..];

    // Prefer a closing brace on its own line (robust against `{}` inside defaults)
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        if line.trim() == "}" {
            if let Some(brace_idx) = line.find('}') {
                return Some((m.start(), m.end() + offset + brace_idx + 1));
            }
        }
        offset += line.len();
    }
    let close_rel = rest.find('}')?;
    Some((m.start(), m.end() + close_rel + 1))
}

/// Replaces NextAuth `Account` / `Session` models with the canonical Prisma Adapter
/// definitions (including `@@unique([provider, providerAccountId])`) and makes sure the
/// `User` model has the matching back-relation fields. Only applies when the models look
/// like NextAuth models and a `User` model exists. Provider-agnostic (no `@db.Text`),
/// so it stays valid after the SQLite fallback.
pub fn apply_nextauth_prisma_template(schema: &str) -> String {
    let (us, ue) = match find_prisma_block(schema, "model", "User") {
        Some(b) => b,
        None => return schema.to_string(),
    };

    // Foreign keys must match the type of User.id (String / Int / BigInt)
    let mut user_id_type = "String".to_string();
    for line in schema[us..ue].lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0] == "id" {
            user_id_type = parts[1].trim_end_matches('?').to_string();
            break;
        }
    }

    let account_tpl = format!(
        "model Account {{\n  id                String  @id @default(cuid())\n  userId            {t}\n  type              String\n  provider          String\n  providerAccountId String\n  refresh_token     String?\n  access_token      String?\n  expires_at        Int?\n  token_type        String?\n  scope             String?\n  id_token          String?\n  session_state     String?\n  user              User    @relation(fields: [userId], references: [id], onDelete: Cascade)\n\n  @@unique([provider, providerAccountId])\n}}",
        t = user_id_type
    );
    let session_tpl = format!(
        "model Session {{\n  id           String   @id @default(cuid())\n  sessionToken String   @unique\n  userId       {t}\n  expires      DateTime\n  user         User     @relation(fields: [userId], references: [id], onDelete: Cascade)\n}}",
        t = user_id_type
    );

    let mut out = schema.to_string();
    let mut needs_accounts = false;
    let mut needs_sessions = false;

    if let Some((s, e)) = find_prisma_block(&out, "model", "Account") {
        if out[s..e].contains("providerAccountId") {
            out.replace_range(s..e, &account_tpl);
            needs_accounts = true;
        }
    }
    if let Some((s, e)) = find_prisma_block(&out, "model", "Session") {
        if out[s..e].contains("sessionToken") {
            out.replace_range(s..e, &session_tpl);
            needs_sessions = true;
        }
    }

    if needs_accounts || needs_sessions {
        if let Some((s, e)) = find_prisma_block(&out, "model", "User") {
            let block = out[s..e].to_string();
            let has_field_of_type = |ty: &str| {
                block.lines().any(|l| {
                    let p: Vec<&str> = l.split_whitespace().collect();
                    p.len() >= 2 && p[1] == ty
                })
            };

            let mut inserts = String::new();
            if needs_accounts && !has_field_of_type("Account[]") {
                inserts.push_str("  accounts      Account[]\n");
            }
            if needs_sessions && !has_field_of_type("Session[]") {
                inserts.push_str("  sessions      Session[]\n");
            }

            if !inserts.is_empty() {
                // Insert before the first block attribute (@@...) or before the closing brace
                let mut insert_at: Option<usize> = None;
                let mut offset = 0usize;
                for line in block.split_inclusive('\n') {
                    if line.trim_start().starts_with("@@") {
                        insert_at = Some(offset);
                        break;
                    }
                    offset += line.len();
                }
                let mut new_block = block.clone();
                match insert_at {
                    Some(pos) => new_block.insert_str(pos, &inserts),
                    None => {
                        if let Some(close) = new_block.rfind('}') {
                            let prefix_ends_with_newline = new_block[..close].ends_with('\n');
                            let to_insert = if prefix_ends_with_newline {
                                inserts.clone()
                            } else {
                                format!("\n{}", inserts)
                            };
                            new_block.insert_str(close, &to_insert);
                        }
                    }
                }
                out.replace_range(s..e, &new_block);
            }
        }
    }

    out
}

/// Runs the deterministic schema.prisma sanitizer on the file on disk.
/// Returns `true` if the file was changed.
pub fn sanitize_prisma_schema_on_disk(schema_path: &std::path::Path) -> bool {
    if let Ok(current) = std::fs::read_to_string(schema_path) {
        if current.trim().is_empty() {
            return false;
        }
        let fixed = post_process_generated_file("prisma/schema.prisma", &current, &serde_json::json!({}), false);
        if !fixed.trim().is_empty() && fixed.replace("\r\n", "\n").trim() != current.replace("\r\n", "\n").trim() {
            return std::fs::write(schema_path, &fixed).is_ok();
        }
    }
    false
}

pub async fn heal_prisma_schema_loop(
    proj_dir: &std::path::Path,
    window: &tauri::Window,
    model_id: &str,
    log_channel: &str,
) -> Result<(), String> {
    let schema_path = if proj_dir.join("prisma/schema.prisma").exists() {
        proj_dir.join("prisma/schema.prisma")
    } else if proj_dir.join("schema.prisma").exists() {
        proj_dir.join("schema.prisma")
    } else {
        return Ok(()); // No schema, nothing to heal
    };

    let enriched_path = ensure_node_toolchain_ready(Some(window), log_channel).await;

    // (1) Deterministic pre-pass: fix known schema mistakes without the LLM
    if sanitize_prisma_schema_on_disk(&schema_path) {
        let _ = window.emit(log_channel, serde_json::json!({
            "text": "> [Prisma Validator] Deterministički popravljač je ispravio poznate greške u schema.prisma (pre AI-ja).\n",
            "type": "info"
        }));
    }

    const MAX_AI_HEALS: u32 = 3;
    let total_checks = MAX_AI_HEALS + 1;
    let mut previous_ai_unchanged = false;
    let project_id = proj_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    struct PendingPrismaFix {
        entry_id: String,
        source: String,
        error_signature: String,
        before: String,
        after: String,
    }
    let mut pending_prisma_fix: Option<PendingPrismaFix> = None;
    let mut pending_ai_fixes: Vec<PendingPrismaFix> = Vec::new();
    let mut tried_katastar_ids: Vec<String> = Vec::new();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let _ = ollama_start_server().await;
    let resolved_model = resolve_valid_ollama_model(&client, model_id).await;

    // (4) attempt 0..=MAX_AI_HEALS -> the schema is re-validated after EVERY AI heal,
    // including the last one.
    for attempt in 0..=MAX_AI_HEALS {
        let _ = window.emit(log_channel, serde_json::json!({
            "text": format!("> [Prisma Validator] Provera i generisanje Prisma klijenta (provera {}/{})...\n", attempt + 1, total_checks),
            "type": "info"
        }));

        let local_prisma_js = proj_dir.join("node_modules").join("prisma").join("build").join("index.js");
        if local_prisma_js.exists() {
            #[cfg(target_os = "windows")]
            let mut fmt_cmd = tokio::process::Command::new("node");
            #[cfg(target_os = "windows")]
            {
                fmt_cmd.arg(&local_prisma_js).arg("format").current_dir(proj_dir);
                fmt_cmd.creation_flags(0x08000000);
            }
            #[cfg(not(target_os = "windows"))]
            let mut fmt_cmd = tokio::process::Command::new("node");
            #[cfg(not(target_os = "windows"))]
            fmt_cmd.arg(&local_prisma_js).arg("format").current_dir(proj_dir);
            let _ = fmt_cmd.output().await;
        }
        let mut prisma_cmd = if local_prisma_js.exists() {
            let mut cmd = tokio::process::Command::new("node");
            cmd.arg(&local_prisma_js).arg("generate").current_dir(proj_dir);
            cmd
        } else {
            #[cfg(target_os = "windows")]
            let mut cmd = tokio::process::Command::new("cmd");
            #[cfg(target_os = "windows")]
            cmd.arg("/C").arg("npx").arg("-y").arg("prisma@5.22.0").arg("generate").current_dir(proj_dir);

            #[cfg(not(target_os = "windows"))]
            let mut cmd = tokio::process::Command::new("npx");
            #[cfg(not(target_os = "windows"))]
            cmd.arg("-y").arg("prisma@5.22.0").arg("generate").current_dir(proj_dir);
            cmd
        };

        #[cfg(target_os = "windows")]
        {
            prisma_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        // Disable colored output at the source as well and inject enriched PATH
        prisma_cmd.env("PATH", &enriched_path).env("NO_COLOR", "1").env("FORCE_COLOR", "0");

        let output = match prisma_cmd.output().await {
            Ok(o) => o,
            Err(e) => return Err(format!("Failed to execute prisma generate: {}", e)),
        };

        if output.status.success() {
            if let Some(pf) = pending_prisma_fix.take() {
                let atomic_patch = if pf.source == "katastar" {
                    None
                } else {
                    crate::katastar::extract_safe_atomic_patch("prisma/schema.prisma", &pf.before, &pf.after)
                };
                crate::katastar::finalize_katastar_outcome(
                    &pf.entry_id,
                    true,
                    &pf.source,
                    "prisma_generate",
                    atomic_patch,
                    None,
                    &resolved_model,
                );
            }
            for pf in pending_ai_fixes.drain(..) {
                let atomic_patch = crate::katastar::extract_safe_atomic_patch(
                    "prisma/schema.prisma",
                    &pf.before,
                    &pf.after,
                );
                crate::katastar::finalize_katastar_outcome(
                    &pf.entry_id,
                    true,
                    "ai",
                    "prisma_generate",
                    atomic_patch,
                    None,
                    &resolved_model,
                );
            }
            if !project_id.is_empty() {
                crate::katastar::complete_pending_project_repairs(
                    &project_id,
                    Some("prisma/schema.prisma"),
                    true,
                    "prisma_generate",
                    &resolved_model,
                );
            }
            let _ = window.emit(log_channel, serde_json::json!({
                "text": "> [Prisma Validator] Prisma Client uspešno generisan!\n",
                "type": "info"
            }));
            return Ok(());
        }

        // (2) Clean ANSI escape codes before logging / sending to the model
        let error_log = clean_cli_output_for_llm(&format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));

        let _ = window.emit(log_channel, serde_json::json!({
            "text": format!("> [Prisma Validator] Greška pri generisanju:\n{}\n", error_log),
            "type": "error"
        }));

        if error_log.contains("is not recognized as an internal or external command")
            || error_log.contains("command not found")
        {
            return Err("Node.js (npx) nije pronađen u sistemu. Proverite internet konekciju za automatsko preuzimanje Node.js-a.".to_string());
        }

        if error_log.contains("CLI.UNKNOWN_COMMAND")
            || error_log.contains("No command registered for")
            || error_log.contains("The \"path\" argument must be of type string")
        {
            return Err("Neodgovarajuća verzija Prisma CLI paketa u kešu u odnosu na @prisma/client.".to_string());
        }

        let cur_sig_info = crate::katastar::normalize_error_signature(
            "prisma",
            &error_log,
            Some("prisma/schema.prisma"),
        );

        // Post-test verification for deterministic or Katastar rule applied on the previous iteration
        if let Some(pf) = pending_prisma_fix.take() {
            if cur_sig_info.error_signature == pf.error_signature {
                crate::katastar::finalize_katastar_outcome(
                    &pf.entry_id,
                    false,
                    &pf.source,
                    "prisma_generate",
                    None,
                    None,
                    &resolved_model,
                );
                if pf.source == "katastar" {
                    let _ = std::fs::write(&schema_path, &pf.before);
                    tried_katastar_ids.push(pf.entry_id.clone());
                    let _ = window.emit(log_channel, serde_json::json!({
                        "text": format!("> [Katastar] Pravilo '{}' nije uklonilo grešku u schema.prisma; vraćam prethodno stanje i nastavljam.\n", pf.entry_id),
                        "type": "warning"
                    }));
                }
            } else {
                let atomic_patch = if pf.source == "katastar" {
                    None
                } else {
                    crate::katastar::extract_safe_atomic_patch("prisma/schema.prisma", &pf.before, &pf.after)
                };
                crate::katastar::finalize_katastar_outcome(
                    &pf.entry_id,
                    true,
                    &pf.source,
                    "error_disappeared",
                    atomic_patch,
                    None,
                    &resolved_model,
                );
            }
        }

        // If the most recent AI fix produced the exact same error signature, mark it failed immediately
        if let Some(last_ai) = pending_ai_fixes.last() {
            if cur_sig_info.error_signature == last_ai.error_signature {
                if let Some(failed_ai) = pending_ai_fixes.pop() {
                    crate::katastar::finalize_katastar_outcome(
                        &failed_ai.entry_id,
                        false,
                        "ai",
                        "prisma_generate",
                        None,
                        None,
                        &resolved_model,
                    );
                }
            }
        }

        if attempt == MAX_AI_HEALS {
            break;
        }

        let schema_content = match std::fs::read_to_string(&schema_path) {
            Ok(c) => c,
            Err(_) => break,
        };

        // Deterministic auto-healing for unsupported enums connector error (P1012)
        if error_log.contains("does not support enums") || error_log.contains("current connector does not support enums") {
            if schema_content.contains("provider = \"sqlite\"") {
                let updated = schema_content.replace("provider = \"sqlite\"", "provider = \"postgresql\"");
                let _ = std::fs::write(&schema_path, &updated);
                let eid = crate::katastar::register_repair_attempt(
                    "prisma",
                    &error_log,
                    "prisma/schema.prisma",
                    "deterministic",
                    "prisma_provider_pg",
                    None,
                    "Switch Prisma datasource provider from sqlite to postgresql when enum types are declared",
                    "",
                );
                pending_prisma_fix = Some(PendingPrismaFix {
                    entry_id: eid,
                    source: "deterministic".to_string(),
                    error_signature: cur_sig_info.error_signature.clone(),
                    before: schema_content,
                    after: updated,
                });
                let _ = window.emit(log_channel, serde_json::json!({
                    "text": "> [Prisma Validator] Deterministički sinhronizujem provider = \"postgresql\" za podršku enum tipova. Ponovo testiram...\n",
                    "type": "info"
                }));
                continue;
            }
        }

        // Deterministic auto-healing for schema validation & relation errors (P1012, SET_NULL, duplicate back-relations)
        if error_log.contains("Ambiguous relation detected")
            || error_log.contains("Wrongly named relation detected")
            || error_log.contains("Invalid referential action")
            || error_log.contains("Error validating")
            || error_log.contains("P1012")
        {
            let deterministically_fixed = post_process_generated_file(
                "prisma/schema.prisma",
                &schema_content,
                &serde_json::json!({}),
                false,
            );
            if !deterministically_fixed.trim().is_empty()
                && deterministically_fixed.replace("\r\n", "\n").trim() != schema_content.replace("\r\n", "\n").trim()
            {
                let _ = std::fs::write(&schema_path, &deterministically_fixed);
                let atomic_patch = crate::katastar::extract_safe_atomic_patch(
                    "prisma/schema.prisma",
                    &schema_content,
                    &deterministically_fixed,
                );
                let eid = crate::katastar::register_repair_attempt(
                    "prisma",
                    &error_log,
                    "prisma/schema.prisma",
                    "deterministic",
                    "deterministic_sanitizer",
                    atomic_patch,
                    "Apply deterministic Prisma referential action and relation pairing sanitizer",
                    "",
                );
                pending_prisma_fix = Some(PendingPrismaFix {
                    entry_id: eid,
                    source: "deterministic".to_string(),
                    error_signature: cur_sig_info.error_signature.clone(),
                    before: schema_content,
                    after: deterministically_fixed,
                });
                let _ = window.emit(log_channel, serde_json::json!({
                    "text": "> [Prisma Validator] Deterministički popravljena schema.prisma (relacije / referencijalne akcije). Ponovo testiram...\n",
                    "type": "info"
                }));
                continue;
            }
        }

        // Phase A (Before calling AI): Consult Local Error Katastar for verified Prisma fixes & hints
        let katastar_lookup = crate::katastar::lookup_katastar(
            "prisma",
            &error_log,
            "prisma/schema.prisma",
            &schema_content,
            &tried_katastar_ids,
        );
        if let Some(best) = katastar_lookup.applicable_patches.first() {
            let candidate = schema_content.replace(&best.patch.search, &best.patch.replace);
            let sanitized_candidate = post_process_generated_file(
                "prisma/schema.prisma",
                &candidate,
                &serde_json::json!({}),
                false,
            );
            if !sanitized_candidate.trim().is_empty()
                && sanitized_candidate.replace("\r\n", "\n").trim() != schema_content.replace("\r\n", "\n").trim()
            {
                let _ = std::fs::write(&schema_path, &sanitized_candidate);
                pending_prisma_fix = Some(PendingPrismaFix {
                    entry_id: best.entry_id.clone(),
                    source: "katastar".to_string(),
                    error_signature: cur_sig_info.error_signature.clone(),
                    before: schema_content,
                    after: sanitized_candidate,
                });
                let _ = window.emit(log_channel, serde_json::json!({
                    "text": format!("> [Katastar] Primenjeno verifikovano pravilo '{}' na schema.prisma pre AI poziva. Ponovo testiram...\n", best.entry_id),
                    "type": "info"
                }));
                continue;
            }
        }

        let _ = window.emit(log_channel, serde_json::json!({
            "text": format!("> [Prisma Validator] Započinjem AI auto-healing za schema.prisma (pokušaj {}/{})...\n", attempt + 1, MAX_AI_HEALS),
            "type": "info"
        }));

        let failing_lines = extract_prisma_error_lines(&error_log, &schema_content);

        let mut system_prompt = "You are an expert database administrator and Prisma specialist. \
            The prisma generate command failed with syntax or validation errors. \
            Review the schema.prisma content and the error log. Correct the schema.prisma file to fix the validation errors. \
            Strictly follow Prisma Schema language rules: \
            1. Optional/nullable relation fields must only have ? on their type (e.g. password String? but NOT password? String?). \
            2. In relations, ensure fields (like userId) are explicitly declared in the model (e.g. userId String) and matches the relation fields parameter. \
            3. Do not use @db.Json native type annotation on String[] fields. Use standard Prisma types. \
            4. The @relation attribute (e.g. `@relation(fields: [userId], references: [id])`) MUST ONLY be placed on relational object fields (like `user User`), never on scalar fields (like `userId String`). Placing @relation on a scalar field will throw an 'Invalid field type, not a relation' error. \
            5. For NextAuth schema, the VerificationToken model does not have a direct relation back to the User model; do not define a verificationTokens relation field on the User model. \
            6. Every relation needs an opposite field on the other model (e.g. `user User @relation(...)` on the child and `dashboards Dashboard[]` on User). If the relation uses a name (e.g. `name: \"TeamMember\"`), the opposite relation array MUST use the exact same name (e.g. `teams Team[] @relation(\"TeamMember\")`). CRITICAL: NEVER delete foreign key scalar fields (like `userId String` or `projectId String`). Always ADD the opposite relation field instead! \
            7. Fields pointing to another model (relations) CANNOT have a `@default()` attribute. For default values, use an `enum` or a scalar type (e.g. `String @default(\"USER\")`). \
            8. Ambiguous Relations (P1012): If a model defines two or more relations pointing to the same target model (e.g. `sourceWallet` and `destinationWallet` both pointing to `Wallet`), Prisma strictly requires unique relation names on BOTH models: `@relation(\"SourceWallet\", fields: ...)` and `@relation(\"DestinationWallet\", fields: ...)`. On the target model (Wallet), define matching opposite named relations: `sourceTransactions Transaction[] @relation(\"SourceWallet\")` and `destinationTransactions Transaction[] @relation(\"DestinationWallet\")`. Remove duplicate un-named arrays like `transactions` and `transactionsTransaction`. \
            9. Referential Actions & Unique Relation Names (P1012): Referential actions inside `@relation(...)` (`onDelete`, `onUpdate`) MUST use exact Prisma PascalCase keywords: `SetNull`, `Cascade`, `Restrict`, `NoAction`, `SetDefault` (NEVER uppercase SQL keywords like `SET_NULL`, `CASCADE`, `NO_ACTION`, or `SET_DEFAULT`). Also NEVER keep two opposite relation fields on the same model using the same `@relation(\"...\")` name (e.g., if `tasks` and `tasksTask` both use `@relation(\"AssignedTo\")`, delete the duplicate `tasksTask`). \
            First, in exactly 1 concise sentence, identify the compiler rule that was violated on the failing line. \
            Then, provide the complete corrected schema.prisma code wrapped in a ```prisma code block.".to_string();

        if !katastar_lookup.hints.is_empty() {
            system_prompt.push_str("\n\nPREVIOUSLY VERIFIED PRISMA SOLUTIONS FROM LOCAL KATASTAR:\n");
            for h in katastar_lookup.hints.iter().take(3) {
                system_prompt.push_str(&format!("- {}\n", h));
            }
        }

        let mut user_prompt = format!(
            "Prisma CLI error log:\n{}\n\nCurrent schema.prisma content:\n{}\n",
            error_log, schema_content
        );
        if !failing_lines.is_empty() {
            user_prompt.push_str(&format!(
                "\nThe errors point to these exact lines. You MUST change them:\n{}",
                failing_lines
            ));
        }
        // (3) If the previous answer did not change anything, escalate the instruction
        if previous_ai_unchanged {
            user_prompt.push_str(
                "\nIMPORTANT: Your previous answer returned the schema UNCHANGED and the same errors remained. \
                 Do not repeat it. You MUST directly modify the failing line(s) reported by the compiler to resolve the error. \
                 Address the specific error message provided by the compiler.\n",
            );
        }
        user_prompt.push_str("\nPlease provide the concise 1-sentence analysis followed by the complete corrected schema.prisma code in a ```prisma code block.");

        let ollama_req = serde_json::json!({
            "model": resolved_model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": false,
            "options": build_ollama_options(&resolved_model, if previous_ai_unchanged { 0.4 } else { 0.1 }, Some(4096))
        });

        let mut ai_content: Option<String> = None;
        let mut ai_analysis_hint = cur_sig_info.normalized_sample.clone();
        match client.post("http://127.0.0.1:11434/api/chat").json(&ollama_req).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(resp_json) = resp.json::<Value>().await {
                    if let Some(content) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                        let first_line = content
                            .lines()
                            .map(|l| l.trim())
                            .find(|l| !l.is_empty() && !l.starts_with("```"))
                            .unwrap_or("");
                        if !first_line.is_empty() {
                            ai_analysis_hint = first_line.to_string();
                        }
                        let cleaned = extract_code_content(content);
                        if !cleaned.trim().is_empty() {
                            ai_content = Some(cleaned);
                        }
                    }
                }
            }
            Ok(resp) => {
                debug_log_to_file(format!("[Prisma Validator] Ollama returned HTTP {}", resp.status()));
            }
            Err(e) => {
                debug_log_to_file(format!("[Prisma Validator] Ollama request failed: {}", e));
            }
        }

        match ai_content {
            Some(cleaned) => {
                // Guard against partial LLM responses (e.g. returning only a single `model Payment { ... }` block
                // instead of the full schema.prisma, which would wipe out `datasource db`, `generator client`, and other models).
                let merged_candidate = merge_prisma_ai_response(&schema_content, &cleaned);
                // (1) Never write raw LLM output: run it through the deterministic sanitizer first
                let sanitized = post_process_generated_file("prisma/schema.prisma", &merged_candidate, &serde_json::json!({}), false);
                let unchanged = sanitized.replace("\r\n", "\n").trim() == schema_content.replace("\r\n", "\n").trim();
                if unchanged {
                    previous_ai_unchanged = true;
                    let _ = window.emit(log_channel, serde_json::json!({
                        "text": "> [Prisma Validator] AI je vratio identičnu šemu. Sledeći pokušaj koristi ciljani prompt sa tačnim linijama greške.\n",
                        "type": "warning"
                    }));
                } else {
                    previous_ai_unchanged = false;
                    let _ = std::fs::write(&schema_path, &sanitized);
                    let atomic_patch = crate::katastar::extract_safe_atomic_patch(
                        "prisma/schema.prisma",
                        &schema_content,
                        &sanitized,
                    );
                    let eid = crate::katastar::register_repair_attempt(
                        "prisma",
                        &error_log,
                        "prisma/schema.prisma",
                        "ai",
                        "prisma_heal",
                        atomic_patch,
                        &ai_analysis_hint,
                        &resolved_model,
                    );
                    pending_ai_fixes.push(PendingPrismaFix {
                        entry_id: eid,
                        source: "ai".to_string(),
                        error_signature: cur_sig_info.error_signature.clone(),
                        before: schema_content,
                        after: sanitized,
                    });
                    let _ = window.emit(log_channel, serde_json::json!({
                        "text": "> [Prisma Validator] schema.prisma ažuriran (AI predlog + deterministički popravljač). Ponovo testiram...\n",
                        "type": "info"
                    }));
                }
            }
            None => {
                let _ = window.emit(log_channel, serde_json::json!({
                    "text": format!("> [Prisma Validator] AI model '{}' nije vratio upotrebljiv odgovor.\n", resolved_model),
                    "type": "warning"
                }));
            }
        }
    }

    for pf in pending_ai_fixes.drain(..) {
        crate::katastar::finalize_katastar_outcome(
            &pf.entry_id,
            false,
            "ai",
            "prisma_generate",
            None,
            None,
            &resolved_model,
        );
    }

    Err(format!("Failed to validate and auto-heal Prisma schema after {} AI attempts.", MAX_AI_HEALS))
}

pub fn merge_prisma_ai_response(original_schema: &str, ai_response: &str) -> String {
    let model_re = match regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{[^}]*\}") {
        Ok(r) => r,
        Err(_) => return ai_response.to_string(),
    };

    let orig_models_count = model_re.find_iter(original_schema).count();
    let ai_models_count = model_re.find_iter(ai_response).count();

    let ai_has_datasource = ai_response.contains("datasource ");
    let ai_has_generator = ai_response.contains("generator ");

    // If the AI returned a partial snippet (missing datasource/generator, or dropped existing models),
    // merge its updated/new model blocks into the original schema without deleting existing models or headers.
    if !ai_has_datasource || !ai_has_generator || (orig_models_count > 1 && ai_models_count < orig_models_count) {
        if ai_models_count == 0 {
            return original_schema.to_string();
        }
        let mut merged = original_schema.to_string();
        for cap in model_re.captures_iter(ai_response) {
            if let (Some(full_match), Some(name_match)) = (cap.get(0), cap.get(1)) {
                let model_name = name_match.as_str();
                let new_block = full_match.as_str();
                let target_pattern = format!(r"(?ms)^model\s+{}\s*\{{[^}}]*\}}", regex::escape(model_name));
                if let Ok(target_re) = regex::Regex::new(&target_pattern) {
                    if target_re.is_match(&merged) {
                        merged = target_re.replace(&merged, new_block).to_string();
                    } else {
                        merged = format!("{}\n\n{}", merged.trim_end(), new_block);
                    }
                }
            }
        }
        if !merged.contains("datasource ") {
            let provider = if merged.contains("enum ") || merged.contains("postgresql://") { "postgresql" } else { "sqlite" };
            merged = format!(
                "datasource db {{\n  provider = \"{}\"\n  url      = env(\"DATABASE_URL\")\n}}\n\n{}",
                provider,
                merged
            );
        }
        if !merged.contains("generator ") {
            merged = format!(
                "generator client {{\n  provider = \"prisma-client-js\"\n}}\n\n{}",
                merged
            );
        }
        return merged;
    }

    ai_response.to_string()
}

pub fn has_prose_leak_in_code(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower.contains("the error indicates")
        || lower.contains("this could be due to")
        || lower.contains("several reasons such as")
        || lower.contains("here is the corrected")
        || lower.contains("here is the fixed")
        || lower.contains("here's the corrected")
        || lower.contains("here's the fixed")
        || lower.contains("to fix this, you")
        || lower.contains("to resolve this error,")
    {
        return true;
    }

    let mut has_step_1 = false;
    let mut has_step_2 = false;
    let mut first_non_comment_checked = false;
    for line in trimmed.lines() {
        let lt = line.trim();
        if lt.is_empty() || lt.starts_with("//") || lt.starts_with("/*") || lt.starts_with('*') {
            continue;
        }
        let ll = lt.to_lowercase();
        if ll.starts_with("1. ") {
            has_step_1 = true;
        } else if ll.starts_with("2. ") {
            has_step_2 = true;
        }
        if !first_non_comment_checked {
            first_non_comment_checked = true;
            let starts_with_prose_opener = ll.starts_with("here is ")
                || ll.starts_with("here's ")
                || ll.starts_with("to fix ")
                || ll.starts_with("to resolve ")
                || ll.starts_with("you can ")
                || ll.starts_with("you need to ")
                || ll.starts_with("you should ")
                || ll.starts_with("please check ")
                || ll.starts_with("please ensure ")
                || ll.starts_with("sure, here")
                || ll.starts_with("i have fixed")
                || ll.starts_with("the error ")
                || ll.starts_with("this error ");
            if starts_with_prose_opener
                && !lt.ends_with(';')
                && !lt.ends_with('{')
                && !lt.ends_with('}')
            {
                return true;
            }
        }
    }
    has_step_1 && has_step_2
}

pub fn looks_like_code_payload(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.starts_with("```") {
        return false;
    }
    // Cannot be a raw JSON object or array
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return false;
    }
    if has_prose_leak_in_code(trimmed) {
        return false;
    }
    // Single-line natural language prose (e.g. explanations ending with a period and lacking code syntax) is NOT code
    if !trimmed.contains('\n')
        && trimmed.ends_with('.')
        && !trimmed.contains(';')
        && !trimmed.contains('{')
        && !trimmed.contains('}')
        && !trimmed.contains('=')
        && !trimmed.contains("=>")
    {
        return false;
    }

    let has_ts_js_kw = trimmed.starts_with("import ")
        || trimmed.starts_with("import{")
        || trimmed.starts_with("export ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("async function ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("interface ")
        || trimmed.starts_with("type ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("'use client'")
        || trimmed.starts_with("\"use client\"")
        || trimmed.starts_with("'use server'")
        || trimmed.starts_with("\"use server\"")
        || trimmed.contains("\nimport ")
        || trimmed.contains("\nexport ")
        || trimmed.contains("\nconst ")
        || trimmed.contains("\nfunction ")
        || trimmed.contains("\nasync function ")
        || trimmed.contains("\ninterface ")
        || trimmed.contains("\ntype ")
        || trimmed.contains("\nenum ")
        || trimmed.contains("from '")
        || trimmed.contains("from \"")
        || trimmed.contains("require(")
        || trimmed.contains("module.exports")
        || trimmed.contains("=>");

    let has_prisma_block = regex::Regex::new(r"(?m)^\s*(?:model|enum|datasource|generator)\s+[A-Za-z_]\w*\s*\{")
        .map(|re| re.is_match(trimmed))
        .unwrap_or(false);

    let has_css_rule = trimmed.contains("@tailwind ")
        || trimmed.contains("@apply ")
        || trimmed.contains(":root")
        || (trimmed.contains('{')
            && trimmed.contains('}')
            && trimmed.contains(':')
            && trimmed.contains(';'));

    let has_jsx_tags = trimmed.contains("</") || (trimmed.contains('<') && trimmed.contains("/>"));
    let has_stmt_block = trimmed.contains("return ")
        && (trimmed.contains(';') || (trimmed.contains('{') && trimmed.contains('}')));

    has_ts_js_kw || has_prisma_block || has_css_rule || has_jsx_tags || has_stmt_block
}

/// Guards against AI "fixing" compile errors by deleting large sections of UI/JSX or logic.
pub fn detect_destructive_code_drop(file_path: &str, before: &str, after: &str) -> Option<String> {
    let lower = file_path.to_lowercase();
    if !(lower.ends_with(".tsx")
        || lower.ends_with(".ts")
        || lower.ends_with(".jsx")
        || lower.ends_with(".js"))
    {
        return None;
    }
    if before.trim().len() < 400 {
        return None;
    }
    let before_non_empty = before.lines().filter(|l| !l.trim().is_empty()).count();
    let after_non_empty = after.lines().filter(|l| !l.trim().is_empty()).count();
    if before_non_empty >= 20 && after_non_empty * 5 < before_non_empty * 2 {
        return Some(format!(
            "Destruktivno brisanje koda u '{}': broj linija opao sa {} na {} (>60% koda obrisano).",
            file_path, before_non_empty, after_non_empty
        ));
    }
    if lower.ends_with(".tsx") || lower.ends_with(".jsx") {
        let count_jsx = |src: &str| -> usize { src.matches("</").count() + src.matches("/>").count() };
        let before_jsx = count_jsx(before);
        let after_jsx = count_jsx(after);
        if before_jsx >= 6 && (after_jsx == 0 || after_jsx * 3 < before_jsx) {
            return Some(format!(
                "Destruktivno brisanje UI/JSX elemenata u '{}': JSX tagovi opali sa {} na {}.",
                file_path, before_jsx, after_jsx
            ));
        }
    }
    None
}

pub fn is_valid_file_path(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.len() > 260 {
        return false;
    }
    let norm = trimmed.replace('\\', "/");
    if norm.starts_with('/')
        || norm.ends_with('/')
        || norm.contains("..")
        || norm.contains(':')
    {
        return false;
    }
    // File paths cannot contain newlines, spaces, or code syntax characters
    if norm.contains('\n')
        || norm.contains('\r')
        || norm.contains('\t')
        || norm.contains(' ')
        || norm.contains(';')
        || norm.contains('{')
        || norm.contains('}')
        || norm.contains('(')
        || norm.contains(')')
        || norm.contains('<')
        || norm.contains('>')
        || norm.contains('=')
        || norm.contains('\'')
        || norm.contains('"')
        || norm.contains('`')
        || norm.contains('*')
        || norm.contains('?')
        || norm.contains(',')
    {
        return false;
    }
    let lower = norm.to_lowercase();
    let has_valid_ext = lower.ends_with(".tsx")
        || lower.ends_with(".ts")
        || lower.ends_with(".jsx")
        || lower.ends_with(".js")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
        || lower.ends_with(".prisma")
        || lower.ends_with(".css")
        || lower.ends_with(".scss")
        || lower.ends_with(".sass")
        || lower.ends_with(".json")
        || lower.ends_with(".html")
        || lower.ends_with(".md")
        || lower.ends_with(".mdx")
        || lower.ends_with(".svg")
        || lower.ends_with(".sql")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || lower.ends_with(".toml");

    if has_valid_ext {
        return true;
    }
    let basename = lower.rsplit('/').next().unwrap_or("");
    matches!(
        basename,
        ".env"
            | ".env.local"
            | ".env.example"
            | ".env.development"
            | ".env.production"
            | ".gitignore"
            | ".eslintrc"
            | ".prettierrc"
            | "dockerfile"
            | "makefile"
    )
}

fn normalize_local_specifier_for_import(spec: &str) -> String {
    let trimmed = spec.trim().replace('\\', "/");
    let mut s = if let Some(rest) = trimmed.strip_prefix("src/").or_else(|| trimmed.strip_prefix("~/")) {
        format!("@/{}", rest)
    } else if let Some(rest) = trimmed.strip_prefix("./src/") {
        format!("@/{}", rest)
    } else {
        trimmed
    };
    if s.starts_with("@/") || s.starts_with("./") || s.starts_with("../") {
        for ext in &[".tsx", ".ts", ".jsx", ".js"] {
            if s.ends_with(ext) {
                s.truncate(s.len() - ext.len());
                break;
            }
        }
    }
    s
}

pub fn recursively_find_edits(value: &Value, current_path: &str, edits: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            let mut detected_code = None;
            let mut detected_path = None;
            let mut detected_target = None;
            let mut has_explicit_empty_replacement = false;
            let mut context_path = current_path.to_string();

            let is_move = map.get("type").and_then(|t| t.as_str()) == Some("move");
            let has_path_swap_keys = (map.contains_key("newPath") || map.contains_key("new_path"))
                && (map.contains_key("path")
                    || map.contains_key("oldPath")
                    || map.contains_key("old_path")
                    || map.contains_key("targetPath")
                    || map.contains_key("from"));
            let has_code_payload_key = map.keys().any(|k| {
                let kl = k.to_lowercase();
                matches!(
                    kl.as_str(),
                    "code" | "content" | "replacement" | "replace" | "replacewith" | "replace_with"
                )
            });

            // Handle import / path rewrite objects (e.g. `{"type":"import","path":"src/models/X","newPath":"src/models/X.ts"}` or `{"from":...,"to":...}`)
            let is_import_rewrite = !is_move
                && !has_code_payload_key
                && (map.get("type").and_then(|t| t.as_str()) == Some("import")
                    || (map.contains_key("from") && map.contains_key("to"))
                    || has_path_swap_keys);

            if is_import_rewrite {
                let has_explicit_old_key = map.contains_key("from")
                    || map.contains_key("oldPath")
                    || map.contains_key("old_path")
                    || map.contains_key("targetPath");

                let explicit_file = map
                    .get("file")
                    .or_else(|| map.get("filePath"))
                    .or_else(|| map.get("file_path"))
                    .or_else(|| map.get("filename"))
                    .or_else(|| if has_explicit_old_key { map.get("path") } else { None })
                    .and_then(|v| v.as_str())
                    .filter(|s| is_valid_file_path(s))
                    .map(|s| normalize_rel_path_str(s))
                    .unwrap_or_else(|| context_path.clone());

                let old_raw = map
                    .get("from")
                    .or_else(|| map.get("oldPath"))
                    .or_else(|| map.get("old_path"))
                    .or_else(|| map.get("targetPath"))
                    .or_else(|| map.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"');

                let new_raw = map
                    .get("to")
                    .or_else(|| map.get("newPath"))
                    .or_else(|| map.get("new_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"');

                let is_numeric_ref = old_raw.chars().all(|c| c.is_ascii_digit())
                    || new_raw.chars().all(|c| c.is_ascii_digit());
                if !old_raw.is_empty()
                    && !new_raw.is_empty()
                    && !is_numeric_ref
                    && !old_raw.contains('\n')
                    && !new_raw.contains('\n')
                {
                    let normalized_new = normalize_local_specifier_for_import(new_raw);
                    if is_valid_file_path(&explicit_file) && normalized_new != old_raw {
                        edits.push(serde_json::json!({
                            "path": explicit_file,
                            "target": format!("'{}'", old_raw),
                            "replacement": format!("'{}'", normalized_new)
                        }));
                        return;
                    }
                }
            }

            // First pass: check if there is an explicit valid file path in this object (e.g. "file": "src/components/Navbar.tsx")
            for (k, v) in map {
                let k_lower = k.to_lowercase();
                if matches!(k_lower.as_str(), "newpath" | "new_path" | "oldpath" | "old_path" | "targetpath") {
                    continue;
                }
                if let Some(s) = v.as_str() {
                    if is_valid_file_path(s)
                        && (k_lower == "file"
                            || k_lower == "filepath"
                            || k_lower == "file_path"
                            || k_lower == "filename"
                            || k_lower == "path")
                    {
                        let norm_p = s.trim().replace('\\', "/");
                        context_path = norm_p.clone();
                        detected_path = Some(norm_p);
                    }
                }
            }

            // If this object is a container envelope holding an array/object of edits, recurse directly into its children
            // so container-level metadata like "explanation" is never mistaken for a code edit.
            let is_container_key = |kl: &str, val: &Value| -> bool {
                (val.is_array() || val.is_object())
                    && (matches!(
                        kl,
                        "edits" | "edit" | "fixes" | "fix" | "changes" | "change" | "patches" | "patch" | "files" | "operations"
                    ) || kl.starts_with("edit")
                        || kl.starts_with("fix")
                        || kl.starts_with("patch"))
            };
            let is_container = map.iter().any(|(k, v)| is_container_key(&k.to_lowercase(), v));
            if is_container {
                for (k, v) in map {
                    let kl = k.to_lowercase();
                    if is_container_key(&kl, v) {
                        recursively_find_edits(v, &context_path, edits);
                    }
                }
                if !edits.is_empty() {
                    return;
                }
            }

            // Second pass: extract code and target/search strings from leaf edit object
            for (k, v) in map {
                let k_lower = k.to_lowercase();

                // Explicitly skip descriptive metadata fields so prose explanations are never treated as code
                if matches!(
                    k_lower.as_str(),
                    "explanation"
                        | "reason"
                        | "reasons"
                        | "description"
                        | "summary"
                        | "analysis"
                        | "message"
                        | "note"
                        | "notes"
                        | "thought"
                        | "error"
                        | "status"
                        | "type"
                        | "name"
                        | "id"
                        | "response"
                        | "answer"
                        | "reply"
                        | "steps"
                        | "instructions"
                        | "details"
                        | "cause"
                        | "causes"
                        | "fix_steps"
                        | "suggestion"
                        | "suggestions"
                        | "symbol"
                        | "line"
                        | "column"
                        | "newpath"
                        | "new_path"
                        | "newfile"
                        | "new_file"
                ) {
                    continue;
                }

                if let Some(s) = v.as_str() {
                    if is_valid_file_path(s)
                        && detected_path.is_none()
                        && (k_lower == "file"
                            || k_lower == "filepath"
                            || k_lower == "file_path"
                            || k_lower == "filename"
                            || k_lower == "path")
                    {
                        detected_path = Some(s.trim().replace('\\', "/"));
                    } else if matches!(
                        k_lower.as_str(),
                        "target" | "search" | "old" | "oldcode" | "old_code" | "original" | "before" | "find" | "match"
                    ) || k_lower.contains("target")
                        || k_lower.contains("search")
                        || k_lower.contains("original")
                    {
                        if !s.is_empty() {
                            detected_target = Some(s.to_string());
                        }
                    } else if (k_lower == "path" || k_lower == "file")
                        && !is_valid_file_path(s)
                        && (s.contains(' ') || s.contains(';') || s.contains('\n'))
                    {
                        // The model put an inline search/target snippet into the "path" field
                        detected_target = Some(s.to_string());
                    } else if matches!(
                        k_lower.as_str(),
                        "replacement"
                            | "replace"
                            | "replacewith"
                            | "replace_with"
                            | "code"
                            | "content"
                            | "newcode"
                            | "new_code"
                            | "newcontent"
                            | "new_content"
                            | "newtext"
                            | "new_text"
                            | "fixedcode"
                            | "fixed_code"
                            | "fixedcontent"
                            | "solution"
                            | "with"
                            | "to"
                            | "insert"
                    ) {
                        if !s.trim().is_empty() {
                            detected_code = Some(s.to_string());
                        } else if matches!(k_lower.as_str(), "replacement" | "replace" | "newcode" | "new_code") {
                            has_explicit_empty_replacement = true;
                        }
                    } else if looks_like_code_payload(s)
                        && detected_code.is_none()
                        && detected_target.as_deref() != Some(s)
                    {
                        detected_code = Some(s.to_string());
                    }
                }
            }

            // Allow empty replacement ("") when a target is specified (deletion of duplicate/erroneous code)
            if detected_code.is_none() && has_explicit_empty_replacement && detected_target.is_some() {
                detected_code = Some(String::new());
            }

            if let Some(code) = detected_code {
                let final_path = detected_path.unwrap_or_else(|| context_path.clone());
                let final_target = detected_target.unwrap_or_default();
                let final_is_md = final_path.to_lowercase().ends_with(".md");
                let final_is_json = final_path.to_lowercase().ends_with(".json");

                let code_ok = if !final_is_md && has_prose_leak_in_code(&code) {
                    false
                } else if final_target.is_empty() && !code.trim().is_empty() {
                    if final_is_json {
                        serde_json::from_str::<Value>(&code).is_ok()
                    } else {
                        looks_like_code_payload(&code)
                    }
                } else {
                    true
                };

                if code_ok && is_valid_file_path(&final_path) {
                    edits.push(serde_json::json!({
                        "path": final_path,
                        "target": final_target,
                        "replacement": code
                    }));
                    return;
                }
            }

            for (k, v) in map {
                let kl = k.to_lowercase();
                if (v.is_object() || v.is_array())
                    && !matches!(
                        kl.as_str(),
                        "explanation"
                            | "reason"
                            | "reasons"
                            | "description"
                            | "summary"
                            | "analysis"
                            | "message"
                            | "note"
                            | "notes"
                            | "thought"
                            | "steps"
                            | "instructions"
                            | "details"
                    )
                {
                    let child_ctx = if is_valid_file_path(k) {
                        k.trim().replace('\\', "/")
                    } else {
                        context_path.clone()
                    };
                    recursively_find_edits(v, &child_ctx, edits);
                } else if let Some(s) = v.as_str() {
                    if is_valid_file_path(k) && looks_like_code_payload(s) {
                        edits.push(serde_json::json!({
                            "path": k.trim().replace('\\', "/"),
                            "target": "",
                            "replacement": s
                        }));
                    }
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                recursively_find_edits(item, current_path, edits);
            }
        }
        Value::String(s) => {
            if is_valid_file_path(current_path) && looks_like_code_payload(s) && edits.is_empty() {
                edits.push(serde_json::json!({
                    "path": current_path,
                    "target": "",
                    "replacement": s
                }));
            }
        }
        _ => {}
    }
}

pub fn extract_code_from_malformed_json_response(raw: &str, default_path: &str) -> Option<Vec<Value>> {
    let trimmed = raw.trim();

    // Check if raw text has an embedded code block inside "content": `...` or "content": "..."
    // or "replacement": `...` or "replacement": "..."
    let markers = ["\"content\":", "\"replacement\":", "\"code\":"];
    for marker in &markers {
        if let Some(pos) = trimmed.find(marker) {
            let before = &trimmed[..pos];
            let mut detected_path = default_path.to_string();
            // Dynamically search for any valid file path referenced in the envelope before the marker
            for token in before.split('"') {
                if is_valid_file_path(token) {
                    detected_path = token.replace('\\', "/");
                }
            }
            if !is_valid_file_path(&detected_path) {
                continue;
            }

            let after = trimmed[pos + marker.len()..].trim();
            if after.starts_with('`') {
                if let Some(end_bt) = after[1..].rfind('`') {
                    let code = after[1..=end_bt].trim();
                    if !code.is_empty() {
                        let unescaped = code.replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\");
                        if looks_like_code_payload(&unescaped) {
                            return Some(vec![serde_json::json!({
                                "path": detected_path,
                                "target": "",
                                "replacement": unescaped
                            })]);
                        }
                    }
                }
            } else if after.starts_with('"') {
                if let Some(end_quote) = after[1..].rfind('"') {
                    let code = after[1..=end_quote].trim();
                    if !code.is_empty() {
                        let unescaped = code.replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\");
                        if looks_like_code_payload(&unescaped) {
                            return Some(vec![serde_json::json!({
                                "path": detected_path,
                                "target": "",
                                "replacement": unescaped
                            })]);
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn extract_edits_from_json(parsed_json: &Value, default_path: &str) -> Vec<Value> {
    let mut edits = Vec::new();
    recursively_find_edits(parsed_json, default_path, &mut edits);
    edits
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SingleFileVerifyOutcome {
    Passed(&'static str),
    Failed(&'static str),
    Inconclusive,
}

struct VerifyDetail {
    outcome: SingleFileVerifyOutcome,
    raw_output: String,
    file_errors: Vec<String>,
}

fn resolve_local_node_executable() -> String {
    #[cfg(target_os = "windows")]
    {
        let toolchain_node = get_base_dir()
            .join("toolchain")
            .join("node")
            .join("node-v20.18.1-win-x64")
            .join("node.exe");
        if toolchain_node.exists() {
            return toolchain_node.to_string_lossy().to_string();
        }
        for p in &[
            r"C:\Program Files\nodejs\node.exe",
            r"C:\Program Files (x86)\nodejs\node.exe",
        ] {
            if std::path::Path::new(p).exists() {
                return (*p).to_string();
            }
        }
    }
    "node".to_string()
}

/// Fast local verification for interactive single-file heals (`tsc --noEmit --incremental false` or `prisma validate`).
/// Checks `rel_file_path` as well as any secondary `.ts`/`.tsx` files touched in `edited_files`,
/// returning exact filtered compiler error lines so failed AI attempts can be rolled back and retried with real diagnostics.
async fn verify_single_file_detail(
    proj_dir: &std::path::Path,
    rel_file_path: &str,
    edited_files: &[String],
    category: &str,
    error_code: Option<&str>,
) -> VerifyDetail {
    let lower_rel = normalize_rel_path_str(rel_file_path).to_lowercase();
    let node_exe = resolve_local_node_executable();

    let includes_prisma = lower_rel.ends_with(".prisma")
        || edited_files.iter().any(|p| p.to_lowercase().ends_with(".prisma"));
    if includes_prisma {
        let local_prisma_js = proj_dir.join("node_modules").join("prisma").join("build").join("index.js");
        if !local_prisma_js.exists() {
            return VerifyDetail {
                outcome: SingleFileVerifyOutcome::Inconclusive,
                raw_output: String::new(),
                file_errors: Vec::new(),
            };
        }
        let mut cmd = tokio::process::Command::new(&node_exe);
        cmd.arg(&local_prisma_js).arg("validate").current_dir(proj_dir);
        cmd.env("NO_COLOR", "1").env("FORCE_COLOR", "0");
        #[cfg(target_os = "windows")]
        cmd.creation_flags(0x08000000);

        return match tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output()).await {
            Ok(Ok(out)) if out.status.success() => VerifyDetail {
                outcome: SingleFileVerifyOutcome::Passed("prisma_validate"),
                raw_output: String::from_utf8_lossy(&out.stdout).to_string(),
                file_errors: Vec::new(),
            },
            Ok(Ok(out)) => {
                let raw_output = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                let file_errors: Vec<String> = raw_output
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| {
                        !l.is_empty()
                            && !l.starts_with("Environment variables loaded")
                            && !l.starts_with("Prisma schema loaded")
                            && !l.starts_with("Datasource ")
                    })
                    .take(15)
                    .map(|l| l.to_string())
                    .collect();
                VerifyDetail {
                    outcome: SingleFileVerifyOutcome::Failed("prisma_validate"),
                    raw_output,
                    file_errors,
                }
            }
            _ => VerifyDetail {
                outcome: SingleFileVerifyOutcome::Inconclusive,
                raw_output: String::new(),
                file_errors: Vec::new(),
            },
        };
    }

    // `tsc` cannot detect Next.js missing `"use client"` or non-ReferenceError runtime errors
    if error_code == Some("NEXT_USE_CLIENT")
        || (category == "runtime" && error_code != Some("ReferenceError"))
    {
        return VerifyDetail {
            outcome: SingleFileVerifyOutcome::Inconclusive,
            raw_output: String::new(),
            file_errors: Vec::new(),
        };
    }

    let mut target_ts_paths: Vec<String> = Vec::new();
    if lower_rel.ends_with(".ts") || lower_rel.ends_with(".tsx") {
        target_ts_paths.push(lower_rel.clone());
    }
    for ef in edited_files {
        let norm_ef = normalize_rel_path_str(ef).to_lowercase();
        if (norm_ef.ends_with(".ts") || norm_ef.ends_with(".tsx")) && !target_ts_paths.contains(&norm_ef) {
            target_ts_paths.push(norm_ef);
        }
    }

    let local_tsc_js = proj_dir.join("node_modules").join("typescript").join("bin").join("tsc");
    let has_local_env = proj_dir.join("tsconfig.json").exists()
        && local_tsc_js.exists()
        && proj_dir.join("node_modules").join("react").exists()
        && proj_dir.join("node_modules").join("next").exists();

    if target_ts_paths.is_empty() || !has_local_env {
        return VerifyDetail {
            outcome: SingleFileVerifyOutcome::Inconclusive,
            raw_output: String::new(),
            file_errors: Vec::new(),
        };
    }

    let mut cmd = tokio::process::Command::new(&node_exe);
    cmd.arg(&local_tsc_js)
        .arg("--noEmit")
        .arg("--incremental")
        .arg("false")
        .arg("--pretty")
        .arg("false")
        .current_dir(proj_dir);
    cmd.env("NO_COLOR", "1").env("FORCE_COLOR", "0");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);

    match tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(out)) => {
            let raw_output = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if out.status.success() {
                return VerifyDetail {
                    outcome: SingleFileVerifyOutcome::Passed("tsc_noemit"),
                    raw_output,
                    file_errors: Vec::new(),
                };
            }

            let combined_lower = raw_output.replace('\\', "/").to_lowercase();
            if combined_lower.contains("ts18003")
                || combined_lower.contains("cannot find type definition file")
            {
                return VerifyDetail {
                    outcome: SingleFileVerifyOutcome::Inconclusive,
                    raw_output,
                    file_errors: Vec::new(),
                };
            }

            let mut matched_errors: Vec<String> = Vec::new();
            for raw_line in raw_output.lines() {
                let norm_line = raw_line.trim().replace('\\', "/").to_lowercase();
                if !norm_line.contains("error ts") {
                    continue;
                }
                let hits_target = target_ts_paths.iter().any(|tp| {
                    norm_line.starts_with(tp)
                        || norm_line.contains(&format!("/{}", tp))
                        || norm_line.contains(&format!("{}(", tp))
                        || norm_line.contains(&format!("{}:", tp))
                });
                if hits_target {
                    matched_errors.push(raw_line.trim().to_string());
                }
            }

            if !matched_errors.is_empty() {
                VerifyDetail {
                    outcome: SingleFileVerifyOutcome::Failed("tsc_noemit"),
                    raw_output,
                    file_errors: matched_errors,
                }
            } else {
                VerifyDetail {
                    outcome: SingleFileVerifyOutcome::Passed("error_disappeared"),
                    raw_output,
                    file_errors: Vec::new(),
                }
            }
        }
        _ => VerifyDetail {
            outcome: SingleFileVerifyOutcome::Inconclusive,
            raw_output: String::new(),
            file_errors: Vec::new(),
        },
    }
}

fn list_project_src_modules(proj_dir: &std::path::Path, max_items: usize) -> Vec<String> {
    let mut files = Vec::new();
    let src_root = proj_dir.join("src");
    let scan_root = if src_root.exists() { &src_root } else { proj_dir };
    let mut stack = vec![scan_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if p.is_dir() {
                    if name != "node_modules" && name != ".next" && name != ".git" && name != "dist" && name != "build" {
                        stack.push(p);
                    }
                } else if p.is_file() {
                    let lower = name.to_lowercase();
                    if lower.ends_with(".ts")
                        || lower.ends_with(".tsx")
                        || lower.ends_with(".js")
                        || lower.ends_with(".jsx")
                    {
                        if let Ok(rel) = p.strip_prefix(proj_dir) {
                            files.push(rel.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
            }
        }
    }
    files.sort();
    files.truncate(max_items);
    files
}

fn truncate_context_snippet(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        content.to_string()
    } else {
        let clipped: String = content.chars().take(max_chars).collect();
        format!("{}\n/* ...[truncated for context budget]... */", clipped)
    }
}

fn apply_single_targeted_edit_with_quote_fallback(
    content: &str,
    target: &str,
    replacement: &str,
) -> Result<String, String> {
    match apply_targeted_edit(content, target, replacement) {
        Ok(res) => Ok(res),
        Err(orig_err) => {
            if !target.contains('\n')
                && target.starts_with('\'')
                && target.ends_with('\'')
                && !target.contains('"')
                && replacement.starts_with('\'')
                && replacement.ends_with('\'')
                && !replacement.contains('"')
            {
                let t_dq = format!("\"{}\"", &target[1..target.len() - 1]);
                let r_dq = format!("\"{}\"", &replacement[1..replacement.len() - 1]);
                if let Ok(res) = apply_targeted_edit(content, &t_dq, &r_dq) {
                    return Ok(res);
                }
            }
            if !target.contains('\n')
                && target.starts_with('"')
                && target.ends_with('"')
                && !target.contains('\'')
                && replacement.starts_with('"')
                && replacement.ends_with('"')
                && !replacement.contains('\'')
            {
                let t_sq = format!("'{}'", &target[1..target.len() - 1]);
                let r_sq = format!("'{}'", &replacement[1..replacement.len() - 1]);
                if let Ok(res) = apply_targeted_edit(content, &t_sq, &r_sq) {
                    return Ok(res);
                }
            }
            Err(orig_err)
        }
    }
}

fn restore_attempt_snapshots(
    proj_dir: &std::path::Path,
    snapshots: &std::collections::HashMap<String, Option<String>>,
) {
    for (rel_p, orig_opt) in snapshots {
        let disk_p = proj_dir.join(rel_p);
        match orig_opt {
            Some(prev_content) => {
                let _ = std::fs::write(&disk_p, prev_content);
            }
            None => {
                let _ = std::fs::remove_file(&disk_p);
            }
        }
    }
}

#[tauri::command]
pub async fn project_heal_compile_error(
    project_id: String,
    file_path: String,
    error_msg: String,
    window: tauri::Window,
) -> Result<bool, String> {
    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": "Inicijalizacija popravke..."
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing] Započinjem popravku za fajl: {}\n", file_path),
        "type": "info"
    }));

    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška: Projekat ne postoji" }));
        return Err("Direktorijum projekta ne postoji.".to_string());
    }
    crate::sanitizer::ensure_valid_tsconfig(&proj_dir);
    ensure_directory_barrel_files(&proj_dir);

    let file_path = resolve_project_file_candidate(&proj_dir, &file_path)
        .map(|p| normalize_rel_path_str(&p))
        .unwrap_or_else(|| normalize_rel_path_str(&file_path));
    let full_path = proj_dir.join(&file_path);
    if !full_path.exists() || !full_path.is_file() || !full_path.starts_with(&proj_dir) {
        let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška: Fajl ne postoji" }));
        return Err(format!("Fajl {} ne postoji.", file_path));
    }

    let rel_file_path = if let Ok(stripped) = full_path.strip_prefix(&proj_dir) {
        normalize_rel_path_str(&stripped.to_string_lossy())
    } else {
        normalize_rel_path_str(&file_path)
    };

    let settings = get_axiom_settings();
    let requested_model = settings.get("modelId")
        .and_then(|m| m.as_str())
        .unwrap_or("qwen2.5-coder:7b")
        .to_string();

    let sig_info = crate::katastar::normalize_error_signature("", &error_msg, Some(&rel_file_path));
    let failed_prev_ids = crate::katastar::resolve_reoccurred_pending_repair(
        &project_id,
        &rel_file_path,
        &sig_info.error_signature,
        &requested_model,
    );
    if !failed_prev_ids.is_empty() {
        debug_log_to_file(format!(
            "[Katastar] Previous pending repair(s) {:?} did not eliminate '{}' in '{}' -> marked failed.",
            failed_prev_ids, sig_info.error_signature, rel_file_path
        ));
    }

    let raw_file_content = std::fs::read_to_string(&full_path)
        .map_err(|e| {
            let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška pri čitanju" }));
            format!("Greška pri čitanju fajla: {}", e)
        })?;

    let raw_clean = strip_axiom_inspector_script(&raw_file_content);
    let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();

    let mut active_file_content = raw_file_content.clone();
    let mut active_clean_content = raw_clean.clone();
    let mut current_error_context = error_msg.clone();

    // Fast deterministic pre-heal pass (rewrites bare `src/...` imports, barrel imports, missing react-icons subpaths, useEffect deps, etc.)
    let mut pre_sanitized = post_process_generated_file(&rel_file_path, &raw_file_content, &serde_json::json!({}), has_tailwind);
    let pre_routes = get_project_routes_from_dir(&proj_dir);
    let pre_routes_list: Vec<String> = pre_routes.into_iter().map(|(r, _)| r).collect();
    if !pre_routes_list.is_empty() {
        pre_sanitized = sanitize_app_router_links_with_routes(&pre_sanitized, &pre_routes_list);
    }
    pre_sanitized = sanitize_unhandled_session_loading(&pre_sanitized);

    if pre_sanitized != raw_file_content
        && crate::orchestrator::validate_file_with_abstract_rules(&rel_file_path, &pre_sanitized, &requested_model)
            .await
            .is_ok()
        && std::fs::write(&full_path, &pre_sanitized).is_ok()
    {
        let pre_clean = strip_axiom_inspector_script(&pre_sanitized);
        let atomic_patch = crate::katastar::extract_safe_atomic_patch(
            &rel_file_path,
            &raw_clean,
            &pre_clean,
        );
        let eid = crate::katastar::register_repair_attempt(
            &sig_info.category,
            &error_msg,
            &rel_file_path,
            "deterministic",
            "deterministic_sanitizer",
            atomic_patch.clone(),
            "Deterministic sanitizer pre-heal (imports, aliases, hooks, client directives, schema)",
            "",
        );
        let verify_detail = verify_single_file_detail(
            &proj_dir,
            &rel_file_path,
            &[],
            &sig_info.category,
            sig_info.error_code.as_deref(),
        )
        .await;
        match verify_detail.outcome {
            SingleFileVerifyOutcome::Passed(vkind) => {
                crate::katastar::finalize_katastar_outcome(
                    &eid,
                    true,
                    "deterministic",
                    vkind,
                    atomic_patch,
                    None,
                    "",
                );
                ensure_axiom_inspector_injected(&proj_dir);
                let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Uspešno popravljeno!" }));
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [AI Auto-Healing] Deterministički saniran i verifikovan ({}) fajl: {}\n", vkind, rel_file_path),
                    "type": "info"
                }));
                return Ok(true);
            }
            SingleFileVerifyOutcome::Inconclusive => {
                crate::katastar::track_pending_project_repair(
                    &project_id,
                    &rel_file_path,
                    &sig_info.error_signature,
                    &eid,
                    "deterministic",
                );
                ensure_axiom_inspector_injected(&proj_dir);
                let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Uspešno popravljeno!" }));
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [AI Auto-Healing] Deterministički saniran kod i zavisnosti u: {}\n", rel_file_path),
                    "type": "info"
                }));
                return Ok(true);
            }
            SingleFileVerifyOutcome::Failed(vkind) => {
                crate::katastar::finalize_katastar_outcome(
                    &eid,
                    false,
                    "deterministic",
                    vkind,
                    None,
                    None,
                    "",
                );
                active_file_content = pre_sanitized;
                active_clean_content = pre_clean;
                if !verify_detail.file_errors.is_empty() {
                    current_error_context = format!(
                        "{}\n\n[Live compiler diagnostics after deterministic import cleanup]:\n{}",
                        error_msg,
                        verify_detail.file_errors.join("\n")
                    );
                }
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [AI Auto-Healing] Deterministički prolaz je sredio lokalne importe/sintaksu, ali '{}' i dalje ima greške ({}); nastavljam sa Katastrom i verifikovanom AI petljom...\n", rel_file_path, vkind),
                    "type": "info"
                }));
            }
        }
    }

    // Phase A (Before calling AI): Consult Local Error Katastar for verified auto-patches & past hints
    let mut katastar_lookup = crate::katastar::lookup_katastar(
        &sig_info.category,
        &current_error_context,
        &rel_file_path,
        &active_clean_content,
        &failed_prev_ids,
    );
    if katastar_lookup.applicable_patches.is_empty() && current_error_context != error_msg {
        katastar_lookup = crate::katastar::lookup_katastar(
            &sig_info.category,
            &error_msg,
            &rel_file_path,
            &active_clean_content,
            &failed_prev_ids,
        );
    }

    if let Some(best) = katastar_lookup.applicable_patches.first() {
        let candidate = active_clean_content.replace(&best.patch.search, &best.patch.replace);
        let mut processed = post_process_generated_file(&rel_file_path, &candidate, &serde_json::json!({}), has_tailwind);
        let project_routes = get_project_routes_from_dir(&proj_dir);
        let routes_list: Vec<String> = project_routes.into_iter().map(|(r, _)| r).collect();
        if !routes_list.is_empty() {
            processed = sanitize_app_router_links_with_routes(&processed, &routes_list);
        }
        processed = sanitize_unhandled_session_loading(&processed);
        if processed != active_clean_content
            && crate::orchestrator::validate_file_with_abstract_rules(&rel_file_path, &processed, &requested_model)
                .await
                .is_ok()
            && std::fs::write(&full_path, &processed).is_ok()
        {
            let verify_detail = verify_single_file_detail(
                &proj_dir,
                &rel_file_path,
                &[],
                &sig_info.category,
                sig_info.error_code.as_deref(),
            )
            .await;
            match verify_detail.outcome {
                SingleFileVerifyOutcome::Failed(vkind) => {
                    crate::katastar::finalize_katastar_outcome(
                        &best.entry_id,
                        false,
                        "katastar",
                        vkind,
                        None,
                        None,
                        &requested_model,
                    );
                    let _ = std::fs::write(&full_path, &active_file_content);
                    if !verify_detail.file_errors.is_empty() {
                        current_error_context = format!(
                            "{}\n\n[Live compiler diagnostics]:\n{}",
                            error_msg,
                            verify_detail.file_errors.join("\n")
                        );
                    }
                    let _ = window.emit("server:log", serde_json::json!({
                        "text": format!("> [Katastar] Pravilo '{}' nije rešilo '{}' ({}), vraćam prethodno stanje i pozivam AI...\n", best.entry_id, rel_file_path, vkind),
                        "type": "warning"
                    }));
                }
                SingleFileVerifyOutcome::Passed(vkind) => {
                    crate::katastar::finalize_katastar_outcome(
                        &best.entry_id,
                        true,
                        "katastar",
                        vkind,
                        None,
                        None,
                        &requested_model,
                    );
                    ensure_axiom_inspector_injected(&proj_dir);
                    let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Uspešno popravljeno!" }));
                    let _ = window.emit("server:log", serde_json::json!({
                        "text": format!("> [Katastar] Primenjeno i verifikovano pravilo '{}' ({}) bez pozivanja AI modela na: {}\n", best.entry_id, vkind, rel_file_path),
                        "type": "info"
                    }));
                    return Ok(true);
                }
                SingleFileVerifyOutcome::Inconclusive => {
                    crate::katastar::track_pending_project_repair(
                        &project_id,
                        &rel_file_path,
                        &sig_info.error_signature,
                        &best.entry_id,
                        "katastar",
                    );
                    ensure_axiom_inspector_injected(&proj_dir);
                    let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Uspešno popravljeno!" }));
                    let _ = window.emit("server:log", serde_json::json!({
                        "text": format!("> [Katastar] Primenjeno verifikovano pravilo '{}' bez pozivanja AI modela na: {}\n", best.entry_id, rel_file_path),
                        "type": "info"
                    }));
                    return Ok(true);
                }
            }
        }
    }

    // Phase B: Brutal Verified 3-Attempt AI Repair Loop
    let _ = ollama_start_server().await;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let model = resolve_valid_ollama_model(&client, &requested_model).await;

    let file_content = active_clean_content.clone();

    let mut system_prompt = "You are a Senior Next.js App Router Developer and TypeScript Compiler Surgeon. A file in the project failed with an error. \
        Instead of rewriting the entire file unnecessarily, you must identify the exact cause and provide valid target/replacement edits in JSON format.\
        \n\n\
        CRITICAL RULES:\n\
        1. Respond ONLY with a valid JSON object matching the format described below. Do not write any explanations, introductory text, or markdown code blocks (do not wrap in ```json). Just the raw JSON.\n\
        2. In the JSON structure, under 'edits', each 'target' block must exactly match (character-for-character, including leading whitespace and indentation) a unique substring in the target file. To create a missing local module file, set 'path' to its full path with extension (e.g. 'src/models/MaintenanceAlert.ts') and 'target' to \"\".\n\
        3. Do NOT include placeholders in 'replacement'. The code must be complete and ready to run.\n\
        4. Focus on resolving the reported error cleanly. Do not make unrelated changes.\n\
        5. DATABASE RESILIENCY: Domain models under 'src/models/' MUST be pure TypeScript interfaces/types ONLY (never import '@/lib/prisma' or 'new PrismaClient()'). If a server route or Prisma query fails at runtime, handle errors safely without importing Prisma into 'use client' files.\n\
        6. NEVER declare unquoted 'data-axiom-component' or 'data-axiom-file' inside TypeScript interfaces or type definitions. Leave existing HTML attributes on JSX tags untouched.\n\
        7. STRICT UI & DESIGN PRESERVATION: NEVER delete or strip UI components, hero sections, product grids, forms, tables, or buttons to bypass a compilation error. Fix the underlying TypeScript, import, export, or syntax error in place.\n\
        8. MANDATORY TAILWIND CSS: All JSX elements MUST continue to use comprehensive Tailwind CSS utility classes. NEVER degrade styled elements into raw, unstyled HTML.\n\
        9. STRICTLY NO PROSE IN CODE: NEVER put natural-language sentences, explanations, 'The error indicates...', or numbered lists ('1. Ensure...', '2. Verify...') inside 'replacement' or source code!\n\
        10. VALID PATHS & ALIASES ONLY: Local Next.js imports MUST use '@/' alias without file extension (e.g. `import { X } from '@/models/MaintenanceAlert'`, NEVER bare `'src/models/...'`). Every JSON edit `'path'` MUST include its proper file extension (e.g. `.ts`, `.tsx`, `.css`, `.prisma`), NEVER an extensionless module path.\n\
        \n\n\
        JSON FORMAT:\n\
        {\n\
          \"explanation\": \"Brief explanation of the compile error and how to fix it\",\n\
          \"edits\": [\n\
            {\n\
              \"path\": \"relative/path/to/file.ts\",\n\
              \"target\": \"exact string of lines in file to be replaced (or empty string ONLY when creating a new file)\",\n\
              \"replacement\": \"the valid source code to put in its place\"\n\
            }\n\
          ]\n\
        }".to_string();
    let pkg_ctx = format!("{}\n{}\n{}", rel_file_path, current_error_context, file_content);
    system_prompt.push_str(&crate::packages::get_relevant_packages_db_summary(Some(&proj_dir), &pkg_ctx));
    let rulepacks = get_stack_rulepacks(&rel_file_path, &file_content, Some(&current_error_context));
    if !rulepacks.is_empty() {
        system_prompt.push_str(&rulepacks);
    }
    if !katastar_lookup.hints.is_empty() {
        system_prompt.push_str("\n\n--- PREVIOUSLY VERIFIED SOLUTIONS FROM LOCAL KATASTAR ---\n");
        for h in katastar_lookup.hints.iter().take(3) {
            system_prompt.push_str(&format!("- {}\n", h));
        }
    }

    let runtime_cfg = get_ollama_runtime_config(&model);
    let compact_ctx = runtime_cfg.num_ctx <= 6144;
    let max_aux_total_chars: usize = if compact_ctx { 6500 } else { 14000 };
    let max_snippet_chars: usize = if compact_ctx { 1800 } else { 3200 };

    let path_lower = rel_file_path.to_lowercase();
    let is_nav_or_link = path_lower.contains("navbar")
        || path_lower.contains("sidebar")
        || path_lower.contains("footer")
        || path_lower.contains("header")
        || path_lower.contains("nav")
        || file_content.contains("<Link")
        || file_content.contains("next/link")
        || current_error_context.to_lowercase().contains("404");

    if is_nav_or_link {
        let project_routes = get_project_routes_from_dir(&proj_dir);
        let routes_prompt = format_routes_prompt(&project_routes);
        if !routes_prompt.is_empty() {
            system_prompt.push_str(&routes_prompt);
        }
    }

    let mut project_context = String::new();
    let mut seen_ctx_files: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen_ctx_files.insert(rel_file_path.to_lowercase());

    let features_path = proj_dir.join("axiom-features.json");
    if features_path.exists() {
        if let Ok(features_content) = std::fs::read_to_string(&features_path) {
            if let Ok(features_json) = serde_json::from_str::<serde_json::Value>(&features_content) {
                if let Some(feat_arr) = features_json.get("features").and_then(|f| f.as_array()) {
                    for feat in feat_arr {
                        if let Some(files) = feat.get("files").and_then(|fl| fl.as_array()) {
                            let matches = files.iter().any(|f| f.get("path").and_then(|p| p.as_str()) == Some(&rel_file_path));
                            if matches {
                                let feat_name = feat.get("name").and_then(|n| n.as_str()).unwrap_or("Feature");
                                let feat_desc = feat.get("description").and_then(|d| d.as_str()).unwrap_or("");
                                project_context.push_str(&format!(
                                    "\n--- Target Feature Scope: {} ---\nRole: {}\n---------------------------------\n",
                                    feat_name, feat_desc
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let contracts_prompt = crate::orchestrator::format_axiom_contracts_prompt(&proj_dir, &rel_file_path);
    if !contracts_prompt.is_empty() && project_context.len() < max_aux_total_chars {
        let capped_contracts = truncate_context_snippet(&contracts_prompt, if compact_ctx { 2000 } else { 4000 });
        project_context.push_str(&format!("\n{}\n", capped_contracts));
    }

    let related_files = extract_related_project_files_for_error(&proj_dir, &file_content, &rel_file_path, &current_error_context);
    let mut multi_file_candidates: Vec<String> = Vec::new();
    for (rel_dep_path, dep_content) in related_files {
        if project_context.len() >= max_aux_total_chars {
            break;
        }
        let norm_key = normalize_rel_path_str(&rel_dep_path).to_lowercase();
        if seen_ctx_files.insert(norm_key) {
            let dep_snippet = truncate_context_snippet(&dep_content, max_snippet_chars);
            project_context.push_str(&format!(
                "\n--- Related Dependency / Exporter File: {} ---\n{}\n----------------------------------------------------\n",
                rel_dep_path, dep_snippet
            ));
            multi_file_candidates.push(rel_dep_path);
        }
    }
    if !multi_file_candidates.is_empty() {
        let dep_list = multi_file_candidates.join(", ");
        system_prompt.push_str(&format!(
            "\n\nCRITICAL MULTI-FILE EDIT RULE:\n\
             The error in '{}' may be caused by missing or incorrect exports/definitions in related files ({}).\n\
             You are allowed to propose edits targeting those dependency files (set 'path' to their exact relative path) OR modify '{}'.",
            rel_file_path, dep_list, rel_file_path
        ));
    }

    let metadata_path = proj_dir.join(".axiom").join("metadata.json");
    if metadata_path.exists() && project_context.len() < max_aux_total_chars {
        if let Ok(meta_content) = std::fs::read_to_string(&metadata_path) {
            if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(&meta_content) {
                if let Some(deps) = meta_json.get("dependencies").and_then(|d| d.get(&rel_file_path)).and_then(|a| a.as_array()) {
                    let mut meta_added = 0usize;
                    for dep_val in deps {
                        if meta_added >= 3 || project_context.len() >= max_aux_total_chars {
                            break;
                        }
                        if let Some(dep_path_str) = dep_val.as_str() {
                            let norm_key = normalize_rel_path_str(dep_path_str).to_lowercase();
                            if !seen_ctx_files.insert(norm_key) {
                                continue;
                            }
                            let dep_full_path = proj_dir.join(dep_path_str);
                            if dep_full_path.exists() {
                                if let Ok(dep_content) = std::fs::read_to_string(&dep_full_path) {
                                    let dep_snippet = truncate_context_snippet(&dep_content, max_snippet_chars);
                                    project_context.push_str(&format!(
                                        "\n--- Related File Context: {} ---\n{}\n-----------------------------------\n",
                                        dep_path_str, dep_snippet
                                    ));
                                    meta_added += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if (rel_file_path.ends_with("page.tsx") || rel_file_path.ends_with("page.js")) && project_context.len() < max_aux_total_chars {
        for layout_name in &[
            "src/app/layout.tsx",
            "src/app/layout.jsx",
            "src/app/layout.js",
            "app/layout.tsx",
            "app/layout.jsx",
            "layout.tsx",
        ] {
            let norm_key = layout_name.to_lowercase();
            if seen_ctx_files.contains(&norm_key) {
                continue;
            }
            let layout_full_path = proj_dir.join(layout_name);
            if layout_full_path.exists() {
                if let Ok(layout_content) = std::fs::read_to_string(&layout_full_path) {
                    seen_ctx_files.insert(norm_key);
                    let clean_layout = strip_axiom_inspector_script(&layout_content);
                    let layout_snippet = truncate_context_snippet(&clean_layout, max_snippet_chars);
                    project_context.push_str(&format!(
                        "\n--- Project Layout File: {} ---\n{}\n-------------------------------------\n",
                        layout_name, layout_snippet
                    ));
                    break;
                }
            }
        }
    }

    if current_error_context.contains("Can't resolve")
        || current_error_context.contains("Cannot find module")
        || current_error_context.contains("Module not found")
        || current_error_context.contains("TS2307")
    {
        let existing_mods = list_project_src_modules(&proj_dir, if compact_ctx { 40 } else { 80 });
        if !existing_mods.is_empty() {
            project_context.push_str(&format!(
                "\n--- Existing Project Source Files (import via `@/...` without extension) ---\n{}\n--------------------------------------------------------------------------\n",
                existing_mods.join("\n")
            ));
        }
    }

    const MAX_AI_ATTEMPTS: usize = 3;
    let mut attempt_feedback = String::new();
    let mut last_failure_reason = "Model nije uspeo da proizvede verifikovanu ispravku.".to_string();
    let mut recorded_any_failure_in_katastar = false;

    for ai_attempt in 1..=MAX_AI_ATTEMPTS {
        let _ = window.emit("server:heal-status", serde_json::json!({
            "status": format!("Pozivanje AI modela ({}, pokušaj {}/{})...", model, ai_attempt, MAX_AI_ATTEMPTS)
        }));
        let _ = window.emit("server:log", serde_json::json!({
            "text": format!("> [AI Auto-Healing] Pozivam model '{}' (pokušaj {}/{}) sa opisom greške...\n", model, ai_attempt, MAX_AI_ATTEMPTS),
            "type": "info"
        }));

        let feedback_block = if attempt_feedback.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nPREVIOUS ATTEMPT FAILED VERIFICATION — FIX THIS CAREFULLY:\n{}\nDo NOT repeat this mistake!\n",
                attempt_feedback
            )
        };

        let user_prompt = format!(
            "File: {}\n\nCompilation Error Details:\n{}\n\nFile Current Content:\n{}\n{}{}{}\n\nPlease analyze the error and output ONLY valid JSON edits matching the required format.",
            rel_file_path,
            current_error_context,
            file_content,
            if project_context.is_empty() { "" } else { "\nAdditional Project Context:" },
            project_context,
            feedback_block
        );

        let temp = if ai_attempt == 1 { 0.1 } else { 0.2 };
        let ollama_req = serde_json::json!({
            "model": model,
            "format": "json",
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": false,
            "options": build_ollama_options(&model, temp, Some(4096))
        });

        let resp = match client.post("http://127.0.0.1:11434/api/chat").json(&ollama_req).send().await {
            Ok(r) => r,
            Err(e) => {
                last_failure_reason = format!("Greška u komunikaciji sa Ollama serverom: {}", e);
                attempt_feedback = last_failure_reason.clone();
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [AI Auto-Healing] Pokušaj {}/{}: {}\n", ai_attempt, MAX_AI_ATTEMPTS, last_failure_reason),
                    "type": "warning"
                }));
                let _ = ollama_start_server().await;
                continue;
            }
        };

        if !resp.status().is_success() {
            last_failure_reason = format!("Ollama HTTP status {}", resp.status());
            attempt_feedback = last_failure_reason.clone();
            continue;
        }

        let Ok(resp_json) = resp.json::<serde_json::Value>().await else {
            last_failure_reason = "Neispravan JSON odgovor sa Ollama servera.".to_string();
            attempt_feedback = last_failure_reason.clone();
            continue;
        };

        let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) else {
            last_failure_reason = "Model je vratio prazan odgovor.".to_string();
            attempt_feedback = last_failure_reason.clone();
            continue;
        };

        let _ = window.emit("server:heal-status", serde_json::json!({
            "status": format!("Validacija i provera koda (pokušaj {}/{})...", ai_attempt, MAX_AI_ATTEMPTS)
        }));
        debug_log_to_file(format!("[AI Auto-Healing Attempt {}/{}] Raw AI response:\n{}", ai_attempt, MAX_AI_ATTEMPTS, ai_msg));
        let clean_content = extract_code_content(ai_msg);

        let mut resolved_edits: Vec<Value> = Vec::new();
        let mut explanation = "Popravka generisana od strane AI modela".to_string();

        let maybe_json: Option<serde_json::Value> = serde_json::from_str(&clean_content).ok().or_else(|| {
            if let (Some(s), Some(e)) = (clean_content.find('{'), clean_content.rfind('}')) {
                if e > s {
                    return serde_json::from_str(&clean_content[s..=e]).ok();
                }
            }
            if let (Some(s), Some(e)) = (clean_content.find('['), clean_content.rfind(']')) {
                if e > s {
                    return serde_json::from_str(&clean_content[s..=e]).ok();
                }
            }
            None
        });

        if let Some(ref parsed_json) = maybe_json {
            if let Some(exp) = parsed_json.get("explanation").or_else(|| parsed_json.get("reason")).and_then(|e| e.as_str()) {
                explanation = exp.to_string();
            }
            resolved_edits = extract_edits_from_json(parsed_json, &rel_file_path);
        }

        resolved_edits.retain(|e| {
            let t = e.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let unquoted = t.trim_matches('\'').trim_matches('"').trim();
            t.is_empty() || !unquoted.chars().all(|c| c.is_ascii_digit())
        });

        if resolved_edits.is_empty() {
            if let Some(malformed_edits) = extract_code_from_malformed_json_response(&clean_content, &rel_file_path) {
                debug_log_to_file(format!("[AI Auto-Healing] Extracted embedded code from malformed JSON for: {}", rel_file_path));
                resolved_edits = malformed_edits;
                explanation = "Izvučen popravljen kod iz AI odgovora".to_string();
            }
        }

        if resolved_edits.is_empty() {
            let trimmed = clean_content.trim();
            let is_raw_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
                || (trimmed.starts_with('[') && trimmed.ends_with(']'));
            let is_json_envelope = trimmed.contains("\"files\":")
                || trimmed.contains("\"content\":")
                || trimmed.contains("\"edits\":")
                || trimmed.contains("\"target\":")
                || trimmed.contains("\"replacement\":")
                || trimmed.contains("\"response\":");

            if !is_raw_json && !is_json_envelope && looks_like_code_payload(&clean_content) {
                let existing_line_count = file_content.lines().count();
                let new_line_count = trimmed.lines().count();
                let is_tiny_snippet = new_line_count < 4 && existing_line_count > 6;
                if !is_tiny_snippet || trimmed.starts_with("import ") || trimmed.starts_with("export ") {
                    debug_log_to_file(format!("[AI Auto-Healing] Whole-file code fallback activated for: {}", rel_file_path));
                    resolved_edits.push(serde_json::json!({
                        "path": rel_file_path.clone(),
                        "target": "",
                        "replacement": if is_tiny_snippet { trimmed.to_string() } else { clean_content.clone() }
                    }));
                    explanation = "Primenjen kompletan popravljen kod iz AI odgovora".to_string();
                }
            }
        }

        resolved_edits.retain(|e| {
            let t = e.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let unquoted = t.trim_matches('\'').trim_matches('"').trim();
            t.is_empty() || !unquoted.chars().all(|c| c.is_ascii_digit())
        });

        if resolved_edits.is_empty() {
            last_failure_reason = "AI odgovor nije sadržao validan kod ili ciljanu izmenu (odbijena proza, neispravna putanja bez ekstenzije ili nevažeći JSON).".to_string();
            attempt_feedback = last_failure_reason.clone();
            let _ = window.emit("server:log", serde_json::json!({
                "text": format!("> [AI Auto-Healing] Pokušaj {}/{} odbijen: {}\n", ai_attempt, MAX_AI_ATTEMPTS, last_failure_reason),
                "type": "warning"
            }));
            continue;
        }

        // Apply all edits in memory and validate BEFORE touching disk
        let mut modified_files: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut before_contents: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut attempt_snapshots: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
        let mut edit_reject_reason: Option<String> = None;

        for edit in resolved_edits {
            let raw_edit_path = edit
                .get("path")
                .and_then(|p| p.as_str())
                .map(|p| normalize_rel_path_str(p))
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| rel_file_path.clone());

            let edit_path = resolve_project_file_candidate(&proj_dir, &raw_edit_path)
                .map(|p| normalize_rel_path_str(&p))
                .unwrap_or(raw_edit_path);

            if !is_valid_file_path(&edit_path) {
                edit_reject_reason = Some(format!(
                    "Neispravna ciljana putanja fajla '{}' (putanja mora imati ekstenziju npr. .ts/.tsx i ne sme biti go import specifer).",
                    edit_path
                ));
                break;
            }

            let target_full_path = proj_dir.join(&edit_path);
            if !target_full_path.starts_with(&proj_dir) {
                edit_reject_reason = Some(format!("Putanja '{}' izlazi van direktorijuma projekta.", edit_path));
                break;
            }

            let Some(target) = edit.get("target").and_then(|t| t.as_str()) else {
                edit_reject_reason = Some("Editu nedostaje polje 'target'.".to_string());
                break;
            };
            let Some(replacement) = edit.get("replacement").and_then(|r| r.as_str()) else {
                edit_reject_reason = Some("Editu nedostaje polje 'replacement'.".to_string());
                break;
            };

            let edit_lower = edit_path.to_lowercase();
            let is_md = edit_lower.ends_with(".md") || edit_lower.ends_with(".mdx");
            let is_json = edit_lower.ends_with(".json");
            if !is_md && has_prose_leak_in_code(replacement) {
                edit_reject_reason = Some(format!(
                    "Odbijena izmena za '{}': model je ubacio prozu/objašnjenje u polje 'replacement'.",
                    edit_path
                ));
                break;
            }

            if !modified_files.contains_key(&edit_path) {
                if !target_full_path.exists() {
                    attempt_snapshots.insert(edit_path.clone(), None);
                    modified_files.insert(edit_path.clone(), String::new());
                    before_contents.insert(edit_path.clone(), String::new());
                } else {
                    match std::fs::read_to_string(&target_full_path) {
                        Ok(content) => {
                            attempt_snapshots.insert(edit_path.clone(), Some(content.clone()));
                            before_contents.insert(edit_path.clone(), strip_axiom_inspector_script(&content));
                            modified_files.insert(edit_path.clone(), content);
                        }
                        Err(e) => {
                            edit_reject_reason = Some(format!("Greška pri čitanju fajla {}: {}", edit_path, e));
                            break;
                        }
                    }
                }
            }

            let file_content_mut = modified_files.get_mut(&edit_path).unwrap();

            if target.is_empty() {
                let repl_trimmed = replacement.trim();
                let payload_ok = if repl_trimmed.is_empty() {
                    false
                } else if is_json {
                    serde_json::from_str::<Value>(repl_trimmed).is_ok()
                } else if is_md {
                    true
                } else {
                    looks_like_code_payload(repl_trimmed)
                };
                if !payload_ok {
                    edit_reject_reason = Some(format!(
                        "Odbijena puna zamena za '{}': sadržaj nije validan kod ili JSON.",
                        edit_path
                    ));
                    break;
                }

                let is_just_import = (repl_trimmed.starts_with("import ") || repl_trimmed.starts_with("import{") || repl_trimmed.starts_with("import type"))
                    && !repl_trimmed.contains("export ")
                    && !repl_trimmed.contains("function ")
                    && !repl_trimmed.contains("const ")
                    && !repl_trimmed.contains("default ")
                    && !repl_trimmed.contains("class ");

                let is_just_export = (repl_trimmed.starts_with("export ") || repl_trimmed.starts_with("export default"))
                    && !repl_trimmed.contains("function ")
                    && !repl_trimmed.contains("class ")
                    && !repl_trimmed.contains("const ")
                    && !repl_trimmed.contains("let ")
                    && !repl_trimmed.contains("var ")
                    && !repl_trimmed.contains("return ")
                    && !repl_trimmed.contains('<');

                let is_partial_helper_snippet = !file_content_mut.is_empty()
                    && file_content_mut.contains("export default")
                    && !repl_trimmed.contains("export default")
                    && repl_trimmed.lines().count() <= 10;

                if is_just_import && !file_content_mut.is_empty() {
                    if !file_content_mut.contains(repl_trimmed) {
                        *file_content_mut = insert_import_after_use_client(file_content_mut, repl_trimmed);
                        *file_content_mut = hoist_use_client_directive(file_content_mut);
                    }
                } else if is_just_export && !file_content_mut.is_empty() {
                    if !file_content_mut.contains(repl_trimmed) {
                        file_content_mut.push_str(&format!("\n{}\n", repl_trimmed));
                    }
                } else if is_partial_helper_snippet {
                    if !file_content_mut.contains(repl_trimmed) {
                        if let Some(idx) = file_content_mut.find("export default") {
                            file_content_mut.insert_str(idx, &format!("{}\n\n", repl_trimmed));
                        }
                    }
                } else {
                    *file_content_mut = replacement.to_string();
                }
            } else {
                match apply_single_targeted_edit_with_quote_fallback(file_content_mut, target, replacement) {
                    Ok(new_content) => {
                        *file_content_mut = new_content;
                    }
                    Err(err) => {
                        edit_reject_reason = Some(format!(
                            "Ciljani 'target' blok nije pronađen u '{}': {}. Proveri tačan tekst linija iz fajla.",
                            edit_path, err
                        ));
                        break;
                    }
                }
            }
        }

        if let Some(reason) = edit_reject_reason {
            last_failure_reason = reason.clone();
            attempt_feedback = reason;
            let _ = window.emit("server:log", serde_json::json!({
                "text": format!("> [AI Auto-Healing] Pokušaj {}/{} odbijen pre upisa: {}\n", ai_attempt, MAX_AI_ATTEMPTS, last_failure_reason),
                "type": "warning"
            }));
            continue;
        }

        // Run post-processing & static validation gate in memory before writing to disk
        let project_routes = get_project_routes_from_dir(&proj_dir);
        let routes_list: Vec<String> = project_routes.into_iter().map(|(r, _)| r).collect();
        let mut prepared_writes: Vec<(String, String, String, String)> = Vec::new();
        let mut validation_reject: Option<String> = None;
        let mut any_effective_change = false;

        for (edit_path, modified_content) in modified_files {
            let mut processed = post_process_generated_file(&edit_path, &modified_content, &serde_json::json!({}), has_tailwind);
            if !routes_list.is_empty() {
                processed = sanitize_app_router_links_with_routes(&processed, &routes_list);
            }
            processed = sanitize_unhandled_session_loading(&processed);

            if let Err(val_err) = crate::orchestrator::validate_file_with_abstract_rules(&edit_path, &processed, &model).await {
                validation_reject = Some(format!("Statička validacija odbila kandidat za '{}': {}", edit_path, val_err));
                break;
            }

            let before_clean = before_contents.get(&edit_path).cloned().unwrap_or_default();
            let after_clean = strip_axiom_inspector_script(&processed);
            if !before_clean.is_empty() {
                if let Some(drop_reason) = detect_destructive_code_drop(&edit_path, &before_clean, &after_clean) {
                    validation_reject = Some(drop_reason);
                    break;
                }
            }
            let orig_disk = attempt_snapshots.get(&edit_path).and_then(|o| o.as_ref()).map(|s| s.as_str()).unwrap_or("");
            if processed != orig_disk && before_clean != after_clean {
                any_effective_change = true;
            }
            prepared_writes.push((edit_path, processed, before_clean, after_clean));
        }

        if let Some(reason) = validation_reject {
            last_failure_reason = reason.clone();
            attempt_feedback = reason;
            let _ = window.emit("server:log", serde_json::json!({
                "text": format!("> [AI Auto-Healing] Pokušaj {}/{} pao statičku kontrolu: {}\n", ai_attempt, MAX_AI_ATTEMPTS, last_failure_reason),
                "type": "warning"
            }));
            continue;
        }

        if !any_effective_change {
            last_failure_reason = "AI predložena izmena nije promenila sadržaj fajla.".to_string();
            attempt_feedback = last_failure_reason.clone();
            let _ = window.emit("server:log", serde_json::json!({
                "text": format!("> [AI Auto-Healing] Pokušaj {}/{} nije doneo efektivnu promenu koda.\n", ai_attempt, MAX_AI_ATTEMPTS),
                "type": "warning"
            }));
            continue;
        }

        let _ = window.emit("server:heal-status", serde_json::json!({
            "status": format!("Upisivanje i verifikacija popravke (pokušaj {}/{})...", ai_attempt, MAX_AI_ATTEMPTS)
        }));

        let mut written_changes: Vec<(String, String, String)> = Vec::new();
        let mut edited_paths: Vec<String> = Vec::new();
        for (edit_path, processed, before_clean, after_clean) in prepared_writes {
            let target_full_path = proj_dir.join(&edit_path);
            if let Some(parent) = target_full_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&target_full_path, &processed) {
                restore_attempt_snapshots(&proj_dir, &attempt_snapshots);
                let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška pri upisu" }));
                return Err(format!("Greška pri upisu fajla {} na disk: {}", edit_path, e));
            }
            if before_clean != after_clean && !after_clean.trim().is_empty() {
                written_changes.push((edit_path.clone(), before_clean, after_clean));
            }
            edited_paths.push(edit_path);
        }

        let verify_detail = verify_single_file_detail(
            &proj_dir,
            &rel_file_path,
            &edited_paths,
            &sig_info.category,
            sig_info.error_code.as_deref(),
        )
        .await;

        let mut registered_entries: Vec<(String, String, Option<crate::katastar::KatastarPatch>)> = Vec::new();
        for (edit_path, before_clean, after_clean) in &written_changes {
            let atomic_patch = if !before_clean.is_empty() {
                crate::katastar::extract_safe_atomic_patch(edit_path, before_clean, after_clean)
            } else {
                None
            };
            let strategy_name = if atomic_patch.is_some() {
                "search_replace"
            } else if before_clean.is_empty() {
                "ai_create_file"
            } else {
                "ai_edit"
            };
            let eid = crate::katastar::register_repair_attempt(
                &sig_info.category,
                &error_msg,
                edit_path,
                "ai",
                strategy_name,
                atomic_patch.clone(),
                &explanation,
                &model,
            );
            registered_entries.push((edit_path.clone(), eid, atomic_patch));
        }

        match verify_detail.outcome {
            SingleFileVerifyOutcome::Passed(vkind) => {
                for (_, eid, atomic_patch) in registered_entries {
                    crate::katastar::finalize_katastar_outcome(
                        &eid,
                        true,
                        "ai",
                        vkind,
                        atomic_patch,
                        Some(&explanation),
                        &model,
                    );
                }
                ensure_axiom_inspector_injected(&proj_dir);
                for ep in &edited_paths {
                    let _ = window.emit("server:log", serde_json::json!({
                        "text": format!("> [AI Auto-Healing] Uspešno popravljen i verifikovan ({}) fajl: {}\n", vkind, ep),
                        "type": "info"
                    }));
                    debug_log_to_file(format!("[AI Auto-Healing] Verified ({}) heal on file: {}", vkind, ep));
                }
                let _ = window.emit("server:heal-status", serde_json::json!({
                    "status": "Završeno!"
                }));
                return Ok(true);
            }
            SingleFileVerifyOutcome::Inconclusive => {
                for (edit_path, eid, _) in registered_entries {
                    crate::katastar::track_pending_project_repair(
                        &project_id,
                        &edit_path,
                        &sig_info.error_signature,
                        &eid,
                        "ai",
                    );
                }
                ensure_axiom_inspector_injected(&proj_dir);
                for ep in &edited_paths {
                    let _ = window.emit("server:log", serde_json::json!({
                        "text": format!("> [AI Auto-Healing] Primenjena AI analiza ({}) na fajl: {}\n", explanation, ep),
                        "type": "info"
                    }));
                }
                let _ = window.emit("server:heal-status", serde_json::json!({
                    "status": "Završeno!"
                }));
                return Ok(true);
            }
            SingleFileVerifyOutcome::Failed(vkind) => {
                for (_, eid, _) in registered_entries {
                    recorded_any_failure_in_katastar = true;
                    crate::katastar::finalize_katastar_outcome(
                        &eid,
                        false,
                        "ai",
                        vkind,
                        None,
                        None,
                        &model,
                    );
                }
                restore_attempt_snapshots(&proj_dir, &attempt_snapshots);

                let err_summary = if !verify_detail.file_errors.is_empty() {
                    verify_detail.file_errors.iter().take(6).cloned().collect::<Vec<String>>().join("\n")
                } else {
                    verify_detail.raw_output.lines().take(6).collect::<Vec<&str>>().join("\n")
                };
                last_failure_reason = format!("Verifikacija ({}) pala: {}", vkind, err_summary);
                attempt_feedback = format!(
                    "Previous AI edit failed {} check and was ROLLED BACK. Exact compiler errors:\n{}",
                    vkind, err_summary
                );
                if !verify_detail.file_errors.is_empty() {
                    current_error_context = format!(
                        "{}\n\n[Live compiler diagnostics]:\n{}",
                        error_msg, err_summary
                    );
                }
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [AI Auto-Healing] Pokušaj {}/{} nije prošao verifikaciju ({}). Vraćam fajl na prethodno stanje i pokušavam ponovo...\n", ai_attempt, MAX_AI_ATTEMPTS, vkind),
                    "type": "warning"
                }));
                debug_log_to_file(format!(
                    "[AI Auto-Healing] Rolled back attempt {}/{} on '{}' ({}): {}",
                    ai_attempt, MAX_AI_ATTEMPTS, rel_file_path, vkind, err_summary
                ));
            }
        }
    }

    if !recorded_any_failure_in_katastar {
        let eid = crate::katastar::register_repair_attempt(
            &sig_info.category,
            &error_msg,
            &rel_file_path,
            "ai",
            "ai_edit",
            None,
            &last_failure_reason,
            &model,
        );
        crate::katastar::finalize_katastar_outcome(
            &eid,
            false,
            "ai",
            "validation_rejected",
            None,
            None,
            &model,
        );
    }

    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": "Greška: Popravka nije uspela"
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing Error] Nije uspela verifikovana popravka fajla '{}' posle {} pokušaja: {}\n", rel_file_path, MAX_AI_ATTEMPTS, last_failure_reason),
        "type": "error"
    }));
    Err(format!("Nije uspela popravka fajla {} posle {} pokušaja: {}", rel_file_path, MAX_AI_ATTEMPTS, last_failure_reason))
}

#[tauri::command]
pub async fn project_install_package(
    project_id: String,
    package_name: String,
    window: tauri::Window,
) -> Result<bool, String> {
    let raw_pkg = package_name.trim();
    let lower_pkg = raw_pkg.to_lowercase();
    let looks_like_file_or_local = crate::packages::is_local_import_specifier(raw_pkg)
        || raw_pkg.contains('\\')
        || raw_pkg.contains(' ')
        || lower_pkg == "package.json"
        || lower_pkg.ends_with(".ts")
        || lower_pkg.ends_with(".tsx")
        || lower_pkg.ends_with(".js")
        || lower_pkg.ends_with(".jsx")
        || lower_pkg.ends_with(".css")
        || lower_pkg.ends_with(".scss")
        || lower_pkg.ends_with(".prisma")
        || lower_pkg.ends_with(".json");

    if looks_like_file_or_local {
        let msg = format!(
            "'{}' je lokalna putanja modula u projektu, a ne npm paket. Koristi 'Popravi sa AI' za ispravku importa ili kreiranje fajla.",
            raw_pkg
        );
        let _ = window.emit("server:heal-status", serde_json::json!({
            "status": "Nije npm paket"
        }));
        let _ = window.emit("server:log", serde_json::json!({
            "text": format!("> [Self-Healing Guard] {}\n", msg),
            "type": "warning"
        }));
        return Err(msg);
    }

    // Normalize subpath specifiers (e.g. `next-auth/providers/credentials` -> `next-auth`, `@mui/material/Button` -> `@mui/material`)
    let root_pkg = {
        let parts: Vec<&str> = raw_pkg.split('/').collect();
        if raw_pkg.starts_with('@') && parts.len() >= 2 {
            format!("{}/{}", parts[0], parts[1])
        } else {
            parts.first().copied().unwrap_or(raw_pkg).to_string()
        }
    };

    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": format!("Instalacija paketa {}...", root_pkg)
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing] Detektovan nedostajući paket. Pokrećem instalaciju: {}\n", root_pkg),
        "type": "info"
    }));

    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }

    let enriched_path = ensure_node_toolchain_ready(Some(&window), "server:log").await;
    let pkg_json_path = proj_dir.join("package.json");
    if sanitize_package_json_for_offline(&pkg_json_path) {
        let _ = window.emit("server:log", serde_json::json!({
            "text": "> [Offline Sanitizer] Očišćen package.json od konfliktnih dev/linter paketa pre instalacije.\n".to_string(),
            "type": "info"
        }));
    }

    let _ = window.emit("server:log", serde_json::json!({
        "text": "> [AI Auto-Healing] Pokrećem npm install za paket...\n".to_string(),
        "type": "info"
    }));

    let mut pkgs_to_install = vec![root_pkg.clone()];
    if root_pkg == "@emotion/styled" || root_pkg == "@mui/material" {
        pkgs_to_install.push("@emotion/react".to_string());
    } else if root_pkg == "@emotion/react" {
        pkgs_to_install.push("@emotion/styled".to_string());
    }

    let mut last_err: Option<String> = None;
    let mut last_raw_npm_err = String::new();
    let mut prefer_offline = true;
    for _attempt in 1..=4 {
        #[cfg(target_os = "windows")]
        let mut cmd = tokio::process::Command::new("cmd");
        #[cfg(target_os = "windows")]
        {
            cmd.arg("/C").arg("npm").arg("install").arg("--legacy-peer-deps");
            if prefer_offline {
                cmd.arg("--prefer-offline");
            }
            for p in &pkgs_to_install {
                cmd.arg(p);
            }
            cmd.current_dir(&proj_dir).env("PATH", &enriched_path);
        }

        #[cfg(not(target_os = "windows"))]
        let mut cmd = tokio::process::Command::new("npm");
        #[cfg(not(target_os = "windows"))]
        {
            cmd.arg("install").arg("--legacy-peer-deps");
            if prefer_offline {
                cmd.arg("--prefer-offline");
            }
            for p in &pkgs_to_install {
                cmd.arg(p);
            }
            cmd.current_dir(&proj_dir).env("PATH", &enriched_path);
        }

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.output().await {
            Ok(output) => {
                if output.status.success() {
                    lock_installed_dependencies(&pkg_json_path);
                    let eid = crate::katastar::register_repair_attempt(
                        "npm",
                        &format!("missing package '{}'", root_pkg),
                        "package.json",
                        "npm",
                        "npm_action",
                        None,
                        &format!("Installed missing npm package(s): {}", pkgs_to_install.join(", ")),
                        "",
                    );
                    crate::katastar::finalize_katastar_outcome(
                        &eid,
                        true,
                        "npm",
                        "npm_install",
                        None,
                        None,
                        "",
                    );
                    let _ = window.emit("server:heal-status", serde_json::json!({
                        "status": "Instalirano!"
                    }));
                    let _ = window.emit("server:log", serde_json::json!({
                        "text": format!("> [AI Auto-Healing] Uspešno instaliran paket {}.\n", root_pkg),
                        "type": "info"
                    }));
                    return Ok(true);
                } else {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let combined_err = format!("{}\n{}", stdout, stderr);
                    last_raw_npm_err = combined_err.clone();
                    last_err = Some(format!("npm install failed: {}", stderr));

                    if is_npm_network_error(&combined_err) {
                        let offline_msg = format!(
                            "Nema internet konekcije ili je npm registar nedostupan — nastavljamo sa instalacijom paketa '{}' kada budemo ponovo online.",
                            root_pkg
                        );
                        last_err = Some(offline_msg.clone());
                        let _ = window.emit("server:log", serde_json::json!({
                            "text": format!("> [Offline / Network Error] {}\n", offline_msg),
                            "type": "error"
                        }));
                        break;
                    }

                    if prefer_offline {
                        prefer_offline = false;
                        let _ = window.emit("server:log", serde_json::json!({
                            "text": format!("> [AI Auto-Healing] Offline npm cache nije uspeo za {}, prelazim na online instalaciju...\n", root_pkg),
                            "type": "info"
                        }));
                        continue;
                    }

                    let mut healed_conflict = false;
                    if let Some(action) = analyze_npm_install_failure(&combined_err) {
                        match action {
                            NpmFailureAction::SetWildcard(conflict_pkg) => {
                                if is_core_framework_package(&conflict_pkg) {
                                    let _ = window.emit("server:log", serde_json::json!({
                                        "text": format!("> [Core Lock] Paket '{}' je zaključani core framework paket i neće biti spušten na '*' tokom instalacije '{}'.\n", conflict_pkg, root_pkg),
                                        "type": "error"
                                    }));
                                } else if apply_npm_etarget_heal(&pkg_json_path, &conflict_pkg, Some(&combined_err)) {
                                    let _ = window.emit("server:log", serde_json::json!({
                                        "text": format!("> [Self-Healing] Primenjen bezbedan dependency/parent-scoped override za konfliktan paket '{}' tokom instalacije '{}'. Ponovo pokušavam...\n", conflict_pkg, root_pkg),
                                        "type": "info"
                                    }));
                                    healed_conflict = true;
                                }
                            }
                            NpmFailureAction::Remove(conflict_pkg) if conflict_pkg != root_pkg => {
                                if is_core_framework_package(&conflict_pkg) {
                                    let _ = window.emit("server:log", serde_json::json!({
                                        "text": format!("> [Core Lock] Paket '{}' je zaključani core framework paket i neće biti uklonjen tokom instalacije '{}'.\n", conflict_pkg, root_pkg),
                                        "type": "error"
                                    }));
                                } else if remove_package_from_json(&pkg_json_path, &conflict_pkg) {
                                    let _ = window.emit("server:log", serde_json::json!({
                                        "text": format!("> [Self-Healing] Uklonjen nepostojeći/nevažeći paket '{}' iz package.json tokom instalacije '{}'. Ponovo pokušavam...\n", conflict_pkg, root_pkg),
                                        "type": "info"
                                    }));
                                    healed_conflict = true;
                                }
                            }
                            NpmFailureAction::Remove(_) => {}
                        }
                    }

                    if !healed_conflict {
                        break;
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to run npm install command: {}", e);
                let eid = crate::katastar::register_repair_attempt(
                    "npm",
                    &format!("missing package '{}': {}", root_pkg, err_msg),
                    "package.json",
                    "npm",
                    "npm_action",
                    None,
                    &format!("npm install execution failed for {}", root_pkg),
                    "",
                );
                crate::katastar::finalize_katastar_outcome(
                    &eid,
                    false,
                    "npm",
                    "npm_install",
                    None,
                    None,
                    "",
                );
                return Err(err_msg);
            }
        }
    }

    let final_err_sample: String = format!(
        "missing package '{}'\n{}",
        root_pkg,
        if last_raw_npm_err.is_empty() {
            last_err.as_deref().unwrap_or("npm install failed")
        } else {
            &last_raw_npm_err
        }
    )
    .chars()
    .take(650)
    .collect();
    let eid = crate::katastar::register_repair_attempt(
        "npm",
        &final_err_sample,
        "package.json",
        "npm",
        "npm_action",
        None,
        last_err.as_deref().unwrap_or("npm install failed"),
        "",
    );
    crate::katastar::finalize_katastar_outcome(
        &eid,
        false,
        "npm",
        "npm_install",
        None,
        None,
        "",
    );

    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": "Greška pri instalaciji"
    }));
    Err(last_err.unwrap_or_else(|| "npm install failed".to_string()))
}

#[tauri::command]
pub async fn project_run_prisma_generate(project_id: String) -> Result<String, String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory does not exist".to_string());
    }
    
    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if !has_prisma {
        return Err("No Prisma schema found in this project".to_string());
    }
    
    let local_prisma_js = proj_dir.join("node_modules").join("prisma").join("build").join("index.js");
    let mut cmd = if local_prisma_js.exists() {
        let mut c = Command::new("node");
        c.arg(&local_prisma_js).arg("generate").current_dir(&proj_dir);
        c
    } else {
        #[cfg(target_os = "windows")]
        let mut c = Command::new("cmd");
        #[cfg(target_os = "windows")]
        c.arg("/C").arg("npx").arg("-y").arg("prisma@5.22.0").arg("generate").current_dir(&proj_dir);
        
        #[cfg(not(target_os = "windows"))]
        let mut c = Command::new("npx");
        #[cfg(not(target_os = "windows"))]
        c.arg("-y").arg("prisma@5.22.0").arg("generate").current_dir(&proj_dir);
        c
    };
    
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    
    let output = cmd.output().await
        .map_err(|e| format!("Failed to run prisma generate command: {}", e))?;
        
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if output.status.success() {
        Ok(stdout.into_owned())
    } else {
        Err(format!("Prisma generate failed:\n{}\n{}", stdout, stderr))
    }
}

pub fn generate_project_metadata(proj_dir: &std::path::Path, project_id: &str, files_list: &[Value]) {
    let mut dependencies = HashMap::new();

    let mut project_files = Vec::new();
    for file_spec in files_list {
        if let Some(path_str) = file_spec.get("path").and_then(|p| p.as_str()) {
            project_files.push(path_str.replace('\\', "/"));
        }
    }

    for file_path in &project_files {
        let full_path = proj_dir.join(file_path);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let mut file_deps = Vec::new();
            
            for line in content.lines() {
                let line_trimmed = line.trim();
                if line_trimmed.starts_with("import ") || line_trimmed.contains("require(") {
                    let quotes = ['\'', '"', '`'];
                    let mut found_path = None;
                    
                    for q in &quotes {
                        let parts: Vec<&str> = line_trimmed.split(*q).collect();
                        if parts.len() >= 3 {
                            for part in parts.iter().skip(1).step_by(2) {
                                if part.starts_with('.') || part.contains('/') {
                                    found_path = Some(part.to_string());
                                    break;
                                }
                            }
                        }
                    }
                    
                    if let Some(import_path) = found_path {
                        let candidate_path = if let Some(stripped) = import_path.strip_prefix("@/") {
                            format!("src/{}", stripped)
                        } else if let Some(stripped) = import_path.strip_prefix("~/") {
                            format!("src/{}", stripped)
                        } else if let Some(parent_dir) = std::path::Path::new(file_path).parent() {
                            let resolved = parent_dir.join(&import_path);
                            let mut normalized_parts = Vec::new();
                            for component in resolved.components() {
                                match component {
                                    std::path::Component::Normal(c) => {
                                        if let Some(c_str) = c.to_str() {
                                            normalized_parts.push(c_str);
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
                            continue;
                        };

                        for pf in &project_files {
                            let pf_norm = pf.replace('\\', "/");
                            let pf_no_ext = std::path::Path::new(&pf_norm).with_extension("");
                            let pf_no_ext_str = pf_no_ext.to_str().unwrap_or("");
                            if pf_no_ext_str == candidate_path
                                || pf_norm == candidate_path
                                || pf_norm == format!("{}.tsx", candidate_path)
                                || pf_norm == format!("{}.ts", candidate_path)
                                || pf_norm == format!("{}/index.ts", candidate_path)
                                || pf_norm == format!("{}/index.tsx", candidate_path)
                            {
                                if !file_deps.contains(&pf_norm) {
                                    file_deps.push(pf_norm);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            dependencies.insert(file_path.clone(), file_deps);
        }
    }

    let metadata = serde_json::json!({
        "projectId": project_id,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "dependencies": dependencies
    });

    let axiom_dir = proj_dir.join(".axiom");
    let _ = fs::create_dir_all(&axiom_dir);
    if let Ok(meta_content) = serde_json::to_string_pretty(&metadata) {
        let _ = fs::write(axiom_dir.join("metadata.json"), meta_content);
    }
}

