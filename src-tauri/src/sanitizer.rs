use crate::*;

pub fn is_html_template_file(file_path: &str) -> bool {
    let path = std::path::Path::new(file_path);
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        matches!(
            ext_lower.as_str(),
            "html" | "htm" | "php" | "twig" | "jinja" | "jinja2" | "njk" | "hbs" | "ejs" | "vue" | "svelte" | "jsx" | "tsx"
        )
    } else {
        false
    }
}

pub fn inject_axiom_attrs(content: &str, relative_path: &str) -> String {
    // First repair any previously misplaced `data-axiom-*` attributes produced before UTF-8 byte-offset fix
    // (e.g. placed immediately before `<div` or split inside `<ta ... g>`).
    let mut normalized_content = content.to_string();
    if normalized_content.contains("data-axiom-component=") {
        if let Ok(split_re) = regex::Regex::new(
            r#"(<[A-Za-z_][A-Za-z0-9_.-]*)\s+data-axiom-component="([^"]*)"\s+data-axiom-file="([^"]*)"([A-Za-z0-9_.-]+)"#,
        ) {
            normalized_content = split_re
                .replace_all(
                    &normalized_content,
                    "$1$4 data-axiom-component=\"$2\" data-axiom-file=\"$3\"",
                )
                .to_string();
        }
        if let Ok(before_tag_re) = regex::Regex::new(
            r#"[ \t]?data-axiom-component="[^"]*"\s+data-axiom-file="[^"]*"(\s*<[A-Za-z_])"#,
        ) {
            normalized_content = before_tag_re.replace_all(&normalized_content, "$1").to_string();
        }
    }

    if normalized_content.contains("data-axiom-component=") {
        return normalized_content;
    }

    let path = std::path::Path::new(relative_path);
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Component");

    // PascalCase
    let mut component_name = String::new();
    let mut capitalize_next = true;
    for c in file_stem.chars() {
        if c == '-' || c == '_' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            component_name.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            component_name.push(c);
        }
    }
    if component_name.is_empty() {
        component_name = "Component".to_string();
    }

    let mut first_match_insert_byte: Option<usize> = None;

    // Collect (byte_offset, char) pairs so slicing uses true UTF-8 byte offsets
    // even when multi-byte characters (curly quotes, em-dashes, accents, emojis) appear earlier in the file.
    let chars: Vec<(usize, char)> = normalized_content.char_indices().collect();
    for i in 0..chars.len() {
        if chars[i].1 == '<' && i + 1 < chars.len() {
            // Preskoči ako je ispred (nakon preskakanja razmaka) alfanumerički karakter, tačka ili donja crta (indikacija TypeScript generika ili poređenja)
            let mut is_generic_or_compare = false;
            if i > 0 {
                let mut prev_idx = i - 1;
                while prev_idx > 0 && chars[prev_idx].1.is_ascii_whitespace() {
                    prev_idx -= 1;
                }
                let prev_c = chars[prev_idx].1;
                if prev_c.is_ascii_alphanumeric() || prev_c == '.' || prev_c == '_' {
                    let mut word_start = prev_idx;
                    while word_start > 0 {
                        let c = chars[word_start - 1].1;
                        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                            word_start -= 1;
                        } else {
                            break;
                        }
                    }
                    let word: String = chars[word_start..=prev_idx].iter().map(|(_, c)| *c).collect();
                    let keywords = ["return", "default", "yield", "await", "as", "typeof", "void", "delete"];
                    if !keywords.contains(&word.as_str()) {
                        is_generic_or_compare = true;
                    }
                }
            }
            if is_generic_or_compare {
                continue;
            }
            let next_c = chars[i + 1].1;
            if next_c.is_ascii_alphabetic() || next_c == '_' {
                let mut tag_name = String::new();
                let mut j = i + 1;
                while j < chars.len() && (chars[j].1.is_ascii_alphanumeric() || chars[j].1 == '-' || chars[j].1 == '_' || chars[j].1 == '.') {
                    tag_name.push(chars[j].1);
                    j += 1;
                }

                let is_context_provider_or_consumer = tag_name.ends_with(".Provider") || tag_name.ends_with(".Consumer");

                let is_valid_tag = if tag_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                    !is_context_provider_or_consumer
                        && tag_name != "ReactNode"
                        && tag_name != "ReactElement"
                        && tag_name != "FC"
                        && tag_name != "ComponentType"
                        && tag_name != "any"
                        && !tag_name.ends_with('.')
                } else {
                    let html_tags = vec![
                        "div", "section", "main", "header", "footer", "article", "aside",
                        "nav", "form", "ul", "ol", "table", "body", "html", "span", "p", "a", "h1", "h2", "h3", "h4", "h5", "h6"
                    ];
                    html_tags.contains(&tag_name.as_str())
                };

                if is_valid_tag {
                    let insert_byte = if j < chars.len() {
                        chars[j].0
                    } else {
                        normalized_content.len()
                    };
                    first_match_insert_byte = Some(insert_byte);
                    break;
                }
            }
        }
    }

    if let Some(insert_byte) = first_match_insert_byte {
        let mut new_content = String::with_capacity(normalized_content.len() + 96);
        new_content.push_str(&normalized_content[..insert_byte]);
        new_content.push_str(&format!(
            " data-axiom-component=\"{}\" data-axiom-file=\"{}\"",
            component_name, relative_path.replace('\\', "/")
        ));
        new_content.push_str(&normalized_content[insert_byte..]);
        new_content
    } else {
        normalized_content
    }
}

pub fn detect_project_type(manifest: &Value) -> String {
    if let Some(t) = manifest.get("projectType").and_then(|s| s.as_str()) {
        return t.to_lowercase();
    }
    if let Some(stack) = manifest.get("resolvedStack").and_then(|s| s.as_str()) {
        let stack_lower = stack.to_lowercase();
        if stack_lower.contains("next") {
            return "nextjs".to_string();
        }
        if stack_lower.contains("mern") || stack_lower.contains("express") || stack_lower.contains("mongodb") {
            return "mern".to_string();
        }
        if stack_lower.contains("electron") {
            return "electron".to_string();
        }
        if stack_lower.contains("rust") || stack_lower.contains("tauri") {
            return "rust".to_string();
        }
        if stack_lower.contains("flutter") {
            return "flutter".to_string();
        }
    }
    if let Some(p) = manifest.get("platform").and_then(|s| s.as_str()) {
        let p_lower = p.to_lowercase();
        if p_lower == "mobile" {
            return "flutter".to_string();
        }
        if p_lower == "desktop" {
            return "electron".to_string();
        }
    }
    "nextjs".to_string()
}

pub fn normalize_project_structure(files: &mut Vec<Value>, metamanifest: &Value, project_type: &str) {
    let root_files: Vec<String> = metamanifest
        .get("rootFiles")
        .and_then(|rf| rf.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();


    for file in files.iter_mut() {
        if let Some(path_str) = file.get("path").and_then(|p| p.as_str()) {
            let normalized = path_str.replace('\\', "/");
            let parts: Vec<&str> = normalized.split('/').collect();
            let filename = parts.last().map(|s| s.to_lowercase()).unwrap_or_default();
            let path_lower = normalized.to_lowercase();

            // 1. Move root configuration files to root
            if root_files.contains(&filename) {
                file["path"] = serde_json::json!(parts.last().unwrap());
                continue;
            }

            // 2. Normalize Prisma schema & seeds
            if filename == "schema.prisma" {
                file["path"] = serde_json::json!("prisma/schema.prisma");
                continue;
            }

            // Normalize Next.js App Router root layout path case-sensitivity
            if path_lower == "src/app/layout.tsx" || path_lower == "src/app/layout.jsx" || path_lower == "app/layout.tsx" || path_lower == "app/layout.jsx" {
                file["path"] = serde_json::json!("src/app/layout.tsx");
                continue;
            }
            if filename == "seed.ts" || filename == "seed.js" {
                if normalized.contains("/prisma/") || normalized.contains("prisma/") {
                    file["path"] = serde_json::json!(format!("prisma/{}", parts.last().unwrap()));
                    continue;
                }
            }

            // 3. Normalize common root files (gitignore, env, readme)
            if filename == ".gitignore" || filename == ".env.example" || filename == "readme.md" {
                file["path"] = serde_json::json!(parts.last().unwrap());
                continue;
            }

            // 4. Normalize Pages Router files to App Router if project is Next.js App Router
            if project_type == "nextjs" {
                let is_in_app = path_lower.starts_with("src/app/");
                let is_in_pages = path_lower.starts_with("src/app/pages/") || path_lower.starts_with("src/pages/") || path_lower.starts_with("pages/");
                
                if is_in_pages || is_in_app {
                    let last_part = parts.last().copied().unwrap_or("");
                    if !last_part.is_empty() {
                        let ext = last_part.rfind('.').map(|pos| &last_part[pos+1..]).unwrap_or("");
                        if ext == "tsx" || ext == "jsx" || ext == "js" {
                            let is_page_file = if is_in_pages {
                                true
                            } else {
                                parts.len() == 3
                            };
                            
                            if is_page_file {
                                let filename_no_ext = last_part.rfind('.').map(|pos| &last_part[..pos]).unwrap_or(last_part);
                                let filename_lower = filename_no_ext.to_lowercase();
                                
                                if filename_lower == "index" {
                                    file["path"] = serde_json::json!(format!("src/app/page.{}", ext));
                                    continue;
                                } else if filename_lower == "_app" || filename_lower == "layout" {
                                    file["path"] = serde_json::json!(format!("src/app/layout.{}", ext));
                                    continue;
                                } else if filename_lower != "page" && filename_lower != "layout" && filename_lower != "loading" && filename_lower != "error" && filename_lower != "not-found" {
                                    file["path"] = serde_json::json!(format!("src/app/{}/page.{}", filename_lower, ext));
                                    continue;
                                }
                            }
                        }
                    }
                }
            }

            // 5. Normalize API Route files to App Router
            if project_type == "nextjs" && path_lower.contains("src/app/api/") {
                let last_part = parts.last().unwrap();
                let ext = last_part.rfind('.').map(|pos| &last_part[pos+1..]).unwrap_or("");
                if ext == "ts" || ext == "js" {
                    let filename_no_ext = last_part.rfind('.').map(|pos| &last_part[..pos]).unwrap_or(last_part);
                    if filename_no_ext.to_lowercase() == "index" || filename_no_ext.to_lowercase() == "route" {
                        let dir_parts = &parts[0..parts.len()-1];
                        file["path"] = serde_json::json!(format!("{}/route.{}", dir_parts.join("/"), ext));
                    } else {
                        // It is a file-based route like api/assignments/[id].ts -> api/assignments/[id]/route.ts
                        let dir_parts = &parts[0..parts.len()-1];
                        file["path"] = serde_json::json!(format!("{}/{}/route.{}", dir_parts.join("/"), filename_no_ext, ext));
                    }
                    continue;
                }
            }
        }
    }

    let mut paths_present = std::collections::HashSet::new();
    for file in files.iter() {
        if let Some(path_str) = file.get("path").and_then(|p| p.as_str()) {
            paths_present.insert(path_str.replace('\\', "/").to_lowercase());
        }
    }

    if let Some(templates) = metamanifest.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
        for template in templates {
            if let Some(path_str) = template.get("path").and_then(|p| p.as_str()) {
                let path_lower = path_str.to_lowercase();
                if !paths_present.contains(&path_lower) {
                    files.push(serde_json::json!({
                        "path": path_str,
                        "description": template.get("description").and_then(|d| d.as_str()).unwrap_or(""),
                        "language": template.get("language").and_then(|l| l.as_str()).unwrap_or("txt")
                    }));
                }
            }
        }
    }

    deduplicate_manifest_files(files);
}

pub fn deduplicate_manifest_files(files: &mut Vec<Value>) {
    let mut seen_idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out: Vec<Value> = Vec::with_capacity(files.len());
    for mut item in files.drain(..) {
        if let Some(raw_path) = item.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()) {
            let clean_path = raw_path
                .trim()
                .replace('\\', "/")
                .trim_start_matches("./")
                .to_string();
            let norm = clean_path.to_lowercase();
            if !norm.is_empty() {
                if let Some(obj) = item.as_object_mut() {
                    obj.insert("path".to_string(), serde_json::json!(clean_path));
                }
                if let Some(&existing_idx) = seen_idx.get(&norm) {
                    if let (Some(existing_obj), Some(new_obj)) = (out[existing_idx].as_object_mut(), item.as_object()) {
                        for field in ["description", "language", "defaultContent", "role"] {
                            let existing_str = existing_obj
                                .get(field)
                                .and_then(|v| v.as_str())
                                .map(|s| s.trim())
                                .unwrap_or("");
                            if let Some(new_val) = new_obj.get(field) {
                                let new_str = new_val.as_str().map(|s| s.trim()).unwrap_or("");
                                if !new_str.is_empty() && (existing_str.is_empty() || new_str.len() > existing_str.len()) {
                                    existing_obj.insert(field.to_string(), new_val.clone());
                                }
                            }
                        }
                    }
                    continue;
                }
                seen_idx.insert(norm, out.len());
            }
        }
        out.push(item);
    }
    *files = out;
}

/// Prevents runaway local LLM manifest expansion bloat while preserving every file
/// originally requested in `original_files`:
/// - Drops foreign framework files (`.vue`, `.svelte`) in Next.js projects
/// - Canonicalizes expansion-only `src/app/{components,utils,config,hooks,lib,types,models,styles}/...`
///   paths back to `src/...` when the original manifest did not use `src/app/<folder>/`
/// - Caps expansion-added translation/locale JSON bundles so Ollama cannot spend 20 minutes
///   generating dozens of regional locale matrices (e.g. `zh-AT.json` .. `zh-VN.json`).
pub fn filter_runaway_manifest_expansion(
    original_files: &[Value],
    expanded_files: Vec<Value>,
    project_type: &str,
) -> Vec<Value> {
    let mut orig_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut orig_nested_app_folders: std::collections::HashSet<String> = std::collections::HashSet::new();
    const NON_ROUTE_FOLDERS: &[&str] = &[
        "components", "utils", "config", "hooks", "lib", "types", "models", "styles",
    ];

    for f in original_files {
        if let Some(p) = f.get("path").and_then(|v| v.as_str()) {
            let norm = p.trim().replace('\\', "/").trim_start_matches("./").to_lowercase();
            if !norm.is_empty() {
                for folder in NON_ROUTE_FOLDERS {
                    let prefix = format!("src/app/{}/", folder);
                    if norm.starts_with(&prefix) {
                        orig_nested_app_folders.insert((*folder).to_string());
                    }
                }
                orig_paths.insert(norm);
            }
        }
    }

    let mut out: Vec<Value> = Vec::with_capacity(expanded_files.len());
    let mut added_locale_json_count = 0usize;

    for mut item in expanded_files {
        let Some(raw_path) = item.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()) else {
            out.push(item);
            continue;
        };
        let mut clean = raw_path.trim().replace('\\', "/").trim_start_matches("./").to_string();
        let norm_low = clean.to_lowercase();
        if norm_low.is_empty() {
            continue;
        }

        if orig_paths.contains(&norm_low) {
            out.push(item);
            continue;
        }

        if project_type == "nextjs" {
            if norm_low.ends_with(".vue") || norm_low.ends_with(".svelte") {
                continue;
            }

            let is_locale_json = norm_low.ends_with(".json")
                && (norm_low.contains("/translations/")
                    || norm_low.starts_with("translations/")
                    || norm_low.contains("/locales/")
                    || norm_low.starts_with("locales/")
                    || norm_low.contains("/messages/")
                    || norm_low.starts_with("messages/")
                    || norm_low.contains("/i18n/"));
            if is_locale_json {
                added_locale_json_count += 1;
                if added_locale_json_count > 2 {
                    continue;
                }
            }

            for folder in NON_ROUTE_FOLDERS {
                if !orig_nested_app_folders.contains(*folder) {
                    let bad_prefix = format!("src/app/{}/", folder);
                    if clean.len() >= bad_prefix.len() && norm_low.starts_with(&bad_prefix) {
                        clean = format!("src/{}/{}", folder, &clean[bad_prefix.len()..]);
                        if let Some(obj) = item.as_object_mut() {
                            obj.insert("path".to_string(), serde_json::json!(clean));
                        }
                        break;
                    }
                }
            }
        }

        out.push(item);
    }

    out
}

pub fn get_axiom_inspector_script() -> &'static str {
    "      <script dangerouslySetInnerHTML={{ __html: `\n        if (!window.__axiom_inspect_injected) {\n          window.__axiom_inspect_injected = true;\n          let isInspectActive = false;\n          let lastReportedOverlaySig = '';\n          function extractNextjsOverlay() {\n            try {\n              const portal = document.querySelector('nextjs-portal');\n              const root = (portal && portal.shadowRoot) ? portal.shadowRoot : document;\n              const dialog = root.querySelector('[data-nextjs-dialog], [role=\"dialog\"], #nextjs__container_errors_label');\n              let rawText = dialog ? (dialog.innerText || dialog.textContent || '').trim() : '';\n              if (!rawText && root !== document) {\n                rawText = Array.from(root.children || []).filter(el => el.tagName !== 'STYLE' && el.tagName !== 'SCRIPT').map(el => el.innerText || el.textContent || '').join(String.fromCharCode(10)).trim();\n              }\n              if (!rawText || (!rawText.includes('Error') && !rawText.includes('Failed to compile') && !rawText.includes('Module not found'))) return null;\n              const normText = rawText.split(String.fromCharCode(92)).join('/');\n              const fileMatch = normText.match(new RegExp('(?:src/|app/|pages/|components/)[A-Za-z0-9_/.+-]+[.](?:tsx|ts|jsx|js)'));\n              const sourceFile = fileMatch ? fileMatch[0] : null;\n              const lines = normText.split(String.fromCharCode(10)).map(l => l.trim()).filter(l => l && !l.startsWith(':host') && !l.startsWith('/*!'));\n              const errLine = lines.find(l => l.includes('Module not found:') || l.includes('Error:') || l.includes('Failed to compile')) || lines[0] || '';\n              const message = errLine.substring(0, 220);\n              return { hasError: true, message, sourceFile, rawText: rawText.substring(0, 1200) };\n            } catch (e) { return null; }\n          }\n          function collectAxiomUiSnapshot() {\n            try {\n              const isVis = (el) => { if (!el) return false; const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0; };\n              const headings = Array.from(document.querySelectorAll('h1, h2, h3')).slice(0, 20).map(h => ({ tag: h.tagName.toLowerCase(), text: (h.innerText || '').trim().substring(0, 120) })).filter(h => h.text);\n              const buttons = Array.from(document.querySelectorAll('button, [role=\"button\"]')).slice(0, 35).map(b => ({ text: (b.innerText || '').trim().substring(0, 80), ariaLabel: b.getAttribute('aria-label') || '', disabled: !!b.disabled, visible: isVis(b) }));\n              const links = Array.from(document.querySelectorAll('a')).slice(0, 35).map(a => ({ text: (a.innerText || '').trim().substring(0, 80), href: a.getAttribute('href') || '', visible: isVis(a) }));\n              const inputs = Array.from(document.querySelectorAll('input, select, textarea')).slice(0, 25).map(i => ({ tag: i.tagName.toLowerCase(), type: i.getAttribute('type') || '', name: i.getAttribute('name') || '', placeholder: i.getAttribute('placeholder') || '', value: (i.value || '').substring(0, 80) }));\n              const overlay = extractNextjsOverlay();\n              const bodyText = (document.body ? document.body.innerText : '').trim();\n              return {\n                url: window.location.href,\n                pathname: window.location.pathname,\n                title: document.title || '',\n                nextjsOverlay: overlay,\n                headings,\n                buttons,\n                links,\n                inputs,\n                layout: {\n                  viewportWidth: window.innerWidth,\n                  viewportHeight: window.innerHeight,\n                  scrollWidth: document.documentElement ? document.documentElement.scrollWidth : 0,\n                  scrollHeight: document.documentElement ? document.documentElement.scrollHeight : 0,\n                  hasHorizontalOverflow: (document.documentElement ? document.documentElement.scrollWidth : 0) > window.innerWidth + 4,\n                  hasNavbar: !!document.querySelector('nav, header'),\n                  hasFooter: !!document.querySelector('footer'),\n                  visibleTextPreview: bodyText.substring(0, 600)\n                }\n              };\n            } catch (e) { return { error: String(e) }; }\n          }\n          function reportAxiomRoute() {\n            try {\n              const overlay = extractNextjsOverlay();\n              if (overlay && overlay.hasError) {\n                const sig = (overlay.sourceFile || '') + '::' + overlay.message;\n                if (sig !== lastReportedOverlaySig) {\n                  lastReportedOverlaySig = sig;\n                  window.parent.postMessage({ channel: 'axiom-preview-error', message: overlay.message, sourceFile: overlay.sourceFile, details: overlay.rawText, path: window.location.pathname }, '*');\n                }\n              }\n              window.parent.postMessage({ channel: 'axiom-preview-route', pathname: window.location.pathname, url: window.location.href, snapshot: collectAxiomUiSnapshot() }, '*');\n            } catch(e) {}\n          }\n          setTimeout(reportAxiomRoute, 400);\n          window.addEventListener('popstate', reportAxiomRoute);\n          setInterval(reportAxiomRoute, 2000);\n          window.addEventListener('error', function(e) { try { window.parent.postMessage({ channel: 'axiom-preview-error', message: e.message || 'Unknown runtime error', sourceFile: e.filename || null, path: window.location.pathname }, '*'); } catch(err) {} });\n          window.addEventListener('unhandledrejection', function(e) { try { window.parent.postMessage({ channel: 'axiom-preview-error', message: e.reason?.message || String(e.reason || 'Unhandled promise rejection'), path: window.location.pathname }, '*'); } catch(err) {} });\n          window.addEventListener('message', (e) => {\n            if (e.data && e.data.channel === 'axiom-toggle-inspect') {\n              isInspectActive = !!e.data.enabled;\n              if (!isInspectActive && highlightedElement) {\n                highlightedElement.classList.remove('axiom-inspect-highlight');\n                highlightedElement = null;\n              }\n            }\n            if (e.data && e.data.channel === 'axiom-agent-dom-cmd') {\n              const cmd = e.data;\n              try {\n                if (cmd.action === 'snapshot') {\n                  window.parent.postMessage({ channel: 'axiom-ui-snapshot', cmdId: cmd.cmdId, actionResult: 'ok', snapshot: collectAxiomUiSnapshot() }, '*');\n                } else if (cmd.action === 'click') {\n                  let el = cmd.selector ? document.querySelector(cmd.selector) : null;\n                  if (!el && cmd.text) {\n                    const needle = cmd.text.toLowerCase();\n                    const candidates = Array.from(document.querySelectorAll('button, a, [role=\"button\"], li, span'));\n                    el = candidates.find(c => ((c.innerText || '').trim().toLowerCase().includes(needle)) || ((c.getAttribute('aria-label') || '').toLowerCase().includes(needle)));\n                  }\n                  if (el) {\n                    el.click();\n                    setTimeout(() => {\n                      window.parent.postMessage({ channel: 'axiom-ui-snapshot', cmdId: cmd.cmdId, actionResult: 'clicked: ' + (el.innerText || el.tagName).trim().substring(0, 50), snapshot: collectAxiomUiSnapshot() }, '*');\n                    }, 600);\n                  } else {\n                    window.parent.postMessage({ channel: 'axiom-ui-snapshot', cmdId: cmd.cmdId, actionResult: 'element_not_found', snapshot: collectAxiomUiSnapshot() }, '*');\n                  }\n                } else if (cmd.action === 'type') {\n                  let el = cmd.selector ? document.querySelector(cmd.selector) : null;\n                  if (!el && cmd.placeholder) {\n                    const needle = cmd.placeholder.toLowerCase();\n                    el = Array.from(document.querySelectorAll('input, textarea')).find(i => (i.getAttribute('placeholder') || '').toLowerCase().includes(needle) || (i.getAttribute('name') || '').toLowerCase().includes(needle));\n                  }\n                  if (el) {\n                    const proto = el.tagName === 'TEXTAREA' ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement.prototype;\n                    const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;\n                    if (setter) setter.call(el, cmd.value || ''); else el.value = cmd.value || '';\n                    el.dispatchEvent(new Event('input', { bubbles: true }));\n                    el.dispatchEvent(new Event('change', { bubbles: true }));\n                    setTimeout(() => {\n                      window.parent.postMessage({ channel: 'axiom-ui-snapshot', cmdId: cmd.cmdId, actionResult: 'typed', snapshot: collectAxiomUiSnapshot() }, '*');\n                    }, 300);\n                  } else {\n                    window.parent.postMessage({ channel: 'axiom-ui-snapshot', cmdId: cmd.cmdId, actionResult: 'input_not_found', snapshot: collectAxiomUiSnapshot() }, '*');\n                  }\n                } else if (cmd.action === 'navigate' && cmd.route) {\n                  window.location.href = cmd.route;\n                }\n              } catch (err) {\n                window.parent.postMessage({ channel: 'axiom-ui-snapshot', cmdId: cmd.cmdId, actionResult: 'error: ' + String(err), snapshot: collectAxiomUiSnapshot() }, '*');\n              }\n            }\n          });\n          const styleEl = document.createElement('style');\n          styleEl.textContent = '.axiom-inspect-highlight { outline: 2px solid #6366f1 !important; outline-offset: -2px !important; cursor: crosshair !important; background-color: rgba(99, 102, 241, 0.12) !important; transition: all 0.1s ease; }';\n          document.head.appendChild(styleEl);\n          let highlightedElement = null;\n          function findAxiomTarget(el) {\n            if (!el || el === document.body || el === document.documentElement) return null;\n            let cur = el;\n            while (cur && cur !== document.body && cur !== document.documentElement) {\n              if (cur.dataset && (cur.dataset.axiomFile || cur.dataset.axiomComponent)) {\n                return cur;\n              }\n              cur = cur.parentElement;\n            }\n            const validTags = ['h1','h2','h3','h4','h5','h6','p','button','a','input','textarea','select','header','footer','nav','section','article','aside','li'];\n            cur = el;\n            while (cur && cur !== document.body && cur !== document.documentElement) {\n              const tag = cur.tagName.toLowerCase();\n              if (validTags.includes(tag) || (cur.innerText && cur.innerText.trim().length > 0 && cur.children.length === 0)) {\n                return cur;\n              }\n              cur = cur.parentElement;\n            }\n            return el !== document.body && el !== document.documentElement ? el : null;\n          }\n          document.addEventListener('mouseover', (e) => {\n            if (!isInspectActive && !e.altKey) return;\n            const target = findAxiomTarget(e.target);\n            if (highlightedElement && highlightedElement !== target) {\n              highlightedElement.classList.remove('axiom-inspect-highlight');\n            }\n            if (target) {\n              target.classList.add('axiom-inspect-highlight');\n              highlightedElement = target;\n            }\n          }, true);\n          document.addEventListener('mouseout', (e) => {\n            if (highlightedElement) {\n              highlightedElement.classList.remove('axiom-inspect-highlight');\n              highlightedElement = null;\n            }\n          }, true);\n          document.addEventListener('click', (e) => {\n            if (!isInspectActive && !e.altKey) return;\n            const target = findAxiomTarget(e.target);\n            if (target) {\n              e.preventDefault();\n              e.stopPropagation();\n              const payload = {\n                file: target.dataset?.axiomFile || null,\n                component: target.dataset?.axiomComponent || null,\n                tagName: target.tagName.toLowerCase(),\n                text: (target.innerText || target.value || '').trim().substring(0, 80),\n                path: window.location.pathname\n              };\n              window.parent.postMessage({ channel: 'axiom-inspect-click', payload }, '*');\n            }\n          }, true);\n        }\n      ` }} />"
}

pub fn strip_axiom_inspector_script(content: &str) -> String {
    let mut cleaned = content.to_string();
    let start_marker = "<script dangerouslySetInnerHTML={{ __html: `";
    while let Some(s_idx) = cleaned.find(start_marker) {
        if let Some(end_rel) = cleaned[s_idx..].find("` }} />") {
            cleaned.replace_range(s_idx..s_idx + end_rel + 7, "");
        } else {
            break;
        }
    }
    cleaned
}

pub fn ensure_axiom_inspector_injected(proj_dir: &std::path::Path) {
    let candidate_layouts = [
        "src/app/[locale]/layout.tsx",
        "src/app/[locale]/layout.jsx",
        "src/app/layout.tsx",
        "src/app/layout.jsx",
        "app/layout.tsx",
        "app/layout.jsx",
        "src/pages/_app.tsx",
        "src/pages/_app.jsx",
        "pages/_app.tsx",
        "pages/_app.jsx",
    ];

    let script = get_axiom_inspector_script();

    for rel_path in &candidate_layouts {
        let full_path = proj_dir.join(rel_path);
        if full_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                let needs_upgrade = !content.contains("axiom-agent-dom-cmd")
                    || content.contains("rawText.match(/(?:")
                    || content.contains("styleEl.textContent = \\`")
                    || content.contains("const rawText = (root !== document && root.textContent)");
                if needs_upgrade {
                    let cleaned = strip_axiom_inspector_script(&content);

                    let new_content = if cleaned.contains("</body>") {
                        cleaned.replace("</body>", &format!("{}\n      </body>", script))
                    } else if cleaned.contains("</html>") {
                        cleaned.replace("</html>", &format!("{}\n</html>", script))
                    } else {
                        format!("{}\n// Axiom Inspect\n{}", cleaned, script)
                    };
                    let _ = std::fs::write(&full_path, new_content);
                }
            }
            break;
        }
    }
}

pub fn ensure_directory_barrel_files(proj_dir: &std::path::Path) {
    let src_dir = proj_dir.join("src");
    let has_src = src_dir.exists();
    let has_root_layout = !has_src
        && (proj_dir.join("app").exists()
            || proj_dir.join("components").exists()
            || proj_dir.join("hooks").exists()
            || proj_dir.join("lib").exists());
    if !has_src && !has_root_layout {
        return;
    }
    let alias_root = if has_src { src_dir.clone() } else { proj_dir.to_path_buf() };

    // Demand-driven scan: find any local import or re-export specifier (`@/...`, `./...`, `../...`)
    // that resolves to a barrel directory (either missing index.* or having a broken index.* re-export)
    let import_re = match regex::Regex::new(
        r#"(?ms)(?:^|[;\n])[ \t]*(?:import|export)\s+(?:type\s+)?(?:(?:[A-Za-z0-9_$]+\s*,\s*)?\{([^}]+)\}|[A-Za-z0-9_$*\s]+)\s+from\s+['"](@/[^'"]+|\.\.?/[^'"]+)['"]"#
    ) {
        Ok(r) => r,
        Err(_) => return,
    };

    let mut demanded_dirs: std::collections::BTreeMap<std::path::PathBuf, std::collections::BTreeSet<String>> = std::collections::BTreeMap::new();

    // Seed standard existing directories if they already have an index.* file so dead re-exports are repaired even if unreferenced
    for std_dir_name in ["components", "hooks", "types", "utils", "lib", "models", "services", "context", "contexts"] {
        let candidate_dir = alias_root.join(std_dir_name);
        if candidate_dir.is_dir()
            && (candidate_dir.join("index.ts").is_file()
                || candidate_dir.join("index.tsx").is_file()
                || candidate_dir.join("index.js").is_file())
        {
            demanded_dirs.entry(normalize_rel_fs_components(&candidate_dir)).or_default();
        }
    }

    fn scan_demanded_barrels(
        dir: &std::path::Path,
        src_root: &std::path::Path,
        re: &regex::Regex,
        demanded: &mut std::collections::BTreeMap<std::path::PathBuf, std::collections::BTreeSet<String>>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules" && name != ".next" && name != ".git" && name != ".axiom" && name != "prisma" {
                        scan_demanded_barrels(&p, src_root, re, demanded);
                    }
                } else if p.is_file() {
                    let s = p.to_string_lossy();
                    if s.ends_with(".ts") || s.ends_with(".tsx") || s.ends_with(".js") || s.ends_with(".jsx") {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            for cap in re.captures_iter(&content) {
                                let spec = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                                let target_dir = if let Some(sub) = spec.strip_prefix("@/") {
                                    normalize_rel_fs_components(&src_root.join(sub))
                                } else if let Some(parent) = p.parent() {
                                    normalize_rel_fs_components(&parent.join(spec))
                                } else {
                                    continue;
                                };

                                // Only trigger if `target_dir` has no file counterpart (e.g. `src/hooks.ts`)
                                let has_file_counterpart = target_dir.with_extension("ts").is_file()
                                    || target_dir.with_extension("tsx").is_file()
                                    || target_dir.with_extension("js").is_file()
                                    || target_dir.with_extension("jsx").is_file();

                                let is_standard_barrel_dir = matches!(
                                    spec,
                                    "@/hooks" | "@/types" | "@/utils" | "@/lib" | "@/components" | "@/context" | "@/contexts" | "@/services" | "@/constants"
                                );

                                if !has_file_counterpart {
                                    if !target_dir.is_dir() && is_standard_barrel_dir {
                                        let _ = std::fs::create_dir_all(&target_dir);
                                    }

                                    if target_dir.is_dir() {
                                        let entry_set = demanded.entry(target_dir).or_default();
                                        if let Some(named_list) = cap.get(1) {
                                            for item in named_list.as_str().split(',') {
                                                let clean = item
                                                    .trim()
                                                    .trim_start_matches("type ")
                                                    .split_whitespace()
                                                    .next()
                                                    .unwrap_or("")
                                                    .trim();
                                                if !clean.is_empty() {
                                                    entry_set.insert(clean.to_string());
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
    }

    if has_src {
        scan_demanded_barrels(&src_dir, &alias_root, &import_re, &mut demanded_dirs);
    } else {
        for sub_name in ["app", "components", "hooks", "lib", "utils", "models", "types", "services"] {
            let sub_dir = proj_dir.join(sub_name);
            if sub_dir.is_dir() {
                scan_demanded_barrels(&sub_dir, &alias_root, &import_re, &mut demanded_dirs);
            }
        }
    }

    let barrel_reexport_re = regex::Regex::new(
        r#"(?ms)(?:^|\n)[ \t]*(?:export|import)\s+(?:type\s+)?(?:\*\s*(?:as\s+[A-Za-z0-9_$]+\s*)?|\{([^}]+)\}|[A-Za-z0-9_$]+(?:\s*,\s*\{([^}]+)\})?)\s+from\s+['"](\.\.?/[^'"]+)['"]\s*;?"#
    ).ok();

    for (dir_path, demanded_symbols) in demanded_dirs {
        let index_ts = dir_path.join("index.ts");
        let index_tsx = dir_path.join("index.tsx");
        let index_js = dir_path.join("index.js");

        let mut stems: Vec<String> = Vec::new();
        let mut siblings: Vec<(String, String)> = Vec::new();
        let mut combined_existing_content = String::new();
        if let Ok(entries) = std::fs::read_dir(&dir_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        if (fname.ends_with(".ts") || fname.ends_with(".tsx"))
                            && !fname.starts_with("index.")
                            && !fname.ends_with(".d.ts")
                        {
                            let stem = fname
                                .trim_end_matches(".tsx")
                                .trim_end_matches(".ts")
                                .to_string();
                            let c = std::fs::read_to_string(&p).unwrap_or_default();
                            combined_existing_content.push_str(&c);
                            combined_existing_content.push('\n');
                            if !stem.is_empty() {
                                stems.push(stem.clone());
                                siblings.push((stem, c));
                            }
                        }
                    }
                } else if p.is_dir() {
                    if let Some(dname) = p.file_name().and_then(|n| n.to_str()) {
                        if dname != "node_modules" && dname != ".next" {
                            let sub_idx = if p.join("index.tsx").is_file() {
                                Some(p.join("index.tsx"))
                            } else if p.join("index.ts").is_file() {
                                Some(p.join("index.ts"))
                            } else {
                                None
                            };
                            if let Some(sub_p) = sub_idx {
                                let c = std::fs::read_to_string(&sub_p).unwrap_or_default();
                                combined_existing_content.push_str(&c);
                                combined_existing_content.push('\n');
                                stems.push(dname.to_string());
                                siblings.push((dname.to_string(), c));
                            }
                        }
                    }
                }
            }
        }
        stems.sort();
        siblings.sort_by(|a, b| a.0.cmp(&b.0));

        let existing_index_path = if index_ts.is_file() {
            Some(index_ts.clone())
        } else if index_tsx.is_file() {
            Some(index_tsx.clone())
        } else if index_js.is_file() {
            Some(index_js.clone())
        } else {
            None
        };

        let dir_name = dir_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_component_dir = dir_name.eq_ignore_ascii_case("components") || dir_name.eq_ignore_ascii_case("ui");

        if let Some(existing_path) = existing_index_path {
            let Ok(existing_content) = std::fs::read_to_string(&existing_path) else {
                continue;
            };
            let mut repaired = existing_content.clone();
            let mut had_broken = false;
            let mut dead_spans: Vec<(usize, usize)> = Vec::new();
            let mut broken_syms: Vec<String> = Vec::new();
            let mut active_reexport_specs: Vec<String> = Vec::new();

            if let Some(ref re) = barrel_reexport_re {
                for cap in re.captures_iter(&existing_content) {
                    let Some(full_m) = cap.get(0) else { continue };
                    let spec = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                    let target_abs = normalize_rel_fs_components(&dir_path.join(spec));
                    let resolves = target_abs.is_file()
                        || target_abs.with_extension("ts").is_file()
                        || target_abs.with_extension("tsx").is_file()
                        || target_abs.with_extension("js").is_file()
                        || target_abs.with_extension("jsx").is_file()
                        || target_abs.join("index.ts").is_file()
                        || target_abs.join("index.tsx").is_file()
                        || target_abs.join("index.js").is_file()
                        || target_abs.join("index.jsx").is_file();

                    if resolves {
                        active_reexport_specs.push(spec.trim_start_matches("./").to_string());
                    } else {
                        had_broken = true;
                        dead_spans.push((full_m.start(), full_m.end()));
                        if let Some(braced) = cap.get(1).or_else(|| cap.get(2)) {
                            for raw in braced.as_str().split(',') {
                                let part = raw.trim().trim_start_matches("type ").trim();
                                if let Some((_, alias)) = part.split_once(" as ") {
                                    let a = alias.trim();
                                    if !a.is_empty() && !broken_syms.contains(&a.to_string()) {
                                        broken_syms.push(a.to_string());
                                    }
                                } else {
                                    let clean = part.split_whitespace().next().unwrap_or("");
                                    if !clean.is_empty() && !broken_syms.contains(&clean.to_string()) {
                                        broken_syms.push(clean.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if !dead_spans.is_empty() {
                dead_spans.sort_by_key(|(s, _)| *s);
                let mut rebuilt = String::with_capacity(existing_content.len());
                let mut cursor = 0usize;
                for (s, e) in dead_spans {
                    if s >= cursor && e <= existing_content.len() {
                        rebuilt.push_str(&existing_content[cursor..s]);
                        cursor = e;
                    }
                }
                if cursor < existing_content.len() {
                    rebuilt.push_str(&existing_content[cursor..]);
                }
                repaired = rebuilt.trim().to_string();
            }

            // Collect needed symbols: both demanded external imports AND any symbols rescued from broken lines
            let mut needed_syms: std::collections::BTreeSet<String> = demanded_symbols.clone();
            for bs in broken_syms {
                needed_syms.insert(bs);
            }

            // Check whether each needed symbol is already covered by valid lines or active sibling wildcard re-exports
            let mut uncovered_syms: Vec<String> = Vec::new();
            for sym in &needed_syms {
                let covered_inline = has_named_export_symbol(&repaired, sym);
                let covered_via_active = active_reexport_specs.iter().any(|stem| {
                    siblings
                        .iter()
                        .find(|(s, _)| s == stem)
                        .map_or(false, |(_, content)| has_named_export_symbol(content, sym))
                });
                if !covered_inline && !covered_via_active {
                    uncovered_syms.push(sym.clone());
                }
            }

            if had_broken || !uncovered_syms.is_empty() {
                let mut appended_lines: Vec<String> = Vec::new();
                let mut handled_stems: std::collections::BTreeSet<String> = active_reexport_specs.into_iter().collect();

                // Match uncovered symbols to real sibling files in this directory
                let mut still_missing_syms: Vec<String> = Vec::new();
                for sym in uncovered_syms {
                    if let Some((sib_stem, sib_content)) = siblings
                        .iter()
                        .find(|(s, content)| s == &sym || has_named_export_symbol(content, &sym))
                    {
                        if handled_stems.insert(sib_stem.clone()) {
                            let has_named_or_types = has_named_export_symbol(sib_content, sib_stem)
                                || sib_content.contains("export const ")
                                || sib_content.contains("export function ")
                                || sib_content.contains("export interface ")
                                || sib_content.contains("export type ")
                                || sib_content.contains("export {");
                            if !has_named_export_symbol(sib_content, sib_stem)
                                && !has_named_or_types
                                && sib_content.contains("export default")
                            {
                                appended_lines.push(format!("export {{ default as {} }} from './{}';", sib_stem, sib_stem));
                            }
                            let star_line = format!("export * from './{}';", sib_stem);
                            if !repaired.contains(&star_line) && !appended_lines.contains(&star_line) {
                                appended_lines.push(star_line);
                            }
                        }
                    } else {
                        still_missing_syms.push(sym);
                    }
                }

                // If the barrel had broken lines and now looks sparse, also re-export existing siblings
                if had_broken {
                    for (sib_stem, sib_content) in &siblings {
                        if handled_stems.insert(sib_stem.clone()) {
                            let has_named = has_named_export_symbol(sib_content, sib_stem)
                                || sib_content.contains("export interface ")
                                || sib_content.contains("export type ");
                            if !has_named && sib_content.contains("export default") {
                                let def_line = format!("export {{ default as {} }} from './{}';", sib_stem, sib_stem);
                                if !repaired.contains(&def_line) && !appended_lines.contains(&def_line) {
                                    appended_lines.push(def_line);
                                }
                            }
                            let star_line = format!("export * from './{}';", sib_stem);
                            if !repaired.contains(&star_line) && !appended_lines.contains(&star_line) {
                                appended_lines.push(star_line);
                            }
                        }
                    }
                }

                for sym in still_missing_syms {
                    if !has_named_export_symbol(&repaired, &sym) && !stems.contains(&sym) {
                        let stub = if is_component_dir && sym.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                            format!("export const {}: any = () => null;", sym)
                        } else if sym.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                            format!("export type {} = any;", sym)
                        } else {
                            format!("export const {}: any = () => ({{}} as any);", sym)
                        };
                        appended_lines.push(stub);
                    }
                }

                if !appended_lines.is_empty() {
                    if !repaired.is_empty() && !repaired.ends_with('\n') {
                        repaired.push('\n');
                    }
                    repaired.push_str(&appended_lines.join("\n"));
                }

                if repaired.trim().is_empty() {
                    repaired = "export {};".to_string();
                }

                if repaired != existing_content {
                    let out = if repaired.ends_with('\n') {
                        repaired
                    } else {
                        format!("{}\n", repaired)
                    };
                    let _ = std::fs::write(&existing_path, out);
                }
            }
            continue;
        }

        let mut lines = Vec::new();
        for (stem, file_content) in &siblings {
            let has_named = has_named_export_symbol(file_content, stem)
                || file_content.contains("export interface ")
                || file_content.contains("export type ");
            if !has_named && file_content.contains("export default") {
                lines.push(format!("export {{ default as {} }} from './{}';", stem, stem));
            }
            lines.push(format!("export * from './{}';", stem));
        }

        // For any demanded symbol not exported in any of its files:
        // PascalCase in components -> safe null component (`export const Foo: any = () => null;`)
        // PascalCase in types/other -> safe type alias (`export type Foo = any;`)
        // camelCase / other -> safe stub hook or function (`export const useFoo: any = () => ({} as any);`)
        for sym in demanded_symbols {
            if stems.contains(&sym) {
                continue;
            }
            let is_defined = combined_existing_content.contains(&format!("interface {}", sym))
                || combined_existing_content.contains(&format!("type {} =", sym))
                || combined_existing_content.contains(&format!("type {}=", sym))
                || combined_existing_content.contains(&format!("enum {}", sym))
                || combined_existing_content.contains(&format!("const {}", sym))
                || combined_existing_content.contains(&format!("function {}", sym))
                || combined_existing_content.contains(&format!("class {}", sym));
            if !is_defined {
                if is_component_dir && sym.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                    lines.push(format!("export const {}: any = () => null;", sym));
                } else if sym.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                    lines.push(format!("export type {} = any;", sym));
                } else {
                    lines.push(format!("export const {}: any = () => ({{}} as any);", sym));
                }
            }
        }

        if lines.is_empty() {
            lines.push("export {};".to_string());
        }

        let _ = std::fs::write(&index_ts, format!("{}\n", lines.join("\n")));
    }
}

pub fn has_named_export_symbol(content: &str, sym: &str) -> bool {
    if sym.trim().is_empty() {
        return false;
    }
    let esc = regex::escape(sym.trim());
    let decl_pat = format!(
        r"(?m)\bexport\s+(?:async\s+)?(?:function|const|let|var|class|interface|type|enum)\s+{}\b",
        esc
    );
    if regex::Regex::new(&decl_pat)
        .map(|re| re.is_match(content))
        .unwrap_or(false)
    {
        return true;
    }
    let braced_pat = format!(r"(?s)\bexport\s*(?:type\s+)?\{{[^}}]*\b{}\b[^}}]*\}}", esc);
    regex::Regex::new(&braced_pat)
        .map(|re| re.is_match(content))
        .unwrap_or(false)
}

pub fn normalize_bare_local_imports(content: &str) -> String {
    let mut changed = false;
    let mut out_lines: Vec<String> = Vec::with_capacity(content.lines().count() + 1);
    let prisma_re = regex::Regex::new(r#"(['"])(?:\.\./)+lib/prisma(['"])|(['"])(?:\.\./)+prisma(['"])|(['"])@/prisma(['"])"#).ok();

    for line in content.lines() {
        let mut cur = line.to_string();
        let trimmed = cur.trim();
        let is_import_line = trimmed.starts_with("import ")
            || trimmed.starts_with("import{")
            || trimmed.starts_with("import type")
            || (trimmed.starts_with("export ") && trimmed.contains(" from "))
            || trimmed.contains("require(")
            || trimmed.contains("require.resolve(");

        if is_import_line {
            if let Some(ref re) = prisma_re {
                let updated = re.replace_all(&cur, |caps: &regex::Captures| {
                    let q = caps.get(1)
                        .or_else(|| caps.get(3))
                        .or_else(|| caps.get(5))
                        .map(|m| m.as_str())
                        .unwrap_or("'");
                    format!("{}@/lib/prisma{}", q, q)
                }).to_string();
                if updated != cur {
                    cur = updated;
                    changed = true;
                }
            }
            if cur.contains("'src/") || cur.contains("\"src/") {
                cur = cur.replace("'src/", "'@/").replace("\"src/", "\"@/");
                changed = true;
            }
            if cur.contains("'~/") || cur.contains("\"~/") {
                cur = cur.replace("'~/", "'@/").replace("\"~/", "\"@/");
                changed = true;
            }
        }
        out_lines.push(cur);
    }

    if !changed {
        return content.to_string();
    }
    let mut joined = out_lines.join("\n");
    if content.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

pub fn ensure_next_server_imports(content: &str, path_lower: &str) -> String {
    let is_route_or_middleware = path_lower.ends_with("middleware.ts")
        || path_lower.ends_with("middleware.js")
        || path_lower.ends_with("route.ts")
        || path_lower.ends_with("route.js")
        || path_lower.contains("/api/");

    let mut result = content.to_string();

    // Fix accidental default imports from 'next/server': `import NextResponse from 'next/server'` -> `{ NextResponse }`
    if result.contains("'next/server'") || result.contains("\"next/server\"") {
        if let Ok(re_def_named) = regex::Regex::new(
            r#"(?m)^(\s*import\s+)(NextResponse|NextRequest)\s*,\s*\{([^}]*)\}\s*from\s*(['"]next/server['"]\s*;?)"#
        ) {
            result = re_def_named.replace_all(&result, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let def_id = &caps[2];
                let existing = caps[3].trim().trim_end_matches(',');
                let suffix = &caps[4];
                if existing.contains(def_id) {
                    format!("{}{{ {} }} from {}", prefix, existing, suffix)
                } else if existing.is_empty() {
                    format!("{}{{ {} }} from {}", prefix, def_id, suffix)
                } else {
                    format!("{}{{ {}, {} }} from {}", prefix, def_id, existing, suffix)
                }
            }).to_string();
        }
        if let Ok(re_def_only) = regex::Regex::new(
            r#"(?m)^(\s*import\s+)(NextResponse|NextRequest)\s+from\s+(['"]next/server['"]\s*;?)"#
        ) {
            result = re_def_only.replace_all(&result, "${1}{ ${2} } from ${3}").to_string();
        }
    }

    if !is_route_or_middleware {
        return result;
    }

    let uses_next_response = result.contains("NextResponse.") || result.contains("NextResponse(") || result.contains("new NextResponse");
    let uses_next_request = result.contains("NextRequest") && !result.contains("interface NextRequest") && !result.contains("type NextRequest =");

    let mut missing: Vec<&str> = Vec::new();
    if uses_next_response && !regex::Regex::new(r#"(?s)import\s+[^;]*\bNextResponse\b[^;]*from\s*['"]next/server['"]"#).map(|r| r.is_match(&result)).unwrap_or(false) {
        missing.push("NextResponse");
    }
    if uses_next_request && !regex::Regex::new(r#"(?s)import\s+[^;]*\bNextRequest\b[^;]*from\s*['"]next/server['"]"#).map(|r| r.is_match(&result)).unwrap_or(false) {
        missing.push("NextRequest");
    }

    if !missing.is_empty() {
        if let Ok(existing_ns_re) = regex::Regex::new(r#"(?m)^(\s*import\s+(?:type\s+)?\{)([^}]*)(\}\s*from\s*['"]next/server['"]\s*;?)"#) {
            if let Some(cap) = existing_ns_re.captures(&result) {
                let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let existing = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim().trim_end_matches(',');
                let suffix = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                if !prefix.contains("import type") {
                    let combined = if existing.is_empty() {
                        format!(" {} ", missing.join(", "))
                    } else {
                        format!(" {}, {} ", existing, missing.join(", "))
                    };
                    let replacement = format!("{}{}{}", prefix, combined, suffix);
                    result = existing_ns_re.replace(&result, replacement.as_str()).to_string();
                    return result;
                }
            }
        }
        result = format!("import {{ {} }} from 'next/server';\n{}", missing.join(", "), result);
    }

    result
}

/// Converts accidental named `{ Link }` imports from `'next/link'` to the required default `import Link from 'next/link'`.
pub fn sanitize_next_link_named_import(content: &str) -> String {
    if !content.contains("next/link") {
        return content.to_string();
    }
    let Ok(re_named_link) = regex::Regex::new(
        r#"(?m)^(\s*)import\s+\{([^}]*)\}\s+from\s+(['"]next/link['"]\s*;?)"#,
    ) else {
        return content.to_string();
    };
    re_named_link
        .replace_all(content, |caps: &regex::Captures| {
            let indent = &caps[1];
            let inner = &caps[2];
            let mod_tail = &caps[3];
            let mut local_link_name: Option<String> = None;
            let mut remaining: Vec<String> = Vec::new();
            for raw_item in inner.split(',') {
                let it = raw_item.trim();
                if it.is_empty() {
                    continue;
                }
                if !it.starts_with("type ") {
                    if let Some((orig, alias)) = it.split_once(" as ") {
                        if orig.trim() == "Link" {
                            local_link_name = Some(alias.trim().to_string());
                            continue;
                        }
                    } else if it == "Link" {
                        local_link_name = Some("Link".to_string());
                        continue;
                    }
                }
                remaining.push(it.to_string());
            }
            if let Some(link_ident) = local_link_name {
                if remaining.is_empty() {
                    format!("{}import {} from {}", indent, link_ident, mod_tail)
                } else {
                    format!(
                        "{}import {}, {{ {} }} from {}",
                        indent,
                        link_ident,
                        remaining.join(", "),
                        mod_tail
                    )
                }
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
}

/// Repairs common NextAuth v4 import mistakes outside the canonical `[...nextauth]` route handler:
/// - Moves provider factories (`CredentialsProvider`, `GoogleProvider`, etc.) out of `'next-auth'`
///   braced imports into `'next-auth/providers/<provider>'` default imports.
/// - Ensures default `NextAuth` is imported from `'next-auth'` when `NextAuth(` is called.
pub fn sanitize_nextauth_imports(content: &str) -> String {
    if !content.contains("next-auth") && !content.contains("NextAuth(") {
        return content.to_string();
    }
    const PROVIDER_MAP: &[(&str, &str)] = &[
        ("CredentialsProvider", "next-auth/providers/credentials"),
        ("GoogleProvider", "next-auth/providers/google"),
        ("GitHubProvider", "next-auth/providers/github"),
        ("GithubProvider", "next-auth/providers/github"),
        ("EmailProvider", "next-auth/providers/email"),
        ("DiscordProvider", "next-auth/providers/discord"),
    ];

    let mut out = content.to_string();
    let mut extracted_providers: Vec<(String, &'static str)> = Vec::new();

    if let Ok(re_na_named) = regex::Regex::new(
        r#"(?m)^(\s*)import\s+(?:([A-Za-z_$][A-Za-z0-9_$]*)\s*,\s*)?\{([^}]*)\}\s*from\s+(['"]next-auth(?:/providers)?['"]\s*;?)"#,
    ) {
        out = re_na_named
            .replace_all(&out, |caps: &regex::Captures| {
                let indent = &caps[1];
                let def_ident = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let inner = &caps[3];
                let mod_tail = &caps[4];
                let mut kept: Vec<String> = Vec::new();
                for raw_item in inner.split(',') {
                    let it = raw_item.trim();
                    if it.is_empty() {
                        continue;
                    }
                    let base_name = it
                        .strip_prefix("type ")
                        .unwrap_or(it)
                        .split(" as ")
                        .next()
                        .unwrap_or("")
                        .trim();
                    if let Some((_, prov_path)) = PROVIDER_MAP.iter().find(|(p, _)| *p == base_name) {
                        let local_name = it
                            .split_once(" as ")
                            .map(|(_, a)| a.trim().to_string())
                            .unwrap_or_else(|| base_name.to_string());
                        if !extracted_providers.iter().any(|(n, _)| n == &local_name) {
                            extracted_providers.push((local_name, *prov_path));
                        }
                    } else {
                        kept.push(it.to_string());
                    }
                }
                if kept.len() == inner.split(',').filter(|s| !s.trim().is_empty()).count() {
                    return caps[0].to_string();
                }
                let q = if mod_tail.trim_start().starts_with('"') { "\"" } else { "'" };
                let has_semi = mod_tail.trim_end().ends_with(';');
                let semi = if has_semi { ";" } else { "" };
                if def_ident.is_empty() && kept.is_empty() {
                    String::new()
                } else if kept.is_empty() {
                    format!("{}import {} from {}next-auth{}{}", indent, def_ident, q, q, semi)
                } else if def_ident.is_empty() {
                    format!("{}import {{ {} }} from {}next-auth{}{}", indent, kept.join(", "), q, q, semi)
                } else {
                    format!(
                        "{}import {}, {{ {} }} from {}next-auth{}{}",
                        indent,
                        def_ident,
                        kept.join(", "),
                        q,
                        q,
                        semi
                    )
                }
            })
            .to_string();
    }

    let mut extra_imports = String::new();
    for (local_name, prov_path) in extracted_providers {
        if !out.contains(prov_path) {
            extra_imports.push_str(&format!("import {} from '{}';\n", local_name, prov_path));
        }
    }

    let calls_nextauth = regex::Regex::new(r"\bNextAuth\s*\(")
        .map(|re| re.is_match(&out))
        .unwrap_or(false);
    let imports_default_nextauth = regex::Regex::new(
        r#"(?m)^\s*import\s+NextAuth(?:\s*,|\s+from\b)"#,
    )
    .map(|re| re.is_match(&out))
    .unwrap_or(false);
    let declares_nextauth = regex::Regex::new(
        r#"(?m)\b(?:function|const|let|var)\s+NextAuth\b"#,
    )
    .map(|re| re.is_match(&out))
    .unwrap_or(false);

    if calls_nextauth && !imports_default_nextauth && !declares_nextauth {
        extra_imports.push_str("import NextAuth from 'next-auth';\n");
    }

    if !extra_imports.is_empty() {
        out = out.trim_start_matches('\n').to_string();
        out = format!("{}{}", extra_imports, out);
    }
    out
}

/// Deterministically repairs declared Prisma Enum usage in server-side files using `prisma/schema.prisma`:
/// - Rewrites Prisma v4 `Prisma.<DeclaredEnum>` references into top-level `<DeclaredEnum>` (required in Prisma v5+)
/// - Ensures any referenced declared enum (`EnumName.MEMBER` or `: EnumName`) is imported from `'@prisma/client'`.
pub fn reconcile_server_prisma_enum_usage(
    proj_dir: &std::path::Path,
    file_path: &str,
    content: &str,
) -> String {
    let path_low = file_path.replace('\\', "/").to_lowercase();
    if !(path_low.ends_with(".ts") || path_low.ends_with(".tsx") || path_low.ends_with(".js") || path_low.ends_with(".jsx")) {
        return content.to_string();
    }
    let trimmed = content.trim_start();
    let is_client_file = trimmed.starts_with("\"use client\"")
        || trimmed.starts_with("'use client'")
        || content.lines().take(5).any(|l| {
            let t = l.trim();
            t == "\"use client\";" || t == "\"use client\"" || t == "'use client';" || t == "'use client'"
        });
    let is_app_route = is_app_router_route_file(file_path);
    let is_hook_file = !is_app_route && (path_low.contains("/hooks/") || path_low.starts_with("hooks/")) && !path_low.contains("/api/");
    if is_client_file || is_hook_file {
        return content.to_string();
    }

    let Some(schema_path) = [
        proj_dir.join("prisma/schema.prisma"),
        proj_dir.join("schema.prisma"),
    ]
    .into_iter()
    .find(|p| p.is_file()) else {
        return content.to_string();
    };
    let Ok(schema_src) = std::fs::read_to_string(&schema_path) else {
        return content.to_string();
    };
    let contract = crate::orchestrator::extract_prisma_schema_contract(&schema_src);
    let Some(enums_obj) = contract.get("enums").and_then(|e| e.as_object()) else {
        return content.to_string();
    };
    if enums_obj.is_empty() {
        return content.to_string();
    }

    let mut out = content.to_string();
    let mut enums_to_import: Vec<String> = Vec::new();

    for enum_name in enums_obj.keys() {
        let esc = regex::escape(enum_name);
        // Strip legacy `Prisma.<EnumName>` namespace prefix (Prisma v5 exports enums at top level)
        if let Ok(re_ns) = regex::Regex::new(&format!(r"\bPrisma\s*\.\s*{}\b", esc)) {
            if re_ns.is_match(&out) {
                out = re_ns.replace_all(&out, enum_name.as_str()).to_string();
            }
        }

        let used_member = regex::Regex::new(&format!(r"\b{}\.[A-Z][A-Z0-9_]*\b", esc))
            .map(|r| r.is_match(&out))
            .unwrap_or(false);
        let used_type_or_z = regex::Regex::new(&format!(
            r"(?::\s*{}|\bz\.nativeEnum\s*\(\s*{}\s*\))",
            esc, esc
        ))
        .map(|r| r.is_match(&out))
        .unwrap_or(false);
        if !used_member && !used_type_or_z {
            continue;
        }

        let already_bound = regex::Regex::new(&format!(
            r#"(?s)(?:\bimport\s+(?:type\s+)?(?:\{{[^}}]*\b{}\b[^}}]*\}}|{}\b)|\b(?:enum|const|let|var|type|interface)\s+{}\b)"#,
            esc, esc, esc
        ))
        .map(|r| r.is_match(&out))
        .unwrap_or(false);
        if !already_bound {
            enums_to_import.push(enum_name.clone());
        }
    }

    if enums_to_import.is_empty() {
        return out;
    }

    // Merge into an existing runtime `@prisma/client` import if present
    if let Ok(re_prisma_val_imp) = regex::Regex::new(
        r#"(?m)^(\s*import\s+)\{([^}]*)\}(\s*from\s*['"]@prisma/client['"]\s*;?)"#,
    ) {
        if re_prisma_val_imp.is_match(&out) {
            let mut merged_once = false;
            out = re_prisma_val_imp
                .replace(&out, |caps: &regex::Captures| {
                    merged_once = true;
                    let prefix = &caps[1];
                    let existing = caps[2].trim().trim_end_matches(',');
                    let suffix = &caps[3];
                    if existing.is_empty() {
                        format!("{}{{ {} }}{}", prefix, enums_to_import.join(", "), suffix)
                    } else {
                        format!(
                            "{}{{ {}, {} }}{}",
                            prefix,
                            existing,
                            enums_to_import.join(", "),
                            suffix
                        )
                    }
                })
                .to_string();
            if merged_once {
                return out;
            }
        }
    }

    format!("import {{ {} }} from '@prisma/client';\n{}", enums_to_import.join(", "), out)
}

pub fn deduplicate_repeated_css_blocks(css: &str) -> String {
    let mut out = String::new();
    let mut seen_blocks: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let chars: Vec<(usize, char)> = css.char_indices().collect();
    let mut i = 0usize;
    let mut chunk_start = 0usize;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        if ch == '{' {
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < chars.len() && depth > 0 {
                if chars[j].1 == '{' {
                    depth += 1;
                } else if chars[j].1 == '}' {
                    depth -= 1;
                }
                j += 1;
            }
            if depth == 0 {
                let end_byte = if j < chars.len() { chars[j].0 } else { css.len() };
                let block = &css[chunk_start..end_byte];
                let norm: String = block.split_whitespace().collect::<Vec<&str>>().join(" ");
                let is_at_rule = norm.trim_start().starts_with('@');
                let count = seen_blocks.entry(norm.clone()).or_insert(0);
                *count += 1;
                if is_at_rule || norm.len() <= 18 || *count == 1 {
                    out.push_str(block);
                }
                chunk_start = end_byte;
                i = j;
                continue;
            } else {
                // Unclosed trailing CSS block from LLM truncation: drop incomplete tail or close cleanly
                let tail = &css[chunk_start..];
                if tail.trim().len() > 4 {
                    out.push_str(tail.trim_end());
                    for _ in 0..depth {
                        out.push_str("\n}");
                    }
                    out.push('\n');
                }
                return out;
            }
        } else if ch == '\n' && css[chunk_start..byte_pos].trim().starts_with("@") && css[chunk_start..byte_pos].contains(';') {
            let line_end = byte_pos + 1;
            out.push_str(&css[chunk_start..line_end]);
            chunk_start = line_end;
        }
        i += 1;
    }

    if chunk_start < css.len() {
        out.push_str(&css[chunk_start..]);
    }
    out
}

fn normalize_rel_fs_components(p: &std::path::Path) -> std::path::PathBuf {
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

pub fn widen_narrow_ui_props_interfaces(content: &str) -> String {
    let Ok(re) = regex::Regex::new(
        r#"(?m)^[ \t]*(?:export\s+)?interface\s+[A-Za-z0-9_]*Props\b(?:\s+extends\s+[^{;]+?)?\s*\{"#,
    ) else {
        return content.to_string();
    };

    let bytes = content.as_bytes();
    let mut insertions: Vec<usize> = Vec::new();

    for m in re.find_iter(content) {
        let open_idx = m.end() - 1;
        if bytes.get(open_idx) != Some(&b'{') {
            continue;
        }
        let mut depth = 0isize;
        let mut close_idx = None;
        let mut i = open_idx;
        let mut in_single = false;
        let mut in_double = false;
        let mut in_tick = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        let mut escaped = false;

        while i < bytes.len() {
            let b = bytes[i];
            if in_line_comment {
                if b == b'\n' {
                    in_line_comment = false;
                }
                i += 1;
                continue;
            }
            if in_block_comment {
                if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    in_block_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if (in_single || in_double || in_tick) && b == b'\\' {
                escaped = true;
                i += 1;
                continue;
            }
            if !in_double && !in_tick && b == b'\'' {
                in_single = !in_single;
                i += 1;
                continue;
            }
            if !in_single && !in_tick && b == b'"' {
                in_double = !in_double;
                i += 1;
                continue;
            }
            if !in_single && !in_double && b == b'`' {
                in_tick = !in_tick;
                i += 1;
                continue;
            }
            if !in_single && !in_double && !in_tick {
                if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
                    in_line_comment = true;
                    i += 2;
                    continue;
                }
                if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }
                if b == b'{' {
                    depth += 1;
                } else if b == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        close_idx = Some(i);
                        break;
                    }
                }
            }
            i += 1;
        }

        if let Some(end_idx) = close_idx {
            let body = &content[open_idx + 1..end_idx];
            if !body.contains("[key:") {
                insertions.push(end_idx);
            }
        }
    }

    if insertions.is_empty() {
        return content.to_string();
    }

    let mut out = content.to_string();
    insertions.sort_unstable();
    insertions.dedup();
    for pos in insertions.into_iter().rev() {
        let before_has_nl = pos > 0 && out.as_bytes()[pos - 1] == b'\n';
        let patch = if before_has_nl {
            "  [key: string]: any;\n"
        } else {
            "\n  [key: string]: any;\n"
        };
        out.insert_str(pos, patch);
    }
    out
}

fn find_balanced_delimiter_bytes(src: &str, open_idx: usize, open_b: u8, close_b: u8) -> Option<usize> {
    let bytes = src.as_bytes();
    if bytes.get(open_idx) != Some(&open_b) {
        return None;
    }
    let mut depth = 0isize;
    let mut i = open_idx;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_tick = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut escaped = false;

    while i < bytes.len() {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if (in_single || in_double || in_tick) && b == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if !in_double && !in_tick && b == b'\'' {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if !in_single && !in_tick && b == b'"' {
            in_double = !in_double;
            i += 1;
            continue;
        }
        if !in_single && !in_double && b == b'`' {
            in_tick = !in_tick;
            i += 1;
            continue;
        }
        if !in_single && !in_double && !in_tick {
            if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
                in_block_comment = true;
                i += 2;
                continue;
            }
            if b == open_b {
                depth += 1;
            } else if b == close_b {
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

/// Upgrades bare `React.FC =` / `React.FunctionComponent =` (missing `<Props>` generic, which defaults
/// to `{}` under React 18 and rejects `children`, `className`, `onClick`, etc.) to `React.FC<any> =`
/// whenever props are destructured `({ ... })` or inside a UI component module (`force_ui_module`).
pub fn repair_bare_react_fc_signatures(content: &str, force_ui_module: bool) -> String {
    let pat = if force_ui_module {
        r"React\.(FC|FunctionComponent)\s*="
    } else {
        r"React\.(FC|FunctionComponent)\s*=\s*(?:async\s*)?\(\s*\{"
    };
    let Ok(re) = regex::Regex::new(pat) else {
        return content.to_string();
    };
    if !re.is_match(content) {
        return content.to_string();
    }
    if force_ui_module {
        re.replace_all(content, "React.${1}<any> =").to_string()
    } else {
        let Ok(re_destr) = regex::Regex::new(r"(React\.(?:FC|FunctionComponent))(\s*=\s*(?:async\s*)?\(\s*\{)") else {
            return content.to_string();
        };
        re_destr.replace_all(content, "${1}<any>${2}").to_string()
    }
}

/// Repairs React UI primitive components (e.g. `export const Select: React.FC<{...}> = ({ name, value }) => (<select>{children}</select>)`)
/// that reference `{children}` inside JSX without declaring `children` in their inline props type or destructuring `children` in parameters.
pub fn repair_ui_fc_missing_children_prop(content: &str) -> String {
    let upgraded = repair_bare_react_fc_signatures(content, false);
    let content = upgraded.as_str();
    let Ok(fc_header_re) = regex::Regex::new(r#"(?m)React\.FC\s*<"#) else {
        return content.to_string();
    };
    let Ok(children_ident_re) = regex::Regex::new(r#"\bchildren\b"#) else {
        return content.to_string();
    };

    let bytes = content.as_bytes();
    let mut edits: Vec<(usize, String)> = Vec::new();

    for m in fc_header_re.find_iter(content) {
        let mut cursor = m.end();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }

        // Track optional inline `{ ... }` right after `React.FC<`
        let inline_type_span = if bytes.get(cursor) == Some(&b'{') {
            find_balanced_delimiter_bytes(content, cursor, b'{', b'}').map(|end| (cursor, end))
        } else {
            None
        };

        // Find closing `>` of `React.FC<...>` by tracking nested braces/parens/brackets
        let mut angle_depth = 1isize;
        let mut brace_d = 0isize;
        let mut paren_d = 0isize;
        let mut i = cursor;
        let mut generic_close = None;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'{' {
                brace_d += 1;
            } else if b == b'}' {
                brace_d = (brace_d - 1).max(0);
            } else if b == b'(' {
                paren_d += 1;
            } else if b == b')' {
                paren_d = (paren_d - 1).max(0);
            } else if brace_d == 0 && paren_d == 0 {
                if b == b'<' {
                    angle_depth += 1;
                } else if b == b'>' && (i == 0 || bytes[i - 1] != b'=') {
                    angle_depth -= 1;
                    if angle_depth == 0 {
                        generic_close = Some(i);
                        break;
                    }
                }
            }
            i += 1;
        }
        let Some(gt_idx) = generic_close else {
            continue;
        };

        // Expect `= ({ ... }) =>`
        let mut p = gt_idx + 1;
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        if bytes.get(p) != Some(&b'=') {
            continue;
        }
        p += 1;
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        if bytes.get(p) != Some(&b'(') {
            continue;
        }
        let Some(paren_close) = find_balanced_delimiter_bytes(content, p, b'(', b')') else {
            continue;
        };
        let mut inner_param = p + 1;
        while inner_param < paren_close && bytes[inner_param].is_ascii_whitespace() {
            inner_param += 1;
        }
        if bytes.get(inner_param) != Some(&b'{') {
            continue;
        }
        let Some(destructure_close) = find_balanced_delimiter_bytes(content, inner_param, b'{', b'}') else {
            continue;
        };

        // Expect `=>` after `)`
        let mut after_paren = paren_close + 1;
        while after_paren < bytes.len() && bytes[after_paren].is_ascii_whitespace() {
            after_paren += 1;
        }
        if bytes.get(after_paren) != Some(&b'=') || bytes.get(after_paren + 1) != Some(&b'>') {
            continue;
        }
        let mut body_start = after_paren + 2;
        while body_start < bytes.len() && bytes[body_start].is_ascii_whitespace() {
            body_start += 1;
        }
        let body_end = if bytes.get(body_start) == Some(&b'{') {
            find_balanced_delimiter_bytes(content, body_start, b'{', b'}').unwrap_or(bytes.len() - 1)
        } else if bytes.get(body_start) == Some(&b'(') {
            find_balanced_delimiter_bytes(content, body_start, b'(', b')').unwrap_or(bytes.len() - 1)
        } else {
            content[body_start..].find(';').map(|off| body_start + off).unwrap_or(bytes.len())
        };
        if body_end <= body_start {
            continue;
        }
        let body_slice = &content[body_start..=body_end.min(bytes.len() - 1)];

        // Check standalone `{children}` / `children` reference not preceded by `.`
        let uses_children = children_ident_re.find_iter(body_slice).any(|cm| {
            let before = body_slice[..cm.start()].trim_end();
            !before.ends_with('.') && !before.ends_with("const") && !before.ends_with("let") && !before.ends_with("var")
        });
        if !uses_children {
            continue;
        }

        let params_slice = &content[inner_param + 1..destructure_close];
        if !children_ident_re.is_match(params_slice) {
            if let Some(rest_off) = params_slice.find("...") {
                edits.push((inner_param + 1 + rest_off, "children, ".to_string()));
            } else {
                let trimmed_end = params_slice.trim_end();
                let insert_pos = inner_param + 1 + trimmed_end.len();
                let patch = if trimmed_end.is_empty() {
                    " children "
                } else if trimmed_end.ends_with(',') {
                    " children "
                } else {
                    ", children "
                };
                edits.push((insert_pos, patch.to_string()));
            }
        }

        if let Some((t_open, t_close)) = inline_type_span {
            let type_slice = &content[t_open + 1..t_close];
            if !children_ident_re.is_match(type_slice) {
                let trimmed_end = type_slice.trim_end();
                let insert_pos = t_open + 1 + trimmed_end.len();
                let patch = if trimmed_end.is_empty() {
                    " children?: React.ReactNode "
                } else if trimmed_end.ends_with(';') {
                    " children?: React.ReactNode;"
                } else {
                    "; children?: React.ReactNode"
                };
                edits.push((insert_pos, patch.to_string()));
            }
        }
    }

    if edits.is_empty() {
        return content.to_string();
    }
    edits.sort_by_key(|(pos, _)| *pos);
    let mut out = content.to_string();
    for (pos, patch) in edits.into_iter().rev() {
        if pos <= out.len() && out.is_char_boundary(pos) {
            out.insert_str(pos, &patch);
        }
    }
    out
}

pub fn ensure_ui_primitives_reconciled(proj_dir: &std::path::Path) {
    let raw_src_dir = proj_dir.join("src");
    let has_src = raw_src_dir.exists();
    let has_root_layout = !has_src
        && (proj_dir.join("app").exists()
            || proj_dir.join("components").exists()
            || proj_dir.join("hooks").exists());
    if !has_src && !has_root_layout {
        return;
    }
    let src_dir = if has_src { raw_src_dir } else { proj_dir.to_path_buf() };
    let components_dir = src_dir.join("components");
    let ui_dir = components_dir.join("ui");

    // 1. Collect all named imports referencing `@/components/ui`, `@/components/ui/<subpath>`,
    //    `@/components` (for standard UI primitive names when `ui.tsx` exists or primitive file is absent),
    //    or relative `'./...'` / `'../...'` UI module imports inside `components/**`.
    let ui_import_re = match regex::Regex::new(
        r#"(?ms)import\s+(?:type\s+)?\{([^}]+)\}\s+from\s+['"]((?:@/components(?:/ui(?:/[^'"]+)?)?|\.\.?/[^'"]+))['"]"#
    ) {
        Ok(r) => r,
        Err(_) => return,
    };

    let is_standard_ui_primitive = |sym: &str| -> bool {
        matches!(
            sym,
            "Button"
                | "Input"
                | "Textarea"
                | "Select"
                | "Option"
                | "Card"
                | "CardHeader"
                | "CardTitle"
                | "CardDescription"
                | "CardContent"
                | "CardFooter"
                | "Badge"
                | "Label"
                | "Checkbox"
                | "Switch"
                | "Tabs"
                | "TabsList"
                | "TabsTrigger"
                | "TabsContent"
                | "Modal"
                | "Dialog"
                | "DialogContent"
                | "DialogHeader"
                | "DialogTitle"
                | "DialogDescription"
                | "Alert"
                | "AlertTitle"
                | "AlertDescription"
                | "Avatar"
                | "Spinner"
                | "Skeleton"
                | "Separator"
                | "MetricCard"
                | "Table"
                | "TableHeader"
                | "TableBody"
                | "TableFooter"
                | "TableRow"
                | "TableHead"
                | "TableCell"
                | "TableCaption"
                | "Link"
        )
    };
    let is_auto_synthesizable_ui_tag = |sym: &str| -> bool {
        is_standard_ui_primitive(sym) && !matches!(sym, "Link" | "Option")
    };

    let mut demanded: std::collections::BTreeMap<std::path::PathBuf, std::collections::BTreeSet<String>> = std::collections::BTreeMap::new();
    let mut demanded_syms_anywhere: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut already_exported_syms: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut unimported_standard_tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut compound_demands: std::collections::BTreeMap<
        std::path::PathBuf,
        std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    > = std::collections::BTreeMap::new();
    let ui_default_import_re = regex::Regex::new(
        r#"(?m)import\s+([A-Z][A-Za-z0-9_]*)\s+from\s+['"]((?:@/components(?:/ui(?:/[^'"]+)?)?|\.\.?/[^'"]+))['"]"#,
    )
    .ok();
    let compound_jsx_re = regex::Regex::new(r"<([A-Z][A-Za-z0-9_]*)\.([A-Z][A-Za-z0-9_]*)\b").ok();
    let export_decl_sym_re = regex::Regex::new(
        r#"(?m)\bexport\s+(?:async\s+)?(?:function|const|let|var|class)\s+([A-Z][A-Za-z0-9_]*)\b"#,
    )
    .ok();
    let export_list_sym_re = regex::Regex::new(r#"(?m)\bexport\s*\{([^}]+)\}"#).ok();
    let export_def_sym_re = regex::Regex::new(
        r#"(?m)\bexport\s+default\s+(?:async\s+)?(?:function\s+|class\s+)?([A-Z][A-Za-z0-9_]*)\b"#,
    )
    .ok();

    let mut stack = vec![src_dir.clone()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let n = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
                    if n != "node_modules" && n != ".next" && n != ".git" && n != ".axiom" && n != "prisma" {
                        stack.push(p);
                    }
                } else if p.is_file() {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if matches!(ext, "ts" | "tsx" | "js" | "jsx") {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            let is_inside_components = p.starts_with(&components_dir);
                            let resolve_ui_spec = |spec: &str, only_existing: bool| -> Option<std::path::PathBuf> {
                                if spec == "@/components" {
                                    let ui_tsx = src_dir.join("components/ui.tsx");
                                    if !ui_tsx.is_file() && !ui_dir.is_dir() {
                                        return None;
                                    }
                                    return Some(if ui_tsx.is_file() {
                                        ui_tsx
                                    } else if ui_dir.join("index.tsx").is_file() {
                                        ui_dir.join("index.tsx")
                                    } else if ui_dir.join("index.ts").is_file() {
                                        ui_dir.join("index.ts")
                                    } else {
                                        ui_tsx
                                    });
                                }
                                let base = if spec.starts_with("@/") {
                                    src_dir.join(spec.trim_start_matches("@/"))
                                } else if is_inside_components {
                                    let parent = p.parent()?;
                                    let joined = normalize_rel_fs_components(&parent.join(spec));
                                    if !(joined.starts_with(&ui_dir) || joined == ui_dir) {
                                        return None;
                                    }
                                    joined
                                } else {
                                    return None;
                                };
                                let existing_target = if base.is_file() {
                                    Some(base.clone())
                                } else if base.with_extension("tsx").is_file() {
                                    Some(base.with_extension("tsx"))
                                } else if base.with_extension("ts").is_file() {
                                    Some(base.with_extension("ts"))
                                } else if base.is_dir() && base.join("index.tsx").is_file() {
                                    Some(base.join("index.tsx"))
                                } else if base.is_dir() && base.join("index.ts").is_file() {
                                    Some(base.join("index.ts"))
                                } else if (spec == "@/components/ui" || base == ui_dir)
                                    && src_dir.join("components/ui.tsx").is_file()
                                {
                                    Some(src_dir.join("components/ui.tsx"))
                                } else {
                                    None
                                };
                                if let Some(found) = existing_target {
                                    Some(found)
                                } else if only_existing {
                                    None
                                } else if base == ui_dir {
                                    Some(src_dir.join("components/ui.tsx"))
                                } else if base.starts_with(&ui_dir) {
                                    Some(if base.extension().is_some() {
                                        base
                                    } else {
                                        base.with_extension("tsx")
                                    })
                                } else {
                                    None
                                }
                            };

                            let mut file_imported_targets: std::collections::HashMap<String, std::path::PathBuf> =
                                std::collections::HashMap::new();

                            for cap in ui_import_re.captures_iter(&content) {
                                let names = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                                let spec = cap.get(2).map(|m| m.as_str()).unwrap_or("");

                                let mut parsed_pascal: Vec<String> = Vec::new();
                                for item in names.split(',') {
                                    let raw = item.trim();
                                    if raw.starts_with("type ") {
                                        continue;
                                    }
                                    let clean = raw.split_whitespace().next().unwrap_or("").trim();
                                    if !clean.is_empty() && clean.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                                        parsed_pascal.push(clean.to_string());
                                    }
                                }
                                if parsed_pascal.is_empty() {
                                    continue;
                                }

                                let Some(target) = resolve_ui_spec(spec, false) else {
                                    continue;
                                };
                                if spec == "@/components" {
                                    for sym in parsed_pascal {
                                        if is_standard_ui_primitive(&sym)
                                            && !components_dir.join(format!("{}.tsx", sym)).is_file()
                                            && !components_dir.join(format!("{}.ts", sym)).is_file()
                                        {
                                            file_imported_targets.insert(sym.clone(), target.clone());
                                            demanded_syms_anywhere.insert(sym.clone());
                                            demanded.entry(target.clone()).or_default().insert(sym);
                                        }
                                    }
                                    continue;
                                }

                                let set = demanded.entry(target.clone()).or_default();
                                for sym in parsed_pascal {
                                    file_imported_targets.insert(sym.clone(), target.clone());
                                    demanded_syms_anywhere.insert(sym.clone());
                                    set.insert(sym);
                                }
                            }

                            let rel_low = p
                                .strip_prefix(proj_dir)
                                .map(|r| r.to_string_lossy().replace('\\', "/").to_lowercase())
                                .unwrap_or_default();
                            if !is_app_router_route_file(&rel_low) && !rel_low.contains("/api/") && !rel_low.starts_with("api/") {
                                if let Some(ref re) = export_decl_sym_re {
                                    for cap in re.captures_iter(&content) {
                                        if let Some(m) = cap.get(1) {
                                            already_exported_syms.insert(m.as_str().to_string());
                                        }
                                    }
                                }
                                if let Some(ref re) = export_list_sym_re {
                                    for cap in re.captures_iter(&content) {
                                        let list = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                                        for item in list.split(',') {
                                            let t = item.trim();
                                            let exp_name = if let Some((_, rhs)) = t.split_once(" as ") {
                                                rhs.trim()
                                            } else {
                                                t.split_whitespace().next().unwrap_or("")
                                            };
                                            if exp_name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                                                already_exported_syms.insert(exp_name.to_string());
                                            }
                                        }
                                    }
                                }
                                if let Some(ref re) = export_def_sym_re {
                                    for cap in re.captures_iter(&content) {
                                        if let Some(m) = cap.get(1) {
                                            already_exported_syms.insert(m.as_str().to_string());
                                        }
                                    }
                                }
                            }

                            if matches!(ext, "tsx" | "jsx") && content.contains('<') {
                                let known = crate::self_healing::collect_declared_or_imported_idents(&content);
                                let missing_jsx = crate::self_healing::extract_unimported_jsx_components(&content, &known, true, &[]);
                                for sym in missing_jsx {
                                    if is_auto_synthesizable_ui_tag(&sym) {
                                        unimported_standard_tags.insert(sym);
                                    }
                                }
                            }

                            if let Some(ref def_re) = ui_default_import_re {
                                for cap in def_re.captures_iter(&content) {
                                    let def_sym = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                                    let spec = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                                    if let Some(target) = resolve_ui_spec(spec, true) {
                                        file_imported_targets.entry(def_sym.to_string()).or_insert(target);
                                    }
                                }
                            }

                            if matches!(ext, "tsx" | "jsx") && !file_imported_targets.is_empty() {
                                if let Some(ref cj_re) = compound_jsx_re {
                                    for cap in cj_re.captures_iter(&content) {
                                        let parent = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                                        let sub = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                                        if let Some(target) = file_imported_targets.get(parent) {
                                            compound_demands
                                                .entry(target.clone())
                                                .or_default()
                                                .entry(parent.to_string())
                                                .or_default()
                                                .insert(sub.to_string());
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

    // Synthesize genuinely unimported standard JSX primitives (e.g. <Spinner />) only when no non-route module already exports them
    if !unimported_standard_tags.is_empty() {
        let ui_tsx_path = src_dir.join("components/ui.tsx");
        let ui_index_tsx = ui_dir.join("index.tsx");
        for sym in unimported_standard_tags {
            if already_exported_syms.contains(&sym) || demanded_syms_anywhere.contains(&sym) {
                continue;
            }
            let target = if ui_tsx_path.is_file() {
                ui_tsx_path.clone()
            } else if ui_index_tsx.is_file() {
                ui_index_tsx.clone()
            } else if ui_dir.is_dir() {
                ui_dir.join(format!("{}.tsx", sym.to_lowercase()))
            } else {
                ui_tsx_path.clone()
            };
            demanded.entry(target).or_default().insert(sym);
        }
    }

    // 2. Also inspect existing `src/components/ui.tsx` and `src/components/ui/*.tsx` to relax strict props & unions
    let mut ui_files: std::collections::BTreeSet<std::path::PathBuf> = demanded.keys().cloned().collect();
    for k in compound_demands.keys() {
        ui_files.insert(k.clone());
    }
    if src_dir.join("components/ui.tsx").is_file() {
        ui_files.insert(src_dir.join("components/ui.tsx"));
    }
    if let Ok(entries) = std::fs::read_dir(&ui_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && matches!(p.extension().and_then(|e| e.to_str()), Some("tsx" | "ts")) {
                ui_files.insert(p);
            }
        }
    }

    // Idempotent full-line match for literal unions on variant/size/color props
    let union_prop_re = regex::Regex::new(r#"(?m)^(\s*(?:variant|size|color)\??:\s*)([^;\r\n]+?)(\s*;?\r?)$"#).ok();
    let literal_union_rhs_re = regex::Regex::new(r#"^\s*(?:['"][^'"]+['"]\s*\|\s*)+['"][^'"]+['"]\s*$"#).ok();

    for ui_file in ui_files {
        let existed_before = ui_file.is_file();
        let original = if existed_before {
            match std::fs::read_to_string(&ui_file) {
                Ok(c) => c,
                Err(_) => continue,
            }
        } else {
            String::new()
        };
        let mut content = original.clone();

        if existed_before {
            if let Some(ref re) = union_prop_re {
                content = re.replace_all(&content, |caps: &regex::Captures| {
                    let prefix = &caps[1];
                    let rhs = caps[2].trim_end();
                    let suffix = &caps[3];
                    let is_literal_union = literal_union_rhs_re
                        .as_ref()
                        .map(|lr| lr.is_match(rhs))
                        .unwrap_or(false);
                    if !rhs.contains("(string & {})") && is_literal_union {
                        format!("{}{} | (string & {{}}){}", prefix, rhs, suffix)
                    } else {
                        caps[0].to_string()
                    }
                }).to_string();
            }
            content = widen_narrow_ui_props_interfaces(&content);
            content = repair_bare_react_fc_signatures(&content, true);
            content = repair_ui_fc_missing_children_prop(&content);

            // Upgrade legacy catch-all `<div ...>` / non-`asChild` stubs previously emitted by older Axiom versions
            for (sym_up, semantic_def) in [
                ("Table", "export const Table = ({ children, className = '', ...props }: any) => <table className={`w-full caption-bottom text-sm ${className}`} {...props}>{children}</table>;"),
                ("TableHeader", "export const TableHeader = ({ children, className = '', ...props }: any) => <thead className={className} {...props}>{children}</thead>;"),
                ("TableBody", "export const TableBody = ({ children, className = '', ...props }: any) => <tbody className={className} {...props}>{children}</tbody>;"),
                ("TableFooter", "export const TableFooter = ({ children, className = '', ...props }: any) => <tfoot className={className} {...props}>{children}</tfoot>;"),
                ("TableRow", "export const TableRow = ({ children, className = '', ...props }: any) => <tr className={`border-b border-slate-200 transition-colors ${className}`} {...props}>{children}</tr>;"),
                ("TableHead", "export const TableHead = ({ children, className = '', ...props }: any) => <th className={`px-3 py-2 text-left font-medium text-slate-600 ${className}`} {...props}>{children}</th>;"),
                ("TableCell", "export const TableCell = ({ children, className = '', ...props }: any) => <td className={`px-3 py-2 align-middle ${className}`} {...props}>{children}</td>;"),
                ("TableCaption", "export const TableCaption = ({ children, className = '', ...props }: any) => <caption className={`mt-2 text-sm text-slate-500 ${className}`} {...props}>{children}</caption>;"),
                ("Label", "export const Label = ({ children, className = '', ...props }: any) => <label className={`text-sm font-medium text-slate-700 ${className}`} {...props}>{children}</label>;"),
                ("Spinner", "export const Spinner = ({ className = '', ...props }: any) => <span role=\"status\" aria-label=\"Loading\" className={`inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent ${className}`} {...props} />;"),
                ("Separator", "export const Separator = ({ className = '', ...props }: any) => <hr className={`my-2 border-slate-200 ${className}`} {...props} />;"),
                ("Link", "export const Link = ({ children, href = '#', prefetch, replace, scroll, shallow, locale, legacyBehavior, className = '', ...props }: any) => <a href={typeof href === 'string' ? href : '#'} className={className} {...props}>{children}</a>;"),
                ("CardDescription", "export const CardDescription = ({ children, className = '', ...props }: any) => <p className={`text-sm text-slate-500 ${className}`} {...props}>{children}</p>;"),
                ("DialogDescription", "export const DialogDescription = ({ children, className = '', ...props }: any) => <p className={`text-sm text-slate-500 ${className}`} {...props}>{children}</p>;"),
                ("AlertDescription", "export const AlertDescription = ({ children, className = '', ...props }: any) => <p className={`text-sm text-slate-600 ${className}`} {...props}>{children}</p>;"),
                ("DialogTitle", "export const DialogTitle = ({ children, className = '', ...props }: any) => <h4 className={`text-base font-semibold text-slate-900 ${className}`} {...props}>{children}</h4>;"),
                ("AlertTitle", "export const AlertTitle = ({ children, className = '', ...props }: any) => <h4 className={`text-base font-semibold text-slate-900 ${className}`} {...props}>{children}</h4>;"),
            ] {
                let legacy_div = format!(
                    "export const {} = ({{ children, className = '', ...props }}: any) => <div className={{className}} {{...props}}>{{children}}</div>;",
                    sym_up
                );
                if content.contains(&legacy_div) {
                    content = content.replace(&legacy_div, semantic_def);
                }
            }
            let legacy_btn = "export const Button = ({ children, variant = 'primary', size = 'md', className = '', ...props }: any) => <button className={`inline-flex items-center justify-center rounded-lg px-4 py-2 text-sm font-medium transition-colors ${className}`} {...props}>{children}</button>;";
            let upgraded_btn = "export const Button = ({ children, variant = 'primary', size = 'md', asChild = false, className = '', ...props }: any) => { const cls = `inline-flex items-center justify-center rounded-lg px-4 py-2 text-sm font-medium transition-colors ${className}`; return asChild ? <span className={cls} {...props}>{children}</span> : <button className={cls} {...props}>{children}</button>; };";
            if content.contains(legacy_btn) {
                content = content.replace(legacy_btn, upgraded_btn);
            }
        }

        // Ensure compound sub-components (`<Tabs.List>` -> `TabsList` + `(Tabs as any).List = TabsList`)
        let mut combined_demands: std::collections::BTreeSet<String> =
            demanded.get(&ui_file).cloned().unwrap_or_default();
        if let Some(comp_map) = compound_demands.get(&ui_file) {
            for (parent, subs) in comp_map {
                combined_demands.insert(parent.clone());
                for sub in subs {
                    let sub_sym = format!("{}{}", parent, sub);
                    let plain_exists = has_named_export_symbol(&content, sub);
                    let combined_exists = has_named_export_symbol(&content, &sub_sym);
                    if !combined_exists && !plain_exists {
                        combined_demands.insert(sub_sym);
                    }
                }
            }
        }

        if !combined_demands.is_empty() {
            let mut appended = String::new();
            let mut added_syms: Vec<String> = Vec::new();
            for sym in &combined_demands {
                if !has_named_export_symbol(&content, sym) {
                    let fallback = match sym.as_str() {
                        "Option" => "export const Option = ({ children, ...props }: any) => <option {...props}>{children}</option>;\n".to_string(),
                        "Button" => "export const Button = ({ children, variant = 'primary', size = 'md', asChild = false, className = '', ...props }: any) => { const cls = `inline-flex items-center justify-center rounded-lg px-4 py-2 text-sm font-medium transition-colors ${className}`; return asChild ? <span className={cls} {...props}>{children}</span> : <button className={cls} {...props}>{children}</button>; };\n".to_string(),
                        "Card" | "CardContent" => format!("export const {} = ({{ children, className = '', ...props }}: any) => <div className={{`rounded-xl border border-slate-200 bg-white p-4 shadow-sm ${{className}}`}} {{...props}}>{{children}}</div>;\n", sym),
                        "CardHeader" => "export const CardHeader = ({ children, className = '', ...props }: any) => <div className={`mb-3 flex flex-col gap-1 ${className}`} {...props}>{children}</div>;\n".to_string(),
                        "CardTitle" => "export const CardTitle = ({ children, className = '', ...props }: any) => <h3 className={`text-base font-semibold text-slate-900 ${className}`} {...props}>{children}</h3>;\n".to_string(),
                        "CardDescription" | "DialogDescription" => format!("export const {} = ({{ children, className = '', ...props }}: any) => <p className={{`text-sm text-slate-500 ${{className}}`}} {{...props}}>{{children}}</p>;\n", sym),
                        "Badge" => "export const Badge = ({ children, variant = 'default', className = '', ...props }: any) => <span className={`inline-flex items-center rounded-full bg-slate-100 px-2.5 py-0.5 text-xs font-medium text-slate-800 ${className}`} {...props}>{children}</span>;\n".to_string(),
                        "Input" => "export const Input = ({ label, error, className = '', ...props }: any) => <input className={`w-full rounded-lg border border-slate-300 px-3 py-2 text-sm ${className}`} {...props} />;\n".to_string(),
                        "Textarea" => "export const Textarea = ({ label, error, className = '', ...props }: any) => <textarea className={`w-full rounded-lg border border-slate-300 px-3 py-2 text-sm ${className}`} {...props} />;\n".to_string(),
                        "Select" => "export const Select = ({ children, options, label, className = '', ...props }: any) => <select className={`w-full rounded-lg border border-slate-300 px-3 py-2 text-sm ${className}`} {...props}>{Array.isArray(options) ? options.map((opt: any, idx: number) => <option key={idx} value={opt?.value ?? opt}>{opt?.label ?? opt}</option>) : children}</select>;\n".to_string(),
                        "Label" => "export const Label = ({ children, className = '', ...props }: any) => <label className={`text-sm font-medium text-slate-700 ${className}`} {...props}>{children}</label>;\n".to_string(),
                        "Checkbox" => "export const Checkbox = ({ className = '', ...props }: any) => <input type=\"checkbox\" className={`h-4 w-4 rounded border-slate-300 ${className}`} {...props} />;\n".to_string(),
                        "Switch" => "export const Switch = ({ children, className = '', ...props }: any) => <button type=\"button\" role=\"switch\" className={`inline-flex h-5 w-9 items-center rounded-full bg-slate-200 transition-colors ${className}`} {...props}>{children}</button>;\n".to_string(),
                        "TabsList" => "export const TabsList = ({ children, className = '', ...props }: any) => <div role=\"tablist\" className={`inline-flex items-center gap-1 rounded-lg bg-slate-100 p-1 ${className}`} {...props}>{children}</div>;\n".to_string(),
                        "TabsTrigger" => "export const TabsTrigger = ({ children, value, className = '', ...props }: any) => <button type=\"button\" role=\"tab\" data-value={value} className={`rounded-md px-3 py-1.5 text-sm font-medium transition-colors hover:bg-white ${className}`} {...props}>{children}</button>;\n".to_string(),
                        "TabsContent" => "export const TabsContent = ({ children, value, className = '', ...props }: any) => <div role=\"tabpanel\" data-value={value} className={className} {...props}>{children}</div>;\n".to_string(),
                        "MetricCard" => "export const MetricCard = ({ title, label, value, subtitle, trend, className = '', children, ...props }: any) => <div className={`rounded-xl border border-slate-200 bg-white p-4 shadow-sm ${className}`} {...props}><div className=\"text-xs font-medium text-slate-500\">{title || label}</div><div className=\"mt-1 text-2xl font-bold text-slate-900\">{value}</div>{subtitle ? <div className=\"mt-1 text-xs text-slate-500\">{subtitle}</div> : null}{children}</div>;\n".to_string(),
                        "Modal" => "export const Modal = ({ isOpen = true, open = true, onClose, title, children, className = '', ...props }: any) => (!isOpen && !open) ? null : <div className=\"fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4\"><div className={`w-full max-w-lg rounded-xl bg-white p-6 shadow-xl ${className}`} {...props}>{title ? <div className=\"mb-4 flex items-center justify-between\"><h3 className=\"text-lg font-semibold\">{title}</h3>{onClose ? <button type=\"button\" onClick={onClose} className=\"text-slate-400 hover:text-slate-600\">×</button> : null}</div> : null}{children}</div></div>;\n".to_string(),
                        "DialogTitle" | "AlertTitle" => format!("export const {} = ({{ children, className = '', ...props }}: any) => <h4 className={{`text-base font-semibold text-slate-900 ${{className}}`}} {{...props}}>{{children}}</h4>;\n", sym),
                        "Alert" => "export const Alert = ({ children, className = '', ...props }: any) => <div role=\"alert\" className={`rounded-lg border border-slate-200 bg-slate-50 p-4 text-sm text-slate-800 ${className}`} {...props}>{children}</div>;\n".to_string(),
                        "AlertDescription" => "export const AlertDescription = ({ children, className = '', ...props }: any) => <p className={`text-sm text-slate-600 ${className}`} {...props}>{children}</p>;\n".to_string(),
                        "Spinner" => "export const Spinner = ({ className = '', ...props }: any) => <span role=\"status\" aria-label=\"Loading\" className={`inline-block h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent ${className}`} {...props} />;\n".to_string(),
                        "Skeleton" => "export const Skeleton = ({ children, className = '', ...props }: any) => <div className={`animate-pulse rounded-md bg-slate-200 ${className}`} {...props}>{children}</div>;\n".to_string(),
                        "Separator" => "export const Separator = ({ className = '', ...props }: any) => <hr className={`my-2 border-slate-200 ${className}`} {...props} />;\n".to_string(),
                        "Table" => "export const Table = ({ children, className = '', ...props }: any) => <table className={`w-full caption-bottom text-sm ${className}`} {...props}>{children}</table>;\n".to_string(),
                        "TableHeader" => "export const TableHeader = ({ children, className = '', ...props }: any) => <thead className={className} {...props}>{children}</thead>;\n".to_string(),
                        "TableBody" => "export const TableBody = ({ children, className = '', ...props }: any) => <tbody className={className} {...props}>{children}</tbody>;\n".to_string(),
                        "TableFooter" => "export const TableFooter = ({ children, className = '', ...props }: any) => <tfoot className={className} {...props}>{children}</tfoot>;\n".to_string(),
                        "TableRow" => "export const TableRow = ({ children, className = '', ...props }: any) => <tr className={`border-b border-slate-200 transition-colors ${className}`} {...props}>{children}</tr>;\n".to_string(),
                        "TableHead" => "export const TableHead = ({ children, className = '', ...props }: any) => <th className={`px-3 py-2 text-left font-medium text-slate-600 ${className}`} {...props}>{children}</th>;\n".to_string(),
                        "TableCell" => "export const TableCell = ({ children, className = '', ...props }: any) => <td className={`px-3 py-2 align-middle ${className}`} {...props}>{children}</td>;\n".to_string(),
                        "TableCaption" => "export const TableCaption = ({ children, className = '', ...props }: any) => <caption className={`mt-2 text-sm text-slate-500 ${className}`} {...props}>{children}</caption>;\n".to_string(),
                        "Link" => "export const Link = ({ children, href = '#', prefetch, replace, scroll, shallow, locale, legacyBehavior, className = '', ...props }: any) => <a href={typeof href === 'string' ? href : '#'} className={className} {...props}>{children}</a>;\n".to_string(),
                        _ => format!("export const {} = ({{ children, className = '', ...props }}: any) => <div className={{className}} {{...props}}>{{children}}</div>;\n", sym),
                    };
                    appended.push_str(&fallback);
                    added_syms.push(sym.clone());
                }
            }
            if !appended.is_empty() {
                if !content.contains("import React") && !content.contains("from 'react'") && !content.contains("from \"react\"") {
                    content = if content.trim().is_empty() {
                        "import React from 'react';\n".to_string()
                    } else {
                        format!("import React from 'react';\n{}", content)
                    };
                }
                if !content.trim_start().starts_with("\"use client\"") && !content.trim_start().starts_with("'use client'") {
                    content = format!("\"use client\";\n\n{}", content);
                }
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(&appended);
                if !existed_before && !content.contains("export default") {
                    if let Some(stem) = ui_file.file_stem().and_then(|s| s.to_str()) {
                        if let Some(primary) = added_syms.iter().find(|s| s.as_str() == stem).or_else(|| added_syms.first()) {
                            content.push_str(&format!("\nexport default {};\n", primary));
                        }
                    }
                }
            }
        }

        // Attach runtime sub-properties (`Tabs.List = TabsList`) and allow TypeScript dot-access (`& Record<string, any>`)
        if let Some(comp_map) = compound_demands.get(&ui_file) {
            let mut attach_lines = String::new();
            for (parent, subs) in comp_map {
                let esc_p = regex::escape(parent);
                // Widen exported symbol annotation to allow compound `.Sub` JSX access in TypeScript
                if let Ok(re_fn) = regex::Regex::new(&format!(r#"(?m)^export\s+function\s+{}\b"#, esc_p)) {
                    if re_fn.is_match(&content) {
                        content = re_fn.replace(&content, format!("function {}", parent).as_str()).to_string();
                        if let Ok(re_def) = regex::Regex::new(&format!(r#"(?m)^export\s+default\s+{}\s*;?"#, esc_p)) {
                            content = re_def.replace(&content, "").to_string();
                        }
                        let mut prop_pairs: Vec<String> = Vec::new();
                        for sub in subs {
                            let combined = format!("{}{}", parent, sub);
                            let child_ref = if has_named_export_symbol(&content, &combined) {
                                combined
                            } else if has_named_export_symbol(&content, sub) {
                                sub.clone()
                            } else {
                                continue;
                            };
                            prop_pairs.push(format!("{}: {}", sub, child_ref));
                        }
                        if !prop_pairs.is_empty() && !content.contains(&format!("const __AxiomCompound{}", parent)) {
                            attach_lines.push_str(&format!(
                                "const __AxiomCompound{}: React.FC<any> & Record<string, any> = Object.assign({}, {{ {} }});\nexport {{ __AxiomCompound{} as {} }};\n",
                                parent, parent, prop_pairs.join(", "), parent, parent
                            ));
                            if !content.contains("export default") {
                                attach_lines.push_str(&format!("export default __AxiomCompound{};\n", parent));
                            }
                        }
                        continue;
                    }
                }

                // For `export const Tabs...` / `const Tabs...`, ensure TS type allows `& Record<string, any>`
                let has_record_on_parent = regex::Regex::new(&format!(
                    r#"(?m)^(?:export\s+)?const\s+{}\b[^\n=]*Record<string,\s*any>"#,
                    esc_p
                ))
                .map(|r| r.is_match(&content))
                .unwrap_or(false);
                if !has_record_on_parent {
                    if let Ok(re_fc_ann) = regex::Regex::new(&format!(
                        r#"(?m)^((?:export\s+)?const\s+{}\s*:\s*React\.(?:FC|FunctionComponent)(?:<[^>\n]+>)?)\s*="#,
                        esc_p
                    )) {
                        if re_fc_ann.is_match(&content) {
                            content = re_fc_ann.replace(&content, "${1} & Record<string, any> =").to_string();
                        } else if let Ok(re_untyped) = regex::Regex::new(&format!(
                            r#"(?m)^((?:export\s+)?const\s+{})\s*="#,
                            esc_p
                        )) {
                            content = re_untyped
                                .replace(&content, "${1}: React.FC<any> & Record<string, any> =")
                                .to_string();
                        }
                    }
                }

                for sub in subs {
                    let already_attached = content.contains(&format!("({}).{} =", parent, sub))
                        || content.contains(&format!("{}.{} =", parent, sub))
                        || content.contains(&format!("Object.assign({},", parent));
                    if already_attached {
                        continue;
                    }
                    let combined = format!("{}{}", parent, sub);
                    let child_ref = if has_named_export_symbol(&content, &combined) {
                        combined
                    } else if has_named_export_symbol(&content, sub) {
                        sub.clone()
                    } else {
                        continue;
                    };
                    attach_lines.push_str(&format!(
                        "if (typeof {} === 'function' || (typeof {} === 'object' && {} !== null)) {{ ({} as any).{} = {}; }}\n",
                        parent, parent, parent, parent, sub, child_ref
                    ));
                }
            }
            if !attach_lines.is_empty() {
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(&attach_lines);
            }
        }

        if content != original && !content.trim().is_empty() {
            if let Some(parent) = ui_file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&ui_file, content);
        }
    }
}

pub fn ensure_valid_tsconfig(proj_dir: &std::path::Path) {
    let tsconfig_path = proj_dir.join("tsconfig.json");
    let has_src = proj_dir.join("src").exists();
    let has_root_app_layout = !has_src
        && (proj_dir.join("app").exists()
            || proj_dir.join("components").exists()
            || proj_dir.join("lib").exists()
            || proj_dir.join("hooks").exists());
    let expected_alias_target = if has_root_app_layout { "./*" } else { "./src/*" };

    if tsconfig_path.exists() {
        let mut needs_write = false;
        let mut parsed_val = None;
        if let Ok(content) = std::fs::read_to_string(&tsconfig_path) {
            if let Ok(mut val) = serde_json::from_str::<Value>(&content) {
                if let Some(co) = val.get_mut("compilerOptions").and_then(|c| c.as_object_mut()) {
                    if !co.get("baseUrl").and_then(|v| v.as_str()).map_or(false, |s| s == ".") {
                        co.insert("baseUrl".to_string(), serde_json::json!("."));
                        needs_write = true;
                    }
                    let has_alias = co.get("paths")
                        .and_then(|p| p.get("@/*"))
                        .and_then(|a| a.as_array())
                        .map_or(false, |arr| {
                            arr.iter().any(|x| {
                                let s = x.as_str().unwrap_or("");
                                if has_root_app_layout {
                                    s == "./*" || s == "*"
                                } else {
                                    s == "./src/*" || s == "src/*"
                                }
                            })
                        });
                    if !has_alias {
                        let paths = co.entry("paths").or_insert_with(|| serde_json::json!({}));
                        if let Some(paths_obj) = paths.as_object_mut() {
                            paths_obj.insert("@/*".to_string(), serde_json::json!([expected_alias_target]));
                            needs_write = true;
                        }
                    }
                    parsed_val = Some(val);
                }
            }
        }

        if let Some(val) = parsed_val {
            if needs_write {
                if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                    let _ = std::fs::write(&tsconfig_path, pretty);
                }
            }
        } else {
            let standard_tsconfig = serde_json::json!({
                "compilerOptions": {
                    "baseUrl": ".",
                    "lib": ["dom", "dom.iterable", "esnext"],
                    "allowJs": true,
                    "skipLibCheck": true,
                    "strict": false,
                    "noEmit": true,
                    "esModuleInterop": true,
                    "module": "esnext",
                    "moduleResolution": "bundler",
                    "resolveJsonModule": true,
                    "isolatedModules": true,
                    "jsx": "preserve",
                    "incremental": true,
                    "plugins": [
                        {
                            "name": "next"
                        }
                    ],
                    "paths": {
                        "@/*": [expected_alias_target]
                    }
                },
                "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
                "exclude": ["node_modules"]
            });
            let _ = std::fs::write(&tsconfig_path, serde_json::to_string_pretty(&standard_tsconfig).unwrap_or_default());
        }
    }
}

/// Checks whether a byte mask of code positions (outside strings and comments, while keeping `${...}` expressions active) applies.
fn compute_ts_code_mask(src: &str) -> Vec<bool> {
    let bytes = src.as_bytes();
    let mut mask = vec![true; bytes.len()];
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut tmpl_brace_stack: Vec<usize> = Vec::new();
    let mut in_tmpl_text = false;
    let mut brace_depth = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if in_line_comment {
            mask[i] = false;
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            mask[i] = false;
            if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                mask[i + 1] = false;
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_single {
            mask[i] = false;
            if b == b'\\' && i + 1 < bytes.len() {
                mask[i + 1] = false;
                i += 2;
                continue;
            }
            if b == b'\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            mask[i] = false;
            if b == b'\\' && i + 1 < bytes.len() {
                mask[i + 1] = false;
                i += 2;
                continue;
            }
            if b == b'"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        if in_tmpl_text {
            mask[i] = false;
            if b == b'\\' && i + 1 < bytes.len() {
                mask[i + 1] = false;
                i += 2;
                continue;
            }
            if b == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                mask[i + 1] = false;
                tmpl_brace_stack.push(brace_depth);
                brace_depth += 1;
                in_tmpl_text = false;
                i += 2;
                continue;
            }
            if b == b'`' {
                in_tmpl_text = false;
            }
            i += 1;
            continue;
        }

        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            mask[i] = false;
            mask[i + 1] = false;
            in_line_comment = true;
            i += 2;
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            mask[i] = false;
            mask[i + 1] = false;
            in_block_comment = true;
            i += 2;
            continue;
        }
        if b == b'\'' {
            mask[i] = false;
            in_single = true;
            i += 1;
            continue;
        }
        if b == b'"' {
            mask[i] = false;
            in_double = true;
            i += 1;
            continue;
        }
        if b == b'`' {
            mask[i] = false;
            in_tmpl_text = true;
            i += 1;
            continue;
        }
        if b == b'{' {
            brace_depth += 1;
        } else if b == b'}' && brace_depth > 0 {
            brace_depth -= 1;
            if let Some(&entry_depth) = tmpl_brace_stack.last() {
                if brace_depth == entry_depth {
                    tmpl_brace_stack.pop();
                    in_tmpl_text = true;
                }
            }
        }
        i += 1;
    }

    mask
}

/// Rewrites uncast optional credential field accesses on Prisma/auth user records
/// (e.g. `user.hashedPassword || user.password`) to `(user as any)?.hashedPassword || (user as any)?.password`,
/// eliminating TS2339 when `schema.prisma` defines only one of the two password field names.
/// Skips comments, string literals, chained member accesses (`req.user.password`), existing casts (`(user as any).password`),
/// and assignment LHS targets (`user.password = ...`).
pub fn sanitize_optional_user_password_access(content: &str) -> String {
    let has_auth_ctx = content.contains("hashedPassword")
        || content.contains("passwordHash")
        || content.contains("CredentialsProvider")
        || content.contains("bcrypt")
        || (content.contains("prisma.") && content.contains(".password"));
    if !has_auth_ctx || (!content.contains("user") && !content.contains("User")) {
        return content.to_string();
    }

    let Ok(re) = regex::Regex::new(
        r"\b(user|foundUser|existingUser|dbUser|authUser|targetUser)\s*(\??\.)\s*(hashedPassword|passwordHash|password)\b",
    ) else {
        return content.to_string();
    };

    let code_mask = compute_ts_code_mask(content);
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for cap in re.captures_iter(content) {
        let Some(full_m) = cap.get(0) else { continue };
        let (Some(ident_m), Some(field_m)) = (cap.get(1), cap.get(3)) else {
            continue;
        };
        let start = full_m.start();
        let end = full_m.end();
        if start >= code_mask.len() || end > code_mask.len() || !code_mask[start..end].iter().all(|&ok| ok) {
            continue;
        }

        let before_trim = content[..start].trim_end();
        if before_trim.ends_with('.') {
            continue;
        }

        let after_ws = content[end..].trim_start_matches([' ', '\t']);
        if (after_ws.starts_with('=') && !after_ws.starts_with("==") && !after_ws.starts_with("=>"))
            || after_ws.starts_with("??=")
            || after_ws.starts_with("+=")
            || after_ws.starts_with("-=")
        {
            continue;
        }

        let repl = format!("({} as any)?.{}", ident_m.as_str(), field_m.as_str());
        replacements.push((start, end, repl));
    }

    if replacements.is_empty() {
        return content.to_string();
    }

    let mut out = String::with_capacity(content.len() + replacements.len() * 12);
    let mut cursor = 0usize;
    for (start, end, repl) in replacements {
        if start >= cursor && end <= content.len() {
            out.push_str(&content[cursor..start]);
            out.push_str(&repl);
            cursor = end;
        }
    }
    out.push_str(&content[cursor..]);
    out
}

pub fn sanitize_react_icons_imports(content: &str) -> String {
    let normalized = content
        .replace("HiOutlineDollarSign", "HiOutlineCurrencyDollar")
        .replace("HiDollarSign", "HiCurrencyDollar");
    let mut result = Vec::new();
    let lines: Vec<&str> = normalized.lines().collect();
    
    for line in lines {
        let trimmed = line.trim();
        let is_mismatched_io = (trimmed.contains("from 'react-icons/io5'") || trimmed.contains("from \"react-icons/io5\""))
            && (trimmed.contains("IoIos") || trimmed.contains("IoMd"));
        let is_bare_react_icons = (trimmed.starts_with("import ") || trimmed.starts_with("import{"))
            && (trimmed.ends_with("from 'react-icons';") 
                || trimmed.ends_with(r#"from "react-icons";"#)
                || trimmed.ends_with("from 'react-icons'")
                || trimmed.ends_with(r#"from "react-icons""#)
                || trimmed.contains("-icons-react'")
                || trimmed.contains("-icons-react\"")
                || is_mismatched_io);

        if is_bare_react_icons {
            if let Some(open_brace) = line.find('{') {
                if let Some(close_brace) = line.find('}') {
                    let icons_str = &line[open_brace + 1..close_brace];
                    let mut subpath_map: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
                    
                    for raw_icon in icons_str.split(',') {
                        let icon = raw_icon.trim();
                        if icon.is_empty() {
                            continue;
                        }
                        
                        let ident = icon.split_whitespace().next().unwrap_or(icon);
                        
                        let subpath = if ident.starts_with("Hi2") {
                            "react-icons/hi2"
                        } else if ident.starts_with("Hi") {
                            "react-icons/hi"
                        } else if ident.starts_with("Fa6") {
                            "react-icons/fa6"
                        } else if ident.starts_with("Fa") {
                            "react-icons/fa"
                        } else if ident.starts_with("Fi") {
                            "react-icons/fi"
                        } else if ident.starts_with("Md") {
                            "react-icons/md"
                        } else if ident.starts_with("IoMd") || ident.starts_with("IoIos") {
                            "react-icons/io"
                        } else if ident.starts_with("Io5") || ident.starts_with("Io") {
                            "react-icons/io5"
                        } else if ident.starts_with("Ai") {
                            "react-icons/ai"
                        } else if ident.starts_with("Bs") {
                            "react-icons/bs"
                        } else if ident.starts_with("Ri") {
                            "react-icons/ri"
                        } else if ident.starts_with("Bi") {
                            "react-icons/bi"
                        } else if ident.starts_with("Tb") {
                            "react-icons/tb"
                        } else if ident.starts_with("Lu") {
                            "react-icons/lu"
                        } else if ident.starts_with("Si") {
                            "react-icons/si"
                        } else if ident.starts_with("Gi") {
                            "react-icons/gi"
                        } else if ident.starts_with("Ti") {
                            "react-icons/ti"
                        } else if ident.starts_with("Wi") {
                            "react-icons/wi"
                        } else if ident.starts_with("Cg") {
                            "react-icons/cg"
                        } else if ident.starts_with("Vsc") {
                            "react-icons/vsc"
                        } else if ident.starts_with("Go") {
                            "react-icons/go"
                        } else if ident.starts_with("Gr") {
                            "react-icons/gr"
                        } else if ident.starts_with("Rx") {
                            "react-icons/rx"
                        } else if ident.starts_with("Pi") {
                            "react-icons/pi"
                        } else if ident.starts_with("Im") {
                            "react-icons/im"
                        } else if ident.starts_with("Sl") {
                            "react-icons/sl"
                        } else if ident.starts_with("Tfi") {
                            "react-icons/tfi"
                        } else if ident.starts_with("Lia") {
                            "react-icons/lia"
                        } else {
                            "react-icons/hi"
                        };
                        
                        subpath_map.entry(subpath).or_default().push(icon);
                    }
                    
                    if !subpath_map.is_empty() {
                        for (subpath, icon_list) in subpath_map {
                            result.push(format!("import {{ {} }} from '{}';", icon_list.join(", "), subpath));
                        }
                        continue;
                    }
                }
            }
        }
        result.push(line.to_string());
    }
    
    result.join("\n")
}


pub fn resolve_best_matching_route(href: &str, valid_routes: &[String]) -> Option<String> {
    let clean = href.trim().trim_end_matches('/');
    if valid_routes.iter().any(|r| r == clean) {
        return None;
    }
    let slug = clean.split('/').filter(|s| !s.is_empty()).last()?;
    let stripped_slug = slug.trim_end_matches("-panel").trim_end_matches("-view").trim_end_matches("-page");

    let matches: Vec<&String> = valid_routes
        .iter()
        .filter(|r| {
            let r_clean = r.trim_end_matches('/');
            r_clean.ends_with(&format!("/{}", slug)) 
                || (slug != stripped_slug && r_clean.ends_with(&format!("/{}", stripped_slug)))
        })
        .collect();

    if matches.len() == 1 {
        return Some(matches[0].clone());
    }
    None
}

pub fn sanitize_app_router_links_with_routes(content: &str, valid_routes: &[String]) -> String {
    if valid_routes.is_empty() || (!content.contains("<Link") && !content.contains("router.push")) {
        return content.to_string();
    }

    let mut result = content.to_string();

    if let Ok(re) = regex::Regex::new(r#"href\s*=\s*(["'])(/[^"'\s>]+)["']"#) {
        for cap in re.captures_iter(content) {
            let quote = cap.get(1).map(|m| m.as_str()).unwrap_or("\"");
            let href = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            if let Some(replacement) = resolve_best_matching_route(href, valid_routes) {
                let old_target = format!("href={}{}{}", quote, href, quote);
                let new_target = format!("href={}{}{}", quote, replacement, quote);
                result = result.replace(&old_target, &new_target);
            }
        }
    }

    if let Ok(re) = regex::Regex::new(r#"router\.push\s*\(\s*(["'])(/[^"'\s)]+)["']\s*\)"#) {
        for cap in re.captures_iter(content) {
            let quote = cap.get(1).map(|m| m.as_str()).unwrap_or("\"");
            let href = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            if let Some(replacement) = resolve_best_matching_route(href, valid_routes) {
                let old_target = format!("router.push({}{}{})", quote, href, quote);
                let new_target = format!("router.push({}{}{})", quote, replacement, quote);
                result = result.replace(&old_target, &new_target);
            }
        }
    }

    result
}

pub fn sanitize_unhandled_session_loading(content: &str) -> String {
    let mut modified = content.to_string();

    // Neutralize client-side preview redirect traps (`if (!user) { router.push('/login'); }` or `if (!session) { router.push('/login'); }`)
    if modified.contains("router.push") && (modified.contains("'/login'") || modified.contains("\"/login\"")) {
        if let Ok(re_redirect_block) = regex::Regex::new(
            r#"(?m)^[ \t]*if\s*\(\s*!(?:user|session)\s*\)\s*\{\s*router\.push\s*\(\s*['"]/login['"]\s*\)\s*;?\s*(?:return\s*;?\s*)?\}[ \t]*\r?\n?"#
        ) {
            modified = re_redirect_block.replace_all(&modified, "").to_string();
        }
        if let Ok(re_redirect_line) = regex::Regex::new(
            r#"(?m)^[ \t]*if\s*\(\s*!(?:user|session)\s*\)\s*router\.push\s*\(\s*['"]/login['"]\s*\)\s*;?[ \t]*\r?\n?"#
        ) {
            modified = re_redirect_line.replace_all(&modified, "").to_string();
        }
    }

    if !modified.contains("useSession") {
        return modified;
    }

    // 1. Ensure setLoading(false) is reached when session is null in `if (session) { ... }`
    if modified.contains("setLoading") {
        if let Ok(re) = regex::Regex::new(r"(?s)if\s*\(\s*session\s*\)\s*\{([^}]+)\}") {
            for cap in re.captures_iter(content) {
                let full_match = cap.get(0).unwrap().as_str();
                let body = cap.get(1).unwrap().as_str();
                if let Some(pos) = content.find(full_match) {
                    let after = content[pos + full_match.len()..].trim_start();
                    if !after.starts_with("else") {
                        let replacement = format!("if (session) {{{}\n  }} else {{\n    setLoading(false);\n  }}", body);
                        modified = modified.replace(full_match, &replacement);
                    }
                }
            }
        }
    }

    // 2. Neutralize preview-blocking early returns (`if (!session) return <div>Loading...</div>;` or `if (status === 'unauthenticated') return <Link...>Log in</Link>;`)
    // ONLY when the component has a main JSX return block below it.
    let return_count = modified.matches("return ").count() + modified.matches("return(").count();
    if return_count >= 2 {
        // Block form: if (!session) { return ...; } or if (status === 'unauthenticated') { return ...; }
        if let Ok(re_block) = regex::Regex::new(
            r#"(?m)^[ \t]*if\s*\(\s*(?:!session|status\s*===\s*['"]unauthenticated['"])\s*\)\s*\{\s*return\s+[^}]*?;\s*\}[ \t]*\r?\n?"#
        ) {
            modified = re_block.replace_all(&modified, "").to_string();
        }

        // Single-line form: if (!session) return ...; or if (status === 'unauthenticated') return ...;
        if let Ok(re_line) = regex::Regex::new(
            r#"(?m)^[ \t]*if\s*\(\s*(?:!session|status\s*===\s*['"]unauthenticated['"])\s*\)\s*return\s+[^\n;]+;[ \t]*\r?\n?"#
        ) {
            modified = re_line.replace_all(&modified, "").to_string();
        }
    }

    modified
}

pub fn sanitize_root_layout_direct_use_session(content: &str) -> String {
    if !content.contains("useSession(") {
        return content.to_string();
    }

    // Locate the RootLayout component function body span
    let root_re = match regex::Regex::new(
        r#"(?m)(?:export\s+default\s+(?:async\s+)?function\s+RootLayout\b|export\s+(?:async\s+)?function\s+RootLayout\b|function\s+RootLayout\b|const\s+RootLayout\b)"#
    ) {
        Ok(r) => r,
        Err(_) => return content.to_string(),
    };

    let Some(root_m) = root_re.find(content) else {
        return content.to_string();
    };

    // Find opening `{` of RootLayout body (skipping parameter destructuring `{ children }`)
    let mut scan = root_m.end();
    let bytes = content.as_bytes();
    let mut paren_depth: i32 = 0;
    let mut angle_depth: i32 = 0;
    let mut body_open: Option<usize> = None;
    let mut after_arrow_or_params = false;
    while scan < bytes.len() {
        let ch = bytes[scan] as char;
        if ch == '(' && angle_depth == 0 {
            paren_depth += 1;
            after_arrow_or_params = true;
        } else if ch == ')' && angle_depth == 0 {
            paren_depth = (paren_depth - 1).max(0);
        } else if ch == '<' && paren_depth == 0 {
            angle_depth += 1;
        } else if ch == '>' && angle_depth > 0 {
            if scan > 0 && bytes[scan - 1] == b'=' {
                after_arrow_or_params = true;
            } else {
                angle_depth = (angle_depth - 1).max(0);
            }
        } else if ch == '{' && paren_depth == 0 && angle_depth == 0 && after_arrow_or_params {
            body_open = Some(scan);
            break;
        }
        scan += 1;
    }

    let Some(open_idx) = body_open else {
        return content.to_string();
    };

    let mut depth = 0i32;
    let mut body_close = content.len();
    let mut i = open_idx;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_tick = false;
    while i < bytes.len() {
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
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    body_close = i + 1;
                    break;
                }
            }
        }
        i += 1;
    }

    let body_slice = &content[open_idx..body_close];
    if !body_slice.contains("useSession(") {
        return content.to_string();
    }

    let destruct_re = regex::Regex::new(
        r#"^(\s*)(?:const|let|var)\s*\{([^}]*)\}\s*=\s*useSession\s*\([^)]*\)\s*(?:(?:\?\?|\|\|)\s*\{\s*\})?\s*(?:as\s+[^;]+)?;?\s*$"#
    ).ok();
    let plain_call_re = regex::Regex::new(r#"useSession\s*\([^)]*\)"#).ok();

    let mut new_body_lines: Vec<String> = Vec::new();
    for line in body_slice.lines() {
        if line.contains("useSession(") {
            if let Some(ref re) = destruct_re {
                if let Some(cap) = re.captures(line) {
                    let indent = cap.get(1).map(|m| m.as_str()).unwrap_or("  ");
                    let inner = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                    let mut stmts: Vec<String> = Vec::new();
                    for raw_part in inner.split(',') {
                        let lhs = raw_part.split('=').next().unwrap_or("").trim();
                        if lhs.is_empty() {
                            continue;
                        }
                        let (key, var_name) = if let Some((k, a)) = lhs.split_once(':') {
                            (k.trim(), a.trim())
                        } else {
                            (lhs, lhs)
                        };
                        if var_name.is_empty() {
                            continue;
                        }
                        if key == "data" {
                            stmts.push(format!("{}const {}: any = null;", indent, var_name));
                        } else if key == "status" {
                            stmts.push(format!("{}const {} = 'unauthenticated';", indent, var_name));
                        } else if key == "update" {
                            stmts.push(format!("{}const {} = async () => null;", indent, var_name));
                        } else {
                            stmts.push(format!("{}const {}: any = null;", indent, var_name));
                        }
                    }
                    if stmts.is_empty() {
                        new_body_lines.push(format!("{}// useSession neutralized in RootLayout", indent));
                    } else {
                        new_body_lines.extend(stmts);
                    }
                    continue;
                }
            }
            if let Some(ref call_re) = plain_call_re {
                new_body_lines.push(
                    call_re
                        .replace_all(line, "({ data: null, status: 'unauthenticated', update: async () => null } as any)")
                        .to_string(),
                );
                continue;
            }
        }
        new_body_lines.push(line.to_string());
    }

    let mut result = format!("{}{}{}", &content[..open_idx], new_body_lines.join("\n"), &content[body_close..]);

    // If `useSession` is no longer called anywhere, prune it from `next-auth/react` imports
    let body_still_uses_session = result.contains("useSession(")
        || result.lines().any(|l| {
            let t = l.trim();
            !t.starts_with("import ") && t.contains("useSession")
        });
    if !body_still_uses_session {
        let mut cleaned_lines: Vec<String> = Vec::new();
        for line in result.lines() {
            let t = line.trim();
            if t.starts_with("import ") && t.contains("useSession") && (t.contains("next-auth/react") || t.contains("next-auth")) {
                if let (Some(ob), Some(cb)) = (line.find('{'), line.find('}')) {
                    let syms: Vec<&str> = line[ob + 1..cb]
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty() && *s != "useSession")
                        .collect();
                    if syms.is_empty() {
                        continue;
                    }
                    let rewritten = format!("{}{{ {} }}{}", &line[..ob], syms.join(", "), &line[cb + 1..]);
                    cleaned_lines.push(rewritten);
                    continue;
                }
            }
            cleaned_lines.push(line.to_string());
        }
        result = cleaned_lines.join("\n");
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }
    }

    result
}

pub fn normalize_project_route_links(proj_dir: &std::path::Path, valid_routes: &[String]) {
    fn walk_and_sanitize(dir: &std::path::Path, valid_routes: &[String]) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != "node_modules" && name != ".next" && name != "build" && name != "dist" && name != ".git" {
                        walk_and_sanitize(&path, valid_routes);
                    }
                } else if path.is_file() {
                    let path_str = path.to_string_lossy();
                    if path_str.ends_with(".tsx") || path_str.ends_with(".jsx") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if content.contains("<Link") || content.contains("router.push") {
                                let sanitized = sanitize_app_router_links_with_routes(&content, valid_routes);
                                if sanitized != content {
                                    let _ = std::fs::write(&path, sanitized);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    walk_and_sanitize(proj_dir, valid_routes);
}

pub fn extract_import_specifier(stmt_flat: &str) -> Option<String> {
    let trimmed = stmt_flat.trim();
    if let Some(rest) = trimmed.strip_prefix("import") {
        let after_kw = rest.trim_start();
        if after_kw.starts_with('\'') || after_kw.starts_with('"') {
            let q = after_kw.chars().next()?;
            let inner = &after_kw[1..];
            if let Some(end) = inner.find(q) {
                return Some(inner[..end].trim().to_string());
            }
        }
    }
    if let Some(idx) = trimmed.rfind(" from ") {
        let after_from = trimmed[idx + 6..].trim_start();
        let q = after_from.chars().next()?;
        if q == '\'' || q == '"' {
            let inner = &after_from[1..];
            if let Some(end) = inner.find(q) {
                return Some(inner[..end].trim().to_string());
            }
        }
    }
    for pat in ["from'", "from\""] {
        if let Some(idx) = trimmed.rfind(pat) {
            let after_from = &trimmed[idx + 4..];
            let q = after_from.chars().next()?;
            let inner = &after_from[1..];
            if let Some(end) = inner.find(q) {
                return Some(inner[..end].trim().to_string());
            }
        }
    }
    None
}

/// Returns true if `file_path` is a special Next.js App Router route convention file
/// (e.g. `page.tsx`, `layout.tsx`, `loading.tsx`, `error.tsx`, `not-found.tsx`,
/// `template.tsx`, `default.tsx`, `global-error.tsx`) outside API routes.
pub fn is_app_router_route_file(file_path: &str) -> bool {
    let norm = file_path
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_lowercase();
    let in_app_dir = norm.starts_with("src/app/")
        || norm.starts_with("app/")
        || norm.contains("/app/");
    if !in_app_dir || norm.starts_with("api/") || norm.contains("/api/") {
        return false;
    }
    let file_name = std::path::Path::new(&norm)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    matches!(
        file_name,
        "page.tsx"
            | "page.jsx"
            | "page.ts"
            | "page.js"
            | "layout.tsx"
            | "layout.jsx"
            | "layout.ts"
            | "layout.js"
            | "loading.tsx"
            | "loading.jsx"
            | "loading.js"
            | "error.tsx"
            | "error.jsx"
            | "error.js"
            | "not-found.tsx"
            | "not-found.jsx"
            | "not-found.js"
            | "template.tsx"
            | "template.jsx"
            | "template.js"
            | "default.tsx"
            | "default.jsx"
            | "default.js"
            | "global-error.tsx"
            | "global-error.jsx"
            | "global-error.js"
    )
}

fn is_reserved_nextjs_route_export(name: &str) -> bool {
    matches!(
        name,
        "metadata"
            | "generateMetadata"
            | "viewport"
            | "generateViewport"
            | "dynamic"
            | "dynamicParams"
            | "revalidate"
            | "fetchCache"
            | "runtime"
            | "preferredRegion"
            | "maxDuration"
            | "generateStaticParams"
            | "generateImageMetadata"
            | "generateSitemaps"
    )
}

/// Scans top-level brace depth (`brace_depth == 0`) outside strings and comments to strip
/// illegal non-reserved named value exports from Next.js App Router route files (`page.tsx`,
/// `layout.tsx`, etc.) while keeping `export default`, `export type`/`interface`/`declare`,
/// and official Next.js route segment config/metadata exports intact. Ensures `export default`
/// exists if a component declaration lost its invalid named `export`.
pub fn sanitize_app_router_route_exports(content: &str, file_path: &str) -> String {
    if !is_app_router_route_file(file_path) {
        return content.to_string();
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut out_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut brace_depth: usize = 0;
    let mut in_block_comment = false;
    let mut in_tmpl_text = false;
    let mut tmpl_expr_stack: Vec<usize> = Vec::new();
    let mut has_default_export = false;
    let mut export_lost_component: Option<String> = None;
    let mut fallback_component_candidate: Option<String> = None;

    let is_pascal_case_component = |name: &str| -> bool {
        name.chars().next().map_or(false, |c| c.is_ascii_uppercase())
            && name.chars().any(|c| c.is_ascii_lowercase())
    };
    let is_route_primary_component = |name: &str| -> bool {
        name.ends_with("Page")
            || name.ends_with("Layout")
            || name == "Home"
            || name == "NotFound"
            || name == "Loading"
            || name == "Error"
            || name.ends_with("Error")
            || name.ends_with("Template")
    };

    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        let top_level = brace_depth == 0 && !in_block_comment && !in_tmpl_text && tmpl_expr_stack.is_empty();

        // Track brace depth for this line (to compute depth for the NEXT line).
        let update_depth = |text: &str, depth: &mut usize, blk_cmt: &mut bool, tmpl_txt: &mut bool, expr_stk: &mut Vec<usize>| {
            let chars: Vec<char> = text.chars().collect();
            let mut k = 0usize;
            let mut quote: Option<char> = None;
            while k < chars.len() {
                let c = chars[k];
                if *blk_cmt {
                    if c == '*' && k + 1 < chars.len() && chars[k + 1] == '/' {
                        *blk_cmt = false;
                        k += 2;
                        continue;
                    }
                    k += 1;
                    continue;
                }
                if *tmpl_txt {
                    if c == '\\' {
                        k += 2;
                        continue;
                    }
                    if c == '`' {
                        *tmpl_txt = false;
                    } else if c == '$' && k + 1 < chars.len() && chars[k + 1] == '{' {
                        *depth += 1;
                        expr_stk.push(1);
                        *tmpl_txt = false;
                        k += 2;
                        continue;
                    }
                    k += 1;
                    continue;
                }
                if let Some(q) = quote {
                    if c == '\\' {
                        k += 2;
                        continue;
                    }
                    if c == q {
                        quote = None;
                    }
                    k += 1;
                    continue;
                }
                if c == '/' && k + 1 < chars.len() {
                    if chars[k + 1] == '/' {
                        break;
                    }
                    if chars[k + 1] == '*' {
                        *blk_cmt = true;
                        k += 2;
                        continue;
                    }
                }
                if c == '\'' || c == '"' {
                    quote = Some(c);
                } else if c == '`' {
                    *tmpl_txt = true;
                } else if c == '{' {
                    *depth += 1;
                    if let Some(top) = expr_stk.last_mut() {
                        *top += 1;
                    }
                } else if c == '}' {
                    *depth = depth.saturating_sub(1);
                    if let Some(top) = expr_stk.last_mut() {
                        *top = top.saturating_sub(1);
                        if *top == 0 {
                            expr_stk.pop();
                            *tmpl_txt = true;
                        }
                    }
                }
                k += 1;
            }
        };

        if top_level && (trimmed.starts_with("export default") || (trimmed.starts_with("export {") && trimmed.contains("as default"))) {
            has_default_export = true;
        }

        // Capture PascalCase component declarations even without `export`, prioritizing primary route components (*Page/*Layout/Home)
        if top_level {
            let check_decl = trimmed.strip_prefix("export default ").or_else(|| trimmed.strip_prefix("export ")).unwrap_or(trimmed).trim();
            let cand_opt = if let Some(rest) = check_decl.strip_prefix("async function ").or_else(|| check_decl.strip_prefix("function ")) {
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if is_pascal_case_component(&name) { Some(name) } else { None }
            } else if let Some(rest) = check_decl.strip_prefix("const ") {
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if is_pascal_case_component(&name)
                    && (check_decl.contains("=>")
                        || check_decl.contains("React.FC")
                        || check_decl.contains("FC<")
                        || check_decl.contains(": FC")
                        || check_decl.contains("forwardRef")
                        || check_decl.contains("React.memo")
                        || check_decl.contains("= (")
                        || check_decl.contains("= async (")
                        || check_decl.contains("= function"))
                {
                    Some(name)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(cand) = cand_opt {
                let is_primary = is_route_primary_component(&cand);
                let prev_primary = fallback_component_candidate.as_deref().map_or(false, |p| is_route_primary_component(p));
                if fallback_component_candidate.is_none() || (is_primary && !prev_primary) {
                    fallback_component_candidate = Some(cand);
                }
            }
        }

        if top_level && trimmed.starts_with("export ") {
            if trimmed.starts_with("export default")
                || trimmed.starts_with("export type ")
                || trimmed.starts_with("export type{")
                || trimmed.starts_with("export type *")
                || trimmed.starts_with("export interface ")
                || trimmed.starts_with("export declare ")
                || trimmed.starts_with("export namespace ")
            {
                out_lines.push(line.to_string());
                update_depth(line, &mut brace_depth, &mut in_block_comment, &mut in_tmpl_text, &mut tmpl_expr_stack);
                i += 1;
                continue;
            }

            // Runtime wildcard re-exports (`export * from '...'`) are illegal on Next.js App Router route files
            if trimmed.starts_with("export *") {
                update_depth(line, &mut brace_depth, &mut in_block_comment, &mut in_tmpl_text, &mut tmpl_expr_stack);
                i += 1;
                continue;
            }

            // Local export list `export { A, B };` (may span multiple lines)
            if trimmed.starts_with("export {") || trimmed.starts_with("export{") {
                let mut stmt = line.to_string();
                let mut end_idx = i;
                while !stmt.contains('}') && end_idx + 1 < lines.len() {
                    end_idx += 1;
                    stmt.push('\n');
                    stmt.push_str(lines[end_idx]);
                }
                let norm_stmt: String = stmt.split_whitespace().collect::<Vec<_>>().join(" ");
                let is_reexport_from = norm_stmt.contains("} from '")
                    || norm_stmt.contains("} from \"")
                    || norm_stmt.contains("}from '")
                    || norm_stmt.contains("}from \"");
                if is_reexport_from {
                    // Preserve `export { default } from '...'` or `export { Foo as default } from '...'`
                    if norm_stmt.contains("default") {
                        has_default_export = true;
                        out_lines.push(stmt);
                    } else {
                        // Drop invalid value re-export from external module on Next.js route file
                    }
                    for j in i..=end_idx {
                        update_depth(lines[j], &mut brace_depth, &mut in_block_comment, &mut in_tmpl_text, &mut tmpl_expr_stack);
                    }
                    i = end_idx + 1;
                    continue;
                }

                if let (Some(ob), Some(cb)) = (stmt.find('{'), stmt.rfind('}')) {
                    let inner = &stmt[ob + 1..cb];
                    let mut kept_items = Vec::new();
                    for raw_item in inner.split(',') {
                        let item = raw_item.trim();
                        if item.is_empty() {
                            continue;
                        }
                        if item == "type" || item.starts_with("type ") {
                            kept_items.push(item.to_string());
                            continue;
                        }
                        let (orig_name, exported_name) = if let Some((lhs, rhs)) = item.split_once(" as ") {
                            (lhs.trim(), rhs.trim())
                        } else {
                            (item, item)
                        };
                        if exported_name == "default" {
                            has_default_export = true;
                            kept_items.push(item.to_string());
                        } else if is_reserved_nextjs_route_export(exported_name) {
                            kept_items.push(item.to_string());
                        } else if is_pascal_case_component(orig_name) {
                            let is_route_primary = is_route_primary_component(orig_name);
                            let prev_primary = export_lost_component.as_deref().map_or(false, |p| is_route_primary_component(p));
                            if export_lost_component.is_none() || (is_route_primary && !prev_primary) {
                                export_lost_component = Some(orig_name.to_string());
                            }
                        }
                    }
                    if !kept_items.is_empty() {
                        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                        out_lines.push(format!("{}export {{ {} }};", indent, kept_items.join(", ")));
                    }
                    for j in i..=end_idx {
                        update_depth(lines[j], &mut brace_depth, &mut in_block_comment, &mut in_tmpl_text, &mut tmpl_expr_stack);
                    }
                    i = end_idx + 1;
                    continue;
                }
            }

            // Declaration export: `export [async] function <Name>` / `export const|let|var|class|enum <Name>`
            let after_export = trimmed.strip_prefix("export ").unwrap_or("").trim_start();
            let decl_kw_and_ident: Option<(&str, String)> = [
                "async function ",
                "function ",
                "const ",
                "let ",
                "var ",
                "class ",
                "enum ",
            ]
            .iter()
            .find_map(|kw| {
                after_export.strip_prefix(kw).map(|rest| {
                    let id = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<String>();
                    (*kw, id)
                })
            });

            if let Some((decl_kw, ident)) = decl_kw_and_ident {
                if !ident.is_empty() && !is_reserved_nextjs_route_export(&ident) {
                    let can_be_component = matches!(decl_kw, "async function " | "function " | "const " | "let ");
                    if can_be_component && is_pascal_case_component(&ident) {
                        let is_route_primary = is_route_primary_component(&ident);
                        let prev_primary = export_lost_component.as_deref().map_or(false, |p| is_route_primary_component(p));
                        if export_lost_component.is_none() || (is_route_primary && !prev_primary) {
                            export_lost_component = Some(ident);
                        }
                    }
                    // Strip only the `export ` keyword, keeping indentation & declaration intact
                    let stripped_line = if let Some(pos) = line.find("export ") {
                        format!("{}{}", &line[..pos], &line[pos + 7..])
                    } else {
                        line.to_string()
                    };
                    out_lines.push(stripped_line.clone());
                    update_depth(&stripped_line, &mut brace_depth, &mut in_block_comment, &mut in_tmpl_text, &mut tmpl_expr_stack);
                    i += 1;
                    continue;
                }
            }
        }

        out_lines.push(line.to_string());
        update_depth(line, &mut brace_depth, &mut in_block_comment, &mut in_tmpl_text, &mut tmpl_expr_stack);
        i += 1;
    }

    let mut result = out_lines.join("\n");
    if content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    if !has_default_export {
        if let Some(comp) = export_lost_component.or(fallback_component_candidate) {
            if !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(&format!("\nexport default {};\n", comp));
        }
    }
    result
}

pub fn post_process_generated_file(
    file_path: &str,
    content: &str,
    metamanifest: &Value,
    has_tailwind: bool,
) -> String {
    let normalized_path = file_path.replace('\\', "/");
    let path_lower = normalized_path.to_lowercase();

    let mut raw_content = content.to_string();

    // If a non-JSON code file received a raw JSON envelope by mistake, extract the code payload
    if !path_lower.ends_with(".json") && raw_content.trim().starts_with('{') && (raw_content.contains("\"fixes\"") || raw_content.contains("\"edits\"") || raw_content.contains("\"code\"") || raw_content.contains("\"explanation\"")) {
        if let Ok(parsed_json) = serde_json::from_str::<Value>(&raw_content) {
            let extracted = extract_edits_from_json(&parsed_json, &normalized_path);
            if let Some(first_edit) = extracted.first() {
                if let Some(repl) = first_edit.get("replacement").and_then(|r| r.as_str()) {
                    if !repl.trim().is_empty() {
                        raw_content = repl.to_string();
                    }
                }
            }
        }
    }

    if raw_content.trim().is_empty() || raw_content.trim() == "{}" {
        if let Some(templates) = metamanifest.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
            for template in templates {
                if let Some(t_path) = template.get("path").and_then(|p| p.as_str()) {
                    if t_path.to_lowercase() == path_lower {
                        if let Some(default_content) = template.get("defaultContent").and_then(|c| c.as_str()) {
                            return default_content.to_string();
                        }
                    }
                }
            }
        }
    }

    let content = raw_content.as_str();

    if path_lower.ends_with("tailwind.config.js") {
        if !content.contains("./src/") && !content.contains("src/") {
            let mut processed_tw = content.to_string();
            if processed_tw.contains("content: [") {
                processed_tw = processed_tw.replace(
                    "content: [",
                    "content: [\n    \"./src/**/*.{js,ts,jsx,tsx}\",\n    \"./app/**/*.{js,ts,jsx,tsx}\","
                );
                return processed_tw;
            }
        }
    }

    if path_lower.ends_with("postcss.config.js") {
        return "module.exports = {\n  plugins: {\n    tailwindcss: {},\n    autoprefixer: {},\n  },\n}".to_string();
    }

    if path_lower.ends_with("tsconfig.json") {
        let mut ts_val = match serde_json::from_str::<Value>(content) {
            Ok(val) => val,
            Err(_) => serde_json::json!({
                "compilerOptions": {},
                "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
                "exclude": ["node_modules"]
            }),
        };

        if let Some(ts_obj) = ts_val.as_object_mut() {
            let compiler_options = ts_obj.entry("compilerOptions").or_insert(serde_json::json!({}));
            if let Some(co_obj) = compiler_options.as_object_mut() {
                co_obj.insert("baseUrl".to_string(), serde_json::json!("."));
                co_obj.insert("esModuleInterop".to_string(), serde_json::json!(true));
                co_obj.insert("resolveJsonModule".to_string(), serde_json::json!(true));
                co_obj.insert("skipLibCheck".to_string(), serde_json::json!(true));
                co_obj.insert("jsx".to_string(), serde_json::json!("preserve"));
                co_obj.insert("moduleResolution".to_string(), serde_json::json!("bundler"));
                co_obj.insert("module".to_string(), serde_json::json!("esnext"));
                
                let paths = co_obj.entry("paths").or_insert(serde_json::json!({}));
                if let Some(paths_obj) = paths.as_object_mut() {
                    paths_obj.insert("@/*".to_string(), serde_json::json!(["./src/*"]));
                }

                if let Some(types) = co_obj.get_mut("types") {
                    if let Some(types_arr) = types.as_array_mut() {
                        types_arr.retain(|val| val.as_str() != Some("next-env"));
                    }
                }
            }
        }
        if let Ok(pretty) = serde_json::to_string_pretty(&ts_val) {
            return pretty;
        }
    }

    if path_lower.ends_with("package.json") {
        let mut merge_rule = None;
        if let Some(templates) = metamanifest.get("requiredFilesTemplates").and_then(|t| t.as_array()) {
            for template in templates {
                if let Some(t_path) = template.get("path").and_then(|p| p.as_str()) {
                    if t_path.to_lowercase() == path_lower {
                        if let Some(merge) = template.get("mergeDependencies") {
                            merge_rule = Some(merge);
                            break;
                        }
                    }
                }
            }
        }

        let mut pkg_val: Value = serde_json::from_str(content).unwrap_or_else(|_| {
            serde_json::json!({
                "name": "generated-package",
                "version": "1.0.0",
                "private": true,
                "dependencies": {},
                "devDependencies": {}
            })
        });

        if let Some(pkg_obj) = pkg_val.as_object_mut() {
            if let Some(rule) = merge_rule {
                if let Some(rule_deps) = rule.get("dependencies").and_then(|d| d.as_object()) {
                    let dependencies = pkg_obj.entry("dependencies").or_insert(serde_json::json!({}));
                    if let Some(deps_obj) = dependencies.as_object_mut() {
                        for (k, v) in rule_deps {
                            deps_obj.insert(k.clone(), v.clone());
                        }
                    }
                }

                if let Some(rule_dev_deps) = rule.get("devDependencies").and_then(|d| d.as_object()) {
                    let dev_dependencies = pkg_obj.entry("devDependencies").or_insert(serde_json::json!({}));
                    if let Some(dev_deps_obj) = dev_dependencies.as_object_mut() {
                        for (k, v) in rule_dev_deps {
                            dev_deps_obj.insert(k.clone(), v.clone());
                        }
                    }
                }

                if let Some(rule_scripts) = rule.get("scripts").and_then(|s| s.as_object()) {
                    let scripts = pkg_obj.entry("scripts").or_insert(serde_json::json!({}));
                    if let Some(scripts_obj) = scripts.as_object_mut() {
                        for (k, v) in rule_scripts {
                            scripts_obj.insert(k.clone(), v.clone());
                        }
                    }
                }
            }

            // Next.js specific cleanup (remove SPA routing)
            let is_nextjs = pkg_obj.get("dependencies")
                .and_then(|d| d.as_object())
                .map_or(false, |deps| deps.contains_key("next"));

            if let Some(deps) = pkg_obj.get_mut("dependencies").and_then(|d| d.as_object_mut()) {
                if deps.contains_key("jwt") {
                    let version = deps.remove("jwt").unwrap_or_else(|| serde_json::json!("^9.0.2"));
                    deps.insert("jsonwebtoken".to_string(), version);
                }
                deps.remove("node-base64");
                deps.remove("base64");
                if is_nextjs {
                    deps.remove("react-router-dom");
                    deps.remove("react-router");
                }
            }

            if let Some(dev_deps) = pkg_obj.get_mut("devDependencies").and_then(|d| d.as_object_mut()) {
                if dev_deps.contains_key("jwt") {
                    let version = dev_deps.remove("jwt").unwrap_or_else(|| serde_json::json!("^9.0.2"));
                    dev_deps.insert("jsonwebtoken".to_string(), version);
                }
                dev_deps.remove("node-base64");
                dev_deps.remove("base64");
            }
        }

        let _ = sanitize_package_json_value_for_offline(&mut pkg_val);
        if let Ok(pretty) = serde_json::to_string_pretty(&pkg_val) {
            return pretty;
        }
    }

    if path_lower.ends_with(".css") || path_lower.ends_with(".scss") {
        let mut is_target_css = false;
        if has_tailwind {
            if let Some(target_files) = metamanifest.get("injectTailwindDirectives").and_then(|i| i.get("targetCssFiles")).and_then(|f| f.as_array()) {
                is_target_css = target_files.iter().any(|val| {
                    val.as_str().map(|s| path_lower.contains(&s.to_lowercase())).unwrap_or(false)
                });
            } else {
                is_target_css = path_lower.contains("globals.css") || path_lower.contains("styles.css");
            }
        }

        let cleaned_css = deduplicate_repeated_css_blocks(content);
        if is_target_css && !cleaned_css.contains("@tailwind base") {
            return format!("@tailwind base;\n@tailwind components;\n@tailwind utilities;\n\n{}", cleaned_css);
        }
        return cleaned_css;
    }

    let pre_sanitized_content = if (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx"))
        && (content.contains("useSession")
            || content.contains("!session")
            || content.contains("!user")
            || content.contains("status ==="))
    {
        sanitize_unhandled_session_loading(content)
    } else {
        content.to_string()
    };

    let mut processed = if is_html_template_file(&normalized_path) {
        inject_axiom_attrs(&pre_sanitized_content, &normalized_path)
    } else {
        pre_sanitized_content
    };

    // Inject Axiom Inspect script into layout files for visual editing mode in development
    if path_lower.ends_with("layout.tsx") || path_lower.ends_with("layout.jsx") || path_lower.ends_with("_app.tsx") || path_lower.ends_with("_app.jsx") {
        if !processed.contains("axiom-inspect-click") {
            let inspector_script = get_axiom_inspector_script();

            if processed.contains("</body>") {
                processed = processed.replace("</body>", &format!("{}\n      </body>", inspector_script));
            } else if processed.contains("</html>") {
                processed = processed.replace("</html>", &format!("{}\n</html>", inspector_script));
            }
        }
    }

    // Fix Next.js 13/14 invalid <Link> with <a> child
    if (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx")) && processed.contains("<Link") && (processed.contains("<a ") || processed.contains("<a>")) {
        if !processed.contains("legacyBehavior") {
            processed = processed.replace("<Link ", "<Link legacyBehavior ");
        }
    }

    // Apply ONLY verified safe atomic rules from the Local Error & Solution Katastar
    for (search, replace) in crate::katastar::get_verified_sanitizer_patches(file_path) {
        if processed.contains(&search) {
            processed = processed.replace(&search, &replace);
        }
    }

    // Rule for next.config.js / next.config.mjs resilient web fallbacks
    if path_lower.contains("next.config") {
        if processed.contains("require.resolve(") && !processed.contains("try { return require.resolve") {
            if let Ok(resolve_re) = regex::Regex::new(r#"require\.resolve\s*\(\s*['"]([^'"]+)['"]\s*\)"#) {
                processed = resolve_re.replace_all(&processed, "(() => { try { return require.resolve('$1'); } catch (_) { return false; } })()").to_string();
            }
        }
    }

    // Rule for schema.prisma corrections
    if path_lower.ends_with("schema.prisma") {
        // Normalize Windows line endings first so regexes and line-by-line passes behave consistently
        if processed.contains('\r') {
            processed = processed.replace("\r\n", "\n");
        }

        // 0. Ensure generator client and datasource db blocks are always present (and have valid url)
        processed = ensure_prisma_generator_and_datasource(&processed);

        // 0a. Ensure provider = "postgresql" if enums, Json or postgresql url are present
        if processed.contains("enum ") || processed.contains("postgresql://") {
            if processed.contains("provider = \"sqlite\"") {
                processed = processed.replace("provider = \"sqlite\"", "provider = \"postgresql\"");
            }
            if processed.contains("url = \"file:./dev.db\"") || processed.contains("url      = \"file:./dev.db\"") {
                processed = processed.replace("url = \"file:./dev.db\"", "url      = env(\"DATABASE_URL\")")
                    .replace("url      = \"file:./dev.db\"", "url      = env(\"DATABASE_URL\")");
            }
        }

        // 0b. Normalize SQL-style / invalid Prisma referential actions (e.g., SET_NULL -> SetNull, CASCADE -> Cascade)
        processed = normalize_prisma_referential_actions(&processed);

        // 0c. Strip EnumName. prefix in @default(EnumName.MEMBER) and ensure used enum defaults exist in declared enums
        processed = sanitize_prisma_enum_defaults(&processed);

        // 1. Ensure `model User` exists if NextAuth Account/Session models are declared without it
        let has_nextauth_adapter_models = (processed.contains("model Account") && processed.contains("providerAccountId"))
            || (processed.contains("model Session") && processed.contains("sessionToken"));
        if has_nextauth_adapter_models {
            processed = ensure_prisma_user_model_if_missing(&processed);
        }

        // 2. Fix VerificationToken definition
        if processed.contains("model VerificationToken") {
            if let Some(start_idx) = processed.find("model VerificationToken") {
                if let Some(open_brace) = processed[start_idx..].find('{') {
                    let absolute_open = start_idx + open_brace;
                    if let Some(close_brace) = processed[absolute_open..].find('}') {
                        let absolute_close = absolute_open + close_brace;
                        let correct_model = "model VerificationToken {\n  identifier String\n  token      String   @unique\n  expires    DateTime\n\n  @@unique([identifier, token])\n}".to_string();
                        processed.replace_range(start_idx..=absolute_close, &correct_model);
                    }
                }
            }
        }

        // 3. Fix references like [User.id] -> [id]
        let mut i = 0;
        while let Some(idx) = processed[i..].find("references: [") {
            let absolute_idx = i + idx;
            let start_bracket = absolute_idx + 13;
            if let Some(end_bracket) = processed[start_bracket..].find(']') {
                let content_inside = processed[start_bracket..start_bracket + end_bracket].to_string();
                if content_inside.contains('.') {
                    let parts: Vec<&str> = content_inside.split('.').collect();
                    if parts.len() > 1 {
                        let correct_field = parts[1];
                        processed.replace_range(start_bracket..start_bracket + end_bracket, correct_field);
                    }
                }
            }
            i = absolute_idx + 1;
        }

        // 4a. Strip invalid fields/references from array fields while PRESERVING relation names:
        // e.g. Team[] @relation(fields: [x], references: [y]) -> Team[]
        // e.g. Team[] @relation("TeamMember", fields: [x], references: [y]) -> Team[] @relation("TeamMember")
        // e.g. Team[] @relation("TeamMember") -> PRESERVED!
        let mut lines_arr: Vec<String> = processed.lines().map(|s| s.to_string()).collect();
        for line in lines_arr.iter_mut() {
            let trimmed = line.trim();
            if trimmed.contains("[]") && trimmed.contains("@relation") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 && parts[1].contains("[]") {
                    if let Some(rel_idx) = line.find("@relation") {
                        let after_rel = &line[rel_idx..];
                        if let Some(open_p) = after_rel.find('(') {
                            if let Some(close_p) = find_matching_paren_in_attr(after_rel, open_p) {
                                let inner = &after_rel[open_p + 1..close_p];
                                if inner.contains("fields:") || inner.contains("references:") {
                                    let rel_name = extract_prisma_relation_name(line);
                                    match rel_name {
                                        Some(name) => {
                                            *line = format!("{}@relation(\"{}\"){}", &line[..rel_idx], name, &after_rel[close_p + 1..]);
                                        }
                                        None => {
                                            *line = format!("{}{}", &line[..rel_idx].trim_end(), &after_rel[close_p + 1..]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        processed = lines_arr.join("\n");

        // 4b. Strip invalid @default(...) from relation fields pointing to another model (e.g. role Role @default(...))
        let model_names: Vec<String> = regex::Regex::new(r"(?m)^model\s+(\w+)\s*\{")
            .ok()
            .map(|re| re.captures_iter(&processed).filter_map(|c| c.get(1).map(|m| m.as_str().to_string())).collect())
            .unwrap_or_default();
        let enum_names: Vec<String> = regex::Regex::new(r"(?m)^enum\s+(\w+)\s*\{")
            .ok()
            .map(|re| re.captures_iter(&processed).filter_map(|c| c.get(1).map(|m| m.as_str().to_string())).collect())
            .unwrap_or_default();

        let mut lines_san: Vec<String> = processed.lines().map(|s| s.to_string()).collect();
        for line in lines_san.iter_mut() {
            let trimmed = line.trim();
            if trimmed.contains("@default") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let clean_type = parts[1].replace('?', "").replace('[', "").replace(']', "");
                    if model_names.contains(&clean_type) && !enum_names.contains(&clean_type) {
                        // This is a relation to another model; it cannot have @default
                        if clean_type == "Role" && trimmed.starts_with("role") {
                            // Standardize common role pattern to String @default("USER")
                            *line = "  role             String   @default(\"USER\")".to_string();
                        } else if let Some(def_idx) = line.find("@default") {
                            if let Some(rel_open) = line[def_idx..].find('(') {
                                let abs_open = def_idx + rel_open;
                                if let Some(abs_close) = find_matching_paren_in_attr(line, abs_open) {
                                    let end_idx = abs_close + 1;
                                    let mut new_l = line[..def_idx].to_string();
                                    new_l.push_str(line[end_idx..].trim_start());
                                    *line = new_l;
                                }
                            }
                        }
                    }
                }
            }
        }
        processed = lines_san.join("\n");

        // 4c. Strip @relation from scalar fields
        let mut lines: Vec<String> = processed.lines().map(|s| s.to_string()).collect();
        let primitives = ["String", "Boolean", "Int", "BigInt", "Float", "Decimal", "DateTime", "Json", "Bytes"];
        for line in lines.iter_mut() {
            let trimmed = line.trim();
            if trimmed.contains("@relation") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let clean_type = parts[1].replace('?', "").replace('[', "").replace(']', "");
                    if primitives.contains(&clean_type.as_str()) {
                        if let Some(rel_idx) = line.find("@relation") {
                            let end_idx = line[rel_idx..]
                                .find('(')
                                .and_then(|rel_open| find_matching_paren_in_attr(line, rel_idx + rel_open))
                                .map(|abs_close| abs_close + 1)
                                .unwrap_or(rel_idx);
                            if end_idx > rel_idx {
                                let mut new_line = line[..rel_idx].to_string();
                                new_line.push_str(&line[end_idx..]);
                                *line = new_line;
                            } else {
                                *line = line[..rel_idx].to_string();
                            }
                        }
                    }
                }
            }
        }
        processed = lines.join("\n");

        // 5. Enforce the canonical NextAuth Prisma Adapter models (Account / Session)
        processed = apply_nextauth_prisma_template(&processed);

        // 5b. Deterministically inject any missing foreign key scalar fields referenced in @relation(fields: [...])
        processed = ensure_prisma_foreign_key_scalars(&processed);

        // 5c. Disambiguate multiple relations between the same models (assign explicit @relation("...") names)
        processed = disambiguate_prisma_relations(&processed);

        // 6. Clean ghost/duplicate back-relations and attach existing single back-relations before synthesizing missing ones
        processed = sanitize_prisma_relations(&processed);

        // 6b. Ensure all @relation definitions have their opposite relation on the target model
        processed = ensure_prisma_opposite_relations(&processed);

        // 7. Enforce Prisma relational integrity (1-to-1 @unique, prune ghost duplicate/self relations)
        processed = sanitize_prisma_relations(&processed);

        // 7b. Disambiguate relations again to guarantee no un-named or ambiguous collisions remain
        processed = disambiguate_prisma_relations(&processed);
    }

    let is_js_ts_file = path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx") || path_lower.ends_with(".ts") || path_lower.ends_with(".js");
    if is_js_ts_file {
        // Strip trailing AI edit JSON artifacts (e.g. `const _unused = { "edit": ... }`)
        if let Some(pos) = processed.rfind("const _unused =") {
            let after = &processed[pos..];
            if after.contains("\"edit\"") || after.contains("\"target\"") {
                processed.truncate(pos);
                processed = processed.trim_end().to_string();
            }
        }

        // Normalize bare `src/...`, `~/...` and relative `../lib/prisma` imports globally BEFORE client detection & server-import stripping
        processed = normalize_bare_local_imports(&processed);
        processed = ensure_next_server_imports(&processed, &path_lower);

        if path_lower.ends_with(".ts") || path_lower.ends_with(".tsx") {
            processed = sanitize_ts_interfaces(&processed);
        }
        // Ensure prisma singleton config exports default if it has a named export
        if path_lower.ends_with("lib/prisma.ts") || path_lower.ends_with("lib/prisma.js") {
            if processed.contains("export const prisma") && !processed.contains("export default") {
                processed.push_str("\nexport default prisma;\n");
            }
        }
        let is_app_route = is_app_router_route_file(&normalized_path);
        let is_nextjs_app_file = normalized_path.contains("/app/") || normalized_path.contains("app/")
            || normalized_path.contains("/components/") || normalized_path.contains("components/");
        let is_client_logic_file = !is_app_route && (normalized_path.contains("/hooks/") || normalized_path.starts_with("hooks/")
            || normalized_path.contains("/context/") || normalized_path.starts_with("context/")
            || normalized_path.contains("/contexts/") || normalized_path.starts_with("contexts/")
            || normalized_path.contains("/providers/") || normalized_path.starts_with("providers/"));
        let is_api_or_route = path_lower.contains("/api/")
            || path_lower.ends_with("route.ts")
            || path_lower.ends_with("route.js");

        if (is_nextjs_app_file || is_client_logic_file) && !is_api_or_route {
            if !is_app_route
                && !path_lower.contains("/components/")
                && !path_lower.starts_with("components/")
                && (path_lower.contains("/hooks/") || path_lower.starts_with("hooks/") || path_lower.contains("useauth") || path_lower.ends_with("hook.ts") || path_lower.ends_with("hook.tsx"))
            {
                processed = sanitize_custom_hook_functions(&processed);
            }

            let mut has_custom_hooks = false;
            let bytes = processed.as_bytes();
            for i in 0..bytes.len() {
                if i + 3 < bytes.len() && &bytes[i..i+3] == b"use" {
                    let next_char = bytes[i+3] as char;
                    if next_char.is_ascii_uppercase() {
                        has_custom_hooks = true;
                        break;
                    }
                }
            }

            let has_client_hooks = processed.contains("useState") 
                || processed.contains("useEffect") 
                || processed.contains("useContext") 
                || processed.contains("useRef") 
                || processed.contains("useReducer") 
                || processed.contains("useMemo") 
                || processed.contains("useCallback")
                || processed.contains("useForm")
                || processed.contains("Controller")
                || processed.contains("useFormContext")
                || processed.contains("useFieldArray")
                || has_custom_hooks;
                
            let has_mui_styles = processed.contains("styled(") 
                || processed.contains("createTheme") 
                || processed.contains("@mui/material/styles") 
                || processed.contains("@mui/system") 
                || processed.contains("@mui/styles") 
                || processed.contains("@emotion/styled") 
                || processed.contains("styled-components") 
                || processed.contains("ThemeProvider")
                || processed.contains("useSession")
                || processed.contains("useRouter")
                || processed.contains("usePathname")
                || processed.contains("useParams")
                || processed.contains("useSearchParams");
                
            if (has_client_hooks || has_mui_styles) && !processed.contains("\"use client\"") && !processed.contains("'use client'") {
                processed = format!("\"use client\";\n\n{}", processed);
            }

            processed = ensure_react_and_next_hook_imports(&processed);
            processed = hoist_use_client_directive(&processed);
            processed = sanitize_use_effect_deps(&processed);
            if is_nextjs_app_file && (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx")) {
                processed = ensure_undeclared_toggle_states(&processed);
            }
        }

        if path_lower.ends_with(".ts") || path_lower.ends_with(".tsx") || path_lower.ends_with(".js") || path_lower.ends_with(".jsx") {
            processed = ensure_zod_imports(&processed);
        }

        // Convert old Material-UI (v4) to modern MUI (v5)
        if processed.contains("@material-ui/core") || processed.contains("@material-ui/icons") || processed.contains("createMuiTheme") {
            processed = processed
                .replace("@material-ui/core/styles", "@mui/material/styles")
                .replace("@material-ui/core", "@mui/material")
                .replace("@material-ui/icons", "@mui/icons-material")
                .replace("@material-ui/styles", "@mui/styles")
                .replace("createMuiTheme", "createTheme");
        }

        // Convert React Router to Next.js App Router equivalents for Next.js projects
        if processed.contains("react-router-dom") {
            processed = processed
                .replace("import { Link } from 'react-router-dom'", "import Link from 'next/link'")
                .replace("import { Link } from \"react-router-dom\"", "import Link from \"next/link\"")
                .replace("import { useNavigate } from 'react-router-dom'", "import { useRouter } from 'next/navigation'")
                .replace("import { useNavigate } from \"react-router-dom\"", "import { useRouter } from \"next/navigation\"")
                .replace("import { useParams } from 'react-router-dom'", "import { useParams } from 'next/navigation'")
                .replace("import { useParams } from \"react-router-dom\"", "import { useParams } from \"next/navigation\"")
                .replace("import { useLocation } from 'react-router-dom'", "import { usePathname } from 'next/navigation'")
                .replace("import { useLocation } from \"react-router-dom\"", "import { usePathname } from \"next/navigation\"")
                .replace("from 'react-router-dom'", "from 'next/link'")
                .replace("from \"react-router-dom\"", "from \"next/link\"")
                .replace("<Link to=", "<Link href=");

            if processed.contains("useNavigate") {
                processed = processed
                    .replace("useNavigate()", "useRouter()")
                    .replace("const navigate =", "const router =")
                    .replace("navigate(", "router.push(");
            }
            if processed.contains("useLocation") {
                processed = processed
                    .replace("useLocation()", "usePathname()")
                    .replace("const location =", "const pathname =")
                    .replace("location.pathname", "pathname")
                    .replace("location", "pathname");
            }
        }

        // Root layout HTML & Body injection + TypeScript children fix for Next.js App Router
        let is_root_layout = path_lower.ends_with("app/layout.tsx") || path_lower.ends_with("app/layout.jsx");
        if is_root_layout {
            // Fix TypeScript children typings
            let has_fc = processed.contains("React.FC") || processed.contains("FC");
            if has_fc {
                let mut result = String::new();
                let chars: Vec<char> = processed.chars().collect();
                let mut i = 0;
                while i < chars.len() {
                    // Check for React.FC
                    if i + 8 <= chars.len() && chars[i..i+8] == ['R', 'e', 'a', 'c', 't', '.', 'F', 'C'] {
                        let mut j = i + 8;
                        let mut is_followed_by_angle_bracket = false;
                        while j < chars.len() {
                            if chars[j].is_whitespace() {
                                j += 1;
                            } else if chars[j] == '<' {
                                is_followed_by_angle_bracket = true;
                                break;
                            } else {
                                break;
                            }
                        }
                        if !is_followed_by_angle_bracket {
                            result.push_str("React.FC<{ children: React.ReactNode }>");
                            i += 8;
                            continue;
                        }
                    }
                    // Check for FC (must be standing alone as a word, e.g. not in "someFC" or "FCBuilder")
                    else if i + 2 <= chars.len() && chars[i..i+2] == ['F', 'C'] {
                        // Check prefix boundary
                        let prefix_ok = i == 0 || !chars[i-1].is_alphanumeric() && chars[i-1] != '_';
                        // Check suffix boundary
                        let suffix_ok = i + 2 == chars.len() || !chars[i+2].is_alphanumeric() && chars[i+2] != '_';
                        if prefix_ok && suffix_ok {
                            let mut j = i + 2;
                            let mut is_followed_by_angle_bracket = false;
                            while j < chars.len() {
                                if chars[j].is_whitespace() {
                                    j += 1;
                                } else if chars[j] == '<' {
                                    is_followed_by_angle_bracket = true;
                                    break;
                                } else {
                                    break;
                                }
                            }
                            if !is_followed_by_angle_bracket {
                                result.push_str("FC<{ children: React.ReactNode }>");
                                i += 2;
                                continue;
                            }
                        }
                    }
                    result.push(chars[i]);
                    i += 1;
                }
                processed = result;
            }

            // Ensure globals.css is imported in the root layout to load Tailwind styles
            if !processed.contains("globals.css") {
                let trimmed = processed.trim_start();
                if trimmed.starts_with("\"use client\"") {
                    if let Some(idx) = processed.find("\"use client\"") {
                        if let Some(semicolon_idx) = processed[idx..].find(';') {
                            let insert_pos = idx + semicolon_idx + 1;
                            processed.insert_str(insert_pos, "\nimport './globals.css';");
                        } else {
                            let insert_pos = idx + "\"use client\"".len();
                            processed.insert_str(insert_pos, ";\nimport './globals.css';");
                        }
                    }
                } else if trimmed.starts_with("'use client'") {
                    if let Some(idx) = processed.find("'use client'") {
                        if let Some(semicolon_idx) = processed[idx..].find(';') {
                            let insert_pos = idx + semicolon_idx + 1;
                            processed.insert_str(insert_pos, "\nimport './globals.css';");
                        } else {
                            let insert_pos = idx + "'use client'".len();
                            processed.insert_str(insert_pos, ";\nimport './globals.css';");
                        }
                    }
                } else {
                    processed = format!("import './globals.css';\n{}", processed);
                }
            }

            if processed.contains("({ children })") && !processed.contains("React.FC<") && !processed.contains("FC<") {
                processed = processed.replace("({ children })", "({ children }: { children: React.ReactNode })");
            }
            
            // Inject <html> and <body> if missing
            if !processed.contains("<html") && !processed.contains("<body") {
                if let Some(return_idx) = processed.find("return (") {
                    let insert_pos = return_idx + "return (".len();
                    processed.insert_str(insert_pos, "\n    <html lang=\"en\">\n      <body>");
                    
                    if let Some(last_close) = processed.rfind(");") {
                        processed.insert_str(last_close, "      </body>\n    </html>\n");
                    }
                }
            }
            
            // Neutralize direct `useSession()` calls inside RootLayout itself (outside SessionProvider)
            let had_use_session = processed.contains("useSession");
            if processed.contains("useSession(") {
                processed = sanitize_root_layout_direct_use_session(&processed);
            }

            // Auto-wrap root layout in SessionProvider if it uses useSession directly or has next-auth dependency
            let has_nextauth_dep = metamanifest.get("requiredFilesTemplates")
                .and_then(|t| t.as_array())
                .map_or(false, |templates| {
                    templates.iter().any(|temp| {
                        temp.get("mergeDependencies")
                            .and_then(|m| m.get("dependencies"))
                            .and_then(|d| d.as_object())
                            .map_or(false, |deps| deps.contains_key("next-auth"))
                    })
                });

            if (had_use_session || processed.contains("useSession") || has_nextauth_dep) && !processed.contains("<SessionProvider") {
                if !processed.contains("SessionProvider") {
                    processed = format!("import {{ SessionProvider }} from 'next-auth/react';\n{}", processed);
                }
                if processed.contains("{children}") {
                    processed = processed.replace("{children}", "<SessionProvider>{children}</SessionProvider>");
                }
                if !processed.trim_start().starts_with("\"use client\"") && !processed.trim_start().starts_with("'use client'") {
                    processed = format!("\"use client\";\n\n{}", processed);
                }
            }

            // Ensure Navbar and Footer are mounted in RootLayout if Navbar component exists in templates
            let project_has_navbar = metamanifest.get("requiredFilesTemplates")
                .and_then(|t| t.as_array())
                .map_or(false, |templates| {
                    templates.iter().any(|temp| {
                        temp.get("path").and_then(|p| p.as_str()).map_or(false, |p| p.contains("Navbar.tsx") || p.contains("Navbar.jsx"))
                    })
                });

            if project_has_navbar && !processed.contains("<Navbar") {
                let mut navbar_import = "";
                let mut footer_import = "";
                if !processed.contains("Navbar from") && !processed.contains("Navbar } from") {
                    navbar_import = "import Navbar from '@/components/Navbar';\n";
                }
                if !processed.contains("Footer from") && !processed.contains("Footer } from") {
                    footer_import = "import Footer from '@/components/Footer';\n";
                }
                
                let imports_to_add = format!("{}{}", navbar_import, footer_import);
                if !imports_to_add.is_empty() {
                    let trimmed = processed.trim_start();
                    if trimmed.starts_with("\"use client\"") {
                        if let Some(idx) = processed.find("\"use client\"") {
                            let semicolon = if processed[idx..].starts_with("\"use client\";") { 1 } else { 0 };
                            let insert_pos = idx + "\"use client\"".len() + semicolon;
                            processed.insert_str(insert_pos, &format!("\n{}", imports_to_add));
                        }
                    } else if trimmed.starts_with("'use client'") {
                        if let Some(idx) = processed.find("'use client'") {
                            let semicolon = if processed[idx..].starts_with("'use client';") { 1 } else { 0 };
                            let insert_pos = idx + "'use client'".len() + semicolon;
                            processed.insert_str(insert_pos, &format!("\n{}", imports_to_add));
                        }
                    } else {
                        processed = format!("{}{}", imports_to_add, processed);
                    }
                }

                if processed.contains("<SessionProvider>") && processed.contains("</SessionProvider>") {
                    processed = processed.replace(
                        "<SessionProvider>",
                        "<SessionProvider>\n        <Navbar />\n        <main className=\"flex-grow flex flex-col w-full\">"
                    );
                    processed = processed.replace(
                        "</SessionProvider>",
                        "</main>\n        <Footer />\n      </SessionProvider>"
                    );
                } else if processed.contains("<body>") && processed.contains("</body>") {
                    processed = processed.replace(
                        "<body>",
                        "<body>\n        <Navbar />\n        <main className=\"flex-grow flex flex-col w-full\">"
                    );
                    processed = processed.replace(
                        "</body>",
                        "</main>\n        <Footer />\n      </body>"
                    );
                }
            }

            // Ensure CartProvider is wrapped if cartContext exists in project
            let project_has_cart = metamanifest.get("requiredFilesTemplates")
                .and_then(|t| t.as_array())
                .map_or(false, |templates| {
                    templates.iter().any(|temp| {
                        temp.get("path").and_then(|p| p.as_str()).map_or(false, |p| p.contains("cartContext") || p.contains("cart.tsx"))
                    })
                }) || processed.contains("CartProvider");

            if project_has_cart && !processed.contains("<CartProvider") {
                if !processed.contains("CartProvider") {
                    let cart_import = "import { CartProvider } from '@/lib/cartContext';\n";
                    let trimmed = processed.trim_start();
                    if trimmed.starts_with("\"use client\"") {
                        if let Some(idx) = processed.find("\"use client\"") {
                            let semicolon = if processed[idx..].starts_with("\"use client\";") { 1 } else { 0 };
                            let insert_pos = idx + "\"use client\"".len() + semicolon;
                            processed.insert_str(insert_pos, &format!("\n{}", cart_import));
                        }
                    } else if trimmed.starts_with("'use client'") {
                        if let Some(idx) = processed.find("'use client'") {
                            let semicolon = if processed[idx..].starts_with("'use client';") { 1 } else { 0 };
                            let insert_pos = idx + "'use client'".len() + semicolon;
                            processed.insert_str(insert_pos, &format!("\n{}", cart_import));
                        }
                    } else {
                        processed = format!("{}{}", cart_import, processed);
                    }
                }
                if processed.contains("<SessionProvider>") {
                    processed = processed.replace("<SessionProvider>", "<SessionProvider>\n        <CartProvider>");
                    processed = processed.replace("</SessionProvider>", "        </CartProvider>\n      </SessionProvider>");
                }
            }

            // Ensure "use client" is at the very top if any client provider is present
            if (processed.contains("<SessionProvider") || processed.contains("<CartProvider") || processed.contains("useSession"))
                && !processed.trim_start().starts_with("\"use client\"") && !processed.trim_start().starts_with("'use client'") {
                processed = format!("\"use client\";\n\n{}", processed);
            }
            processed = hoist_use_client_directive(&processed);
            processed = sanitize_root_layout_missing_children(&processed, &normalized_path, false);
        }

        // NextAuth v3 vs v4 rewrite for route handlers
        if path_lower.contains("src/app/api/auth/[...nextauth]/route.ts") || path_lower.contains("src/app/api/auth/[...nextauth]/route.js") {
            processed = r#"import NextAuth from "next-auth";
import CredentialsProvider from "next-auth/providers/credentials";
import bcrypt from "bcryptjs";
import prisma from "@/lib/prisma";

const authOptions = {
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
          const user = await prisma.user.findUnique({ where: { email: credentials.email } }) as any;
          if (!user) return null;
          const passwordMatch = await bcrypt.compare(credentials.password, user.hashedPassword || user.password || "");
          if (!passwordMatch) return null;
          return { id: String(user.id), email: user.email, name: user.name };
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
      }
      return token;
    },
    async session({ session, token }: any) {
      if (session.user && token) {
        session.user.id = token.id;
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
export { handler as GET, handler as POST };
"#.to_string();
        }

        // React Native web icon leakage safeguard (auto-correct to lucide-react-native)
        if path_lower.contains("react-native") || path_lower.contains("expo") {
            if processed.contains("from 'lucide-react'") {
                processed = processed.replace("from 'lucide-react'", "from 'lucide-react-native'");
            }
            if processed.contains("from \"lucide-react\"") {
                processed = processed.replace("from \"lucide-react\"", "from \"lucide-react-native\"");
            }
        }

        // Next.js Edge Middleware safeguard: prevent Express middleware leakage or missing middleware export
        if path_lower.ends_with("middleware.ts") || path_lower.ends_with("middleware.js") {
            let has_express = processed.contains("'express'") || processed.contains("\"express\"") || processed.contains("NextFunction");
            let has_valid_export = processed.contains("export function middleware")
                || processed.contains("export async function middleware")
                || processed.contains("export default")
                || processed.contains("withAuth");
            if has_express || !has_valid_export {
                processed = r#"import { NextResponse } from 'next/server';
import type { NextRequest } from 'next/server';

export function middleware(request: NextRequest) {
  return NextResponse.next();
}

export const config = {
  matcher: ['/((?!api|_next/static|_next/image|favicon.ico).*)'],
};
"#.to_string();
            }
        }

        // Dual-export synchronization (both named and default export) for components, UI primitives, and hooks
        // Prevents: Attempted import error: 'Button' is not exported from '@/components/ui/button'
        // Strictly NEVER apply to Next.js App Router route files (`page.tsx`, `layout.tsx`, `error.tsx`, etc.).
        if !is_app_router_route_file(&normalized_path)
            && !path_lower.contains("/api/")
            && !path_lower.ends_with("route.ts")
            && !path_lower.ends_with("route.js")
            && !path_lower.ends_with("middleware.ts")
            && !path_lower.ends_with("middleware.js")
        {
            if let Some(stem) = std::path::Path::new(&normalized_path).file_stem().and_then(|s| s.to_str()) {
                if stem != "index" {
                    let mut pascal_name = String::new();
                    let mut cap_next = true;
                    for c in stem.chars() {
                        if c == '-' || c == '_' || c == ' ' {
                            cap_next = true;
                        } else if cap_next {
                            pascal_name.push(c.to_ascii_uppercase());
                            cap_next = false;
                        } else {
                            pascal_name.push(c);
                        }
                    }

                    // 1. If file has a default export of an identifier: "export default <Ident>;" or "export default function <Ident>"
                    let default_ident_re = regex::Regex::new(r#"(?m)^export\s+default\s+(?:function\s+|class\s+)?([A-Za-z0-9_]+)\b"#).ok();
                    let default_ident = default_ident_re.as_ref().and_then(|re| {
                        re.captures(&processed).and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                    });

                    if let Some(ref def_id) = default_ident {
                        if def_id != "function" && def_id != "class" && def_id != "default" {
                            if !has_named_export_symbol(&processed, def_id) {
                                processed.push_str(&format!("\nexport {{ {} }};\n", def_id));
                            }
                        }
                    }

                    // 2. If file has a named export matching pascal_name or stem (e.g. "export const Button", "export const Skeleton"), but NO default export
                    let target_names = vec![pascal_name.clone(), stem.to_string()];
                    for name in &target_names {
                        if !name.is_empty() {
                            let has_named = has_named_export_symbol(&processed, name);
                            let has_default = processed.contains("export default");
                            if has_named && !has_default {
                                processed.push_str(&format!("\nexport default {};\n", name));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if normalized_path.contains("src/app/") || normalized_path.starts_with("app/") {
            if !path_lower.ends_with("route.ts") && !path_lower.ends_with("route.js") {
                let has_hooks = processed.contains("useState") || processed.contains("useEffect") || 
                                processed.contains("useContext") || processed.contains("useRef") || 
                                processed.contains("useReducer") || processed.contains("useSession") ||
                                processed.contains("useRouter") || processed.contains("usePathname") ||
                                processed.contains("useParams") || processed.contains("useSearchParams");
                if has_hooks && !processed.trim_start().starts_with("\"use client\"") && !processed.trim_start().starts_with("'use client'") {
                    processed = format!("\"use client\";\n\n{}", processed);
                }
                processed = hoist_use_client_directive(&processed);
            }

            // Re-check async client component after potential "use client" injection above
            if (processed.contains("\"use client\"") || processed.contains("'use client'")) && processed.contains("export default async function") {
                processed = processed.replace("export default async function", "export default function");
            }

            // NOTE: App Router default export synthesis + illegal named export cleanup
            // is handled by `sanitize_app_router_route_exports(...)` below (AST-depth aware).

            // Prisma Client import normalization
            processed = processed
                .replace("\"../../../../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../../../../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../../../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../../../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../lib/prisma\"", "\"@/lib/prisma\"")
                .replace("'../lib/prisma'", "'@/lib/prisma'")
                .replace("\"../../../prisma\"", "\"@/lib/prisma\"")
                .replace("'../../../prisma'", "'@/lib/prisma'")
                .replace("\"../../prisma\"", "\"@/lib/prisma\"")
                .replace("'../../prisma'", "'@/lib/prisma'")
                .replace("\"../prisma\"", "\"@/lib/prisma\"")
                .replace("'../prisma'", "'@/lib/prisma'")
                .replace("\"@/prisma\"", "\"@/lib/prisma\"")
                .replace("'@/prisma'", "'@/lib/prisma'");

            // React Router Dom to Next Link/Navigation rewrite for Next.js App Router
            if processed.contains("react-router-dom") {
                processed = processed
                    .replace("from 'react-router-dom'", "from 'next/link'")
                    .replace("from \"react-router-dom\"", "from \"next/link\"")
                    .replace("import { Link } from 'next/link'", "import Link from 'next/link'")
                    .replace("import { Link } from \"next/link\"", "import Link from \"next/link\"")
                    .replace("NavLink", "Link");
            }

            // react-icons root import normalization (converts bare 'react-icons' or '*-icons-react' imports into specific subpaths)
            if processed.contains("from 'react-icons'") || processed.contains("from \"react-icons\"") || processed.contains("-icons-react'") || processed.contains("-icons-react\"") {
                processed = sanitize_react_icons_imports(&processed);
            }

            // Clean invalid page level react-router configurations
            if (path_lower.ends_with("page.tsx") || path_lower.ends_with("page.jsx")) && 
               (processed.contains("<BrowserRouter") || processed.contains("<Switch") || processed.contains("<Route")) {
                processed = r#"import React from 'react';
import Link from 'next/link';

export default function Home() {
  return (
    <main className="min-h-screen bg-slate-950 text-white flex flex-col items-center justify-center p-6 font-sans">
      <div className="max-w-3xl text-center space-y-6">
        <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-indigo-500/10 border border-indigo-500/20 text-indigo-400 text-sm font-medium">
          <span className="w-2 h-2 rounded-full bg-indigo-400 animate-pulse"></span>
          Axiom Forge Dev Preview
        </div>
        <h1 className="text-5xl font-extrabold tracking-tight bg-gradient-to-r from-indigo-400 via-purple-400 to-pink-400 bg-clip-text text-transparent pb-2">
          Project Generated Successfully
        </h1>
        <p className="text-lg text-slate-400 max-w-xl mx-auto">
          Your Next.js App Router application has been successfully generated and compiled. 
          Use this preview to explore your project structures and database collections.
        </p>
        <div className="flex flex-wrap justify-center gap-4 pt-4">
          <Link href="/api/auth/signin" className="px-6 py-3 bg-indigo-600 hover:bg-indigo-700 text-white font-medium rounded-lg transition-all shadow-lg shadow-indigo-600/20 hover:scale-[1.02]">
            Sign In / Register
          </Link>
          <a href="https://nextjs.org/docs" target="_blank" rel="noopener noreferrer" className="px-6 py-3 bg-slate-900 hover:bg-slate-800 text-slate-200 font-medium rounded-lg transition-colors border border-slate-800">
            Next.js Documentation
          </a>
        </div>
      </div>
    </main>
  );
}
"#.to_string();
            }

            // Router query normalization
            if processed.contains("router.query") {
                processed = processed.replace("router.query", "useParams()");
                if processed.contains("useRouter") && !processed.contains("useParams") {
                    processed = processed.replace("useRouter", "useRouter, useParams");
                }
            }

            // Router imports normalization
            processed = processed
                .replace("from 'next/router'", "from 'next/navigation'")
                .replace("from \"next/router\"", "from \"next/navigation\"");
                
            // Resend dummy key fallback
            processed = processed
                .replace("new Resend(process.env.RESEND_API_KEY)", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY as string)", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY!)", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY || \"\")", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')")
                .replace("new Resend(process.env.RESEND_API_KEY || '')", "new Resend(process.env.RESEND_API_KEY || 're_dummy_key_for_dev_preview')");
        }

        // Enforce client & custom-hook boundaries AFTER all `"use client"` directives and import normalizations have run
        let is_client_file = processed.contains("\"use client\"") || processed.contains("'use client'");
        let is_hook_file = !is_app_router_route_file(&normalized_path)
            && (path_lower.contains("/hooks/") || path_lower.starts_with("hooks/"))
            && !path_lower.contains("/api/");
        let is_route_handler = path_lower.ends_with("route.ts") || path_lower.ends_with("route.js");

        // Normalize `@/prisma/schema` & `@/prisma/client` imports to `@prisma/client` (as `import type` on client/hook files)
        // or `@/lib/prisma` (when importing the `prisma` runtime singleton), BEFORE server-import boundary stripping runs.
        processed = normalize_prisma_schema_imports(&processed, (is_client_file || is_hook_file) && !is_route_handler);

        if is_client_file && processed.contains("export default async function") {
            processed = processed.replace("export default async function", "export default function");
        }

        if (is_client_file || is_hook_file) && !is_route_handler {
            // Strip server-only and API-route value imports (Prisma, bcrypt, fs, api/**/route)
            // while preserving pure `import type ...` declarations (multi-line safe).
            let raw_lines: Vec<&str> = processed.lines().collect();
            let mut filtered_lines: Vec<String> = Vec::with_capacity(raw_lines.len());
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

                    let spec_opt = extract_import_specifier(flat_trim);
                    let is_forbidden_server_import = if let Some(ref spec) = spec_opt {
                        let s_low = spec.to_lowercase();
                        let is_server_mod = s_low == "@prisma/client"
                            || s_low == "@/lib/prisma"
                            || s_low == "~/lib/prisma"
                            || s_low.ends_with("/lib/prisma")
                            || s_low == "bcrypt"
                            || s_low == "bcryptjs"
                            || s_low == "fs"
                            || s_low == "node:fs"
                            || s_low == "fs/promises";
                        let is_api_route_import = s_low.starts_with("@/app/api/")
                            || s_low.starts_with("~/app/api/")
                            || s_low.starts_with("src/app/api/")
                            || s_low.starts_with("app/api/")
                            || (s_low.contains("/api/")
                                && (s_low.ends_with("/route")
                                    || s_low.ends_with("/route.ts")
                                    || s_low.ends_with("/route.js")));
                        !is_type_only && (is_server_mod || is_api_route_import)
                    } else {
                        false
                    };

                    if is_forbidden_server_import {
                        idx_l = end_l + 1;
                        continue;
                    }
                    for k in idx_l..=end_l {
                        filtered_lines.push(raw_lines[k].to_string());
                    }
                    idx_l = end_l + 1;
                    continue;
                }
                filtered_lines.push(raw_lines[idx_l].to_string());
                idx_l += 1;
            }
            let had_trailing_nl = processed.ends_with('\n');
            processed = filtered_lines.join("\n");
            if had_trailing_nl && !processed.ends_with('\n') {
                processed.push('\n');
            }
        }

        // Strip invalid named value exports on Next.js App Router route files (page/layout/error/loading/etc.)
        if is_app_router_route_file(&normalized_path) {
            processed = sanitize_app_router_route_exports(&processed, &normalized_path);
        }

        // Apply across all TS/TSX/JS/JSX files (hooks, components, pages, API routes):
        if processed.contains("react-icons") || processed.contains("-icons-react") {
            processed = sanitize_react_icons_imports(&processed);
        }
        processed = sanitize_next_link_named_import(&processed);
        processed = sanitize_nextauth_imports(&processed);
        if path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx") {
            let is_ui_mod = path_lower.contains("/components/ui") || path_lower.ends_with("/ui.tsx") || path_lower.ends_with("/ui.jsx");
            processed = repair_bare_react_fc_signatures(&processed, is_ui_mod);
        }

        // Expand barrel hook imports `import { useA, useB } from '@/hooks'` into individual `import { useA } from '@/hooks/useA'`
        if processed.contains("from '@/hooks'") || processed.contains("from \"@/hooks\"") {
            let mut rewritten_lines = Vec::new();
            for line in processed.lines() {
                let t = line.trim();
                let is_hooks_barrel = (t.starts_with("import ") || t.starts_with("import{"))
                    && (t.ends_with("from '@/hooks';")
                        || t.ends_with("from \"@/hooks\";")
                        || t.ends_with("from '@/hooks'")
                        || t.ends_with("from \"@/hooks\""));
                if is_hooks_barrel {
                    if let (Some(ob), Some(cb)) = (line.find('{'), line.find('}')) {
                        let inner = &line[ob + 1..cb];
                        let symbols: Vec<&str> = inner.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                        let all_hooks = !symbols.is_empty() && symbols.iter().all(|s| {
                            let ident = s.split_whitespace().next().unwrap_or(s);
                            ident.starts_with("use") && ident.chars().nth(3).map_or(false, |c| c.is_ascii_uppercase())
                        });
                        if all_hooks {
                            for sym in symbols {
                                let ident = sym.split_whitespace().next().unwrap_or(sym);
                                rewritten_lines.push(format!("import {{ {} }} from '@/hooks/{}';", sym, ident));
                            }
                            continue;
                        }
                    }
                }
                rewritten_lines.push(line.to_string());
            }
            processed = rewritten_lines.join("\n");
        }

        // Guard common array/object property accesses against undefined in generated UI/hooks
        if processed.contains(".skills.join(") {
            processed = processed.replace(".skills.join(", ".skills?.join(");
        }
        if processed.contains(".tags.join(") {
            processed = processed.replace(".tags.join(", ".tags?.join(");
        }
        if processed.contains("${user.id}") {
            processed = processed.replace("${user.id}", "${user?.id || 'preview-user'}");
        }

        // Aesthetic sanitizer for Footer.tsx: replace solid primary button classes on footer <Link> items with clean footer text link styling
        if path_lower.ends_with("footer.tsx") || path_lower.ends_with("footer.jsx") {
            if let Ok(re_footer_btn) = regex::Regex::new(
                r#"(<Link\b[^>]*?\bclassName=["'])([^"']*?\b(?:bg-(?:teal|indigo|blue|emerald|violet|purple|rose|amber|cyan|sky)-600)\b[^"']*?\brounded-(?:xl|lg)\b[^"']*?)(["'])"#
            ) {
                processed = re_footer_btn
                    .replace_all(&processed, "${1}text-sm text-slate-400 hover:text-white transition-colors mr-6 last:mr-0${3}")
                    .to_string();
            }
        }

        // Aesthetic sanitizer for Navbar.tsx: replace hamburger menu icon (<HiMenu / <FiMenu) accidentally placed inside the Brand Logo Mark badge
        if path_lower.ends_with("navbar.tsx") || path_lower.ends_with("navbar.jsx") {
            if let Ok(re_brand_menu) = regex::Regex::new(
                r#"(<div\s+className=["'][^"']*?\bw-9\s+h-9\s+rounded-xl\s+bg-gradient-to-br\b[^"']*?["']\s*>\s*)<(?:HiMenu|FiMenu|HiOutlineMenu)\b[^>]*/?>"#
            ) {
                processed = re_brand_menu
                    .replace_all(
                        &processed,
                        r#"${1}<svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}><path strokeLinecap="round" strokeLinejoin="round" d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>"#,
                    )
                    .to_string();
            }
        }

        // Fix LLM `finally { setIsLoading(true); }` / `finally { setLoading(true); }` forever-loading trap
        if processed.contains("finally") && (processed.contains("setIsLoading(true)") || processed.contains("setLoading(true)")) {
            if let Ok(re) = regex::Regex::new(r"finally\s*\{\s*(set(?:Is)?Loading)\s*\(\s*true\s*\)\s*;?\s*\}") {
                processed = re.replace_all(&processed, "finally {\n        $1(false);\n      }").to_string();
            }
        }

        // Auto-inject `import { z } from 'zod';` when `z.object(` / `z.string(` / `z.enum(` / `z.array(` is used without importing zod
        if (processed.contains("z.object(") || processed.contains("z.string(") || processed.contains("z.enum(") || processed.contains("z.array("))
            && !processed.contains("from 'zod'")
            && !processed.contains("from \"zod\"")
        {
            if processed.trim_start().starts_with("\"use client\"") || processed.trim_start().starts_with("'use client'") {
                if let Some(newline_pos) = processed.find('\n') {
                    let (first, rest) = processed.split_at(newline_pos + 1);
                    processed = format!("{}import {{ z }} from 'zod';\n{}", first, rest);
                }
            } else {
                processed = format!("import {{ z }} from 'zod';\n{}", processed);
            }
        }

        // Auto-inject `import bcrypt from 'bcryptjs';` in server/non-client JS/TS files using `bcrypt.*` without import
        if (!is_client_file && !is_hook_file) || is_route_handler {
            processed = ensure_bcrypt_imports(&processed);
        }

        // Auto-inject `import Link from 'next/link';` when `<Link ` or `<Link>` is used in JSX without importing Link
        if (path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx"))
            && (processed.contains("<Link ") || processed.contains("<Link>"))
            && !processed.contains("import Link")
            && !processed.contains("Link } from")
            && !processed.contains("Link} from")
            && !processed.contains("function Link")
            && !processed.contains("const Link ")
        {
            if processed.trim_start().starts_with("\"use client\"") || processed.trim_start().starts_with("'use client'") {
                if let Some(newline_pos) = processed.find('\n') {
                    let (first, rest) = processed.split_at(newline_pos + 1);
                    processed = format!("{}import Link from 'next/link';\n{}", first, rest);
                }
            } else {
                processed = format!("import Link from 'next/link';\n{}", processed);
            }
        }

        // Safe default parameter for custom hooks with destructured object props:
        // `const useTasks = ({ boardId }: UseTasksProps)` -> `const useTasks = ({ boardId }: UseTasksProps = {} as any)`
        if !is_app_router_route_file(&normalized_path) && (normalized_path.contains("/hooks/") || normalized_path.starts_with("hooks/")) && processed.contains("({") {
            if let Ok(re) = regex::Regex::new(r"(const\s+use\w+\s*=\s*\(\s*\{[^}]+\}\s*:\s*\w+)(\s*\))") {
                processed = re.replace_all(&processed, "$1 = {} as any$2").to_string();
            }
            if let Ok(re) = regex::Regex::new(r"(function\s+use\w+\s*\(\s*\{[^}]+\}\s*:\s*\w+)(\s*\))") {
                processed = re.replace_all(&processed, "$1 = {} as any$2").to_string();
            }
        }
        processed = ensure_hook_type_exports(&processed, &normalized_path);

        processed = sanitize_optional_user_password_access(&processed);
    }

    processed
}


/// Automatically detects `@relation(fields: [fk1, fk2], references: [ref1, ref2])` where
/// the scalar foreign key field `fk1` is not declared in the model, and injects it
/// deterministically (matching the referenced model's `@id` or target field scalar type,
/// defaulting to `String` or `String?` if the relation field is optional).
pub fn ensure_prisma_foreign_key_scalars(schema: &str) -> String {
    let model_re = match regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{([^}]+)\}") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut model_bodies: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for cap in model_re.captures_iter(schema) {
        if let (Some(m_name), Some(m_body)) = (cap.get(1), cap.get(2)) {
            model_bodies.insert(m_name.as_str().to_string(), m_body.as_str().to_string());
        }
    }

    if model_bodies.is_empty() {
        return schema.to_string();
    }

    let fields_re = match regex::Regex::new(r"fields:\s*\[([^\]]+)\]") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };
    let refs_re = regex::Regex::new(r"references:\s*\[([^\]]+)\]").ok();

    let mut result = schema.to_string();

    for (model_name, body) in &model_bodies {
        // Collect all declared field names in this model
        let mut declared_fields: std::collections::HashSet<String> = std::collections::HashSet::new();
        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("@@") {
                continue;
            }
            if let Some(first_token) = trimmed.split_whitespace().next() {
                declared_fields.insert(first_token.to_string());
            }
        }

        let mut new_lines: Vec<String> = Vec::new();
        let mut modified = false;

        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.contains("@relation") && trimmed.contains("fields:") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let raw_target_type = parts[1];
                    let is_optional = raw_target_type.ends_with('?');
                    let clean_target_model = raw_target_type.replace(['?', '[', ']'], "");

                    if let Some(f_cap) = fields_re.captures(trimmed) {
                        let fk_list: Vec<&str> = f_cap[1].split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                        let ref_list: Vec<String> = refs_re
                            .as_ref()
                            .and_then(|r| r.captures(trimmed))
                            .map(|c| c[1].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                            .unwrap_or_default();

                        for (idx, fk_name) in fk_list.iter().enumerate() {
                            if !declared_fields.contains(*fk_name) {
                                let ref_field = ref_list.get(idx).map(|s| s.as_str()).unwrap_or("id");
                                let mut scalar_type = "String".to_string();
                                if let Some(target_body) = model_bodies.get(&clean_target_model) {
                                    for t_line in target_body.lines() {
                                        let t_trim = t_line.trim();
                                        let t_parts: Vec<&str> = t_trim.split_whitespace().collect();
                                        if t_parts.len() >= 2 && t_parts[0] == ref_field {
                                            let candidate = t_parts[1].replace(['?', '[', ']'], "");
                                            if ["String", "Int", "BigInt", "Float", "Decimal"].contains(&candidate.as_str()) {
                                                scalar_type = candidate;
                                            }
                                            break;
                                        }
                                    }
                                }
                                if is_optional {
                                    scalar_type.push('?');
                                }
                                new_lines.push(format!("  {:14} {}", fk_name, scalar_type));
                                declared_fields.insert((*fk_name).to_string());
                                modified = true;
                            }
                        }
                    }
                }
            }
            new_lines.push(line.to_string());
        }

        if modified {
            let pattern = format!(r"(?ms)(^model\s+{}\s*\{{)([\s\S]*?)(\n*\}})", model_name);
            if let Ok(block_re) = regex::Regex::new(&pattern) {
                let updated_body = new_lines.join("\n");
                result = block_re
                    .replace(&result, |caps: &regex::Captures| {
                        format!("{}{}\n{}", &caps[1], updated_body.trim_end(), caps[3].trim_start())
                    })
                    .to_string();
            }
        }
    }

    result
}

/// Extracts the Prisma relation name from `@relation(...)` on a field line.
/// Recognizes both positional (`@relation("MyRel", ...)`) and explicit (`@relation(..., name: "MyRel")`)
/// syntax without mistaking `map: "fk_name"` or trailing comments for a relation name.
fn find_matching_paren_in_attr(s: &str, open_byte_idx: usize) -> Option<usize> {
    let slice = s.get(open_byte_idx..)?;
    if !slice.starts_with('(') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for (rel_idx, ch) in slice.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if (in_double || in_single) && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            continue;
        }
        if in_double || in_single {
            continue;
        }
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(open_byte_idx + rel_idx);
            }
        }
    }
    None
}

fn append_prisma_attr_before_comment(line: &str, attr: &str) -> String {
    if let Some(comment_idx) = find_unquoted_comment_start(line) {
        let before_comment = line[..comment_idx].trim_end();
        let comment_part = &line[comment_idx..];
        format!("{} {} {}", before_comment, attr, comment_part)
    } else {
        format!("{} {}", line.trim_end(), attr)
    }
}

fn find_unquoted_comment_start(line: &str) -> Option<usize> {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        if (in_double || in_single) && b == b'\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if b == b'"' && !in_single {
            in_double = !in_double;
        } else if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if !in_double && !in_single && b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn extract_prisma_relation_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("//") {
        return None;
    }
    let rel_idx = trimmed.find("@relation")?;
    let after_rel = &trimmed[rel_idx + "@relation".len()..];
    let after_ws = after_rel.trim_start();
    if !after_ws.starts_with('(') {
        return None;
    }

    let close_idx = find_matching_paren_in_attr(after_ws, 0)?;
    let inner = &after_ws[1..close_idx];

    if let Ok(named_re) = regex::Regex::new(r#"(?:^|[\s,])name\s*:\s*["']([^"']+)["']"#) {
        if let Some(cap) = named_re.captures(inner) {
            if let Some(m) = cap.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }

    if let Ok(pos_re) = regex::Regex::new(r#"^\s*["']([^"']+)["']"#) {
        if let Some(cap) = pos_re.captures(inner) {
            if let Some(m) = cap.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }

    None
}

/// Normalizes SQL-style / invalid Prisma referential action casing inside `@relation(...)`
/// (e.g. `onDelete: SET_NULL` -> `onDelete: SetNull`, `onDelete: CASCADE` -> `onDelete: Cascade`).
pub fn normalize_prisma_referential_actions(schema: &str) -> String {
    let action_re = match regex::Regex::new(
        r"(?i)\b(onDelete|onUpdate)\s*:\s*(SET[\s_]*NULL|SET[\s_]*DEFAULT|NO[\s_]*ACTION|CASCADE|RESTRICT)\b",
    ) {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut out_lines: Vec<String> = Vec::with_capacity(schema.lines().count() + 1);
    for line in schema.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || !trimmed.contains("@relation") {
            out_lines.push(line.to_string());
            continue;
        }

        if let Some(rel_idx) = line.find("@relation") {
            let after_rel = &line[rel_idx..];
            if let Some(open_p) = after_rel.find('(') {
                let close_rel = find_matching_paren_in_attr(after_rel, open_p).map(|end_idx| end_idx + 1);
                if let Some(end_span) = close_rel {
                    let rel_span = &after_rel[..end_span];
                    let fixed_span = action_re
                        .replace_all(rel_span, |caps: &regex::Captures| {
                            let attr = if caps[1].eq_ignore_ascii_case("onDelete") {
                                "onDelete"
                            } else {
                                "onUpdate"
                            };
                            let compact: String = caps[2]
                                .chars()
                                .filter(|c| c.is_ascii_alphabetic())
                                .collect::<String>()
                                .to_ascii_uppercase();
                            let canonical = match compact.as_str() {
                                "SETNULL" => "SetNull",
                                "SETDEFAULT" => "SetDefault",
                                "NOACTION" => "NoAction",
                                "CASCADE" => "Cascade",
                                "RESTRICT" => "Restrict",
                                _ => &caps[2],
                            };
                            format!("{}: {}", attr, canonical)
                        })
                        .to_string();
                    out_lines.push(format!("{}{}{}", &line[..rel_idx], fixed_span, &after_rel[end_span..]));
                    continue;
                }
            }
        }
        out_lines.push(line.to_string());
    }

    let mut result = out_lines.join("\n");
    if schema.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn is_single_field_unique(line: &str, fk_name: &str) -> bool {
    let lt = line.trim();
    if lt.is_empty() || lt.starts_with("//") {
        return false;
    }
    if lt.split_whitespace().next() == Some(fk_name) && lt.contains("@unique") {
        return true;
    }
    if lt.starts_with("@@unique") {
        if let (Some(lb), Some(rb)) = (lt.find('['), lt.find(']')) {
            if rb > lb {
                let inner = lt[lb + 1..rb].trim();
                return inner == fk_name;
            }
        }
    }
    false
}

/// Automatically identifies any @relation(fields: [...], references: [...]) that lacks
/// an opposite relation field on the referenced model and deterministically injects it.
/// Preserves explicit relation names (e.g. name: "TeamMember").
pub fn ensure_prisma_opposite_relations(schema: &str) -> String {
    let model_re = match regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{([^}]+)\}") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut model_names: Vec<String> = Vec::new();
    let mut model_bodies: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for cap in model_re.captures_iter(schema) {
        if let (Some(m_name), Some(m_body)) = (cap.get(1), cap.get(2)) {
            let name = m_name.as_str().to_string();
            let body = m_body.as_str().to_string();
            model_names.push(name.clone());
            model_bodies.insert(name, body);
        }
    }

    if model_names.is_empty() {
        return schema.to_string();
    }

    let mut defining_counts: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    for source_model in &model_names {
        if let Some(body) = model_bodies.get(source_model) {
            for line in body.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || !trimmed.contains("@relation") || !trimmed.contains("fields:") || !trimmed.contains("references:") {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let clean_target = parts[1].replace(['?', '[', ']'], "");
                    if model_names.contains(&clean_target) && clean_target != *source_model {
                        *defining_counts.entry((source_model.clone(), clean_target)).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    let mut pending_inserts: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    // (target_model -> Vec<(field_name, rel_name)>) when upgrading an existing un-named opposite in place
    let mut pending_name_attachments: std::collections::HashMap<String, Vec<(String, String)>> = std::collections::HashMap::new();
    for m in &model_names {
        pending_inserts.insert(m.clone(), Vec::new());
        pending_name_attachments.insert(m.clone(), Vec::new());
    }

    for source_model in &model_names {
        let body = match model_bodies.get(source_model) {
            Some(b) => b,
            None => continue,
        };

        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || !trimmed.contains("@relation") || !trimmed.contains("fields:") || !trimmed.contains("references:") {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let clean_target = parts[1].replace(['?', '[', ']'], "");
            if !model_names.contains(&clean_target) || clean_target == *source_model {
                continue;
            }

            let rel_name = extract_prisma_relation_name(trimmed);

            let target_body = match model_bodies.get(&clean_target) {
                Some(b) => b,
                None => continue,
            };

            let mut has_opposite = false;
            for t_line in target_body.lines() {
                let t_trim = t_line.trim();
                if t_trim.starts_with("//") {
                    continue;
                }
                let t_parts: Vec<&str> = t_trim.split_whitespace().collect();
                if t_parts.len() >= 2 {
                    let t_clean_type = t_parts[1].replace(['?', '[', ']'], "");
                    if t_clean_type == *source_model && !t_trim.contains("fields:") {
                        let t_rel_name = extract_prisma_relation_name(t_trim);
                        match (&rel_name, &t_rel_name) {
                            (Some(name), Some(t_name)) if name == t_name => {
                                has_opposite = true;
                                break;
                            }
                            (None, None) => {
                                has_opposite = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            if !has_opposite {
                // If source_model only defines 1 relation to clean_target with an explicit name,
                // and clean_target already has an un-named non-defining opposite field of type source_model
                // (e.g. `User.tasks Task[]`), attach `@relation("...")` to it instead of creating `tasksTask`.
                let def_count = defining_counts.get(&(source_model.clone(), clean_target.clone())).copied().unwrap_or(1);
                if def_count == 1 {
                    if let Some(ref r_name) = rel_name {
                        let existing_unnamed_field = target_body.lines().find_map(|l| {
                            let lt = l.trim();
                            if lt.starts_with("//") || lt.contains("fields:") {
                                return None;
                            }
                            let l_parts: Vec<&str> = lt.split_whitespace().collect();
                            if l_parts.len() >= 2
                                && l_parts[1].replace(['?', '[', ']'], "") == *source_model
                                && extract_prisma_relation_name(lt).is_none()
                            {
                                Some(l_parts[0].to_string())
                            } else {
                                None
                            }
                        });
                        if let Some(existing_field_name) = existing_unnamed_field {
                            if let Some(attaches) = pending_name_attachments.get_mut(&clean_target) {
                                if !attaches.iter().any(|(_, rn)| rn == r_name) {
                                    attaches.push((existing_field_name, r_name.clone()));
                                }
                            }
                            continue;
                        }
                    }
                }

                // Check if foreign key is unique (1-to-1 relation)
                let is_one_to_one = if let Some(open_f) = trimmed.find("fields: [") {
                    let fk_slice = &trimmed[open_f + 9..];
                    if let Some(close_f) = fk_slice.find(']') {
                        let fk_name = fk_slice[..close_f].trim();
                        body.lines().any(|l| is_single_field_unique(l, fk_name))
                    } else {
                        false
                    }
                } else {
                    false
                };

                let mut field_name = if is_one_to_one {
                    let s = source_model.to_string();
                    if let Some(first_char) = s.chars().next() {
                        format!("{}{}", first_char.to_lowercase(), &s[first_char.len_utf8()..])
                    } else {
                        s
                    }
                } else {
                    let mut plural = source_model.to_string();
                    if let Some(first_char) = plural.chars().next() {
                        plural = format!("{}{}", first_char.to_lowercase(), &plural[first_char.len_utf8()..]);
                    }

                    if plural.ends_with('y') && !plural.ends_with("ey") && !plural.ends_with("ay") && !plural.ends_with("oy") {
                        plural.pop();
                        plural.push_str("ies");
                    } else if !plural.ends_with('s') {
                        plural.push('s');
                    }
                    plural
                };

                let field_type = if is_one_to_one {
                    format!("{}?", source_model)
                } else {
                    format!("{}[]", source_model)
                };

                // Check collisions with existing fields or pending inserts
                let existing_inserts = pending_inserts.get(&clean_target).cloned().unwrap_or_default();
                let has_existing_field_of_type = target_body.lines().any(|l| {
                    let t = l.trim();
                    if t.starts_with("//") { return false; }
                    let parts: Vec<&str> = t.split_whitespace().collect();
                    parts.len() >= 2 && parts[1].replace(['?', '[', ']'], "") == *source_model
                }) || existing_inserts.iter().any(|l| {
                    let parts: Vec<&str> = l.trim().split_whitespace().collect();
                    parts.len() >= 2 && parts[1].replace(['?', '[', ']'], "") == *source_model
                });

                if rel_name.is_none() && has_existing_field_of_type {
                    // Do not inject multiple un-named back relations of the same type (would cause Prisma P1012 ambiguous relation)
                    continue;
                }

                if let Some(ref r_name) = rel_name {
                    let already_has_this_named_rel = target_body.lines().any(|l| {
                        extract_prisma_relation_name(l).as_deref() == Some(r_name.as_str())
                    }) || existing_inserts.iter().any(|l| {
                        extract_prisma_relation_name(l).as_deref() == Some(r_name.as_str())
                    }) || pending_name_attachments.get(&clean_target).map_or(false, |a| {
                        a.iter().any(|(_, rn)| rn == r_name)
                    });
                    if already_has_this_named_rel {
                        continue;
                    }
                }

                let has_collision = target_body.lines().any(|l| {
                    let t = l.trim();
                    !t.starts_with("//") && t.split_whitespace().next() == Some(field_name.as_str())
                }) || existing_inserts.iter().any(|i| {
                    i.trim().split_whitespace().next() == Some(field_name.as_str())
                });
                if has_collision {
                    if rel_name.is_some() {
                        field_name = format!("{}{}", field_name, source_model);
                    } else {
                        continue;
                    }
                }

                let insert_line = match rel_name {
                    Some(name) => format!("  {:12} {} @relation(\"{}\")", field_name, field_type, name),
                    None => format!("  {:12} {}", field_name, field_type),
                };

                if let Some(inserts) = pending_inserts.get_mut(&clean_target) {
                    if !inserts.contains(&insert_line) {
                        inserts.push(insert_line);
                    }
                }
            }
        }
    }

    let mut result = schema.to_string();
    for target_model in &model_names {
        let inserts = pending_inserts.get(target_model).cloned().unwrap_or_default();
        let attaches = pending_name_attachments.get(target_model).cloned().unwrap_or_default();
        if inserts.is_empty() && attaches.is_empty() {
            continue;
        }

        let pattern = format!(r"(?ms)(^model\s+{}\s*\{{)([\s\S]*?)(\n*\}})", target_model);
        if let Ok(block_re) = regex::Regex::new(&pattern) {
            result = block_re.replace(&result, |caps: &regex::Captures| {
                let p1 = &caps[1];
                let p2 = &caps[2];
                let p3 = &caps[3];
                let mut body_lines: Vec<String> = p2.lines().map(|l| l.to_string()).collect();
                for (fname, rname) in &attaches {
                    for l in body_lines.iter_mut() {
                        let t = l.trim();
                        if !t.starts_with("//") && t.split_whitespace().next() == Some(fname.as_str()) && extract_prisma_relation_name(t).is_none() {
                            *l = append_prisma_attr_before_comment(l, &format!("@relation(\"{}\")", rname));
                            break;
                        }
                    }
                }
                let updated_body = body_lines.join("\n");
                if inserts.is_empty() {
                    format!("{}{}\n{}", p1, updated_body.trim_end(), p3.trim_start())
                } else {
                    let insert_str = inserts.join("\n");
                    format!("{}{}\n{}\n{}", p1, updated_body.trim_end(), insert_str, p3.trim_start())
                }
            }).to_string();
        }
    }

    result
}

fn capitalize_first_letter(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Disambiguates multiple relations between the same pair of models in Prisma schema.
/// When a model defines two or more relations pointing to the same target model (e.g. `sourceWallet` and `destinationWallet` in `Transaction`),
/// Prisma requires unique relation names on both sides.
/// This function:
/// 1. Finds models that have 2+ defining relations pointing to the same target model.
/// 2. If any of those relations lack explicit relation names or share duplicate names, assigns unique relation names based on the model and field name.
/// 3. In the target model, prunes any ambiguous un-named back-relations pointing to that model, and injects matching named back-relation fields.
pub fn disambiguate_prisma_relations(schema: &str) -> String {
    let model_re = match regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{([^}]+)\}") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut model_names: Vec<String> = Vec::new();
    let mut model_bodies: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for cap in model_re.captures_iter(schema) {
        if let (Some(m_name), Some(m_body)) = (cap.get(1), cap.get(2)) {
            let name = m_name.as_str().to_string();
            let lines: Vec<String> = m_body.as_str().lines().map(|s| s.to_string()).collect();
            model_names.push(name.clone());
            model_bodies.insert(name, lines);
        }
    }

    if model_names.is_empty() {
        return schema.to_string();
    }

    struct DefiningRel {
        source_model: String,
        target_model: String,
        line_idx: usize,
        field_name: String,
        rel_name: Option<String>,
        is_one_to_one: bool,
    }

    let mut defining_by_pair: std::collections::HashMap<(String, String), Vec<DefiningRel>> = std::collections::HashMap::new();

    for source_model in &model_names {
        if let Some(lines) = model_bodies.get(source_model) {
            for (line_idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("//") || !trimmed.contains("@relation") || !trimmed.contains("fields:") || !trimmed.contains("references:") {
                    continue;
                }

                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() < 2 {
                    continue;
                }

                let field_name = parts[0].to_string();
                let clean_target = parts[1].replace(['?', '[', ']'], "");
                if !model_names.contains(&clean_target) {
                    continue;
                }

                let rel_name = extract_prisma_relation_name(trimmed);

                let is_one_to_one = if let Some(open_f) = trimmed.find("fields: [") {
                    let fk_slice = &trimmed[open_f + 9..];
                    if let Some(close_f) = fk_slice.find(']') {
                        let fk_name = fk_slice[..close_f].trim();
                        lines.iter().any(|l| is_single_field_unique(l, fk_name))
                    } else {
                        false
                    }
                } else {
                    false
                };

                let pair_key = if source_model <= &clean_target {
                    (source_model.clone(), clean_target.clone())
                } else {
                    (clean_target.clone(), source_model.clone())
                };

                defining_by_pair.entry(pair_key).or_default().push(DefiningRel {
                    source_model: source_model.clone(),
                    target_model: clean_target,
                    line_idx,
                    field_name,
                    rel_name,
                    is_one_to_one,
                });
            }
        }
    }

    // Process pairs where disambiguation is needed:
    // 1. Multiple relations between models (rels.len() >= 2)
    // 2. Self-relations (m1 == m2) lacking explicit relation names
    for ((m1, m2), rels) in defining_by_pair {
        let is_self_relation = m1 == m2;
        if rels.len() < 2 && !is_self_relation {
            continue;
        }

        // Check if any lacks relation name or if any share relation names
        let mut rel_names_used = std::collections::HashSet::new();
        let mut needs_disambiguation = false;
        for r in &rels {
            if let Some(ref name) = r.rel_name {
                if !rel_names_used.insert(name.clone()) {
                    needs_disambiguation = true;
                }
            } else {
                needs_disambiguation = true;
            }
        }

        if !needs_disambiguation {
            continue;
        }

        // Assign distinct relation names to all relations in this pair
        for r in &rels {
            let cap_field = capitalize_first_letter(&r.field_name);
            let rel_name = if is_self_relation && rels.len() == 1 {
                format!("{}Hierarchy", r.source_model)
            } else {
                format!("{}{}", r.source_model, cap_field)
            };

            if let Some(src_lines) = model_bodies.get_mut(&r.source_model) {
                let original_line = &src_lines[r.line_idx];
                let updated_line = if let Some(ref existing) = r.rel_name {
                    let dq = format!("\"{}\"", existing);
                    let sq = format!("'{}'", existing);
                    if original_line.contains(&dq) {
                        original_line.replace(&dq, &format!("\"{}\"", rel_name))
                    } else if original_line.contains(&sq) {
                        original_line.replace(&sq, &format!("\"{}\"", rel_name))
                    } else {
                        original_line.clone()
                    }
                } else if original_line.contains("@relation(") {
                    original_line.replace("@relation(", &format!("@relation(\"{}\", ", rel_name))
                } else {
                    original_line.clone()
                };
                src_lines[r.line_idx] = updated_line;
            }

            // In target_model: prune un-named back relations and inject/update named opposite
            if let Some(target_lines) = model_bodies.get_mut(&r.target_model) {
                if !is_self_relation {
                    target_lines.retain(|l| {
                        let trimmed = l.trim();
                        if trimmed.starts_with("//") {
                            return true;
                        }
                        let parts: Vec<&str> = trimmed.split_whitespace().collect();
                        if parts.len() >= 2 {
                            let field_type = parts[1].replace(['?', '[', ']'], "");
                            if field_type == r.source_model && !trimmed.contains("fields:") {
                                if extract_prisma_relation_name(trimmed).is_none() {
                                    return false;
                                }
                            }
                        }
                        true
                    });
                }

                let mut plural_source = r.source_model.clone();
                if let Some(first_char) = plural_source.chars().next() {
                    plural_source = format!("{}{}", first_char.to_lowercase(), &plural_source[first_char.len_utf8()..]);
                }
                if plural_source.ends_with('y') && !plural_source.ends_with("ey") && !plural_source.ends_with("ay") && !plural_source.ends_with("oy") {
                    plural_source.pop();
                    plural_source.push_str("ies");
                } else if !plural_source.ends_with('s') {
                    plural_source.push('s');
                }

                let already_has_named_opposite = target_lines.iter().any(|l| {
                    let trimmed = l.trim();
                    if trimmed.starts_with("//") {
                        return false;
                    }
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let field_type = parts[1].replace(['?', '[', ']'], "");
                        field_type == r.source_model && extract_prisma_relation_name(trimmed).as_deref() == Some(rel_name.as_str())
                    } else {
                        false
                    }
                });

                if !already_has_named_opposite {
                    // Check if there is an un-named back-relation array of source_model that we can name
                    let mut updated_existing_back = false;
                    for t_line in target_lines.iter_mut() {
                        let trimmed = t_line.trim();
                        if trimmed.starts_with("//") {
                            continue;
                        }
                        let parts: Vec<&str> = trimmed.split_whitespace().collect();
                        if parts.len() >= 2 {
                            let field_type = parts[1].replace(['?', '[', ']'], "");
                            if field_type == r.source_model && !trimmed.contains("fields:") && !trimmed.contains("@relation") {
                                *t_line = append_prisma_attr_before_comment(t_line, &format!("@relation(\"{}\")", rel_name));
                                updated_existing_back = true;
                                break;
                            }
                        }
                    }

                    if !updated_existing_back {
                        let opposite_field_name = if is_self_relation {
                            if r.field_name == "parent" { "children".to_string() } else { format!("{}Children", r.field_name) }
                        } else if r.is_one_to_one {
                            format!("{}{}", r.field_name, r.source_model)
                        } else {
                            format!("{}{}", r.field_name, capitalize_first_letter(&plural_source))
                        };

                        let field_type = if r.is_one_to_one {
                            format!("{}?", r.source_model)
                        } else {
                            format!("{}[]", r.source_model)
                        };

                        let new_line = format!("  {:14} {} @relation(\"{}\")", opposite_field_name, field_type, rel_name);
                        target_lines.push(new_line);
                    }
                }
            }
        }
    }

    // Reconstruct schema
    let mut result = schema.to_string();
    for (m_name, lines) in model_bodies {
        let pattern = format!(r"(?ms)(^model\s+{}\s*\{{)([\s\S]*?)(\n*\}})", m_name);
        if let Ok(block_re) = regex::Regex::new(&pattern) {
            let updated_body = lines.join("\n");
            result = block_re
                .replace(&result, |caps: &regex::Captures| {
                    format!("{}{}\n{}", &caps[1], updated_body.trim_end(), caps[3].trim_start())
                })
                .to_string();
        }
    }

    result
}

/// Enforces Prisma relational integrity:
/// 1. Prunes ghost self-relations while preserving valid paired self-relations (e.g. `Category.parent` <-> `Category.children`).
/// 2. Prunes orphaned or duplicate back-relations (e.g. `tasks` + `tasksTask` sharing `@relation("AssignedTo")`, or `UserSubscriptionsSecondary`).
/// 3. Synchronizes unmatched un-named back-relation field relation names if only 1 unclaimed defining relation exists.
/// 4. Enforces 1-to-1 foreign key `@unique` attribute on defining side (e.g. `projectId String @unique` for `Analytics.project`).
pub fn sanitize_prisma_relations(schema: &str) -> String {
    let model_re = match regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{([^}]+)\}") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut model_names: Vec<String> = Vec::new();
    let mut model_bodies: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for cap in model_re.captures_iter(schema) {
        if let (Some(m_name), Some(m_body)) = (cap.get(1), cap.get(2)) {
            let name = m_name.as_str().to_string();
            let lines: Vec<String> = m_body.as_str().lines().map(|s| s.to_string()).collect();
            model_names.push(name.clone());
            model_bodies.insert(name, lines);
        }
    }

    if model_names.is_empty() {
        return schema.to_string();
    }

    let fields_re = match regex::Regex::new(r"fields:\s*\[([^\]]+)\]") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    // 1. Prune ghost / duplicate self-relations (preserving valid paired self-relations like parent <-> children
    // and valid symmetric named self-M2M relations, while pruning un-paired or un-annotated self-lists like `projects Club[]`).
    for m in &model_names {
        if let Some(lines) = model_bodies.get_mut(m) {
            let mut defining_self_rel_names: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut has_unnamed_defining_self_rel = false;
            let mut named_self_rel_indices: Vec<(usize, String)> = Vec::new();
            let mut unnamed_self_rel_indices: Vec<usize> = Vec::new();
            for (idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("@@") || trimmed.starts_with('@') || trimmed.starts_with("/*") {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let field_type = parts[1].replace(['?', '[', ']'], "");
                    if field_type == *m {
                        let rel_name = extract_prisma_relation_name(trimmed);
                        if trimmed.contains("fields:") {
                            if let Some(rname) = rel_name {
                                defining_self_rel_names.insert(rname);
                            } else {
                                has_unnamed_defining_self_rel = true;
                            }
                        } else if let Some(rname) = rel_name {
                            named_self_rel_indices.push((idx, rname));
                        } else {
                            unnamed_self_rel_indices.push(idx);
                        }
                    }
                }
            }

            let mut to_remove = std::collections::HashSet::new();
            let mut seen_defining_self_opposites: std::collections::HashSet<String> = std::collections::HashSet::new();
            for (idx, rname) in &named_self_rel_indices {
                if defining_self_rel_names.contains(rname) {
                    // Valid self-relation opposite paired with a defining `fields:` relation on the same model.
                    // Keep the first copy; prune any extra duplicate opposite copies.
                    if !seen_defining_self_opposites.insert(rname.clone()) {
                        to_remove.insert(*idx);
                    }
                } else {
                    let references_other_model = model_names.iter().any(|other| other != m && rname.contains(other.as_str()));
                    let matches_count = named_self_rel_indices.iter().filter(|(_, r)| r == rname).count();
                    if references_other_model || matches_count == 1 {
                        to_remove.insert(*idx);
                    }
                }
            }

            // Prune any un-annotated self-relation without an un-named defining `fields:` self-relation on the same model
            let mut kept_one_unnamed_self = false;
            for idx in unnamed_self_rel_indices {
                if has_unnamed_defining_self_rel && !kept_one_unnamed_self {
                    kept_one_unnamed_self = true;
                } else {
                    to_remove.insert(idx);
                }
            }

            if !to_remove.is_empty() {
                let mut filtered = Vec::new();
                for (idx, line) in lines.iter().enumerate() {
                    if !to_remove.contains(&idx) {
                        filtered.push(line.clone());
                    }
                }
                *lines = filtered;
            }
        }
    }

    // 2. Prune ghost / duplicate back-relations & sync relation names
    for m1 in &model_names {
        let mut target_defining_map: std::collections::HashMap<String, Vec<(Option<String>, Vec<String>)>> = std::collections::HashMap::new();
        let mut target_non_defining_map: std::collections::HashMap<String, Vec<Option<String>>> = std::collections::HashMap::new();

        for m2 in &model_names {
            if m2 == m1 { continue; }
            if let Some(m2_lines) = model_bodies.get(m2) {
                for line in m2_lines {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("@@") || trimmed.starts_with('@') || trimmed.starts_with("/*") {
                        continue;
                    }
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let t_type = parts[1].replace(['?', '[', ']'], "");
                        if t_type == *m1 {
                            let r_name = extract_prisma_relation_name(trimmed);
                            if trimmed.contains("fields:") {
                                let fks: Vec<String> = fields_re.captures(trimmed)
                                    .map(|c| c[1].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                                    .unwrap_or_default();
                                target_defining_map.entry(m2.clone()).or_default().push((r_name, fks));
                            } else {
                                target_non_defining_map.entry(m2.clone()).or_default().push(r_name);
                            }
                        }
                    }
                }
            }
        }

        if let Some(lines) = model_bodies.get_mut(m1) {
            struct BackCand {
                idx: usize,
                field_name: String,
                t_type: String,
                r_name: Option<String>,
            }

            let mut candidates: Vec<BackCand> = Vec::new();
            for (idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("@@") || trimmed.starts_with('@') || trimmed.starts_with("/*") {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    let t_type = parts[1].replace(['?', '[', ']'], "");
                    let is_defining = trimmed.contains("fields:") && trimmed.contains("references:");
                    if model_names.contains(&t_type) && t_type != *m1 && !is_defining {
                        candidates.push(BackCand {
                            idx,
                            field_name: parts[0].to_string(),
                            t_type,
                            r_name: extract_prisma_relation_name(trimmed),
                        });
                    }
                }
            }

            // Order exact-name candidates so natural names (e.g. `tasks`) claim before synthetic
            // collision suffixes (e.g. `tasksTask`) even if they appear later in the model block.
            let is_synthetic_dup = |c: &BackCand| -> bool {
                candidates.iter().any(|other| {
                    other.idx != c.idx
                        && other.t_type == c.t_type
                        && other.r_name == c.r_name
                        && c.field_name == format!("{}{}", other.field_name, c.t_type)
                })
            };

            let mut named_order: Vec<usize> = (0..candidates.len())
                .filter(|&i| candidates[i].r_name.is_some())
                .collect();
            named_order.sort_by_key(|&i| (is_synthetic_dup(&candidates[i]), candidates[i].idx));

            let mut to_remove = std::collections::HashSet::new();
            let mut line_updates: Vec<(usize, String)> = Vec::new();
            let mut claimed_defining: std::collections::HashMap<String, std::collections::HashSet<usize>> = std::collections::HashMap::new();
            let mut claimed_m2m_named: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
            let mut claimed_m2m_unnamed_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

            // Pass 2A: Exact named matches and explicit named M2M
            for ci in named_order {
                let c = &candidates[ci];
                let r_str = match c.r_name.as_deref() {
                    Some(s) => s,
                    None => continue,
                };

                if let Some(defining_list) = target_defining_map.get(&c.t_type) {
                    let claimed = claimed_defining.entry(c.t_type.clone()).or_default();
                    if let Some((d_idx, _)) = defining_list.iter().enumerate().find(|(di, (dr, _))| {
                        !claimed.contains(di) && dr.as_deref() == Some(r_str)
                    }) {
                        claimed.insert(d_idx);
                    } else if defining_list.iter().any(|(dr, _)| dr.as_deref() == Some(r_str)) {
                        // Duplicate opposite field pointing to an already-claimed defining relation (e.g. `tasks` + `tasksTask`)
                        to_remove.insert(c.idx);
                    } else {
                        // Not pointing to any defining rel — preserve only if target has symmetric named M2M (and not duplicated)
                        let has_sym_m2m = target_non_defining_map
                            .get(&c.t_type)
                            .map_or(false, |list| list.iter().any(|rn| rn.as_deref() == Some(r_str)));
                        if has_sym_m2m && claimed_m2m_named.insert((c.t_type.clone(), r_str.to_string())) {
                            // Keep symmetric named M2M
                        } else {
                            to_remove.insert(c.idx);
                        }
                    }
                } else {
                    // Explicit named M2M where neither model defines `fields:` — require symmetric opposite on target and deduplicate
                    let has_sym_m2m = target_non_defining_map
                        .get(&c.t_type)
                        .map_or(false, |list| list.iter().any(|rn| rn.as_deref() == Some(r_str)));
                    if has_sym_m2m && claimed_m2m_named.insert((c.t_type.clone(), r_str.to_string())) {
                        // Keep symmetric named M2M
                    } else {
                        to_remove.insert(c.idx);
                    }
                }
            }

            // Pass 2B: Un-named opposite fields
            for c in candidates.iter().filter(|c| c.r_name.is_none()) {
                if let Some(defining_list) = target_defining_map.get(&c.t_type) {
                    let claimed = claimed_defining.entry(c.t_type.clone()).or_default();
                    if let Some((d_idx, _)) = defining_list.iter().enumerate().find(|(di, (dr, _))| {
                        !claimed.contains(di) && dr.is_none()
                    }) {
                        claimed.insert(d_idx);
                    } else if defining_list.len() == 1 && !claimed.contains(&0) {
                        if let Some(ref d_name) = defining_list[0].0 {
                            claimed.insert(0);
                            line_updates.push((
                                c.idx,
                                append_prisma_attr_before_comment(&lines[c.idx], &format!("@relation(\"{}\")", d_name)),
                            ));
                        }
                    } else {
                        // Allow coexisting implicit un-named M2M only if target also has un-named non-defining field(s) pointing back to m1
                        let target_unnamed_total = target_non_defining_map
                            .get(&c.t_type)
                            .map_or(0, |list| list.iter().filter(|rn| rn.is_none()).count());
                        let used = claimed_m2m_unnamed_counts.entry(c.t_type.clone()).or_insert(0);
                        if *used < target_unnamed_total {
                            *used += 1;
                        } else {
                            to_remove.insert(c.idx);
                        }
                    }
                } else {
                    // No defining `fields:` relation on target model: keep only symmetric implicit un-named M2M; prune one-sided orphan fields
                    let target_unnamed_total = target_non_defining_map
                        .get(&c.t_type)
                        .map_or(0, |list| list.iter().filter(|rn| rn.is_none()).count());
                    let used = claimed_m2m_unnamed_counts.entry(c.t_type.clone()).or_insert(0);
                    if *used < target_unnamed_total {
                        *used += 1;
                    } else {
                        to_remove.insert(c.idx);
                    }
                }
            }

            for (idx, updated) in line_updates {
                lines[idx] = updated;
            }

            if !to_remove.is_empty() {
                let mut filtered = Vec::new();
                for (idx, line) in lines.iter().enumerate() {
                    if !to_remove.contains(&idx) {
                        filtered.push(line.clone());
                    }
                }
                *lines = filtered;
            }
        }
    }

    // 3. 1-to-1 foreign key `@unique` enforcement
    for m1 in &model_names {
        let m1_lines = model_bodies.get(m1).cloned().unwrap_or_default();
        for line in &m1_lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("@@") || trimmed.starts_with('@') || trimmed.starts_with("/*") {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let raw_type = parts[1];
                let is_singular = !raw_type.contains('[');
                let target_model = raw_type.replace(['?', '[', ']'], "");

                if is_singular && model_names.contains(&target_model) && target_model != *m1 && !trimmed.contains("fields:") {
                    let r_name = extract_prisma_relation_name(trimmed);

                    if let Some(b_lines) = model_bodies.get_mut(&target_model) {
                        let mut fk_to_mark_unique: Option<String> = None;
                        for b_line in b_lines.iter() {
                            let bt = b_line.trim();
                            if bt.starts_with("//") {
                                continue;
                            }
                            let bparts: Vec<&str> = bt.split_whitespace().collect();
                            if bparts.len() >= 2 {
                                let bt_type = bparts[1].replace(['?', '[', ']'], "");
                                if bt_type == *m1 && bt.contains("fields:") {
                                    let b_rname = extract_prisma_relation_name(bt);
                                    let matches = match (&r_name, &b_rname) {
                                        (Some(r1), Some(r2)) => r1 == r2,
                                        (None, None) => true,
                                        _ => false,
                                    };
                                    if matches {
                                        if let Some(f_cap) = fields_re.captures(bt) {
                                            let fks: Vec<&str> = f_cap[1].split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                                            if let Some(first_fk) = fks.first() {
                                                fk_to_mark_unique = Some((*first_fk).to_string());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(fk) = fk_to_mark_unique {
                            let already_unique = b_lines.iter().any(|l| is_single_field_unique(l, &fk));

                            if !already_unique {
                                for b_line in b_lines.iter_mut() {
                                    let lt = b_line.trim();
                                    if !lt.starts_with("//") {
                                        let l_parts: Vec<&str> = lt.split_whitespace().collect();
                                        if l_parts.first() == Some(&fk.as_str()) {
                                            *b_line = append_prisma_attr_before_comment(b_line, "@unique");
                                            break;
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

    // Reconstruct schema
    let mut result = schema.to_string();
    for (m_name, lines) in &model_bodies {
        let pattern = format!(r"(?ms)(^model\s+{}\s*\{{)([\s\S]*?)(\n*\}})", m_name);
        if let Ok(block_re) = regex::Regex::new(&pattern) {
            let body_str = lines.join("\n");
            result = block_re.replace(&result, |caps: &regex::Captures| {
                format!("{}{}\n{}", &caps[1], body_str.trim_end(), &caps[3].trim_start())
            }).to_string();
        }
    }

    result
}

pub fn sanitize_ts_interfaces(content: &str) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut in_interface_or_type = false;
    let mut brace_depth: i32 = 0;
    let kebab_prop_re = regex::Regex::new(r#"^(\s*)([a-zA-Z0-9_]+-[a-zA-Z0-9_-]+)(\s*\??:)"#).ok();

    for line in content.lines() {
        let trimmed = line.trim();

        // Strip orphaned data-axiom-* attributes that were misplaced outside of any JSX tag (e.g. after }; at module level)
        if (trimmed.starts_with("data-axiom-component=") || trimmed.starts_with("data-axiom-file="))
            && !trimmed.starts_with('<')
        {
            let prev_non_empty = output.iter().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim()).unwrap_or("");
            if prev_non_empty.is_empty()
                || prev_non_empty.ends_with(';')
                || prev_non_empty.ends_with("};")
                || prev_non_empty.ends_with("/>")
                || prev_non_empty.starts_with("import ")
                || prev_non_empty.starts_with("export ")
            {
                continue;
            }
        }

        if (trimmed.starts_with("interface ") || trimmed.starts_with("export interface ")
            || (trimmed.starts_with("type ") && (trimmed.contains('=') || trimmed.contains('{')))
            || (trimmed.starts_with("export type ") && (trimmed.contains('=') || trimmed.contains('{'))))
            && trimmed.contains('{')
        {
            in_interface_or_type = true;
        }

        if in_interface_or_type {
            for c in trimmed.chars() {
                if c == '{' {
                    brace_depth += 1;
                } else if c == '}' {
                    brace_depth -= 1;
                    if brace_depth <= 0 {
                        in_interface_or_type = false;
                        brace_depth = 0;
                    }
                }
            }

            // Strip internal axiom data props if mistakenly typed in component interfaces
            if trimmed.contains("data-axiom-component") || trimmed.contains("data-axiom-file") {
                continue;
            }

            // Quote any unquoted kebab-case prop name in interface/type (e.g. data-testid?: string -> "data-testid"?: string)
            if let Some(ref re) = kebab_prop_re {
                if re.is_match(line) {
                    output.push(re.replace(line, r#"$1"$2"$3"#).to_string());
                    continue;
                }
            }
        }

        output.push(line.to_string());
    }

    output.join("\n")
}

pub fn hoist_use_client_directive(content: &str) -> String {
    let has_use_client = content.contains("\"use client\"") || content.contains("'use client'");
    if !has_use_client {
        return content.to_string();
    }

    let mut statements_before_use_client = 0;
    let mut use_client_count = 0;
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
            continue;
        }
        if t.starts_with("\"use client\"") || t.starts_with("'use client'") {
            use_client_count += 1;
        } else if use_client_count == 0 {
            statements_before_use_client += 1;
        }
    }

    if statements_before_use_client == 0 && use_client_count == 1 {
        return content.to_string();
    }

    let mut filtered_lines: Vec<String> = Vec::new();
    let mut leading_comments: Vec<String> = Vec::new();
    let mut in_leading_comments = true;

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("\"use client\"") || t.starts_with("'use client'") {
            continue;
        }
        if in_leading_comments && (t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') || t.is_empty()) {
            leading_comments.push(line.to_string());
        } else {
            in_leading_comments = false;
            filtered_lines.push(line.to_string());
        }
    }

    let mut res = String::new();
    let comments = leading_comments.join("\n");
    let trimmed_comments = comments.trim_end();
    if !trimmed_comments.is_empty() {
        res.push_str(trimmed_comments);
        res.push_str("\n\n");
    }
    res.push_str("\"use client\";\n");
    res.push_str(&filtered_lines.join("\n"));
    res
}

pub fn insert_import_after_use_client(content: &str, import_line: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut insert_idx = 0;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if t.starts_with("\"use client\"") || t.starts_with("'use client'") {
            insert_idx = i + 1;
        }
        break;
    }
    lines.insert(insert_idx, import_line.to_string());
    let joined = lines.join("\n");
    hoist_use_client_directive(&joined)
}

pub fn ensure_zod_imports(content: &str) -> String {
    let has_zod_call = regex::Regex::new(r"(?m)\bz\.(?:object|string|number|boolean|date|enum|array|record|any|unknown|infer|literal|union|optional|nullable|lazy|coerce)\b")
        .map(|re| re.is_match(content))
        .unwrap_or(false);

    if !has_zod_call {
        return content.to_string();
    }

    let has_zod_import = content.contains("from 'zod'")
        || content.contains("from \"zod\"")
        || content.contains("require('zod')")
        || content.contains("require(\"zod\")");

    if has_zod_import {
        return content.to_string();
    }

    let has_decl = regex::Regex::new(r"(?m)\b(?:const|let|var|function|class)\s+z\s*[:=]")
        .map(|re| re.is_match(content))
        .unwrap_or(false);
    if has_decl {
        return content.to_string();
    }

    insert_import_after_use_client(content, "import { z } from 'zod';")
}

/// Strips Next.js navigation hooks (`useRouter`, `usePathname`, `useParams`, `useSearchParams`)
/// if the AI accidentally imported them from 'react' instead of 'next/navigation'.
pub fn clean_react_navigation_hooks_from_react_imports(content: &str) -> String {
    let next_nav_hooks = ["useRouter", "usePathname", "useParams", "useSearchParams"];
    let react_import_re = match regex::Regex::new(r#"(?m)^(\s*import\s+)([^;]+?)(\s+from\s*['"]react['"]\s*;?)"#) {
        Ok(r) => r,
        Err(_) => return content.to_string(),
    };

    let mut result = content.to_string();
    result = react_import_re.replace_all(&result, |caps: &regex::Captures| {
        let prefix = &caps[1];
        let import_clause = caps[2].trim();

        if !import_clause.contains('{') {
            return caps[0].to_string();
        }

        let has_nav_hook = next_nav_hooks.iter().any(|h| {
            regex::Regex::new(&format!(r"\b{}\b", h)).map(|re| re.is_match(import_clause)).unwrap_or(false)
        });

        if !has_nav_hook {
            return caps[0].to_string();
        }

        let open_brace = match import_clause.find('{') {
            Some(idx) => idx,
            None => return caps[0].to_string(),
        };
        let close_brace = match import_clause.find('}') {
            Some(idx) => idx,
            None => return caps[0].to_string(),
        };

        let default_part = import_clause[..open_brace].trim().trim_end_matches(',').trim();
        let named_part = &import_clause[open_brace + 1..close_brace];

        let filtered_named: Vec<String> = named_part
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .filter(|s| {
                let name = s.split_whitespace().next().unwrap_or("");
                !next_nav_hooks.contains(&name)
            })
            .map(|s| s.to_string())
            .collect();

        if filtered_named.is_empty() {
            if !default_part.is_empty() {
                format!("{}{} from 'react';", prefix, default_part)
            } else {
                String::new()
            }
        } else if !default_part.is_empty() {
            format!("{}{}, {{ {} }} from 'react';", prefix, default_part, filtered_named.join(", "))
        } else {
            format!("{}{{ {} }} from 'react';", prefix, filtered_named.join(", "))
        }
    }).to_string();

    let mut cleaned_lines = Vec::new();
    for line in result.lines() {
        if !line.trim().is_empty() || cleaned_lines.last().map(|l: &&str| !l.trim().is_empty()).unwrap_or(false) {
            cleaned_lines.push(line);
        }
    }
    cleaned_lines.join("\n")
}

pub fn ensure_react_and_next_hook_imports(content: &str) -> String {
    // 0. Clean any Next.js navigation hooks mistakenly imported from 'react'
    let content_cleaned = clean_react_navigation_hooks_from_react_imports(content);

    let code_body: String = content_cleaned
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with("import ") && !t.starts_with("//") && !t.starts_with('*')
        })
        .collect::<Vec<&str>>()
        .join("\n");

    let mut result = content_cleaned;

    // 1. Standard React hooks
    let react_hooks = [
        "useState",
        "useEffect",
        "useContext",
        "useRef",
        "useReducer",
        "useMemo",
        "useCallback",
    ];
    let mut missing_react_hooks: Vec<&str> = Vec::new();
    for hook in &react_hooks {
        let call_pattern = format!(r"(?m)(?:^|[^a-zA-Z0-9_.$]){}\s*(?:<[^>]*>)?\s*\(", hook);
        let is_called = regex::Regex::new(&call_pattern)
            .map(|re| re.is_match(&code_body))
            .unwrap_or(false);
        if !is_called {
            continue;
        }
        let import_pattern = format!(
            r#"(?s)import\s+[^;]*?\b{}\b[^;]*?from\s*['"]react['"]"#,
            hook
        );
        let destructure_pattern = format!(r"\bconst\s*\{{[^}}]*\b{}\b[^}}]*\}}\s*=\s*React\b", hook);
        let decl_pattern = format!(r"\b(?:function|const|let|var)\s+{}\b", hook);
        let already_present = regex::Regex::new(&import_pattern)
            .map(|re| re.is_match(&result))
            .unwrap_or(false)
            || regex::Regex::new(&destructure_pattern)
                .map(|re| re.is_match(&result))
                .unwrap_or(false)
            || regex::Regex::new(&decl_pattern)
                .map(|re| re.is_match(&result))
                .unwrap_or(false);
        if !already_present {
            missing_react_hooks.push(hook);
        }
    }

    if !missing_react_hooks.is_empty() {
        let named_react_re = regex::Regex::new(
            r#"(?m)^(\s*import\s+(?:[A-Za-z0-9_$]+\s*,\s*)?\{)([^}]*)(\}\s*from\s*['"]react['"]\s*;?)"#,
        )
        .ok();
        let default_react_re = regex::Regex::new(
            r#"(?m)^(\s*import\s+React)(\s+from\s*['"]react['"]\s*;?)"#,
        )
        .ok();

        let mut handled = false;
        if let Some(ref re) = named_react_re {
            if let Some(cap) = re.captures(&result) {
                let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let existing = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim();
                let suffix = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                if !prefix.contains("import type") {
                    let combined = if existing.is_empty() {
                        format!(" {} ", missing_react_hooks.join(", "))
                    } else {
                        format!(" {}, {} ", existing.trim_end_matches(','), missing_react_hooks.join(", "))
                    };
                    let replacement = format!("{}{}{}", prefix, combined, suffix);
                    result = re.replace(&result, replacement.as_str()).to_string();
                    handled = true;
                }
            }
        }
        if !handled {
            if let Some(ref re) = default_react_re {
                if re.is_match(&result) {
                    let replacement = format!("$1, {{ {} }}$2", missing_react_hooks.join(", "));
                    result = re.replace(&result, replacement.as_str()).to_string();
                    handled = true;
                }
            }
        }
        if !handled {
            let import_stmt = format!("import {{ {} }} from 'react';", missing_react_hooks.join(", "));
            result = insert_import_after_use_client(&result, &import_stmt);
        }
    }

    // 2. Next.js App Router navigation hooks
    let next_hooks = ["usePathname", "useRouter", "useParams", "useSearchParams"];
    let mut missing_next_hooks: Vec<&str> = Vec::new();
    for hook in &next_hooks {
        let call_pattern = format!(r"(?m)(?:^|[^a-zA-Z0-9_.$]){}\s*(?:<[^>]*>)?\s*\(", hook);
        let is_called = regex::Regex::new(&call_pattern)
            .map(|re| re.is_match(&code_body))
            .unwrap_or(false);
        if !is_called {
            continue;
        }
        let import_pattern = format!(
            r#"(?s)import\s+[^;]*?\b{}\b[^;]*?from\s*['"]next/(?:navigation|router)['"]"#,
            hook
        );
        let decl_pattern = format!(r"\b(?:function|const|let|var)\s+{}\b", hook);
        let already_present = regex::Regex::new(&import_pattern)
            .map(|re| re.is_match(&result))
            .unwrap_or(false)
            || regex::Regex::new(&decl_pattern)
                .map(|re| re.is_match(&result))
                .unwrap_or(false);
        if !already_present {
            missing_next_hooks.push(hook);
        }
    }

    if !missing_next_hooks.is_empty() {
        let named_next_re = regex::Regex::new(
            r#"(?m)^(\s*import\s+\{)([^}]*)(\}\s*from\s*['"]next/navigation['"]\s*;?)"#,
        )
        .ok();
        let mut handled = false;
        if let Some(ref re) = named_next_re {
            if let Some(cap) = re.captures(&result) {
                let prefix = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let existing = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim();
                let suffix = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                let combined = if existing.is_empty() {
                    format!(" {} ", missing_next_hooks.join(", "))
                } else {
                    format!(" {}, {} ", existing.trim_end_matches(','), missing_next_hooks.join(", "))
                };
                let replacement = format!("{}{}{}", prefix, combined, suffix);
                result = re.replace(&result, replacement.as_str()).to_string();
                handled = true;
            }
        }
        if !handled {
            let import_stmt = format!("import {{ {} }} from 'next/navigation';", missing_next_hooks.join(", "));
            result = insert_import_after_use_client(&result, &import_stmt);
        }
    }

    result
}

/// Traverses all source files in `src/` and runs AST import sanitization
/// (removes invalid React navigation imports, verifies @/ alias imports, hoists 'use client').
pub fn apply_ast_sanity_pass_to_project(proj_dir: &std::path::Path) {
    let src_dir = proj_dir.join("src");
    if !src_dir.exists() {
        return;
    }

    let has_tailwind = proj_dir.join("tailwind.config.js").exists() || proj_dir.join("tailwind.config.ts").exists();

    fn walk_dir(dir: &std::path::Path, proj_dir: &std::path::Path, has_tailwind: bool) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk_dir(&path, proj_dir, has_tailwind);
                } else if path.is_file() {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "tsx" || ext == "ts" || ext == "jsx" || ext == "js" {
                        if let Ok(rel) = path.strip_prefix(proj_dir) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let sanitized = post_process_generated_file(&rel_str, &content, &serde_json::json!({}), has_tailwind);
                                if sanitized.replace("\r\n", "\n").trim() != content.replace("\r\n", "\n").trim() {
                                    let _ = std::fs::write(&path, &sanitized);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    walk_dir(&src_dir, proj_dir, has_tailwind);
}

fn is_action_verb_identifier(name: &str) -> bool {
    let prefixes = [
        "get", "fetch", "load", "set", "update", "handle", "create", 
        "delete", "send", "sync", "check", "refresh", "save", "query", 
        "refetch", "mutate", "post", "patch", "put"
    ];
    for p in prefixes {
        if name.starts_with(p) && name.len() > p.len() {
            let next_char = name.chars().nth(p.len()).unwrap_or(' ');
            if next_char.is_ascii_uppercase() || next_char == '_' {
                return true;
            }
        }
    }
    false
}

/// Automatically sanitizes React `useEffect` dependency arrays.
/// Dynamically extracts all identifiers destructured from any `use*()` hook or declared locally,
/// and strips any identifier that is invoked as a function inside the effect body (or matches router/action verbs).
/// Any data, props, or state variables (regardless of naming) are 100% PRESERVED.
pub fn sanitize_use_effect_deps(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut remaining = content;

    // 1. Universally collect all identifiers destructured from any custom or standard hook:
    // e.g. `const { foo, bar, baz } = useAnyHook(...)`
    let mut hook_destructured_ids: Vec<String> = Vec::new();
    if let Ok(hook_re) = regex::Regex::new(r"(?m)const\s*\{([^}]+)\}\s*=\s*use\w+\s*\(") {
        for cap in hook_re.captures_iter(content) {
            if let Some(inner) = cap.get(1) {
                for item in inner.as_str().split(',') {
                    let id = item.trim().split(':').next().unwrap_or("").trim();
                    if !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        hook_destructured_ids.push(id.to_string());
                    }
                }
            }
        }
    }

    while let Some(pos) = remaining.find("useEffect(") {
        result.push_str(&remaining[..pos]);
        let after_use_effect = &remaining[pos..];

        let mut matched = false;
        if let Some(open_body) = after_use_effect.find('{') {
            let mut brace_depth = 0;
            let mut body_end = None;
            for (idx, ch) in after_use_effect[open_body..].char_indices() {
                if ch == '{' {
                    brace_depth += 1;
                } else if ch == '}' {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        body_end = Some(open_body + idx);
                        break;
                    }
                }
            }

            if let Some(end_brace) = body_end {
                let body_slice = &after_use_effect[open_body + 1..end_brace];
                let after_body = &after_use_effect[end_brace + 1..];
                let trimmed_after_body = after_body.trim_start();
                let ws1_len = after_body.len() - trimmed_after_body.len();

                if trimmed_after_body.starts_with(',') {
                    let comma_slice = &trimmed_after_body[1..];
                    let after_comma = comma_slice.trim_start();
                    let ws2_len = comma_slice.len() - after_comma.len();

                    if after_comma.starts_with('[') {
                        if let Some(close_bracket) = after_comma.find(']') {
                            let deps_str = &after_comma[1..close_bracket];
                            let deps: Vec<&str> = deps_str
                                .split(',')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .collect();

                            let mut filtered_deps = Vec::new();
                            for dep in deps {
                                let is_called = body_slice.contains(&format!("{}(", dep))
                                    || body_slice.contains(&format!("{}.", dep))
                                    || body_slice.contains(&format!("{}.then(", dep))
                                    || body_slice.contains(&format!("{}.catch(", dep));
                                let is_from_hook = hook_destructured_ids.iter().any(|h| h == dep);
                                let is_action = is_action_verb_identifier(dep);
                                let is_router = dep == "router" || dep == "navigate";

                                // If an identifier is called as a function inside the effect body AND
                                // either originates from a hook, matches action verbs, or is router/navigate:
                                // It is an unmemoized action that triggers infinite re-render loops!
                                if (is_called && (is_from_hook || is_action)) || is_router {
                                    continue;
                                }
                                filtered_deps.push(dep);
                            }

                            let new_deps_str = filtered_deps.join(", ");
                            let prefix = &after_use_effect[..end_brace + 1];
                            result.push_str(prefix);
                            result.push_str(&format!(", [{}]", new_deps_str));

                            // Mathematical exact offset to past the ']' bracket:
                            // end_brace + 1 ('}') + ws1_len + 1 (',') + ws2_len + close_bracket + 1 (']')
                            let offset = end_brace + 1 + ws1_len + 1 + ws2_len + close_bracket + 1;
                            remaining = &after_use_effect[offset..];
                            matched = true;
                        }
                    }
                }
            }
        }

        if !matched {
            result.push_str("useEffect(");
            remaining = &after_use_effect[10..];
        }
    }

    result.push_str(remaining);
    result
}

/// Generically memoizes action functions declared in custom React hooks.
/// Finds any `return { foo, bar, ... }` in a custom hook, and if the returned identifier
/// is declared as an unmemoized arrow function, wraps it with `useCallback(async (...) => { ... }, [])`.
/// Operates on ANY custom hook, ANY function name, and ANY domain.
pub fn sanitize_custom_hook_functions(content: &str) -> String {
    if !content.contains("const use") && !content.contains("function use") && !content.contains("export function use") {
        return content.to_string();
    }

    let return_re = match regex::Regex::new(r"(?s)return\s*\{([^}]+)\}") {
        Ok(r) => r,
        Err(_) => return content.to_string(),
    };

    let mut returned_ids: Vec<String> = Vec::new();
    for cap in return_re.captures_iter(content) {
        if let Some(inner) = cap.get(1) {
            for item in inner.as_str().split(',') {
                let id = item.trim().split(':').next().unwrap_or("").trim();
                if !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    returned_ids.push(id.to_string());
                }
            }
        }
    }

    if returned_ids.is_empty() {
        return content.to_string();
    }

    let mut modified = content.to_string();
    let mut needs_use_callback_import = false;

    for id in &returned_ids {
        let fn_decl_pattern = format!(r"(?m)^(\s*)const\s+{}\s*=\s*(async\s*)?\(([^)]*)\)\s*=>\s*\{{", id);
        if let Ok(fn_re) = regex::Regex::new(&fn_decl_pattern) {
            if let Some(mat) = fn_re.find(&modified) {
                let open_pos = mat.end() - 1;
                let mut brace_depth = 0;
                let mut close_pos = None;
                for (char_idx, ch) in modified[open_pos..].char_indices() {
                    if ch == '{' {
                        brace_depth += 1;
                    } else if ch == '}' {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            close_pos = Some(open_pos + char_idx);
                            break;
                        }
                    }
                }

                if let Some(end_brace) = close_pos {
                    let after_brace = &modified[end_brace + 1..];
                    let semi_len = if after_brace.trim_start().starts_with(';') {
                        after_brace.find(';').map(|p| p + 1).unwrap_or(0)
                    } else {
                        0
                    };
                    let full_fn_end = end_brace + 1 + semi_len;
                    let full_fn_text = &modified[mat.start()..full_fn_end];

                    if !full_fn_text.contains("useCallback(") {
                        if let Some(cap) = fn_re.captures(&modified[mat.start()..mat.end()]) {
                            let indent = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                            let async_kw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                            let params = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                            let body = &modified[open_pos..=end_brace];

                            let memoized = format!(
                                "{}const {} = useCallback({}({}) => {} , []);",
                                indent, id, async_kw, params, body
                            );

                            modified.replace_range(mat.start()..full_fn_end, &memoized);
                            needs_use_callback_import = true;
                        }
                    }
                }
            }
        }
    }

    if needs_use_callback_import {
        modified = ensure_react_and_next_hook_imports(&modified);
    }

    modified
}

/// Automatically detects and declares missing boolean toggle states (such as `is[A-Z]\w*Open`)
/// that are referenced in JSX onClick or ternary conditions but were forgotten by the LLM in useState.
pub fn ensure_undeclared_toggle_states(content: &str) -> String {
    if !content.contains("Open") {
        return content.to_string();
    }

    let toggle_re = match regex::Regex::new(r"\b(is[A-Z][a-zA-Z0-9_]*Open)\b") {
        Ok(r) => r,
        Err(_) => return content.to_string(),
    };

    let mut missing_states: Vec<String> = Vec::new();
    for cap in toggle_re.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let var_name = m.as_str();
            let is_declared = content.contains(&format!("const [{},", var_name))
                || content.contains(&format!("const [ {},", var_name))
                || content.contains(&format!("let {}", var_name))
                || content.contains(&format!("const {} =", var_name))
                || content.contains(&format!("{}: boolean", var_name))
                || content.contains(&format!("{}:", var_name))
                || content.contains(&format!("props.{}", var_name))
                || content.contains(&format!("{{ {}", var_name));

            if !is_declared && !missing_states.contains(&var_name.to_string()) {
                missing_states.push(var_name.to_string());
            }
        }
    }

    if missing_states.is_empty() {
        return content.to_string();
    }

    let mut result = content.to_string();

    if !result.contains("useState") {
        result = ensure_react_and_next_hook_imports(&result);
    }

    for var_name in missing_states {
        let setter_name = format!("set{}{}", var_name[..1].to_uppercase(), &var_name[1..]);
        let declaration = format!("  const [{}, {}] = useState(false);\n", var_name, setter_name);

        let mut inserted = false;
        if let Some(pos) = result.find(") => {") {
            let insert_idx = pos + 6;
            result.insert_str(insert_idx, &format!("\n{}", declaration));
            inserted = true;
        } else if let Some(pos) = result.find(") {") {
            let insert_idx = pos + 3;
            result.insert_str(insert_idx, &format!("\n{}", declaration));
            inserted = true;
        }

        if !inserted {
            if let Some(pos) = result.find('{') {
                result.insert_str(pos + 1, &format!("\n{}", declaration));
            }
        }
    }

    result
}

/// Removes model field declarations whose base type is neither a Prisma built-in scalar,
/// `Unsupported(...)`, nor any declared `model`, `enum`, composite `type`, or `view` in `schema`.
/// Used as a deterministic fallback in Prisma healing if an undefined type persists after AI repair.
pub fn prune_unresolved_prisma_relations(schema: &str) -> String {
    let mut known_types: std::collections::HashSet<String> = [
        "String", "Boolean", "Int", "BigInt", "Float", "Decimal", "DateTime", "Json", "Bytes",
        "Unsupported",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();

    if let Ok(decl_re) = regex::Regex::new(r"(?m)^(?:model|enum|type|view)\s+(\w+)\s*\{") {
        for cap in decl_re.captures_iter(schema) {
            if let Some(m) = cap.get(1) {
                known_types.insert(m.as_str().to_string());
            }
        }
    }

    let model_re = match regex::Regex::new(r"(?ms)^model\s+(\w+)\s*\{([^}]+)\}") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut replacements: Vec<(String, Vec<String>)> = Vec::new();
    for cap in model_re.captures_iter(schema) {
        let Some(m_name) = cap.get(1).map(|m| m.as_str().to_string()) else {
            continue;
        };
        let Some(m_body) = cap.get(2).map(|m| m.as_str()) else {
            continue;
        };

        let mut kept_lines = Vec::new();
        let mut changed = false;
        let mut in_block_comment = false;

        for line in m_body.lines() {
            let trimmed = line.trim();
            if in_block_comment {
                kept_lines.push(line.to_string());
                if trimmed.contains("*/") {
                    in_block_comment = false;
                }
                continue;
            }
            if trimmed.starts_with("/*") {
                kept_lines.push(line.to_string());
                if !trimmed.contains("*/") {
                    in_block_comment = true;
                }
                continue;
            }
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("@@")
                || trimmed.starts_with('@')
            {
                kept_lines.push(line.to_string());
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let raw_type = parts[1];
                let clean_base = raw_type
                    .trim_end_matches(&['?', '[', ']'][..])
                    .replace(['?', '[', ']'], "");
                let base_ident = clean_base.split('(').next().unwrap_or(&clean_base).trim();
                if !base_ident.is_empty() && !known_types.contains(base_ident) {
                    changed = true;
                    continue;
                }
            }

            kept_lines.push(line.to_string());
        }

        if changed {
            replacements.push((m_name, kept_lines));
        }
    }

    if replacements.is_empty() {
        return schema.to_string();
    }

    let mut result = schema.to_string();
    for (m_name, lines) in replacements {
        let pattern = format!(r"(?ms)(^model\s+{}\s*\{{)([\s\S]*?)(\n*\}})", m_name);
        if let Ok(block_re) = regex::Regex::new(&pattern) {
            let body_str = lines.join("\n");
            result = block_re
                .replace(&result, |caps: &regex::Captures| {
                    format!("{}{}\n{}", &caps[1], body_str.trim_end(), caps[3].trim_start())
                })
                .to_string();
        }
    }

    result
}

/// Ensures `generator client` and `datasource db` blocks exist in `schema.prisma`,
/// and ensures an existing `datasource` block declares a `url`.
pub fn ensure_prisma_generator_and_datasource(schema: &str) -> String {
    let mut out = schema.to_string();
    let has_generator = regex::Regex::new(r"(?m)^\s*generator\s+\w+\s*\{")
        .map(|re| re.is_match(&out))
        .unwrap_or(false);
    let has_datasource = regex::Regex::new(r"(?m)^\s*datasource\s+\w+\s*\{")
        .map(|re| re.is_match(&out))
        .unwrap_or(false);

    if has_datasource {
        if let Ok(ds_re) = regex::Regex::new(r"(?ms)(^\s*datasource\s+\w+\s*\{)([^}]*)(\})") {
            out = ds_re
                .replace(&out, |caps: &regex::Captures| {
                    let body = &caps[2];
                    if !body.contains("url") {
                        format!("{}\n  url      = env(\"DATABASE_URL\"){}{}", &caps[1], body, &caps[3])
                    } else {
                        caps[0].to_string()
                    }
                })
                .to_string();
        }
    }

    let mut prefix = String::new();
    if !has_generator {
        prefix.push_str("generator client {\n  provider = \"prisma-client-js\"\n}\n\n");
    }
    if !has_datasource {
        prefix.push_str("datasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}\n\n");
    }

    if !prefix.is_empty() {
        format!("{}{}", prefix, out.trim_start_matches('\n'))
    } else {
        out
    }
}

fn is_valid_prisma_enum_member(tok: &str) -> bool {
    let mut chars = tok.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(tok, "true" | "false" | "null")
}

/// Strips illegal `EnumName.` prefixes from Prisma `@default(EnumName.MEMBER)` attributes
/// (converting them to `@default(MEMBER)`), and ensures any bare identifier used as `@default(MEMBER)`
/// on a field of declared enum type `E` is declared inside `enum E { ... }`.
pub fn sanitize_prisma_enum_defaults(schema: &str) -> String {
    let enum_re = match regex::Regex::new(r"(?ms)^[ \t]*enum[ \t]+(\w+)[ \t]*\{([^}]*)\}") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut enum_members: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for cap in enum_re.captures_iter(schema) {
        let Some(e_name) = cap.get(1).map(|m| m.as_str().to_string()) else {
            continue;
        };
        let Some(e_body) = cap.get(2).map(|m| m.as_str()) else {
            continue;
        };
        let mut members = Vec::new();
        for line in e_body.lines() {
            let clean = line.split("//").next().unwrap_or("").trim();
            if clean.is_empty() || clean.starts_with('@') || clean.starts_with("/*") {
                continue;
            }
            if let Some(tok) = clean.split_whitespace().next() {
                if is_valid_prisma_enum_member(tok) {
                    members.push(tok.to_string());
                }
            }
        }
        enum_members.insert(e_name, members);
    }

    if enum_members.is_empty() && !schema.contains("@default(") {
        return schema.to_string();
    }

    let model_re = match regex::Regex::new(r"(?ms)^[ \t]*model[ \t]+(\w+)[ \t]*\{([^}]*)\}") {
        Ok(r) => r,
        Err(_) => return schema.to_string(),
    };

    let mut missing_enum_members: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    let mut model_replacements: Vec<(String, String)> = Vec::new();

    for cap in model_re.captures_iter(schema) {
        let Some(m_name) = cap.get(1).map(|m| m.as_str().to_string()) else {
            continue;
        };
        let Some(m_body) = cap.get(2).map(|m| m.as_str()) else {
            continue;
        };
        let mut updated_lines = Vec::new();
        let mut model_changed = false;

        for line in m_body.lines() {
            let mut cur_line = line.to_string();
            let trimmed = cur_line.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("@@")
                && !trimmed.starts_with("/*")
                && trimmed.contains("@default(")
            {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let clean_type_opt = if parts.len() >= 2 {
                    Some(parts[1].replace(['?', '[', ']'], ""))
                } else {
                    None
                };
                if let Some(clean_type) = clean_type_opt {
                    if let Some(def_idx) = cur_line.find("@default(") {
                        let open_idx = def_idx + "@default".len();
                        if let Some(close_idx) = find_matching_paren_in_attr(&cur_line, open_idx) {
                            let arg = cur_line[open_idx + 1..close_idx].trim().to_string();
                            if !arg.contains('(') && !arg.starts_with('[') {
                                let mut normalized_member: Option<String> = None;

                                if let Some((prefix, member)) = arg.split_once('.') {
                                    let p_trim = prefix.trim().to_string();
                                    let m_trim = member.trim().to_string();
                                    if (p_trim == clean_type || enum_members.contains_key(&p_trim))
                                        && is_valid_prisma_enum_member(&m_trim)
                                    {
                                        let mut new_line = cur_line[..open_idx + 1].to_string();
                                        new_line.push_str(&m_trim);
                                        new_line.push_str(&cur_line[close_idx..]);
                                        cur_line = new_line;
                                        model_changed = true;
                                        normalized_member = Some(m_trim);
                                    }
                                } else if enum_members.contains_key(&clean_type) {
                                    let unquoted = arg.trim_matches(|c| c == '"' || c == '\'').to_string();
                                    if is_valid_prisma_enum_member(&unquoted) {
                                        if unquoted != arg {
                                            let mut new_line = cur_line[..open_idx + 1].to_string();
                                            new_line.push_str(&unquoted);
                                            new_line.push_str(&cur_line[close_idx..]);
                                            cur_line = new_line;
                                            model_changed = true;
                                        }
                                        normalized_member = Some(unquoted);
                                    }
                                }

                                if let Some(mem) = normalized_member {
                                    if let Some(existing) = enum_members.get(&clean_type) {
                                        if !existing.contains(&mem) {
                                            let add_list = missing_enum_members.entry(clean_type).or_default();
                                            if !add_list.contains(&mem) {
                                                add_list.push(mem);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            updated_lines.push(cur_line);
        }

        if model_changed {
            model_replacements.push((m_name, updated_lines.join("\n")));
        }
    }

    let mut result = schema.to_string();
    for (m_name, body_str) in model_replacements {
        let pattern = format!(r"(?ms)(^[ \t]*model[ \t]+{}\s*\{{)([\s\S]*?)(\n*\}})", regex::escape(&m_name));
        if let Ok(block_re) = regex::Regex::new(&pattern) {
            result = block_re
                .replace(&result, |caps: &regex::Captures| {
                    format!("{}{}\n{}", &caps[1], body_str.trim_end(), caps[3].trim_start())
                })
                .to_string();
        }
    }

    for (e_name, new_members) in missing_enum_members {
        if new_members.is_empty() {
            continue;
        }
        let pattern = format!(r"(?ms)(^[ \t]*enum[ \t]+{}\s*\{{)([\s\S]*?)(\n*\}})", regex::escape(&e_name));
        if let Ok(block_re) = regex::Regex::new(&pattern) {
            result = block_re
                .replace(&result, |caps: &regex::Captures| {
                    let mut body = caps[2].trim_end().to_string();
                    for mem in &new_members {
                        body.push_str(&format!("\n  {}", mem));
                    }
                    format!("{}{}\n{}", &caps[1], body, caps[3].trim_start())
                })
                .to_string();
        }
    }

    result
}

/// Ensures `model User` exists when the schema or project uses NextAuth/user authentication.
pub fn ensure_prisma_user_model_if_missing(schema: &str) -> String {
    let has_user = regex::Regex::new(r"(?m)^[ \t]*model[ \t]+User[ \t]*\{")
        .map(|re| re.is_match(schema))
        .unwrap_or(false);
    if has_user {
        return schema.to_string();
    }

    let mut out = schema.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(
        "model User {\n  id             String    @id @default(cuid())\n  name           String?\n  email          String    @unique\n  emailVerified  DateTime?\n  image          String?\n  hashedPassword String?\n  role           String    @default(\"USER\")\n  createdAt      DateTime  @default(now())\n  updatedAt      DateTime  @updatedAt\n}\n",
    );
    out
}

/// Rewrites incorrect imports pointing at Prisma schema files (e.g. `@/prisma/schema`,
/// `~/prisma/schema`, `src/prisma/schema`, `@/prisma/client`, `../prisma/schema`) to
/// `@/lib/prisma` (when importing `prisma` runtime singleton) or `@prisma/client`
/// (automatically promoting non-type imports to `import type` on client/hook files).
pub fn normalize_prisma_schema_imports(content: &str, is_client_or_hook: bool) -> String {
    let raw_lines: Vec<&str> = content.lines().collect();
    let mut out_lines: Vec<String> = Vec::with_capacity(raw_lines.len());
    let mut changed = false;
    let mut idx = 0usize;

    while idx < raw_lines.len() {
        let trimmed = raw_lines[idx].trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("import{") {
            let mut end_idx = idx;
            let mut stmt = raw_lines[idx].to_string();
            let is_side_effect = trimmed.starts_with("import '") || trimmed.starts_with("import \"");
            if !is_side_effect {
                while !stmt.contains(" from ")
                    && !stmt.contains("from'")
                    && !stmt.contains("from\"")
                    && !stmt.trim_end().ends_with(';')
                    && end_idx + 1 < raw_lines.len()
                {
                    end_idx += 1;
                    stmt.push('\n');
                    stmt.push_str(raw_lines[end_idx]);
                }
            }

            let flat = stmt.replace('\n', " ");
            let flat_trim = flat.trim();
            if let Some(spec) = extract_import_specifier(flat_trim) {
                let s_low = spec.trim().replace('\\', "/").to_lowercase();
                let is_schema_path = s_low == "@/prisma/schema"
                    || s_low == "~/prisma/schema"
                    || s_low == "src/prisma/schema"
                    || s_low == "prisma/schema"
                    || s_low == "@/prisma/client"
                    || s_low == "~/prisma/client"
                    || s_low == "src/prisma/client"
                    || s_low == "prisma/client"
                    || s_low.ends_with("/prisma/schema")
                    || s_low.ends_with("/prisma/client");

                if is_schema_path {
                    let imports_prisma_singleton = regex::Regex::new(r"(?m)(?:import\s+prisma\b|\{[^}]*\bprisma\b)")
                        .map(|re| re.is_match(flat_trim))
                        .unwrap_or(false);
                    let target_mod = if imports_prisma_singleton {
                        "@/lib/prisma"
                    } else {
                        "@prisma/client"
                    };
                    let mut rewritten_stmt = stmt
                        .replace(&format!("'{}'", spec), &format!("'{}'", target_mod))
                        .replace(&format!("\"{}\"", spec), &format!("\"{}\"", target_mod));

                    let is_already_top_type = flat_trim.starts_with("import type ") || flat_trim.starts_with("import type{");
                    let is_inline_all_types = if !is_already_top_type && flat_trim.contains('{') && flat_trim.contains('}') {
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
                    if target_mod == "@prisma/client"
                        && is_client_or_hook
                        && !is_already_top_type
                        && !is_inline_all_types
                    {
                        // Remove any partial inline `type ` modifiers before promoting statement to `import type`
                        if let Ok(inline_type_re) = regex::Regex::new(r"([\{,]\s*)type\s+") {
                            rewritten_stmt = inline_type_re.replace_all(&rewritten_stmt, "$1").to_string();
                        }
                        if let Some(pos) = rewritten_stmt.find("import") {
                            let after = &rewritten_stmt[pos + 6..];
                            let new_after = if after.starts_with('{') {
                                format!(" type {}", after)
                            } else {
                                format!(" type{}", after)
                            };
                            rewritten_stmt = format!("{}{}{}", &rewritten_stmt[..pos], "import", new_after);
                        }
                    }

                    if rewritten_stmt != stmt {
                        changed = true;
                        for line_piece in rewritten_stmt.lines() {
                            out_lines.push(line_piece.to_string());
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
            continue;
        }
        out_lines.push(raw_lines[idx].to_string());
        idx += 1;
    }

    if !changed {
        return content.to_string();
    }
    let mut joined = out_lines.join("\n");
    if content.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// Auto-injects `import bcrypt from 'bcryptjs';` when `bcrypt.*` methods are invoked
/// without an existing `bcrypt` / `bcryptjs` import or local binding.
pub fn ensure_bcrypt_imports(content: &str) -> String {
    let has_bcrypt_call = regex::Regex::new(
        r"(?m)\bbcrypt\s*\.\s*(?:hash|compare|genSalt|hashSync|compareSync|getRounds)\s*\(",
    )
    .map(|re| re.is_match(content))
    .unwrap_or(false);

    if !has_bcrypt_call {
        return content.to_string();
    }

    let has_bcrypt_import = content.contains("from 'bcryptjs'")
        || content.contains("from \"bcryptjs\"")
        || content.contains("from 'bcrypt'")
        || content.contains("from \"bcrypt\"")
        || content.contains("require('bcryptjs')")
        || content.contains("require(\"bcryptjs\")")
        || content.contains("require('bcrypt')")
        || content.contains("require(\"bcrypt\")");

    if has_bcrypt_import {
        return content.to_string();
    }

    let has_decl = regex::Regex::new(r"(?m)\b(?:const|let|var|function|class|import)\s+bcrypt\b")
        .map(|re| re.is_match(content))
        .unwrap_or(false);
    if has_decl {
        return content.to_string();
    }

    insert_import_after_use_client(content, "import bcrypt from 'bcryptjs';")
}

/// Promotes local unexported `interface Foo` / `type Foo = ...` declarations inside custom hook
/// files (`src/hooks/*.ts`) to `export interface Foo` / `export type Foo = ...` whenever `Foo`
/// appears inside an exported declaration or signature, so consumers importing `{ Foo }` succeed.
pub fn ensure_hook_type_exports(content: &str, rel_path: &str) -> String {
    let norm = rel_path.replace('\\', "/").to_lowercase();
    let is_hook = !is_app_router_route_file(rel_path)
        && !norm.contains("/api/")
        && (norm.contains("/hooks/") || norm.starts_with("hooks/"));
    if !is_hook || (!content.contains("interface ") && !content.contains("type ")) {
        return content.to_string();
    }

    let exported_section: String = content
        .lines()
        .filter(|l| l.contains("export "))
        .collect::<Vec<&str>>()
        .join("\n");

    let Ok(decl_re) = regex::Regex::new(r"(?m)^([ \t]*)(interface|type)\s+([A-Z][A-Za-z0-9_]*)\b") else {
        return content.to_string();
    };

    let mut to_export: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cap in decl_re.captures_iter(content) {
        let indent = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if !indent.is_empty() {
            continue;
        }
        let Some(name_m) = cap.get(3) else { continue };
        let sym = name_m.as_str();
        // Promote if used inside an export line OR if there is no `export interface <sym>` / `export type <sym>` anywhere yet
        let already_exported = content.contains(&format!("export interface {}", sym))
            || content.contains(&format!("export type {}", sym));
        if already_exported {
            continue;
        }
        let word_re = regex::Regex::new(&format!(r"\b{}\b", regex::escape(sym))).ok();
        let used_in_export = word_re
            .as_ref()
            .map(|r| r.is_match(&exported_section))
            .unwrap_or(false);
        let match_count = word_re
            .as_ref()
            .map(|r| r.find_iter(content).count())
            .unwrap_or(0);
        if used_in_export || match_count >= 2 {
            to_export.insert(sym.to_string());
        }
    }

    if to_export.is_empty() {
        return content.to_string();
    }

    let mut out_lines: Vec<String> = Vec::new();
    for line in content.lines() {
        let mut next_line = line.to_string();
        if let Some(cap) = decl_re.captures(line) {
            let indent = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let kw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let sym = cap.get(3).map(|m| m.as_str()).unwrap_or("");
            if indent.is_empty() && to_export.contains(sym) {
                next_line = format!("export {} {}", kw, &line[kw.len()..].trim_start());
            }
        }
        out_lines.push(next_line);
    }

    let mut joined = out_lines.join("\n");
    if content.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// Ensures Next.js App Router `layout.tsx` / `layout.jsx` accepts `{ children }: { children: React.ReactNode }`
/// and actually renders `{children}` in its JSX tree. When `allow_even_if_table` is false, a
/// dashboard-stuffed RootLayout with `<table>` is left intact so generation validation can reject & retry it.
pub fn sanitize_root_layout_missing_children(content: &str, rel_path: &str, allow_even_if_table: bool) -> String {
    let norm = rel_path.replace('\\', "/").to_lowercase();
    let is_layout = norm == "layout.tsx"
        || norm == "layout.jsx"
        || norm.ends_with("/layout.tsx")
        || norm.ends_with("/layout.jsx");
    if !is_layout {
        return content.to_string();
    }

    let already_has_children = if let Ok(re) = regex::Regex::new(r"\{\s*children\s*\}") {
        re.find_iter(content).any(|m| {
            let next_ch = content[m.end()..].chars().find(|c| !c.is_whitespace());
            !matches!(next_ch, Some(':') | Some(',') | Some('=') | Some(')'))
        })
    } else {
        content.contains("{children}")
    };
    if already_has_children {
        return content.to_string();
    }
    if !allow_even_if_table && norm.ends_with("/app/layout.tsx") && content.to_lowercase().contains("<table") {
        return content.to_string();
    }
    if !allow_even_if_table && norm.ends_with("/app/layout.jsx") && content.to_lowercase().contains("<table") {
        return content.to_string();
    }

    let mut out = content.to_string();
    if let Ok(empty_fn_re) = regex::Regex::new(r"(export\s+default\s+(?:async\s+)?function\s+\w+\s*)\(\s*\)") {
        out = empty_fn_re
            .replace(&out, "${1}({ children }: { children: React.ReactNode })")
            .to_string();
    }

    if let Some(pos) = out.rfind("</main>") {
        out.insert_str(pos, "{children}\n        ");
        return out;
    }
    if let Some(pos) = out.rfind("</SessionProvider>") {
        out.insert_str(pos, "{children}\n        ");
        return out;
    }
    if let Some(pos) = out.rfind("</body>") {
        out.insert_str(pos, "<main className=\"flex-1 flex flex-col w-full\">{children}</main>\n      ");
        return out;
    }
    if let Some(pos) = out.rfind("</div>") {
        out.insert_str(pos, "{children}\n      ");
        return out;
    }
    out
}
