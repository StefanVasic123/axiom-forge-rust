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
    if content.contains("data-axiom-component=") {
        return content.to_string();
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

    let mut first_match_idx = None;
    let mut matched_tag_len = 0;

    let chars: Vec<char> = content.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '<' && i + 1 < chars.len() {
            // Preskoči ako je ispred (nakon preskakanja razmaka) alfanumerički karakter, tačka ili donja crta (indikacija TypeScript generika ili poređenja)
            let mut is_generic_or_compare = false;
            if i > 0 {
                let mut prev_idx = i - 1;
                while prev_idx > 0 && chars[prev_idx].is_ascii_whitespace() {
                    prev_idx -= 1;
                }
                let prev_c = chars[prev_idx];
                if prev_c.is_ascii_alphanumeric() || prev_c == '.' || prev_c == '_' {
                    let mut word_start = prev_idx;
                    while word_start > 0 {
                        let c = chars[word_start - 1];
                        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                            word_start -= 1;
                        } else {
                            break;
                        }
                    }
                    let word: String = chars[word_start..=prev_idx].iter().collect();
                    let keywords = ["return", "default", "yield", "await", "as", "typeof", "void", "delete"];
                    if !keywords.contains(&word.as_str()) {
                        is_generic_or_compare = true;
                    }
                }
            }
            if is_generic_or_compare {
                continue;
            }
            let next_c = chars[i + 1];
            if next_c.is_ascii_alphabetic() || next_c == '_' {
                let mut tag_name = String::new();
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '-' || chars[j] == '_' || chars[j] == '.') {
                    tag_name.push(chars[j]);
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
                    first_match_idx = Some(i);
                    matched_tag_len = tag_name.len() + 1;
                    break;
                }
            }
        }
    }

    if let Some(idx) = first_match_idx {
        let mut new_content = String::new();
        new_content.push_str(&content[..idx + matched_tag_len]);
        new_content.push_str(&format!(
            " data-axiom-component=\"{}\" data-axiom-file=\"{}\"",
            component_name, relative_path.replace('\\', "/")
        ));
        new_content.push_str(&content[idx + matched_tag_len..]);
        new_content
    } else {
        content.to_string()
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
}

pub fn get_axiom_inspector_script() -> &'static str {
    "      <script dangerouslySetInnerHTML={{ __html: `\n        if (!window.__axiom_inspect_injected) {\n          window.__axiom_inspect_injected = true;\n          let isInspectActive = false;\n          window.addEventListener('message', (e) => {\n            if (e.data && e.data.channel === 'axiom-toggle-inspect') {\n              isInspectActive = !!e.data.enabled;\n              if (!isInspectActive && highlightedElement) {\n                highlightedElement.classList.remove('axiom-inspect-highlight');\n                highlightedElement = null;\n              }\n            }\n          });\n          const styleEl = document.createElement('style');\n          styleEl.textContent = \\`\n            .axiom-inspect-highlight {\n              outline: 2px solid #6366f1 !important;\n              outline-offset: -2px !important;\n              cursor: crosshair !important;\n              background-color: rgba(99, 102, 241, 0.12) !important;\n              transition: all 0.1s ease;\n            }\n          \\`;\n          document.head.appendChild(styleEl);\n          let highlightedElement = null;\n          function findAxiomTarget(el) {\n            if (!el || el === document.body || el === document.documentElement) return null;\n            let cur = el;\n            while (cur && cur !== document.body && cur !== document.documentElement) {\n              if (cur.dataset && (cur.dataset.axiomFile || cur.dataset.axiomComponent)) {\n                return cur;\n              }\n              cur = cur.parentElement;\n            }\n            const validTags = ['h1','h2','h3','h4','h5','h6','p','button','a','input','textarea','select','header','footer','nav','section','article','aside','li'];\n            cur = el;\n            while (cur && cur !== document.body && cur !== document.documentElement) {\n              const tag = cur.tagName.toLowerCase();\n              if (validTags.includes(tag) || (cur.innerText && cur.innerText.trim().length > 0 && cur.children.length === 0)) {\n                return cur;\n              }\n              cur = cur.parentElement;\n            }\n            return el !== document.body && el !== document.documentElement ? el : null;\n          }\n          document.addEventListener('mouseover', (e) => {\n            if (!isInspectActive && !e.altKey) return;\n            const target = findAxiomTarget(e.target);\n            if (highlightedElement && highlightedElement !== target) {\n              highlightedElement.classList.remove('axiom-inspect-highlight');\n            }\n            if (target) {\n              target.classList.add('axiom-inspect-highlight');\n              highlightedElement = target;\n            }\n          }, true);\n          document.addEventListener('mouseout', (e) => {\n            if (highlightedElement) {\n              highlightedElement.classList.remove('axiom-inspect-highlight');\n              highlightedElement = null;\n            }\n          }, true);\n          document.addEventListener('click', (e) => {\n            if (!isInspectActive && !e.altKey) return;\n            const target = findAxiomTarget(e.target);\n            if (target) {\n              e.preventDefault();\n              e.stopPropagation();\n              const payload = {\n                file: target.dataset?.axiomFile || null,\n                component: target.dataset?.axiomComponent || null,\n                tagName: target.tagName.toLowerCase(),\n                text: (target.innerText || target.value || '').trim().substring(0, 80),\n                path: window.location.pathname\n              };\n              window.parent.postMessage({ channel: 'axiom-inspect-click', payload }, '*');\n            }\n          }, true);\n        }\n      ` }} />"
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
                if !content.contains("axiom-toggle-inspect") {
                    let cleaned = if content.contains("axiom-inspect-click") {
                        let start_marker = "<script dangerouslySetInnerHTML={{ __html: `\n        if (!window.__axiom_inspect_injected)";
                        if let Some(s_idx) = content.find(start_marker) {
                            if let Some(end_rel) = content[s_idx..].find("` }} />") {
                                let mut c = content.clone();
                                c.replace_range(s_idx..s_idx + end_rel + 7, "");
                                c
                            } else {
                                content.clone()
                            }
                        } else {
                            content.clone()
                        }
                    } else {
                        content.clone()
                    };

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

pub fn ensure_valid_tsconfig(proj_dir: &std::path::Path) {
    let tsconfig_path = proj_dir.join("tsconfig.json");
    if tsconfig_path.exists() {
        let is_valid = if let Ok(content) = std::fs::read_to_string(&tsconfig_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                val.get("compilerOptions").is_some()
            } else {
                false
            }
        } else {
            false
        };

        if !is_valid {
            let standard_tsconfig = serde_json::json!({
                "compilerOptions": {
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
                        "@/*": ["./src/*"]
                    }
                },
                "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
                "exclude": ["node_modules"]
            });
            let _ = std::fs::write(&tsconfig_path, serde_json::to_string_pretty(&standard_tsconfig).unwrap_or_default());
        }
    }
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

            if let Ok(pretty) = serde_json::to_string_pretty(&pkg_val) {
                return pretty;
            }
        }
    }

    let mut is_target_css = false;
    if has_tailwind && (path_lower.ends_with(".css") || path_lower.ends_with(".scss")) {
        if let Some(target_files) = metamanifest.get("injectTailwindDirectives").and_then(|i| i.get("targetCssFiles")).and_then(|f| f.as_array()) {
            is_target_css = target_files.iter().any(|val| {
                val.as_str().map(|s| path_lower.contains(&s.to_lowercase())).unwrap_or(false)
            });
        } else {
            is_target_css = path_lower.contains("globals.css") || path_lower.contains("styles.css");
        }
    }

    if is_target_css {
        let trimmed = content.trim();
        if !trimmed.contains("@tailwind base") {
            let mut prepended = "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n\n".to_string();
            prepended.push_str(content);
            return prepended;
        }
    }

    let mut processed = if is_html_template_file(&normalized_path) {
        inject_axiom_attrs(content, &normalized_path)
    } else {
        content.to_string()
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

    // Apply learned global healing rules
    let rules_path = get_base_dir().join("axiom-healing-rules.json");
    if rules_path.exists() {
        if let Ok(rules_content) = std::fs::read_to_string(&rules_path) {
            if let Ok(rules_val) = serde_json::from_str::<Value>(&rules_content) {
                if let Some(rules_arr) = rules_val.as_array() {
                    for rule in rules_arr {
                        if let (Some(search), Some(replace)) = (rule.get("search").and_then(|s| s.as_str()), rule.get("replace").and_then(|r| r.as_str())) {
                            if processed.contains(search) {
                                processed = processed.replace(search, replace);
                            }
                        }
                    }
                }
            }
        }
    }

    // Rule for schema.prisma corrections
    if path_lower.ends_with("schema.prisma") {
        // 1. Fix missing url in datasource block
        if processed.contains("datasource db {") {
            if let Some(start_idx) = processed.find("datasource db {") {
                if let Some(close_brace) = processed[start_idx..].find('}') {
                    let datasource_block = &processed[start_idx..start_idx + close_brace];
                    if !datasource_block.contains("url") {
                        let mut fixed_datasource = datasource_block.to_string();
                        if let Some(open_brace_idx) = fixed_datasource.find('{') {
                            fixed_datasource.insert_str(open_brace_idx + 1, "\n  url      = env(\"DATABASE_URL\")");
                            processed = processed.replace(datasource_block, &fixed_datasource);
                        }
                    }
                }
            }
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

        // 4a. Strip @relation with fields/references from array fields (e.g. Project[] @relation(...))
        let array_rel_regex = regex::Regex::new(r"(?m)(\b\w+\[\]\s*)@relation\([^)]*\)").ok();
        if let Some(re) = array_rel_regex {
            processed = re.replace_all(&processed, "$1").to_string();
        }

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
                            let mut paren_count = 0;
                            let mut end_idx = def_idx;
                            let chars: Vec<char> = line.chars().collect();
                            for k in def_idx..chars.len() {
                                if chars[k] == '(' { paren_count += 1; }
                                else if chars[k] == ')' {
                                    paren_count -= 1;
                                    if paren_count == 0 { end_idx = k + 1; break; }
                                }
                            }
                            if end_idx > def_idx {
                                let mut new_l = line[..def_idx].to_string();
                                new_l.push_str(line[end_idx..].trim_start());
                                *line = new_l;
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
                            let mut paren_count = 0;
                            let mut end_idx = rel_idx;
                            let chars: Vec<char> = line.chars().collect();
                            for k in rel_idx..chars.len() {
                                if chars[k] == '(' {
                                    paren_count += 1;
                                } else if chars[k] == ')' {
                                    paren_count -= 1;
                                    if paren_count == 0 {
                                        end_idx = k + 1;
                                        break;
                                    }
                                }
                            }
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
    }

    let is_js_ts_file = path_lower.ends_with(".tsx") || path_lower.ends_with(".jsx") || path_lower.ends_with(".ts") || path_lower.ends_with(".js");
    if is_js_ts_file {
        // Ensure prisma singleton config exports default if it has a named export
        if path_lower.ends_with("lib/prisma.ts") || path_lower.ends_with("lib/prisma.js") {
            if processed.contains("export const prisma") && !processed.contains("export default") {
                processed.push_str("\nexport default prisma;\n");
            }
        }
        let is_nextjs_app_file = normalized_path.contains("/app/") || normalized_path.contains("app/")
            || normalized_path.contains("/components/") || normalized_path.contains("components/");
        if is_nextjs_app_file {
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
                || processed.contains("useParams");
                
            if (has_client_hooks || has_mui_styles) && !processed.contains("\"use client\"") && !processed.contains("'use client'") {
                processed = format!("\"use client\";\n\n{}", processed);
            }
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

            if (processed.contains("useSession") || has_nextauth_dep) && !processed.contains("<SessionProvider") {
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

        // Ensure client components are never async functions (causes RSC 'Cannot access default.then on the server' error)
        let is_client_file = processed.contains("\"use client\"") || processed.contains("'use client'");
        if is_client_file {
            if processed.contains("export default async function") {
                processed = processed.replace("export default async function", "export default function");
            }
            // Strip server-only imports (Prisma, bcrypt, fs) from client components to prevent webpack & RSC crashes
            if !path_lower.ends_with("route.ts") && !path_lower.ends_with("route.js") {
                let filtered_lines: Vec<&str> = processed
                    .lines()
                    .filter(|line| {
                        let t = line.trim();
                        if t.starts_with("import ") {
                            let is_server_import = t.contains("'@prisma/client'")
                                || t.contains("\"@prisma/client\"")
                                || t.contains("'@/lib/prisma'")
                                || t.contains("\"@/lib/prisma\"")
                                || t.contains("'bcrypt'")
                                || t.contains("\"bcrypt\"")
                                || t.contains("'bcryptjs'")
                                || t.contains("\"bcryptjs\"");
                            return !is_server_import;
                        }
                        true
                    })
                    .collect();
                processed = filtered_lines.join("\n");
            }
        }

        // Dual-export synchronization (both named and default export) for hooks and components
        // Prevents: Attempted import error: 'useAuth' is not exported from '@/hooks/useAuth'
        if !path_lower.contains("/api/") && !path_lower.ends_with("route.ts") && !path_lower.ends_with("route.js") && !path_lower.ends_with("middleware.ts") {
            if let Some(stem) = std::path::Path::new(&normalized_path).file_stem().and_then(|s| s.to_str()) {
                let valid_ident = stem.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && stem.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_');
                if valid_ident && stem != "page" && stem != "layout" && stem != "index" && stem != "loading" && stem != "error" {
                    let has_ident_decl = processed.contains(&format!("function {}", stem))
                        || processed.contains(&format!("const {} =", stem))
                        || processed.contains(&format!("const {}:", stem));
                    if has_ident_decl {
                        let has_default_export = processed.contains("export default");
                        let has_named_export = processed.contains(&format!("export function {}", stem))
                            || processed.contains(&format!("export const {}", stem))
                            || processed.contains(&format!("export {{ {}", stem))
                            || processed.contains(&format!("export {{{}", stem));
                        if has_default_export && !has_named_export {
                            processed.push_str(&format!("\nexport {{ {} }};\n", stem));
                        } else if has_named_export && !has_default_export {
                            processed.push_str(&format!("\nexport default {};\n", stem));
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
            }

            // Re-check async client component after potential "use client" injection above
            if (processed.contains("\"use client\"") || processed.contains("'use client'")) && processed.contains("export default async function") {
                processed = processed.replace("export default async function", "export default function");
            }

            // Ensure layout.tsx / page.tsx never omit export default (prevents RSC default.then crash)
            if path_lower.ends_with("layout.tsx") || path_lower.ends_with("layout.jsx") || path_lower.ends_with("page.tsx") || path_lower.ends_with("page.jsx") {
                if !processed.contains("export default") {
                    let mut found_comp: Option<String> = None;
                    for candidate in &["RootLayout", "Layout", "HomePage", "Page", "Home", "DashboardPage", "Dashboard", "ProfilePage", "Profile", "LoginPage", "RegisterPage"] {
                        if processed.contains(&format!("function {}", candidate)) || processed.contains(&format!("const {}", candidate)) {
                            found_comp = Some(candidate.to_string());
                            break;
                        }
                    }
                    if found_comp.is_none() {
                        // Dynamically scan for any PascalCase function or const declaration
                        for line in processed.lines() {
                            let t = line.trim().trim_start_matches("export ").trim();
                            if let Some(rest) = t.strip_prefix("function ") {
                                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                                if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                                    found_comp = Some(name);
                                    break;
                                }
                            } else if let Some(rest) = t.strip_prefix("const ") {
                                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                                if name.chars().next().map_or(false, |c| c.is_uppercase()) && (t.contains("=>") || t.contains("React.FC") || t.contains("FC")) {
                                    found_comp = Some(name);
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(comp) = found_comp {
                        processed.push_str(&format!("\nexport default {};\n", comp));
                    }
                }
            }

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
    }

    processed
}

