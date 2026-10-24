use crate::*;

pub fn resolve_import_path(proj_dir: &std::path::Path, import_path: &str, current_file: &str) -> Option<String> {
    let clean_path = import_path.trim().replace('\'', "").replace('"', "");
    let mut resolved = None;
    
    if clean_path.starts_with("@/") {
        let suffix = &clean_path[2..];
        let test_src = proj_dir.join("src").join(suffix);
        if test_src.exists() || proj_dir.join("src").join(format!("{}.tsx", suffix)).exists() || proj_dir.join("src").join(format!("{}.ts", suffix)).exists() {
            resolved = Some(format!("src/{}", suffix));
        } else {
            resolved = Some(suffix.to_string());
        }
    } else if clean_path.starts_with('.') {
        if let Some(parent) = std::path::Path::new(current_file).parent() {
            let path = parent.join(&clean_path);
            resolved = path.to_str().map(|s| s.replace('\\', "/"));
        }
    }
    
    if let Some(path_str) = resolved {
        let normalized = path_str.replace('\\', "/");
        for ext in &["tsx", "ts", "jsx", "js"] {
            let test_file = format!("{}.{}", normalized, ext);
            if proj_dir.join(&test_file).exists() {
                return Some(test_file);
            }
            let index_file = format!("{}/index.{}", normalized, ext);
            if proj_dir.join(&index_file).exists() {
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
            for sub in &["src/components", "components", "src/hooks", "hooks", "src/lib", "lib", "src/utils", "utils"] {
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

        if attempt == MAX_AI_HEALS {
            break;
        }

        let schema_content = match std::fs::read_to_string(&schema_path) {
            Ok(c) => c,
            Err(_) => break,
        };

        let _ = window.emit(log_channel, serde_json::json!({
            "text": format!("> [Prisma Validator] Započinjem AI auto-healing za schema.prisma (pokušaj {}/{})...\n", attempt + 1, MAX_AI_HEALS),
            "type": "info"
        }));

        let failing_lines = extract_prisma_error_lines(&error_log, &schema_content);

        let system_prompt = "You are an expert database administrator and Prisma specialist. \
            The prisma generate command failed with syntax or validation errors. \
            Review the schema.prisma content and the error log. Correct the schema.prisma file to fix the validation errors. \
            Strictly follow Prisma Schema language rules: \
            1. Optional/nullable relation fields must only have ? on their type (e.g. password String? but NOT password? String?). \
            2. In relations, ensure fields (like userId) are explicitly declared in the model (e.g. userId String) and matches the relation fields parameter. \
            3. Do not use @db.Json native type annotation on String[] fields. Use standard Prisma types. \
            4. The @relation attribute (e.g. `@relation(fields: [userId], references: [id])`) MUST ONLY be placed on relational object fields (like `user User`), never on scalar fields (like `userId String`). Placing @relation on a scalar field will throw an 'Invalid field type, not a relation' error. \
            5. For NextAuth schema, the VerificationToken model does not have a direct relation back to the User model; do not define a verificationTokens relation field on the User model. \
            6. Every relation needs an opposite field on the other model (e.g. `user User @relation(...)` on the child and `accounts Account[]` on User). \
            7. Fields pointing to another model (relations) CANNOT have a `@default()` attribute. For default values, use an `enum` or a scalar type (e.g. `String @default(\"USER\")`). \
            First, in exactly 1 concise sentence, identify the compiler rule that was violated on the failing line. \
            Then, provide the complete corrected schema.prisma code wrapped in a ```prisma code block.";

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
        match client.post("http://127.0.0.1:11434/api/chat").json(&ollama_req).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(resp_json) = resp.json::<Value>().await {
                    if let Some(content) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
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
                // (1) Never write raw LLM output: run it through the deterministic sanitizer first
                let sanitized = post_process_generated_file("prisma/schema.prisma", &cleaned, &serde_json::json!({}), false);
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

    Err(format!("Failed to validate and auto-heal Prisma schema after {} AI attempts.", MAX_AI_HEALS))
}

pub fn looks_like_code_payload(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Cannot be a raw JSON object or array
    if (trimmed.starts_with('{') && trimmed.ends_with('}')) || (trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return false;
    }
    trimmed.contains('\n')
        || trimmed.contains("import ")
        || trimmed.contains("export ")
        || trimmed.contains("const ")
        || trimmed.contains("function ")
        || trimmed.contains("class ")
        || trimmed.contains("interface ")
        || trimmed.contains("type ")
        || trimmed.contains("return ")
        || trimmed.contains("from '")
        || trimmed.contains("from \"")
        || trimmed.contains("<")
        || trimmed.contains("=>")
        || trimmed.contains("model ")
        || trimmed.contains("datasource ")
}

pub fn is_valid_file_path(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.len() > 260 {
        return false;
    }
    // File paths cannot contain newlines, spaces, or code syntax characters
    if trimmed.contains('\n')
        || trimmed.contains(' ')
        || trimmed.contains(';')
        || trimmed.contains('{')
        || trimmed.contains('}')
        || trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.contains('<')
        || trimmed.contains('>')
        || trimmed.contains('=')
        || trimmed.contains('\'')
        || trimmed.contains('"')
    {
        return false;
    }
    let lower = trimmed.to_lowercase().replace('\\', "/");
    lower.ends_with(".tsx")
        || lower.ends_with(".ts")
        || lower.ends_with(".jsx")
        || lower.ends_with(".js")
        || lower.ends_with(".prisma")
        || lower.ends_with(".css")
        || lower.ends_with(".scss")
        || lower.ends_with(".json")
        || lower.ends_with(".html")
        || lower.ends_with(".md")
        || lower.ends_with(".svg")
        || (lower.contains('/') && !lower.contains("import") && !lower.contains("export"))
}

pub fn recursively_find_edits(value: &Value, current_path: &str, edits: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            let mut detected_code = None;
            let mut detected_path = None;
            let mut detected_target = None;
            let mut context_path = current_path.to_string();

            // First pass: check if there is an explicit valid file path in this object (e.g. "file": "src/components/Navbar.tsx")
            for (k, v) in map {
                let k_lower = k.to_lowercase();
                if let Some(s) = v.as_str() {
                    if is_valid_file_path(s) {
                        if k_lower.contains("file") || k_lower.contains("path") || k_lower.contains("filename") {
                            context_path = s.to_string();
                            detected_path = Some(s.to_string());
                        }
                    }
                }
            }

            // Second pass: extract code and target/search strings
            for (k, v) in map {
                let k_lower = k.to_lowercase();
                if let Some(s) = v.as_str() {
                    if is_valid_file_path(s) && detected_path.is_none() {
                        detected_path = Some(s.to_string());
                    } else if k_lower.contains("target") || k_lower.contains("search") || k_lower.contains("old") || k_lower.contains("original") || k_lower.contains("before") || k_lower.contains("find") {
                        if !s.is_empty() {
                            detected_target = Some(s.to_string());
                        }
                    } else if (k_lower == "path" || k_lower == "file") && !is_valid_file_path(s) && !s.is_empty() {
                        // The model put the search/target string in the "path" field!
                        detected_target = Some(s.to_string());
                    } else if k_lower.contains("replacement") || k_lower.contains("replace") || k_lower.contains("code") || k_lower.contains("content") || k_lower.contains("solution") || k_lower.contains("fixed") || k_lower.contains("new") || k_lower == "with" || k_lower == "to" || k_lower == "insert" || k_lower == "val" || k_lower == "value" {
                        if !s.trim().is_empty() {
                            detected_code = Some(s.to_string());
                        }
                    } else if looks_like_code_payload(s) && detected_code.is_none() && detected_target.as_deref() != Some(s) {
                        detected_code = Some(s.to_string());
                    }
                }
            }

            if let Some(code) = detected_code {
                let final_path = detected_path.unwrap_or_else(|| context_path.clone());
                let final_target = detected_target.unwrap_or_default();
                edits.push(serde_json::json!({
                    "path": final_path,
                    "target": final_target,
                    "replacement": code
                }));
                return;
            }

            for (_, v) in map {
                recursively_find_edits(v, &context_path, edits);
            }
        },
        Value::Array(arr) => {
            for item in arr {
                recursively_find_edits(item, current_path, edits);
            }
        },
        Value::String(s) => {
            if looks_like_code_payload(s) && edits.is_empty() {
                edits.push(serde_json::json!({
                    "path": current_path,
                    "target": "",
                    "replacement": s
                }));
            }
        },
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
            // Dynamically search for any file path referenced in the envelope before the marker
            for token in before.split('"') {
                if is_valid_file_path(token) {
                    detected_path = token.replace('\\', "/");
                }
            }

            let after = trimmed[pos + marker.len()..].trim();
            if after.starts_with('`') {
                if let Some(end_bt) = after[1..].rfind('`') {
                    let code = after[1..=end_bt].trim();
                    if !code.is_empty() {
                        let unescaped = code.replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\");
                        return Some(vec![serde_json::json!({
                            "path": detected_path,
                            "target": "",
                            "replacement": unescaped
                        })]);
                    }
                }
            } else if after.starts_with('"') {
                if let Some(end_quote) = after[1..].rfind('"') {
                    let code = after[1..=end_quote].trim();
                    if !code.is_empty() {
                        let unescaped = code.replace("\\\"", "\"").replace("\\n", "\n").replace("\\\\", "\\");
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
    None
}

pub fn extract_edits_from_json(parsed_json: &Value, default_path: &str) -> Vec<Value> {
    let mut edits = Vec::new();
    recursively_find_edits(parsed_json, default_path, &mut edits);
    edits
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
    let full_path = proj_dir.join(&file_path);
    if !full_path.exists() {
        let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška: Fajl ne postoji" }));
        return Err(format!("Fajl {} ne postoji.", file_path));
    }

    let file_content = std::fs::read_to_string(&full_path)
        .map_err(|e| {
            let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška pri čitanju" }));
            format!("Greška pri čitanju fajla: {}", e)
        })?;

    let settings = get_axiom_settings();
    let model = settings.get("modelId")
        .and_then(|m| m.as_str())
        .unwrap_or("qwen2.5-coder:7b")
        .to_string();

    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": format!("Pozivanje AI modela ({}) u toku...", model)
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing] Pozivam model '{}' sa opisom greške...\n", model),
        "type": "info"
    }));

    let client = reqwest::Client::new();
    let mut system_prompt = "You are a Senior Next.js App Router Developer. A file in the project failed with an error. \
        Instead of rewriting the entire file, you must identify the exact lines causing the error and provide a list of target replacements in JSON format.\
        \n\n\
        CRITICAL RULES:\n\
        1. Respond ONLY with a valid JSON object matching the format described below. Do not write any explanations, introductory text, or markdown code blocks (do not wrap in ```json). Just the raw JSON.\n\
        2. In the JSON structure, under 'edits', each 'target' block must exactly match (character-for-character, including leading whitespace and indentation) a unique substring in the original file.\n\
        3. Do NOT include placeholders in 'replacement'. The code must be complete and ready to run.\n\
        4. Focus only on resolving the reported error. Do not make unrelated changes.\n\
        5. DATABASE RESILIENCY: If a database query (like a Prisma call `findMany`) throws a database connection error, wrap the query in try-catch and fall back to returning a mock array containing 10 realistic mockup items so the application remains previewable even if the database server is offline.\n\
        6. NEVER declare, use, or destructure 'data-axiom-component' or 'data-axiom-file' props in React components or TypeScript interfaces/types. These are automatically injected during compilation post-processing, and manual declaration will cause syntax errors.\n\
        7. STRICT UI & DESIGN PRESERVATION: NEVER delete or strip UI components, hero sections, product grids, forms, tables, or buttons to bypass a compilation error. Fix the underlying TypeScript, import, export, or syntax error in place. The page richness and intent MUST remain 100% intact.\n\
        8. MANDATORY TAILWIND CSS: All JSX elements MUST continue to use comprehensive Tailwind CSS utility classes. NEVER degrade styled elements into raw, unstyled HTML.\n\
        \n\n\
        JSON FORMAT:\n\
        {\n\
          \"explanation\": \"Brief explanation of the compile error and how to fix it\",\n\
          \"edits\": [\n\
            {\n\
              \"target\": \"exact string of lines in original file to be replaced\",\n\
              \"replacement\": \"the new code to put in its place\"\n\
            }\n\
          ]\n\
        }".to_string();
    system_prompt.push_str(&get_packages_db_summary());
    let rulepacks = get_stack_rulepacks(&file_path, &file_content, Some(&error_msg));
    if !rulepacks.is_empty() {
        system_prompt.push_str(&rulepacks);
    }

    let rel_file_path = if let Ok(stripped) = std::path::Path::new(&file_path).strip_prefix(&proj_dir) {
        stripped.to_str().unwrap_or("").replace('\\', "/")
    } else {
        file_path.replace('\\', "/")
    };

    let path_lower = rel_file_path.to_lowercase();
    let is_nav_or_link = path_lower.contains("navbar") 
        || path_lower.contains("sidebar") 
        || path_lower.contains("footer") 
        || path_lower.contains("header") 
        || path_lower.contains("nav")
        || file_content.contains("<Link") 
        || file_content.contains("next/link")
        || error_msg.to_lowercase().contains("404");

    if is_nav_or_link {
        let project_routes = get_project_routes_from_dir(&proj_dir);
        let routes_prompt = format_routes_prompt(&project_routes);
        if !routes_prompt.is_empty() {
            system_prompt.push_str(&routes_prompt);
        }
    }

    let mut project_context = String::new();
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

    let metadata_path = proj_dir.join(".axiom").join("metadata.json");

    if metadata_path.exists() {
        if let Ok(meta_content) = std::fs::read_to_string(&metadata_path) {
            if let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(&meta_content) {
                if let Some(deps) = meta_json.get("dependencies").and_then(|d| d.get(&rel_file_path)).and_then(|a| a.as_array()) {
                    for dep_val in deps {
                        if let Some(dep_path_str) = dep_val.as_str() {
                            let dep_full_path = proj_dir.join(dep_path_str);
                            if dep_full_path.exists() {
                                if let Ok(dep_content) = std::fs::read_to_string(&dep_full_path) {
                                    project_context.push_str(&format!(
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

    // Proactively include layout.tsx if healing page.tsx (important for next-auth provider issues)
    if rel_file_path.ends_with("page.tsx") || rel_file_path.ends_with("page.js") {
        for layout_name in &["src/app/layout.tsx", "src/app/layout.jsx", "src/app/layout.js", "layout.tsx"] {
            let layout_full_path = proj_dir.join(layout_name);
            if layout_full_path.exists() {
                if let Ok(layout_content) = std::fs::read_to_string(&layout_full_path) {
                    if !project_context.contains(layout_name) {
                        project_context.push_str(&format!(
                            "\n--- Project Layout File: {} ---\n{}\n-------------------------------------\n",
                            layout_name, layout_content
                        ));
                    }
                }
            }
        }
    }

    // Abstractly extract and inject all related exporter/dependency files based on error message, tokens, and imports
    let related_files = extract_related_project_files_for_error(&proj_dir, &file_content, &rel_file_path, &error_msg);
    for (rel_dep_path, dep_content) in related_files {
        if !project_context.contains(&rel_dep_path) {
            project_context.push_str(&format!(
                "\n--- Related Dependency / Exporter File: {} ---\n{}\n----------------------------------------------------\n",
                rel_dep_path, dep_content
            ));
            system_prompt.push_str(&format!(
                "\n\nCRITICAL MULTI-FILE EDIT RULE:\n\
                 The error in '{}' may be caused by missing or incorrect exports/definitions in '{}'.\n\
                 You are allowed to propose edits for '{}' (set 'path' to '{}' in edits) to fix/add exports or declarations, OR modify '{}'.",
                rel_file_path, rel_dep_path, rel_dep_path, rel_dep_path, rel_file_path
            ));
        }
    }

    let user_prompt = format!(
        "File: {}\n\nCompilation Error Details:\n{}\n\nFile Current Content:\n{}\n{}{}\n\nPlease analyze the error and output the required JSON edits to fix it.",
        file_path,
        error_msg,
        file_content,
        if project_context.is_empty() { "" } else { "\nAdditional Project Context:" },
        project_context
    );

    let ollama_req = serde_json::json!({
        "model": model,
        "format": "json",
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "stream": false,
        "options": build_ollama_options(&model, 0.1, Some(4096))
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
                        let _ = window.emit("server:heal-status", serde_json::json!({
                            "status": "Ispravljanje koda i post-processing..."
                        }));
                        let _ = window.emit("server:log", serde_json::json!({
                            "text": "> [AI Auto-Healing] AI je vratio ispravku. Primjenjujem post-processing...\n".to_string(),
                            "type": "info"
                        }));

                        debug_log_to_file(format!("[AI Auto-Healing] Raw AI response:\n{}", ai_msg));
                        let clean_content = extract_code_content(ai_msg);
                        
                        let mut resolved_edits = Vec::new();
                        let mut explanation = "Popravka generisana od strane AI modela".to_string();

                        // Try parsing as JSON first
                        let maybe_json: Option<serde_json::Value> = serde_json::from_str(&clean_content).ok()
                            .or_else(|| {
                                if let Some(start_json) = clean_content.find('{') {
                                    if let Some(end_json) = clean_content.rfind('}') {
                                        let json_str = &clean_content[start_json..=end_json];
                                        serde_json::from_str(json_str).ok()
                                    } else {
                                        None
                                    }
                                } else if let Some(start_arr) = clean_content.find('[') {
                                    if let Some(end_arr) = clean_content.rfind(']') {
                                        let json_str = &clean_content[start_arr..=end_arr];
                                        serde_json::from_str(json_str).ok()
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            });

                        if let Some(ref parsed_json) = maybe_json {
                            if let Some(exp) = parsed_json.get("explanation").or_else(|| parsed_json.get("reason")).and_then(|e| e.as_str()) {
                                explanation = exp.to_string();
                            }

                            resolved_edits = extract_edits_from_json(parsed_json, &rel_file_path);
                        }

                        // Try extracting code payload if the model returned malformed/partially-formatted JSON with backticks
                        if resolved_edits.is_empty() {
                            if let Some(malformed_edits) = extract_code_from_malformed_json_response(&clean_content, &rel_file_path) {
                                debug_log_to_file(format!("[AI Auto-Healing] Extracted embedded code from malformed JSON for: {}", rel_file_path));
                                let _ = window.emit("server:log", serde_json::json!({
                                    "text": format!("> [AI Auto-Healing] Prepoznat kod unutar odgovora modela. Primenjujem na: {}\n", rel_file_path),
                                    "type": "info"
                                }));
                                resolved_edits = malformed_edits;
                                explanation = "Izvučen popravljen kod iz AI odgovora".to_string();
                            }
                        }

                        // Whole-File Code Fallback: If no valid JSON edits were resolved, but clean_content is strictly raw code (NOT a JSON dictionary)
                        if resolved_edits.is_empty() {
                            let trimmed = clean_content.trim();
                            let is_raw_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
                                || (trimmed.starts_with('[') && trimmed.ends_with(']'));
                            let is_json_envelope = trimmed.contains("\"files\":") 
                                || trimmed.contains("\"content\":") 
                                || trimmed.contains("\"edits\":") 
                                || trimmed.contains("\"target\":") 
                                || trimmed.contains("\"replacement\":");

                            if !is_raw_json && !is_json_envelope {
                                let looks_like_code = clean_content.contains("import ") 
                                    || clean_content.contains("export ") 
                                    || clean_content.contains("const ") 
                                    || clean_content.contains("function ") 
                                    || clean_content.contains("class ") 
                                    || clean_content.contains("interface ") 
                                    || clean_content.contains("type ") 
                                    || clean_content.contains("return ") 
                                    || clean_content.contains("<");

                                if looks_like_code && !trimmed.is_empty() {
                                    debug_log_to_file(format!("[AI Auto-Healing] Whole-file code fallback activated for: {}", rel_file_path));
                                    let _ = window.emit("server:log", serde_json::json!({
                                        "text": format!("> [AI Auto-Healing] Prepoznat direktan kod celog fajla (Whole-File Fallback). Primenjujem na: {}\n", rel_file_path),
                                        "type": "info"
                                    }));
                                    resolved_edits.push(serde_json::json!({
                                        "path": rel_file_path.clone(),
                                        "target": "",
                                        "replacement": clean_content
                                    }));
                                    explanation = "Primenjen kompletan popravljen kod iz AI odgovora".to_string();
                                }
                            }
                        }

                        if resolved_edits.is_empty() {
                            let err_msg = "Model response could not be parsed as targeted JSON edits or valid whole-file code replacement.".to_string();
                            debug_log_to_file(format!("[AI Auto-Healing Error] {}", err_msg));
                            return Err(err_msg);
                        }

                        let _ = window.emit("server:log", serde_json::json!({
                            "text": format!("> [AI Auto-Healing] AI analiza: {}\n", explanation),
                            "type": "info"
                        }));

                        let edits = resolved_edits;

                        use std::collections::HashMap;
                        let mut modified_files: HashMap<String, String> = HashMap::new();
                        
                        for edit in edits {
                            let edit_path = match edit.get("path").and_then(|p| p.as_str()) {
                                Some(p) => p.to_string().replace('\\', "/"),
                                None => rel_file_path.clone(),
                            };
                            
                            let target = match edit.get("target").and_then(|t| t.as_str()) {
                                Some(t) => t,
                                None => {
                                    let err_msg = "An edit is missing its 'target' field.".to_string();
                                    debug_log_to_file(format!("[AI Auto-Healing Error] {}", err_msg));
                                    return Err(err_msg);
                                }
                            };
                            let replacement = match edit.get("replacement").and_then(|r| r.as_str()) {
                                Some(r) => r,
                                None => {
                                    let err_msg = "An edit is missing its 'replacement' field.".to_string();
                                    debug_log_to_file(format!("[AI Auto-Healing Error] {}", err_msg));
                                    return Err(err_msg);
                                }
                            };

                            // Load the file content if it is not already loaded
                            if !modified_files.contains_key(&edit_path) {
                                let target_full_path = proj_dir.join(&edit_path);
                                if !target_full_path.exists() {
                                    modified_files.insert(edit_path.clone(), String::new());
                                } else {
                                    let content = std::fs::read_to_string(&target_full_path)
                                        .map_err(|e| {
                                            let err_msg = format!("Greška pri čitanju fajla {}: {}", edit_path, e);
                                            debug_log_to_file(format!("[AI Auto-Healing Error] {}", err_msg));
                                            err_msg
                                        })?;
                                    modified_files.insert(edit_path.clone(), content);
                                }
                            }

                            let file_content_mut = modified_files.get_mut(&edit_path).unwrap();

                            if target.is_empty() {
                                let repl_trimmed = replacement.trim();
                                let is_just_import = (repl_trimmed.starts_with("import ") || repl_trimmed.starts_with("import{") || repl_trimmed.starts_with("import type"))
                                    && !repl_trimmed.contains("export ")
                                    && !repl_trimmed.contains("function ")
                                    && !repl_trimmed.contains("const ")
                                    && !repl_trimmed.contains("default ")
                                    && !repl_trimmed.contains("class ");

                                let is_partial_helper_snippet = !file_content_mut.is_empty()
                                    && file_content_mut.contains("export default")
                                    && !repl_trimmed.contains("export default")
                                    && repl_trimmed.lines().count() <= 10;

                                if is_just_import && !file_content_mut.is_empty() {
                                    if !file_content_mut.contains(repl_trimmed) {
                                        *file_content_mut = format!("{}\n{}", repl_trimmed, file_content_mut);
                                    }
                                } else if is_partial_helper_snippet {
                                    // Do not wipe out a full React page with a 1-10 line helper declaration
                                    if !file_content_mut.contains(repl_trimmed) {
                                        if let Some(idx) = file_content_mut.find("export default") {
                                            file_content_mut.insert_str(idx, &format!("{}\n\n", repl_trimmed));
                                        }
                                    }
                                } else {
                                    *file_content_mut = replacement.to_string();
                                }
                            } else {
                                match apply_targeted_edit(file_content_mut, target, replacement) {
                                    Ok(new_content) => {
                                        *file_content_mut = new_content;
                                    },
                                    Err(err) => {
                                        let err_msg = format!("Greška pri primeni ispravke u fajlu {}: {}", edit_path, err);
                                        debug_log_to_file(format!("[AI Auto-Healing Error] {}", err_msg));
                                        return Err(err_msg);
                                    }
                                }
                            }
                        }

                        let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();
                        
                        let _ = window.emit("server:heal-status", serde_json::json!({
                            "status": "Upisivanje ispravki na disk..."
                        }));

                        for (edit_path, processed_content) in modified_files {
                            let processed = post_process_generated_file(&edit_path, &processed_content, &serde_json::json!({}), has_tailwind);
                            let target_full_path = proj_dir.join(&edit_path);
                            std::fs::write(&target_full_path, &processed)
                                .map_err(|e| {
                                    let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška pri upisu" }));
                                    let err_msg = format!("Greška pri upisu fajla {} na disk: {}", edit_path, e);
                                    debug_log_to_file(format!("[AI Auto-Healing Error] {}", err_msg));
                                    err_msg
                                })?;
                            
                            let _ = window.emit("server:log", serde_json::json!({
                                "text": format!("> [AI Auto-Healing] Uspešno popravljen fajl: {}\n", edit_path),
                                "type": "info"
                            }));
                            debug_log_to_file(format!("[AI Auto-Healing] Successfully healed file: {}", edit_path));
                        }

                        let _ = window.emit("server:heal-status", serde_json::json!({
                            "status": "Završeno!"
                        }));

                        return Ok(true);
                    }
                }
            }
            let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška: Prazan odgovor modela" }));
            Err("Model nije vratio ispravan odgovor.".to_string())
        }
        Err(e) => {
            let _ = window.emit("server:heal-status", serde_json::json!({ "status": "Greška u komunikaciji sa AI" }));
            Err(format!("Greška u komunikaciji sa Ollama serverom: {}", e))
        }
    }
}

#[tauri::command]
pub async fn project_install_package(
    project_id: String,
    package_name: String,
    window: tauri::Window,
) -> Result<bool, String> {
    let _ = window.emit("server:heal-status", serde_json::json!({
        "status": format!("Instalacija paketa {}...", package_name)
    }));
    let _ = window.emit("server:log", serde_json::json!({
        "text": format!("> [AI Auto-Healing] Detektovan nedostajući paket. Pokrećem instalaciju: {}\n", package_name),
        "type": "info"
    }));

    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }

    let enriched_path = ensure_node_toolchain_ready(Some(&window), "server:log").await;

    let _ = window.emit("server:log", serde_json::json!({
        "text": "> [AI Auto-Healing] Pokrećem npm install za paket...\n".to_string(),
        "type": "info"
    }));

    let mut pkgs_to_install = vec![package_name.clone()];
    if package_name == "@emotion/styled" || package_name == "@mui/material" {
        pkgs_to_install.push("@emotion/react".to_string());
    } else if package_name == "@emotion/react" {
        pkgs_to_install.push("@emotion/styled".to_string());
    }

    #[cfg(target_os = "windows")]
    let mut cmd = tokio::process::Command::new("cmd");
    #[cfg(target_os = "windows")]
    {
        cmd.arg("/C").arg("npm").arg("install").arg("--legacy-peer-deps").arg("--prefer-offline");
        for p in &pkgs_to_install {
            cmd.arg(p);
        }
        cmd.current_dir(&proj_dir).env("PATH", &enriched_path);
    }

    #[cfg(not(target_os = "windows"))]
    let mut cmd = tokio::process::Command::new("npm");
    #[cfg(not(target_os = "windows"))]
    {
        cmd.arg("install").arg("--legacy-peer-deps").arg("--prefer-offline");
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
                let _ = window.emit("server:heal-status", serde_json::json!({
                    "status": "Instalirano!"
                }));
                let _ = window.emit("server:log", serde_json::json!({
                    "text": format!("> [AI Auto-Healing] Uspešno instaliran paket {}.\n", package_name),
                    "type": "info"
                }));
                Ok(true)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let _ = window.emit("server:heal-status", serde_json::json!({
                    "status": "Greška pri instalaciji"
                }));
                Err(format!("npm install failed: {}", stderr))
            }
        }
        Err(e) => Err(format!("Failed to run npm install command: {}", e))
    }
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
                        if let Some(parent_dir) = std::path::Path::new(file_path).parent() {
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
                            let normalized_path_str = normalized_parts.join("/");
                            
                            for pf in &project_files {
                                let pf_no_ext = std::path::Path::new(pf).with_extension("");
                                let pf_no_ext_str = pf_no_ext.to_str().unwrap_or("").replace('\\', "/");
                                if pf_no_ext_str == normalized_path_str {
                                    file_deps.push(pf.clone());
                                    break;
                                }
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

