use crate::*;

// ==================== ORCHESTRATOR API ====================

pub fn detect_infinite_loop(content: &str) -> bool {
    // 1. Check character repetition (e.g. } } } } } } } } } } } } } } })
    // Ignore common banner, divider, and comment formatting characters that frequently occur in large blocks
    let divider_chars = ['=', '-', '*', '/', '#', '_', ' ', '.', '\t', '~', ':'];
    let trimmed_chars: Vec<char> = content.trim_end().chars().rev().take(30).collect();
    if trimmed_chars.len() >= 25 {
        let first = trimmed_chars[0];
        if !first.is_alphanumeric() && !divider_chars.contains(&first) && trimmed_chars.iter().all(|&c| c == first) {
            return true;
        }
    }

    // 2. Check line block repetition (e.g. repeating a block of 1-5 lines 5 times)
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let n = lines.len();
    for block_size in 1..=5 {
        if n >= block_size * 5 {
            let mut is_loop = true;
            for i in 0..block_size {
                let idx_curr = n - block_size + i;
                let idx_prev1 = n - 2 * block_size + i;
                let idx_prev2 = n - 3 * block_size + i;
                let idx_prev3 = n - 4 * block_size + i;
                let idx_prev4 = n - 5 * block_size + i;
                if lines[idx_curr] != lines[idx_prev1]
                    || lines[idx_curr] != lines[idx_prev2]
                    || lines[idx_curr] != lines[idx_prev3]
                    || lines[idx_curr] != lines[idx_prev4]
                {
                    is_loop = false;
                    break;
                }
            }
            if is_loop {
                let block_content: String = lines[n-block_size..n].join(" ");
                let trimmed = block_content.trim();
                if trimmed.len() > 8 && trimmed.chars().any(|c| c.is_alphanumeric()) {
                    return true;
                }
            }
        }
    }

    // 3. Check word repetition on the last line (require at least 8 repetitions to avoid false positives on CSS shorthands)
    if let Some(last_line) = content.lines().last() {
        let words: Vec<&str> = last_line.split_whitespace().collect();
        let nw = words.len();
        for w_size in 1..=4 {
            if nw >= w_size * 8 {
                let mut w_loop = true;
                for i in 0..w_size {
                    let w_curr = words[nw - w_size + i];
                    for repeat_idx in 1..8 {
                        if w_curr != words[nw - (repeat_idx + 1) * w_size + i] {
                            w_loop = false;
                            break;
                        }
                    }
                    if !w_loop {
                        break;
                    }
                }
                if w_loop {
                    let word_block = words[nw - w_size..nw].join(" ");
                    if word_block.len() > 3 {
                        return true;
                    }
                }
            }
        }
    }

    false
}

pub fn get_safe_fallback_content(file_path: &str, _lang: &str, desc: &str) -> String {
    let lower = file_path.to_lowercase();
    if lower.ends_with(".css") {
        format!("/* {}\n * {}\n */\n\n.container {{\n  display: flex;\n  flex-direction: column;\n}}\n", file_path, desc)
    } else if lower.ends_with(".json") {
        "{\n}\n".to_string()
    } else if lower.ends_with(".tsx") || lower.ends_with(".jsx") {
        let comp_name = std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Component");
        format!("import React from 'react';\n\nexport default function {}() {{\n  return (\n    <div className=\"p-4\">\n      <h1 className=\"text-xl font-bold\">{}</h1>\n    </div>\n  );\n}}\n", comp_name, comp_name)
    } else if lower.ends_with(".ts") || lower.ends_with(".js") {
        format!("// {}\n// {}\n\nexport {{}};\n", file_path, desc)
    } else {
        format!("// {}\n", desc)
    }
}

pub fn index_modules(dir: &std::path::Path, src_root: &std::path::Path, index: &mut std::collections::HashMap<String, Vec<String>>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "node_modules" && name != ".next" && name != ".git" {
                    index_modules(&path, src_root, index);
                }
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(rel_path) = path.strip_prefix(src_root) {
                            let mut rel_str = rel_path.to_string_lossy().replace('\\', "/");
                            if let Some(dot_idx) = rel_str.rfind('.') {
                                rel_str.truncate(dot_idx);
                            }
                            index.entry(stem.to_string()).or_insert_with(Vec::new).push(rel_str);
                        }
                    }
                }
            }
        }
    }
}

fn normalize_path_components(p: &std::path::Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn scan_and_fix_imports(dir: &std::path::Path, src_root: &std::path::Path, index: &std::collections::HashMap<String, Vec<String>>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "node_modules" && name != ".next" && name != ".git" {
                    scan_and_fix_imports(&path, src_root, index);
                }
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let normalized_first = crate::sanitizer::normalize_bare_local_imports(&content);
                        let mut changed = normalized_first != content;
                        let current_rel_no_ext = path
                            .strip_prefix(src_root)
                            .ok()
                            .map(|p| {
                                let mut s = p.to_string_lossy().replace('\\', "/");
                                if let Some(dot_idx) = s.rfind('.') {
                                    s.truncate(dot_idx);
                                }
                                s
                            })
                            .unwrap_or_default();

                        let mut lines: Vec<String> = normalized_first.lines().map(|s| s.to_string()).collect();
                        
                        for line in lines.iter_mut() {
                            let is_reexport_from = line.trim_start().starts_with("export ") && line.contains(" from ");
                            if (line.contains("import ") || line.contains("import{") || is_reexport_from || line.contains("require(")) &&
                               (line.contains('\'') || line.contains('"') || line.contains('`')) {
                                
                                let quotes = ['\'', '"', '`'];
                                let mut literal = String::new();
                                for quote in quotes {
                                    if let Some(start) = line.find(quote) {
                                        if let Some(end) = line[start+1..].find(quote) {
                                            literal = line[start+1..start+1+end].to_string();
                                            break;
                                        }
                                    }
                                }
                                
                                let literal = literal.trim();
                                let lit_low = literal.replace('\\', "/").to_lowercase();
                                let is_prisma_pseudo_spec = lit_low == "@/prisma/schema"
                                    || lit_low == "~/prisma/schema"
                                    || lit_low == "src/prisma/schema"
                                    || lit_low == "prisma/schema"
                                    || lit_low == "@/prisma/client"
                                    || lit_low == "~/prisma/client"
                                    || lit_low == "src/prisma/client"
                                    || lit_low == "prisma/client"
                                    || lit_low.ends_with("/prisma/schema")
                                    || lit_low.ends_with("/prisma/client")
                                    || lit_low.ends_with("/prisma/schema.prisma");
                                if !literal.is_empty() && !is_prisma_pseudo_spec && (literal.starts_with('.') || literal.starts_with("@/")) {
                                    let mut exists = false;
                                    
                                    let raw_target = if let Some(sub) = literal.strip_prefix("@/") {
                                        src_root.join(sub)
                                    } else if let Some(parent) = path.parent() {
                                        parent.join(literal)
                                    } else {
                                        src_root.join(literal)
                                    };
                                    let target_abs = normalize_path_components(&raw_target);
                                    
                                    let variations = [
                                        target_abs.clone(),
                                        target_abs.with_extension("ts"),
                                        target_abs.with_extension("tsx"),
                                        target_abs.with_extension("js"),
                                        target_abs.with_extension("jsx"),
                                        target_abs.join("index.ts"),
                                        target_abs.join("index.tsx"),
                                        target_abs.join("index.js"),
                                        target_abs.join("index.jsx"),
                                    ];
                                    
                                    for var_path in variations {
                                        if var_path.is_file() {
                                            exists = true;
                                            break;
                                        }
                                    }
                                    
                                    if !exists {
                                        let parts: Vec<&str> = literal.split('/').collect();
                                        if let Some(&last_part_raw) = parts.last() {
                                            let last_part = last_part_raw
                                                .trim_end_matches(".tsx")
                                                .trim_end_matches(".ts")
                                                .trim_end_matches(".jsx")
                                                .trim_end_matches(".js");
                                            if !last_part.is_empty() {
                                                if let Some(candidates) = index.get(last_part) {
                                                    // Prevent rewriting into a self-import (`src/utils/iot.ts` importing `@/utils/iot`)
                                                    let non_self: Vec<&String> = candidates
                                                        .iter()
                                                        .filter(|c| **c != current_rel_no_ext)
                                                        .collect();
                                                    if non_self.len() == 1 {
                                                        let corrected_import = format!("@/{}", non_self[0]);
                                                        let old_import_quote = format!("'{}'", literal);
                                                        let old_import_dquote = format!("\"{}\"", literal);
                                                        let old_import_backtick = format!("`{}`", literal);
                                                        
                                                        let new_import_quote = format!("'{}'", corrected_import);
                                                        let new_import_dquote = format!("\"{}\"", corrected_import);
                                                        let new_import_backtick = format!("`{}`", corrected_import);
                                                        
                                                        if line.contains(&old_import_quote) {
                                                            *line = line.replace(&old_import_quote, &new_import_quote);
                                                            changed = true;
                                                        } else if line.contains(&old_import_dquote) {
                                                            *line = line.replace(&old_import_dquote, &new_import_dquote);
                                                            changed = true;
                                                        } else if line.contains(&old_import_backtick) {
                                                            *line = line.replace(&old_import_backtick, &new_import_backtick);
                                                            changed = true;
                                                        }
                                                        
                                                        debug_log_to_file(format!("[Import Fixer] Fixed broken import in {:?}: {} -> {}", path, literal, corrected_import));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        if changed {
                            let mut out = lines.join("\n");
                            if content.ends_with('\n') && !out.ends_with('\n') {
                                out.push('\n');
                            }
                            let _ = std::fs::write(&path, out);
                        }
                    }
                }
            }
        }
    }
}

pub struct MissingImport {
    pub target_path: std::path::PathBuf,
    pub importing_file_path: std::path::PathBuf,
    pub import_line: String,
    pub callers: Vec<(std::path::PathBuf, String)>,
}

pub fn collect_missing_imports(
    dir: &std::path::Path,
    src_root: &std::path::Path,
    missing: &mut Vec<MissingImport>,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "node_modules" && name != ".next" && name != ".git" {
                    collect_missing_imports(&path, src_root, missing);
                }
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for line in content.lines() {
                            let is_reexport_from = line.trim_start().starts_with("export ") && line.contains(" from ");
                            if (line.contains("import ") || line.contains("import{") || is_reexport_from || line.contains("require(")) &&
                               (line.contains('\'') || line.contains('"') || line.contains('`')) {
                                
                                let quotes = ['\'', '"', '`'];
                                let mut literal = String::new();
                                for quote in quotes {
                                    if let Some(start) = line.find(quote) {
                                        if let Some(end) = line[start+1..].find(quote) {
                                            literal = line[start+1..start+1+end].to_string();
                                            break;
                                        }
                                    }
                                }
                                
                                let literal = literal.trim();
                                let lit_low = literal.replace('\\', "/").to_lowercase();
                                let is_prisma_pseudo_spec = lit_low == "@/prisma/schema"
                                    || lit_low == "~/prisma/schema"
                                    || lit_low == "src/prisma/schema"
                                    || lit_low == "prisma/schema"
                                    || lit_low == "@/prisma/client"
                                    || lit_low == "~/prisma/client"
                                    || lit_low == "src/prisma/client"
                                    || lit_low == "prisma/client"
                                    || lit_low.ends_with("/prisma/schema")
                                    || lit_low.ends_with("/prisma/client")
                                    || lit_low.ends_with("/prisma/schema.prisma");
                                if !literal.is_empty() && !is_prisma_pseudo_spec && crate::packages::is_local_import_specifier(literal) && !literal.ends_with(".css") && !literal.ends_with(".scss") && !literal.ends_with(".json") {
                                    let mut exists = false;
                                    
                                    let raw_target = if let Some(sub) = literal.strip_prefix("@/").or_else(|| literal.strip_prefix("~/")) {
                                        src_root.join(sub)
                                    } else if let Some(sub) = literal.strip_prefix("src/") {
                                        src_root.join(sub)
                                    } else if literal.starts_with('.') {
                                        if let Some(parent) = path.parent() {
                                            parent.join(literal)
                                        } else {
                                            src_root.join(literal)
                                        }
                                    } else {
                                        src_root.join(literal.trim_start_matches('/'))
                                    };
                                    let target_abs = normalize_path_components(&raw_target);
                                    let target_norm = target_abs.to_string_lossy().replace('\\', "/").to_lowercase();
                                    if target_norm.ends_with("/prisma/schema")
                                        || target_norm.ends_with("/prisma/client")
                                        || target_norm.ends_with("/prisma/schema.prisma")
                                    {
                                        continue;
                                    }
                                    
                                    let variations = [
                                        target_abs.clone(),
                                        target_abs.with_extension("ts"),
                                        target_abs.with_extension("tsx"),
                                        target_abs.with_extension("js"),
                                        target_abs.with_extension("jsx"),
                                        target_abs.join("index.ts"),
                                        target_abs.join("index.tsx"),
                                        target_abs.join("index.js"),
                                        target_abs.join("index.jsx"),
                                    ];
                                    
                                    for var_path in variations {
                                        if var_path.is_file() {
                                            exists = true;
                                            break;
                                        }
                                    }
                                    
                                    if !exists {
                                        let target_file_path = if target_abs.is_dir() { target_abs.join("index.ts") } else if target_abs.extension().is_some() {
                                            target_abs.clone()
                                        } else {
                                            let ext_str = path.extension().and_then(|e| e.to_str()).unwrap_or("tsx");
                                            target_abs.with_extension(ext_str)
                                        };
                                        
                                        if let Some(existing) = missing.iter_mut().find(|m| m.target_path == target_file_path) {
                                            if existing.callers.len() < 12
                                                && !existing.callers.iter().any(|(p, l)| p == &path && l == line)
                                            {
                                                existing.callers.push((path.clone(), line.to_string()));
                                            }
                                        } else {
                                            missing.push(MissingImport {
                                                target_path: target_file_path,
                                                importing_file_path: path.clone(),
                                                import_line: line.to_string(),
                                                callers: vec![(path.clone(), line.to_string())],
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn repair_broken_imports(proj_dir: &std::path::Path, model: &str) {
    let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();
    let src_dir = proj_dir.join("src");
    let has_src = src_dir.exists();
    let scan_roots: Vec<std::path::PathBuf> = if has_src {
        vec![src_dir.clone()]
    } else {
        ["app", "components", "hooks", "lib", "utils", "models", "types", "services"]
            .iter()
            .map(|d| proj_dir.join(d))
            .filter(|p| p.is_dir())
            .collect()
    };
    if scan_roots.is_empty() {
        return;
    }
    crate::sanitizer::ensure_ui_primitives_reconciled(proj_dir);
    crate::sanitizer::ensure_directory_barrel_files(proj_dir);
    let alias_root = if has_src { src_dir } else { proj_dir.to_path_buf() };

    let mut module_index = std::collections::HashMap::new();
    for root in &scan_roots {
        index_modules(root, &alias_root, &mut module_index);
    }
    for root in &scan_roots {
        scan_and_fix_imports(root, &alias_root, &module_index);
    }

    let mut missing_imports = Vec::new();
    for root in &scan_roots {
        collect_missing_imports(root, &alias_root, &mut missing_imports);
    }

    if !missing_imports.is_empty() {
        debug_log_to_file(format!("[Import Fixer] Found {} missing modules that need generation.", missing_imports.len()));

        let client = reqwest::Client::new();
        let compound_re = regex::Regex::new(r"<([A-Z][A-Za-z0-9_]*)\.([A-Z][A-Za-z0-9_]*)\b").ok();

        for item in missing_imports {
            let relative_target = item.target_path.strip_prefix(proj_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| item.target_path.to_string_lossy().to_string());
            let relative_target_norm = relative_target.replace('\\', "/");

            if !item.target_path.starts_with(proj_dir) || !crate::healer::is_valid_file_path(&relative_target_norm) {
                debug_log_to_file(format!(
                    "[Import Fixer] Skipping invalid missing-module target path: {}",
                    relative_target_norm
                ));
                continue;
            }

            let all_callers: Vec<(std::path::PathBuf, String)> = if item.callers.is_empty() {
                vec![(item.importing_file_path.clone(), item.import_line.clone())]
            } else {
                item.callers.clone()
            };

            let mut all_import_lines = Vec::new();
            let mut caller_snippets = String::new();
            let mut compound_usages: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut snippet_budget = 6000usize;
            let mut seen_caller_files = std::collections::HashSet::new();

            for (caller_path, caller_imp_line) in &all_callers {
                let rel_caller = caller_path
                    .strip_prefix(proj_dir)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| caller_path.to_string_lossy().replace('\\', "/"));
                all_import_lines.push(format!("- `{}`: `{}`", rel_caller, caller_imp_line.trim()));

                if let Ok(caller_src) = std::fs::read_to_string(caller_path) {
                    if let Some(ref c_re) = compound_re {
                        for cap in c_re.captures_iter(&caller_src) {
                            let parent = cap.get(1).map_or("", |m| m.as_str());
                            let sub = cap.get(2).map_or("", |m| m.as_str());
                            if caller_imp_line.contains(parent) {
                                compound_usages.insert(format!("<{}.{}>", parent, sub));
                            }
                        }
                    }

                    if seen_caller_files.insert(rel_caller.clone()) && snippet_budget > 200 && seen_caller_files.len() <= 4 {
                        let per_file_cap = snippet_budget.min(2200);
                        let snippet: String = caller_src.chars().take(per_file_cap).collect();
                        snippet_budget = snippet_budget.saturating_sub(snippet.chars().count());
                        caller_snippets.push_str(&format!(
                            "Caller `{}`:\n```\n{}\n```\n\n",
                            rel_caller, snippet
                        ));
                    }
                }
            }

            let relative_importing = item.importing_file_path.strip_prefix(proj_dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| item.importing_file_path.to_string_lossy().to_string());

            debug_log_to_file(format!(
                "[Import Fixer] Generating missing file: {} (imported by {} caller(s), primary {} via: {})",
                relative_target_norm,
                all_callers.len(),
                relative_importing,
                item.import_line.trim()
            ));

            let mut sibling_note = String::new();
            if relative_target_norm.ends_with("/index.ts") || relative_target_norm.ends_with("/index.tsx") {
                if let Some(parent_dir) = item.target_path.parent() {
                    if let Ok(entries) = std::fs::read_dir(parent_dir) {
                        let mut sibs: Vec<String> = entries
                            .flatten()
                            .filter_map(|e| e.file_name().into_string().ok())
                            .filter(|n| !n.starts_with("index."))
                            .collect();
                        sibs.sort();
                        if !sibs.is_empty() {
                            sibling_note = format!(
                                "\nIMPORTANT: This file is a directory barrel index. ONLY re-export files that actually exist in the same folder: [{}]. NEVER re-export non-existent filenames!\n",
                                sibs.join(", ")
                            );
                        }
                    }
                }
            }

            let target_lower = relative_target_norm.to_lowercase();
            let ui_rules = if target_lower.ends_with(".tsx") || target_lower.ends_with(".jsx") {
                let mut r = String::from(
                    "\nREACT 18 / UI MODULE RULES:\n\
                     - Add `\"use client\";` as line 1 if any component uses React hooks, event handlers, or interactive UI primitives.\n\
                     - NEVER write bare `React.FC = ({ ... }) =>` without a props generic (in React 18, `React.FC` defaults to `{}` and rejects `children`, `className`, `onClick`, `variant`). ALWAYS use `React.FC<any> =` or an explicit props interface with `children?: React.ReactNode; className?: string; [key: string]: any;`.\n\
                     - Export EVERY symbol requested across ALL caller import lines listed above (matching exact named vs default imports).\n"
                );
                if !compound_usages.is_empty() {
                    let compound_list: Vec<String> = compound_usages.into_iter().collect();
                    r.push_str(&format!(
                        "- CRITICAL COMPOUND JSX REQUIREMENT: Callers render `[{}]`. Do NOT leave subcomponents undefined! Define every subcomponent cleanly (e.g. `TabsList`, `TabsTrigger`, `TabsContent`), attach them on the parent with runtime properties (e.g. `(Tabs as any).List = TabsList;`) AND type the parent with `React.FC<any> & Record<string, any>` (or `Object.assign`) so `<Parent.Sub>` works both in TypeScript AND at runtime SSR.\n",
                        compound_list.join(", ")
                    ));
                }
                r
            } else {
                String::new()
            };

            let system_prompt = "You are a Senior Next.js/React Developer. Respond with ONLY the raw, complete file content. Do not include markdown code block formatting (like ```tsx), explanations, introduction, or outro. Respond with clean source code only.";
            let user_prompt = format!(
                "The project is missing an imported module/file.\n\
                 Missing File path to generate: {}\n\
                 All caller import statements referencing this module:\n\
                 {}\n{}{}\n\
                 Caller file contents:\n\
                 {}\
                 Generate the complete implementation for `{}` so that it exports EVERY symbol, type, prop, and compound subcomponent expected by ALL callers above and compiles cleanly with zero TypeScript errors.",
                relative_target_norm,
                all_import_lines.join("\n"),
                sibling_note,
                ui_rules,
                caller_snippets,
                relative_target_norm
            );

            let ollama_req = serde_json::json!({
                "model": model,
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ],
                "stream": false,
                "options": build_ollama_options(model, 0.1, Some(4096))
            });

            if let Ok(resp) = client.post("http://127.0.0.1:11434/api/chat")
                .json(&ollama_req)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(resp_json) = resp.json::<serde_json::Value>().await {
                        if let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                            let clean_content = extract_code_content(ai_msg);
                            let processed = post_process_generated_file(&relative_target_norm, &clean_content, &serde_json::json!({}), has_tailwind);

                            if !crate::healer::looks_like_code_payload(&processed) {
                                debug_log_to_file(format!(
                                    "[Import Fixer] Rejected non-code payload for missing module '{}'",
                                    relative_target_norm
                                ));
                                continue;
                            }
                            if let Err(val_err) = crate::orchestrator::validate_file_with_abstract_rules(&relative_target_norm, &processed, model).await {
                                debug_log_to_file(format!(
                                    "[Import Fixer] Rejected invalid generated module '{}': {}",
                                    relative_target_norm, val_err
                                ));
                                continue;
                            }

                            if let Some(parent) = item.target_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }

                            if std::fs::write(&item.target_path, &processed).is_ok() {
                                debug_log_to_file(format!(
                                    "[Import Fixer] Successfully created missing module: {}",
                                    relative_target_norm
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Re-run deterministic UI & barrel reconcilers after generating missing modules so freshly
        // created UI files inherit React 18 props, compound `<Parent.Sub>` properties, and clean barrels.
        crate::sanitizer::ensure_ui_primitives_reconciled(proj_dir);
        crate::sanitizer::ensure_directory_barrel_files(proj_dir);
    }
}

pub fn ensure_theme_file_exists(proj_dir: &std::path::Path) {
    let src_dir = proj_dir.join("src");
    if !src_dir.exists() {
        return;
    }
    
    fn scan_and_create(dir: &std::path::Path, src_root: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_and_create(&path, src_root);
                } else if path.is_file() {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx" {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let mut needs_theme = false;
                            let mut theme_target_path = None;
                            
                            if content.contains("'./theme'") || content.contains("\"./theme\"") {
                                needs_theme = true;
                                if let Some(parent) = path.parent() {
                                    theme_target_path = Some(parent.join("theme.ts"));
                                }
                            } else if content.contains("'@/theme'") || content.contains("\"@/theme\"") {
                                needs_theme = true;
                                theme_target_path = Some(src_root.join("theme.ts"));
                            } else if content.contains("'@/styles/theme'") || content.contains("\"@/styles/theme\"") {
                                needs_theme = true;
                                theme_target_path = Some(src_root.join("styles").join("theme.ts"));
                            }
                            
                            if needs_theme {
                                if let Some(target) = theme_target_path {
                                    let base = target.with_extension("");
                                    let exists = base.with_extension("ts").exists() ||
                                                 base.with_extension("tsx").exists() ||
                                                 base.with_extension("js").exists() ||
                                                 base.with_extension("jsx").exists();
                                                 
                                    if !exists {
                                        if let Some(parent) = target.parent() {
                                            let _ = std::fs::create_dir_all(parent);
                                        }
                                        let default_theme = r#"import { createTheme } from '@mui/material/styles';

const theme = createTheme({
  palette: {
    mode: 'dark',
    primary: {
      main: '#6366f1', // indigo-500
    },
    secondary: {
      main: '#ec4899', // pink-500
    },
    background: {
      default: '#0f172a', // slate-900
      paper: '#1e293b',   // slate-800
    },
  },
});

export default theme;
"#;
                                        let _ = std::fs::write(&target, default_theme);
                                        debug_log_to_file(format!("[Theme Fixer] Created default theme file at {:?}", target));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    scan_and_create(&src_dir, &src_dir);
}

pub fn repair_cargo_toml_dependencies(proj_dir: &std::path::Path) {
    let cargo_paths = [
        proj_dir.join("Cargo.toml"),
        proj_dir.join("src-tauri/Cargo.toml"),
    ];

    for cargo_path in &cargo_paths {
        if cargo_path.exists() {
            if let Ok(content) = std::fs::read_to_string(cargo_path) {
                let mut changed = false;
                let mut new_lines = Vec::new();
                for line in content.lines() {
                    let mut new_line = line.to_string();
                    let trimmed = line.trim();
                    if trimmed.starts_with("tauri ") || trimmed.starts_with("tauri=") {
                        if !trimmed.contains("version = \"1.6\"") && !trimmed.contains("\"1.6\"") {
                            new_line = "tauri = { version = \"1.6\", features = [\"api-all\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("tauri-build ") || trimmed.starts_with("tauri-build=") {
                        if !trimmed.contains("\"1.6\"") {
                            new_line = "tauri-build = \"1.6\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("serde ") || trimmed.starts_with("serde=") {
                        if !trimmed.contains("version = \"1.0\"") && !trimmed.contains("\"1.0\"") {
                            new_line = "serde = { version = \"1.0\", features = [\"derive\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("serde_json ") || trimmed.starts_with("serde_json=") {
                        if !trimmed.contains("\"1.0\"") {
                            new_line = "serde_json = \"1.0\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("tokio ") || trimmed.starts_with("tokio=") {
                        if !trimmed.contains("version = \"1\"") && !trimmed.contains("\"1\"") {
                            new_line = "tokio = { version = \"1\", features = [\"full\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("reqwest ") || trimmed.starts_with("reqwest=") {
                        if !trimmed.contains("version = \"0.12\"") && !trimmed.contains("\"0.12\"") {
                            new_line = "reqwest = { version = \"0.12\", features = [\"json\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("uuid ") || trimmed.starts_with("uuid=") {
                        if !trimmed.contains("version = \"1.8\"") && !trimmed.contains("\"1.8\"") {
                            new_line = "uuid = { version = \"1.8\", features = [\"v4\", \"serde\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("chrono ") || trimmed.starts_with("chrono=") {
                        if !trimmed.contains("version = \"0.4\"") && !trimmed.contains("\"0.4\"") {
                            new_line = "chrono = { version = \"0.4\", features = [\"serde\"] }".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("bcrypt ") || trimmed.starts_with("bcrypt=") {
                        if !trimmed.contains("\"0.15\"") {
                            new_line = "bcrypt = \"0.15\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("jsonwebtoken ") || trimmed.starts_with("jsonwebtoken=") {
                        if !trimmed.contains("\"9\"") {
                            new_line = "jsonwebtoken = \"9\"".to_string();
                            changed = true;
                        }
                    } else if trimmed.starts_with("once_cell ") || trimmed.starts_with("once_cell=") {
                        if !trimmed.contains("\"1.19\"") {
                            new_line = "once_cell = \"1.19\"".to_string();
                            changed = true;
                        }
                    }
                    new_lines.push(new_line);
                }
                if changed {
                    let _ = std::fs::write(cargo_path, new_lines.join("\n"));
                }
            }
        }
    }
}

pub fn repair_pubspec_yaml_dependencies(proj_dir: &std::path::Path) {
    let pubspec_path = proj_dir.join("pubspec.yaml");
    if pubspec_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pubspec_path) {
            let mut changed = false;
            let mut new_lines = Vec::new();
            for line in content.lines() {
                let mut new_line = line.to_string();
                let trimmed = line.trim();
                if trimmed.starts_with("cupertino_icons:") {
                    if !trimmed.contains("1.0.6") {
                        new_line = "  cupertino_icons: ^1.0.6".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("http:") {
                    if !trimmed.contains("1.2.1") {
                        new_line = "  http: ^1.2.1".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("provider:") {
                    if !trimmed.contains("6.1.2") {
                        new_line = "  provider: ^6.1.2".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("shared_preferences:") {
                    if !trimmed.contains("2.2.3") {
                        new_line = "  shared_preferences: ^2.2.3".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("sqflite:") {
                    if !trimmed.contains("2.3.2") {
                        new_line = "  sqflite: ^2.3.2".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("path_provider:") {
                    if !trimmed.contains("2.1.2") {
                        new_line = "  path_provider: ^2.1.2".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("uuid:") {
                    if !trimmed.contains("4.3.3") {
                        new_line = "  uuid: ^4.3.3".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("intl:") {
                    if !trimmed.contains("0.19.0") {
                        new_line = "  intl: ^0.19.0".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("bloc:") {
                    if !trimmed.contains("8.1.3") {
                        new_line = "  bloc: ^8.1.3".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("flutter_bloc:") {
                    if !trimmed.contains("8.1.5") {
                        new_line = "  flutter_bloc: ^8.1.5".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("go_router:") {
                    if !trimmed.contains("13.2.0") {
                        new_line = "  go_router: ^13.2.0".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("cached_network_image:") {
                    if !trimmed.contains("3.3.1") {
                        new_line = "  cached_network_image: ^3.3.1".to_string();
                        changed = true;
                    }
                } else if trimmed.starts_with("google_fonts:") {
                    if !trimmed.contains("6.1.0") {
                        new_line = "  google_fonts: ^6.1.0".to_string();
                        changed = true;
                    }
                }
                new_lines.push(new_line);
            }
            if changed {
                let _ = std::fs::write(&pubspec_path, new_lines.join("\n"));
            }
        }
    }
}

pub async fn generate_and_save_healing_rule(
    file_path: &str,
    original_content: &str,
    repaired_content: &str,
    model: &str,
) {
    // Route rule extraction deterministically through the Local Error Katastar as pending
    // (never blindly persist unverified AI patches directly to global sanitizer rules).
    if let Some(patch) = crate::katastar::extract_safe_atomic_patch(file_path, original_content, repaired_content) {
        let _ = crate::katastar::register_repair_attempt(
            "compile",
            "Legacy healing rule extraction",
            file_path,
            "ai",
            "search_replace",
            Some(patch),
            "Atomic code fix extracted for Katastar verification",
            model,
        );
    }
}

#[derive(Debug, Clone)]
pub struct TscDiagnostic {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub code: String,
    pub message: String,
}

pub fn parse_tsc_diagnostics(tsc_output: &str) -> Vec<TscDiagnostic> {
    let mut diagnostics = Vec::new();
    let Ok(re) = regex::Regex::new(r#"^([^\(\r\n]+)\((\d+),(\d+)\):\s*(?:error|warning)\s+(TS\d+):\s*(.+)$"#) else {
        return diagnostics;
    };
    for line in tsc_output.lines() {
        let t = line.trim();
        if let Some(cap) = re.captures(t) {
            let raw_file = cap.get(1).map(|m| m.as_str()).unwrap_or("").replace('\\', "/");
            let file = raw_file.trim_start_matches("./").to_string();
            let line_num = cap.get(2).and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(1);
            let col_num = cap.get(3).and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(1);
            let code = cap.get(4).map(|m| m.as_str()).unwrap_or("").to_string();
            let message = cap.get(5).map(|m| m.as_str()).unwrap_or("").trim().to_string();
            if !file.is_empty() {
                diagnostics.push(TscDiagnostic {
                    file,
                    line: line_num,
                    col: col_num,
                    code,
                    message,
                });
            }
        }
    }
    diagnostics
}

pub fn file_dependency_layer_priority(rel_path: &str) -> u8 {
    let lower = rel_path.replace('\\', "/").to_lowercase();
    let is_app_route = crate::sanitizer::is_app_router_route_file(rel_path);
    let is_api_path = lower.contains("/api/") || lower.starts_with("api/");
    if lower.contains("schema.prisma") || lower.starts_with("prisma/") {
        0
    } else if !is_app_route && !is_api_path && (lower.contains("/models/") || lower.starts_with("models/") || lower.contains("/types/") || lower.starts_with("types/")) {
        1
    } else if !is_app_route && (lower.contains("/lib/") || lower.starts_with("lib/") || lower.contains("/utils/") || lower.starts_with("utils/") || lower.contains("/config/") || lower.contains("/services/")) {
        2
    } else if lower.contains("/components/ui") {
        3
    } else if !is_app_route && !is_api_path && (lower.contains("/hooks/") || lower.starts_with("hooks/") || lower.contains("/context") || lower.contains("/providers")) {
        4
    } else if is_api_path || lower.ends_with("route.ts") || lower.ends_with("route.js") || lower.ends_with("middleware.ts") {
        5
    } else if !is_app_route && (lower.contains("/components/") || lower.starts_with("components/")) {
        6
    } else if is_app_route || lower.contains("/app/") || lower.starts_with("app/") {
        7
    } else {
        8
    }
}

pub fn parse_batch_file_fixes_response(raw: &str, expected_paths: &[String]) -> Vec<(String, String)> {
    let clean = extract_code_content(raw);
    let trimmed = clean.trim();

    let extract_item = |item: &Value| -> Option<(String, String)> {
        let p = item
            .get("path")
            .or_else(|| item.get("file"))
            .or_else(|| item.get("filePath"))
            .or_else(|| item.get("file_path"))
            .or_else(|| item.get("filename"))
            .and_then(|x| x.as_str())?;
        let norm_p = p.trim().replace('\\', "/").trim_start_matches("./").to_string();
        let c = item
            .get("content")
            .or_else(|| item.get("code"))
            .or_else(|| item.get("source"))
            .or_else(|| item.get("replacement"))
            .and_then(|x| x.as_str())?
            .to_string();
        if norm_p.is_empty() || c.trim().is_empty() {
            None
        } else {
            Some((norm_p, c))
        }
    };

    let parsed_val: Option<Value> = serde_json::from_str::<Value>(trimmed).ok().or_else(|| {
        if let (Some(s), Some(e)) = (trimmed.find('{'), trimmed.rfind('}')) {
            if e > s {
                if let Ok(v) = serde_json::from_str::<Value>(&trimmed[s..=e]) {
                    return Some(v);
                }
            }
        }
        if let (Some(s), Some(e)) = (trimmed.find('['), trimmed.rfind(']')) {
            if e > s {
                if let Ok(v) = serde_json::from_str::<Value>(&trimmed[s..=e]) {
                    return Some(v);
                }
            }
        }
        None
    });

    if let Some(val) = parsed_val {
        match &val {
            Value::Array(arr) => {
                let items: Vec<(String, String)> = arr.iter().filter_map(extract_item).collect();
                if !items.is_empty() {
                    return items;
                }
            }
            Value::Object(map) => {
                for key in ["files", "edits", "fixes", "changes", "updates"] {
                    if let Some(Value::Array(arr)) = map.get(key) {
                        let items: Vec<(String, String)> = arr.iter().filter_map(extract_item).collect();
                        if !items.is_empty() {
                            return items;
                        }
                    }
                }
                if let Some(single) = extract_item(&val) {
                    return vec![single];
                }
                let mut by_path = Vec::new();
                for (k, v) in map {
                    let nk = k.trim().replace('\\', "/").trim_start_matches("./").to_string();
                    if crate::healer::is_valid_file_path(&nk) {
                        if let Some(code_str) = v.as_str() {
                            if !code_str.trim().is_empty() {
                                by_path.push((nk, code_str.to_string()));
                            }
                        }
                    }
                }
                if !by_path.is_empty() {
                    return by_path;
                }
            }
            _ => {}
        }
    }

    if expected_paths.len() == 1 && crate::healer::looks_like_code_payload(trimmed) {
        let p = expected_paths[0].trim().replace('\\', "/").trim_start_matches("./").to_string();
        if !p.is_empty() {
            return vec![(p, clean)];
        }
    }

    Vec::new()
}

fn restore_pending_file_snapshot(proj_dir: &std::path::Path, rel_path: &str, before: &str) {
    let target = proj_dir.join(rel_path);
    if before.is_empty() {
        let _ = std::fs::remove_file(&target);
    } else {
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&target, before);
    }
}

pub fn run_hygiene_pass_on_project(proj_dir: &std::path::Path) {
    crate::sanitizer::ensure_valid_tsconfig(proj_dir);
    crate::packages::repair_package_json_dependencies(proj_dir);

    fn walk_hygiene(dir: &std::path::Path, proj_dir: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
                    if n != "node_modules" && n != ".next" && n != ".git" && n != ".axiom" && n != "prisma" {
                        walk_hygiene(&p, proj_dir);
                    }
                } else if p.is_file() {
                    let rel = p.strip_prefix(proj_dir).ok().map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                    let rel_lower = rel.to_lowercase();
                    if rel_lower.ends_with(".css") || rel_lower.ends_with(".scss") {
                        if let Ok(c) = std::fs::read_to_string(&p) {
                            let deduped = crate::sanitizer::deduplicate_repeated_css_blocks(&c);
                            if deduped != c {
                                let _ = std::fs::write(&p, deduped);
                            }
                        }
                    } else if rel_lower.ends_with(".ts") || rel_lower.ends_with(".tsx") || rel_lower.ends_with(".js") || rel_lower.ends_with(".jsx") {
                        if let Ok(c) = std::fs::read_to_string(&p) {
                            let mut next = crate::sanitizer::normalize_bare_local_imports(&c);
                            next = crate::sanitizer::ensure_next_server_imports(&next, &rel_lower);
                            if next.contains("react-icons") || next.contains("-icons-react") {
                                next = crate::sanitizer::sanitize_react_icons_imports(&next);
                            }
                            next = crate::sanitizer::sanitize_next_link_named_import(&next);
                            next = crate::sanitizer::sanitize_nextauth_imports(&next);
                            if rel_lower.ends_with(".tsx") || rel_lower.ends_with(".jsx") {
                                let is_ui_mod = rel_lower.contains("/components/ui") || rel_lower.ends_with("/ui.tsx") || rel_lower.ends_with("/ui.jsx");
                                next = crate::sanitizer::repair_bare_react_fc_signatures(&next, is_ui_mod);
                            }
                            next = crate::sanitizer::sanitize_optional_user_password_access(&next);
                            next = crate::sanitizer::ensure_hook_type_exports(&next, &rel);
                            next = crate::sanitizer::sanitize_root_layout_missing_children(&next, &rel, true);
                            next = reconcile_external_icon_imports_in_content(proj_dir, &next);
                            next = crate::sanitizer::reconcile_server_prisma_enum_usage(proj_dir, &rel, &next);
                            if crate::sanitizer::is_app_router_route_file(&rel) {
                                next = crate::sanitizer::sanitize_app_router_route_exports(&next, &rel);
                            }
                            if next != c {
                                let _ = std::fs::write(&p, next);
                            }
                        }
                    }
                }
            }
        }
    }

    if proj_dir.join("src").exists() {
        walk_hygiene(&proj_dir.join("src"), proj_dir);
    } else {
        for dir_name in ["app", "components", "hooks", "lib", "utils", "models", "types", "services", "styles"] {
            let d = proj_dir.join(dir_name);
            if d.is_dir() {
                walk_hygiene(&d, proj_dir);
            }
        }
    }
    for root_candidate in ["middleware.ts", "middleware.js"] {
        let p = proj_dir.join(root_candidate);
        if p.is_file() {
            if let Ok(c) = std::fs::read_to_string(&p) {
                let mut next = crate::sanitizer::normalize_bare_local_imports(&c);
                next = crate::sanitizer::ensure_next_server_imports(&next, root_candidate);
                if next != c {
                    let _ = std::fs::write(&p, next);
                }
            }
        }
    }

    crate::apply_ast_sanity_pass_to_project(proj_dir);
    crate::sanitizer::ensure_ui_primitives_reconciled(proj_dir);
    crate::sanitizer::ensure_directory_barrel_files(proj_dir);
    ensure_nextauth_env_vars(proj_dir);
    let _ = auto_import_missing_symbols_in_project(proj_dir);
}

pub fn extract_prisma_schema_summary_for_healing(proj_dir: &std::path::Path) -> String {
    let schema_path = [
        proj_dir.join("prisma/schema.prisma"),
        proj_dir.join("schema.prisma"),
    ]
    .into_iter()
    .find(|p| p.is_file());
    let Some(sp) = schema_path else {
        return String::new();
    };
    let Ok(src) = std::fs::read_to_string(&sp) else {
        return String::new();
    };
    let contract = crate::orchestrator::extract_prisma_schema_contract(&src);
    let mut out = String::new();
    if let Some(enums_obj) = contract.get("enums").and_then(|e| e.as_object()) {
        if !enums_obj.is_empty() {
            out.push_str("Prisma Enums (type/value enums ONLY — NO `prisma.<enum>` delegate exists on PrismaClient! Never write `prisma.subscriptionPlan` etc.; import enums from '@prisma/client' for typing or z.nativeEnum):\n");
            for (ename, evals) in enums_obj {
                let vals: Vec<&str> = evals
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                if !vals.is_empty() {
                    out.push_str(&format!("- enum {}: {}\n", ename, vals.join(" | ")));
                }
            }
        }
    }
    if let Some(models_obj) = contract.get("models").and_then(|m| m.as_object()) {
        if !models_obj.is_empty() {
            out.push_str("Prisma Models (use ONLY declared delegate names and fields below):\n");
            for (mname, mval) in models_obj {
                let delegate = mval.get("delegate").and_then(|v| v.as_str()).unwrap_or("");
                let typed: Vec<&str> = mval
                    .get("typedFields")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                let req: Vec<&str> = mval
                    .get("requiredFields")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                let rels: Vec<&str> = mval
                    .get("relations")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- {} (prisma.{}) | fields: [{}]{}{}\n",
                    mname,
                    delegate,
                    typed.join(", "),
                    if req.is_empty() { String::new() } else { format!(" | required: [{}]", req.join(", ")) },
                    if rels.is_empty() { String::new() } else { format!(" | relations: [{}]", rels.join(", ")) },
                ));
            }
        }
    }
    if out.is_empty() {
        return String::new();
    }
    let capped: String = out.chars().take(2600).collect();
    format!(
        "\n--- ACTUAL PRISMA SCHEMA CONTRACT (prisma/schema.prisma) ---\n{}\n------------------------------------------------------------\n",
        capped.trim_end()
    )
}

pub const ZOD_PRISMA_TS2322_HEAL_GUIDE: &str = "\nTS2322 / ZOD-VS-PRISMA HEALING GUIDE (tsconfig uses \"strict\": false):\n\
- Under \"strict\": false, `z.infer<typeof schema>` makes properties optional (`{ field?: T }`), so `prisma.<model>.create({ data: validatedBody })` fails with TS2322 on required fields!\n\
- NEVER pass `validatedBody` directly into `.create({ data: validatedBody })`: build an EXPLICIT typed `data: { requiredField: parsed.requiredField, ... }` object (guaranteeing every required field is defined via guard/default).\n\
- For Prisma Enum fields: import the Enum from '@prisma/client' and use `z.nativeEnum(EnumName)` (or cast `parsed.status as EnumName`), NEVER bare `z.string()`.\n\
- For DateTime fields: use `z.coerce.date()` or `new Date(parsed.date)`.\n\
- Use ONLY exact delegates, field names, and relation names from ACTUAL PRISMA SCHEMA CONTRACT.\n";

fn collect_exporter_context_for_file(proj_dir: &std::path::Path, rel_path: &str, content: &str) -> String {
    let mut ctx = String::new();
    let Ok(imp_re) = regex::Regex::new(r#"(?:import\s+[^;]+\s+from\s+|require\s*\(\s*)['"](@/[^'"]+|\.\.?/[^'"]+)['"]"#) else {
        return ctx;
    };
    let mut visited = std::collections::HashSet::new();
    let file_parent = proj_dir.join(rel_path).parent().map(|p| p.to_path_buf());
    let alias_base = if proj_dir.join("src").is_dir() {
        proj_dir.join("src")
    } else {
        proj_dir.to_path_buf()
    };

    for cap in imp_re.captures_iter(content) {
        let spec = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let raw = if let Some(sub) = spec.strip_prefix("@/") {
            alias_base.join(sub)
        } else if let Some(ref par) = file_parent {
            par.join(spec)
        } else {
            continue;
        };
        let norm = normalize_path_components(&raw);
        let candidates = [
            norm.clone(),
            norm.with_extension("ts"),
            norm.with_extension("tsx"),
            norm.join("index.ts"),
            norm.join("index.tsx"),
        ];
        for cand in candidates {
            if cand.is_file() {
                if let Ok(rel) = cand.strip_prefix(proj_dir) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if visited.insert(rel_str.clone()) {
                        if let Ok(exp_content) = std::fs::read_to_string(&cand) {
                            let snippet: String = exp_content.chars().take(1600).collect();
                            ctx.push_str(&format!("\nImported module `{}`:\n```ts\n{}\n```\n", rel_str, snippet));
                        }
                    }
                }
                break;
            }
        }
        if visited.len() >= 3 {
            break;
        }
    }
    ctx
}

fn strip_dts_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut in_block = false;
    while i < chars.len() {
        if in_block {
            if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                in_block = false;
                i += 2;
            } else {
                if chars[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            in_block = true;
            i += 2;
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn split_pkg_specifier(spec: &str) -> (String, String) {
    let clean = spec.split(['?', '#']).next().unwrap_or(spec).trim();
    if let Some(rest) = clean.strip_prefix('@') {
        let mut parts = rest.splitn(3, '/');
        let scope = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        let sub = parts.next().unwrap_or("");
        if scope.is_empty() || name.is_empty() {
            (clean.to_string(), String::new())
        } else {
            (format!("@{}/{}", scope, name), sub.to_string())
        }
    } else {
        let mut parts = clean.splitn(2, '/');
        let name = parts.next().unwrap_or("");
        let sub = parts.next().unwrap_or("");
        (name.to_string(), sub.to_string())
    }
}

fn extract_types_path_from_export_val(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => {
            if s.ends_with(".d.ts") || s.ends_with(".d.mts") || s.ends_with(".d.cts") {
                Some(s.clone())
            } else {
                None
            }
        }
        Value::Object(map) => {
            if let Some(t) = map.get("types").and_then(|v| extract_types_path_from_export_val(v)) {
                return Some(t);
            }
            for key in ["import", "default", "require", "node"] {
                if let Some(sub) = map.get(key) {
                    if let Some(t) = extract_types_path_from_export_val(sub) {
                        return Some(t);
                    }
                }
            }
            None
        }
        Value::Array(arr) => arr.iter().find_map(extract_types_path_from_export_val),
        _ => None,
    }
}

fn resolve_dts_entry_in_dir(base_dir: &std::path::Path, rel: &str) -> Option<std::path::PathBuf> {
    let clean = rel.trim().trim_start_matches("./");
    if clean.is_empty() {
        return None;
    }
    let candidate = normalize_path_components(&base_dir.join(clean));
    if !candidate.starts_with(base_dir) {
        return None;
    }
    for cand in [
        candidate.clone(),
        std::path::PathBuf::from(format!("{}.d.ts", candidate.to_string_lossy())),
        std::path::PathBuf::from(format!("{}.d.mts", candidate.to_string_lossy())),
        candidate.join("index.d.ts"),
        candidate.join("index.d.mts"),
    ] {
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

pub fn resolve_external_dts_path(proj_dir: &std::path::Path, spec: &str) -> Option<std::path::PathBuf> {
    let nm_dir = proj_dir.join("node_modules");
    if !nm_dir.is_dir() {
        return None;
    }
    let (pkg_root, subpath) = split_pkg_specifier(spec);
    if pkg_root.is_empty() {
        return None;
    }
    let pkg_dir = nm_dir.join(&pkg_root);
    let types_pkg_name = if let Some(stripped) = pkg_root.strip_prefix('@') {
        stripped.replace('/', "__")
    } else {
        pkg_root.clone()
    };
    let at_types_dir = nm_dir.join("@types").join(&types_pkg_name);

    // 1. Check package.json in node_modules/<pkg>
    let pkg_json_path = pkg_dir.join("package.json");
    if pkg_json_path.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&pkg_json_path) {
            if let Ok(pkg_val) = serde_json::from_str::<Value>(&raw) {
                let export_key = if subpath.is_empty() {
                    ".".to_string()
                } else {
                    format!("./{}", subpath)
                };
                if let Some(exports_obj) = pkg_val.get("exports").and_then(|e| e.as_object()) {
                    if let Some(target_val) = exports_obj.get(&export_key) {
                        if let Some(types_rel) = target_val
                            .get("types")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                            .or_else(|| extract_types_path_from_export_val(target_val))
                        {
                            if let Some(p) = resolve_dts_entry_in_dir(&pkg_dir, &types_rel) {
                                return Some(p);
                            }
                        }
                    } else if subpath.is_empty()
                        && (exports_obj.contains_key("types")
                            || exports_obj.contains_key("import")
                            || exports_obj.contains_key("default")
                            || exports_obj.contains_key("require")
                            || exports_obj.contains_key("node"))
                    {
                        // Conditional root exports without `"."` key (e.g. `stripe@23.1.0`: `{ "import": {...}, "default": {...} }`)
                        if let Some(types_rel) = extract_types_path_from_export_val(&Value::Object(exports_obj.clone())) {
                            if let Some(p) = resolve_dts_entry_in_dir(&pkg_dir, &types_rel) {
                                return Some(p);
                            }
                        }
                    }
                    // Wildcard export fallback (e.g. "./*": { "types": "./dist/*.d.ts" })
                    if !subpath.is_empty() {
                        for (pat, val) in exports_obj {
                            if let Some((prefix, suffix)) = pat.split_once('*') {
                                let clean_prefix = prefix.trim_start_matches("./");
                                if subpath.starts_with(clean_prefix) && subpath.ends_with(suffix) {
                                    let wildcard_mid = &subpath[clean_prefix.len()..subpath.len() - suffix.len()];
                                    if let Some(tmpl) = val
                                        .get("types")
                                        .and_then(|t| t.as_str())
                                        .map(|s| s.to_string())
                                        .or_else(|| extract_types_path_from_export_val(val))
                                    {
                                        let concrete = tmpl.replace('*', wildcard_mid);
                                        if let Some(p) = resolve_dts_entry_in_dir(&pkg_dir, &concrete) {
                                            return Some(p);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if subpath.is_empty() {
                    for field in ["types", "typings"] {
                        if let Some(rel) = pkg_val.get(field).and_then(|v| v.as_str()) {
                            if let Some(p) = resolve_dts_entry_in_dir(&pkg_dir, rel) {
                                return Some(p);
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Direct file/index fallbacks in node_modules/<pkg>
    if pkg_dir.is_dir() {
        if subpath.is_empty() {
            for rel in ["index.d.ts", "index.d.mts", "dist/index.d.ts", "lib/index.d.ts", "types/index.d.ts"] {
                if let Some(p) = resolve_dts_entry_in_dir(&pkg_dir, rel) {
                    return Some(p);
                }
            }
        } else {
            for rel in [
                subpath.clone(),
                format!("dist/{}", subpath),
                format!("lib/{}", subpath),
                format!("types/{}", subpath),
            ] {
                if let Some(p) = resolve_dts_entry_in_dir(&pkg_dir, &rel) {
                    return Some(p);
                }
            }
        }
    }

    // 3. Fallback to node_modules/@types/<pkg>
    if at_types_dir.is_dir() {
        if subpath.is_empty() {
            return resolve_dts_entry_in_dir(&at_types_dir, "index.d.ts");
        } else {
            return resolve_dts_entry_in_dir(&at_types_dir, &subpath);
        }
    }

    None
}

struct DtsParseResult {
    symbols: Vec<String>,
    has_default: bool,
    incomplete: bool,
}

fn collect_dts_exports_single_file(content: &str, out_syms: &mut Vec<String>, seen: &mut std::collections::HashSet<String>, has_default: &mut bool) -> (Vec<String>, bool) {
    let cleaned = strip_dts_comments(content);
    let mut wildcard_targets: Vec<String> = Vec::new();
    let mut external_or_unparseable_wildcard = false;

    let Ok(decl_re) = regex::Regex::new(r#"(?m)(?:^|;)\s*export\s+(?:declare\s+)?(?:abstract\s+)?(?:async\s+)?(?:function\*?|const|let|var|class|enum|interface|type|namespace)\s+([A-Za-z_$][A-Za-z0-9_$]*)"#) else {
        return (wildcard_targets, true);
    };
    for cap in decl_re.captures_iter(&cleaned) {
        if let Some(m) = cap.get(1) {
            let sym = m.as_str().to_string();
            if sym != "default" && seen.insert(sym.clone()) {
                out_syms.push(sym);
            }
        }
    }

    let Ok(default_re) = regex::Regex::new(r#"(?m)(?:^|;)\s*export\s+(?:default\b|=\s*)"#) else {
        return (wildcard_targets, true);
    };
    if default_re.is_match(&cleaned) {
        *has_default = true;
    }

    let Ok(braced_re) = regex::Regex::new(r#"(?ms)(?:^|;)\s*export\s+(?:type\s+)?\{([^}]+)\}"#) else {
        return (wildcard_targets, true);
    };
    for cap in braced_re.captures_iter(&cleaned) {
        let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        for raw_item in inner.split(',') {
            let item = raw_item.trim();
            if item.is_empty() {
                continue;
            }
            let item = item.strip_prefix("type ").map(|s| s.trim()).unwrap_or(item);
            let exported_name = if let Some((_, rhs)) = item.split_once(" as ") {
                rhs.trim()
            } else {
                item
            };
            if exported_name == "default" {
                *has_default = true;
            } else if !exported_name.is_empty()
                && exported_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                && seen.insert(exported_name.to_string())
            {
                out_syms.push(exported_name.to_string());
            }
        }
    }

    let Ok(ns_star_re) = regex::Regex::new(r#"(?m)(?:^|;)\s*export\s+\*\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*)\s+from\s+['"][^'"]+['"]"#) else {
        return (wildcard_targets, true);
    };
    for cap in ns_star_re.captures_iter(&cleaned) {
        if let Some(m) = cap.get(1) {
            let sym = m.as_str().to_string();
            if seen.insert(sym.clone()) {
                out_syms.push(sym);
            }
        }
    }

    let Ok(star_re) = regex::Regex::new(r#"(?m)(?:^|;)\s*export\s+(?:type\s+)?\*\s+from\s+['"]([^'"]+)['"]"#) else {
        return (wildcard_targets, true);
    };
    for cap in star_re.captures_iter(&cleaned) {
        if let Some(m) = cap.get(1) {
            let target = m.as_str().trim();
            if target.starts_with('.') {
                wildcard_targets.push(target.to_string());
            } else {
                wildcard_targets.push(format!("pkg:{}", target));
                external_or_unparseable_wildcard = true;
            }
        }
    }

    (wildcard_targets, external_or_unparseable_wildcard)
}

fn parse_dts_exports_with_guard(entry_path: &std::path::Path) -> DtsParseResult {
    let mut symbols = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut has_default = false;

    let Ok(root_content) = std::fs::read_to_string(entry_path) else {
        return DtsParseResult { symbols, has_default, incomplete: true };
    };

    let (wildcards, ext_bad) = collect_dts_exports_single_file(&root_content, &mut symbols, &mut seen, &mut has_default);
    if ext_bad || wildcards.len() > 15 {
        return DtsParseResult { symbols, has_default, incomplete: true };
    }

    let parent_dir = entry_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    for rel_target in wildcards {
        let joined = normalize_path_components(&parent_dir.join(&rel_target));
        let resolved_sub = [
            joined.clone(),
            joined.with_extension("d.ts"),
            joined.with_extension("d.mts"),
            std::path::PathBuf::from(format!("{}.d.ts", joined.to_string_lossy())),
            joined.join("index.d.ts"),
            joined.join("index.d.mts"),
        ]
        .into_iter()
        .find(|p| p.is_file());

        let Some(sub_path) = resolved_sub else {
            return DtsParseResult { symbols, has_default, incomplete: true };
        };
        let Ok(sub_content) = std::fs::read_to_string(&sub_path) else {
            return DtsParseResult { symbols, has_default, incomplete: true };
        };
        let (nested_wildcards, nested_ext_bad) = collect_dts_exports_single_file(&sub_content, &mut symbols, &mut seen, &mut has_default);
        // Depth > 1 wildcard re-exports cannot be safely closed without deep traversal: bail out cleanly
        if nested_ext_bad || !nested_wildcards.is_empty() {
            return DtsParseResult { symbols, has_default, incomplete: true };
        }
    }

    DtsParseResult {
        symbols,
        has_default,
        incomplete: false,
    }
}

/// Queries `.d.ts` files on disk (including deep wildcard re-exports) to verify if package `spec`
/// exports a concrete symbol `sym`. Unlike prompt-listing, a single-symbol boolean query never
/// feeds partial symbol dumps to the LLM, so it safely traverses up to depth 3 / 90 files.
pub fn dts_package_exports_symbol(proj_dir: &std::path::Path, spec: &str, sym: &str) -> bool {
    if sym.trim().is_empty() {
        return false;
    }
    let Some(entry) = resolve_external_dts_path(proj_dir, spec) else {
        return false;
    };

    const MAX_FILES: usize = 90;
    const MAX_DEPTH: usize = 3;
    let mut visited: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<(std::path::PathBuf, usize)> = std::collections::VecDeque::new();
    queue.push_back((entry, 0));

    while let Some((cur_path, depth)) = queue.pop_front() {
        if visited.len() >= MAX_FILES || !visited.insert(cur_path.clone()) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&cur_path) else {
            continue;
        };
        let mut syms = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut has_default = false;
        let (wildcards, _) = collect_dts_exports_single_file(&content, &mut syms, &mut seen, &mut has_default);
        if seen.contains(sym) {
            return true;
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        let parent_dir = cur_path.parent().unwrap_or_else(|| std::path::Path::new("."));
        for target in wildcards {
            if let Some(ext_spec) = target.strip_prefix("pkg:") {
                if let Some(ext_path) = resolve_external_dts_path(proj_dir, ext_spec) {
                    if !visited.contains(&ext_path) {
                        queue.push_back((ext_path, depth + 1));
                    }
                }
            } else {
                let joined = normalize_path_components(&parent_dir.join(&target));
                if let Some(sub_path) = [
                    joined.clone(),
                    joined.with_extension("d.ts"),
                    joined.with_extension("d.mts"),
                    std::path::PathBuf::from(format!("{}.d.ts", joined.to_string_lossy())),
                    joined.join("index.d.ts"),
                    joined.join("index.d.mts"),
                ]
                .into_iter()
                .find(|p| p.is_file())
                {
                    if !visited.contains(&sub_path) {
                        queue.push_back((sub_path, depth + 1));
                    }
                }
            }
        }
    }

    false
}

fn collect_subpath_candidates_in_pkg(
    pkg_dir: &std::path::Path,
    pkg_name: &str,
    sym_lower: &str,
    out: &mut Vec<String>,
) {
    let mut sub_specs: Vec<(bool, usize, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(pkg_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || matches!(
                name.as_str(),
                "node_modules" | "lib" | "dist" | "cjs" | "esm" | "umd" | "build" | "src" | "types"
            )
        {
            continue;
        }
        if p.is_dir() {
            if p.join("index.d.ts").is_file() || p.join("index.d.mts").is_file() {
                let spec = format!("{}/{}", pkg_name, name);
                let base = name.trim_end_matches(|c: char| c.is_ascii_digit()).to_lowercase();
                let exact = !base.is_empty() && sym_lower.starts_with(&base);
                sub_specs.push((exact, base.len(), spec));
            }
            // Also check depth-2 subdirs (e.g. `@heroicons/react/24/outline` or scoped icon sets)
            if let Ok(sub_entries) = std::fs::read_dir(&p) {
                for sub_ent in sub_entries.flatten() {
                    let sp = sub_ent.path();
                    if sp.is_dir() {
                        let sname = sub_ent.file_name().to_string_lossy().to_string();
                        if !sname.starts_with('.')
                            && sname != "node_modules"
                            && (sp.join("index.d.ts").is_file() || sp.join("index.d.mts").is_file())
                        {
                            let spec = format!("{}/{}/{}", pkg_name, name, sname);
                            let base = sname.trim_end_matches(|c: char| c.is_ascii_digit()).to_lowercase();
                            let exact = !base.is_empty() && sym_lower.starts_with(&base);
                            sub_specs.push((exact, base.len(), spec));
                        }
                    }
                }
            }
        }
    }
    sub_specs.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.len().cmp(&b.2.len()))
            .then_with(|| a.2.cmp(&b.2))
    });
    for (_, _, spec) in sub_specs {
        if !out.contains(&spec) {
            out.push(spec);
        }
    }
}

pub fn find_installed_dts_package_for_symbol(
    proj_dir: &std::path::Path,
    preferred_specs: &[String],
    sym: &str,
) -> Option<String> {
    if sym.trim().is_empty() || !proj_dir.join("node_modules").is_dir() {
        return None;
    }

    // 1. Check packages already imported by the file first
    for spec in preferred_specs {
        if !spec.starts_with('.') && !spec.starts_with("@/") && !spec.starts_with("~/") {
            if dts_package_exports_symbol(proj_dir, spec, sym) {
                return Some(spec.clone());
            }
        }
    }

    // 2. Discover candidate packages & subpaths dynamically from preferred roots, package.json, and node_modules
    let nm_dir = proj_dir.join("node_modules");
    let sym_lower = sym.to_lowercase();
    let mut candidates: Vec<String> = Vec::new();
    let mut pkg_roots: Vec<String> = Vec::new();

    for spec in preferred_specs {
        if !spec.starts_with('.') && !spec.starts_with("@/") && !spec.starts_with("~/") {
            let (root, _) = split_pkg_specifier(spec);
            if !root.is_empty() && !pkg_roots.contains(&root) {
                pkg_roots.push(root);
            }
        }
    }

    let pkg_json = proj_dir.join("package.json");
    if pkg_json.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&pkg_json) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(deps) = val.get("dependencies").and_then(|d| d.as_object()) {
                    for dep in deps.keys() {
                        let dl = dep.to_lowercase();
                        if matches!(
                            dl.as_str(),
                            "next" | "react" | "react-dom" | "prisma" | "@prisma/client" | "typescript" | "tailwindcss" | "autoprefixer" | "postcss" | "eslint" | "zod" | "bcryptjs"
                        ) {
                            continue;
                        }
                        if !pkg_roots.contains(dep) {
                            pkg_roots.push(dep.clone());
                        }
                    }
                }
            }
        }
    }

    let is_tooling_pkg = |name: &str| -> bool {
        let dl = name.to_lowercase();
        matches!(
            dl.as_str(),
            "next" | "react" | "react-dom" | "prisma" | "@prisma/client" | "typescript"
                | "tailwindcss" | "autoprefixer" | "postcss" | "eslint" | "prettier"
                | "zod" | "bcryptjs"
        ) || dl.starts_with("@types/")
            || dl.starts_with("@babel/")
            || dl.starts_with("@eslint")
            || dl.starts_with("@next/")
            || dl.starts_with("@prisma/")
            || dl.starts_with("@swc/")
            || dl.starts_with("eslint")
    };

    // Expand each prioritized root (both root entry and installed subpaths)
    for root in &pkg_roots {
        if is_tooling_pkg(root) {
            continue;
        }
        let p = nm_dir.join(root);
        if p.is_dir() {
            if !candidates.contains(root) {
                candidates.push(root.clone());
            }
            collect_subpath_candidates_in_pkg(&p, root, &sym_lower, &mut candidates);
        }
    }

    // Fallback discovery across top-level installed packages in node_modules/ (handles packages omitted from package.json)
    if let Ok(nm_entries) = std::fs::read_dir(&nm_dir) {
        let mut fallback_roots: Vec<String> = Vec::new();
        for ent in nm_entries.flatten() {
            let p = ent.path();
            if !p.is_dir() {
                continue;
            }
            let name = ent.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if name.starts_with('@') {
                if name == "@types" || name == "@babel" || name == "@next" || name == "@prisma" || name == "@swc" || name.starts_with("@eslint") {
                    continue;
                }
                if let Ok(scoped_entries) = std::fs::read_dir(&p) {
                    for sc in scoped_entries.flatten() {
                        if sc.path().is_dir() {
                            let sname = format!("{}/{}", name, sc.file_name().to_string_lossy());
                            if !is_tooling_pkg(&sname) && !pkg_roots.contains(&sname) {
                                fallback_roots.push(sname);
                            }
                        }
                    }
                }
            } else if !is_tooling_pkg(&name) && !pkg_roots.contains(&name) {
                fallback_roots.push(name);
            }
        }
        fallback_roots.sort();
        for root in fallback_roots.into_iter().take(120) {
            if candidates.len() >= 180 {
                break;
            }
            let p = nm_dir.join(&root);
            if !candidates.contains(&root) {
                candidates.push(root.clone());
            }
            collect_subpath_candidates_in_pkg(&p, &root, &sym_lower, &mut candidates);
        }
    }

    for spec in candidates {
        if preferred_specs.iter().any(|p| p == &spec) {
            continue;
        }
        if dts_package_exports_symbol(proj_dir, &spec, sym) {
            return Some(spec);
        }
    }

    None
}

pub fn extract_missing_ident_candidates_from_error(err_ctx: &str) -> Vec<String> {
    let mut out = Vec::new();
    let patterns = [
        r#"ReferenceError:\s*([A-Za-z_$][A-Za-z0-9_$]*)\s+is not defined"#,
        r#"Cannot find name\s+['"`]([A-Za-z_$][A-Za-z0-9_$]*)['"`]"#,
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\s+is not defined\b"#,
    ];
    for pat in &patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            for cap in re.captures_iter(err_ctx) {
                if let Some(m) = cap.get(1) {
                    let sym = m.as_str().trim().to_string();
                    if !sym.is_empty() && !out.contains(&sym) {
                        out.push(sym);
                    }
                }
            }
        }
    }
    out
}

pub fn collect_declared_or_imported_idents(content: &str) -> std::collections::HashSet<String> {
    let mut known: std::collections::HashSet<String> = [
        "React", "Fragment", "Suspense", "StrictMode", "Component", "PureComponent",
        "Promise", "Error", "TypeError", "SyntaxError", "Array", "Object", "Date",
        "Math", "JSON", "Map", "Set", "WeakMap", "WeakSet", "RegExp", "URL",
        "URLSearchParams", "Headers", "Request", "Response", "FormData", "Blob",
        "File", "Event", "CustomEvent", "Intl", "Symbol", "Number", "String",
        "Boolean", "BigInt", "JSX", "Metadata", "NextResponse", "NextRequest",
        "Window", "Document", "HTMLElement", "HTMLInputElement", "HTMLButtonElement",
        "HTMLFormElement", "HTMLDivElement", "HTMLSelectElement", "HTMLTextAreaElement",
        "Record", "Partial", "Required", "Readonly", "Pick", "Omit", "Exclude", "Extract",
        "ReturnType", "Awaited", "ReadonlyArray",
        "window", "document", "globalThis", "navigator", "location", "history",
        "localStorage", "sessionStorage", "fetch", "console", "process", "Buffer",
        "setTimeout", "clearTimeout", "setInterval", "clearInterval",
        "requestAnimationFrame", "cancelAnimationFrame", "alert", "confirm", "prompt",
        "Infinity", "NaN", "undefined", "null", "true", "false",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect();

    // 1. All imported bindings (supports multiline imports, aliases `Orig as Alias`, default + braced)
    if let Ok(imp_re) = regex::Regex::new(r#"(?ms)\bimport\s+(?:type\s+)?([\s\S]*?)\s+from\s+['"][^'"]+['"]"#) {
        for cap in imp_re.captures_iter(content) {
            let clause = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if let Some(ob) = clause.find('{') {
                let default_part = clause[..ob].trim().trim_end_matches(',').trim();
                if !default_part.is_empty() && !default_part.starts_with('*') {
                    known.insert(default_part.to_string());
                }
                if let Some(cb) = clause.rfind('}') {
                    let inner = &clause[ob + 1..cb];
                    for raw_item in inner.split(',') {
                        let it = raw_item.trim().trim_start_matches("type ").trim();
                        if it.is_empty() {
                            continue;
                        }
                        let local = if let Some((_, alias)) = it.split_once(" as ") {
                            alias.trim()
                        } else {
                            it.split_whitespace().next().unwrap_or("")
                        };
                        if !local.is_empty() {
                            known.insert(local.to_string());
                        }
                    }
                }
            } else if let Some(star_pos) = clause.find("* as ") {
                let alias = clause[star_pos + 5..].trim();
                if !alias.is_empty() {
                    known.insert(alias.to_string());
                }
            } else {
                let def = clause.trim();
                if !def.is_empty() {
                    known.insert(def.to_string());
                }
            }
        }
    }

    // 2. Local declarations (`function`, `const`, `let`, `var`, `class`, `interface`, `type`, `enum`)
    if let Ok(decl_re) = regex::Regex::new(
        r#"(?m)\b(?:function\*?|const|let|var|class|interface|type|enum)\s+([A-Za-z_$][A-Za-z0-9_$]*)\b"#,
    ) {
        for cap in decl_re.captures_iter(content) {
            if let Some(m) = cap.get(1) {
                known.insert(m.as_str().to_string());
            }
        }
    }

    // 3. Destructured variable declarations `const { A, B: C } = ...`
    if let Ok(destr_re) = regex::Regex::new(r#"(?ms)\b(?:const|let|var)\s*\{([^}]+)\}\s*="#) {
        for cap in destr_re.captures_iter(content) {
            let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            for item in inner.split(',') {
                let t = item.trim();
                if t.is_empty() {
                    continue;
                }
                let local = if let Some((_, rhs)) = t.split_once(':') {
                    rhs.split('=').next().unwrap_or("").trim()
                } else {
                    t.split('=').next().unwrap_or("").trim()
                };
                if !local.is_empty() && local.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                    known.insert(local.to_string());
                }
            }
        }
    }

    known
}

pub fn extract_unimported_jsx_components(
    content: &str,
    known: &std::collections::HashSet<String>,
    is_jsx_file: bool,
    extra_missing_idents: &[String],
) -> Vec<String> {
    let mut missing = Vec::new();
    for extra in extra_missing_idents {
        let t = extra.trim();
        if !t.is_empty() && !known.contains(t) && !missing.iter().any(|m| m == t) {
            missing.push(t.to_string());
        }
    }

    if !is_jsx_file {
        return missing;
    }

    let Ok(tag_re) = regex::Regex::new(r"<([A-Z][A-Za-z0-9_]*)(?:\s|/>|>)") else {
        return missing;
    };

    let mut in_block_comment = false;
    let lines: Vec<&str> = content.lines().collect();
    for raw_line in lines {
        let line_trim = raw_line.trim();
        if line_trim.starts_with("//") || line_trim.starts_with("{/*") {
            if line_trim.starts_with("/*") && !line_trim.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        if in_block_comment {
            if line_trim.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if line_trim.starts_with("import ")
            || line_trim.starts_with("type ")
            || line_trim.starts_with("export type ")
            || line_trim.starts_with("interface ")
            || line_trim.starts_with("export interface ")
        {
            continue;
        }

        for cap in tag_re.captures_iter(raw_line) {
            let Some(m) = cap.get(1) else { continue };
            let lt_pos = cap.get(0).map(|x| x.start()).unwrap_or(0);
            let sym = m.as_str();
            if known.contains(sym) || missing.iter().any(|s| s == sym) {
                continue;
            }

            // Skip TypeScript generic arguments: check char immediately before `<`
            let prefix = &raw_line[..lt_pos];
            let prev_ch = prefix.chars().last();
            if matches!(prev_ch, Some(c) if c.is_alphanumeric() || c == '_' || c == '$' || c == ']' || c == ')') {
                continue;
            }
            // Skip type-annotation context `<Generic>` right after `:` or `as` or `extends` or `implements`
            let prefix_trim = prefix.trim_end();
            if prefix_trim.ends_with(':')
                || prefix_trim.ends_with(" as")
                || prefix_trim.ends_with(" extends")
                || prefix_trim.ends_with(" implements")
            {
                // Exception: ternary `cond ? <Comp /> : <Other />` ends with `:` before `<Other />` or `?` before `<Comp />`!
                // Note `prefix_trim.ends_with(':')` in TS could be `x: Record<...>` (already filtered by prev_ch alphanumeric) or `case X: <Comp />` or ternary `: <Comp />`!
                // Wait: in `const x: Promise<Foo>` char before `<` is 'e', already caught above! What about `const x: <T>() => ...`? Handled below via `after` check!
                if !prefix_trim.ends_with("? :") && !content.contains(&format!("</{}>", sym)) {
                    // Check if closing /> or JSX attributes follow
                    let after = &raw_line[m.end()..];
                    if !after.contains("/>") && !after.starts_with('>') {
                        continue;
                    }
                }
            }

            // Skip generic arrow fn `<T>(...)`, `<T, ...>`, `<T extends ...>`, member `<Foo.Bar>`
            let after = &raw_line[m.end()..];
            if after.starts_with('.')
                || after.starts_with('=')
                || after.starts_with(',')
                || after.starts_with(" extends ")
                || after.starts_with(">(")
            {
                continue;
            }

            // Confirm strong JSX signal: self-closing `<Sym .../>`, closing `</Sym>`, or JSX tag `<Sym prop=` or `<Sym>`
            let has_closing_tag = content.contains(&format!("</{}>", sym));
            let has_inline_self_close = after.contains("/>") || after.trim_start().starts_with('>');
            let has_multiline_jsx_open = raw_line[m.end()..].trim().is_empty()
                || after.trim_start().chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '{');
            if has_closing_tag || has_inline_self_close || has_multiline_jsx_open {
                missing.push(sym.to_string());
            }
        }
    }

    missing
}

fn find_unique_local_module_for_symbol(
    proj_dir: &std::path::Path,
    abs_file: &std::path::Path,
    sym: &str,
) -> Option<(String, bool)> {
    let raw_src = proj_dir.join("src");
    let has_src = raw_src.exists();
    let scan_root = if has_src { raw_src.clone() } else { proj_dir.to_path_buf() };

    let default_decl_re = regex::Regex::new(&format!(
        r#"(?m)\bexport\s+default\s+(?:async\s+)?(?:function\s+{}|class\s+{}|{};)"#,
        regex::escape(sym),
        regex::escape(sym),
        regex::escape(sym)
    ))
    .ok();

    let mut matches: Vec<(String, bool)> = Vec::new();
    let mut stack = vec![scan_root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if n != "node_modules" && n != ".next" && n != ".git" && n != ".axiom" && n != "prisma" && n != "api" {
                    stack.push(p);
                }
            } else if p.is_file() && p != abs_file {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "ts" | "tsx" | "js" | "jsx") {
                    continue;
                }
                let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if matches!(fname, "page.tsx" | "page.jsx" | "layout.tsx" | "layout.jsx" | "route.ts" | "route.js") {
                    continue;
                }
                if let Ok(c) = std::fs::read_to_string(&p) {
                    let has_named = crate::sanitizer::has_named_export_symbol(&c, sym);
                    let has_default_only = !has_named
                        && default_decl_re
                            .as_ref()
                            .map(|re| re.is_match(&c))
                            .unwrap_or(false);
                    if has_named || has_default_only {
                        if let Ok(rel) = p.strip_prefix(&scan_root) {
                            let mut rel_str = rel.to_string_lossy().replace('\\', "/");
                            if let Some(dot_idx) = rel_str.rfind('.') {
                                rel_str.truncate(dot_idx);
                            }
                            if let Some(stripped) = rel_str.strip_suffix("/index") {
                                rel_str = stripped.to_string();
                            }
                            if !rel_str.is_empty() {
                                matches.push((format!("@/{}", rel_str), has_default_only));
                                if matches.len() > 1 {
                                    return None;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if matches.len() == 1 {
        matches.into_iter().next()
    } else {
        None
    }
}

fn match_react_icons_prefix(sym: &str) -> Option<(&'static str, &str)> {
    const PREFIXES: &[(&str, &str)] = &[
        ("HiOutline", "react-icons/hi"),
        ("HiMini", "react-icons/hi2"),
        ("IoLogo", "react-icons/io"),
        ("IoIos", "react-icons/io"),
        ("IoMd", "react-icons/io"),
        ("FaReg", "react-icons/fa"),
        ("Hi2", "react-icons/hi2"),
        ("Io5", "react-icons/io5"),
        ("Vsc", "react-icons/vsc"),
        ("Lia", "react-icons/lia"),
        ("Tfi", "react-icons/tfi"),
        ("Hi", "react-icons/hi"),
        ("Fa", "react-icons/fa"),
        ("Fi", "react-icons/fi"),
        ("Md", "react-icons/md"),
        ("Io", "react-icons/io5"),
        ("Ai", "react-icons/ai"),
        ("Bs", "react-icons/bs"),
        ("Ri", "react-icons/ri"),
        ("Bi", "react-icons/bi"),
        ("Tb", "react-icons/tb"),
        ("Lu", "react-icons/lu"),
        ("Ci", "react-icons/ci"),
        ("Si", "react-icons/si"),
        ("Gi", "react-icons/gi"),
        ("Ti", "react-icons/ti"),
        ("Wi", "react-icons/wi"),
        ("Pi", "react-icons/pi"),
        ("Go", "react-icons/go"),
        ("Gr", "react-icons/gr"),
        ("Sl", "react-icons/sl"),
        ("Im", "react-icons/im"),
        ("Cg", "react-icons/cg"),
        ("Rx", "react-icons/rx"),
    ];
    for &(prefix, subpath) in PREFIXES {
        if let Some(rest) = sym.strip_prefix(prefix) {
            if let Some(first_c) = rest.chars().next() {
                if first_c.is_ascii_uppercase() || first_c.is_ascii_digit() {
                    return Some((subpath, rest));
                }
            }
        }
    }
    None
}

fn candidate_react_icon_subpaths(primary: &str) -> Vec<&'static str> {
    match primary {
        "react-icons/hi" => vec!["react-icons/hi", "react-icons/hi2"],
        "react-icons/hi2" => vec!["react-icons/hi2", "react-icons/hi"],
        "react-icons/fa" => vec!["react-icons/fa", "react-icons/fa6"],
        "react-icons/fa6" => vec!["react-icons/fa6", "react-icons/fa"],
        "react-icons/io5" => vec!["react-icons/io5", "react-icons/io"],
        "react-icons/io" => vec!["react-icons/io", "react-icons/io5"],
        "react-icons/fi" => vec!["react-icons/fi"],
        "react-icons/md" => vec!["react-icons/md"],
        "react-icons/ai" => vec!["react-icons/ai"],
        "react-icons/bs" => vec!["react-icons/bs"],
        "react-icons/ri" => vec!["react-icons/ri"],
        "react-icons/bi" => vec!["react-icons/bi"],
        "react-icons/tb" => vec!["react-icons/tb"],
        "react-icons/lu" => vec!["react-icons/lu"],
        "react-icons/ci" => vec!["react-icons/ci"],
        "react-icons/si" => vec!["react-icons/si"],
        "react-icons/gi" => vec!["react-icons/gi"],
        "react-icons/ti" => vec!["react-icons/ti"],
        "react-icons/wi" => vec!["react-icons/wi"],
        "react-icons/pi" => vec!["react-icons/pi"],
        "react-icons/go" => vec!["react-icons/go"],
        "react-icons/gr" => vec!["react-icons/gr"],
        "react-icons/sl" => vec!["react-icons/sl"],
        "react-icons/im" => vec!["react-icons/im"],
        "react-icons/cg" => vec!["react-icons/cg"],
        "react-icons/rx" => vec!["react-icons/rx"],
        "react-icons/vsc" => vec!["react-icons/vsc"],
        "react-icons/lia" => vec!["react-icons/lia"],
        "react-icons/tfi" => vec!["react-icons/tfi"],
        _ => Vec::new(),
    }
}

/// Verifies `lucide-react` and `react-icons/*` named imports against the installed `.d.ts`
/// definitions in `node_modules/` and repairs cross-library hallucinations (e.g.
/// `import { FiServer } from 'lucide-react'` -> `react-icons/fi`, or aliased `Server as FiServer`
/// from `lucide-react` when `react-icons/fi` is not installed).
pub fn reconcile_external_icon_imports_in_content(proj_dir: &std::path::Path, content: &str) -> String {
    if !proj_dir.join("node_modules").is_dir()
        || (!content.contains("lucide-react") && !content.contains("react-icons/"))
    {
        return content.to_string();
    }

    let raw_lines: Vec<&str> = content.lines().collect();
    let mut out_lines: Vec<String> = Vec::with_capacity(raw_lines.len());
    let mut moved_imports: Vec<(String, String)> = Vec::new();
    let mut any_changed = false;
    let mut idx = 0usize;

    while idx < raw_lines.len() {
        let line_trim = raw_lines[idx].trim();
        let is_import_start = (line_trim.starts_with("import ") || line_trim.starts_with("import{"))
            && !line_trim.starts_with("import type ")
            && !line_trim.starts_with("import type{")
            && !line_trim.starts_with("import '")
            && !line_trim.starts_with("import \"");
        if !is_import_start {
            out_lines.push(raw_lines[idx].to_string());
            idx += 1;
            continue;
        }

        let mut end_idx = idx;
        let mut stmt = raw_lines[idx].to_string();
        while !stmt.contains(" from ")
            && !stmt.contains("from'")
            && !stmt.contains("from\"")
            && !stmt.trim_end().ends_with(';')
            && end_idx + 1 < raw_lines.len()
            && end_idx - idx < 60
        {
            end_idx += 1;
            stmt.push('\n');
            stmt.push_str(raw_lines[end_idx]);
        }

        let flat = stmt.replace('\n', " ");
        let flat_trim = flat.trim();
        let spec_opt = crate::sanitizer::extract_import_specifier(flat_trim);
        let Some(spec) = spec_opt else {
            for k in idx..=end_idx {
                out_lines.push(raw_lines[k].to_string());
            }
            idx = end_idx + 1;
            continue;
        };

        let is_icon_spec = spec == "lucide-react" || spec.starts_with("react-icons/");
        if !is_icon_spec || resolve_external_dts_path(proj_dir, &spec).is_none() {
            for k in idx..=end_idx {
                out_lines.push(raw_lines[k].to_string());
            }
            idx = end_idx + 1;
            continue;
        }

        let Some(after_import) = flat_trim.strip_prefix("import").map(|s| s.trim_start()) else {
            for k in idx..=end_idx {
                out_lines.push(raw_lines[k].to_string());
            }
            idx = end_idx + 1;
            continue;
        };
        let (Some(ob), Some(cb)) = (after_import.find('{'), after_import.find('}')) else {
            for k in idx..=end_idx {
                out_lines.push(raw_lines[k].to_string());
            }
            idx = end_idx + 1;
            continue;
        };
        if ob >= cb {
            for k in idx..=end_idx {
                out_lines.push(raw_lines[k].to_string());
            }
            idx = end_idx + 1;
            continue;
        }

        let default_part = after_import[..ob].trim().trim_end_matches(',').trim();
        let inner = &after_import[ob + 1..cb];
        let mut kept_items: Vec<String> = Vec::new();
        let mut stmt_changed = false;

        for raw_item in inner.split(',') {
            let item = raw_item.trim();
            if item.is_empty() {
                continue;
            }
            if item.starts_with("type ") {
                kept_items.push(item.to_string());
                continue;
            }
            let (orig_sym, alias_opt) = if let Some((lhs, rhs)) = item.split_once(" as ") {
                (lhs.trim(), Some(rhs.trim()))
            } else {
                (item, None)
            };
            if orig_sym.is_empty()
                || !orig_sym
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            {
                kept_items.push(item.to_string());
                continue;
            }

            if dts_package_exports_symbol(proj_dir, &spec, orig_sym) {
                kept_items.push(item.to_string());
                continue;
            }

            let prefix_match = match_react_icons_prefix(orig_sym);
            let mut resolved = false;

            if spec == "lucide-react" {
                if let Some((base_sub, rest)) = prefix_match {
                    for cand_sub in candidate_react_icon_subpaths(base_sub) {
                        if dts_package_exports_symbol(proj_dir, cand_sub, orig_sym) {
                            moved_imports.push((cand_sub.to_string(), item.to_string()));
                            stmt_changed = true;
                            resolved = true;
                            break;
                        }
                    }
                    if !resolved && dts_package_exports_symbol(proj_dir, "lucide-react", rest) {
                        let local_name = alias_opt.unwrap_or(orig_sym);
                        let rewritten = if rest == local_name {
                            rest.to_string()
                        } else {
                            format!("{} as {}", rest, local_name)
                        };
                        if !kept_items.iter().any(|k| k == &rewritten) {
                            kept_items.push(rewritten);
                        }
                        stmt_changed = true;
                        resolved = true;
                    }
                }
            } else if spec.starts_with("react-icons/") {
                if let Some((base_sub, rest)) = prefix_match {
                    for cand_sub in candidate_react_icon_subpaths(base_sub) {
                        if cand_sub != spec && dts_package_exports_symbol(proj_dir, cand_sub, orig_sym) {
                            moved_imports.push((cand_sub.to_string(), item.to_string()));
                            stmt_changed = true;
                            resolved = true;
                            break;
                        }
                    }
                    if !resolved && dts_package_exports_symbol(proj_dir, "lucide-react", orig_sym) {
                        moved_imports.push(("lucide-react".to_string(), item.to_string()));
                        stmt_changed = true;
                        resolved = true;
                    }
                    if !resolved && dts_package_exports_symbol(proj_dir, "lucide-react", rest) {
                        let local_name = alias_opt.unwrap_or(orig_sym);
                        let rewritten = if rest == local_name {
                            rest.to_string()
                        } else {
                            format!("{} as {}", rest, local_name)
                        };
                        moved_imports.push(("lucide-react".to_string(), rewritten));
                        stmt_changed = true;
                        resolved = true;
                    }
                } else if dts_package_exports_symbol(proj_dir, "lucide-react", orig_sym) {
                    moved_imports.push(("lucide-react".to_string(), item.to_string()));
                    stmt_changed = true;
                    resolved = true;
                }
            }

            if !resolved {
                kept_items.push(item.to_string());
            }
        }

        if !stmt_changed {
            for k in idx..=end_idx {
                out_lines.push(raw_lines[k].to_string());
            }
            idx = end_idx + 1;
            continue;
        }

        any_changed = true;
        let indent_len = raw_lines[idx].len() - raw_lines[idx].trim_start().len();
        let indent = &raw_lines[idx][..indent_len];
        let quote = if flat_trim.contains(&format!("\"{}\"", spec)) { '"' } else { '\'' };
        let semi = if flat_trim.ends_with(';') { ";" } else { "" };

        if !kept_items.is_empty() && !default_part.is_empty() {
            out_lines.push(format!(
                "{}import {}, {{ {} }} from {}{}{}{}",
                indent,
                default_part,
                kept_items.join(", "),
                quote,
                spec,
                quote,
                semi
            ));
        } else if !kept_items.is_empty() {
            out_lines.push(format!(
                "{}import {{ {} }} from {}{}{}{}",
                indent,
                kept_items.join(", "),
                quote,
                spec,
                quote,
                semi
            ));
        } else if !default_part.is_empty() {
            out_lines.push(format!(
                "{}import {} from {}{}{}{}",
                indent, default_part, quote, spec, quote, semi
            ));
        }
        idx = end_idx + 1;
    }

    if !any_changed && moved_imports.is_empty() {
        return content.to_string();
    }

    let mut result = out_lines.join("\n");
    if content.ends_with('\n') && !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    for (target_spec, sym_item) in moved_imports {
        result = merge_or_insert_import_statement(&result, &target_spec, &sym_item, false);
    }
    result
}

pub fn merge_or_insert_import_statement(
    content: &str,
    module_spec: &str,
    sym: &str,
    is_default: bool,
) -> String {
    if !is_default {
        let esc_mod = regex::escape(module_spec);
        let pat = format!(
            r#"(?m)^([ \t]*import\s+(?:[A-Za-z_$][A-Za-z0-9_$]*\s*,\s*)?\{{)([^}}]*)(\}}\s*from\s*['"]{}['"]\s*;?)"#,
            esc_mod
        );
        if let Ok(re) = regex::Regex::new(&pat) {
            if let Some(cap) = re.captures(content) {
                let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let existing = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim();
                let suffix = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                if !prefix.starts_with("import type") {
                    let already_in = existing
                        .split(',')
                        .any(|s| s.trim() == sym || s.trim().split_whitespace().next() == Some(sym));
                    if already_in {
                        return content.to_string();
                    }
                    let combined = if existing.is_empty() {
                        format!(" {} ", sym)
                    } else {
                        format!(" {}, {} ", existing.trim_end_matches(','), sym)
                    };
                    let replacement = format!("{}{}{}", prefix, combined, suffix);
                    return re.replace(content, replacement.as_str()).to_string();
                }
            }
        }
    }

    let import_line = if is_default {
        format!("import {} from '{}';", sym, module_spec)
    } else {
        format!("import {{ {} }} from '{}';", sym, module_spec)
    };
    let with_imp = crate::sanitizer::insert_import_after_use_client(content, &import_line);
    crate::sanitizer::hoist_use_client_directive(&with_imp)
}

fn reconcile_ui_barrel_hook_imports(
    proj_dir: &std::path::Path,
    abs_file: &std::path::Path,
    content: &str,
) -> String {
    let raw_lines: Vec<&str> = content.lines().collect();
    let mut out_lines: Vec<String> = Vec::with_capacity(raw_lines.len());
    let mut moved_hooks: Vec<(String, String)> = Vec::new();
    let mut changed = false;
    let mut idx = 0usize;

    while idx < raw_lines.len() {
        let trim = raw_lines[idx].trim();
        let is_imp = (trim.starts_with("import ") || trim.starts_with("import{"))
            && !trim.starts_with("import type ")
            && !trim.starts_with("import type{")
            && !trim.starts_with("import '")
            && !trim.starts_with("import \"");
        if !is_imp {
            out_lines.push(raw_lines[idx].to_string());
            idx += 1;
            continue;
        }
        let mut end_idx = idx;
        let mut stmt = raw_lines[idx].to_string();
        while !stmt.contains(" from ")
            && !stmt.contains("from'")
            && !stmt.contains("from\"")
            && !stmt.trim_end().ends_with(';')
            && end_idx + 1 < raw_lines.len()
            && end_idx - idx < 40
        {
            end_idx += 1;
            stmt.push('\n');
            stmt.push_str(raw_lines[end_idx]);
        }
        let flat = stmt.replace('\n', " ");
        let flat_trim = flat.trim();
        let spec_opt = crate::sanitizer::extract_import_specifier(flat_trim);
        let is_ui_barrel = matches!(
            spec_opt.as_deref(),
            Some("@/components/ui" | "@/components" | "~/components/ui" | "~/components")
        );
        if !is_ui_barrel {
            for k in idx..=end_idx {
                out_lines.push(raw_lines[k].to_string());
            }
            idx = end_idx + 1;
            continue;
        }
        let spec = spec_opt.unwrap_or_default();
        if let (Some(ob), Some(cb)) = (flat_trim.find('{'), flat_trim.find('}')) {
            if ob < cb {
                let before_brace = flat_trim[..ob].trim().trim_start_matches("import").trim().trim_end_matches(',').trim();
                let inner = &flat_trim[ob + 1..cb];
                let mut kept = Vec::new();
                let mut stmt_changed = false;
                for raw_item in inner.split(',') {
                    let item = raw_item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    let orig_sym = item.split(" as ").next().unwrap_or(item).trim();
                    let is_hook_ident = orig_sym.starts_with("use")
                        && orig_sym.chars().nth(3).map_or(false, |c| c.is_ascii_uppercase());
                    if is_hook_ident {
                        if let Some((hook_mod, _is_def)) = find_unique_local_module_for_symbol(proj_dir, abs_file, orig_sym) {
                            if hook_mod.contains("/hooks/") {
                                moved_hooks.push((hook_mod, item.to_string()));
                                stmt_changed = true;
                                continue;
                            }
                        }
                    }
                    kept.push(item.to_string());
                }
                if stmt_changed {
                    changed = true;
                    let quote = if flat_trim.contains(&format!("\"{}\"", spec)) { '"' } else { '\'' };
                    let semi = if flat_trim.ends_with(';') { ";" } else { "" };
                    if !kept.is_empty() && !before_brace.is_empty() {
                        out_lines.push(format!("import {}, {{ {} }} from {}{}{}{}", before_brace, kept.join(", "), quote, spec, quote, semi));
                    } else if !kept.is_empty() {
                        out_lines.push(format!("import {{ {} }} from {}{}{}{}", kept.join(", "), quote, spec, quote, semi));
                    } else if !before_brace.is_empty() {
                        out_lines.push(format!("import {} from {}{}{}{}", before_brace, quote, spec, quote, semi));
                    }
                    idx = end_idx + 1;
                    continue;
                }
            }
        }
        for k in idx..=end_idx {
            out_lines.push(raw_lines[k].to_string());
        }
        idx = end_idx + 1;
    }

    if !changed && moved_hooks.is_empty() {
        return content.to_string();
    }
    let mut res = out_lines.join("\n");
    if content.ends_with('\n') && !res.is_empty() && !res.ends_with('\n') {
        res.push('\n');
    }
    for (hook_mod, item) in moved_hooks {
        res = merge_or_insert_import_statement(&res, &hook_mod, &item, false);
    }
    res
}

fn collect_prisma_schema_symbol_names(proj_dir: &std::path::Path) -> (std::collections::HashSet<String>, std::collections::HashSet<String>) {
    let mut models = std::collections::HashSet::new();
    let mut enums = std::collections::HashSet::new();
    let schema_path = [
        proj_dir.join("prisma/schema.prisma"),
        proj_dir.join("schema.prisma"),
    ]
    .into_iter()
    .find(|p| p.is_file());
    let Some(sp) = schema_path else {
        return (models, enums);
    };
    let Ok(src) = std::fs::read_to_string(&sp) else {
        return (models, enums);
    };
    let contract = crate::orchestrator::extract_prisma_schema_contract(&src);
    if let Some(m_obj) = contract.get("models").and_then(|v| v.as_object()) {
        for k in m_obj.keys() {
            models.insert(k.clone());
        }
    }
    if let Some(e_obj) = contract.get("enums").and_then(|v| v.as_object()) {
        for k in e_obj.keys() {
            enums.insert(k.clone());
        }
    }
    (models, enums)
}

fn extract_missing_type_ident_candidates(
    content: &str,
    known: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    let pats = [
        r"(?m)(?::\s*|\bas\s+|\|\s*|&\s*)([A-Z][A-Za-z0-9_]*)(?:\[\])?\b",
        r"(?m)(?:<|,\s*)([A-Z][A-Za-z0-9_]*)\s*\[\s*\]",
        r"(?m)\buseState\s*<\s*([A-Z][A-Za-z0-9_]*)",
        r"(?m)\b([A-Z][A-Za-z0-9_]*)\[\]",
        r"(?m)\bz\.nativeEnum\s*\(\s*([A-Z][A-Za-z0-9_]*)\s*\)",
    ];
    for pat in &pats {
        if let Ok(re) = regex::Regex::new(pat) {
            for cap in re.captures_iter(content) {
                if let Some(m) = cap.get(1) {
                    let sym = m.as_str();
                    if !known.contains(sym) && !out.iter().any(|x| x == sym) {
                        out.push(sym.to_string());
                    }
                }
            }
        }
    }
    out
}

fn reconcile_hook_generic_array_args(proj_dir: &std::path::Path, content: &str) -> String {
    let Ok(call_re) = regex::Regex::new(r"\b(use[A-Z][A-Za-z0-9_]*)<\s*([A-Z][A-Za-z0-9_]*)\s*\[\s*\]\s*>") else {
        return content.to_string();
    };
    if !call_re.is_match(content) {
        return content.to_string();
    }
    let hooks_dirs = [proj_dir.join("src/hooks"), proj_dir.join("hooks")];
    let mut out = content.to_string();
    let caps: Vec<(String, String, String)> = call_re
        .captures_iter(content)
        .filter_map(|c| {
            let full = c.get(0)?.as_str().to_string();
            let hook = c.get(1)?.as_str().to_string();
            let ty = c.get(2)?.as_str().to_string();
            Some((full, hook, ty))
        })
        .collect();

    for (full, hook_name, ty_name) in caps {
        let mut needs_unwrap = false;
        for hd in &hooks_dirs {
            if !hd.is_dir() {
                continue;
            }
            for ext in ["ts", "tsx", "js", "jsx"] {
                let hp = hd.join(format!("{}.{}", hook_name, ext));
                if hp.is_file() {
                    if let Ok(hook_src) = std::fs::read_to_string(&hp) {
                        let constraint_pat = format!(
                            r"(?m)\b{}<\s*[A-Za-z_]\w*\s+extends\s+{}\b",
                            regex::escape(&hook_name),
                            regex::escape(&ty_name)
                        );
                        let array_constraint_pat = format!(
                            r"(?m)\b{}<\s*[A-Za-z_]\w*\s+extends\s+{}\s*\[\s*\]",
                            regex::escape(&hook_name),
                            regex::escape(&ty_name)
                        );
                        let has_item_constraint = regex::Regex::new(&constraint_pat)
                            .map(|r| r.is_match(&hook_src))
                            .unwrap_or(false);
                        let has_array_constraint = regex::Regex::new(&array_constraint_pat)
                            .map(|r| r.is_match(&hook_src))
                            .unwrap_or(false);
                        if has_item_constraint && !has_array_constraint {
                            needs_unwrap = true;
                        }
                    }
                    break;
                }
            }
        }
        if needs_unwrap {
            let fixed = format!("{}<{}>", hook_name, ty_name);
            out = out.replace(&full, &fixed);
        }
    }
    out
}

pub fn auto_import_missing_symbols_in_content(
    proj_dir: &std::path::Path,
    abs_file: &std::path::Path,
    rel_path: &str,
    orig_content: &str,
    extra_missing_idents: &[String],
) -> Option<String> {
    let lower = rel_path.to_lowercase();
    let is_jsx = lower.ends_with(".tsx") || lower.ends_with(".jsx");
    let is_ts = lower.ends_with(".ts") || lower.ends_with(".js");
    if !is_jsx && !is_ts {
        return None;
    }
    if lower.contains("/api/") && lower.ends_with("route.ts") {
        return None;
    }

    let mut content = reconcile_external_icon_imports_in_content(proj_dir, orig_content);
    content = reconcile_ui_barrel_hook_imports(proj_dir, abs_file, &content);
    content = reconcile_hook_generic_array_args(proj_dir, &content);

    let mut existing_specs: Vec<String> = regex::Regex::new(
        r#"(?ms)(?:import\s+(?:type\s+)?[\s\S]*?\s+from\s+|import\s+|require\s*\(\s*)['"]([^'"]+)['"]"#,
    )
    .ok()
    .map(|re| {
        re.captures_iter(&content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
            .collect()
    })
    .unwrap_or_default();

    let known = collect_declared_or_imported_idents(&content);
    let mut missing_syms = extract_unimported_jsx_components(&content, &known, is_jsx, extra_missing_idents);

    let (prisma_models, prisma_enums) = collect_prisma_schema_symbol_names(proj_dir);
    let type_candidates = extract_missing_type_ident_candidates(&content, &known);
    for tc in type_candidates {
        if (prisma_models.contains(&tc) || prisma_enums.contains(&tc)) && !missing_syms.iter().any(|m| m == &tc) {
            missing_syms.push(tc);
        }
    }

    let is_client_or_hook = content.contains("\"use client\"")
        || content.contains("'use client'")
        || lower.contains("/hooks/")
        || lower.starts_with("hooks/");

    for sym in missing_syms {
        if let Some(pkg_spec) = find_installed_dts_package_for_symbol(proj_dir, &existing_specs, &sym) {
            content = merge_or_insert_import_statement(&content, &pkg_spec, &sym, false);
            if !existing_specs.iter().any(|s| s == &pkg_spec) {
                existing_specs.push(pkg_spec);
            }
        } else if let Some((local_spec, is_def)) = find_unique_local_module_for_symbol(proj_dir, abs_file, &sym) {
            content = merge_or_insert_import_statement(&content, &local_spec, &sym, is_def);
        } else if prisma_models.contains(&sym) || prisma_enums.contains(&sym) {
            let used_as_value = !is_client_or_hook
                && prisma_enums.contains(&sym)
                && (content.contains(&format!("{}.", sym))
                    || content.contains(&format!("nativeEnum({})", sym)));
            let already_has_prisma_imp = content.contains("from '@prisma/client'")
                || content.contains("from \"@prisma/client\"");
            if used_as_value {
                content = merge_or_insert_import_statement(&content, "@prisma/client", &sym, false);
            } else if !already_has_prisma_imp {
                let imp_line = format!("import type {{ {} }} from '@prisma/client';", sym);
                content = crate::sanitizer::insert_import_after_use_client(&content, &imp_line);
                content = crate::sanitizer::hoist_use_client_directive(&content);
            } else {
                content = merge_or_insert_import_statement(&content, "@prisma/client", &format!("type {}", sym), false);
            }
        }
    }

    if content != orig_content {
        Some(content)
    } else {
        None
    }
}

pub fn auto_import_missing_symbols_in_file(
    proj_dir: &std::path::Path,
    abs_file: &std::path::Path,
    rel_path: &str,
    extra_missing_idents: &[String],
) -> Option<String> {
    let Ok(orig_content) = std::fs::read_to_string(abs_file) else {
        return None;
    };
    auto_import_missing_symbols_in_content(proj_dir, abs_file, rel_path, &orig_content, extra_missing_idents)
}

pub fn auto_import_missing_symbols_in_project(proj_dir: &std::path::Path) -> usize {
    let mut updated = 0usize;
    fn walk_tsx(dir: &std::path::Path, proj_dir: &std::path::Path, updated: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
                if n != "node_modules" && n != ".next" && n != ".git" && n != ".axiom" && n != "prisma" && n != "api" {
                    walk_tsx(&p, proj_dir, updated);
                }
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "tsx" | "jsx" | "ts" | "js") {
                    if let Ok(rel) = p.strip_prefix(proj_dir) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        if let Some(next_content) = auto_import_missing_symbols_in_file(proj_dir, &p, &rel_str, &[]) {
                            if std::fs::write(&p, next_content).is_ok() {
                                *updated += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if proj_dir.join("src").is_dir() {
        walk_tsx(&proj_dir.join("src"), proj_dir, &mut updated);
    } else {
        for d_name in ["app", "components", "hooks", "lib", "pages"] {
            let d = proj_dir.join(d_name);
            if d.is_dir() {
                walk_tsx(&d, proj_dir, &mut updated);
            }
        }
    }
    updated
}

/// Inspects real `.d.ts` type definitions in `node_modules/` for external npm packages imported
/// by `file_content` when `error_context` indicates missing symbol/export/module/reference errors.
/// Enforces a strict wildcard bailout guard (max depth 1, <= 15 relative `export *`) so a
/// truncated partial symbol list is never presented as exhaustive truth to the LLM, while also
/// performing single-symbol `.d.ts` resolution for `TS2304`/`ReferenceError` missing identifiers.
pub fn inspect_dts_exports_for_file(proj_dir: &std::path::Path, file_content: &str, error_context: &str) -> String {
    let err_lower = error_context.to_lowercase();
    let has_symbol_or_mod_err = error_context.contains("TS2305")
        || error_context.contains("TS2614")
        || error_context.contains("TS2724")
        || error_context.contains("TS2307")
        || error_context.contains("TS2339")
        || error_context.contains("TS2304")
        || error_context.contains("ReferenceError")
        || err_lower.contains("has no exported member")
        || err_lower.contains("does not contain a default export")
        || err_lower.contains("attempted import error")
        || err_lower.contains("module not found")
        || err_lower.contains("is not defined")
        || err_lower.contains("cannot find name");
    let mentions_stripe = file_content.contains("'stripe'")
        || file_content.contains("\"stripe\"")
        || err_lower.contains("stripe")
        || err_lower.contains("latestapiversion");
    let has_stripe_api_err = mentions_stripe && (
        has_symbol_or_mod_err
        || error_context.contains("TS2322")
        || error_context.contains("TS2345")
        || error_context.contains("TS2561")
        || error_context.contains("TS2769")
        || err_lower.contains("apiversion")
        || err_lower.contains("latestapiversion")
    );
    if !(has_symbol_or_mod_err || has_stripe_api_err) || !proj_dir.join("node_modules").is_dir() {
        return String::new();
    }

    let Ok(ext_re) = regex::Regex::new(r#"(?:import\s+(?:type\s+)?[\s\S]*?\s+from\s+|import\s+|require\s*\(\s*)['"]([^'"]+)['"]"#) else {
        return String::new();
    };
    let mut specs: Vec<String> = Vec::new();
    for cap in ext_re.captures_iter(file_content) {
        let raw_spec = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if raw_spec.is_empty()
            || raw_spec.starts_with('.')
            || raw_spec.starts_with('/')
            || raw_spec.starts_with("@/")
            || raw_spec.starts_with("~/")
            || raw_spec.starts_with("src/")
            || raw_spec.starts_with("app/")
            || raw_spec.starts_with("node:")
            || raw_spec.ends_with(".css")
            || raw_spec.ends_with(".scss")
        {
            continue;
        }
        if !specs.iter().any(|s| s == raw_spec) {
            specs.push(raw_spec.to_string());
        }
    }

    let mut lines: Vec<String> = Vec::new();
    if has_symbol_or_mod_err && !specs.is_empty() {
        // Prioritize external specifiers directly mentioned in `error_context`
        let mentioned: Vec<String> = specs
            .iter()
            .filter(|s| error_context.contains(s.as_str()) || err_lower.contains(&s.to_lowercase()))
            .cloned()
            .collect();
        let ordered_specs = if !mentioned.is_empty() { mentioned } else { specs.clone() };

        let err_tokens: std::collections::HashSet<String> = error_context
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .filter(|t| t.len() >= 2)
            .map(|t| t.to_lowercase())
            .collect();

        for spec in ordered_specs.into_iter().take(3) {
            let Some(dts_path) = resolve_external_dts_path(proj_dir, &spec) else {
                continue;
            };
            let parsed = parse_dts_exports_with_guard(&dts_path);
            if parsed.incomplete || (parsed.symbols.is_empty() && !parsed.has_default) {
                continue;
            }

            let total_syms = parsed.symbols.len();
            let mut syms = parsed.symbols;
            syms.sort_by(|a, b| {
                let a_low = a.to_lowercase();
                let b_low = b.to_lowercase();
                let a_hit = err_tokens.contains(&a_low) || err_tokens.iter().any(|t| t.len() >= 3 && (a_low.contains(t) || t.contains(&a_low)));
                let b_hit = err_tokens.contains(&b_low) || err_tokens.iter().any(|t| t.len() >= 3 && (b_low.contains(t) || t.contains(&b_low)));
                b_hit.cmp(&a_hit).then_with(|| a.cmp(b))
            });

            let truncated_extra = total_syms.saturating_sub(40);
            let displayed: Vec<String> = syms.into_iter().take(40).collect();
            let sym_str = if displayed.is_empty() {
                "(default export only)".to_string()
            } else if truncated_extra > 0 {
                format!("{} (+{} more exports)", displayed.join(", "), truncated_extra)
            } else {
                displayed.join(", ")
            };
            lines.push(format!(
                "- `{}` [default export: {}]: {}",
                spec,
                if parsed.has_default { "yes" } else { "no" },
                sym_str
            ));
        }
    }

    // Dynamic installed Stripe SDK inspection from `node_modules/stripe`
    let stripe_root = proj_dir.join("node_modules/stripe");
    if (has_stripe_api_err || specs.iter().any(|s| s == "stripe" || s.starts_with("stripe/"))) && stripe_root.is_dir() {
        let mut detected_api_version: Option<String> = None;
        if let Ok(ver_re) = regex::Regex::new(r#"(?:LatestApiVersion|ApiVersion)\s*=\s*['"]([^'"]+)['"]"#) {
            for rel_candidate in [
                "esm/apiVersion.d.ts",
                "cjs/apiVersion.d.ts",
                "types/apiVersion.d.ts",
                "types/index.d.ts",
            ] {
                let cand = stripe_root.join(rel_candidate);
                if cand.is_file() {
                    if let Ok(dts_src) = std::fs::read_to_string(&cand) {
                        if let Some(cap) = ver_re.captures(&dts_src) {
                            if let Some(m) = cap.get(1) {
                                detected_api_version = Some(m.as_str().to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }
        match detected_api_version {
            Some(ver) => lines.push(format!(
                "- `stripe` installed `.d.ts` contract: `LatestApiVersion` on disk is `'{}'`. Recommended: instantiate `new Stripe(process.env.STRIPE_SECRET_KEY || '')` WITHOUT hardcoding an outdated `apiVersion`, or set `apiVersion: '{}' as any` only if explicitly required.",
                ver, ver
            )),
            None => lines.push(
                "- `stripe` installed `.d.ts` contract: instantiate `new Stripe(process.env.STRIPE_SECRET_KEY || '')` WITHOUT hardcoding a stale `apiVersion` string literal.".to_string(),
            ),
        }

        if err_lower.contains("subscriptions") || err_lower.contains("'del'") || file_content.contains("subscriptions.del") {
            let mut has_cancel_in_dts = false;
            let mut has_del_in_dts = false;
            for rel_sub in [
                "esm/resources/Subscriptions.d.ts",
                "cjs/resources/Subscriptions.d.ts",
                "types/SubscriptionsResource.d.ts",
            ] {
                let cand = stripe_root.join(rel_sub);
                if cand.is_file() {
                    if let Ok(sub_src) = std::fs::read_to_string(&cand) {
                        if sub_src.contains("cancel(") {
                            has_cancel_in_dts = true;
                        }
                        if sub_src.contains("del(") {
                            has_del_in_dts = true;
                        }
                        break;
                    }
                }
            }
            if has_cancel_in_dts && !has_del_in_dts {
                lines.push(
                    "- `stripe.subscriptions` (verified in `node_modules/stripe/.../Subscriptions.d.ts`): use `stripe.subscriptions.cancel(subscriptionId)` instead of `.del(...)` (`.del` does not exist).".to_string(),
                );
            }
        }
        if file_content.contains("payment_method_types") && file_content.contains("automatic_payment_methods") {
            lines.push(
                "- `stripe.paymentIntents.create`: do NOT pass both `payment_method_types` and `automatic_payment_methods` together (Stripe rejects conflicting parameters).".to_string(),
            );
        }

        // Stripe v20-v23 breaking changes verified on disk in node_modules/stripe
        let has_webhooks = file_content.contains("constructEvent") || err_lower.contains("constructevent") || err_lower.contains("readablestream");
        if has_webhooks && (file_content.contains("request.body") || file_content.contains("req.body") || err_lower.contains("readablestream")) {
            lines.push(
                "- `stripe.webhooks.constructEvent`: NEVER pass `request.body` (`ReadableStream | null`). In Next.js App Router route handlers, read `const rawBody = await request.text();` and pass `rawBody` (string).".to_string(),
            );
        }
        if file_content.contains("z.nativeEnum(Stripe.") || err_lower.contains("stripe.paymentintent") {
            lines.push(
                "- `Stripe` Zod validation: `Stripe.PaymentIntent.Status` (and other `Stripe.*` namespaces) are TypeScript type definitions ONLY, NOT runtime JS enums. Replace `z.nativeEnum(Stripe....)` with `z.string()` or `z.enum([...])`.".to_string(),
            );
        }

        let mut invoice_has_payment_intent = false;
        for rel_inv in [
            "esm/resources/Invoices.d.ts",
            "cjs/resources/Invoices.d.ts",
            "types/InvoicesResource.d.ts",
        ] {
            let cand = stripe_root.join(rel_inv);
            if cand.is_file() {
                if let Ok(inv_src) = std::fs::read_to_string(&cand) {
                    if inv_src.contains("payment_intent:") || inv_src.contains("payment_intent?:") {
                        invoice_has_payment_intent = true;
                    }
                    break;
                }
            }
        }
        if !invoice_has_payment_intent && (file_content.contains(".payment_intent") || err_lower.contains("payment_intent")) {
            lines.push(
                "- `Stripe.Invoice` on disk: `invoice.payment_intent` does NOT exist in this installed Stripe SDK version (v20+). Access `(invoice as any).payment_intent` or `invoice.payments` instead.".to_string(),
            );
        }

        if file_content.contains(".plan") || err_lower.contains("subscriptionitem") || err_lower.contains("'plan'") {
            lines.push(
                "- `Stripe.SubscriptionItem` on disk: legacy `.plan` property is replaced by `.price` (`subscription.items.data[0]?.price?.id`).".to_string(),
            );
        }
        if file_content.contains("checkout.sessions.create") && (err_lower.contains("payment_method_types") || file_content.contains("payment_method_types")) {
            lines.push(
                "- `stripe.checkout.sessions.create`: remove conflicting/deprecated `payment_method_types` literal typing (or let Stripe Checkout dynamically choose payment methods without `payment_method_types`).".to_string(),
            );
        }
        if file_content.contains(".customer") && (err_lower.contains("deletedcustomer") || err_lower.contains("string | stripe.customer")) {
            lines.push(
                "- `subscription.customer` / `invoice.customer`: type is `string | Stripe.Customer | Stripe.DeletedCustomer | null`. Narrow with `typeof sub.customer === 'string' ? sub.customer : sub.customer?.id`.".to_string(),
            );
        }
    }

    // Resolve concrete installed package subpath for missing TS2304 / ReferenceError symbols
    if has_symbol_or_mod_err {
        let missing_idents = extract_missing_ident_candidates_from_error(error_context);
        for ident in missing_idents.into_iter().take(4) {
            if let Some(resolved_spec) = find_installed_dts_package_for_symbol(proj_dir, &specs, &ident) {
                lines.push(format!(
                    "- Verified symbol on disk for `{}`: `import {{ {} }} from '{}';`",
                    ident, ident, resolved_spec
                ));
            }
        }
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!(
            "\nExternal package `.d.ts` exports (verified from node_modules):\n{}\n",
            lines.join("\n")
        )
    }
}

fn run_tsc_check(proj_dir: &std::path::Path, local_tsc_js: &std::path::Path, enriched_path: &str) -> Result<(bool, String, Vec<TscDiagnostic>), String> {
    let mut cmd = std::process::Command::new("node");
    cmd.arg(local_tsc_js)
        .arg("--noEmit")
        .arg("--incremental")
        .arg("false")
        .arg("--pretty")
        .arg("false")
        .current_dir(proj_dir);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.env("PATH", enriched_path).env("NO_COLOR", "1");
    let output = cmd.output().map_err(|e| format!("Failed to execute tsc: {}", e))?;
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let log = format!("{}\n{}", stdout_str, stderr_str);
    let diags = parse_tsc_diagnostics(&log);
    Ok((output.status.success() && diags.is_empty(), log, diags))
}

pub async fn perform_self_healing_loop(proj_dir: &std::path::Path, model: &str, files_list: &[Value]) -> Vec<String> {
    let mut unresolved_warnings: Vec<String> = Vec::new();
    let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();
    let is_nextjs = proj_dir.join("next.config.js").exists()
        || proj_dir.join("next.config.mjs").exists()
        || proj_dir.join("package.json").exists();
    if !is_nextjs {
        return unresolved_warnings;
    }

    debug_log_to_file(format!("[Self-Healing] Running Pre-Build Hygiene Pass (v{})...", env!("CARGO_PKG_VERSION")));
    run_hygiene_pass_on_project(proj_dir);

    let enriched_path = ensure_node_toolchain_ready(None, "self-heal").await;
    let local_next_js = proj_dir.join("node_modules").join("next").join("dist").join("bin").join("next");
    let has_local_next = local_next_js.exists();
    let local_tsc_js = proj_dir.join("node_modules").join("typescript").join("bin").join("tsc");
    let has_local_tsc = local_tsc_js.exists();

    if !has_local_next && !has_local_tsc {
        let msg = "TypeScript i Next.js nisu pronađeni u node_modules (npm install nije završen).".to_string();
        debug_log_to_file(format!("[Self-Healing] Skipping build loop: {}", msg));
        unresolved_warnings.push(msg);
        return unresolved_warnings;
    }

    // Collect candidate paths for matching errors in build logs
    let mut candidate_paths: Vec<String> = files_list
        .iter()
        .filter_map(|f| f.get("path").and_then(|p| p.as_str()).map(|s| s.replace('\\', "/")))
        .collect();
    for cfg in ["next.config.js", "next.config.mjs", "tailwind.config.js", "tailwind.config.ts", "tsconfig.json", "middleware.ts"] {
        if proj_dir.join(cfg).exists() && !candidate_paths.iter().any(|p| p == cfg) {
            candidate_paths.push(cfg.to_string());
        }
    }
    fn collect_src_candidates(dir: &std::path::Path, proj_dir: &std::path::Path, out: &mut Vec<String>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
                    if n != "node_modules" && n != ".next" && n != ".git" {
                        collect_src_candidates(&p, proj_dir, out);
                    }
                } else if p.is_file() {
                    if let Ok(rel) = p.strip_prefix(proj_dir) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        if !out.contains(&rel_str) {
                            out.push(rel_str);
                        }
                    }
                }
            }
        }
    }
    if proj_dir.join("src").exists() {
        collect_src_candidates(&proj_dir.join("src"), proj_dir, &mut candidate_paths);
    } else {
        for dir_name in ["app", "components", "hooks", "lib", "utils", "models", "types", "services"] {
            let d = proj_dir.join(dir_name);
            if d.is_dir() {
                collect_src_candidates(&d, proj_dir, &mut candidate_paths);
            }
        }
    }

    struct PendingBuildFix {
        path: String,
        before: String,
        after: String,
        entry_id: String,
        source: String,
        prev_file_errors: usize,
        prev_diags: Vec<String>,
    }

    let client = reqwest::Client::new();
    let mut tried_katastar_ids: Vec<String> = Vec::new();

    // Extract project specs once for context
    let mut category_stack_info = "Next.js App Router + Tailwind".to_string();
    let mut lbg_file_specs: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
    if lbg_path.exists() {
        if let Ok(lbg_content) = std::fs::read_to_string(&lbg_path) {
            if let Ok(lbg_json) = serde_json::from_str::<Value>(&lbg_content) {
                let cat = lbg_json.get("projectSpecification").and_then(|s| s.get("category")).and_then(|c| c.as_str()).unwrap_or("web-app");
                let stk = lbg_json.get("projectSpecification").and_then(|s| s.get("stack")).and_then(|c| c.as_str()).unwrap_or("Next.js");
                category_stack_info = format!("Stack: {}, Category: {}", stk, cat);
                if let Some(req_routes) = lbg_json.get("projectSpecification").and_then(|s| s.get("categoryRequirements")).and_then(|r| r.get("requiredRoutes")).and_then(|r| r.as_array()) {
                    for rr in req_routes {
                        if let Some(rr_str) = rr.as_str() {
                            lbg_file_specs.insert(rr_str.to_string(), rr_str.to_string());
                        }
                    }
                }
            }
        }
    }

    // =========================================================================
    // TIER 1: Full-Project TypeScript Verification (`tsc --noEmit --pretty false`)
    // Discovers ALL type & import errors simultaneously and repairs in topological order.
    // =========================================================================
    if has_local_tsc {
        const MAX_TSC_ROUNDS: usize = 10;
        let mut pending_tsc_fixes: Vec<PendingBuildFix> = Vec::new();
        let mut prev_total_tsc_errors: Option<usize> = None;
        let mut rollback_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut stagnant_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for round in 1..=(MAX_TSC_ROUNDS + 1) {
            let Ok((mut tsc_ok, mut tsc_log, mut diags)) = run_tsc_check(proj_dir, &local_tsc_js, &enriched_path) else {
                break;
            };

            let mut file_err_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            let mut per_file_diag_lines: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
            for d in &diags {
                let key = resolve_project_file_candidate(proj_dir, &d.file)
                    .unwrap_or_else(|| d.file.clone())
                    .to_lowercase();
                *file_err_counts.entry(key.clone()).or_insert(0) += 1;
                per_file_diag_lines
                    .entry(key)
                    .or_default()
                    .push(format!("Line {}, Col {}: [{}] {}", d.line, d.col, d.code, d.message));
            }
            let mut total_errors = diags.len();
            let mut rolled_back_this_round = false;

            // Verify previous Tier 1 fixes with diff-based partial progress retention & rollback protection
            if !pending_tsc_fixes.is_empty() {
                let total_spiked = prev_total_tsc_errors
                    .map(|prev| total_errors > prev + 3)
                    .unwrap_or(false);

                for pf in pending_tsc_fixes.drain(..) {
                    let pf_key = pf.path.to_lowercase();
                    let cur_diags = per_file_diag_lines.get(&pf_key).cloned().unwrap_or_default();
                    let cur_errs = *file_err_counts.get(&pf_key).unwrap_or(&0);
                    let diff = crate::katastar::compute_diagnostic_diff(&pf.prev_diags, &cur_diags);
                    let improved = !total_spiked
                        && diff.introduced.is_empty()
                        && (cur_errs < pf.prev_file_errors
                            || (!diff.resolved.is_empty() && cur_errs <= pf.prev_file_errors));
                    let round_num = round.saturating_sub(1).max(1);

                    if tsc_ok || cur_errs == 0 {
                        let atomic_patch = if pf.source == "ai" {
                            crate::katastar::extract_safe_atomic_patch(&pf.path, &pf.before, &pf.after)
                        } else {
                            None
                        };
                        crate::katastar::finalize_katastar_outcome(
                            &pf.entry_id,
                            true,
                            &pf.source,
                            "tsc_noemit",
                            atomic_patch,
                            None,
                            model,
                        );
                        crate::katastar::record_project_heal_round(
                            proj_dir,
                            &pf.path,
                            round_num,
                            &pf.source,
                            "verified",
                            &diff.resolved,
                            &cur_diags,
                            &format!("tsc cleared errors ({} -> 0)", pf.prev_file_errors),
                        );
                        debug_log_to_file(format!("[Katastar] Tier 1 tsc passed for '{}' (entry {}).", pf.path, pf.entry_id));
                    } else if pf.source == "ai" && improved {
                        // Verified partial progress: keep edit on disk, record resolved vs unresolved in project ledger,
                        // and do NOT promote partial fix as globally verified in Katastar.
                        crate::katastar::finalize_katastar_outcome(
                            &pf.entry_id,
                            false,
                            &pf.source,
                            "partial_progress",
                            None,
                            None,
                            model,
                        );
                        crate::katastar::record_project_heal_round(
                            proj_dir,
                            &pf.path,
                            round_num,
                            &pf.source,
                            "partial_progress",
                            &diff.resolved,
                            &cur_diags,
                            &format!(
                                "kept partial progress (errors {} -> {}, resolved {}, introduced 0)",
                                pf.prev_file_errors,
                                cur_errs,
                                diff.resolved.len()
                            ),
                        );
                        debug_log_to_file(format!(
                            "[Self-Healing] Kept partial Tier 1 improvement on '{}' (errors {} -> {}, resolved {}).",
                            pf.path, pf.prev_file_errors, cur_errs, diff.resolved.len()
                        ));
                    } else {
                        crate::katastar::finalize_katastar_outcome(
                            &pf.entry_id,
                            false,
                            &pf.source,
                            "tsc_noemit",
                            None,
                            None,
                            model,
                        );
                        if pf.source == "katastar" {
                            tried_katastar_ids.push(pf.entry_id);
                        }
                        restore_pending_file_snapshot(proj_dir, &pf.path, &pf.before);
                        rolled_back_this_round = true;
                        if pf.source == "ai" {
                            *rollback_counts.entry(pf_key.clone()).or_insert(0) += 1;
                        }
                        crate::katastar::record_project_heal_round(
                            proj_dir,
                            &pf.path,
                            round_num,
                            &pf.source,
                            "rolled_back",
                            &[],
                            &pf.prev_diags,
                            &format!(
                                "rolled back (errors {} -> {}, introduced {}, total_spiked={})",
                                pf.prev_file_errors,
                                cur_errs,
                                diff.introduced.len(),
                                total_spiked
                            ),
                        );
                        debug_log_to_file(format!(
                            "[Self-Healing] Rolled back Tier 1 {} edit on '{}' (errors {} -> {}, introduced {}, total spiked={}).",
                            pf.source, pf.path, pf.prev_file_errors, cur_errs, diff.introduced.len(), total_spiked
                        ));
                    }
                }
            }

            if rolled_back_this_round {
                if let Ok((fresh_ok, fresh_log, fresh_diags)) = run_tsc_check(proj_dir, &local_tsc_js, &enriched_path) {
                    tsc_ok = fresh_ok;
                    tsc_log = fresh_log;
                    diags = fresh_diags;
                    file_err_counts.clear();
                    per_file_diag_lines.clear();
                    for d in &diags {
                        let key = resolve_project_file_candidate(proj_dir, &d.file)
                            .unwrap_or_else(|| d.file.clone())
                            .to_lowercase();
                        *file_err_counts.entry(key.clone()).or_insert(0) += 1;
                        per_file_diag_lines
                            .entry(key)
                            .or_default()
                            .push(format!("Line {}, Col {}: [{}] {}", d.line, d.col, d.code, d.message));
                    }
                    total_errors = diags.len();
                }
            }

            if tsc_ok || diags.is_empty() {
                debug_log_to_file(format!("[Self-Healing] Tier 1 tsc --noEmit passed cleanly (round {}).", round));
                break;
            }

            prev_total_tsc_errors = Some(total_errors);
            if round > MAX_TSC_ROUNDS {
                let sample: Vec<String> = diags
                    .iter()
                    .take(4)
                    .map(|d| format!("{}:{} {} ({})", d.file, d.line, d.code, d.message.chars().take(70).collect::<String>()))
                    .collect();
                unresolved_warnings.push(format!("TypeScript greške ({}): {}", diags.len(), sample.join(" | ")));
                break;
            }

            // Group diagnostics by resolved workspace file, ordered topologically by layer priority
            let mut per_file_diags: std::collections::BTreeMap<String, Vec<&TscDiagnostic>> = std::collections::BTreeMap::new();
            for d in &diags {
                let resolved = resolve_project_file_candidate(proj_dir, &d.file)
                    .unwrap_or_else(|| d.file.clone());
                if proj_dir.join(&resolved).is_file() {
                    per_file_diags.entry(resolved).or_default().push(d);
                }
            }

            if per_file_diags.is_empty() {
                debug_log_to_file("[Self-Healing] Tier 1 tsc errors could not be mapped to editable project files.".to_string());
                break;
            }

            let attempts_failed_for = |path_key: &str| -> usize {
                let k = path_key.to_lowercase();
                rollback_counts.get(&k).copied().unwrap_or(0) + stagnant_counts.get(&k).copied().unwrap_or(0)
            };

            let mut sorted_files: Vec<(String, Vec<&TscDiagnostic>)> = per_file_diags.into_iter().collect();
            sorted_files.sort_by_key(|(path_str, _)| {
                let af = attempts_failed_for(path_str);
                (af >= 2, af, file_dependency_layer_priority(path_str), path_str.clone())
            });
            if sorted_files.iter().all(|(p, _)| attempts_failed_for(p) >= 2) {
                let sample: Vec<String> = diags
                    .iter()
                    .take(4)
                    .map(|d| format!("{}:{} {} ({})", d.file, d.line, d.code, d.message.chars().take(70).collect::<String>()))
                    .collect();
                unresolved_warnings.push(format!("TypeScript greške ({}): {}", diags.len(), sample.join(" | ")));
                debug_log_to_file("[Self-Healing] Stopping Tier 1 tsc loop: all remaining failing files reached max failure/stagnation threshold (2).".to_string());
                break;
            }
            sorted_files.retain(|(p, _)| attempts_failed_for(p) < 2);
            sorted_files.truncate(4);

            // Try Katastar verified patches first
            let mut katastar_hints: Vec<String> = Vec::new();
            let mut katastar_applied_any = false;
            let mut per_file_diag_blocks: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            for (path_str, file_diags) in &sorted_files {
                let full_path = proj_dir.join(path_str);
                let Ok(orig_content) = std::fs::read_to_string(&full_path) else { continue };
                let diag_block: String = file_diags
                    .iter()
                    .map(|d| format!("{}({}:{}): {} {}", d.file, d.line, d.col, d.code, d.message))
                    .collect::<Vec<String>>()
                    .join("\n");
                per_file_diag_blocks.insert(path_str.to_lowercase(), diag_block.clone());
                let lookup = crate::katastar::lookup_katastar("compile", &diag_block, path_str, &orig_content, &tried_katastar_ids);
                for h in lookup.hints {
                    if !katastar_hints.contains(&h) {
                        katastar_hints.push(h);
                    }
                }
                if let Some(best) = lookup.applicable_patches.first() {
                    let candidate = orig_content.replace(&best.patch.search, &best.patch.replace);
                    let processed = post_process_generated_file(path_str, &candidate, &serde_json::json!({}), has_tailwind);
                    if processed != orig_content
                        && crate::healer::looks_like_code_payload(&processed)
                        && std::fs::write(&full_path, &processed).is_ok()
                    {
                        katastar_applied_any = true;
                        let prev_diags = per_file_diag_lines
                            .get(&path_str.to_lowercase())
                            .cloned()
                            .unwrap_or_default();
                        pending_tsc_fixes.push(PendingBuildFix {
                            path: path_str.clone(),
                            before: orig_content,
                            after: processed,
                            entry_id: best.entry_id.clone(),
                            source: "katastar".to_string(),
                            prev_file_errors: file_diags.len(),
                            prev_diags,
                        });
                    }
                }
            }
            if katastar_applied_any {
                continue;
            }

            // Build Tier 1 AI prompt with exact TS diagnostics, imported exporter contracts & chain-of-information ledger
            let mut batch_payload = Vec::new();
            let mut exporter_context = String::new();
            let mut diag_summary = String::new();
            let mut chain_ctx = String::new();
            let mut extra_rulepacks = String::new();
            let mut expected_paths: Vec<String> = Vec::new();
            let mut pkg_ctx = String::new();
            let heal_ledger = crate::katastar::load_project_heal_ledger(proj_dir);
            let mut batch_needs_prisma_contract = false;
            let mut batch_has_prior_stagnation = false;

            for (path_str, file_diags) in &sorted_files {
                let full_path = proj_dir.join(path_str);
                let Ok(orig_content) = std::fs::read_to_string(&full_path) else { continue };
                let prior_fails = rollback_counts.get(&path_str.to_lowercase()).copied().unwrap_or(0)
                    + stagnant_counts.get(&path_str.to_lowercase()).copied().unwrap_or(0);
                if prior_fails >= 1 {
                    batch_has_prior_stagnation = true;
                }
                let path_low = path_str.to_lowercase().replace('\\', "/");
                if path_low.contains("/api/")
                    || path_low.ends_with("route.ts")
                    || path_low.ends_with("route.js")
                    || orig_content.contains("@prisma/client")
                    || orig_content.contains("@/lib/prisma")
                    || file_diags.iter().any(|d| matches!(d.code.as_str(), "TS2322" | "TS2353" | "TS2339" | "TS2305" | "TS2551") || d.message.to_lowercase().contains("prisma"))
                {
                    batch_needs_prisma_contract = true;
                }

                expected_paths.push(path_str.clone());
                let file_lines: Vec<&str> = orig_content.lines().collect();
                diag_summary.push_str(&format!("\n--- {} ({} errors) ---\n", path_str, file_diags.len()));
                if prior_fails >= 1 {
                    diag_summary.push_str("  [WARNING: Previous AI repair attempt for this file made NO valid progress or was rejected. You MUST apply a concrete different code fix now!]\n");
                }
                for (d_idx, d) in file_diags.iter().take(12).enumerate() {
                    diag_summary.push_str(&format!("  Line {}, Col {}: [{}] {}\n", d.line, d.col, d.code, d.message));
                    if d_idx < 5 && d.line > 0 {
                        if let Some(src_line) = file_lines.get(d.line - 1) {
                            let clipped: String = src_line.chars().take(160).collect();
                            diag_summary.push_str(&format!("    -> Line {}: {}\n", d.line, clipped));
                        }
                    }
                }
                let norm_ledger_key = path_str.trim().replace('\\', "/").trim_start_matches("./").to_string();
                if heal_ledger.files.get(&norm_ledger_key).map_or(false, |e| !e.attempts.is_empty() || !e.resolved.is_empty()) {
                    let cur_lines = per_file_diag_lines
                        .get(&path_str.to_lowercase())
                        .cloned()
                        .unwrap_or_default();
                    let file_coi = crate::katastar::format_chain_of_information_prompt(proj_dir, path_str, &cur_lines, round);
                    if !file_coi.is_empty() {
                        chain_ctx.push_str(&format!("\n[History for {}]{}", path_str, file_coi));
                    }
                }
                if exporter_context.len() < 4500 {
                    exporter_context.push_str(&collect_exporter_context_for_file(proj_dir, path_str, &orig_content));
                }
                if exporter_context.len() < 6000 {
                    let file_diag_ref = per_file_diag_blocks.get(&path_str.to_lowercase()).map(|s| s.as_str()).unwrap_or("");
                    exporter_context.push_str(&inspect_dts_exports_for_file(proj_dir, &orig_content, file_diag_ref));
                }
                pkg_ctx.push_str(path_str);
                pkg_ctx.push('\n');
                pkg_ctx.push_str(&orig_content.chars().take(1200).collect::<String>());
                let rp = get_stack_rulepacks(path_str, &orig_content, Some(&tsc_log));
                if !rp.is_empty() && extra_rulepacks.is_empty() {
                    extra_rulepacks = rp;
                }
                if path_low.ends_with("middleware.ts") || path_low.ends_with("middleware.js") {
                    let mw_meta = crate::orchestrator::get_middleware_metacognitive_rules(Some(proj_dir), &diag_summary);
                    if !extra_rulepacks.contains("METACOGNITIVE MIDDLEWARE CHECKLIST") {
                        extra_rulepacks.push_str(&mw_meta);
                    }
                }
                batch_payload.push(serde_json::json!({
                    "path": path_str,
                    "content": orig_content
                }));
            }

            if batch_payload.is_empty() {
                break;
            }
            pkg_ctx.push_str(&diag_summary);

            let prisma_contract_ctx = if batch_needs_prisma_contract {
                extract_prisma_schema_summary_for_healing(proj_dir)
            } else {
                String::new()
            };
            let zod_prisma_guide = if batch_needs_prisma_contract
                && (diag_summary.contains("TS2322") || diag_summary.contains("TS2353"))
            {
                ZOD_PRISMA_TS2322_HEAL_GUIDE
            } else {
                ""
            };

            let contracts_ctx = crate::orchestrator::format_axiom_contracts_prompt(
                proj_dir,
                sorted_files[0].0.as_str(),
            );

            let mut system_prompt = format!(
                "You are a Principal TypeScript & Next.js Compiler Surgeon.\n\
                 Project Architecture: {}\n\
                 Fix ALL TypeScript errors reported below while keeping 100% of existing UI, styles, and business logic intact.\n\
                 MANDATORY RULES:\n\
                 1. NEVER output prose, numbered lists, or markdown outside the JSON object.\n\
                 2. All local project imports MUST use `@/...` alias (never bare `src/...`). Only import modules/symbols that actually exist in the project.\n\
                 3. Domain models in `src/models/*.ts` must be pure TypeScript interfaces/types (NEVER import `@/lib/prisma` or instantiate `new PrismaClient()`).\n\
                 4. Files starting with `\"use client\"` MUST NEVER import `@/lib/prisma` or `@prisma/client` value symbols.\n\
                 Respond ONLY with a valid JSON object matching: `{{\"files\": [{{\"path\": \"...\", \"content\": \"complete fixed file source\"}}]}}`.{}",
                category_stack_info,
                zod_prisma_guide
            );
            system_prompt.push_str(&crate::packages::get_relevant_packages_db_summary(Some(proj_dir), &pkg_ctx));
            if !katastar_hints.is_empty() {
                system_prompt.push_str("\n\n--- VERIFIED KATASTAR SOLUTIONS ---\n");
                for h in katastar_hints.iter().take(3) {
                    system_prompt.push_str(&format!("- {}\n", h));
                }
            }
            if !extra_rulepacks.is_empty() {
                system_prompt.push_str(&extra_rulepacks);
            }

            let user_prompt = format!(
                "TypeScript Diagnostics:{}\n\n{}{}{}{}\nFiles to fix (JSON):\n{}\n\nReturn ONLY a JSON object `{{\"files\": [{{\"path\": \"...\", \"content\": \"...\"}}]}}` containing the complete repaired files.",
                diag_summary,
                exporter_context,
                prisma_contract_ctx,
                contracts_ctx,
                chain_ctx,
                serde_json::to_string_pretty(&batch_payload).unwrap_or_default()
            );

            let tier1_temp = if batch_has_prior_stagnation { 0.28 } else { 0.15 };
            let ollama_req = serde_json::json!({
                "model": model,
                "format": "json",
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": user_prompt }
                ],
                "stream": false,
                "options": build_ollama_options(model, tier1_temp, Some(4096))
            });

            let mut handled_or_counted: std::collections::HashSet<String> = std::collections::HashSet::new();
            if let Ok(resp) = client.post("http://127.0.0.1:11434/api/chat").json(&ollama_req).send().await {
                if resp.status().is_success() {
                    if let Ok(resp_json) = resp.json::<Value>().await {
                        if let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                            let parsed_items = parse_batch_file_fixes_response(ai_msg, &expected_paths);

                            for (path_str, content_str) in parsed_items {
                                if !crate::healer::is_valid_file_path(&path_str) || !crate::healer::looks_like_code_payload(&content_str) {
                                    continue;
                                }
                                let full_path = proj_dir.join(&path_str);
                                if !full_path.starts_with(proj_dir) {
                                    continue;
                                }
                                let path_key_low = path_str.to_lowercase();
                                let before = std::fs::read_to_string(&full_path).unwrap_or_default();
                                let processed = post_process_generated_file(&path_str, &content_str, &serde_json::json!({}), has_tailwind);
                                let prev_diags = per_file_diag_lines
                                    .get(&path_key_low)
                                    .cloned()
                                    .unwrap_or_default();
                                if processed == before {
                                    handled_or_counted.insert(path_key_low.clone());
                                    *stagnant_counts.entry(path_key_low).or_insert(0) += 1;
                                    crate::katastar::record_project_heal_round(
                                        proj_dir,
                                        &path_str,
                                        round,
                                        "ai",
                                        "unchanged_after_postprocess",
                                        &[],
                                        &prev_diags,
                                        "AI rewrite normalized identically to existing file on disk (no effective code delta).",
                                    );
                                    continue;
                                }
                                if !before.is_empty() {
                                    if let Some(drop_reason) = crate::healer::detect_destructive_code_drop(&path_str, &before, &processed) {
                                        handled_or_counted.insert(path_key_low.clone());
                                        *stagnant_counts.entry(path_key_low).or_insert(0) += 1;
                                        debug_log_to_file(format!("[Self-Healing] {}", drop_reason));
                                        crate::katastar::record_project_heal_round(
                                            proj_dir,
                                            &path_str,
                                            round,
                                            "ai",
                                            "rejected_drop",
                                            &[],
                                            &prev_diags,
                                            &drop_reason,
                                        );
                                        continue;
                                    }
                                }
                                if let Err(val_err) = crate::orchestrator::validate_file_with_project_context(proj_dir, &path_str, &processed, model).await {
                                    handled_or_counted.insert(path_key_low.clone());
                                    *stagnant_counts.entry(path_key_low).or_insert(0) += 1;
                                    debug_log_to_file(format!("[Self-Healing] Rejected Tier 1 candidate for '{}': {}", path_str, val_err));
                                    crate::katastar::record_project_heal_round(
                                        proj_dir,
                                        &path_str,
                                        round,
                                        "ai",
                                        "validation_rejected",
                                        &[],
                                        &prev_diags,
                                        &val_err,
                                    );
                                    continue;
                                }
                                if let Some(parent) = full_path.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                }
                                if std::fs::write(&full_path, &processed).is_ok() {
                                    handled_or_counted.insert(path_key_low.clone());
                                    let prev_err = sorted_files
                                        .iter()
                                        .find(|(p, _)| p.eq_ignore_ascii_case(&path_str))
                                        .map(|(_, d)| d.len())
                                        .unwrap_or(1);
                                    let file_diag_raw = per_file_diag_blocks
                                        .get(&path_key_low)
                                        .map(|s| s.as_str())
                                        .unwrap_or(&diag_summary);
                                    let atomic_patch = crate::katastar::extract_safe_atomic_patch(&path_str, &before, &processed);
                                    let sig_info = crate::katastar::normalize_error_signature("compile", file_diag_raw, Some(&path_str));
                                    let entry_id = crate::katastar::register_repair_attempt(
                                        "compile",
                                        file_diag_raw,
                                        &path_str,
                                        "ai",
                                        if atomic_patch.is_some() { "search_replace" } else { "ai_edit" },
                                        atomic_patch,
                                        &sig_info.normalized_sample,
                                        model,
                                    );
                                    pending_tsc_fixes.push(PendingBuildFix {
                                        path: path_str,
                                        before,
                                        after: processed,
                                        entry_id,
                                        source: "ai".to_string(),
                                        prev_file_errors: prev_err,
                                        prev_diags,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            for ep in &expected_paths {
                let ep_low = ep.to_lowercase();
                if !handled_or_counted.contains(&ep_low) {
                    *stagnant_counts.entry(ep_low).or_insert(0) += 1;
                }
            }
        }
    }

    // Re-run fast deterministic hygiene pass before Tier 2 Next.js build
    run_hygiene_pass_on_project(proj_dir);

    if !has_local_next {
        return unresolved_warnings;
    }

    // =========================================================================
    // TIER 2: Full Next.js Production Build (`next build`)
    // Catches Webpack bundling, RSC server/client boundaries, and App Router issues.
    // =========================================================================
    let mut pending_fixes: Vec<PendingBuildFix> = Vec::new();
    const MAX_BUILD_REPAIRS: usize = 2;
    for attempt in 1..=(MAX_BUILD_REPAIRS + 1) {
        debug_log_to_file(format!("[Self-Healing] Tier 2 Next.js build attempt {}/{}...", attempt, MAX_BUILD_REPAIRS + 1));

        let mut cmd = std::process::Command::new("node");
        cmd.arg(&local_next_js).arg("build").current_dir(proj_dir);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.env("PATH", &enriched_path).env("NO_COLOR", "1");

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                debug_log_to_file(format!("[Self-Healing] Failed to execute next build: {}", e));
                break;
            }
        };

        if output.status.success() {
            debug_log_to_file("[Self-Healing] Tier 2 Next.js build completed successfully!".to_string());
            for pf in pending_fixes.drain(..) {
                let atomic_patch = if pf.source == "ai" {
                    crate::katastar::extract_safe_atomic_patch(&pf.path, &pf.before, &pf.after)
                } else {
                    None
                };
                crate::katastar::finalize_katastar_outcome(
                    &pf.entry_id,
                    true,
                    &pf.source,
                    "next_build",
                    atomic_patch,
                    None,
                    model,
                );
            }
            unresolved_warnings.clear();
            break;
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        let build_output = format!("{}\n{}", stdout_str, stderr_str);
        let build_lower = build_output.to_lowercase().replace('\\', "/");

        if !pending_fixes.is_empty() {
            let blocked_on_next_config = build_lower.contains("next.config");
            for pf in pending_fixes.drain(..) {
                let pf_lower = pf.path.to_lowercase().replace('\\', "/");
                let still_failing = build_lower.contains(&pf_lower);
                if still_failing {
                    crate::katastar::finalize_katastar_outcome(
                        &pf.entry_id,
                        false,
                        &pf.source,
                        "next_build",
                        None,
                        None,
                        model,
                    );
                    restore_pending_file_snapshot(proj_dir, &pf.path, &pf.before);
                    debug_log_to_file(format!(
                        "[Self-Healing] Rolled back Tier 2 {} edit on '{}' because it still failed next build.",
                        pf.source, pf.path
                    ));
                    if pf.source == "katastar" {
                        tried_katastar_ids.push(pf.entry_id);
                    }
                } else if !blocked_on_next_config || pf_lower.contains("next.config") {
                    let atomic_patch = if pf.source == "ai" {
                        crate::katastar::extract_safe_atomic_patch(&pf.path, &pf.before, &pf.after)
                    } else {
                        None
                    };
                    crate::katastar::finalize_katastar_outcome(
                        &pf.entry_id,
                        true,
                        &pf.source,
                        "error_disappeared",
                        atomic_patch,
                        None,
                        model,
                    );
                } else {
                    crate::katastar::finalize_katastar_outcome(
                        &pf.entry_id,
                        false,
                        &pf.source,
                        "next_build",
                        None,
                        None,
                        model,
                    );
                    restore_pending_file_snapshot(proj_dir, &pf.path, &pf.before);
                    if pf.source == "katastar" {
                        tried_katastar_ids.push(pf.entry_id);
                    }
                }
            }
        }

        if attempt > MAX_BUILD_REPAIRS {
            let err_lines: Vec<String> = build_output
                .lines()
                .map(|l| l.trim())
                .filter(|l| l.contains("Error:") || l.contains("Module not found") || l.contains("Type error:") || l.contains("Failed to compile"))
                .take(3)
                .map(|l| l.chars().take(120).collect())
                .collect();
            let tail_hint = if err_lines.is_empty() {
                "next build nije uspeo (proveri dev logove)".to_string()
            } else {
                format!("next build greška: {}", err_lines.join(" | "))
            };
            if !unresolved_warnings.iter().any(|w| w.contains("next build")) {
                unresolved_warnings.push(tail_hint);
            }
            break;
        }

        // Map errors in build log to files, prioritizing direct error targets over `Import trace for requested module:` callers
        let mut tier2_priority_paths: Vec<String> = Vec::new();
        let mut secondary_extracted_paths: Vec<String> = Vec::new();
        let mut trace_paths: Vec<String> = Vec::new();
        let build_lines: Vec<&str> = build_output.lines().collect();
        let mut in_import_trace = false;

        for (idx, line) in build_lines.iter().enumerate() {
            let t = line.trim();
            if t.is_empty() {
                in_import_trace = false;
                continue;
            }
            if t.starts_with("Import trace for requested module") {
                in_import_trace = true;
                continue;
            }
            if let Some(extracted) = extract_file_path_from_line(t) {
                let norm = extracted.replace('\\', "/");
                if let Some(resolved) = resolve_project_file_candidate(proj_dir, &norm) {
                    if in_import_trace {
                        if !trace_paths.contains(&resolved) {
                            trace_paths.push(resolved);
                        }
                    } else {
                        let next_line = build_lines.get(idx + 1).map(|s| s.trim()).unwrap_or("");
                        let is_direct_header = t.contains("Module not found")
                            || t.contains("Type error:")
                            || t.contains("SyntaxError")
                            || next_line.starts_with("Module not found")
                            || next_line.starts_with("Attempted import error")
                            || next_line.starts_with("Type error:")
                            || next_line.starts_with("SyntaxError")
                            || next_line.contains("Can't resolve");
                        if is_direct_header {
                            if !tier2_priority_paths.contains(&resolved) {
                                tier2_priority_paths.push(resolved);
                            }
                        } else if !secondary_extracted_paths.contains(&resolved) {
                            secondary_extracted_paths.push(resolved);
                        }
                    }
                }
            }
        }
        for p in secondary_extracted_paths.into_iter().chain(trace_paths.into_iter()) {
            if !tier2_priority_paths.contains(&p) {
                tier2_priority_paths.push(p);
            }
        }
        for path_str in &candidate_paths {
            let path_lower = path_str.to_lowercase().replace('\\', "/");
            let no_ext_lower = std::path::Path::new(&path_lower)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let boundary_hit = no_ext_lower.contains('/') && {
                let mut hit = false;
                let mut offset = 0usize;
                while let Some(rel_idx) = build_lower[offset..].find(&no_ext_lower) {
                    let pos = offset + rel_idx;
                    let left_ok = pos == 0
                        || !build_lower.as_bytes()[pos - 1].is_ascii_alphanumeric()
                            && build_lower.as_bytes()[pos - 1] != b'_';
                    let end = pos + no_ext_lower.len();
                    let right_ok = end >= build_lower.len()
                        || (!build_lower.as_bytes()[end].is_ascii_alphanumeric()
                            && build_lower.as_bytes()[end] != b'_'
                            && build_lower.as_bytes()[end] != b'/');
                    if left_ok && right_ok {
                        hit = true;
                        break;
                    }
                    offset = pos + 1;
                }
                hit
            };
            if (build_lower.contains(&path_lower) || boundary_hit) && !tier2_priority_paths.contains(path_str) {
                tier2_priority_paths.push(path_str.clone());
            }
        }

        let mut files_with_errors = Vec::new();
        let mut matched_paths = std::collections::HashSet::new();
        for path_str in tier2_priority_paths {
            if matched_paths.insert(path_str.clone()) {
                let file_path = proj_dir.join(&path_str);
                if file_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&file_path) {
                        files_with_errors.push(serde_json::json!({
                            "path": path_str,
                            "content": content
                        }));
                    }
                }
            }
        }

        if files_with_errors.is_empty() {
            unresolved_warnings.push("next build nije uspeo bez prepoznate putanje fajla u logu".to_string());
            break;
        }

        // Katastar check
        let mut katastar_hints: Vec<String> = Vec::new();
        let mut katastar_applied_any = false;
        for err_file in &files_with_errors {
            if let (Some(path_str), Some(orig_content)) = (
                err_file.get("path").and_then(|p| p.as_str()),
                err_file.get("content").and_then(|c| c.as_str()),
            ) {
                let lookup = crate::katastar::lookup_katastar("compile", &build_output, path_str, orig_content, &tried_katastar_ids);
                for h in lookup.hints {
                    if !katastar_hints.contains(&h) {
                        katastar_hints.push(h);
                    }
                }
                if let Some(best) = lookup.applicable_patches.first() {
                    let candidate = orig_content.replace(&best.patch.search, &best.patch.replace);
                    let processed = post_process_generated_file(path_str, &candidate, &serde_json::json!({}), has_tailwind);
                    if processed != orig_content
                        && crate::healer::looks_like_code_payload(&processed)
                        && std::fs::write(proj_dir.join(path_str), &processed).is_ok()
                    {
                        katastar_applied_any = true;
                        pending_fixes.push(PendingBuildFix {
                            path: path_str.to_string(),
                            before: orig_content.to_string(),
                            after: processed,
                            entry_id: best.entry_id.clone(),
                            source: "katastar".to_string(),
                            prev_file_errors: 1,
                            prev_diags: Vec::new(),
                        });
                    }
                }
            }
        }
        if katastar_applied_any {
            continue;
        }

        files_with_errors.truncate(2);

        let mut targeted_specs = String::new();
        let mut exporter_context = String::new();
        let mut expected_paths: Vec<String> = Vec::new();
        let mut pkg_ctx = String::new();
        for err_file in &files_with_errors {
            if let (Some(err_path), Some(c)) = (
                err_file.get("path").and_then(|p| p.as_str()),
                err_file.get("content").and_then(|c| c.as_str()),
            ) {
                expected_paths.push(err_path.to_string());
                for spec in lbg_file_specs.values() {
                    if spec.contains(err_path) {
                        targeted_specs.push_str(&format!("- Target File Role: {}\n", spec));
                    }
                }
                if exporter_context.len() < 4500 {
                    exporter_context.push_str(&collect_exporter_context_for_file(proj_dir, err_path, c));
                }
                if exporter_context.len() < 6000 {
                    exporter_context.push_str(&inspect_dts_exports_for_file(proj_dir, c, &build_output));
                }
                pkg_ctx.push_str(err_path);
                pkg_ctx.push('\n');
                pkg_ctx.push_str(&c.chars().take(1200).collect::<String>());
            }
        }

        let truncated_log = if build_output.len() > 3000 {
            format!("...[truncated]...\n{}", &build_output[build_output.len() - 3000..])
        } else {
            build_output.clone()
        };
        pkg_ctx.push('\n');
        pkg_ctx.push_str(&truncated_log);

        let needs_prisma_ctx = expected_paths.iter().any(|p| p.contains("/api/") || p.ends_with("route.ts") || p.ends_with("route.js"))
            || build_lower.contains("prisma")
            || build_output.contains("TS2322")
            || build_output.contains("TS2353");
        let prisma_contract_ctx = if needs_prisma_ctx {
            extract_prisma_schema_summary_for_healing(proj_dir)
        } else {
            String::new()
        };
        let zod_guide = if needs_prisma_ctx && (build_output.contains("TS2322") || build_output.contains("TS2353")) {
            ZOD_PRISMA_TS2322_HEAL_GUIDE
        } else {
            ""
        };

        let mut system_prompt = format!(
            "You are a Senior Next.js Developer fixing compilation and bundling errors.\n\
             Project Architecture: {}\n{}\n\
             CRITICAL HEALING RULES:\n\
             1. STRICT UI & DESIGN PRESERVATION: Fix COMPILATION errors without deleting UI layout or features.\n\
             2. SURGICAL REPAIR & ALIASES: Always import local modules via `@/...` (never bare `src/...`).\n\
             3. CLIENT BOUNDARY: `'use client'` files MUST NOT import `@/lib/prisma` or `@prisma/client` values.\n\
             4. Respond ONLY with a valid JSON object `{{\"files\": [{{\"path\": \"...\", \"content\": \"...\"}}]}}`. No prose or markdown explanations.{}",
            category_stack_info, targeted_specs, zod_guide
        );
        system_prompt.push_str(&crate::packages::get_relevant_packages_db_summary(Some(proj_dir), &pkg_ctx));
        if !katastar_hints.is_empty() {
            system_prompt.push_str("\n\n--- PREVIOUSLY VERIFIED SOLUTIONS FROM LOCAL KATASTAR ---\n");
            for h in katastar_hints.iter().take(3) {
                system_prompt.push_str(&format!("- {}\n", h));
            }
        }
        for err_file in &files_with_errors {
            if let (Some(path_str), Some(content_str)) = (err_file.get("path").and_then(|p| p.as_str()), err_file.get("content").and_then(|c| c.as_str())) {
                let rp = get_stack_rulepacks(path_str, content_str, Some(&truncated_log));
                if !rp.is_empty() {
                    system_prompt.push_str(&rp);
                    break;
                }
            }
        }

        let user_prompt = format!(
            "Build log with error details:\n{}\n\n{}{}Files with errors current contents:\n{}\n\nReturn ONLY a JSON object `{{\"files\": [{{\"path\": \"...\", \"content\": \"...\"}}]}}` containing the repaired files.",
            truncated_log,
            exporter_context,
            prisma_contract_ctx,
            serde_json::to_string_pretty(&files_with_errors).unwrap_or_default()
        );

        let ollama_req = serde_json::json!({
            "model": model,
            "format": "json",
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "stream": false,
            "options": build_ollama_options(model, 0.2, Some(4096))
        });

        if let Ok(resp) = client.post("http://127.0.0.1:11434/api/chat").json(&ollama_req).send().await {
            if resp.status().is_success() {
                if let Ok(resp_json) = resp.json::<Value>().await {
                    if let Some(ai_msg) = resp_json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                        let items = parse_batch_file_fixes_response(ai_msg, &expected_paths);

                        for (path_str, content_str) in items {
                            if !crate::healer::is_valid_file_path(&path_str) || !crate::healer::looks_like_code_payload(&content_str) {
                                continue;
                            }
                            let full_path = proj_dir.join(&path_str);
                            if !full_path.starts_with(proj_dir) {
                                continue;
                            }
                            let before_disk = std::fs::read_to_string(&full_path).unwrap_or_default();
                            let processed = post_process_generated_file(&path_str, &content_str, &serde_json::json!({}), has_tailwind);
                            if processed == before_disk {
                                continue;
                            }
                            if !before_disk.is_empty() {
                                if let Some(drop_reason) = crate::healer::detect_destructive_code_drop(&path_str, &before_disk, &processed) {
                                    debug_log_to_file(format!("[Self-Healing] {}", drop_reason));
                                    continue;
                                }
                            }
                            if let Err(val_err) = crate::orchestrator::validate_file_with_project_context(proj_dir, &path_str, &processed, model).await {
                                debug_log_to_file(format!("[Self-Healing] Rejected Tier 2 candidate for '{}': {}", path_str, val_err));
                                continue;
                            }
                            if let Some(parent) = full_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            if std::fs::write(&full_path, &processed).is_ok() {
                                let atomic_patch = crate::katastar::extract_safe_atomic_patch(&path_str, &before_disk, &processed);
                                let sig_info = crate::katastar::normalize_error_signature("compile", &build_output, Some(&path_str));
                                let entry_id = crate::katastar::register_repair_attempt(
                                    "compile",
                                    &build_output,
                                    &path_str,
                                    "ai",
                                    if atomic_patch.is_some() { "search_replace" } else { "ai_edit" },
                                    atomic_patch,
                                    &sig_info.normalized_sample,
                                    model,
                                );
                                pending_fixes.push(PendingBuildFix {
                                    path: path_str,
                                    before: before_disk,
                                    after: processed,
                                    entry_id,
                                    source: "ai".to_string(),
                                    prev_file_errors: 1,
                                    prev_diags: Vec::new(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    unresolved_warnings
}

pub fn is_postgres_listening_on_port(port: u16) -> bool {
    use std::net::{TcpStream, SocketAddr};
    use std::time::Duration;
    if let Ok(addr) = format!("127.0.0.1:{}", port).parse::<SocketAddr>() {
        TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
    } else {
        false
    }
}

pub fn create_postgres_db_via_tcp(port: u16, db_name: &str) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, SocketAddr};
    use std::time::Duration;

    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().map_err(|e| format!("{:?}", e))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1500)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_millis(2000))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_millis(2000))).map_err(|e| e.to_string())?;

    let clean_db: String = db_name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if clean_db.is_empty() {
        return Err("Invalid database name".to_string());
    }

    // StartupMessage: [int32 len] [int32 protocol (196608)] "user\0postgres\0database\0postgres\0\0"
    let mut payload = Vec::new();
    payload.extend_from_slice(&(196608u32).to_be_bytes());
    payload.extend_from_slice(b"user\0postgres\0database\0postgres\0\0");
    let total_len = (payload.len() + 4) as u32;
    let mut startup_msg = Vec::new();
    startup_msg.extend_from_slice(&total_len.to_be_bytes());
    startup_msg.extend_from_slice(&payload);

    stream.write_all(&startup_msg).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    let mut buf = [0u8; 2048];
    let mut ready = false;
    for _ in 0..15 {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        for i in 0..n {
            if buf[i] == b'Z' && i + 5 <= n {
                ready = true;
                break;
            }
        }
        if ready { break; }
    }

    if !ready {
        return Err("PostgreSQL did not become ReadyForQuery".to_string());
    }

    // Simple query: CREATE DATABASE "clean_db";\0
    let query_str = format!("CREATE DATABASE \"{}\";\0", clean_db);
    let q_len = (query_str.len() + 4) as u32;
    let mut q_msg = Vec::new();
    q_msg.push(b'Q');
    q_msg.extend_from_slice(&q_len.to_be_bytes());
    q_msg.extend_from_slice(query_str.as_bytes());

    stream.write_all(&q_msg).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
    if n > 0 {
        let msg_type = buf[0];
        if msg_type == b'C' {
            let _ = stream.write_all(b"X\0\0\0\x04");
            return Ok(());
        } else if msg_type == b'E' {
            let err_body = String::from_utf8_lossy(&buf[1..n]);
            if err_body.contains("already exists") || err_body.contains("42P04") {
                let _ = stream.write_all(b"X\0\0\0\x04");
                return Ok(());
            }
            return Err(format!("PostgreSQL query error: {}", err_body));
        }
    }

    let _ = stream.write_all(b"X\0\0\0\x04");
    Ok(())
}

pub fn ensure_postgres_database_exists_multi_tier(port: u16, db_name: &str) {
    debug_log_to_file(format!("[Postgres Multi-Tier] Ensuring database '{}' exists on port {}...", db_name, port));
    
    // Tier 1: Embedded TCP wire protocol handshake (fastest, zero dependencies)
    match create_postgres_db_via_tcp(port, db_name) {
        Ok(()) => {
            debug_log_to_file(format!("[Postgres Multi-Tier] Tier 1: Successfully created or verified database '{}' via raw TCP handshake.", db_name));
            return;
        }
        Err(e) => {
            debug_log_to_file(format!("[Postgres Multi-Tier] Tier 1 TCP handshake info/fallback: {}. Trying Tier 2...", e));
        }
    }

    // Tier 2: AppData local psql.exe
    let app_data = dirs::data_local_dir().map(|d| d.join("axiom-forge").join("postgres"));
    if let Some(app_data_path) = app_data {
        let psql_exe = app_data_path.join("pgsql").join("bin").join("psql.exe");
        if psql_exe.exists() {
            let mut db_cmd = std::process::Command::new(psql_exe);
            db_cmd.arg("-h").arg("127.0.0.1")
                .arg("-p").arg(port.to_string())
                .arg("-U").arg("postgres")
                .arg("-d").arg("postgres")
                .arg("-c").arg(format!("CREATE DATABASE {};", db_name));
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                db_cmd.creation_flags(0x08000000);
            }
            if let Ok(st) = db_cmd.status() {
                if st.success() {
                    debug_log_to_file(format!("[Postgres Multi-Tier] Tier 2: Successfully created database '{}' using AppData psql.exe.", db_name));
                    return;
                }
            }
        }
    }

    // Tier 3: System PATH psql
    let mut sys_psql = std::process::Command::new("psql");
    sys_psql.arg("-h").arg("127.0.0.1")
        .arg("-p").arg(port.to_string())
        .arg("-U").arg("postgres")
        .arg("-d").arg("postgres")
        .arg("-c").arg(format!("CREATE DATABASE {};", db_name));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        sys_psql.creation_flags(0x08000000);
    }
    if let Ok(st) = sys_psql.status() {
        if st.success() {
            debug_log_to_file(format!("[Postgres Multi-Tier] Tier 3: Successfully created database '{}' using system psql.", db_name));
            return;
        }
    }

    // Tier 4: Prisma native DB creation pass-through
    debug_log_to_file(format!("[Postgres Multi-Tier] Tier 4: Passing through to Prisma db push native database creation for '{}'.", db_name));
}

pub async fn setup_local_postgres(_proj_dir: &std::path::Path, db_name: &str) -> Result<String, String> {
    // 0. TCP Port Probing: Pre bilo kakvog pokretanja ili fallbacka, proveri da li je PostgreSQL vec aktivan
    if is_postgres_listening_on_port(54321) {
        debug_log_to_file("[Postgres Probe] PostgreSQL is already listening on 127.0.0.1:54321. Reusing active instance.".to_string());
        ensure_postgres_database_exists_multi_tier(54321, db_name);
        return Ok(format!("postgresql://postgres@localhost:54321/{}?schema=public", db_name));
    }
    // Create docker-compose.yml in the project directory
    let docker_compose_content = format!(
        "version: '3.8'\n\
         services:\n\
         \x20\x20db:\n\
         \x20\x20\x20\x20image: postgres:15-alpine\n\
         \x20\x20\x20\x20environment:\n\
         \x20\x20\x20\x20\x20\x20POSTGRES_DB: {}\n\
         \x20\x20\x20\x20\x20\x20POSTGRES_USER: postgres\n\
         \x20\x20\x20\x20\x20\x20POSTGRES_PASSWORD: postgres_password\n\
         \x20\x20\x20\x20ports:\n\
         \x20\x20\x20\x20\x20\x20- \"54321:5432\"\n\
         \x20\x20\x20\x20volumes:\n\
         \x20\x20\x20\x20\x20\x20- pgdata:/var/lib/postgresql/data\n\
         volumes:\n\
         \x20\x20pgdata:\n",
        db_name
    );
    let docker_compose_path = _proj_dir.join("docker-compose.yml");
    let _ = std::fs::write(&docker_compose_path, docker_compose_content);

    // Create .env.example
    let env_example_content = format!(
        "DATABASE_URL=\"postgresql://postgres:postgres_password@localhost:54321/{}?schema=public\"\n\
         NEXTAUTH_SECRET=\"axiom-dev-secret-change-in-production\"\n",
        db_name
    );
    let env_example_path = _proj_dir.join(".env.example");
    let _ = std::fs::write(&env_example_path, env_example_content);

    // Try starting PostgreSQL via Docker Compose
    let mut docker_success = false;
    
    let mut compose_cmd = std::process::Command::new("docker");
    compose_cmd.arg("compose").arg("up").arg("-d").current_dir(_proj_dir);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        compose_cmd.creation_flags(0x08000000);
    }
    
    if let Ok(status) = compose_cmd.status() {
        if status.success() {
            docker_success = true;
        }
    }
    
    if !docker_success {
        let mut legacy_cmd = std::process::Command::new("docker-compose");
        legacy_cmd.arg("up").arg("-d").current_dir(_proj_dir);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            legacy_cmd.creation_flags(0x08000000);
        }
        if let Ok(status) = legacy_cmd.status() {
            if status.success() {
                docker_success = true;
            }
        }
    }

    if docker_success {
        // Wait for database initialization
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        return Ok(format!("postgresql://postgres:postgres_password@localhost:54321/{}?schema=public", db_name));
    }

    let app_data = dirs::data_local_dir()
        .ok_or_else(|| "Could not determine local app data directory".to_string())?
        .join("axiom-forge")
        .join("postgres");
    
    let pg_dir = app_data.join("pgsql");
    let pg_bin = pg_dir.join("bin");
    let pg_postgres_exe = pg_bin.join("postgres.exe");
    let pg_data_dir = app_data.join("data");
    
    if !pg_postgres_exe.exists() {
        let _ = std::fs::create_dir_all(&app_data);
        let zip_path = app_data.join("postgresql.zip");
        
        let pg_url = "https://get.enterprisedb.com/postgresql/postgresql-15.8-1-windows-x64-binaries.zip";
        let response = reqwest::get(pg_url).await
            .map_err(|e| format!("Failed to download PostgreSQL: {}", e))?;
            
        let bytes = response.bytes().await
            .map_err(|e| format!("Failed to read PostgreSQL download bytes: {}", e))?;
            
        std::fs::write(&zip_path, &bytes)
            .map_err(|e| format!("Failed to save PostgreSQL zip file: {}", e))?;
            
        let mut unzip_cmd = std::process::Command::new("powershell");
        unzip_cmd.arg("-Command")
            .arg(format!("Expand-Archive -Path '{}' -DestinationPath '{}' -Force", zip_path.to_string_lossy(), app_data.to_string_lossy()));
            
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            unzip_cmd.creation_flags(0x08000000);
        }
        
        let status = unzip_cmd.status()
            .map_err(|e| format!("Failed to run PowerShell Expand-Archive: {}", e))?;
            
        if !status.success() {
            return Err("PowerShell Expand-Archive exited with error status".to_string());
        }
        
        let _ = std::fs::remove_file(&zip_path);
    }
    
    if !pg_data_dir.exists() || std::fs::read_dir(&pg_data_dir).map(|mut d| d.next().is_none()).unwrap_or(true) {
        let _ = std::fs::create_dir_all(&pg_data_dir);
        let initdb_exe = pg_bin.join("initdb.exe");
        let mut init_cmd = std::process::Command::new(initdb_exe);
        init_cmd.arg("-D").arg(&pg_data_dir)
            .arg("-U").arg("postgres")
            .arg("--auth-local=trust")
            .arg("--auth-host=trust");
            
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            init_cmd.creation_flags(0x08000000);
        }
        
        let status = init_cmd.status()
            .map_err(|e| format!("Failed to run initdb: {}", e))?;
            
        if !status.success() {
            return Err("initdb failed to initialize database cluster".to_string());
        }
    }
    
    let pg_ctl_exe = pg_bin.join("pg_ctl.exe");
    let mut start_cmd = std::process::Command::new(pg_ctl_exe);
    start_cmd.arg("start")
        .arg("-D").arg(&pg_data_dir)
        .arg("-o").arg("-p 54321 -h 127.0.0.1 -N 20 -B 1024 -c work_mem=1MB -c maintenance_work_mem=8MB -c shared_buffers=16MB");
        
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        start_cmd.creation_flags(0x08000000);
    }
    
    let _ = start_cmd.status();
    
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    
    let psql_exe = pg_bin.join("psql.exe");
    let mut db_cmd = std::process::Command::new(psql_exe);
    db_cmd.arg("-h").arg("127.0.0.1")
        .arg("-p").arg("54321")
        .arg("-U").arg("postgres")
        .arg("-d").arg("postgres")
        .arg("-c").arg(format!("CREATE DATABASE {};", db_name));
        
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        db_cmd.creation_flags(0x08000000);
    }
    
    let _ = db_cmd.status();
    
    Ok(format!("postgresql://postgres@localhost:54321/{}?schema=public", db_name))
}

pub fn ensure_nextauth_env_vars(proj_dir: &std::path::Path) {
    let has_auth_route = proj_dir.join("src/app/api/auth").exists()
        || proj_dir.join("app/api/auth").exists()
        || proj_dir.join("src/pages/api/auth").exists();
    let has_pkg_nextauth = std::fs::read_to_string(proj_dir.join("package.json"))
        .map(|s| s.contains("\"next-auth\""))
        .unwrap_or(false);
    if !has_auth_route && !has_pkg_nextauth {
        return;
    }

    let env_paths = [proj_dir.join(".env"), proj_dir.join(".env.local")];
    for env_path in env_paths {
        let existed = env_path.exists();
        let content = std::fs::read_to_string(&env_path).unwrap_or_default();
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut changed = false;

        for (key, default_val) in [
            ("NEXTAUTH_URL", "http://localhost:3000"),
            ("NEXTAUTH_SECRET", "axiom-dev-secret-change-in-production"),
        ] {
            let mut found = false;
            for line in lines.iter_mut() {
                let t = line.trim_start();
                if t.starts_with(&format!("{}=", key)) {
                    let cur_val = t[key.len() + 1..]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'');
                    if cur_val.is_empty() {
                        *line = format!("{}=\"{}\"", key, default_val);
                        changed = true;
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                lines.push(format!("{}=\"{}\"", key, default_val));
                changed = true;
            }
        }

        if changed || !existed {
            let mut out = lines.join("\n");
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            let _ = std::fs::write(&env_path, out);
        }
    }
}

pub fn update_env_database_url(proj_dir: &std::path::Path, db_url: &str) {
    let env_paths = [proj_dir.join(".env"), proj_dir.join(".env.local")];
    for env_path in env_paths {
        if env_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&env_path) {
                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let mut found = false;
                for line in lines.iter_mut() {
                    if line.starts_with("DATABASE_URL=") {
                        *line = format!("DATABASE_URL=\"{}\"", db_url);
                        found = true;
                    }
                }
                if !found {
                    lines.push(format!("DATABASE_URL=\"{}\"", db_url));
                }
                let _ = std::fs::write(&env_path, lines.join("\n"));
            }
        } else {
            let _ = std::fs::write(&env_path, format!("DATABASE_URL=\"{}\"\n", db_url));
        }
    }
    ensure_nextauth_env_vars(proj_dir);
}

pub fn fallback_to_sqlite(proj_dir: &std::path::Path) -> Result<(), String> {
    let schema_paths = [proj_dir.join("prisma/schema.prisma"), proj_dir.join("schema.prisma")];
    let mut schema_path = None;
    for path in schema_paths {
        if path.exists() {
            schema_path = Some(path);
            break;
        }
    }
    
    if let Some(path) = schema_path {
        if let Ok(content) = std::fs::read_to_string(&path) {
            // Zastita od SQLite prelaza: Ako sema sadrzi enum, Json ili skalarne liste, zabrani jednostavan fallback na SQLite
            let has_json = content.contains(" Json") || content.contains("\tJson") || content.contains("Json?") || content.contains("Json\n") || content.contains("Json ");
            if content.contains("enum ") || has_json || content.contains("String[]") || content.contains("Int[]") {
                debug_log_to_file("[Prisma Safety Guard] Cannot fallback to SQLite: schema uses enums, Json or scalar lists unsupported by Prisma SQLite connector.".to_string());
                return Err("Schema incompatible with SQLite: contains enum, Json, or scalar list.".to_string());
            }
            let updated = content
                .replace("provider = \"postgresql\"", "provider = \"sqlite\"")
                .replace("url      = env(\"DATABASE_URL\")", "url      = \"file:./dev.db\"");
            let _ = std::fs::write(&path, updated);
        }
    }
    
    let env_paths = [proj_dir.join(".env"), proj_dir.join(".env.local")];
    for env_path in env_paths {
        if env_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&env_path) {
                let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let mut found = false;
                for line in lines.iter_mut() {
                    if line.starts_with("DATABASE_URL=") {
                        *line = "DATABASE_URL=\"file:./dev.db\"".to_string();
                        found = true;
                    }
                }
                if !found {
                    lines.push("DATABASE_URL=\"file:./dev.db\"".to_string());
                }
                let _ = std::fs::write(&env_path, lines.join("\n"));
            }
        } else {
            let _ = std::fs::write(&env_path, "DATABASE_URL=\"file:./dev.db\"\n");
        }
    }
    
    ensure_nextauth_env_vars(proj_dir);
    Ok(())
}

/// Automated post-generation test runner and crawler.
/// Audits and verifies all UI pages and API endpoints against runtime traps
/// (e.g. React infinite update loops, unhandled hook dependencies, broken API imports).
pub async fn run_generation_auto_test_suite(
    proj_dir: &std::path::Path,
    window: &tauri::Window,
    _model_id: &str,
) -> Vec<String> {
    let _ = window.emit("orchestrator:progress", serde_json::json!({
        "phase": "auto-testing",
        "message": "> [Auto-Test Suite] Započinjem automatsko testiranje ruta, importa i Prisma šeme...",
        "progress": 74.0
    }));
    debug_log_to_file("[Auto-Test Suite] Starting automated route verification and crawler...".to_string());

    let mut warnings: Vec<String> = Vec::new();

    // 1. Discover all page routes from src/app
    let mut page_routes: Vec<String> = Vec::new();
    let app_dir = proj_dir.join("src").join("app");
    if app_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&app_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let dir_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if dir_name != "api" && !dir_name.starts_with('_') && !dir_name.starts_with('.') {
                        if p.join("page.tsx").exists() || p.join("page.jsx").exists() || p.join("page.js").exists() {
                            page_routes.push(format!("/{}", dir_name));
                        }
                    }
                }
            }
        }
    }
    // Always include root route
    page_routes.insert(0, "/".to_string());

    // 2. Discover API routes from src/app/api
    let mut api_routes: Vec<String> = Vec::new();
    let api_dir = app_dir.join("api");
    if api_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&api_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let dir_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if p.join("route.ts").exists() || p.join("route.js").exists() {
                        api_routes.push(format!("/api/{}", dir_name));
                    }
                }
            }
        }
    }

    debug_log_to_file(format!("[Auto-Test Suite] Discovered {} page routes and {} API routes.", page_routes.len(), api_routes.len()));

    // 3. Full Recursive Static Code Audit & Deterministic Self-Heal across the ENTIRE `src/` tree
    // (all pages, components, modals, hooks, CSS, and layouts)
    let mut auto_healed_count = 0;
    let mut prose_leak_files: Vec<String> = Vec::new();
    let mut client_prisma_files: Vec<String> = Vec::new();
    let mut bare_src_import_files: Vec<String> = Vec::new();

    let src_dir = proj_dir.join("src");
    if src_dir.exists() {
        run_hygiene_pass_on_project(proj_dir);

        fn audit_and_heal_dir(
            dir: &std::path::Path,
            proj_dir: &std::path::Path,
            healed_count: &mut usize,
            prose_files: &mut Vec<String>,
            client_prisma: &mut Vec<String>,
            bare_src: &mut Vec<String>,
            window: &tauri::Window,
        ) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        audit_and_heal_dir(&p, proj_dir, healed_count, prose_files, client_prisma, bare_src, window);
                    } else if p.is_file() {
                        let path_str = p.to_string_lossy().replace('\\', "/");
                        let rel_str = p
                            .strip_prefix(proj_dir)
                            .map(|r| r.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_else(|_| path_str.clone());
                        let is_code_file = path_str.ends_with(".tsx") || path_str.ends_with(".jsx") || path_str.ends_with(".ts") || path_str.ends_with(".js");
                        if is_code_file {
                            if let Ok(content) = std::fs::read_to_string(&p) {
                                let mut updated = crate::sanitizer::normalize_bare_local_imports(&content);
                                updated = crate::sanitizer::ensure_next_server_imports(&updated, &rel_str.to_lowercase());
                                
                                // Sanitize useEffect dependencies across all UI components and hooks
                                updated = crate::sanitizer::sanitize_use_effect_deps(&updated);

                                // If inside hooks directory or defines custom hooks, also memoize action functions
                                let rel_low = rel_str.to_lowercase();
                                if !crate::sanitizer::is_app_router_route_file(&rel_str)
                                    && !rel_low.contains("/components/")
                                    && !rel_low.starts_with("components/")
                                    && (rel_low.contains("/hooks/")
                                        || rel_low.starts_with("hooks/")
                                        || rel_low.contains("useauth")
                                        || rel_low.ends_with("hook.ts")
                                        || rel_low.ends_with("hook.tsx"))
                                {
                                    updated = crate::sanitizer::sanitize_custom_hook_functions(&updated);
                                }

                                // Ensure invalid 'react' navigation imports and missing hook imports are fixed
                                if !path_str.contains("/api/") && !path_str.ends_with("route.ts") && !path_str.ends_with("route.js") {
                                    updated = crate::sanitizer::ensure_react_and_next_hook_imports(&updated);
                                    updated = crate::sanitizer::hoist_use_client_directive(&updated);
                                }

                                // Check prose leak, client Prisma usage, or residual bare src/ imports
                                let lower_head: String = updated.chars().take(1500).collect::<String>().to_lowercase();
                                if updated.contains("The error indicates") || (lower_head.contains("1. ensure") && lower_head.contains("2. ")) {
                                    prose_files.push(rel_str.clone());
                                }
                                let is_client = updated.trim_start().starts_with("\"use client\"") || updated.trim_start().starts_with("'use client'");
                                if is_client && !rel_str.contains("/api/") && (updated.contains("@/lib/prisma") || updated.contains("new PrismaClient")) {
                                    client_prisma.push(rel_str.clone());
                                }
                                if updated.contains("from 'src/") || updated.contains("from \"src/") {
                                    bare_src.push(rel_str.clone());
                                }

                                if updated != content {
                                    let _ = std::fs::write(&p, &updated);
                                    *healed_count += 1;
                                    debug_log_to_file(format!("[Auto-Test Suite] Auto-healed: {}", rel_str));
                                    let _ = window.emit("orchestrator:progress", serde_json::json!({
                                        "phase": "auto-testing",
                                        "message": format!("> [Auto-Test] Ispravljene nestabilne zavisnosti u: {}", rel_str),
                                        "progress": 77.0
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
        audit_and_heal_dir(
            &src_dir,
            proj_dir,
            &mut auto_healed_count,
            &mut prose_leak_files,
            &mut client_prisma_files,
            &mut bare_src_import_files,
            window,
        );

        if !prose_leak_files.is_empty() {
            warnings.push(format!("detektovana proza u kodu ({})", prose_leak_files.join(", ")));
        }
        if !client_prisma_files.is_empty() {
            warnings.push(format!("nedozvoljen Prisma import u klijentskom fajlu ({})", client_prisma_files.join(", ")));
        }
        if !bare_src_import_files.is_empty() {
            warnings.push(format!("nekonvertovani 'src/' importi ({})", bare_src_import_files.join(", ")));
        }

        let mut remaining_missing = Vec::new();
        collect_missing_imports(&src_dir, &src_dir, &mut remaining_missing);
        if !remaining_missing.is_empty() {
            warnings.push(format!("nerazrešeni lokalni importi ({})", remaining_missing.len()));
        }
    }

    if proj_dir.join("package.json").exists() {
        let nm_dir = proj_dir.join("node_modules");
        let is_next_proj = proj_dir.join("next.config.js").exists() || proj_dir.join("next.config.mjs").exists();
        if !nm_dir.exists() || (is_next_proj && !nm_dir.join("next").exists()) {
            warnings.push("node_modules nije instaliran/kompletan".to_string());
        } else {
            let local_tsc_js = nm_dir.join("typescript").join("bin").join("tsc");
            if local_tsc_js.exists() {
                let enriched_path = ensure_node_toolchain_ready(None, "auto-test").await;
                if let Ok((tsc_ok, _, diags)) = run_tsc_check(proj_dir, &local_tsc_js, &enriched_path) {
                    if !tsc_ok && !diags.is_empty() {
                        warnings.push(format!("TypeScript provera prijavila {} grešaka", diags.len()));
                    } else if tsc_ok {
                        if let Some(proj_id) = proj_dir.file_name().and_then(|n| n.to_str()) {
                            crate::katastar::complete_pending_project_repairs(
                                proj_id,
                                None,
                                true,
                                "tsc_noemit",
                                _model_id,
                            );
                        }
                    }
                }
            }
        }
    }

    let has_prisma = proj_dir.join("prisma/schema.prisma").exists() || proj_dir.join("schema.prisma").exists();
    if has_prisma {
        let local_prisma_js = proj_dir.join("node_modules").join("prisma").join("build").join("index.js");
        if local_prisma_js.exists() {
            let mut val_cmd = std::process::Command::new("node");
            val_cmd.arg(&local_prisma_js).arg("validate").current_dir(proj_dir);
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                val_cmd.creation_flags(0x08000000);
            }
            match val_cmd.output() {
                Ok(out) if out.status.success() => {
                    if let Some(proj_id) = proj_dir.file_name().and_then(|n| n.to_str()) {
                        crate::katastar::complete_pending_project_repairs(
                            proj_id,
                            Some("prisma/schema.prisma"),
                            true,
                            "prisma_validate",
                            _model_id,
                        );
                    }
                }
                Ok(_) => warnings.push("prisma validate nije prošao".to_string()),
                Err(_) => warnings.push("nije moguće pokrenuti prisma validate".to_string()),
            }
        } else {
            warnings.push("Prisma CLI nije instaliran u node_modules".to_string());
        }
    }

    let total_routes = page_routes.len() + api_routes.len();
    let summary_msg = if warnings.is_empty() {
        format!(
            "> [Auto-Test Suite] Verifikovano {} ruta i endpoint-a (sanirano {} fajlova). Statičke, import i Prisma provere su prošle.",
            total_routes, auto_healed_count
        )
    } else {
        format!(
            "> [Auto-Test Suite] Verifikovano {} ruta i endpoint-a (sanirano {} fajlova), sa upozorenjima: {}.",
            total_routes,
            auto_healed_count,
            warnings.join("; ")
        )
    };
    let _ = window.emit("orchestrator:progress", serde_json::json!({
        "phase": "auto-testing",
        "message": summary_msg,
        "progress": 79.0
    }));
    debug_log_to_file(format!(
        "[Auto-Test Suite] Verification finished. {} total routes, {} healed files, warnings={:?}.",
        total_routes, auto_healed_count, warnings
    ));

    warnings
}

