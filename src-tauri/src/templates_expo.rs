//! Deterministic Expo SDK 52 infrastructure templates.
//!
//! Renders reproducible Expo Router / React Native Web configuration, navigation shells,
//! HTML Live Edit inspector bridge (`app/+html.tsx`), and design tokens (`src/theme.ts`).
//! Screens, custom components, hooks, and stores remain 100% LLM-authored so both
//! compact 7B models and frontier models generate creative, working mobile apps.

use std::path::Path;

use crate::templates::{templates_enabled, DeterministicFile};

pub fn is_expo_project_type(project_type: &str) -> bool {
    matches!(
        project_type.trim().to_lowercase().as_str(),
        "expo" | "react_native" | "react-native"
    )
}

/// Returns `true` when `proj_dir/package.json` explicitly targets Expo / Expo Router
/// and is NOT a Next.js workspace (protecting legacy web/hybrid workspaces).
pub fn is_expo_project_dir(proj_dir: &Path) -> bool {
    let pkg_path = proj_dir.join("package.json");
    if !pkg_path.is_file() {
        return false;
    }
    let Ok(raw) = std::fs::read_to_string(&pkg_path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };

    let deps = json.get("dependencies").and_then(|v| v.as_object());
    let dev_deps = json.get("devDependencies").and_then(|v| v.as_object());

    if deps.map_or(false, |d| d.contains_key("next")) {
        return false;
    }

    let has_expo_dep = deps.map_or(false, |d| d.contains_key("expo") || d.contains_key("expo-router"))
        || dev_deps.map_or(false, |d| d.contains_key("expo") || d.contains_key("expo-router"));

    has_expo_dep
}

/// Returns locked, mutually compatible Expo SDK 52 dependencies, devDependencies,
/// entrypoint, and CLI scripts tuned for in-Editor Metro Web preview + Expo Go.
pub fn expo_default_dependencies() -> serde_json::Value {
    serde_json::json!({
        "main": "expo-router/entry",
        "scripts": {
            "start": "expo start",
            "web": "expo start --web --port 8081 --non-interactive",
            "android": "expo start --android",
            "ios": "expo start --ios",
            "export:web": "expo export --platform web"
        },
        "dependencies": {
            "expo": "~52.0.0",
            "expo-constants": "~17.0.3",
            "expo-linking": "~7.0.3",
            "expo-router": "~4.0.9",
            "expo-status-bar": "~2.0.0",
            "react": "18.3.1",
            "react-dom": "18.3.1",
            "react-native": "0.76.5",
            "react-native-web": "~0.19.13",
            "react-native-safe-area-context": "4.12.0",
            "react-native-screens": "~4.4.0",
            "@expo/vector-icons": "^14.0.4",
            "@react-native-async-storage/async-storage": "1.23.1",
            "@expo/metro-runtime": "~4.0.0",
            "query-string": "^7.1.3"
        },
        "devDependencies": {
            "@babel/core": "^7.25.2",
            "@types/react": "~18.3.12",
            "typescript": "~5.3.3"
        }
    })
}

fn read_project_display_name(proj_dir: &Path) -> String {
    if let Some(cp) = crate::orchestrator::load_generation_checkpoint(proj_dir) {
        if let Some(name) = cp.get("name").and_then(|v| v.as_str()) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
    if let Ok(raw) = std::fs::read_to_string(&lbg_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(name) = json
                .get("projectSpecification")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
            {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    "Axiom Mobile App".to_string()
}

fn to_expo_slug(raw: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in raw.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "axiom-mobile-app".to_string()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn to_expo_scheme(slug: &str) -> String {
    let mut out: String = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if out.is_empty() || !out.chars().next().unwrap_or('a').is_ascii_alphabetic() {
        out = format!("app{}", out);
    }
    out.chars().take(24).collect()
}

fn render_expo_package_json(proj_dir: &Path) -> String {
    let name = read_project_display_name(proj_dir);
    let slug = to_expo_slug(&name);
    let defaults = expo_default_dependencies();
    let pkg = serde_json::json!({
        "name": slug,
        "version": "1.0.0",
        "private": true,
        "main": defaults["main"],
        "scripts": defaults["scripts"],
        "dependencies": defaults["dependencies"],
        "devDependencies": defaults["devDependencies"],
    });
    serde_json::to_string_pretty(&pkg).unwrap_or_else(|_| "{}".to_string()) + "\n"
}

fn render_expo_app_json(proj_dir: &Path) -> String {
    let name = read_project_display_name(proj_dir);
    let slug = to_expo_slug(&name);
    let scheme = to_expo_scheme(&slug);
    let cfg = serde_json::json!({
        "expo": {
            "name": name,
            "slug": slug,
            "scheme": scheme,
            "version": "1.0.0",
            "orientation": "portrait",
            "userInterfaceStyle": "automatic",
            "web": {
                "bundler": "metro",
                "output": "single"
            },
            "plugins": ["expo-router"],
            "experiments": {
                "typedRoutes": false,
                "tsconfigPaths": true
            }
        }
    });
    serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".to_string()) + "\n"
}

const BABEL_CONFIG_TEMPLATE: &str = r#"module.exports = function (api) {
  api.cache(true);
  return {
    presets: ['babel-preset-expo'],
  };
};
"#;

const METRO_CONFIG_TEMPLATE: &str = r#"const { getDefaultConfig } = require('expo/metro-config');

const config = getDefaultConfig(__dirname);

module.exports = config;
"#;

const EXPO_TSCONFIG_TEMPLATE: &str = r#"{
  "extends": "expo/tsconfig.base",
  "compilerOptions": {
    "strict": false,
    "skipLibCheck": true,
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*", "./*"]
    }
  },
  "include": ["**/*.ts", "**/*.tsx"],
  "exclude": ["node_modules"]
}
"#;

const EXPO_ROOT_LAYOUT_TEMPLATE: &str = r#"import React, { useEffect } from 'react';
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { installAxiomWebInspector } from '../src/axiomInspector';

export default function RootLayout() {
  useEffect(() => {
    installAxiomWebInspector();
  }, []);

  return (
    <SafeAreaProvider>
      <StatusBar style="auto" />
      <Stack screenOptions={{ headerShown: false }}>
        <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
      </Stack>
    </SafeAreaProvider>
  );
}
"#;

pub fn extract_axiom_inspector_js() -> String {
    let raw = crate::sanitizer::get_axiom_inspector_script();
    let start_marker = "__html: `";
    let end_marker = "` }} />";
    if let Some(s_idx) = raw.find(start_marker) {
        let after = &raw[s_idx + start_marker.len()..];
        if let Some(e_idx) = after.rfind(end_marker) {
            return after[..e_idx].trim().to_string();
        }
    }
    String::new()
}

pub fn render_expo_web_inspector_ts() -> String {
    let js_body = extract_axiom_inspector_js();
    let js_literal = serde_json::to_string(&js_body).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "import {{ Platform }} from 'react-native';\n\n\
const AXIOM_INSPECTOR_JS: string = {};\n\n\
export function installAxiomWebInspector(): void {{\n  \
if (Platform.OS !== 'web' || !AXIOM_INSPECTOR_JS) return;\n  \
const w = (globalThis as any).window;\n  \
const d = (globalThis as any).document;\n  \
if (!w || !d || w.__axiom_inspect_injected) return;\n  \
const s = d.createElement('script');\n  \
s.textContent = AXIOM_INSPECTOR_JS;\n  \
(d.head || d.documentElement)?.appendChild(s);\n\
}}\n",
        js_literal
    )
}

pub fn render_expo_html_shell() -> String {
    let inspector_script = crate::sanitizer::get_axiom_inspector_script();
    format!(
        "import React, {{ type PropsWithChildren }} from 'react';\n\
import {{ ScrollViewStyleReset }} from 'expo-router/html';\n\n\
export default function RootHtml({{ children }}: PropsWithChildren) {{\n  \
return (\n    \
<html lang=\"en\">\n      \
<head>\n        \
<meta charSet=\"utf-8\" />\n        \
<meta httpEquiv=\"X-UA-Compatible\" content=\"IE=edge\" />\n        \
<meta\n          \
name=\"viewport\"\n          \
content=\"width=device-width, initial-scale=1, shrink-to-fit=no\"\n        \
/>\n        \
<ScrollViewStyleReset />\n      \
</head>\n      \
<body>\n        \
{{children}}\n\
{}\n      \
</body>\n    \
</html>\n  \
);\n\
}}\n",
        inspector_script
    )
}

pub fn ensure_expo_html_inspector_injected(proj_dir: &Path) {
    let inspector_ts_path = proj_dir.join("src").join("axiomInspector.ts");
    if let Some(parent) = inspector_ts_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let needs_ts = std::fs::read_to_string(&inspector_ts_path).map_or(true, |c| {
        !c.contains("axiom-agent-dom-cmd")
            || !c.contains("findAxiomOwner")
            || !c.contains("installAxiomWebInspector")
    });
    if needs_ts {
        let _ = std::fs::write(&inspector_ts_path, render_expo_web_inspector_ts());
    }

    let html_path = proj_dir.join("app").join("+html.tsx");
    if let Some(parent) = html_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if !html_path.exists() {
        let _ = std::fs::write(&html_path, render_expo_html_shell());
    } else if let Ok(content) = std::fs::read_to_string(&html_path) {
        let needs_upgrade = !content.contains("axiom-agent-dom-cmd")
            || !content.contains("findAxiomOwner")
            || !content.contains("extractProjectFileFromStack");
        if needs_upgrade {
            let cleaned = crate::sanitizer::strip_axiom_inspector_script(&content);
            let script = crate::sanitizer::get_axiom_inspector_script();
            let updated = if cleaned.contains("</body>") {
                cleaned.replace("</body>", &format!("{}\n      </body>", script))
            } else {
                render_expo_html_shell()
            };
            let _ = std::fs::write(&html_path, updated);
        }
    }

    let root_layout_path = proj_dir.join("app").join("_layout.tsx");
    if let Ok(layout_src) = std::fs::read_to_string(&root_layout_path) {
        if !layout_src.contains("installAxiomWebInspector") {
            let _ = std::fs::write(&root_layout_path, EXPO_ROOT_LAYOUT_TEMPLATE);
        }
    }
}

fn read_lbg_theme_override(proj_dir: &Path) -> (String, String, String, String, String) {
    let mut background = "#0f172a".to_string();
    let mut surface = "#1e293b".to_string();
    let mut primary = "#6366f1".to_string();
    let mut text = "#f8fafc".to_string();
    let mut border = "#334155".to_string();

    let lbg_path = proj_dir.join(".axiom").join("live_builder_guide.json");
    if let Ok(raw) = std::fs::read_to_string(&lbg_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
            let ds = json
                .get("designSystem")
                .or_else(|| json.get("palette"))
                .or_else(|| json.get("projectSpecification").and_then(|p| p.get("palette")));
            if let Some(obj) = ds.and_then(|v| v.as_object()) {
                if let Some(v) = obj.get("background").and_then(|s| s.as_str()).filter(|s| s.starts_with('#')) {
                    background = v.to_string();
                }
                if let Some(v) = obj.get("surface").and_then(|s| s.as_str()).filter(|s| s.starts_with('#')) {
                    surface = v.to_string();
                }
                if let Some(v) = obj.get("primary").and_then(|s| s.as_str()).filter(|s| s.starts_with('#')) {
                    primary = v.to_string();
                }
                if let Some(v) = obj.get("text").and_then(|s| s.as_str()).filter(|s| s.starts_with('#')) {
                    text = v.to_string();
                }
                if let Some(v) = obj.get("border").and_then(|s| s.as_str()).filter(|s| s.starts_with('#')) {
                    border = v.to_string();
                }
            }
        }
    }
    (background, surface, primary, text, border)
}

fn render_expo_theme_ts(proj_dir: &Path) -> String {
    let (bg, surface, primary, text, border) = read_lbg_theme_override(proj_dir);
    format!(
        "export const theme = {{\n  \
colors: {{\n    \
background: '{}',\n    \
surface: '{}',\n    \
surfaceAlt: '#273549',\n    \
primary: '{}',\n    \
primaryText: '#ffffff',\n    \
secondary: '#22d3ee',\n    \
text: '{}',\n    \
textMuted: '#94a3b8',\n    \
border: '{}',\n    \
success: '#22c55e',\n    \
warning: '#f59e0b',\n    \
danger: '#ef4444',\n  \
}},\n  \
spacing: {{\n    \
xs: 4,\n    \
sm: 8,\n    \
md: 16,\n    \
lg: 24,\n    \
xl: 32,\n  \
}},\n  \
radius: {{\n    \
sm: 8,\n    \
md: 14,\n    \
lg: 20,\n    \
full: 9999,\n  \
}},\n  \
typography: {{\n    \
title: {{ fontSize: 24, fontWeight: '700' as const }},\n    \
subtitle: {{ fontSize: 17, fontWeight: '600' as const }},\n    \
body: {{ fontSize: 15, fontWeight: '400' as const }},\n    \
caption: {{ fontSize: 12, fontWeight: '500' as const }},\n  \
}},\n\
}} as const;\n\n\
export type AppTheme = typeof theme;\n\
export default theme;\n",
        bg, surface, primary, text, border
    )
}

fn title_case_tab(stem: &str) -> String {
    if stem.eq_ignore_ascii_case("index") {
        return "Home".to_string();
    }
    stem.split(['-', '_'])
        .filter(|p| !p.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn icon_for_tab(stem: &str) -> &'static str {
    let low = stem.to_lowercase();
    if low == "index" || low == "home" || low == "dashboard" {
        "home"
    } else if low.contains("explore") || low.contains("discover") || low.contains("search") {
        "compass"
    } else if low.contains("analytics") || low.contains("stats") || low.contains("report") {
        "bar-chart"
    } else if low.contains("calendar") || low.contains("schedule") || low.contains("booking") {
        "calendar"
    } else if low.contains("cart") || low.contains("shop") || low.contains("order") {
        "cart"
    } else if low.contains("chat") || low.contains("message") || low.contains("inbox") {
        "chatbubbles"
    } else if low.contains("profile") || low.contains("account") || low.contains("user") {
        "person"
    } else if low.contains("setting") || low.contains("config") {
        "settings"
    } else if low.contains("favorite") || low.contains("health") || low.contains("fitness") || low.contains("workout") {
        "fitness"
    } else if low.contains("wallet") || low.contains("finance") || low.contains("budget") {
        "wallet"
    } else {
        "list"
    }
}

fn discover_tab_screens(proj_dir: &Path) -> Vec<String> {
    let mut tabs: Vec<String> = Vec::new();
    let mut push_tab = |raw: &str| {
        let stem = raw
            .trim()
            .trim_end_matches(".tsx")
            .trim_end_matches(".jsx")
            .trim_end_matches(".ts")
            .trim_end_matches(".js");
        if stem.is_empty() || stem == "_layout" || stem.starts_with('[') || stem.contains('/') {
            return;
        }
        if !tabs.iter().any(|t| t.eq_ignore_ascii_case(stem)) {
            tabs.push(stem.to_string());
        }
    };

    if let Some(cp) = crate::orchestrator::load_generation_checkpoint(proj_dir) {
        if let Some(arr) = cp.get("filesToGenerate").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(p) = item.get("path").and_then(|v| v.as_str()) {
                    let norm = p.replace('\\', "/");
                    if let Some(rest) = norm.strip_prefix("app/(tabs)/") {
                        push_tab(rest);
                    }
                }
            }
        }
    }

    let tabs_dir = proj_dir.join("app").join("(tabs)");
    if let Ok(entries) = std::fs::read_dir(&tabs_dir) {
        let mut disk_names: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        disk_names.sort();
        for name in disk_names {
            if name.ends_with(".tsx") || name.ends_with(".jsx") {
                push_tab(&name);
            }
        }
    }

    if !tabs.iter().any(|t| t == "index") {
        tabs.insert(0, "index".to_string());
    } else if let Some(pos) = tabs.iter().position(|t| t == "index") {
        if pos != 0 {
            let idx = tabs.remove(pos);
            tabs.insert(0, idx);
        }
    }

    tabs
}

fn render_expo_tabs_layout(proj_dir: &Path) -> String {
    let tabs = discover_tab_screens(proj_dir);
    let mut screen_entries = String::new();
    for tab in &tabs {
        let title = title_case_tab(tab);
        let icon = icon_for_tab(tab);
        screen_entries.push_str(&format!(
            "      <Tabs.Screen\n        \
name=\"{}\"\n        \
options={{{{\n          \
title: '{}',\n          \
tabBarIcon: ({{ color, size }}) => (\n            \
<Ionicons name=\"{}\" size={{size}} color={{color}} />\n          \
),\n        \
}}}}\n      />\n",
            tab, title, icon
        ));
    }

    format!(
        "import React from 'react';\n\
import {{ Tabs }} from 'expo-router';\n\
import {{ Ionicons }} from '@expo/vector-icons';\n\
import {{ theme }} from '../../src/theme';\n\n\
export default function TabLayout() {{\n  \
return (\n    \
<Tabs\n      \
screenOptions={{{{\n        \
headerShown: false,\n        \
tabBarActiveTintColor: theme.colors.primary,\n        \
tabBarInactiveTintColor: theme.colors.textMuted,\n        \
tabBarStyle: {{\n          \
backgroundColor: theme.colors.surface,\n          \
borderTopColor: theme.colors.border,\n        \
}},\n      \
}}}}\n    \
>\n\
{}\
    </Tabs>\n  \
);\n\
}}\n",
        screen_entries
    )
}

pub fn render_expo_deterministic_file(
    proj_dir: &Path,
    project_type: &str,
    file_path: &str,
    _desc: &str,
) -> Option<DeterministicFile> {
    if !templates_enabled() || !is_expo_project_type(project_type) {
        return None;
    }

    let p = file_path
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_lowercase();

    let plain = |content: String| {
        Some(DeterministicFile {
            content,
            side_files: Vec::new(),
        })
    };

    match p.as_str() {
        "package.json" => plain(render_expo_package_json(proj_dir)),
        "app.json" => plain(render_expo_app_json(proj_dir)),
        "babel.config.js" => plain(BABEL_CONFIG_TEMPLATE.to_string()),
        "metro.config.js" => plain(METRO_CONFIG_TEMPLATE.to_string()),
        "tsconfig.json" => plain(EXPO_TSCONFIG_TEMPLATE.to_string()),
        "src/theme.ts" | "theme.ts" => plain(render_expo_theme_ts(proj_dir)),
        "src/axiominspector.ts" | "axiominspector.ts" => plain(render_expo_web_inspector_ts()),
        "app/+html.tsx" => plain(render_expo_html_shell()),
        "app/_layout.tsx" => Some(DeterministicFile {
            content: EXPO_ROOT_LAYOUT_TEMPLATE.to_string(),
            side_files: vec![
                ("src/axiomInspector.ts".to_string(), render_expo_web_inspector_ts()),
                ("app/+html.tsx".to_string(), render_expo_html_shell()),
            ],
        }),
        "app/(tabs)/_layout.tsx" => Some(DeterministicFile {
            content: render_expo_tabs_layout(proj_dir),
            side_files: vec![("src/theme.ts".to_string(), render_expo_theme_ts(proj_dir))],
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiom_expo_test_{}_{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_expo_deterministic_templates_render_valid_configs() {
        let dir = make_temp_dir("render");
        crate::orchestrator::save_generation_checkpoint(
            &dir,
            &serde_json::json!({
                "name": "FitPulse Tracker",
                "filesToGenerate": [
                    { "path": "app/(tabs)/index.tsx" },
                    { "path": "app/(tabs)/workouts.tsx" },
                    { "path": "app/(tabs)/settings.tsx" }
                ]
            }),
        );

        let pkg = render_expo_deterministic_file(&dir, "expo", "package.json", "").unwrap();
        let pkg_json: serde_json::Value = serde_json::from_str(&pkg.content).unwrap();
        assert_eq!(pkg_json["main"], "expo-router/entry");
        assert!(pkg_json["scripts"]["web"].as_str().unwrap().contains("8081"));
        assert!(pkg_json["dependencies"]["expo"].is_string());
        assert!(pkg_json["dependencies"]["expo-router"].is_string());
        assert!(pkg_json["dependencies"].get("next").is_none());

        let app_cfg = render_expo_deterministic_file(&dir, "expo", "app.json", "").unwrap();
        let app_json: serde_json::Value = serde_json::from_str(&app_cfg.content).unwrap();
        assert_eq!(app_json["expo"]["slug"], "fitpulse-tracker");
        assert_eq!(app_json["expo"]["web"]["bundler"], "metro");

        assert!(render_expo_deterministic_file(&dir, "expo", "babel.config.js", "").is_some());
        assert!(render_expo_deterministic_file(&dir, "expo", "metro.config.js", "").is_some());

        let ts = render_expo_deterministic_file(&dir, "expo", "tsconfig.json", "").unwrap();
        let ts_json: serde_json::Value = serde_json::from_str(&ts.content).unwrap();
        assert_eq!(ts_json["extends"], "expo/tsconfig.base");

        let root_layout = render_expo_deterministic_file(&dir, "expo", "app/_layout.tsx", "").unwrap();
        assert!(root_layout.content.contains("SafeAreaProvider"));
        assert!(root_layout.content.contains("installAxiomWebInspector"));
        let html_side = root_layout.side_files.iter().find(|(p, _)| p == "app/+html.tsx").unwrap();
        assert!(html_side.1.contains("findAxiomOwner"));
        assert!(html_side.1.contains("axiom-agent-dom-cmd"));
        let ts_side = root_layout.side_files.iter().find(|(p, _)| p == "src/axiomInspector.ts").unwrap();
        assert!(ts_side.1.contains("findAxiomOwner"));

        let tabs = render_expo_deterministic_file(&dir, "expo", "app/(tabs)/_layout.tsx", "").unwrap();
        assert!(tabs.content.contains("name=\"index\""));
        assert!(tabs.content.contains("name=\"workouts\""));
        assert!(tabs.content.contains("name=\"settings\""));
        assert_eq!(tabs.side_files[0].0, "src/theme.ts");

        // LLM-owned screens must NOT be intercepted by templates
        assert!(render_expo_deterministic_file(&dir, "expo", "app/(tabs)/index.tsx", "").is_none());
        assert!(render_expo_deterministic_file(&dir, "expo", "src/components/WorkoutCard.tsx", "").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_expo_project_dir_isolates_hybrids() {
        let expo_dir = make_temp_dir("pure_expo");
        let _ = std::fs::write(
            expo_dir.join("package.json"),
            r#"{"dependencies":{"expo":"~52.0.0","expo-router":"~4.0.9","react":"18.3.1"}}"#,
        );
        assert!(is_expo_project_dir(&expo_dir));

        let next_dir = make_temp_dir("pure_next");
        let _ = std::fs::write(
            next_dir.join("package.json"),
            r#"{"dependencies":{"next":"14.2.35","react":"^18.2.0"}}"#,
        );
        assert!(!is_expo_project_dir(&next_dir));

        let hybrid_dir = make_temp_dir("hybrid");
        let _ = std::fs::write(
            hybrid_dir.join("package.json"),
            r#"{"dependencies":{"next":"14.2.35","expo":"~52.0.0"}}"#,
        );
        assert!(!is_expo_project_dir(&hybrid_dir));

        let _ = std::fs::remove_dir_all(&expo_dir);
        let _ = std::fs::remove_dir_all(&next_dir);
        let _ = std::fs::remove_dir_all(&hybrid_dir);
    }

    #[test]
    fn test_detect_project_type_expo_and_mobile() {
        let m1 = serde_json::json!({ "resolvedStack": "React Native (Expo)" });
        assert_eq!(crate::sanitizer::detect_project_type(&m1), "expo");

        let m2 = serde_json::json!({ "platform": "mobile" });
        assert_eq!(crate::sanitizer::detect_project_type(&m2), "expo");

        let m3 = serde_json::json!({ "projectType": "react_native" });
        assert_eq!(crate::sanitizer::detect_project_type(&m3), "expo");

        let m4 = serde_json::json!({ "resolvedStack": "Flutter 3" });
        assert_eq!(crate::sanitizer::detect_project_type(&m4), "flutter");

        let m5 = serde_json::json!({ "resolvedStack": "Next.js 14" });
        assert_eq!(crate::sanitizer::detect_project_type(&m5), "nextjs");
    }

    #[test]
    fn test_build_default_metamanifest_expo_is_pure() {
        let mm = crate::orchestrator::build_default_metamanifest("expo");
        assert_eq!(mm["framework"], "expo");
        let serialized = mm.to_string();
        assert!(serialized.contains("app/_layout.tsx"));
        assert!(serialized.contains("app/+html.tsx"));
        assert!(serialized.contains("app/(tabs)/_layout.tsx"));
        assert!(serialized.contains("src/theme.ts"));
        assert!(!serialized.contains("\"next\""));
        assert!(!serialized.contains("next-auth"));
        assert!(!serialized.contains("tailwindcss"));
        assert!(!serialized.contains("prisma"));
    }

    #[test]
    fn test_expo_hygiene_and_package_repair_do_not_inject_nextjs() {
        let dir = make_temp_dir("hygiene");
        let _ = std::fs::create_dir_all(dir.join("app/(tabs)"));
        let _ = std::fs::write(
            dir.join("package.json"),
            r#"{"name":"demo-mobile","dependencies":{"expo":"~52.0.0"}}"#,
        );
        let _ = std::fs::write(
            dir.join("app/(tabs)/index.tsx"),
            "\"use client\";\nimport React from 'react';\nimport { Pressable, Text } from 'react-native';\nexport default function Home() {\n  return <Pressable onClick={() => {}}><Text>Go</Text></Pressable>;\n}\n",
        );

        crate::self_healing::run_hygiene_pass_on_project(&dir);
        crate::packages::repair_package_json_dependencies(&dir);

        assert!(!dir.join("src/app/page.tsx").exists());
        assert!(!dir.join("tailwind.config.js").exists());

        let pkg = std::fs::read_to_string(dir.join("package.json")).unwrap();
        let pkg_json: serde_json::Value = serde_json::from_str(&pkg).unwrap();
        assert_eq!(pkg_json["main"], "expo-router/entry");
        assert!(pkg_json["dependencies"].get("next").is_none());
        assert!(pkg_json["dependencies"].get("tailwindcss").is_none());
        assert!(pkg_json["dependencies"].get("expo-router").is_some());
        assert!(pkg_json["dependencies"].get("query-string").is_some());

        let index_src = std::fs::read_to_string(dir.join("app/(tabs)/index.tsx")).unwrap();
        assert!(!index_src.contains("\"use client\""));
        assert!(index_src.contains("onPress="));
        assert!(!index_src.contains("onClick="));
        assert!(dir.join("app/+html.tsx").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extract_file_path_metro_expo_route() {
        let err1 = "Unable to resolve \"./Missing\" from \"app/(tabs)/index.tsx\"";
        assert_eq!(
            crate::server::extract_file_path_from_line(err1).as_deref(),
            Some("app/(tabs)/index.tsx")
        );

        let err2 = "SyntaxError: app/(tabs)/settings: Unexpected token (14:2)";
        assert_eq!(
            crate::server::extract_file_path_from_line(err2).as_deref(),
            Some("app/(tabs)/settings")
        );
    }

    #[test]
    fn test_parse_tsc_diagnostics_handles_expo_route_groups() {
        let tsc = concat!(
            "app/(tabs)/defects.tsx(9,31): error TS2339: Property 'x' does not exist on type 'Y'.\n",
            "app/(tabs)/editInspection/[id].tsx(30,32): error TS2304: Cannot find name 'AsyncStorage'.\n",
            "src/components/Card.tsx(5,3): error TS2322: Bad prop.\n"
        );
        let parsed = crate::self_healing::parse_tsc_diagnostics(tsc);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].file, "app/(tabs)/defects.tsx");
        assert_eq!(parsed[0].line, 9);
        assert_eq!(parsed[0].code, "TS2339");
        assert_eq!(parsed[1].file, "app/(tabs)/editInspection/[id].tsx");
        assert_eq!(parsed[1].code, "TS2304");
        assert_eq!(parsed[2].file, "src/components/Card.tsx");
    }

    #[test]
    fn test_expo_generation_phase_orders_theme_types_store_before_screens() {
        use crate::orchestrator::get_generation_phase;
        let p_theme = get_generation_phase("src/theme.ts", "expo");
        let p_types = get_generation_phase("src/types/index.ts", "expo");
        let p_store = get_generation_phase("src/store/useSiteStore.ts", "expo");
        let p_comp = get_generation_phase("src/components/InspectionCard.tsx", "expo");
        let p_layout = get_generation_phase("app/(tabs)/_layout.tsx", "expo");
        let p_screen = get_generation_phase("app/(tabs)/index.tsx", "expo");
        assert!(p_theme < p_types);
        assert!(p_types < p_store);
        assert!(p_store < p_comp);
        assert!(p_comp < p_layout);
        assert!(p_layout < p_screen);
    }

    #[test]
    fn test_sanitize_expo_strips_next_imports_and_adds_missing_rn_primitives() {
        let dirty = concat!(
            "\"use client\";\n",
            "import { useRouter } from 'next/navigation';\n",
            "import { useRouter as useExpoRouter } from 'expo-router';\n",
            "import { View, Text } from 'react-native';\n\n",
            "export default function Screen() {\n",
            "  const load = () => AsyncStorage.getItem('k');\n",
            "  return <View style={s.root}><TouchableOpacity onPress={load}><Text>Ok</Text></TouchableOpacity></View>;\n",
            "}\n",
            "const s = StyleSheet.create({ root: { flex: 1 } });\n"
        );
        let cleaned = crate::sanitizer::sanitize_expo_react_native_file(dirty, "app/(tabs)/test.tsx");
        assert!(!cleaned.contains("next/navigation"));
        assert!(cleaned.contains("from 'expo-router'"));
        assert!(cleaned.contains("import AsyncStorage from '@react-native-async-storage/async-storage';"));
        assert!(cleaned.contains("TouchableOpacity"));
        assert!(cleaned.contains("StyleSheet"));
    }

    #[test]
    fn test_normalize_expo_strips_src_api_routes() {
        let mut files = vec![
            serde_json::json!({ "path": "app/(tabs)/index.tsx", "description": "home", "language": "typescript" }),
            serde_json::json!({ "path": "src/api/defects/route.ts", "description": "bad api", "language": "typescript" }),
            serde_json::json!({ "path": "src/app/api/register/route.ts", "description": "bad auth api", "language": "typescript" }),
        ];
        let mm = crate::orchestrator::build_default_metamanifest("expo");
        crate::sanitizer::normalize_project_structure(&mut files, &mm, "expo");
        let paths: Vec<String> = files
            .iter()
            .filter_map(|f| f.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();
        assert!(!paths.iter().any(|p| p.contains("route.ts")), "{:?}", paths);
        assert!(paths.iter().any(|p| p == "app/(tabs)/index.tsx"));
    }

    #[test]
    fn test_root_layout_generates_web_inspector_module() {
        let dir = make_temp_dir("web_inspector");
        let root_layout = render_expo_deterministic_file(&dir, "expo", "app/_layout.tsx", "").unwrap();
        assert!(root_layout.content.contains("installAxiomWebInspector"));
        let ts_mod = root_layout
            .side_files
            .iter()
            .find(|(p, _)| p == "src/axiomInspector.ts")
            .expect("missing src/axiomInspector.ts side file");
        assert!(ts_mod.1.contains("export function installAxiomWebInspector"));
        assert!(ts_mod.1.contains("findAxiomOwner"));
        assert!(ts_mod.1.contains("axiom-inspect-click"));
        assert!(ts_mod.1.contains("axiom-agent-dom-cmd"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sanitize_expo_fixes_async_storage_and_adds_dual_exports() {
        let store_default_only = concat!(
            "import AsyncStorage from '@react-native-async-storage/async-storage';\n",
            "export default function useFitnessStore() {\n",
            "  const load = async () => AsyncStorage.getItemAsync('k');\n",
            "  const save = async (v: string) => AsyncStorage.setItemAsync('k', v);\n",
            "  return { load, save };\n",
            "}\n"
        );
        let fixed_a = crate::sanitizer::sanitize_expo_react_native_file(store_default_only, "src/store/useFitnessStore.ts");
        assert!(fixed_a.contains("AsyncStorage.getItem('k')"));
        assert!(fixed_a.contains("AsyncStorage.setItem('k', v)"));
        assert!(fixed_a.contains("export { useFitnessStore };"));

        let comp_named_only = concat!(
            "import React from 'react';\n",
            "import { View, Text } from 'react-native';\n",
            "export function WorkoutCard({ title }: { title: string }) {\n",
            "  return <View style={{ backgroundColor: theme.colors.primary }}><Text>{title}</Text></View>;\n",
            "}\n"
        );
        let fixed_b = crate::sanitizer::sanitize_expo_react_native_file(comp_named_only, "src/components/WorkoutCard.tsx");
        assert!(fixed_b.contains("export default WorkoutCard;"));
        assert!(fixed_b.contains("import { theme } from '@/theme';"));
    }

    #[test]
    fn test_strip_metadata_from_client_layout_only_targets_client_layouts() {
        let client_layout = concat!(
            "'use client';\n",
            "import './globals.css';\n",
            "import { Metadata } from 'next';\n",
            "import { SessionProvider } from 'next-auth/react';\n\n",
            "export const metadata: Metadata = {\n",
            "  title: 'MedFlow Clinic Scheduler',\n",
            "  description: 'Medical SaaS',\n",
            "};\n\n",
            "export default function RootLayout({ children }: { children: React.ReactNode }) {\n",
            "  return <html><body><SessionProvider>{children}</SessionProvider></body></html>;\n",
            "}\n"
        );
        let stripped = crate::sanitizer::strip_metadata_from_client_layout(client_layout, "src/app/layout.tsx");
        assert!(!stripped.contains("export const metadata"));
        assert!(!stripped.contains("from 'next'"));
        assert!(stripped.contains("<SessionProvider>{children}</SessionProvider>"));

        let server_layout = concat!(
            "import type { Metadata } from 'next';\n",
            "export const metadata: Metadata = {\n",
            "  title: 'Server Root',\n",
            "};\n",
            "export default function RootLayout({ children }: { children: React.ReactNode }) {\n",
            "  return <html><body>{children}</body></html>;\n",
            "}\n"
        );
        let untouched = crate::sanitizer::strip_metadata_from_client_layout(server_layout, "src/app/layout.tsx");
        assert_eq!(untouched, server_layout);
    }

    #[test]
    fn test_sanitize_nextjs_fixes_react_icons_zod_time_and_generic_hook_cast() {
        let navbar = "import HiMenu from 'react-icons/hi';\nexport default function Navbar() { return <HiMenu />; }\n";
        let fixed_nav = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(navbar, "src/components/Navbar.tsx");
        assert!(fixed_nav.contains("import { HiMenu } from 'react-icons/hi';"));

        let api = concat!(
            "import { z } from 'zod';\n",
            "const schema = z.object({\n",
            "  day: z.date().coerce.date().optional(),\n",
            "  start: z.time().coerce.string(),\n",
            "  end: z.time(),\n",
            "});\n"
        );
        let fixed_api = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(api, "src/app/api/doctors/route.ts");
        assert!(fixed_api.contains("z.coerce.date()"));
        assert!(fixed_api.contains("z.coerce.string()"));
        assert!(fixed_api.contains("end: z.string()"));
        assert!(!fixed_api.contains("z.time("));

        let hook = "const useAlerts = <T extends CalibrationAlert[]>({ fallbackData = [] as unknown as T }) => { setData(initialFallbackData as T[]); };\n";
        let fixed_hook = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(hook, "src/hooks/useAlerts.ts");
        assert!(fixed_hook.contains("setData(initialFallbackData as T);"));
        assert!(!fixed_hook.contains("as T[]"));
    }

    #[test]
    fn test_sanitize_expo_fixes_ns_asyncstorage_theme_icons_and_pct_dimension() {
        let screen = concat!(
            "import * as AsyncStorage from '@react-native-async-storage/async-storage';\n",
            "import { View, StyleSheet } from 'react-native';\n",
            "import Ionicons from '@expo/vector-icons/Ionicons';\n",
            "export default function Card({ pct }: { pct: number }) {\n",
            "  const load = () => AsyncStorage.getItem('x');\n",
            "  return <View style={{ width: `${pct}%`, padding: theme.spacing.xxs }}><Ionicons name=\"weight-lifter\" /><Ionicons name='heart-pulse' /><Ionicons name='circle-outline' /><Ionicons name='add-circle-outline' /></View>;\n",
            "}\n"
        );
        let fixed = crate::sanitizer::sanitize_expo_react_native_file(screen, "src/components/Card.tsx");
        assert!(fixed.contains("import AsyncStorage from '@react-native-async-storage/async-storage';"));
        assert!(!fixed.contains("* as AsyncStorage"));
        assert!(fixed.contains("theme.spacing.xs"));
        assert!(!fixed.contains("theme.spacing.xxs"));
        assert!(fixed.contains("name=\"barbell-outline\""));
        assert!(fixed.contains("name='heart-outline'"));
        assert!(fixed.contains("name='ellipse-outline'"));
        assert!(fixed.contains("name='add-circle-outline'"));
        assert!(fixed.contains("width: `${pct}%` as any,"));
    }

    #[test]
    fn test_reconcile_expo_types_and_unions_in_project() {
        let dir = std::env::temp_dir().join(format!("axiom-expo-unions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/types")).unwrap();
        std::fs::create_dir_all(dir.join("src/store")).unwrap();
        std::fs::create_dir_all(dir.join("app/(tabs)")).unwrap();

        std::fs::write(
            dir.join("src/types/index.ts"),
            "export type InspectionStatus = 'PASS' | 'ACTION_REQUIRED' | 'PENDING';\nexport type SeverityLevel = 'CRITICAL' | 'MAJOR' | 'MINOR';\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/store/useHabitStore.ts"),
            "export function useHabitStore() { const habits: any[] = []; const addHabit = () => {}; return { habits, addHabit }; }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app/(tabs)/defects.tsx"),
            concat!(
                "import { View } from 'react-native';\n",
                "import { useHabitStore } from '@/store/useHabitStore';\n",
                "export default function DefectsScreen() {\n",
                "  const [habits, addHabit] = useHabitStore();\n",
                "  const levels = Object.values(SeverityLevel);\n",
                "  const initialSev: SeverityLevel = 'CRITICAL';\n",
                "  const status: InspectionStatus = InspectionStatus.PASS;\n",
                "  return <View />;\n",
                "}\n"
            ),
        )
        .unwrap();

        crate::sanitizer::reconcile_expo_types_and_unions_in_project(&dir);
        let updated = std::fs::read_to_string(dir.join("app/(tabs)/defects.tsx")).unwrap();
        assert!(updated.contains("const { habits, addHabit } = useHabitStore();"));
        assert!(updated.contains("(['CRITICAL', 'MAJOR', 'MINOR'] as const)"));
        assert!(updated.contains("const status: InspectionStatus = 'PASS';"));
        assert!(updated.contains("from '@/types'"));
        assert!(updated.contains("SeverityLevel"));
        assert!(updated.contains("InspectionStatus"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_reconcile_expo_resolves_barrel_glob_and_wrong_store_hook_names() {
        let dir = std::env::temp_dir().join(format!("axiom-expo-imports-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/store")).unwrap();
        std::fs::create_dir_all(dir.join("src/components")).unwrap();
        std::fs::create_dir_all(dir.join("app/(tabs)")).unwrap();

        std::fs::write(
            dir.join("src/store/useExpenseStore.ts"),
            "export function useExpenseStore() { return { expenses: [], addExpense: () => {} }; }\nexport default useExpenseStore;\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/components/InspectionCard.tsx"),
            "export function InspectionCard() { return null; }\nexport default InspectionCard;\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/components/StatusPill.tsx"),
            "export function StatusPill() { return null; }\nexport default StatusPill;\n",
        )
        .unwrap();

        std::fs::write(
            dir.join("app/(tabs)/settings.tsx"),
            concat!(
                "import { useExpenseStore, useBudgetStore } from '@/store/use*';\n",
                "import { InspectionCard, StatusPill } from '@/components/ui';\n",
                "import { useWrongNameStore } from '@/store/useExpenseStore';\n",
                "export default function Settings() { return null; }\n"
            ),
        )
        .unwrap();

        crate::sanitizer::reconcile_expo_types_and_unions_in_project(&dir);
        let updated = std::fs::read_to_string(dir.join("app/(tabs)/settings.tsx")).unwrap();
        assert!(!updated.contains("@/store/use*"));
        assert!(!updated.contains("@/components/ui"));
        assert!(updated.contains("from '@/store/useExpenseStore'"));
        assert!(updated.contains("useExpenseStore as useBudgetStore"));
        assert!(updated.contains("from '@/components/InspectionCard'"));
        assert!(updated.contains("from '@/components/StatusPill'"));
        assert!(updated.contains("useExpenseStore as useWrongNameStore"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_strip_bogus_middleware_locale_rewrite_and_multiline_link_anchor() {
        let dir = std::env::temp_dir().join(format!("axiom-mw-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/app/login")).unwrap();

        let mw = concat!(
            "import { NextRequest, NextResponse } from 'next/server';\n",
            "export async function middleware(request: NextRequest) {\n",
            "  const pathname = request.nextUrl.pathname;\n",
            "  if (pathname.startsWith('/dashboard')) {\n",
            "    return NextResponse.redirect(new URL('/login', request.url));\n",
            "  }\n",
            "  const locale = pathname.split('/')[1];\n",
            "  if (locale && !['en', 'es'].includes(locale)) {\n",
            "    return NextResponse.rewrite(new URL(`/${locale}${pathname}`, request.url));\n",
            "  }\n",
            "  return NextResponse.next();\n",
            "}\n"
        );
        let cleaned = crate::sanitizer::strip_bogus_middleware_locale_rewrite(mw, "src/middleware.ts", Some(&dir));
        assert!(!cleaned.contains("NextResponse.rewrite"));
        assert!(!cleaned.contains("const locale"));
        assert!(cleaned.contains("NextResponse.redirect(new URL('/login', request.url))"));

        let link_tsx = concat!(
            "<Link href=\"/dashboard\">Dashboard</Link>\n",
            "<a href=\"https://example.com\">Docs</a>\n",
            "<Link href={`/invoices/${id}`} prefetch={false}>\n",
            "  <a\n",
            "    className=\"card\">\n",
            "    View\n",
            "  </a>\n",
            "</Link>\n"
        );
        let fixed_link = crate::sanitizer::ensure_link_legacy_behavior_for_anchor(link_tsx, "src/app/invoices/page.tsx");
        assert!(fixed_link.contains("<Link href=\"/dashboard\">Dashboard</Link>"));
        assert!(fixed_link.contains("<Link legacyBehavior href={`/invoices/${id}`} prefetch={false}>"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_katastar_blocks_project_local_and_domain_patches() {
        let local_btn = crate::katastar::KatastarPatch {
            search: "import { Button } from '@/components/ui/button';".to_string(),
            replace: "import Button from '@/components/ui/button';".to_string(),
            extensions: vec![".tsx".to_string()],
        };
        assert!(!crate::katastar::is_safe_auto_patch("src/app/page.tsx", &local_btn));

        let local_rel = crate::katastar::KatastarPatch {
            search: "import type { Plant } from './Plant';".to_string(),
            replace: "import type { Plant } from './plant';".to_string(),
            extensions: vec![".ts".to_string()],
        };
        assert!(!crate::katastar::is_safe_auto_patch("src/types/user.ts", &local_rel));

        let domain_fn = crate::katastar::KatastarPatch {
            search: "export async function createReview(review: Review) {".to_string(),
            replace: "export async function createReview(review: any) {".to_string(),
            extensions: vec![".ts".to_string()],
        };
        assert!(!crate::katastar::is_safe_auto_patch("src/utils/prisma.ts", &domain_fn));

        let prisma_in_client = crate::katastar::KatastarPatch {
            search: "import { Role } from 'lucide-react';".to_string(),
            replace: "import { PrismaClient } from '@prisma/client';".to_string(),
            extensions: vec![".tsx".to_string()],
        };
        assert!(!crate::katastar::is_safe_auto_patch("src/components/Hero.tsx", &prisma_in_client));

        let ext_icons = crate::katastar::KatastarPatch {
            search: "import { Home } from 'react-icons';".to_string(),
            replace: "import { FiHome as Home } from 'react-icons/fi';".to_string(),
            extensions: vec![".tsx".to_string()],
        };
        assert!(crate::katastar::is_safe_auto_patch("src/components/Hero.tsx", &ext_icons));
    }

    #[test]
    fn test_desktop_web_api_scope_search_params_and_prisma_input() {
        let api_outer_body = concat!(
            "export async function POST(request: NextRequest) {\n",
            "  try {\n",
            "    let body: any = {};\n",
            "    try { body = await request.json(); } catch {}\n",
            "    return NextResponse.json(body);\n",
            "  } catch (error) {\n",
            "    return NextResponse.json({ id: 'mock', ...body }, { status: 201 });\n",
            "  }\n",
            "}\n"
        );
        let hoisted = crate::sanitizer::sanitize_try_catch_body_scope(api_outer_body);
        assert!(hoisted.contains("let body: any = {};\n  try {"));

        let api_sp = "export async function GET(request: NextRequest) {\n  const { id } = request.nextUrl.searchParams;\n  return NextResponse.json({ id });\n}\n";
        let fixed_sp = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(api_sp, "src/app/api/alerts/route.ts");
        assert!(fixed_sp.contains("const id = request.nextUrl.searchParams.get('id');"));

        let prisma_util = concat!(
            "import { PartItemCreateInput } from '@/types';\n",
            "import prisma from '@/lib/prisma';\n",
            "export async function createPart(partItemData: PartItemCreateInput) {\n",
            "  return await prisma.partItem.create({ data: partItemData });\n",
            "}\n"
        );
        let fixed_prisma = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(prisma_util, "src/utils/prismaUtils.ts");
        assert!(fixed_prisma.contains("data: partItemData as any"));
    }

    #[test]
    fn test_sanitize_expo_stylesheet_fallback_and_theme_style_array() {
        let screen = concat!(
            "import React from 'react';\n",
            "import { View, Text, StyleSheet } from 'react-native';\n",
            "import { theme } from '@/theme';\n",
            "export default function Screen() {\n",
            "  return <View style={styles.container}><Text style={[styles.text, styles.subtitle, theme.typography.body, theme.colors.primaryText]}>Hi</Text></View>;\n",
            "}\n",
            "const styles = StyleSheet.create({\n",
            "  container: { flex: 1 },\n",
            "  body: {\n",
            "    fontFamily: theme.typography.body.fontFamily,\n",
            "    fontSize: theme.typography.body.fontSize,\n",
            "  }\n",
            "});\n"
        );
        let fixed = crate::sanitizer::sanitize_expo_react_native_file(screen, "app/(tabs)/index.tsx");
        assert!(!fixed.contains("fontFamily: theme.typography.body.fontFamily"));
        assert!(fixed.contains("{ color: theme.colors.primaryText }"));
        assert!(fixed.contains("text: { color: '#f8fafc' }"));
        assert!(fixed.contains("subtitle: { fontSize: 16"));
    }

    #[test]
    fn test_expo_ionicons_auto_import_ellipsis_and_store_type_redirect() {
        let icon_src = concat!(
            "import React from 'react';\n",
            "import { Pressable } from 'react-native';\n",
            "export const Card = () => <Pressable><Ionicons name=\"ellipsis-outline\" size={20} /></Pressable>;\n"
        );
        let fixed_icon = crate::sanitizer::sanitize_expo_react_native_file(icon_src, "src/components/InspectionCard.tsx");
        assert!(fixed_icon.contains("import { Ionicons } from '@expo/vector-icons';"), "missing Ionicons import:\n{}", fixed_icon);
        assert!(fixed_icon.contains("\"ellipsis-horizontal-outline\""), "bad ellipsis icon not mapped:\n{}", fixed_icon);

        let dir = std::env::temp_dir().join(format!("axiom-expo-store-types-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/types")).unwrap();
        std::fs::create_dir_all(dir.join("src/store")).unwrap();
        std::fs::create_dir_all(dir.join("app/(tabs)")).unwrap();
        std::fs::write(
            dir.join("src/types/index.ts"),
            "export interface HabitItem { id: string; title: string; }\nexport interface WorkoutSession { id: string; calories: number; }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/store/useHabitStore.ts"),
            "export function useHabitStore() { return { habits: [] }; }\nexport default useHabitStore;\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/store/useFitnessStore.ts"),
            "export function useFitnessStore() { return { workouts: [] }; }\nexport default useFitnessStore;\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app/(tabs)/index.tsx"),
            concat!(
                "import React from 'react';\n",
                "import { useHabitStore, HabitItem } from '@/store/useHabitStore';\n",
                "import { useFitnessStore, WorkoutSession } from '@/store/useFitnessStore';\n",
                "export default function Screen() {\n",
                "  const { habits } = useHabitStore();\n",
                "  const item: HabitItem = { id: '1', title: 'Run' };\n",
                "  const w: WorkoutSession = { id: '2', calories: 100 };\n",
                "  return null;\n",
                "}\n"
            ),
        )
        .unwrap();

        crate::sanitizer::reconcile_expo_types_and_unions_in_project(&dir);
        let updated = std::fs::read_to_string(dir.join("app/(tabs)/index.tsx")).unwrap();
        assert!(updated.contains("import { useHabitStore } from '@/store/useHabitStore';"), "got:\n{}", updated);
        assert!(updated.contains("import { useFitnessStore } from '@/store/useFitnessStore';"), "got:\n{}", updated);
        assert!(updated.contains("import type { HabitItem } from '@/types';"), "got:\n{}", updated);
        assert!(updated.contains("import type { WorkoutSession } from '@/types';"), "got:\n{}", updated);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_generic_hook_use_state_fallback_cast() {
        let hook = concat!(
            "\"use client\";\n",
            "import { useState } from 'react';\n",
            "export const useBudgetAlerts = <T extends BudgetAlert[]>({ fallbackData = [] }: any) => {\n",
            "  const [data, setData] = useState<T>(fallbackData);\n",
            "  return [data, setData];\n",
            "};\n"
        );
        let fixed = crate::sanitizer::sanitize_nextjs_and_zod_hallucinations(hook, "src/hooks/useBudgetAlerts.ts");
        assert!(fixed.contains("useState<T>(fallbackData as any)"), "expected fallbackData as any, got:\n{}", fixed);
    }

    #[test]
    fn test_expo_hygiene_fixes_habit_card_safe_area_store_contract_and_zustand_calls() {
        let dir = std::env::temp_dir().join(format!("axiom-expo-full-hygiene-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/components")).unwrap();
        std::fs::create_dir_all(dir.join("src/store")).unwrap();
        std::fs::create_dir_all(dir.join("app/(tabs)")).unwrap();

        std::fs::write(
            dir.join("package.json"),
            r#"{"name":"pulsefit-test","dependencies":{"expo":"~52.0.0","react-native":"0.76.5"}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("src/components/HabitCard.tsx"),
            "import React from 'react';\nimport { View, Text } from 'react-native';\nconst HabitCard = ({ habit }: { habit: any }) => <View><Text>{habit?.title}</Text></View>;\nexport default HabitCard;\nexport { HabitCard };\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/store/useFitnessStore.ts"),
            concat!(
                "import { useState } from 'react';\n",
                "interface FitnessStore {\n",
                "  workouts: any[];\n",
                "}\n",
                "export function useFitnessStore(): FitnessStore {\n",
                "  const [workouts, setWorkouts] = useState<any[]>([]);\n",
                "  const addWorkout = (w: any) => setWorkouts([...workouts, w]);\n",
                "  return { workouts, addWorkout };\n",
                "}\n",
                "export default useFitnessStore;\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("app/(tabs)/index.tsx"),
            "import React from 'react';\nimport { Text } from 'react-native';\nexport default function Home() {\n  return <SafeAreaView><Text>Home</Text></SafeAreaView>;\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app/(tabs)/habits.tsx"),
            "import React from 'react';\nimport { View } from 'react-native';\nexport default function Habits() {\n  const item = { title: 'Water' };\n  return <View><HabitCard habit={item} /></View>;\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("app/(tabs)/settings.tsx"),
            concat!(
                "import React from 'react';\n",
                "import { View, Pressable, Text } from 'react-native';\n",
                "import { useFitnessStore } from '@/store/useFitnessStore';\n",
                "export default function Settings() {\n",
                "  const onImport = () => {\n",
                "    const cur = useFitnessStore.getState();\n",
                "    if (cur) useFitnessStore.setState({ workouts: [] });\n",
                "  };\n",
                "  return <View><Pressable onPress={onImport}><Text>Import</Text></Pressable></View>;\n",
                "}\n"
            ),
        )
        .unwrap();

        crate::self_healing::run_hygiene_pass_on_project(&dir);

        let habits_after = std::fs::read_to_string(dir.join("app/(tabs)/habits.tsx")).unwrap();
        assert!(
            habits_after.contains("@/components/HabitCard") && habits_after.contains("HabitCard"),
            "expected HabitCard auto-import, got:\n{}",
            habits_after
        );
        let index_after = std::fs::read_to_string(dir.join("app/(tabs)/index.tsx")).unwrap();
        assert!(
            index_after.contains("import { SafeAreaView } from 'react-native-safe-area-context';"),
            "expected SafeAreaView auto-import, got:\n{}",
            index_after
        );
        let store_after = std::fs::read_to_string(dir.join("src/store/useFitnessStore.ts")).unwrap();
        assert!(
            store_after.contains("addWorkout?: any;"),
            "expected FitnessStore interface widened with addWorkout?: any, got:\n{}",
            store_after
        );
        assert!(
            store_after.contains("[key: string]: any;"),
            "expected FitnessStore interface index signature, got:\n{}",
            store_after
        );
        assert!(
            store_after.contains("(useFitnessStore as any).getState =")
                && store_after.contains("(useFitnessStore as any).setState ="),
            "expected runtime getState/setState shim, got:\n{}",
            store_after
        );
        let settings_after = std::fs::read_to_string(dir.join("app/(tabs)/settings.tsx")).unwrap();
        assert!(
            settings_after.contains("(useFitnessStore as any).getState()")
                && settings_after.contains("(useFitnessStore as any).setState({ workouts: [] })"),
            "expected rewritten static getState/setState calls, got:\n{}",
            settings_after
        );

        assert!(!dir.join("src/app/page.tsx").exists());
        assert!(!dir.join("prisma").exists());
        assert!(!habits_after.contains("@prisma/client") && !index_after.contains("@prisma/client"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_expo_use_styles_object_invocation_rewritten() {
        let src = concat!(
            "import React from 'react';\n",
            "import { View, Text, StyleSheet } from 'react-native';\n",
            "const useStyles = StyleSheet.create({\n",
            "  pill: { padding: 8 },\n",
            "  label: { fontSize: 12 },\n",
            "});\n",
            "export function StatusPill({ status }: { status: string }) {\n",
            "  const styles = useStyles();\n",
            "  return <View style={styles.pill}><Text style={useStyles().label}>{status}</Text></View>;\n",
            "}\n"
        );
        let cleaned = crate::sanitizer::sanitize_expo_react_native_file(src, "src/components/StatusPill.tsx");
        assert!(
            cleaned.contains("const styles = useStyles;"),
            "expected useStyles() call rewritten to useStyles, got:\n{}",
            cleaned
        );
        assert!(
            cleaned.contains("style={useStyles.label}"),
            "expected inline useStyles().label rewritten to useStyles.label, got:\n{}",
            cleaned
        );
        assert!(
            cleaned.contains("const useStyles = StyleSheet.create("),
            "expected declaration preserved, got:\n{}",
            cleaned
        );
    }

    #[test]
    fn test_expo_store_reconciliation_binds_real_array_alias_and_local_fn() {
        let dir = std::env::temp_dir().join(format!("axiom-expo-store-alias-real-fn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/store")).unwrap();
        std::fs::create_dir_all(dir.join("app/(tabs)")).unwrap();
        std::fs::write(
            dir.join("package.json"),
            r#"{"name":"pulsefit","dependencies":{"expo":"~52.0.0","react-native":"0.76.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("src/store/useFitnessStore.ts"),
            concat!(
                "import { useState } from 'react';\n",
                "export function useFitnessStore() {\n",
                "  const [workouts, setWorkouts] = useState<any[]>([]);\n",
                "  const loadInitialData = async () => { setWorkouts([{ id: '1' }]); };\n",
                "  return {\n",
                "    workouts,\n",
                "  };\n",
                "}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("app/(tabs)/index.tsx"),
            concat!(
                "import React, { useEffect } from 'react';\n",
                "import { View, Text } from 'react-native';\n",
                "import { useFitnessStore } from '@/store/useFitnessStore';\n",
                "export default function Home() {\n",
                "  const { items: workouts, loadInitialData } = useFitnessStore();\n",
                "  useEffect(() => { loadInitialData(); }, []);\n",
                "  return <View><Text>{workouts.length}</Text></View>;\n",
                "}\n"
            ),
        )
        .unwrap();

        let _ = crate::ast_engine::reconcile_hook_contracts_in_project(&dir);
        let after = std::fs::read_to_string(dir.join("src/store/useFitnessStore.ts")).unwrap();
        assert!(
            after.contains("items: workouts"),
            "expected items aliased to live workouts state array, got:\n{}",
            after
        );
        assert!(
            !after.contains("items: undefined"),
            "must never inject items: undefined, got:\n{}",
            after
        );
        assert!(
            after.contains("loadInitialData") && !after.contains("loadInitialData: async (..._args: any[]) => {}"),
            "expected real local loadInitialData shorthand exposed, got:\n{}",
            after
        );
        assert_eq!(
            crate::ast_engine::default_value_for_missing_hook_key("items", true),
            "[] as any"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_strip_bogus_middleware_locale_redirect_preserves_auth_and_real_locale_projects() {
        let mw = concat!(
            "import { NextRequest, NextResponse } from 'next/server';\n",
            "export async function middleware(request: NextRequest) {\n",
            "  const pathname = request.nextUrl.pathname;\n",
            "  if (pathname.startsWith('/dashboard/')) {\n",
            "    return NextResponse.redirect(new URL('/login', request.url));\n",
            "  }\n",
            "  const locale = pathname.split('/')[1];\n",
            "  if (locale && !['en', 'es'].includes(locale)) {\n",
            "    return NextResponse.redirect(new URL('/en', request.url));\n",
            "  }\n",
            "  return NextResponse.next();\n",
            "}\n"
        );

        let dir_no_locale = std::env::temp_dir().join(format!("axiom-mw-no-locale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir_no_locale);
        std::fs::create_dir_all(dir_no_locale.join("src/app")).unwrap();

        let cleaned = crate::sanitizer::strip_bogus_middleware_locale_rewrite(mw, "src/middleware.ts", Some(&dir_no_locale));
        assert!(!cleaned.contains("'/en'"), "bogus locale redirect should be stripped, got:\n{}", cleaned);
        assert!(!cleaned.contains("const locale"), "unused locale decl should be stripped, got:\n{}", cleaned);
        assert!(cleaned.contains("'/login'"), "legitimate auth redirect MUST be preserved, got:\n{}", cleaned);

        // When src/app/[locale] exists on disk, middleware MUST stay untouched
        std::fs::create_dir_all(dir_no_locale.join("src/app/[locale]")).unwrap();
        let preserved = crate::sanitizer::strip_bogus_middleware_locale_rewrite(mw, "src/middleware.ts", Some(&dir_no_locale));
        assert_eq!(preserved, mw, "real [locale] project middleware must never be altered");

        let _ = std::fs::remove_dir_all(&dir_no_locale);
    }

    #[test]
    fn test_sanitize_openai_hybrid_configuration_with_v4_new_openai() {
        let hybrid = concat!(
            "import OpenAI from 'openai';\n",
            "const openAiConfig = new Configuration({\n",
            "  apiKey: process.env.OPENAI_API_KEY,\n",
            "});\n",
            "export const openaiClient = new OpenAI(openAiConfig);\n"
        );
        let fixed = crate::sanitizer::sanitize_openai_v3_to_v4(hybrid);
        assert!(!fixed.contains("new Configuration("), "hybrid Configuration should be unwrapped, got:\n{}", fixed);
        assert!(fixed.contains("apiKey: process.env.OPENAI_API_KEY"), "apiKey preserved, got:\n{}", fixed);
        assert!(fixed.contains("new OpenAI(openAiConfig)"), "new OpenAI preserved, got:\n{}", fixed);

        // Local class Configuration must never be touched
        let local_cls = "import OpenAI from 'openai';\nclass Configuration { constructor(public o: any) {} }\nconst c = new Configuration({ apiKey: 'x' });\nconst client = new OpenAI(c as any);\n";
        let untouched = crate::sanitizer::sanitize_openai_v3_to_v4(local_cls);
        assert!(untouched.contains("new Configuration("), "local class Configuration must be preserved");
    }

    #[test]
    fn test_ensure_server_prisma_import_skips_client_and_param_and_fixes_utils() {
        let dir = std::env::temp_dir().join(format!("axiom-prisma-autoimp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/lib")).unwrap();
        std::fs::write(dir.join("src/lib/prisma.ts"), "export const prisma = {} as any;\nexport default prisma;\n").unwrap();

        let util_src = "export const createSession = async (data: any) => prisma.audioSession.create({ data });\n";
        let fixed = crate::self_healing::ensure_server_prisma_import_if_used(&dir, "src/utils/session.ts", util_src);
        assert!(fixed.starts_with("import { prisma } from '@/lib/prisma';\n"), "expected named prisma import, got:\n{}", fixed);

        // Client file must NEVER get Prisma imported
        let client_src = "\"use client\";\nexport function Widget() { return prisma.user; }\n";
        assert_eq!(crate::self_healing::ensure_server_prisma_import_if_used(&dir, "src/components/W.tsx", client_src), client_src);

        // Parameter-injected prisma must NEVER get Prisma imported
        let param_src = "export const helper = (prisma: any) => prisma.user.findMany();\n";
        assert_eq!(crate::self_healing::ensure_server_prisma_import_if_used(&dir, "src/utils/h.ts", param_src), param_src);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_expo_router_route_link_reconciliation_fixes_tabs_prefix_and_kebab_case() {
        let dir = std::env::temp_dir().join(format!("axiom-expo-routes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app/(tabs)")).unwrap();
        std::fs::write(dir.join("package.json"), r#"{"dependencies":{"expo":"~52.0.0","expo-router":"~4.0.0"}}"#).unwrap();
        std::fs::write(dir.join("app/(tabs)/_layout.tsx"), "export default function L() { return null; }\n").unwrap();
        std::fs::write(dir.join("app/(tabs)/addExpense.tsx"), "export default function A() { return null; }\n").unwrap();
        std::fs::write(dir.join("app/(tabs)/budgets.tsx"), "export default function B() { return null; }\n").unwrap();
        std::fs::write(
            dir.join("app/(tabs)/index.tsx"),
            "export default function Home({ router }: any) { router.push('/tabs/add-expense'); router.push('/budgets'); return null; }\n",
        )
        .unwrap();

        let routes = crate::sanitizer::collect_expo_router_routes(&dir);
        assert!(routes.contains(&"/".to_string()) && routes.contains(&"/addExpense".to_string()) && routes.contains(&"/budgets".to_string()), "got routes: {:?}", routes);
        crate::sanitizer::normalize_project_route_links(&dir, &routes);

        let after = std::fs::read_to_string(dir.join("app/(tabs)/index.tsx")).unwrap();
        assert!(after.contains("router.push('/addExpense')"), "expected /tabs/add-expense rewritten to /addExpense, got:\n{}", after);
        assert!(after.contains("router.push('/budgets')"), "expected valid /budgets preserved, got:\n{}", after);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_expo_fixes_search_params_array_destructure_and_top_level_stylesheet_state_ternary() {
        let src = concat!(
            "import React, { useState } from 'react';\n",
            "import { View, StyleSheet } from 'react-native';\n",
            "import { useLocalSearchParams } from 'expo-router';\n",
            "import { theme } from '@/theme';\n",
            "export default function Screen() {\n",
            "  const [params] = useLocalSearchParams<{ category?: string }>();\n",
            "  const [selectedCategory, setSelectedCategory] = useState('ALL');\n",
            "  return <View style={styles.btn} />;\n",
            "}\n",
            "const styles = StyleSheet.create({\n",
            "  btn: {\n",
            "    backgroundColor: selectedCategory === 'FOOD' ? theme.colors.primary : 'transparent',\n",
            "    color: selectedCategory === 'FOOD' ? theme.colors.surface : theme.colors.primaryText,\n",
            "  },\n",
            "});\n"
        );
        let cleaned = crate::sanitizer::sanitize_expo_react_native_file(src, "app/(tabs)/expenses.tsx");
        assert!(
            cleaned.contains("const params = useLocalSearchParams<"),
            "expected array destructuring of useLocalSearchParams unwrapped, got:\n{}",
            cleaned
        );
        assert!(
            !cleaned.contains("backgroundColor: selectedCategory") && cleaned.contains("backgroundColor: 'transparent',"),
            "expected out-of-scope state ternary in top-level StyleSheet replaced with safe fallback, got:\n{}",
            cleaned
        );
        assert!(
            !cleaned.contains("color: selectedCategory") && cleaned.contains("color: theme.colors.primaryText,"),
            "expected second out-of-scope state ternary replaced with safe fallback, got:\n{}",
            cleaned
        );

        // Negative test: StyleSheet.create inside component scope (indented) must NOT be altered
        let in_scope = concat!(
            "import React, { useState } from 'react';\n",
            "import { View, StyleSheet } from 'react-native';\n",
            "export default function Scoped() {\n",
            "  const [active, setActive] = useState(false);\n",
            "  const styles = StyleSheet.create({\n",
            "    box: { backgroundColor: active ? '#fff' : '#000' },\n",
            "  });\n",
            "  return <View style={styles.box} />;\n",
            "}\n"
        );
        let kept = crate::sanitizer::sanitize_expo_react_native_file(in_scope, "app/(tabs)/scoped.tsx");
        assert!(
            kept.contains("active ? '#fff' : '#000'"),
            "in-scope StyleSheet inside component must never be altered, got:\n{}",
            kept
        );
    }
}
