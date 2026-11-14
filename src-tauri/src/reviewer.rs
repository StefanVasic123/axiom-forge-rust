use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewIssue {
    pub rule_id: String,
    pub severity: ReviewSeverity,
    pub message: String,
    pub repair_directive: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MiddlewareMatcherSpec {
    pub matcher_strings: Vec<String>,
    pub excluded_literals: Vec<String>,
    pub excludes_dotted_files: bool,
    pub has_negative_lookahead: bool,
}

/// Token-aware delimiter balance check (`{}`, `()`, `[]` for code; `{}` for CSS).
/// Skips comments, quoted strings, template literals (`${...}` aware), and JS/TS regex literals.
pub fn check_delimiter_balance(code: &str, is_css: bool) -> Result<(), ReviewIssue> {
    if is_css {
        let mut depth: isize = 0;
        let bytes = code.as_bytes();
        let mut i = 0usize;
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;
        while i < code.len() {
            if !in_single && !in_double && i + 1 < code.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                if let Some(rel_end) = code[i + 2..].find("*/") {
                    i += 2 + rel_end + 2;
                    continue;
                }
                return Err(ReviewIssue {
                    rule_id: "syntax:css-unclosed-comment".to_string(),
                    severity: ReviewSeverity::Error,
                    message: "CSS file contains an unclosed `/* ... */` comment.".to_string(),
                    repair_directive: "Close the CSS block comment with `*/`.".to_string(),
                });
            }
            let ch = code[i..].chars().next().unwrap();
            if escape {
                escape = false;
            } else if ch == '\\' && (in_single || in_double) {
                escape = true;
            } else if ch == '\'' && !in_double {
                in_single = !in_single;
            } else if ch == '"' && !in_single {
                in_double = !in_double;
            } else if !in_single && !in_double {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth < 0 {
                        return Err(ReviewIssue {
                            rule_id: "syntax:css-unexpected-closing-brace".to_string(),
                            severity: ReviewSeverity::Error,
                            message: "CSS file contains an unexpected unmatched closing brace `}`.".to_string(),
                            repair_directive: "Remove stray `}` outside rule blocks and close every rule cleanly.".to_string(),
                        });
                    }
                }
            }
            i += ch.len_utf8();
        }
        if depth > 0 {
            return Err(ReviewIssue {
                rule_id: "syntax:css-unclosed-block".to_string(),
                severity: ReviewSeverity::Error,
                message: format!("CSS file has {} unclosed '{{' block(s).", depth),
                repair_directive: "Ensure every CSS selector or @-rule `{` has a matching `}`.".to_string(),
            });
        }
        return Ok(());
    }

    // TS / JS / TSX / JSX: track curly `{}` strictly (and `${...}` inside template literals)
    let bytes = code.as_bytes();
    let mut i = 0usize;
    let mut curly_depth: isize = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut tmpl_expr_braces: Vec<isize> = Vec::new(); // nonzero = inside template text wait, tracks outer curly depth at `${`
    let mut in_tmpl_text = false;
    let mut escape = false;
    let mut prev_sig_char: Option<char> = None;
    let mut prev_word = String::new();

    while i < code.len() {
        let in_string = in_single || in_double || in_tmpl_text;
        if !in_string && i + 1 < code.len() {
            if bytes[i] == b'/' && bytes[i + 1] == b'/' {
                let end = code[i..].find('\n').map_or(code.len(), |n| i + n + 1);
                i = end;
                continue;
            }
            if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                if let Some(rel) = code[i + 2..].find("*/") {
                    i += 2 + rel + 2;
                    continue;
                }
                return Err(ReviewIssue {
                    rule_id: "syntax:unclosed-comment".to_string(),
                    severity: ReviewSeverity::Error,
                    message: "Source file contains an unclosed `/* ... */` block comment.".to_string(),
                    repair_directive: "Close the block comment with `*/`.".to_string(),
                });
            }
        }

        let ch = code[i..].chars().next().unwrap();
        if escape {
            escape = false;
            i += ch.len_utf8();
            continue;
        }
        if in_tmpl_text {
            if ch == '\\' {
                escape = true;
            } else if ch == '`' {
                in_tmpl_text = false;
            } else if ch == '$' && i + 1 < code.len() && bytes[i + 1] == b'{' {
                tmpl_expr_braces.push(curly_depth);
                curly_depth += 1;
                in_tmpl_text = false;
                prev_sig_char = Some('{');
                prev_word.clear();
                i += 2;
                continue;
            }
            i += ch.len_utf8();
            continue;
        }
        if in_single {
            if ch == '\\' {
                escape = true;
            } else if ch == '\'' {
                in_single = false;
                prev_sig_char = Some('\'');
            }
            i += ch.len_utf8();
            continue;
        }
        if in_double {
            if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_double = false;
                prev_sig_char = Some('"');
            }
            i += ch.len_utf8();
            continue;
        }

        // Possible JS/TS regex literal after expression-prefix punctuators or `return`/`case`/`typeof`
        if ch == '/' && i + 1 < code.len() && bytes[i + 1] != b'/' && bytes[i + 1] != b'*' {
            let regex_allowed = matches!(
                prev_sig_char,
                None | Some('=') | Some('(') | Some('[') | Some('{') | Some(',') | Some(':') | Some(';') | Some('!') | Some('?') | Some('&') | Some('|') | Some('>')
            ) || matches!(prev_word.as_str(), "return" | "throw" | "case" | "typeof" | "void" | "delete" | "await" | "yield");
            if regex_allowed {
                let mut r_idx = i + 1;
                let mut in_class = false;
                let mut r_esc = false;
                let mut closed = false;
                while r_idx < code.len() {
                    let rc = code[r_idx..].chars().next().unwrap();
                    if r_esc {
                        r_esc = false;
                    } else if rc == '\\' {
                        r_esc = true;
                    } else if rc == '[' {
                        in_class = true;
                    } else if rc == ']' && in_class {
                        in_class = false;
                    } else if rc == '/' && !in_class {
                        r_idx += 1;
                        while r_idx < code.len() && code[r_idx..].chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
                            r_idx += 1;
                        }
                        closed = true;
                        break;
                    } else if rc == '\n' {
                        break;
                    }
                    r_idx += rc.len_utf8();
                }
                if closed {
                    prev_sig_char = Some('/');
                    prev_word.clear();
                    i = r_idx;
                    continue;
                }
            }
        }

        match ch {
            '\'' => {
                in_single = true;
                prev_word.clear();
            }
            '"' => {
                in_double = true;
                prev_word.clear();
            }
            '`' => {
                in_tmpl_text = true;
                prev_word.clear();
            }
            '{' => {
                curly_depth += 1;
                prev_sig_char = Some('{');
                prev_word.clear();
            }
            '}' => {
                curly_depth -= 1;
                if curly_depth < 0 {
                    return Err(ReviewIssue {
                        rule_id: "syntax:unexpected-closing-brace".to_string(),
                        severity: ReviewSeverity::Error,
                        message: "Code contains an unexpected unmatched closing brace `}`.".to_string(),
                        repair_directive: "Remove or balance the unmatched `}`.".to_string(),
                    });
                }
                if tmpl_expr_braces.last().copied() == Some(curly_depth) {
                    tmpl_expr_braces.pop();
                    in_tmpl_text = true;
                }
                prev_sig_char = Some('}');
                prev_word.clear();
            }
            c if c.is_ascii_alphanumeric() || c == '_' || c == '$' => {
                prev_word.push(c);
                prev_sig_char = Some(c);
            }
            c if !c.is_whitespace() => {
                prev_sig_char = Some(c);
                prev_word.clear();
            }
            _ => {}
        }
        i += ch.len_utf8();
    }

    if curly_depth > 2 {
        return Err(ReviewIssue {
            rule_id: "syntax:unclosed-brace-truncation".to_string(),
            severity: ReviewSeverity::Error,
            message: format!("Generated code appears truncated: {} unclosed '{{' block(s).", curly_depth),
            repair_directive: "Complete the truncated block and close all `{` braces cleanly.".to_string(),
        });
    }
    Ok(())
}

/// Parses `export const config = { matcher: ... }` in Next.js middleware without hardcoding paths.
pub fn extract_middleware_matcher_exclusions(content: &str) -> MiddlewareMatcherSpec {
    let mut spec = MiddlewareMatcherSpec::default();
    let Ok(cfg_re) = regex::Regex::new(r#"(?s)export\s+const\s+config\s*=\s*\{(.*?)\}"#) else {
        return spec;
    };
    let Some(cap) = cfg_re.captures(content) else {
        return spec;
    };
    let cfg_body = cap.get(1).map_or("", |m| m.as_str());
    let Ok(str_re) = regex::Regex::new(r#"'([^']*)'|"([^"]*)"|`([^`]*)`"#) else {
        return spec;
    };

    for m_cap in str_re.captures_iter(cfg_body) {
        let val = m_cap
            .get(1)
            .or_else(|| m_cap.get(2))
            .or_else(|| m_cap.get(3))
            .map_or("", |m| m.as_str());
        if !val.starts_with('/') {
            continue;
        }
        spec.matcher_strings.push(val.to_string());
        if let Some(neg_start) = val.find("(?!") {
            spec.has_negative_lookahead = true;
            let rest = &val[neg_start + 3..];
            // Read balanced group inside `(?!...)`
            let mut paren_depth = 1usize;
            let mut end_idx = rest.len();
            let mut esc = false;
            for (idx, c) in rest.char_indices() {
                if esc {
                    esc = false;
                    continue;
                }
                if c == '\\' {
                    esc = true;
                    continue;
                }
                if c == '(' {
                    paren_depth += 1;
                } else if c == ')' {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        end_idx = idx;
                        break;
                    }
                }
            }
            let inner = &rest[..end_idx];
            for alt in inner.split('|') {
                let cleaned = alt
                    .trim()
                    .replace("\\\\", "\\")
                    .replace("\\.", ".")
                    .replace("\\/", "/");
                if cleaned.is_empty() {
                    continue;
                }
                if cleaned.contains(".*..*") || cleaned == ".*\\..*" || cleaned.contains("[^/]+\\.[^/]+") || cleaned == ".*\\..*" {
                    spec.excludes_dotted_files = true;
                } else {
                    spec.excluded_literals.push(cleaned);
                }
            }
        }
    }
    // Check dot-file pattern in raw matcher string as well (because of regex `\\.` escaping)
    for m in &spec.matcher_strings {
        if m.contains(".*\\\\..*") || m.contains(".*\\..*") {
            spec.excludes_dotted_files = true;
        }
    }
    spec
}

fn is_condition_covered_by_matcher(cond: &str, spec: &MiddlewareMatcherSpec) -> bool {
    if !spec.has_negative_lookahead || cond.trim().is_empty() {
        return false;
    }
    // Split OR-clauses `||` (top-level only, no nested `&&` unless all atoms covered)
    if cond.contains("&&") {
        return false;
    }
    let parts: Vec<&str> = cond.split("||").map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return false;
    }

    parts.iter().all(|clause| {
        if let Some(arg) = extract_quoted_arg_after_method(clause, "startsWith") {
            let trimmed = arg.trim_start_matches('/');
            return spec.excluded_literals.iter().any(|ex| {
                let ex_t = ex.trim_start_matches('/').trim_end_matches('$').trim_end_matches('/');
                trimmed == ex_t || trimmed.starts_with(&format!("{}/", ex_t))
            });
        }
        if let Some(arg) = extract_quoted_arg_after_method(clause, "endsWith") {
            let trimmed = arg.trim_start_matches('/');
            if spec.excludes_dotted_files && trimmed.contains('.') {
                return true;
            }
            return spec.excluded_literals.iter().any(|ex| ex.trim_start_matches('/') == trimmed);
        }
        if let Some(arg) = extract_quoted_arg_after_method(clause, "includes") {
            let trimmed = arg.trim_start_matches('/');
            if spec.excludes_dotted_files && trimmed == "." {
                return true;
            }
            return spec.excluded_literals.iter().any(|ex| ex.trim_start_matches('/') == trimmed);
        }
        false
    })
}

fn extract_quoted_arg_after_method(clause: &str, method: &str) -> Option<String> {
    let needle = format!(".{}(", method);
    let pos = clause.find(&needle)?;
    let after = clause[pos + needle.len()..].trim_start();
    let quote = after.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let rest = &after[1..];
    let end_q = rest.find(quote)?;
    Some(rest[..end_q].to_string())
}

fn find_matching_block_end(src: &str, open_brace: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    if bytes.get(open_brace).copied() != Some(b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut i = open_brace;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_tick = false;
    let mut esc = false;
    while i < src.len() {
        let ch = src[i..].chars().next()?;
        if esc {
            esc = false;
        } else if ch == '\\' && (in_single || in_double || in_tick) {
            esc = true;
        } else if ch == '\'' && !in_double && !in_tick {
            in_single = !in_single;
        } else if ch == '"' && !in_single && !in_tick {
            in_double = !in_double;
        } else if ch == '`' && !in_single && !in_double {
            in_tick = !in_tick;
        } else if !in_single && !in_double && !in_tick {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i + 1);
                }
            }
        }
        i += ch.len_utf8();
    }
    None
}

fn find_matching_paren_end(src: &str, open_paren: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    if bytes.get(open_paren).copied() != Some(b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut i = open_paren;
    let mut in_single = false;
    let mut in_double = false;
    let mut esc = false;
    while i < src.len() {
        let ch = src[i..].chars().next()?;
        if esc {
            esc = false;
        } else if ch == '\\' && (in_single || in_double) {
            esc = true;
        } else if ch == '\'' && !in_double {
            in_single = !in_single;
        } else if ch == '"' && !in_single {
            in_double = !in_double;
        } else if !in_single && !in_double {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i + 1);
                }
            }
        }
        i += ch.len_utf8();
    }
    None
}

/// Principled middleware sanitizer driven by `export const config.matcher` exclusions:
/// removes unreachable `if (...) { ... }` guards inside `middleware()` whose path/extension
/// checks are already excluded at the Next.js router `config.matcher` layer (so long as no
/// other matcher entry re-includes that prefix and the `if` has no `else` branch).
pub fn sanitize_middleware_principled(content: &str, rel_path: &str) -> String {
    let low = rel_path.replace('\\', "/").to_ascii_lowercase();
    if !(low.ends_with("middleware.ts") || low.ends_with("middleware.js")) {
        return content.to_string();
    }
    let spec = extract_middleware_matcher_exclusions(content);
    if !spec.has_negative_lookahead || (spec.excluded_literals.is_empty() && !spec.excludes_dotted_files) {
        return content.to_string();
    }
    // Only auto-strip unreachable prefix branches when no other matcher explicitly opts `/<excluded>` back in
    let has_opt_in_matcher = spec.matcher_strings.iter().any(|m| {
        !m.contains("(?!") && spec.excluded_literals.iter().any(|ex| m.starts_with(&format!("/{}", ex.trim_start_matches('/'))))
    });
    if has_opt_in_matcher {
        return content.to_string();
    }

    let mut out = String::with_capacity(content.len());
    let mut i = 0usize;
    let bytes = content.as_bytes();
    while i < content.len() {
        // Look at line start `[optional // comment line(s)] + indent + if (`
        let rest = &content[i..];
        let at_line_start = i == 0 || bytes[i - 1] == b'\n';
        if at_line_start {
            // Optional single-line comment right above the unreachable `if`
            let mut scan = i;
            let trimmed_cur = rest.lines().next().unwrap_or("").trim();
            if trimmed_cur.starts_with("//") {
                if let Some(nl) = rest.find('\n') {
                    scan = i + nl + 1;
                }
            }
            let after_comment = &content[scan..];
            let indent_len = after_comment.chars().take_while(|c| *c == ' ' || *c == '\t').map(|c| c.len_utf8()).sum::<usize>();
            let after_indent = &after_comment[indent_len..];
            if after_indent.starts_with("if") && after_indent[2..].trim_start().starts_with('(') {
                let open_p = scan + indent_len + after_indent.find('(').unwrap();
                if let Some(close_p) = find_matching_paren_end(content, open_p) {
                    let cond = &content[open_p + 1..close_p - 1];
                    let after_p = content[close_p..].trim_start();
                    if after_p.starts_with('{') && is_condition_covered_by_matcher(cond, &spec) {
                        let open_b = content.len() - after_p.len();
                        if let Some(close_b) = find_matching_block_end(content, open_b) {
                            let after_block = content[close_b..].trim_start();
                            // Ensure there is no `else` chaining onto this `if` block
                            if !after_block.starts_with("else") {
                                let mut next_pos = close_b;
                                while next_pos < content.len() && matches!(bytes[next_pos], b' ' | b'\t' | b'\r') {
                                    next_pos += 1;
                                }
                                if next_pos < content.len() && bytes[next_pos] == b'\n' {
                                    next_pos += 1;
                                }
                                // Skip extra blank line if both sides had blank lines
                                if out.ends_with("\n\n") && content[next_pos..].starts_with('\n') {
                                    next_pos += 1;
                                } else if out.ends_with("\n\n") && content[next_pos..].starts_with("\r\n") {
                                    next_pos += 2;
                                }
                                i = next_pos;
                                continue;
                            }
                        }
                    }
                }
            }
        }
        let ch = content[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Principled middleware semantic & Edge optimization audit.
pub fn review_middleware_semantics(content: &str) -> Vec<ReviewIssue> {
    let mut issues = Vec::new();
    let edge_unsafe_imports = [
        ("@prisma/client", "PrismaClient"),
        ("@/lib/prisma", "prisma"),
        ("bcryptjs", "bcrypt"),
        ("bcrypt", "bcrypt"),
        ("child_process", "child_process"),
        ("node:fs", "fs"),
        ("'fs'", "fs"),
        ("\"fs\"", "fs"),
    ];
    for (pat, label) in edge_unsafe_imports {
        if content.contains(pat) {
            issues.push(ReviewIssue {
                rule_id: "middleware:edge-unsafe-import".to_string(),
                severity: ReviewSeverity::Error,
                message: format!(
                    "Next.js Edge Middleware must NEVER import `{}` ({}) because Node/DB modules fail in Edge Runtime.",
                    pat, label
                ),
                repair_directive: "Use lightweight JWT token verification (`getToken` from `'next-auth/jwt'`) and `NextRequest`/`NextResponse` in middleware.".to_string(),
            });
            break;
        }
    }

    // Check redirect loop hazard: redirecting to `/login` or `/maintenance` without guarding that pathname
    for target in ["/maintenance", "/waitlist"] {
        let redir_single = format!("'{}'", target);
        let redir_double = format!("\"{}\"", target);
        if (content.contains(&format!("new URL({}", redir_single)) || content.contains(&format!("new URL({}", redir_double)))
            && !content.contains(&format!("startsWith('{}')", target))
            && !content.contains(&format!("startsWith(\"{}\")", target))
            && !content.contains(&format!("=== '{}'", target))
            && !content.contains(&format!("=== \"{}\"", target))
        {
            issues.push(ReviewIssue {
                rule_id: "middleware:redirect-loop".to_string(),
                severity: ReviewSeverity::Warning,
                message: format!(
                    "Middleware redirects to `{}` without excluding `pathname.startsWith('{}')`, risking an infinite redirect loop.",
                    target, target
                ),
                repair_directive: format!(
                    "Guard redirect with `if (!pathname.startsWith('{}')) {{ ... }}` and place maintenance gate before auth redirects.",
                    target
                ),
            });
        }
    }

    let spec = extract_middleware_matcher_exclusions(content);
    if spec.has_negative_lookahead {
        for lit in &spec.excluded_literals {
            let trimmed = lit.trim_start_matches('/').trim_end_matches('$');
            if !trimmed.is_empty()
                && !trimmed.contains('*')
                && (content.contains(&format!("pathname.startsWith('/{}')", trimmed))
                    || content.contains(&format!("pathname.startsWith(\"/{}\")", trimmed)))
            {
                issues.push(ReviewIssue {
                    rule_id: "middleware:unreachable-matcher-branch".to_string(),
                    severity: ReviewSeverity::Warning,
                    message: format!(
                        "Middleware checks `pathname.startsWith('/{}')`, which is unreachable because `config.matcher` already excludes `{}`.",
                        trimmed, trimmed
                    ),
                    repair_directive: format!(
                        "Remove the unreachable `if (pathname.startsWith('/{}'))` branch or explicitly opt it into `config.matcher` if middleware must handle it.",
                        trimmed
                    ),
                });
            }
        }
    }

    issues
}

/// Checks Next.js App Router API Route Handlers (`src/app/api/**/route.{ts,js}`) for method-form mismatches.
pub fn check_app_router_api_route_form(content: &str, file_path: &str) -> Vec<ReviewIssue> {
    let mut issues = Vec::new();
    let low = file_path.replace('\\', "/").to_ascii_lowercase();
    let is_api_route = (low.starts_with("src/app/api/") || low.starts_with("app/api/") || low.contains("/app/api/"))
        && (low.ends_with("/route.ts") || low.ends_with("/route.js"));
    if !is_api_route {
        return issues;
    }

    if content.contains("NextApiRequest") || content.contains("NextApiResponse") {
        issues.push(ReviewIssue {
            rule_id: "route:pages-api-in-app-router".to_string(),
            severity: ReviewSeverity::Error,
            message: format!(
                "App Router route handler `{}` uses Pages Router types (`NextApiRequest`/`NextApiResponse`).",
                file_path
            ),
            repair_directive: "Use Web/Next App Router signature `export async function POST(request: NextRequest)` (reading body via `await request.json()`, NOT `request.body`) and return `NextResponse.json(...)`.".to_string(),
        });
    }

    // Also catch non-awaited Pages-style `request.body` parsed directly into Zod/JSON
    if let Ok(req_body_re) = regex::Regex::new(r"\b(?:req|request)\.body\b") {
        if req_body_re.is_match(content) && !content.contains("request.json()") && !content.contains("req.json()") {
            issues.push(ReviewIssue {
                rule_id: "route:pages-req-body-in-app-router".to_string(),
                severity: ReviewSeverity::Error,
                message: format!(
                    "App Router route `{}` accesses synchronous `request.body` (Pages Router API) instead of `await request.json()`.",
                    file_path
                ),
                repair_directive: "Replace `request.body` with `const body = await request.json();` inside the async HTTP handler.".to_string(),
            });
        }
    }

    issues
}

/// Detects variables parsed from `(req|request).json()` inside a `try { ... }` block
/// and referenced inside its paired `catch (...) { ... }` block (triggering TS2304).
pub fn check_try_catch_body_scope(content: &str, file_path: &str) -> Vec<ReviewIssue> {
    let mut issues = Vec::new();
    let Ok(try_re) = regex::Regex::new(r"\btry\s*\{") else {
        return issues;
    };
    let Ok(decl_re) = regex::Regex::new(
        r"\b(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=\n]+)?=\s*(?:await\s+)?(?:req|request)\s*\.\s*json\s*\(",
    ) else {
        return issues;
    };
    let Ok(catch_re) = regex::Regex::new(r"^\s*catch\s*(?:\([^)]*\))?\s*\{") else {
        return issues;
    };

    for m in try_re.find_iter(content) {
        let open_brace = m.end() - 1;
        let Some(try_end) = find_matching_block_end(content, open_brace) else {
            continue;
        };
        let try_body = &content[open_brace + 1..try_end - 1];
        let after_try = &content[try_end..];
        let Some(catch_m) = catch_re.find(after_try) else {
            continue;
        };
        let catch_open = try_end + catch_m.end() - 1;
        let Some(catch_end) = find_matching_block_end(content, catch_open) else {
            continue;
        };
        let catch_body = &content[catch_open + 1..catch_end - 1];

        for cap in decl_re.captures_iter(try_body) {
            let var_name = cap.get(1).map_or("", |g| g.as_str());
            if var_name.is_empty() {
                continue;
            }
            let esc = regex::escape(var_name);
            let redecl_in_catch = regex::Regex::new(&format!(r"\b(?:const|let|var)\s+{}\b", esc))
                .ok()
                .map_or(false, |re| re.is_match(catch_body));
            if redecl_in_catch {
                continue;
            }
            if let Ok(ref_re) = regex::Regex::new(&format!(r"\b{}\b", esc)) {
                let referenced_in_catch = ref_re.find_iter(catch_body).any(|rm| {
                    let prev_ch = catch_body[..rm.start()].trim_end().chars().next_back();
                    let next_ch = catch_body[rm.end()..].trim_start().chars().next();
                    // Exclude property access `.body` and object-key `body:` unless shorthand `{ body }`
                    prev_ch != Some('.') && next_ch != Some(':')
                });
                if referenced_in_catch {
                    issues.push(ReviewIssue {
                        rule_id: "api.try-catch-body-scope".to_string(),
                        severity: ReviewSeverity::Error,
                        message: format!(
                            "Variable `{}` in `{}` is declared inside `try` and referenced inside `catch` (causes TS2304).",
                            var_name, file_path
                        ),
                        repair_directive: format!(
                            "Hoist `let {}: any = {{}};` before `try {{` and assign `{} = await request.json();` inside `try`.",
                            var_name, var_name
                        ),
                    });
                    break;
                }
            }
        }
    }

    issues
}

/// Holistic semantic & formatting review executed across generated or healed files.
pub fn review_generated_file(file_path: &str, content: &str) -> Vec<ReviewIssue> {
    let mut issues = Vec::new();
    let low = file_path.replace('\\', "/").to_ascii_lowercase();
    let is_css = low.ends_with(".css") || low.ends_with(".scss");
    let is_ts_js = low.ends_with(".ts") || low.ends_with(".tsx") || low.ends_with(".js") || low.ends_with(".jsx");
    let is_dts = low.ends_with(".d.ts");

    if is_css {
        if let Err(issue) = check_delimiter_balance(content, true) {
            issues.push(issue);
        }
        return issues;
    }

    if !is_ts_js || is_dts {
        return issues;
    }

    if let Err(issue) = check_delimiter_balance(content, false) {
        issues.push(issue);
    } else if let Some(ast_issues) = crate::ast_engine::syntax_issues_for(file_path, content) {
        if let Some(first) = ast_issues.first() {
            let summary = ast_issues
                .iter()
                .take(3)
                .map(|i| i.describe())
                .collect::<Vec<_>>()
                .join(" | ");
            issues.push(ReviewIssue {
                rule_id: "syntax:ast-parse-error".to_string(),
                severity: ReviewSeverity::Error,
                message: format!(
                    "AST syntax check found {} parse error(s) in `{}` ({}).",
                    ast_issues.len(),
                    file_path,
                    summary
                ),
                repair_directive: format!(
                    "Fix the TypeScript/TSX syntax error at line {}, col {} (`{}`). Ensure all statements, expressions, JSX tags, and type annotations are valid.",
                    first.line, first.column, first.snippet
                ),
            });
        }
    }

    if low.ends_with("middleware.ts") || low.ends_with("middleware.js") {
        issues.extend(review_middleware_semantics(content));
    }

    issues.extend(check_app_router_api_route_form(content, file_path));
    issues.extend(check_try_catch_body_scope(content, file_path));

    issues
}

/// Generates targeted, non-generic architectural & structural directives when auto-healing stagnates
/// (e.g., repeated `rolled_back` rounds with unchanged error counts) or encounters structural mismatches.
pub fn diagnose_heal_stagnation(
    file_path: &str,
    content: &str,
    diagnostics_text: &str,
    prior_attempts: usize,
) -> Vec<String> {
    let mut directives: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for issue in review_generated_file(file_path, content) {
        if seen.insert(issue.rule_id.clone()) {
            directives.push(format!("[{}] {} -> {}", issue.rule_id, issue.message, issue.repair_directive));
        }
    }

    let low = file_path.replace('\\', "/").to_ascii_lowercase();
    let diag_low = diagnostics_text.to_lowercase();

    if low.ends_with(".ts")
        && !low.ends_with(".d.ts")
        && (diagnostics_text.contains("TS1005") || diagnostics_text.contains("TS1109") || diagnostics_text.contains("TS1161"))
        && (content.contains("</") || content.contains("/>"))
    {
        if seen.insert("diag:ts-jsx-mismatch".to_string()) {
            directives.push(
                "[format:jsx-in-plain-ts] This file ends in `.ts` (JSX syntax `<Tag>...</Tag>` is invalid here). Replace ALL JSX with `React.createElement(Tag, props, children)` + `import React from 'react';`, or export non-JSX helpers."
                    .to_string(),
            );
        }
    }

    if (diag_low.contains("next/server") || diag_low.contains("nextapirequest") || diag_low.contains("route") && diag_low.contains("export"))
        && (low.contains("/api/") && (low.ends_with("/route.ts") || low.ends_with("/route.js")))
    {
        if seen.insert("diag:route-export-boundary".to_string()) {
            directives.push(
                "[route:app-router-boundary] App Router `route.ts` may ONLY export HTTP handlers (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`) taking `(request: NextRequest)` and returning `NextResponse.json(...)`. Move shared `authOptions` or helpers into `@/config/auth` or `@/lib/...`."
                    .to_string(),
            );
        }
    }

    if diag_low.contains("has no exported member 'authoptions'")
        || content.contains("/api/auth/[...nextauth]/route")
    {
        if seen.insert("diag:nextauth-route-import".to_string()) {
            directives.push(
                "[route:app-router-module-import] NEVER import from `@/app/api/auth/[...nextauth]/route` in another file. Import shared `authOptions` from `@/config/auth` or `@/lib/auth`."
                    .to_string(),
            );
        }
    }

    if prior_attempts >= 1 && seen.insert("diag:anti-stagnation".to_string()) {
        directives.push(format!(
            "[heal:anti-stagnation] {} prior repair attempt(s) left the exact same diagnostic count/form. Do NOT emit cosmetic or identical code; change the offending expression/method form directly.",
            prior_attempts
        ));
    }

    directives
}
