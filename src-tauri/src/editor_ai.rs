use crate::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

static MAP_REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => format!("{}...", &s[..byte_idx]),
        None => s.to_string(),
    }
}

fn append_within_budget(dst: &mut String, block: &str, max_total_chars: usize) {
    if block.is_empty() || dst.chars().count() >= max_total_chars {
        return;
    }
    let remaining = max_total_chars.saturating_sub(dst.chars().count());
    let clipped = truncate_chars(block, remaining);
    if !clipped.is_empty() {
        if !dst.is_empty() && !dst.ends_with('\n') {
            dst.push('\n');
        }
        dst.push_str(&clipped);
    }
}

pub fn is_ignored_or_meta_edit_path(path: &str) -> bool {
    let low = path.trim().replace('\\', "/").to_ascii_lowercase();
    let norm = low.trim_start_matches("./");
    norm.is_empty()
        || norm.starts_with(".axiom/")
        || norm == ".axiom"
        || matches!(
            norm,
            "axiom-features.json"
                | "axiom-map.json"
                | "axiom-contracts.json"
                | "package-lock.json"
                | "yarn.lock"
                | "pnpm-lock.yaml"
        )
}

pub fn target_layer_priority(rel_path: &str) -> u8 {
    let low = rel_path.trim().replace('\\', "/").to_ascii_lowercase();
    if low.ends_with("schema.prisma") || low.ends_with(".prisma") {
        0
    } else if low.contains("/types/")
        || low.starts_with("types/")
        || low.contains("/models/")
        || low.starts_with("models/")
    {
        1
    } else if low.contains("/lib/")
        || low.starts_with("lib/")
        || low.contains("/utils/")
        || low.starts_with("utils/")
    {
        2
    } else if low.contains("/api/")
        || low.starts_with("api/")
        || low.ends_with("route.ts")
        || low.ends_with("route.js")
    {
        3
    } else if low.contains("/hooks/")
        || low.starts_with("hooks/")
        || low.contains("/store/")
        || low.starts_with("store/")
    {
        4
    } else if low.contains("/components/") || low.starts_with("components/") {
        5
    } else if low.ends_with(".css")
        || low.ends_with(".scss")
        || low.starts_with("tailwind.config.")
    {
        7
    } else {
        6
    }
}

fn classify_file_layer(rel_path: &str) -> &'static str {
    let low = rel_path.replace('\\', "/").to_ascii_lowercase();
    if low.ends_with(".prisma") {
        "schema"
    } else if low.contains("/api/") || low.starts_with("api/") || low.ends_with("/route.ts") || low.ends_with("/route.js") {
        "api"
    } else if low.contains("/hooks/") || low.starts_with("hooks/") {
        "hook"
    } else if low.contains("/store/") || low.starts_with("store/") {
        "store"
    } else if low.contains("/models/") || low.starts_with("models/") {
        "model"
    } else if low.contains("/types/") || low.starts_with("types/") {
        "type"
    } else if low.contains("/components/") || low.starts_with("components/") {
        "component"
    } else if low.contains("/services/") || low.starts_with("services/") {
        "service"
    } else if low.ends_with("/page.tsx")
        || low.ends_with("/page.jsx")
        || low.starts_with("app/")
        || low.starts_with("src/app/")
    {
        "page"
    } else {
        "module"
    }
}

fn normalize_domain_key(raw: &str) -> String {
    let s: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if s.len() > 4 && s.ends_with("ies") {
        format!("{}y", &s[..s.len() - 3])
    } else if s.len() > 3 && s.ends_with('s') && !s.ends_with("ss") && !s.ends_with("status") {
        s[..s.len() - 1].to_string()
    } else {
        s
    }
}

fn is_shell_or_shared_path(rel_path: &str) -> bool {
    let low = rel_path.replace('\\', "/").to_ascii_lowercase();
    low.ends_with("/layout.tsx")
        || low.ends_with("/layout.jsx")
        || low.ends_with("/_layout.tsx")
        || low.ends_with("/globals.css")
        || low.ends_with("/+html.tsx")
        || low.contains("/lib/prisma")
        || low.contains("/lib/auth")
        || low.ends_with("/providers.tsx")
        || low.ends_with("/navbar.tsx")
        || low.ends_with("/footer.tsx")
        || low.ends_with("/ui.tsx")
        || low.contains("/components/ui/")
        || low.ends_with("/useusertier.ts")
        || low.ends_with("/useuser.ts")
        || low.ends_with("/types/index.ts")
        || low.contains("/api/auth/")
        || low.contains("/api/register")
}

fn build_vertical_features(proj_dir: &std::path::Path, deduped_files: &[Value]) -> Vec<Value> {
    let is_excluded_segment = |seg: &str| -> bool {
        matches!(
            seg.to_ascii_lowercase().as_str(),
            "" | "api"
                | "auth"
                | "login"
                | "register"
                | "about"
                | "pricing"
                | "account"
                | "legal"
                | "privacy"
                | "terms"
                | "blog"
                | "dashboard"
                | "settings"
                | "index"
                | "_layout"
                | "+html"
                | "health"
                | "theme"
                | "user"
                | "usertier"
                | "toast"
                | "app"
                | "[id]"
                | "[...nextauth]"
        ) || seg.starts_with('[')
            || seg.starts_with('(')
    };

    struct ContractMeta {
        internal_deps: Vec<String>,
        endpoints: Vec<String>,
        api_calls: Vec<String>,
        models: Vec<String>,
        connected_routes: Vec<String>,
    }

    let mut contracts_by_path: HashMap<String, ContractMeta> = HashMap::new();
    let contracts_path = proj_dir.join(".axiom").join("axiom-contracts.json");
    if let Ok(raw) = fs::read_to_string(&contracts_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if let Some(arr) = val.get("files").and_then(|f| f.as_array()) {
                for entry in arr {
                    let Some(p) = entry.get("path").and_then(|v| v.as_str()) else { continue };
                    let norm = p.replace('\\', "/");
                    let str_arr = |v: Option<&Value>| -> Vec<String> {
                        v.and_then(|x| x.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|i| i.as_str().map(|s| s.replace('\\', "/")))
                                    .collect()
                            })
                            .unwrap_or_default()
                    };
                    contracts_by_path.insert(
                        norm.to_lowercase(),
                        ContractMeta {
                            internal_deps: str_arr(entry.get("dependencies").and_then(|d| d.get("internal"))),
                            endpoints: str_arr(entry.get("capabilities").and_then(|c| c.get("endpoints"))),
                            api_calls: str_arr(entry.get("consumedContracts").and_then(|c| c.get("apiCalls"))),
                            models: str_arr(entry.get("capabilities").and_then(|c| c.get("models"))),
                            connected_routes: str_arr(entry.get("connectedRoutes")),
                        },
                    );
                }
            }
        }
    }

    let mut map_deps_by_path: HashMap<String, Vec<String>> = HashMap::new();
    let map_path = proj_dir.join("axiom-map.json");
    if let Ok(raw) = fs::read_to_string(&map_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if let Some(files_obj) = val.get("files").and_then(|f| f.as_object()) {
                for (p, info) in files_obj {
                    let deps = info
                        .get("dependencies")
                        .and_then(|d| d.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|i| i.as_str().map(|s| s.replace('\\', "/")))
                                .collect()
                        })
                        .unwrap_or_default();
                    map_deps_by_path.insert(p.replace('\\', "/").to_lowercase(), deps);
                }
            }
        }
    }

    let mut all_paths: Vec<String> = deduped_files
        .iter()
        .filter_map(|v| v.get("path").and_then(|p| p.as_str()).map(|s| s.replace('\\', "/")))
        .collect();
    all_paths.sort();
    all_paths.dedup();

    struct FeatureBucket {
        display_name: String,
        files: BTreeSet<String>,
        routes: BTreeSet<String>,
        endpoints: BTreeSet<String>,
        models: BTreeSet<String>,
    }

    let mut buckets: BTreeMap<String, FeatureBucket> = BTreeMap::new();

    fn ensure_bucket<'a>(
        buckets: &'a mut BTreeMap<String, FeatureBucket>,
        key: &str,
        raw_label: &str,
    ) -> &'a mut FeatureBucket {
        buckets.entry(key.to_string()).or_insert_with(|| {
            let mut chars = raw_label.chars();
            let cap = match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => raw_label.to_string(),
            };
            FeatureBucket {
                display_name: cap,
                files: BTreeSet::new(),
                routes: BTreeSet::new(),
                endpoints: BTreeSet::new(),
                models: BTreeSet::new(),
            }
        })
    }

    // Pass 1: Identify strong vertical anchors (feature pages, API resources, domain hooks/stores/models)
    for rel in &all_paths {
        if is_ignored_or_meta_edit_path(rel) || is_shell_or_shared_path(rel) {
            continue;
        }
        let low = rel.to_ascii_lowercase();
        let parts: Vec<&str> = rel.split('/').collect();

        // API route: src/app/api/<seg>/... or app/api/<seg>/...
        let api_seg = if parts.len() >= 5 && parts[0].eq_ignore_ascii_case("src") && parts[1].eq_ignore_ascii_case("app") && parts[2].eq_ignore_ascii_case("api") {
            Some(parts[3])
        } else if parts.len() >= 4 && parts[0].eq_ignore_ascii_case("app") && parts[1].eq_ignore_ascii_case("api") {
            Some(parts[2])
        } else {
            None
        };
        if let Some(seg) = api_seg {
            if !is_excluded_segment(seg) {
                let key = normalize_domain_key(seg);
                if key.len() >= 3 {
                    let b = ensure_bucket(&mut buckets, &key, seg);
                    b.files.insert(rel.clone());
                    b.endpoints.insert(format!("/api/{}", seg));
                    continue;
                }
            }
        }

        // Next.js page: src/app/<seg>/page.* or Expo screen: app/(tabs)/<seg>.* or app/<seg>.*
        let page_info: Option<(&str, String)> = if parts.len() >= 4
            && parts[0].eq_ignore_ascii_case("src")
            && parts[1].eq_ignore_ascii_case("app")
            && (low.ends_with("/page.tsx") || low.ends_with("/page.jsx"))
        {
            Some((parts[2], format!("/{}", parts[2])))
        } else if parts.len() == 3
            && parts[0].eq_ignore_ascii_case("app")
            && parts[1].starts_with('(')
            && (low.ends_with(".tsx") || low.ends_with(".jsx"))
        {
            let stem = parts[2].trim_end_matches(".tsx").trim_end_matches(".jsx");
            Some((stem, format!("/{}", stem)))
        } else if parts.len() == 2
            && parts[0].eq_ignore_ascii_case("app")
            && (low.ends_with(".tsx") || low.ends_with(".jsx"))
        {
            let stem = parts[1].trim_end_matches(".tsx").trim_end_matches(".jsx");
            Some((stem, format!("/{}", stem)))
        } else {
            None
        };
        if let Some((seg, route_str)) = page_info {
            if !is_excluded_segment(seg) {
                let key = normalize_domain_key(seg);
                if key.len() >= 3 {
                    let b = ensure_bucket(&mut buckets, &key, seg);
                    b.files.insert(rel.clone());
                    b.routes.insert(route_str);
                    continue;
                }
            }
        }

        // Hooks / Stores / Models
        let file_name = parts.last().copied().unwrap_or("");
        let stem = file_name
            .trim_end_matches(".tsx")
            .trim_end_matches(".ts")
            .trim_end_matches(".jsx")
            .trim_end_matches(".js");
        if (low.contains("/hooks/") || low.starts_with("hooks/") || low.contains("/store/") || low.starts_with("store/"))
            && stem.starts_with("use")
            && stem.len() > 3
        {
            let trimmed = stem.trim_start_matches("use").trim_end_matches("Store");
            if !is_excluded_segment(trimmed) {
                let key = normalize_domain_key(trimmed);
                if key.len() >= 3 {
                    let b = ensure_bucket(&mut buckets, &key, trimmed);
                    b.files.insert(rel.clone());
                }
            }
        } else if (low.contains("/models/") || low.starts_with("models/")) && !stem.eq_ignore_ascii_case("index") {
            let key = normalize_domain_key(stem);
            if key.len() >= 3 && !is_excluded_segment(stem) {
                let b = ensure_bucket(&mut buckets, &key, stem);
                b.files.insert(rel.clone());
                b.models.insert(stem.to_string());
            }
        }
    }

    let normalize_endpoint = |raw: &str| -> Option<String> {
        let trimmed = raw.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let no_query = trimmed.split('?').next().unwrap_or(trimmed).trim_end_matches('/');
        if no_query.len() < 2 {
            return None;
        }
        if let Ok(re_tpl) = regex::Regex::new(r"\$\{[^}]+\}") {
            Some(re_tpl.replace_all(no_query, "[id]").to_string())
        } else {
            Some(no_query.to_string())
        }
    };

    let mut prisma_models: Vec<String> = Vec::new();
    let schema_path = proj_dir.join("prisma").join("schema.prisma");
    if let Ok(schema_src) = fs::read_to_string(&schema_path) {
        if let Ok(re_model) = regex::Regex::new(r"(?m)^\s*model\s+([A-Z][A-Za-z0-9_]*)\s*\{") {
            for cap in re_model.captures_iter(&schema_src) {
                if let Some(m) = cap.get(1) {
                    let name = m.as_str().to_string();
                    if !prisma_models.contains(&name) {
                        prisma_models.push(name);
                    }
                }
            }
        }
    }

    // Pass 2: Attach stem-matching components & follow 1-hop contract/map dependencies
    let bucket_keys: Vec<String> = buckets.keys().cloned().collect();
    for key in &bucket_keys {
        if let Some(b) = buckets.get_mut(key) {
            for pm in &prisma_models {
                if pm.eq_ignore_ascii_case(key) || normalize_domain_key(pm) == *key {
                    b.models.insert(pm.clone());
                }
            }
        }
        for rel in &all_paths {
            if is_ignored_or_meta_edit_path(rel) || is_shell_or_shared_path(rel) {
                continue;
            }
            let file_name = rel.rsplit('/').next().unwrap_or("");
            let stem = file_name
                .trim_end_matches(".tsx")
                .trim_end_matches(".ts")
                .trim_end_matches(".jsx")
                .trim_end_matches(".js")
                .to_ascii_lowercase();
            let norm_stem = normalize_domain_key(&stem);
            if norm_stem == *key
                || norm_stem.starts_with(key.as_str())
                || (key.len() >= 4 && stem.contains(key.as_str()))
            {
                if let Some(b) = buckets.get_mut(key) {
                    b.files.insert(rel.clone());
                }
            }
        }

        let seed_files: Vec<String> = buckets
            .get(key)
            .map(|b| b.files.iter().cloned().collect())
            .unwrap_or_default();

        for f in seed_files {
            let f_low = f.to_lowercase();
            if let Some(meta) = contracts_by_path.get(&f_low) {
                if let Some(b) = buckets.get_mut(key) {
                    for dep in &meta.internal_deps {
                        let dep_base = dep.rsplit('/').next().unwrap_or(dep);
                        if dep_base.contains('.')
                            && !is_shell_or_shared_path(dep)
                            && !is_ignored_or_meta_edit_path(dep)
                        {
                            b.files.insert(dep.clone());
                        }
                    }
                    for ep in &meta.endpoints {
                        if let Some(clean_ep) = normalize_endpoint(ep) {
                            b.endpoints.insert(clean_ep);
                        }
                    }
                    for call in &meta.api_calls {
                        if let Some(clean_ep) = normalize_endpoint(call) {
                            b.endpoints.insert(clean_ep);
                        }
                    }
                    for m in &meta.models {
                        if m.eq_ignore_ascii_case(key) || normalize_domain_key(m) == *key {
                            b.models.insert(m.clone());
                        }
                    }
                    for rt in &meta.connected_routes {
                        if rt.to_ascii_lowercase().contains(key.as_str()) {
                            b.routes.insert(rt.clone());
                        }
                    }
                }
            }
            if let Some(deps) = map_deps_by_path.get(&f_low) {
                if let Some(b) = buckets.get_mut(key) {
                    for dep in deps {
                        let dep_base = dep.rsplit('/').next().unwrap_or(dep);
                        if dep_base.contains('.')
                            && !is_shell_or_shared_path(dep)
                            && !is_ignored_or_meta_edit_path(dep)
                        {
                            b.files.insert(dep.clone());
                        }
                    }
                }
            }
        }
    }

    buckets
        .into_iter()
        .filter(|(_, b)| !b.files.is_empty())
        .map(|(key, b)| {
            let mut file_vec: Vec<String> = b.files.into_iter().collect();
            file_vec.sort_by_key(|p| (target_layer_priority(p), p.clone()));
            let files_json: Vec<Value> = file_vec
                .into_iter()
                .map(|p| {
                    let layer = classify_file_layer(&p);
                    serde_json::json!({ "path": p, "layer": layer })
                })
                .collect();
            let routes_vec: Vec<String> = b.routes.into_iter().collect();
            let endpoints_vec: Vec<String> = b.endpoints.into_iter().collect();
            let models_vec: Vec<String> = b.models.into_iter().collect();
            serde_json::json!({
                "id": key,
                "name": b.display_name,
                "routes": routes_vec,
                "apiEndpoints": endpoints_vec,
                "models": models_vec,
                "files": files_json
            })
        })
        .collect()
}

pub fn format_features_for_router(proj_dir: &std::path::Path, max_chars: usize) -> String {
    let features_path = proj_dir.join("axiom-features.json");
    let Ok(raw) = fs::read_to_string(&features_path) else {
        return String::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&raw) else {
        return String::new();
    };

    let mut lines: Vec<String> = Vec::new();
    if let Some(vertical) = root.get("verticalFeatures").and_then(|v| v.as_array()) {
        for vf in vertical {
            let name = vf.get("name").and_then(|n| n.as_str()).unwrap_or("Feature");
            let file_paths: Vec<&str> = vf
                .get("files")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.get("path").and_then(|p| p.as_str()))
                        .collect()
                })
                .unwrap_or_default();
            if file_paths.is_empty() {
                continue;
            }
            let routes: Vec<&str> = vf
                .get("routes")
                .and_then(|r| r.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let apis: Vec<&str> = vf
                .get("apiEndpoints")
                .and_then(|r| r.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let models: Vec<&str> = vf
                .get("models")
                .and_then(|r| r.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();

            let mut meta_parts: Vec<String> = Vec::new();
            if !routes.is_empty() {
                meta_parts.push(format!("routes:[{}]", routes.join(",")));
            }
            if !apis.is_empty() {
                meta_parts.push(format!("api:[{}]", apis.join(",")));
            }
            if !models.is_empty() {
                meta_parts.push(format!("models:[{}]", models.join(",")));
            }
            let meta_str = if meta_parts.is_empty() {
                String::new()
            } else {
                format!(" ({})", meta_parts.join(" "))
            };
            lines.push(format!("- {}{}: {}", name, meta_str, file_paths.join(", ")));
        }
    }

    if lines.is_empty() {
        if let Some(feats) = root.get("features").and_then(|v| v.as_array()) {
            for f in feats {
                let name = f.get("name").and_then(|n| n.as_str()).unwrap_or("Feature");
                let paths: Vec<&str> = f
                    .get("files")
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.get("path").and_then(|p| p.as_str()))
                            .collect()
                    })
                    .unwrap_or_default();
                if !paths.is_empty() {
                    lines.push(format!("- {}: {}", name, paths.join(", ")));
                }
            }
        }
    }

    if lines.is_empty() {
        return String::new();
    }
    let body = format!(
        "\nFeature & Architecture Map (grouped domains -> files):\n{}\n",
        lines.join("\n")
    );
    truncate_chars(&body, max_chars)
}

pub fn format_reverse_dependents_prompt(
    proj_dir: &std::path::Path,
    rel_file_path: &str,
    max_chars: usize,
) -> String {
    let cur_norm = rel_file_path.trim().replace('\\', "/").trim_start_matches("./").to_string();
    let cur_low = cur_norm.to_lowercase();
    if cur_low.is_empty() {
        return String::new();
    }
    let cur_no_ext = cur_low
        .trim_end_matches(".tsx")
        .trim_end_matches(".ts")
        .trim_end_matches(".jsx")
        .trim_end_matches(".js");

    let mut callers: BTreeSet<String> = BTreeSet::new();

    let contracts_path = proj_dir.join(".axiom").join("axiom-contracts.json");
    if let Ok(raw) = fs::read_to_string(&contracts_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
            if let Some(files) = val.get("files").and_then(|f| f.as_array()) {
                for entry in files {
                    let Some(p) = entry.get("path").and_then(|v| v.as_str()) else { continue };
                    let p_norm = p.replace('\\', "/");
                    if p_norm.eq_ignore_ascii_case(&cur_norm) {
                        continue;
                    }
                    let deps = entry
                        .get("dependencies")
                        .and_then(|d| d.get("internal"))
                        .and_then(|a| a.as_array());
                    if let Some(dep_arr) = deps {
                        let hits = dep_arr.iter().any(|d| {
                            let dl = d.as_str().unwrap_or("").replace('\\', "/").to_lowercase();
                            let dl_no_ext = dl
                                .trim_end_matches(".tsx")
                                .trim_end_matches(".ts")
                                .trim_end_matches(".jsx")
                                .trim_end_matches(".js");
                            dl == cur_low || (!cur_no_ext.is_empty() && dl_no_ext == cur_no_ext)
                        });
                        if hits {
                            callers.insert(p_norm);
                        }
                    }
                }
            }
        }
    }

    if callers.is_empty() {
        let map_path = proj_dir.join("axiom-map.json");
        if let Ok(raw) = fs::read_to_string(&map_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(files_obj) = val.get("files").and_then(|f| f.as_object()) {
                    for (p, info) in files_obj {
                        let p_norm = p.replace('\\', "/");
                        if p_norm.eq_ignore_ascii_case(&cur_norm) {
                            continue;
                        }
                        if let Some(dep_arr) = info.get("dependencies").and_then(|d| d.as_array()) {
                            let hits = dep_arr.iter().any(|d| {
                                let dl = d.as_str().unwrap_or("").replace('\\', "/").to_lowercase();
                                let dl_no_ext = dl
                                    .trim_end_matches(".tsx")
                                    .trim_end_matches(".ts")
                                    .trim_end_matches(".jsx")
                                    .trim_end_matches(".js");
                                dl == cur_low || (!cur_no_ext.is_empty() && dl_no_ext == cur_no_ext)
                            });
                            if hits {
                                callers.insert(p_norm);
                            }
                        }
                    }
                }
            }
        }
    }

    if callers.is_empty() {
        return String::new();
    }
    let list: Vec<String> = callers.into_iter().take(15).collect();
    let msg = format!(
        "\nREVERSE DEPENDENTS (files importing {} — preserve exported names/types unless callers also update):\n- {}\n",
        cur_norm,
        list.join("\n- ")
    );
    truncate_chars(&msg, max_chars)
}

pub fn post_process_edited_file(
    proj_dir: &std::path::Path,
    rel_file_path: &str,
    content: &str,
    allow_next_and_tailwind: bool,
) -> String {
    if crate::templates_expo::is_expo_project_dir(proj_dir) {
        return post_process_generated_file(
            rel_file_path,
            content,
            &serde_json::json!({ "framework": "expo" }),
            false,
        );
    }
    let mut out = content.to_string();
    if allow_next_and_tailwind {
        let routes_list: Vec<String> = get_project_routes_from_dir(proj_dir)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        if !routes_list.is_empty() {
            out = sanitize_app_router_links_with_routes(&out, &routes_list);
        }
    }
    post_process_generated_file(
        rel_file_path,
        &out,
        &serde_json::json!({}),
        allow_next_and_tailwind,
    )
}

pub fn refresh_project_maps_after_file_write(
    proj_dir: std::path::PathBuf,
    project_id: String,
    rel_file_path: String,
    content: String,
) {
    if is_ignored_or_meta_edit_path(&rel_file_path) {
        return;
    }
    let low = rel_file_path.replace('\\', "/").to_ascii_lowercase();
    let is_trackable = low.ends_with(".ts")
        || low.ends_with(".tsx")
        || low.ends_with(".js")
        || low.ends_with(".jsx")
        || low.ends_with(".prisma")
        || low.ends_with(".css")
        || low.ends_with(".scss");
    if !is_trackable {
        return;
    }

    std::thread::spawn(move || {
        let lock = MAP_REFRESH_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = match lock.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !proj_dir.is_dir() {
            return;
        }

        let mut files_specs: Vec<Value> = Vec::new();
        let mut stack = vec![proj_dir.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if p.is_dir() {
                    if !matches!(
                        name.as_str(),
                        "node_modules" | ".next" | ".expo" | ".expo-verify" | ".expo-e2e-dist" | ".git" | ".axiom" | "dist" | "build" | "target"
                    ) {
                        stack.push(p);
                    }
                } else if p.is_file() {
                    if let Ok(rel) = p.strip_prefix(&proj_dir) {
                        let rel_norm = rel.to_string_lossy().replace('\\', "/");
                        if !is_ignored_or_meta_edit_path(&rel_norm) {
                            files_specs.push(serde_json::json!({ "path": rel_norm }));
                        }
                    }
                }
            }
        }
        files_specs.sort_by(|a, b| {
            a.get("path")
                .and_then(|v| v.as_str())
                .cmp(&b.get("path").and_then(|v| v.as_str()))
        });

        let rel_norm = rel_file_path.replace('\\', "/").trim_start_matches("./").to_string();
        crate::orchestrator::harvest_file_to_axiom_contracts(
            &proj_dir,
            &project_id,
            &rel_norm,
            &content,
            &files_specs,
        );
        let _ = crate::orchestrator::generate_axiom_map(&proj_dir);

        let proj_name = fs::read_to_string(proj_dir.join("axiom-features.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|v| v.get("projectName").and_then(|s| s.as_str()).map(|s| s.to_string()))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| project_id.clone());

        generate_axiom_features_json(&proj_dir, &proj_name, &files_specs);
    });
}

pub fn generate_axiom_features_json(proj_dir: &std::path::Path, project_name: &str, files_list: &[Value]) {
    let mut deduped_files = files_list.to_vec();
    crate::sanitizer::deduplicate_manifest_files(&mut deduped_files);
    let mut files_by_dir: std::collections::BTreeMap<String, Vec<Value>> = std::collections::BTreeMap::new();
    
    for file_spec in &deduped_files {
        if let Some(path_str) = file_spec.get("path").and_then(|p| p.as_str()) {
            let normalized = path_str.replace('\\', "/");
            let parts: Vec<&str> = normalized.split('/').collect();
            let top_dir = if parts.len() > 3 && parts[0].eq_ignore_ascii_case("src") && parts[1].eq_ignore_ascii_case("app") && parts[2].eq_ignore_ascii_case("api") {
                "api".to_string()
            } else if parts.len() > 2 && parts[0].eq_ignore_ascii_case("src") {
                parts[1].to_string()
            } else if parts.len() > 2 && parts[0].eq_ignore_ascii_case("app") && parts[1].eq_ignore_ascii_case("api") {
                "api".to_string()
            } else if parts.len() > 1 {
                parts[0].to_string()
            } else {
                "__root__".to_string()
            };
            
            files_by_dir.entry(top_dir).or_insert_with(Vec::new).push(file_spec.clone());
        }
    }

    let mut features = Vec::new();
    for (dir, dir_files) in files_by_dir {
        let feature_name = match dir.to_lowercase().as_str() {
            "app" => "App Pages & Routes".to_string(),
            "components" => "UI Components".to_string(),
            "pages" => "Pages & Routes".to_string(),
            "api" => "API & Backend".to_string(),
            "routes" => "Routes".to_string(),
            "controllers" => "Controllers".to_string(),
            "models" => "Data Models".to_string(),
            "types" => "TypeScript Types".to_string(),
            "views" => "Views & Templates".to_string(),
            "services" => "Services".to_string(),
            "utils" => "Utilities".to_string(),
            "hooks" => "React Hooks".to_string(),
            "context" | "contexts" => "App State & Context".to_string(),
            "store" => "State Management".to_string(),
            "auth" => "Authentication".to_string(),
            "config" => "Configuration".to_string(),
            "prisma" => "Database Schema".to_string(),
            "lib" => "Core Library".to_string(),
            "styles" => "Styles".to_string(),
            "src" => "Source Files".to_string(),
            "__root__" => "Root Files".to_string(),
            other => other.to_string(),
        };

        let mut files_array = Vec::new();
        for df in dir_files {
            if let Some(p) = df.get("path").and_then(|path| path.as_str()) {
                let role = if p.contains("index") {
                    "entry point"
                } else if p.contains("config") || p.contains("settings") {
                    "configuration"
                } else if p.contains(".css") {
                    "stylesheet"
                } else {
                    "logic"
                };
                files_array.push(serde_json::json!({
                    "path": p,
                    "role": role
                }));
            }
        }

        features.push(serde_json::json!({
            "id": feature_name.to_lowercase().replace(' ', "-"),
            "name": feature_name,
            "description": format!("Files related to {}.", dir),
            "editableDescription": format!("Files related to {}.", dir),
            "files": files_array
        }));
    }

    let vertical_features = build_vertical_features(proj_dir, &deduped_files);

    let features_map = serde_json::json!({
        "version": "1.0",
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "projectName": project_name,
        "totalFiles": deduped_files.len(),
        "features": features,
        "verticalFeatures": vertical_features,
        "_usage": "This file is used by the Axiom Forge AI editor. Edit 'editableDescription' to change what a feature does."
    });

    if let Ok(content) = serde_json::to_string_pretty(&features_map) {
        let _ = fs::write(proj_dir.join("axiom-features.json"), content);
    }
}

pub fn extract_code_content(raw: &str) -> String {
    let raw_trimmed = raw.trim();
    if let Some(start_idx) = raw_trimmed.find("```") {
        let after_start = &raw_trimmed[start_idx + 3..];
        if let Some(line_end_idx) = after_start.find('\n') {
            let after_line_end = &after_start[line_end_idx + 1..];
            if let Some(end_idx) = after_line_end.find("```") {
                return after_line_end[..end_idx].trim().to_string();
            } else {
                return after_line_end.trim().to_string();
            }
        }
    }
    raw_trimmed.to_string()
}

#[derive(Serialize)]
pub struct AIEditResult {
    pub success: bool,
    #[serde(rename = "newContent")]
    pub new_content: Option<String>,
    pub error: Option<String>,
}

pub async fn resolve_valid_ollama_model(client: &reqwest::Client, requested: &str) -> String {
    if let Ok(tags_resp) = client.get("http://127.0.0.1:11434/api/tags").send().await {
        if let Ok(tags_json) = tags_resp.json::<Value>().await {
            if let Some(models_arr) = tags_json.get("models").and_then(|m| m.as_array()) {
                let installed_names: Vec<String> = models_arr
                    .iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect();

                // 1. If exact or prefix match exists, use it
                for name in &installed_names {
                    if name == requested || name.starts_with(requested) || requested.starts_with(name) {
                        return name.clone();
                    }
                }

                // 2. If requested is not found, fallback to best installed model
                if !installed_names.is_empty() {
                    if let Some(coder) = installed_names.iter().find(|n| n.contains("coder") || n.contains("qwen")) {
                        return coder.clone();
                    }
                    return installed_names[0].clone();
                }
            }
        }
    }
    requested.to_string()
}

fn verify_targeted_edit_axiom_attrs(before: &str, after: &str, target: &str) -> Result<(), String> {
    let attr_re = regex::Regex::new(r#"(?i)data-axiom-(file|component)="([^"]*)""#).ok();
    let Some(re) = attr_re else {
        return Ok(());
    };
    let mut before_attrs: Vec<(String, String)> = Vec::new();
    for cap in re.captures_iter(before) {
        if let (Some(k), Some(v)) = (cap.get(1), cap.get(2)) {
            before_attrs.push((k.as_str().to_lowercase(), v.as_str().to_string()));
        }
    }
    let mut after_attrs: Vec<(String, String)> = Vec::new();
    for cap in re.captures_iter(after) {
        if let (Some(k), Some(v)) = (cap.get(1), cap.get(2)) {
            let kind = k.as_str().to_lowercase();
            let val = v.as_str();
            if val.contains(';')
                || val.contains('{')
                || val.contains('}')
                || val.contains("=>")
                || (kind == "file" && val.chars().any(|c| c.is_whitespace()))
            {
                return Err(format!(
                    "Edit rejected: would corrupt data-axiom-{} metadata attribute with code ({:?}). Copy actual code lines into 'target'.",
                    kind, val
                ));
            }
            after_attrs.push((kind, val.to_string()));
        }
    }
    if !target.contains("data-axiom-") && !before_attrs.is_empty() && before_attrs != after_attrs {
        return Err(
            "Edit rejected: 'target' matched inside a data-axiom-file/data-axiom-component metadata attribute instead of source code."
                .to_string(),
        );
    }
    Ok(())
}

pub fn apply_targeted_edit(content: &str, target: &str, replacement: &str) -> Result<String, String> {
    let apply_raw = |content: &str, target: &str, replacement: &str| -> Result<String, String> {
        let target_trimmed = target.trim();
        if target_trimmed.is_empty() {
            return Err("Target block is empty".to_string());
        }

        let is_specific_target = |t: &str| -> bool {
            let trimmed = t.trim();
            trimmed.len() >= 15
                || trimmed.contains("<Link")
                || trimmed.contains("href=")
                || trimmed.contains('\n')
        };

        // 1. Try exact match
        let exact_matches: Vec<_> = content.match_indices(target).collect();
        if exact_matches.len() == 1 {
            return Ok(content.replace(target, replacement));
        } else if exact_matches.len() > 1 {
            if is_specific_target(target) {
                return Ok(content.replace(target, replacement));
            }
            return Err(format!("Target block is duplicated ({}) in file. It must be unique.", exact_matches.len()));
        }

        // 2. Try trimmed match
        let trimmed_matches: Vec<_> = content.match_indices(target_trimmed).collect();
        if trimmed_matches.len() == 1 {
            return Ok(content.replace(target_trimmed, replacement.trim()));
        } else if trimmed_matches.len() > 1 {
            if is_specific_target(target_trimmed) {
                return Ok(content.replace(target_trimmed, replacement.trim()));
            }
            return Err(format!("Trimmed target block is duplicated ({}) in file.", trimmed_matches.len()));
        }

        // 3. Whitespace-collapsed / normalized search
        let target_chars: Vec<char> = target.chars().filter(|c| !c.is_whitespace()).collect();
        if !target_chars.is_empty() {
            let content_chars: Vec<(usize, char)> = content.char_indices().collect();
            let n = content_chars.len();
            let m = target_chars.len();
            let mut matches_found = Vec::new();

            for i in 0..n {
                if content_chars[i].1.is_whitespace() {
                    continue;
                }
                let mut target_idx = 0;
                let mut j = i;
                while j < n && target_idx < m {
                    let ch = content_chars[j].1;
                    if ch.is_whitespace() {
                        j += 1;
                        continue;
                    }
                    if ch == target_chars[target_idx] {
                        target_idx += 1;
                        j += 1;
                    } else {
                        break;
                    }
                }
                if target_idx == m && j > 0 {
                    let start_byte = content_chars[i].0;
                    let end_byte = content_chars[j - 1].0 + content_chars[j - 1].1.len_utf8();
                    matches_found.push((start_byte, end_byte));
                }
            }

            if matches_found.len() == 1 {
                let (start, end) = matches_found[0];
                let mut result = String::with_capacity(content.len() + replacement.len());
                result.push_str(&content[..start]);
                result.push_str(replacement);
                result.push_str(&content[end..]);
                return Ok(result);
            } else if matches_found.len() > 1 {
                if is_specific_target(target) {
                    let mut result = content.to_string();
                    for &(start, end) in matches_found.iter().rev() {
                        result.replace_range(start..end, replacement);
                    }
                    return Ok(result);
                }
                return Err(format!("Normalized target matches multiple ({}) places in file.", matches_found.len()));
            }
        }

        // 4. Line-trimmed sliding window match
        let target_lines: Vec<&str> = target.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        if !target_lines.is_empty() {
            let content_lines: Vec<&str> = content.lines().collect();
            let mut line_matches = Vec::new();

            for i in 0..=content_lines.len().saturating_sub(target_lines.len()) {
                let mut matched = true;
                for (k, t_line) in target_lines.iter().enumerate() {
                    if content_lines[i + k].trim() != *t_line {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    line_matches.push(i);
                }
            }

            if line_matches.len() == 1 {
                let match_line_idx = line_matches[0];
                let lines_before: String = content_lines[..match_line_idx].join("\n");
                let prefix = if lines_before.is_empty() { String::new() } else { format!("{}\n", lines_before) };
                let lines_after: String = content_lines[match_line_idx + target_lines.len()..].join("\n");
                let suffix = if lines_after.is_empty() { String::new() } else { format!("\n{}", lines_after) };
                return Ok(format!("{}{}{}", prefix, replacement, suffix));
            } else if line_matches.len() > 1 && is_specific_target(target) {
                let mut modified_lines = content_lines.clone();
                let replacement_lines: Vec<&str> = replacement.lines().collect();
                for &idx in line_matches.iter().rev() {
                    modified_lines.splice(idx..idx + target_lines.len(), replacement_lines.clone());
                }
                return Ok(modified_lines.join("\n"));
            }
        }

        Err(format!("Target code block not found in file: {:?}", target))
    };

    let edited = apply_raw(content, target, replacement)?;
    verify_targeted_edit_axiom_attrs(content, &edited, target)?;
    Ok(edited)
}

async fn post_ollama_chat_with_retry(
    client: &reqwest::Client,
    payload: &Value,
) -> Result<reqwest::Response, String> {
    let mut last_err = String::new();
    for attempt in 0..3u8 {
        match client
            .post("http://127.0.0.1:11434/api/chat")
            .json(payload)
            .send()
            .await
        {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                last_err = e.to_string();
                debug_log_to_file(format!(
                    "[AI Edit] Ollama connection error (attempt {}/3): {}",
                    attempt + 1,
                    last_err
                ));
                if attempt == 2 {
                    break;
                }
                if attempt == 0 {
                    let _ = ollama_start_server().await;
                }
                let delay_ms = if attempt == 0 { 1500 } else { 3000 };
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
    Err(last_err)
}

fn find_balanced_jsx_blocks(src: &str, tag: &str) -> Vec<(usize, usize)> {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let tag_bytes = tag.as_bytes();
    let tlen = tag_bytes.len();
    let mut stack: Vec<usize> = Vec::new();
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;

    while i < len {
        // Ignore single-line JSX `{/* ... */}` or JS strings when looking for top-level tag boundaries
        if bytes[i] == b'<' {
            // Closing tag: </tag...>
            if i + 2 + tlen <= len
                && bytes[i + 1] == b'/'
                && src[i + 2..i + 2 + tlen].eq_ignore_ascii_case(tag)
            {
                let next_pos = i + 2 + tlen;
                if next_pos < len && (bytes[next_pos].is_ascii_whitespace() || bytes[next_pos] == b'>') {
                    if let Some(rel_gt) = src[next_pos..].find('>') {
                        let close_end = next_pos + rel_gt + 1;
                        if let Some(open_start) = stack.pop() {
                            blocks.push((open_start, close_end));
                        }
                        i = close_end;
                        continue;
                    }
                }
            }
            // Opening tag: <tag ...> (and skip self-closing <tag ... />)
            if i + 1 + tlen <= len && src[i + 1..i + 1 + tlen].eq_ignore_ascii_case(tag) {
                let next_pos = i + 1 + tlen;
                if next_pos < len
                    && (bytes[next_pos].is_ascii_whitespace()
                        || bytes[next_pos] == b'>'
                        || bytes[next_pos] == b'/')
                {
                    let mut j = next_pos;
                    let mut quote: Option<u8> = None;
                    let mut brace_depth = 0usize;
                    let mut gt_pos: Option<usize> = None;
                    while j < len {
                        let b = bytes[j];
                        if let Some(q) = quote {
                            if b == b'\\' {
                                j += 2;
                                continue;
                            }
                            if b == q {
                                quote = None;
                            }
                        } else if b == b'"' || b == b'\'' || b == b'`' {
                            quote = Some(b);
                        } else if b == b'{' {
                            brace_depth += 1;
                        } else if b == b'}' {
                            brace_depth = brace_depth.saturating_sub(1);
                        } else if b == b'>' && brace_depth == 0 {
                            gt_pos = Some(j);
                            break;
                        }
                        j += 1;
                    }
                    if let Some(end_gt) = gt_pos {
                        let mut back = end_gt;
                        while back > i && bytes[back - 1].is_ascii_whitespace() {
                            back -= 1;
                        }
                        let is_self_closing = back > i && bytes[back - 1] == b'/';
                        if !is_self_closing {
                            stack.push(i);
                        }
                        i = end_gt + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }

    blocks
}

fn extract_jsx_text_anchors(snippet: &str) -> Vec<String> {
    let mut anchors: Vec<String> = Vec::new();
    let mut pos = 0usize;
    while let Some(gt_rel) = snippet[pos..].find('>') {
        let gt = pos + gt_rel + 1;
        if let Some(lt_rel) = snippet[gt..].find('<') {
            let inner = snippet[gt..gt + lt_rel].trim();
            if inner.len() >= 4
                && !inner.contains('{')
                && !inner.contains('}')
                && inner.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
                && !anchors.iter().any(|a| a == inner)
            {
                anchors.push(inner.to_string());
            }
            pos = gt + lt_rel;
        } else {
            break;
        }
    }
    anchors
}

/// Attempts to splice a partial JSX block (e.g. `<section>...</section>` or `<div ...>...</div>`)
/// back into the matching balanced JSX block in `original` using visible text anchors.
pub fn try_splice_partial_jsx_snippet(original: &str, snippet: &str) -> Option<String> {
    let trimmed = snippet.trim().trim_end_matches(';').trim();
    if !trimmed.starts_with('<') {
        return None;
    }
    let after_lt = &trimmed[1..];
    let tag_len = after_lt
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(after_lt.len());
    if tag_len == 0 {
        return None;
    }
    let tag = &after_lt[..tag_len];
    if !tag.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
        return None;
    }
    let expected_end = format!("</{}>", tag);
    if !trimmed.to_ascii_lowercase().ends_with(&expected_end.to_ascii_lowercase()) {
        return None;
    }

    let blocks = find_balanced_jsx_blocks(original, tag);
    if blocks.is_empty() {
        return None;
    }

    let anchors = extract_jsx_text_anchors(trimmed);
    let mut scored: Vec<(usize, usize, usize, usize)> = blocks
        .into_iter()
        .map(|(start, end)| {
            let block = &original[start..end];
            let score = anchors.iter().filter(|a| block.contains(a.as_str())).count();
            (score, end - start, start, end)
        })
        .collect();

    let best_score = scored.iter().map(|s| s.0).max().unwrap_or(0);
    if anchors.is_empty() {
        if scored.len() != 1 {
            return None;
        }
    } else if best_score == 0 {
        return None;
    }

    // Prefer highest anchor score, then narrowest/innermost balanced block
    scored.retain(|s| s.0 == best_score);
    scored.sort_by_key(|s| (s.1, s.2));
    let (_, _, best_start, best_end) = scored[0];

    // Guard against ambiguous disjoint siblings with identical top score
    for &(_, _, o_start, o_end) in scored.iter().skip(1) {
        let encloses_best = o_start <= best_start && o_end >= best_end;
        if !encloses_best {
            return None;
        }
    }

    let mut out = String::with_capacity(original.len() + trimmed.len());
    out.push_str(&original[..best_start]);
    out.push_str(trimmed);
    out.push_str(&original[best_end..]);
    Some(out)
}

fn is_global_color_request(prompt: &str) -> bool {
    let low = prompt.to_lowercase();
    [
        "boje",
        "boju",
        "boja",
        "color",
        "colors",
        "theme",
        "temu",
        "tema",
        "palette",
        "palet",
        "pozadin",
        "background",
    ]
    .iter()
    .any(|kw| low.contains(kw))
}

/// Validates and canonicalizes Router target paths against `existing_files`:
/// - Fixes wrong directories when a unique basename exists on disk (e.g. `src/globals.css` -> `src/app/globals.css`),
/// - Drops hallucinated non-existent paths unless the prompt explicitly asks to create new files,
/// - Always ensures existing `globals.css` is included on global color/theme prompts.
pub fn reconcile_router_target_files(
    raw_targets: Vec<String>,
    existing_files: &[String],
    prompt: &str,
) -> Vec<String> {
    let prompt_low = prompt.to_lowercase();
    let asks_to_create = [
        "create ",
        "add new",
        "new page",
        "new component",
        "new file",
        "napravi nov",
        "dodaj nov",
        "nova stranica",
        "novu stranicu",
    ]
    .iter()
    .any(|kw| prompt_low.contains(kw));

    let norm = |p: &str| p.trim().replace('\\', "/").trim_start_matches("./").to_string();
    let mut existing_by_low: HashMap<String, String> = HashMap::new();
    let mut existing_by_base: HashMap<String, Vec<String>> = HashMap::new();
    for f in existing_files {
        let n = norm(f);
        if n.is_empty() || is_ignored_or_meta_edit_path(&n) {
            continue;
        }
        existing_by_low.insert(n.to_lowercase(), n.clone());
        let base = n.rsplit('/').next().unwrap_or(&n).to_lowercase();
        existing_by_base.entry(base).or_default().push(n);
    }

    let mut result: Vec<String> = Vec::new();
    let mut push_unique = |path: String| {
        let low = path.to_lowercase();
        if !path.is_empty()
            && !is_ignored_or_meta_edit_path(&path)
            && !result.iter().any(|x| x.to_lowercase() == low)
        {
            result.push(path);
        }
    };

    for raw in raw_targets {
        let n = norm(&raw);
        if n.is_empty() || is_ignored_or_meta_edit_path(&n) {
            continue;
        }
        let n_low = n.to_lowercase();
        if let Some(exact) = existing_by_low.get(&n_low) {
            push_unique(exact.clone());
            continue;
        }
        let base = n_low.rsplit('/').next().unwrap_or(&n_low);
        if let Some(matches) = existing_by_base.get(base) {
            if matches.len() == 1 {
                push_unique(matches[0].clone());
                continue;
            }
            if let Some(preferred) = matches.iter().find(|m| m.to_lowercase().contains("src/app/")) {
                push_unique(preferred.clone());
                continue;
            }
        }
        if asks_to_create {
            push_unique(n);
        }
    }

    if is_global_color_request(prompt) {
        if let Some(css) = existing_by_low
            .get("src/app/globals.css")
            .or_else(|| existing_by_low.get("app/globals.css"))
        {
            push_unique(css.clone());
        }
    }

    result
}

/// For global color/theme prompts on projects that hardcode Tailwind utility classes instead of
/// semantic CSS variable tokens (`--primary` + `bg-primary`/`bg-background`), adds the key visual
/// shell files so re-theming visibly changes the rendered application.
pub fn expand_global_theme_targets(
    mut targets: Vec<String>,
    existing_files: &[String],
    prompt: &str,
    proj_dir: &Path,
) -> Vec<String> {
    if !is_global_color_request(prompt) {
        return targets;
    }
    let css_path = ["src/app/globals.css", "app/globals.css"]
        .into_iter()
        .find(|p| existing_files.iter().any(|e| e.eq_ignore_ascii_case(p)));
    let page_path = ["src/app/page.tsx", "app/page.tsx"]
        .into_iter()
        .find(|p| existing_files.iter().any(|e| e.eq_ignore_ascii_case(p)));

    let css_has_tokens = css_path.map_or(false, |p| {
        fs::read_to_string(proj_dir.join(p)).map_or(false, |s| s.contains("--primary:"))
    });
    let page_uses_tokens = page_path.map_or(false, |p| {
        fs::read_to_string(proj_dir.join(p)).map_or(false, |s| {
            s.contains("bg-primary") || s.contains("bg-background") || s.contains("bg-surface")
        })
    });

    if !(css_has_tokens && page_uses_tokens) {
        for cand in [
            "src/app/globals.css",
            "app/globals.css",
            "src/app/layout.tsx",
            "app/layout.tsx",
            "src/components/Navbar.tsx",
            "components/Navbar.tsx",
            "src/components/Footer.tsx",
            "components/Footer.tsx",
            "src/app/page.tsx",
            "app/page.tsx",
            "src/app/dashboard/page.tsx",
            "app/dashboard/page.tsx",
        ] {
            if let Some(exact) = existing_files.iter().find(|e| e.eq_ignore_ascii_case(cand)) {
                if !targets.iter().any(|t| t.eq_ignore_ascii_case(exact)) {
                    targets.push(exact.clone());
                }
            }
        }
    }

    targets
}

/// Detects when an LLM accidentally returns an unrelated auxiliary file (e.g., a non-JSX hook
/// like `useInvoices.ts`) instead of the target React/TSX component or page.
pub fn detect_wrong_file_overwrite(rel_file_path: &str, original: &str, candidate: &str) -> Option<String> {
    let low = rel_file_path.replace('\\', "/").to_ascii_lowercase();
    let is_jsx_file = low.ends_with(".tsx") || low.ends_with(".jsx");
    if !is_jsx_file {
        return None;
    }

    let orig_clean = strip_axiom_inspector_script(original);
    let orig_has_jsx = (orig_clean.contains("</") || orig_clean.contains("/>"))
        && (orig_clean.contains("return (") || orig_clean.contains("return(") || orig_clean.contains("return <"));
    let cand_has_jsx = candidate.contains("</") || candidate.contains("/>");

    if orig_has_jsx && !cand_has_jsx {
        return Some(format!(
            "AI returned a non-JSX module instead of JSX target file '{}'; aborted to avoid overwriting the file.",
            rel_file_path
        ));
    }

    let is_hook_or_store_path = low.contains("/hooks/")
        || low.starts_with("hooks/")
        || low.contains("/store/")
        || low.starts_with("store/");
    if !is_hook_or_store_path && orig_has_jsx {
        if let Ok(re_def) = regex::Regex::new(r"(?m)^\s*export\s+default\s+(?:function\s+)?([A-Za-z_$][A-Za-z0-9_$]*)") {
            let orig_sym = re_def.captures(&orig_clean).and_then(|c| c.get(1)).map(|m| m.as_str());
            let cand_sym = re_def.captures(candidate).and_then(|c| c.get(1)).map(|m| m.as_str());
            let looks_like_hook = |s: &str| {
                s.starts_with("use")
                    && s.chars().nth(3).map(|c| c.is_ascii_uppercase()).unwrap_or(false)
            };
            if let Some(cs) = cand_sym {
                if looks_like_hook(cs) && !orig_sym.map(looks_like_hook).unwrap_or(false) {
                    return Some(format!(
                        "AI returned hook '{}' instead of UI target file '{}'; aborted to avoid overwriting the file.",
                        cs, rel_file_path
                    ));
                }
            }
        }
    }

    None
}

#[tauri::command]
pub async fn editor_ai_edit(
    file_path: String,
    content: String,
    prompt: String,
    project_id: String,
    model_id: Option<String>,
    visual_context: Option<Value>,
) -> Result<AIEditResult, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let initial_model = model_id.unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    let model = resolve_valid_ollama_model(&client, &initial_model).await;

    let mut base_system_prompt = "You are an expert developer assistant helping the user edit a source code file. \
        Instead of rewriting the entire file, you must identify the exact lines that need to change and provide a list of target replacements in JSON format. \
        \n\n\
        CRITICAL RULES:\n\
        1. Respond ONLY with a valid JSON object matching the format described below. Do not write any explanations, introductory text, or markdown code blocks (do not wrap in ```json). Just the raw JSON.\n\
        2. In the JSON structure, under 'edits', each 'target' block must exactly match (character-for-character, including leading whitespace and indentation) a unique substring in the original file.\n\
        3. Do NOT include placeholders in 'replacement'. The code must be complete and ready to run.\n\
        4. Focus only on resolving the user's request. Do not make unrelated changes.\n\
        \n\n\
        JSON FORMAT:\n\
        {\n\
          \"explanation\": \"Brief explanation of the changes made\",\n\
          \"edits\": [\n\
            {\n\
              \"target\": \"exact string of lines in original file to be replaced\",\n\
              \"replacement\": \"the new code to put in its place\"\n\
            }\n\
          ]\n\
        }".to_string();

    let mut full_file_retry_system_prompt = "You are an expert developer assistant helping the user edit a source code file. \
        Return ONLY the COMPLETE updated file content from top to bottom (including all imports and exports) with the requested change applied. \
        Do NOT return JSON edit instructions or a partial snippet, and do not include explanations."
        .to_string();

    let rulepacks = get_stack_rulepacks(&file_path, &content, None);
    if !rulepacks.is_empty() {
        base_system_prompt.push_str(&rulepacks);
        full_file_retry_system_prompt.push_str(&rulepacks);
    }

    let mut user_prompt = format!(
        "Original File Path: {}\n\
         Original File Content:\n\
         ```\n\
         {}\n\
         ```\n\n\
         User Prompt / Instructed Edit: {}\n",
        file_path,
        strip_axiom_inspector_script(&content),
        prompt
    );

    if let Some(ref ctx) = visual_context {
        if let Some(tag_name) = ctx.get("tagName").and_then(|t| t.as_str()) {
            user_prompt.push_str(&format!(
                "\nVisual Context (user clicked/inspected this element in the browser preview):\n\
                 - HTML Tag: {}\n",
                tag_name
            ));
        }
        if let Some(text) = ctx.get("text").and_then(|t| t.as_str()) {
            user_prompt.push_str(&format!("- Element Text: {}\n", text));
        }
        if let Some(cls) = ctx.get("className").and_then(|c| c.as_str()) {
            if !cls.trim().is_empty() {
                user_prompt.push_str(&format!("- Element className: {}\n", cls.trim()));
            }
        }
        if let Some(component) = ctx.get("component").and_then(|c| c.as_str()) {
            user_prompt.push_str(&format!("- Component Name: {}\n", component));
        }
    }

    let proj_dir = get_project_path(&project_id);
    let is_expo_proj = crate::templates_expo::is_expo_project_dir(&proj_dir);
    let runtime_cfg = get_ollama_runtime_config(&model);
    let aux_budget_chars: usize = if runtime_cfg.num_ctx <= 6144 { 3200 } else { 6400 };
    let metadata_path = proj_dir.join(".axiom").join("metadata.json");
    let rel_file_path = if let Ok(stripped) = std::path::Path::new(&file_path).strip_prefix(&proj_dir) {
        stripped.to_str().unwrap_or("").replace('\\', "/")
    } else {
        file_path.replace('\\', "/")
    };

    let path_lower = rel_file_path.to_lowercase();
    let is_nav_or_link = !is_expo_proj
        && (path_lower.contains("navbar")
            || path_lower.contains("sidebar")
            || path_lower.contains("footer")
            || path_lower.contains("header")
            || path_lower.contains("nav")
            || content.contains("<Link")
            || content.contains("next/link"));

    if is_nav_or_link {
        let project_routes = get_project_routes_from_dir(&proj_dir);
        let routes_prompt = format_routes_prompt(&project_routes);
        if !routes_prompt.is_empty() {
            base_system_prompt.push_str(&routes_prompt);
            full_file_retry_system_prompt.push_str(&routes_prompt);
        }
    }

    let mut aux_context = String::new();
    let contracts_block = crate::orchestrator::format_axiom_contracts_prompt(&proj_dir, &rel_file_path);
    if !contracts_block.is_empty() {
        append_within_budget(
            &mut aux_context,
            &truncate_chars(&contracts_block, aux_budget_chars / 2),
            aux_budget_chars,
        );
    }

    let reverse_deps_block = format_reverse_dependents_prompt(&proj_dir, &rel_file_path, 650);
    if !reverse_deps_block.is_empty() {
        append_within_budget(&mut aux_context, &reverse_deps_block, aux_budget_chars);
    }

    if !is_expo_proj
        && (path_lower.contains("/api/")
            || path_lower.starts_with("api/")
            || path_lower.contains("prisma")
            || path_lower.contains("/models/")
            || path_lower.starts_with("models/")
            || path_lower.contains("/hooks/")
            || path_lower.starts_with("hooks/")
            || path_lower.contains("/types/")
            || path_lower.starts_with("types/"))
    {
        let prisma_summary = crate::self_healing::extract_prisma_schema_summary_for_healing(&proj_dir);
        if !prisma_summary.is_empty() {
            append_within_budget(&mut aux_context, &truncate_chars(&prisma_summary, 1100), aux_budget_chars);
        }
    }

    if metadata_path.exists() && aux_context.chars().count() < aux_budget_chars {
        if let Ok(meta_content) = fs::read_to_string(&metadata_path) {
            if let Ok(meta_json) = serde_json::from_str::<Value>(&meta_content) {
                if let Some(deps) = meta_json.get("dependencies").and_then(|d| d.get(&rel_file_path)).and_then(|a| a.as_array()) {
                    for dep_val in deps {
                        if aux_context.chars().count() >= aux_budget_chars {
                            break;
                        }
                        if let Some(dep_path_str) = dep_val.as_str() {
                            let dep_full_path = proj_dir.join(dep_path_str);
                            if dep_full_path.exists() {
                                if let Ok(dep_content) = fs::read_to_string(&dep_full_path) {
                                    let dep_block = format!(
                                        "\n--- READ-ONLY Reference File (DO NOT return this file): {} ---\n{}\n-----------------------------------\n",
                                        dep_path_str,
                                        truncate_chars(&dep_content, 900)
                                    );
                                    append_within_budget(&mut aux_context, &dep_block, aux_budget_chars);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let core_user_prompt = user_prompt.clone();
    if !aux_context.is_empty() {
        user_prompt = format!(
            "READ-ONLY Project Contracts & Reference Context (for awareness ONLY — NEVER output or copy these files; ONLY edit '{}'):\n{}\n\n{}",
            rel_file_path,
            aux_context,
            core_user_prompt
        );
    }

    let mut last_error: Option<String> = None;

    for attempt in 0..2u8 {
        let (sys_prompt, cur_user_prompt) = if attempt == 0 {
            (&base_system_prompt, user_prompt.clone())
        } else {
            (
                &full_file_retry_system_prompt,
                format!(
                    "{}\n\nIMPORTANT RETRY INSTRUCTION: Your previous edit could not be applied cleanly ({}). Return ONLY the COMPLETE updated content of target file '{}' from top to bottom (including all imports and exports) with the requested change applied. Do NOT return JSON, a partial fragment, or any other file.",
                    core_user_prompt,
                    last_error.as_deref().unwrap_or("format/target mismatch"),
                    rel_file_path
                ),
            )
        };

        let ollama_req = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": sys_prompt },
                { "role": "user", "content": cur_user_prompt }
            ],
            "stream": false,
            "options": build_ollama_options(&model, 0.2, Some(4096))
        });

        let resp = match post_ollama_chat_with_retry(&client, &ollama_req).await {
            Ok(r) => r,
            Err(e) => {
                return Ok(AIEditResult {
                    success: false,
                    new_content: None,
                    error: Some(format!("Failed to connect to Ollama: {}", e)),
                });
            }
        };

        if !resp.status().is_success() {
            let status_code = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            let clean_msg = if let Ok(err_json) = serde_json::from_str::<Value>(&err_body) {
                err_json.get("error").and_then(|e| e.as_str()).unwrap_or(&err_body).to_string()
            } else {
                err_body
            };
            return Ok(AIEditResult {
                success: false,
                new_content: None,
                error: Some(format!("Ollama Error ({}): {}", status_code, clean_msg)),
            });
        }

        let Ok(data) = resp.json::<Value>().await else {
            return Ok(AIEditResult {
                success: false,
                new_content: None,
                error: Some("Ollama returned an empty response".to_string()),
            });
        };
        let Some(msg_content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) else {
            return Ok(AIEditResult {
                success: false,
                new_content: None,
                error: Some("Ollama returned an empty response".to_string()),
            });
        };

        debug_log_to_file(format!("[AI Edit Attempt {}] Raw AI response:\n{}", attempt + 1, msg_content));
        let clean_content = extract_code_content(msg_content);

        let mut parsed_ok = false;
        let mut final_content = content.clone();
        let mut fail_reason = String::new();

        let is_json_response = (clean_content.trim().starts_with('{')
            || clean_content.contains("\"edits\"")
            || clean_content.contains("\"edit\"")
            || clean_content.contains("\"changes\"")
            || clean_content.contains("\"fixes\"")
            || clean_content.contains("\"fix\"")
            || clean_content.contains("\"target\"")
            || clean_content.contains("\"replacement\"")
            || clean_content.contains("\"explanation\""))
            && (clean_content.contains("\"target\"")
                || clean_content.contains("\"replacement\"")
                || clean_content.contains("\"edits\"")
                || clean_content.contains("\"edit\""));

        let maybe_parsed_json = serde_json::from_str::<Value>(&clean_content)
            .ok()
            .or_else(|| {
                let sanitized_json = clean_content
                    .find('{')
                    .and_then(|s| clean_content.rfind('}').map(|e| &clean_content[s..=e]))
                    .unwrap_or(&clean_content);
                serde_json::from_str::<Value>(sanitized_json).ok()
            });

        if let Some(parsed_json) = maybe_parsed_json {
            let extracted_edits = extract_edits_from_json(&parsed_json, &rel_file_path);
            if !extracted_edits.is_empty() {
                let mut temp_content = content.clone();
                let mut apply_failed = false;

                for edit in extracted_edits {
                    let target = edit.get("target").and_then(|t| t.as_str()).unwrap_or("");
                    let replacement = edit.get("replacement").and_then(|r| r.as_str()).unwrap_or("");
                    match apply_targeted_edit(&temp_content, target, replacement) {
                        Ok(modified) => temp_content = modified,
                        Err(err) => {
                            apply_failed = true;
                            fail_reason = err;
                            break;
                        }
                    }
                }

                if !apply_failed {
                    final_content = post_process_edited_file(&proj_dir, &rel_file_path, &temp_content, true);
                    parsed_ok = true;
                } else {
                    debug_log_to_file(format!("[AI Edit] Targeted edit apply failed: {}", fail_reason));
                }
            }
        }

        if !parsed_ok {
            if is_json_response {
                debug_log_to_file(format!("[AI Edit] Model returned JSON edits but target code could not be matched: {}", fail_reason));
                last_error = Some(format!(
                    "AI model generated targeted edit JSON, but the target code was not matched in the file: {}",
                    fail_reason
                ));
                continue;
            }

            // 1. PRISMA SCHEMA PARTIAL MERGE
            if rel_file_path.ends_with("schema.prisma") {
                debug_log_to_file("[AI Edit] Merging Prisma schema models and enums.".to_string());
                let merged_schema = crate::healer::merge_prisma_ai_response(&content, &clean_content);
                final_content = post_process_edited_file(&proj_dir, &rel_file_path, &merged_schema, false);
            }
            // 2. API ROUTE HANDLER CHECK
            else if (rel_file_path.contains("/api/") || rel_file_path.ends_with("route.ts") || rel_file_path.ends_with("route.js"))
                && (clean_content.contains("export async function GET")
                    || clean_content.contains("export async function POST")
                    || clean_content.contains("export async function PUT")
                    || clean_content.contains("export async function DELETE")
                    || clean_content.contains("export async function PATCH"))
            {
                if clean_content.contains("import ") {
                    final_content = post_process_edited_file(&proj_dir, &rel_file_path, &clean_content, true);
                } else {
                    let methods = ["GET", "POST", "PUT", "DELETE", "PATCH"];
                    let mut replaced_method = false;
                    for m in methods {
                        if clean_content.contains(&format!("export async function {}", m)) {
                            let pattern = format!(r"(?ms)^export\s+async\s+function\s+{}\b[^{{]*\{{.*?\n\}}", m);
                            if let Ok(method_re) = regex::Regex::new(&pattern) {
                                if method_re.is_match(&content) {
                                    let updated = method_re.replace(&content, &clean_content).to_string();
                                    final_content = post_process_edited_file(&proj_dir, &rel_file_path, &updated, true);
                                    replaced_method = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !replaced_method {
                        let updated = format!("{}\n\n{}", content.trim_end(), clean_content.trim());
                        final_content = post_process_edited_file(&proj_dir, &rel_file_path, &updated, true);
                    }
                }
            } else {
                // 3. UI / REACT CODE FILE CHECK + PARTIAL JSX SPLICE
                let is_code_file = rel_file_path.ends_with(".tsx")
                    || rel_file_path.ends_with(".ts")
                    || rel_file_path.ends_with(".jsx")
                    || rel_file_path.ends_with(".js");
                let is_complete_code = !is_code_file
                    || clean_content.contains("export default")
                    || clean_content.contains("export const")
                    || (clean_content.lines().count() >= 15 && clean_content.lines().count() >= (content.lines().count() / 2));

                if is_complete_code {
                    if let Some(guard_err) = detect_wrong_file_overwrite(&rel_file_path, &content, &clean_content) {
                        debug_log_to_file(format!(
                            "[AI Edit Safety Guard] Attempt {} blocked wrong-file overwrite: {}",
                            attempt + 1,
                            guard_err
                        ));
                        last_error = Some(guard_err);
                        continue;
                    }
                    debug_log_to_file("[AI Edit] Accepting full file content.".to_string());
                    final_content = post_process_edited_file(&proj_dir, &rel_file_path, &clean_content, true);
                } else if let Some(spliced) = try_splice_partial_jsx_snippet(&content, &clean_content) {
                    debug_log_to_file("[AI Edit] Spliced partial JSX block using text-anchor matching.".to_string());
                    final_content = post_process_edited_file(&proj_dir, &rel_file_path, &spliced, true);
                } else {
                    // 4. UNIVERSAL SURGICAL OPENING-TAG REPLACEMENT
                    let mut applied_visual_snippet = false;
                    if let Some(ref ctx) = visual_context {
                        if let Some(tag_name) = ctx.get("tagName").and_then(|t| t.as_str()) {
                            let tag_lower = tag_name.to_lowercase();
                            let clean_trimmed = clean_content.trim();
                            if clean_trimmed.to_lowercase().contains(&format!("<{}", tag_lower)) {
                                let element_text = ctx.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
                                let mut matched_range = None;
                                if let Ok(tag_re) = regex::Regex::new(&format!(r#"(?s)<{}\b[^>]*>"#, regex::escape(&tag_lower))) {
                                    for mat in tag_re.find_iter(&content) {
                                        if !element_text.is_empty() {
                                            let after_tag = &content[mat.end()..];
                                            let sample_len = std::cmp::min(120, after_tag.len());
                                            if after_tag[..sample_len].contains(element_text) {
                                                matched_range = Some(mat.range());
                                                break;
                                            }
                                        } else {
                                            matched_range = Some(mat.range());
                                            break;
                                        }
                                    }
                                    if matched_range.is_none() {
                                        matched_range = tag_re.find(&content).map(|m| m.range());
                                    }
                                }
                                if let Some(range) = matched_range {
                                    let repl_tag = if let Some(close_bracket) = clean_trimmed.find('>') {
                                        &clean_trimmed[..=close_bracket]
                                    } else {
                                        clean_trimmed
                                    };
                                    let mut updated = content.clone();
                                    updated.replace_range(range, repl_tag);
                                    final_content = post_process_edited_file(&proj_dir, &rel_file_path, &updated, true);
                                    applied_visual_snippet = true;
                                    debug_log_to_file(format!("[AI Edit] Successfully applied visual tag replacement for <{}>.", tag_lower));
                                }
                            }
                        }
                    }

                    if !applied_visual_snippet {
                        debug_log_to_file(format!(
                            "[AI Edit Safety Guard] Attempt {} returned an un-spliceable partial snippet.",
                            attempt + 1
                        ));
                        last_error = Some(
                            "AI model returned a partial code fragment instead of the full file or targeted JSON edits. Aborted to avoid corrupting file."
                                .to_string(),
                        );
                        continue;
                    }
                }
            }
        }

        if strip_axiom_inspector_script(&final_content).trim() == strip_axiom_inspector_script(&content).trim() {
            debug_log_to_file(format!("[AI Edit] No effective changes produced for '{}'.", rel_file_path));
            return Ok(AIEditResult {
                success: false,
                new_content: None,
                error: Some(format!(
                    "AI produced no effective changes for '{}'. Verify the target file and try describing the exact element or style change.",
                    rel_file_path
                )),
            });
        }

        return Ok(AIEditResult {
            success: true,
            new_content: Some(final_content),
            error: None,
        });
    }

    Ok(AIEditResult {
        success: false,
        new_content: None,
        error: Some(last_error.unwrap_or_else(|| "AI edit could not be applied cleanly.".to_string())),
    })
}

#[derive(Serialize)]
pub struct ProjectEditResult {
    pub success: bool,
    #[serde(rename = "modifiedFiles")]
    pub modified_files: Option<HashMap<String, String>>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn editor_ai_project_edit(
    window: tauri::Window,
    project_id: String,
    prompt: String,
    model_id: Option<String>,
) -> Result<ProjectEditResult, String> {
    editor_ai_project_edit_opt(Some(&window), project_id, prompt, model_id).await
}

pub async fn editor_ai_project_edit_opt(
    window: Option<&tauri::Window>,
    project_id: String,
    prompt: String,
    model_id: Option<String>,
) -> Result<ProjectEditResult, String> {
    let emit_progress = |status: &str, percent: f64| {
        let payload = serde_json::json!({
            "status": status,
            "percent": percent
        });
        if let Some(w) = window {
            let _ = w.emit("editor:project-edit-progress", payload);
        } else {
            println!("__AXIOM_EDIT_PROGRESS__:{}", payload);
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let initial_model = model_id.unwrap_or_else(|| "qwen2.5-coder:7b".to_string());
    let model = resolve_valid_ollama_model(&client, &initial_model).await;
    let proj_dir = get_project_path(&project_id);

    if !proj_dir.exists() {
        return Ok(ProjectEditResult {
            success: false,
            modified_files: None,
            error: Some("Project directory does not exist".into()),
        });
    }

    emit_progress("Scanning project files...", 5.0);

    // Step 1: Scan project files recursively
    let mut files = Vec::new();
    fn scan_dir(dir: &Path, base_dir: &Path, files: &mut Vec<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let path = entry.path();

                    if file_type.is_dir() {
                        if !name.starts_with('.') && name != "node_modules" && name != "build" && name != "dist" {
                            scan_dir(&path, base_dir, files);
                        }
                    } else if let Ok(rel_path) = path.strip_prefix(base_dir) {
                        let rel_str = rel_path.to_string_lossy().replace("\\", "/");
                        if !is_ignored_or_meta_edit_path(&rel_str) {
                            files.push(rel_str);
                        }
                    }
                }
            }
        }
    }
    scan_dir(&proj_dir, &proj_dir, &mut files);

    emit_progress("Routing request to identify affected files...", 10.0);

    // Step 2: Routing Agent (Layer 1)
    let is_expo_proj = crate::templates_expo::is_expo_project_dir(&proj_dir);
    let aux_budget_chars: usize = if get_ollama_runtime_config(&model).num_ctx <= 6144 { 3200 } else { 6400 };
    let files_list_str = files.join("\n");
    let features_map_str = format_features_for_router(&proj_dir, 2400);
    let routing_system_prompt = "You are a Project Router Agent. Select the MINIMAL set of existing file paths from `Project Files` (unless explicitly asked to create a new file) that must change to satisfy the request. \n\
        RULES:\n\
        - For UI, copy, color, styling, or layout tweaks, select ONLY the 1-2 directly relevant UI files (NEVER touch schema.prisma, API routes, or hooks).\n\
        - Include schema/models, API routes, hooks, and UI together ONLY when the request explicitly adds or changes stored data fields, validation, or backend endpoints.\n\
        - Never select `axiom-*.json`, `.axiom/*`, or lockfiles.\n\
        Respond ONLY with a JSON array of relative file paths.";
    let routing_user_prompt = format!(
        "Project Files:\n{}\n{}\nUser Request: {}\n\nJSON array of files to modify or create:",
        files_list_str, features_map_str, prompt
    );

    let ollama_req = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": routing_system_prompt },
            { "role": "user", "content": routing_user_prompt }
        ],
        "stream": false,
        "options": build_ollama_options(&model, 0.1, Some(1024))
    });

    let res = post_ollama_chat_with_retry(&client, &ollama_req)
        .await
        .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

    let mut raw_targets = Vec::new();
    if res.status().is_success() {
        if let Ok(data) = res.json::<Value>().await {
            if let Some(msg_content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                let cleaned = msg_content.trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();

                if let Ok(parsed_files) = serde_json::from_str::<Vec<String>>(cleaned) {
                    raw_targets = parsed_files;
                } else if let (Some(start_idx), Some(end_idx)) = (cleaned.find('['), cleaned.rfind(']')) {
                    if end_idx >= start_idx {
                        if let Ok(parsed_files) = serde_json::from_str::<Vec<String>>(&cleaned[start_idx..=end_idx]) {
                            raw_targets = parsed_files;
                        }
                    }
                }
            }
        }
    }

    let mut target_files = expand_global_theme_targets(
        reconcile_router_target_files(raw_targets, &files, &prompt),
        &files,
        &prompt,
        &proj_dir,
    );
    if !is_global_color_request(&prompt) && target_files.len() > 6 {
        target_files.truncate(6);
    }
    target_files.sort_by_key(|p| target_layer_priority(p));

    if target_files.is_empty() {
        return Ok(ProjectEditResult {
            success: false,
            modified_files: None,
            error: Some("Router agent failed to identify valid target files, or returned empty list".into()),
        });
    }

    let files_found_msg = format!("Routing completed. Identified files: {}", target_files.join(", "));
    emit_progress(&files_found_msg, 20.0);

    // Step 3: Planner & Executor (Layers 2 & 3)
    let mut modified_files_map: HashMap<String, String> = HashMap::new();
    let num_targets = target_files.len() as f64;

    for (idx, file_path) in target_files.iter().enumerate() {
        let file_idx = idx as f64;
        let base_percent = 20.0 + (file_idx / num_targets) * 75.0;
        let step_size = 75.0 / num_targets;

        // Read current content BEFORE planning so Planner sees actual classes & tokens
        let full_path = proj_dir.join(file_path);
        let current_content = if full_path.exists() {
            strip_axiom_inspector_script(&fs::read_to_string(&full_path).unwrap_or_default())
        } else {
            String::new()
        };
        let preview_lines: String = current_content.lines().take(100).collect::<Vec<_>>().join("\n");

        // Build budgeted cross-file & in-session upstream context (focused so UI pages aren't drowned in unrelated backend files)
        let mut session_aux = String::new();
        let mut upstream_keys: Vec<String> = modified_files_map.keys().cloned().collect();
        upstream_keys.sort_by_key(|p| (target_layer_priority(p), p.clone()));
        let mut schema_edited_in_session = false;
        let cur_low = file_path.replace('\\', "/").to_ascii_lowercase();
        let is_data_or_backend_file = cur_low.ends_with(".prisma")
            || cur_low.contains("/api/")
            || cur_low.starts_with("api/")
            || cur_low.contains("/models/")
            || cur_low.starts_with("models/")
            || cur_low.contains("/hooks/")
            || cur_low.starts_with("hooks/")
            || cur_low.contains("/store/")
            || cur_low.starts_with("store/")
            || cur_low.contains("/types/")
            || cur_low.starts_with("types/");
        for up_path in upstream_keys {
            let up_low = up_path.replace('\\', "/").to_ascii_lowercase();
            let is_schema = up_low.ends_with("schema.prisma") || up_low.ends_with(".prisma");
            if is_schema {
                schema_edited_in_session = true;
            }
            let up_stem = up_path
                .rsplit('/')
                .next()
                .unwrap_or(&up_path)
                .trim_end_matches(".tsx")
                .trim_end_matches(".ts")
                .trim_end_matches(".jsx")
                .trim_end_matches(".js");
            let directly_referenced = !up_stem.is_empty()
                && up_stem != "index"
                && up_stem != "route"
                && current_content.contains(up_stem);
            let is_shared_contract = directly_referenced
                || (is_data_or_backend_file
                    && (is_schema
                        || up_low.contains("/types/")
                        || up_low.starts_with("types/")
                        || up_low.contains("/models/")
                        || up_low.starts_with("models/")));
            if is_shared_contract {
                if let Some(up_src) = modified_files_map.get(&up_path) {
                    let block = format!(
                        "\n--- Updated In-Session Dependency ({}) ---\n{}\n",
                        up_path,
                        truncate_chars(up_src, 1100)
                    );
                    append_within_budget(&mut session_aux, &block, aux_budget_chars);
                }
            }
        }

        let contracts_block = crate::orchestrator::format_axiom_contracts_prompt(&proj_dir, file_path);
        if !contracts_block.is_empty() && is_data_or_backend_file {
            append_within_budget(
                &mut session_aux,
                &truncate_chars(&contracts_block, aux_budget_chars / 2),
                aux_budget_chars,
            );
        }
        let reverse_deps = format_reverse_dependents_prompt(&proj_dir, file_path, 550);
        if !reverse_deps.is_empty() {
            append_within_budget(&mut session_aux, &reverse_deps, aux_budget_chars);
        }
        if !is_expo_proj && !schema_edited_in_session && is_data_or_backend_file {
            let prisma_sum = crate::self_healing::extract_prisma_schema_summary_for_healing(&proj_dir);
            if !prisma_sum.is_empty() {
                append_within_budget(&mut session_aux, &truncate_chars(&prisma_sum, 900), aux_budget_chars);
            }
        }

        // Progress: Planning
        let plan_status = format!("Planning changes for {}...", file_path);
        emit_progress(&plan_status, base_percent + step_size * 0.3);

        // Layer 2: Planning Agent (file-grounded + contract-grounded)
        let other_files: Vec<String> = target_files.iter()
            .filter(|f| *f != file_path)
            .cloned()
            .collect();
        let other_files_str = if other_files.is_empty() { "None".to_string() } else { other_files.join(", ") };

        let planning_system_prompt = "You are a Project Planner Agent. Inspect the provided file preview, contracts, and dependencies, then write a concise, concrete edit directive specifying the exact classes, CSS variables, types, API fields, or blocks to change to fulfill the user request.";
        let planning_user_prompt = format!(
            "Target File: {}\nOther affected files: {}\n{}Current File Preview (first 100 lines):\n```\n{}\n```\n\nUser Request: {}\n\nCreate a clear, file-grounded instruction of what needs to be changed in this file.",
            file_path, other_files_str, session_aux, preview_lines, prompt
        );

        let plan_req = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": planning_system_prompt },
                { "role": "user", "content": planning_user_prompt }
            ],
            "stream": false,
            "options": build_ollama_options(&model, 0.2, Some(1024))
        });

        let plan_res = post_ollama_chat_with_retry(&client, &plan_req)
            .await
            .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

        let mut directive = format!("Modify the file to fulfill: {}", prompt);
        if plan_res.status().is_success() {
            if let Ok(data) = plan_res.json::<Value>().await {
                if let Some(content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                    directive = content.to_string();
                }
            }
        }

        // Progress: Coding/Executing
        let code_status = format!("Generating code for {}...", file_path);
        emit_progress(&code_status, base_percent + step_size * 0.6);

        // Layer 3: Executor/Coder Agent (without injecting original design_archetype so user styles win)
        let mut coder_system_prompt = "You are an expert assistant. You are helping the user edit a source code file. \
                                   Provide ONLY the modified full content of the file. Do not include any introductory or concluding text. \
                                   Do not wrap the code in markdown code blocks unless the file itself is a markdown file. Just return the raw code.\n\
                                   CRITICAL: NEVER manually declare `data-axiom-component` or `data-axiom-file` in the code.".to_string();
        let rulepacks = get_stack_rulepacks(file_path, &current_content, None);
        if !rulepacks.is_empty() {
            coder_system_prompt.push_str(&rulepacks);
        }
        let coder_aux_note = if session_aux.is_empty() {
            String::new()
        } else {
            format!("\nCross-File Contracts & Dependencies:\n{}", truncate_chars(&session_aux, aux_budget_chars / 2))
        };
        let coder_user_prompt = format!(
            "Original File Path: {}\n{}Original File Content:\n```\n{}\n```\n\nDirective / Instructions: {}\n\nReturn only the complete updated file content.",
            file_path, coder_aux_note, current_content, directive
        );

        let coder_req = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": coder_system_prompt },
                { "role": "user", "content": coder_user_prompt }
            ],
            "stream": false,
            "options": build_ollama_options(&model, 0.2, Some(4096))
        });

        let coder_res = post_ollama_chat_with_retry(&client, &coder_req)
            .await
            .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

        if coder_res.status().is_success() {
            if let Ok(data) = coder_res.json::<Value>().await {
                if let Some(msg_content) = data.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                    let cleaned = extract_code_content(msg_content);
                    let processed = post_process_edited_file(
                        &proj_dir,
                        file_path,
                        &cleaned,
                        !file_path.ends_with("schema.prisma"),
                    );
                    if strip_axiom_inspector_script(&processed).trim() != current_content.trim() {
                        modified_files_map.insert(file_path.clone(), processed);
                    } else {
                        debug_log_to_file(format!(
                            "[AI Project Edit] Skipping unchanged file '{}' (no effective diff).",
                            file_path
                        ));
                    }
                }
            }
        }
    }

    emit_progress("Done generating project changes!", 100.0);

    if modified_files_map.is_empty() {
        return Ok(ProjectEditResult {
            success: false,
            modified_files: None,
            error: Some("AI produced no effective changes in any selected file.".into()),
        });
    }

    Ok(ProjectEditResult {
        success: true,
        modified_files: Some(modified_files_map),
        error: None,
    })
}

