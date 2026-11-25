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
        for ext in &[".tsx", ".ts", ".jsx", ".js", ".css", ".scss", ".mjs", ".cjs"] {
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
                    || norm_low.contains(".expo/")
                    || norm_low.starts_with(".expo")
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

    // Fallback: extensionless source paths emitted in TypeScript/Next/Metro build errors (e.g. `typeof import(".../src/app/models/page")` or `app/(tabs)/index`)
    for token in cleaned.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ',' | '<' | '>')) {
        let norm = token.replace('\\', "/");
        let mut norm_trim = norm.trim_matches(|c: char| matches!(c, '[' | ']' | ':' | ';'));
        if norm_trim.starts_with('(') {
            let trimmed_open = norm_trim.trim_start_matches('(');
            if !trimmed_open.is_empty() {
                norm_trim = trimmed_open;
            }
        }
        while norm_trim.ends_with(')') && norm_trim.matches(')').count() > norm_trim.matches('(').count() {
            norm_trim = &norm_trim[..norm_trim.len() - 1];
        }
        if norm_trim.is_empty() {
            continue;
        }
        let low = norm_trim.to_lowercase();
        if low.starts_with("http://")
            || low.starts_with("https://")
            || low.contains("node_modules/")
            || low.contains(".next/")
            || low.contains(".expo/")
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
        } else if let Some(app_idx) = low.find("/app/") {
            let candidate = norm_trim[app_idx + 1..].trim_end_matches('/');
            if candidate.matches('/').count() >= 1 {
                return Some(candidate.to_string());
            }
        } else if low.starts_with("app/") && norm_trim.matches('/').count() >= 1 {
            return Some(norm_trim.trim_end_matches('/').to_string());
        } else if low.starts_with("./app/") && norm_trim.matches('/').count() >= 2 {
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

pub fn collect_project_source_files(proj_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if !proj_dir.is_dir() {
        return files;
    }
    let mut stack = vec![proj_dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name_os = entry.file_name();
            let name = name_os.to_string_lossy();
            if path.is_dir() {
                if matches!(
                    name.as_ref(),
                    "node_modules" | ".next" | ".expo" | ".expo-verify" | ".git" | "dist" | "build" | "out" | "target" | ".axiom" | ".turbo" | ".vercel"
                ) {
                    continue;
                }
                stack.push(path);
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(proj_dir) {
                    files.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    files.sort();
    files
}

fn strip_source_extension(path: &str) -> &str {
    let last_slash = path.rfind('/');
    let dot_idx = path.rfind('.').filter(|&d| last_slash.map_or(true, |sl| d > sl));
    match dot_idx {
        Some(d) => &path[..d],
        None => path,
    }
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
    } else if norm_rel.to_lowercase().starts_with(&format!("{}/", proj_dir_norm.to_lowercase())) {
        norm_rel = norm_rel[proj_dir_norm.len() + 1..].to_string();
    } else if let Some(src_idx) = norm_rel.rfind("/src/") {
        norm_rel = norm_rel[src_idx + 1..].to_string();
    } else if let Some(prisma_idx) = norm_rel.rfind("/prisma/") {
        norm_rel = norm_rel[prisma_idx + 1..].to_string();
    }
    let rel_path = norm_rel.trim_start_matches("./");

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

    // Universal tree matching against all actual project files (no hardcoded directory allowlists):
    // 1. Exact suffix match, 2. Extension-tolerant suffix match, 3. Exact basename match.
    let all_files = collect_project_source_files(proj_dir);
    if !all_files.is_empty() && !rel_path.is_empty() {
        let suffix = format!("/{}", rel_path);
        let mut suffix_matches: Vec<&String> = all_files
            .iter()
            .filter(|f| f.as_str() == rel_path || f.ends_with(&suffix))
            .collect();
        if !suffix_matches.is_empty() {
            suffix_matches.sort_by_key(|f| (f.len(), f.as_str()));
            return Some(suffix_matches[0].clone());
        }

        if !base_no_ext.is_empty() {
            let stem_suffix = format!("/{}", base_no_ext);
            let mut ext_tolerant_matches: Vec<&String> = all_files
                .iter()
                .filter(|f| {
                    let f_stem = strip_source_extension(f);
                    f_stem == base_no_ext || f_stem.ends_with(&stem_suffix)
                })
                .collect();
            if !ext_tolerant_matches.is_empty() {
                ext_tolerant_matches.sort_by_key(|f| (f.len(), f.as_str()));
                return Some(ext_tolerant_matches[0].clone());
            }
        }

        let base_name = rel_path.rsplit('/').next().unwrap_or(rel_path);
        if !base_name.is_empty() && base_name.contains('.') {
            let mut base_matches: Vec<&String> = all_files
                .iter()
                .filter(|f| f.rsplit('/').next() == Some(base_name))
                .collect();
            if !base_matches.is_empty() {
                base_matches.sort_by_key(|f| (f.len(), f.as_str()));
                return Some(base_matches[0].clone());
            }
        }
    }

    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedErrorSource {
    pub rel_path: String,
    pub line: Option<usize>,
    pub matched_by: String,
}

fn normalize_ws_for_frame(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_msg_path_candidates(proj_dir: &Path, clean_err: &str) -> Vec<(String, Option<usize>)> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let norm_err = clean_err.replace('\\', "/");
    let proj_dir_norm = proj_dir.to_string_lossy().replace('\\', "/");
    let proj_dir_low = proj_dir_norm.to_lowercase();

    // Also inspect chunk URLs like `/_next/static/chunks/app/layout.js`
    if let Ok(chunk_re) = regex::Regex::new(r"/_next/static/chunks/(?:app|pages)/[^\s\)\]'`<>]+?\.(?:m?js)(?:[?#][^\s\)\]'`<>]*)?") {
        for m in chunk_re.find_iter(&norm_err) {
            if let Some(mapped) = map_next_chunk_url_to_source(m.as_str()) {
                if seen.insert((mapped.clone(), None)) {
                    out.push((mapped, None));
                }
            }
        }
    }

    let Ok(path_re) = regex::Regex::new(
        r#"(?i)((?:webpack-internal:///[^\s\)\]'`<>:]*|file:///[^\s\)\]'`<>:]*|[a-zA-Z]:/[^\s\)\]'`<>:]*|[A-Za-z0-9_.@\[\]()~/-]+/)?[A-Za-z0-9_.\[\]()~-]+\.(?:tsx|ts|jsx|js|mjs|cjs|css|scss|prisma))(?:\s*:\s*(\d+)(?::\d+)?|\s*\(\s*(\d+)\s*[,:]\s*\d+\s*\))?"#,
    ) else {
        return out;
    };

    for cap in path_re.captures_iter(&norm_err) {
        let raw_path = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if raw_path.is_empty() {
            continue;
        }
        let line_num = cap
            .get(2)
            .or_else(|| cap.get(3))
            .and_then(|m| m.as_str().parse::<usize>().ok());

        let mut cleaned = raw_path
            .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '(' | '[' | '<'))
            .to_string();

        let low_pre = cleaned.to_lowercase();
        if low_pre.starts_with("http://") || low_pre.starts_with("https://") {
            continue;
        }

        if let Some(wi_idx) = cleaned.find("webpack-internal:///") {
            let after = &cleaned[wi_idx + "webpack-internal:///".len()..];
            // Strip optional `(rsc)/` or `(app-pages-browser)/` wrapper group
            let stripped = if after.starts_with('(') {
                if let Some(close) = after.find(")/") {
                    &after[close + 2..]
                } else {
                    after
                }
            } else {
                after
            };
            cleaned = stripped.to_string();
        } else if let Some(stripped_file) = cleaned.strip_prefix("file://") {
            // On Windows file:///C:/... -> strip extra leading '/' if drive follows
            let s = if stripped_file.starts_with('/')
                && stripped_file.as_bytes().get(2) == Some(&b':')
            {
                &stripped_file[1..]
            } else {
                stripped_file
            };
            cleaned = s.to_string();
        }

        // Strip proj_dir prefix (case-insensitive for Windows drive/casing parity)
        if !proj_dir_norm.is_empty() {
            let prefix_with_slash = format!("{}/", proj_dir_norm);
            let prefix_low_slash = format!("{}/", proj_dir_low);
            if cleaned.starts_with(&prefix_with_slash) {
                cleaned = cleaned[prefix_with_slash.len()..].to_string();
            } else if cleaned.to_lowercase().starts_with(&prefix_low_slash) {
                cleaned = cleaned[prefix_low_slash.len()..].to_string();
            }
        }

        while cleaned.starts_with("./") {
            cleaned = cleaned[2..].to_string();
        }
        let cleaned_low = cleaned.to_lowercase();
        if cleaned.is_empty()
            || cleaned_low.contains("node_modules/")
            || cleaned_low.ends_with("/node_modules")
            || cleaned_low.contains("/.next/")
            || cleaned_low.starts_with(".next/")
            || cleaned_low.contains("/_next/")
            || cleaned_low.starts_with("_next/")
            || cleaned_low.contains("/.git/")
            || cleaned_low.starts_with(".git/")
            || cleaned_low.starts_with("http://")
            || cleaned_low.starts_with("https://")
        {
            continue;
        }

        // If an external absolute path didn't match proj_dir directly (e.g. symlink / casing / `/src/` inside path),
        // also trim to `src/` if it starts with a drive letter or `/`
        if cleaned.starts_with('/') || cleaned.as_bytes().get(1) == Some(&b':') {
            if let Some(idx) = cleaned_low.rfind("/src/") {
                cleaned = cleaned[idx + 1..].to_string();
            } else if let Some(idx) = cleaned_low.rfind("/prisma/") {
                cleaned = cleaned[idx + 1..].to_string();
            }
        }

        if seen.insert((cleaned.clone(), line_num)) {
            out.push((cleaned, line_num));
        }
    }

    out
}

pub fn locate_error_source_file(
    proj_dir: &Path,
    hinted_path: &str,
    error_msg: &str,
) -> Option<LocatedErrorSource> {
    let all_files = collect_project_source_files(proj_dir);
    if all_files.is_empty() {
        return None;
    }

    let clean_err = strip_ansi_codes(error_msg);
    let msg_candidates = extract_msg_path_candidates(proj_dir, &clean_err);

    // Normalize hinted_path
    let proj_dir_norm = proj_dir.to_string_lossy().replace('\\', "/");
    let proj_dir_low = proj_dir_norm.to_lowercase();
    let mut norm_hint = hinted_path.trim().replace('\\', "/");
    if !proj_dir_norm.is_empty() {
        let p_slash = format!("{}/", proj_dir_norm);
        let p_low = format!("{}/", proj_dir_low);
        if norm_hint.starts_with(&p_slash) {
            norm_hint = norm_hint[p_slash.len()..].to_string();
        } else if norm_hint.to_lowercase().starts_with(&p_low) {
            norm_hint = norm_hint[p_low.len()..].to_string();
        }
    }
    while norm_hint.starts_with("./") {
        norm_hint = norm_hint[2..].to_string();
    }
    let norm_hint_low = norm_hint.to_lowercase();
    let hint_valid = !norm_hint.is_empty()
        && !norm_hint_low.starts_with("http://")
        && !norm_hint_low.starts_with("https://")
        && !norm_hint_low.starts_with("webpack-internal:")
        && !norm_hint_low.contains("node_modules/");

    // Extract code-frame lines
    let mut primary_frames: Vec<(usize, String)> = Vec::new();
    let mut primary_line_nums = std::collections::HashSet::new();
    if let Ok(re_prim) = regex::Regex::new(r"(?m)^\s*>\s*(\d+)\s*\|\s*(.+)$") {
        for cap in re_prim.captures_iter(&clean_err) {
            let Some(ln) = cap.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) else { continue };
            let snippet = normalize_ws_for_frame(cap.get(2).map(|m| m.as_str()).unwrap_or(""));
            if snippet.len() >= 4 {
                primary_line_nums.insert(ln);
                primary_frames.push((ln, snippet));
            }
        }
    }
    let mut context_frames: Vec<(usize, String)> = Vec::new();
    if let Ok(re_ctx) = regex::Regex::new(r"(?m)^\s*(\d+)\s*\|\s*(.+)$") {
        for cap in re_ctx.captures_iter(&clean_err) {
            let Some(ln) = cap.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) else { continue };
            if primary_line_nums.contains(&ln) {
                continue;
            }
            let snippet = normalize_ws_for_frame(cap.get(2).map(|m| m.as_str()).unwrap_or(""));
            if snippet.len() >= 6 {
                context_frames.push((ln, snippet));
            }
        }
    }

    #[derive(Clone, Default)]
    struct FileScore {
        score: i32,
        line: Option<usize>,
        matched_code_frame: bool,
        reasons: Vec<&'static str>,
    }

    let mut scores: Vec<FileScore> = vec![FileScore::default(); all_files.len()];

    for (idx, f) in all_files.iter().enumerate() {
        let f_stem = strip_source_extension(f);
        let f_base = f.rsplit('/').next().unwrap_or(f);

        // Score best match from error_msg path candidates
        let mut best_msg_pts = 0i32;
        let mut best_msg_line: Option<usize> = None;
        let mut best_msg_reason: Option<&'static str> = None;
        for (cand, cand_line) in &msg_candidates {
            let cand_stem = strip_source_extension(cand);
            let cand_base = cand.rsplit('/').next().unwrap_or(cand);
            let cand_has_ext = cand_base.contains('.');

            let (pts, reason) = if f == cand {
                (100, "error-msg exact path")
            } else if f.ends_with(&format!("/{}", cand))
                || (f_stem != f && f_stem.ends_with(&format!("/{}", cand_stem)))
                || (cand_has_ext && f_stem == cand_stem)
            {
                (70, "error-msg suffix path")
            } else if cand_has_ext && f_base == cand_base {
                (30, "error-msg basename")
            } else {
                (0, "")
            };
            if pts > best_msg_pts || (pts == best_msg_pts && pts > 0 && best_msg_line.is_none() && cand_line.is_some()) {
                best_msg_pts = pts;
                best_msg_line = *cand_line;
                best_msg_reason = Some(reason);
            }
        }
        if best_msg_pts > 0 {
            scores[idx].score += best_msg_pts;
            if best_msg_line.is_some() {
                scores[idx].line = best_msg_line;
            }
            if let Some(r) = best_msg_reason {
                scores[idx].reasons.push(r);
            }
        }

        // Score hinted_path
        if hint_valid {
            let hint_stem = strip_source_extension(&norm_hint);
            let hint_base = norm_hint.rsplit('/').next().unwrap_or(&norm_hint);
            let hint_has_ext = hint_base.contains('.');
            let (h_pts, h_reason) = if f == &norm_hint {
                (60, "hint exact path")
            } else if f.ends_with(&format!("/{}", norm_hint))
                || (f_stem != f && f_stem.ends_with(&format!("/{}", hint_stem)))
            {
                (45, "hint suffix path")
            } else if hint_has_ext && f_base == hint_base {
                (20, "hint basename")
            } else {
                (0, "")
            };
            if h_pts > 0 {
                scores[idx].score += h_pts;
                scores[idx].reasons.push(h_reason);
            }
        }
    }

    let check_code_frame = |idx: usize, scores: &mut [FileScore]| {
        if primary_frames.is_empty() && context_frames.is_empty() {
            return;
        }
        let rel = &all_files[idx];
        let full = proj_dir.join(rel);
        let Ok(meta) = std::fs::metadata(&full) else { return };
        if meta.len() > 256 * 1024 {
            return;
        }
        let Ok(content) = std::fs::read_to_string(&full) else { return };
        let file_lines: Vec<String> = content.lines().map(normalize_ws_for_frame).collect();
        if file_lines.is_empty() {
            return;
        }

        let mut prim_bonus = 0i32;
        for (ln, snippet) in &primary_frames {
            let target_idx = ln.saturating_sub(1);
            let start = target_idx.saturating_sub(2);
            let end = (target_idx + 2).min(file_lines.len().saturating_sub(1));
            let near_match = start <= end && (start..=end).any(|li| file_lines[li].contains(snippet));
            if near_match {
                if prim_bonus < 120 {
                    prim_bonus = 120;
                }
                scores[idx].line = Some(*ln);
                scores[idx].matched_code_frame = true;
                if !scores[idx].reasons.contains(&"code-frame line match") {
                    scores[idx].reasons.push("code-frame line match");
                }
            } else if file_lines.iter().any(|l| l.contains(snippet)) {
                if prim_bonus < 80 {
                    prim_bonus = 80;
                }
                scores[idx].matched_code_frame = true;
                if !scores[idx].reasons.contains(&"code-frame snippet match") {
                    scores[idx].reasons.push("code-frame snippet match");
                }
            }
        }
        scores[idx].score += prim_bonus;

        let mut ctx_bonus = 0i32;
        for (ln, snippet) in &context_frames {
            if ctx_bonus >= 75 {
                break;
            }
            let target_idx = ln.saturating_sub(1);
            let start = target_idx.saturating_sub(2);
            let end = (target_idx + 2).min(file_lines.len().saturating_sub(1));
            if start <= end && (start..=end).any(|li| file_lines[li].contains(snippet)) {
                ctx_bonus = (ctx_bonus + 25).min(75);
                scores[idx].matched_code_frame = true;
                if !scores[idx].reasons.contains(&"code-frame context match") {
                    scores[idx].reasons.push("code-frame context match");
                }
            }
        }
        scores[idx].score += ctx_bonus;
    };

    let has_frames = !primary_frames.is_empty() || !context_frames.is_empty();
    let candidate_indices: Vec<usize> = scores
        .iter()
        .enumerate()
        .filter(|(_, s)| s.score > 0)
        .map(|(i, _)| i)
        .collect();

    if has_frames {
        for &idx in &candidate_indices {
            check_code_frame(idx, &mut scores);
        }
        let any_candidate_matched_frame = candidate_indices
            .iter()
            .any(|&idx| scores[idx].matched_code_frame);
        if candidate_indices.is_empty() || !any_candidate_matched_frame {
            let cand_set: std::collections::HashSet<usize> = candidate_indices.into_iter().collect();
            for idx in 0..all_files.len() {
                if cand_set.contains(&idx) {
                    continue;
                }
                let low = all_files[idx].to_ascii_lowercase();
                if low.ends_with(".tsx")
                    || low.ends_with(".ts")
                    || low.ends_with(".jsx")
                    || low.ends_with(".js")
                    || low.ends_with(".css")
                    || low.ends_with(".scss")
                {
                    check_code_frame(idx, &mut scores);
                }
            }
        }
    }

    let best_idx = scores
        .iter()
        .enumerate()
        .filter(|(_, s)| s.score > 0)
        .max_by(|(i_a, s_a), (i_b, s_b)| {
            let a_src = all_files[*i_a].starts_with("src/");
            let b_src = all_files[*i_b].starts_with("src/");
            s_a.score
                .cmp(&s_b.score)
                .then_with(|| a_src.cmp(&b_src))
                .then_with(|| all_files[*i_b].len().cmp(&all_files[*i_a].len()))
                .then_with(|| all_files[*i_b].cmp(&all_files[*i_a]))
        })
        .map(|(i, _)| i)?;

    let best_score = &scores[best_idx];
    let matched_by = if best_score.reasons.is_empty() {
        "path score".to_string()
    } else {
        best_score.reasons.join(" + ")
    };

    Some(LocatedErrorSource {
        rel_path: all_files[best_idx].clone(),
        line: best_score.line,
        matched_by,
    })
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
            || trimmed.contains("✓ Ready in")
            || trimmed.contains("Web Bundled")
            || trimmed.contains("Waiting on http://")
            || trimmed.contains("Web is waiting on")
            || trimmed.contains("Metro waiting on");

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
            || clean_line.contains("Unable to resolve")
            || clean_line.contains("Metro has encountered an error")
            || clean_line.contains("Bundling failed")
            || clean_line.contains("Web Bundling failed")
            || clean_line.contains("TransformError")
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
    fn walk_and_fix(dir: &std::path::Path, proj_dir: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules" && name != ".next" && name != "build" && name != "dist" && name != ".git" {
                        walk_and_fix(&path, proj_dir);
                    }
                } else if path.is_file() {
                    let path_str = path.to_string_lossy();
                    if path_str.ends_with(".tsx") || path_str.ends_with(".jsx") || path_str.ends_with(".ts") || path_str.ends_with(".js") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if content.contains("react-icons") || content.contains("-icons-react") || content.contains("lucide-react") {
                                let mut fixed = sanitize_react_icons_imports(&content);
                                fixed = crate::self_healing::reconcile_external_icon_imports_in_content(proj_dir, &fixed);
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

    walk_and_fix(proj_dir, proj_dir);
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

    let is_expo = crate::templates_expo::is_expo_project_dir(&proj_dir);
    if is_expo {
        kill_process_on_port(8081);
        crate::templates_expo::ensure_expo_html_inspector_injected(&proj_dir);
        repair_package_json_dependencies(&proj_dir);
    } else {
        // Ensure multi-platform entry routes (Electron src/renderer/App.tsx, Mobile app/ routes) are bridged into src/app/page.tsx
        ensure_multiplatform_entry_routes_bridged(&proj_dir);
        sanitize_tailwind_config_in_project(&proj_dir);

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
                if is_expo || (platform_str == "mobile" && scripts.contains_key("web")) {
                    run_args = vec!["run".to_string(), "web".to_string()];
                } else if platform_str == "mobile" {
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
    if is_expo {
        cmd.env("BROWSER", "none").env("EXPO_NO_TELEMETRY", "1");
    }
    
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
                            || filtered.contains("Waiting on http://")
                            || filtered.contains("Web is waiting on")
                            || filtered.contains("Web Bundled")
                            || filtered.contains("exp://")
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

    // Always aggressively liberate port 3000 (and 8081 for Expo Metro) to prevent any lingering/zombie processes
    kill_process_on_port(3000);
    if crate::templates_expo::is_expo_project_dir(&get_project_path(&project_id)) {
        kill_process_on_port(8081);
    }

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

