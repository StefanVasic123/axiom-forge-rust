pub mod mode;
pub mod git_engine;
pub mod relay;
pub mod ollama;
pub mod project;
pub mod sanitizer;
pub mod packages;
pub mod editor_ai;
pub mod hardware;
pub mod server;
pub mod healer;
pub mod katastar;
pub mod self_healing;
pub mod orchestrator;

pub use ollama::*;
pub use project::*;
pub use sanitizer::*;
pub use packages::*;
pub use editor_ai::*;
pub use hardware::*;
pub use server::*;
pub use healer::*;
pub use katastar::*;
pub use self_healing::*;
pub use orchestrator::*;

pub use serde::{Deserialize, Serialize};
pub use sysinfo::System;
pub use std::collections::HashMap;
pub use keyring::Entry;
pub use std::sync::Mutex;
pub use std::fs;
pub use std::path::{Path, PathBuf};
pub use serde_json::Value;
pub use std::time::Duration;
pub use tauri::{Emitter, Manager, State};
pub use tokio::process::Command;

pub struct PendingDeepLink(pub Mutex<Option<String>>);

use std::io::Write;

#[tauri::command]
fn debug_log_to_file(message: String) {
    let projects_dir = get_projects_dir();
    let _ = std::fs::create_dir_all(&projects_dir);
    let log_path = projects_dir.join("deeplink_debug.txt");
    println!("[JS/Rust Debug Log] {}", message);
    if let Ok(meta) = std::fs::metadata(&log_path) {
        if meta.len() > 2 * 1024 * 1024 {
            let backup_path = projects_dir.join("deeplink_debug.old.txt");
            let _ = std::fs::rename(&log_path, &backup_path);
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path) {
            let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", time, message);
        }
}

pub fn get_agent_bridge_dir() -> PathBuf {
    let dir = get_base_dir().join("agent_bridge");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[tauri::command]
fn agent_update_live_state(state_json: Value) -> Result<(), String> {
    let bridge_dir = get_agent_bridge_dir();
    let state_path = bridge_dir.join("live_state.json");
    if let Ok(pretty) = serde_json::to_string_pretty(&state_json) {
        let _ = std::fs::write(&state_path, pretty);
    }
    Ok(())
}

#[tauri::command]
fn agent_poll_command() -> Option<Value> {
    let bridge_dir = get_agent_bridge_dir();
    let cmd_path = bridge_dir.join("command.json");
    if !cmd_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&cmd_path).ok()?;
    let _ = std::fs::remove_file(&cmd_path);
    serde_json::from_str::<Value>(&content).ok()
}

#[tauri::command]
fn agent_write_command_result(result_json: Value) -> Result<(), String> {
    let bridge_dir = get_agent_bridge_dir();
    let res_path = bridge_dir.join("command_result.json");
    if let Ok(pretty) = serde_json::to_string_pretty(&result_json) {
        let _ = std::fs::write(&res_path, pretty);
    }
    let cmd_id = result_json.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let action = result_json.get("action").and_then(|v| v.as_str()).unwrap_or("unknown");
    let ok = result_json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    debug_log_to_file(format!("[Agent Bridge] Command '{}' (action: {}) completed: success={}", cmd_id, action, ok));
    Ok(())
}

#[tauri::command]
fn get_pending_deep_link(state: tauri::State<'_, PendingDeepLink>) -> Option<String> {
    if let Ok(pending) = state.0.lock() {
        let val = pending.clone();
        debug_log_to_file(format!("get_pending_deep_link called. Returning: {:?}", val));
        val
    } else {
        None
    }
}

#[tauri::command]
fn clear_pending_deep_link(state: tauri::State<'_, PendingDeepLink>) {
    if let Ok(mut pending) = state.0.lock() {
        debug_log_to_file("clear_pending_deep_link called. Resetting to None.".to_string());
        *pending = None;
    }
}

#[tauri::command]
async fn show_floating_window(app: tauri::AppHandle, project_id: String) -> Result<(), String> {
    let label = format!("floating-{}", project_id);
    if let Some(existing_win) = app.get_webview_window(&label) {
        let _ = existing_win.close();
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.show();
        let _ = main_win.unminimize();
        let _ = main_win.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn close_floating_window(window: tauri::Window) -> Result<(), String> {
    if window.label().starts_with("floating-") {
        let _ = window.close();
    }
    Ok(())
}

fn handle_axiom_url<R: tauri::Runtime>(app: &tauri::AppHandle<R>, url_str: &str) {
    debug_log_to_file(format!("handle_axiom_url intercepted deep link URL: {}", url_str));

    // Also save in global state for JIT pulling on cold starts
    if let Some(state) = app.try_state::<PendingDeepLink>() {
        if let Ok(mut pending) = state.0.lock() {
            *pending = Some(url_str.to_string());
            debug_log_to_file(format!("Saved pending URL in Rust state: {}", url_str));
        }
    }

    if let Some(rest) = url_str.strip_prefix("axiom://") {
        let mut parts = rest.splitn(2, '?');
        let action = parts.next().unwrap_or("").trim_end_matches('/');
        let query = parts.next().unwrap_or("");
        
        let mut params = std::collections::HashMap::new();
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                params.insert(k, v);
            }
        }
        
        match action {
            "build" | "generate" => {
                let manifest_id = params.get("id").copied().unwrap_or("");
                let project_id = params.get("projectId").copied().unwrap_or(manifest_id);
                let custom_name = params.get("name").or_else(|| params.get("projectName")).copied().unwrap_or("");
                let token = params.get("token").copied().unwrap_or("");
                let host = params.get("host").copied().unwrap_or("");
                
                debug_log_to_file(format!(
                    "Emitting deep-link:build event with manifestId={} token={} host={} name={}",
                    manifest_id, token, host, custom_name
                ));
                
                if !manifest_id.is_empty() {
                    let _ = app.emit("deep-link:build", serde_json::json!({
                        "manifestId": manifest_id,
                        "projectId": project_id,
                        "token": token,
                        "host": host,
                        "name": custom_name
                    }));
                }
            },
            "config" => {
                let project_id = params.get("projectId").copied().unwrap_or("");
                debug_log_to_file(format!("Emitting deep-link:config event with projectId={}", project_id));
                if !project_id.is_empty() {
                    let _ = app.emit("deep-link:config", serde_json::json!({
                        "projectId": project_id
                    }));
                }
            },
            "deploy" => {
                let project_id = params.get("projectId").copied().unwrap_or("");
                debug_log_to_file(format!("Emitting deep-link:deploy event with projectId={}", project_id));
                if !project_id.is_empty() {
                    let _ = app.emit("deep-link:deploy", serde_json::json!({
                        "projectId": project_id
                    }));
                }
            },
            other => {
                debug_log_to_file(format!("Warning: Unknown deep link action received: {}", other));
            }
        }
    } else {
        debug_log_to_file(format!("Warning: Deep link URL lacks axiom:// prefix: {}", url_str));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            debug_log_to_file(format!("Single instance callback triggered! argv: {:?}", argv));
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            for arg in argv {
                let cleaned = arg.trim_matches('"').trim_matches('\'').to_string();
                debug_log_to_file(format!("Single instance checking arg: {}", cleaned));
                if cleaned.starts_with("axiom://") {
                    handle_axiom_url(app, &cleaned);
                }
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let app_handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        handle_axiom_url(&app_handle, url.as_str());
                    }
                });
                
                // 1. Prioritize scanning CLI arguments for cold start deep-links
                let mut found_cli_url = false;
                for arg in std::env::args() {
                    let arg_lower = arg.to_lowercase();
                    if arg_lower.starts_with("axiom://") || arg_lower.contains("axiom://") {
                        let cleaned_url = arg.trim_matches('"').trim_matches('\'').to_string();
                        debug_log_to_file(format!("Cold start CLI argument match found: {}", cleaned_url));
                        handle_axiom_url(app.handle(), &cleaned_url);
                        found_cli_url = true;
                    }
                }
                
                // 2. Fall back to plugin retrieval if CLI scan produced no deep link
                if !found_cli_url {
                    if let Ok(Some(urls)) = app.deep_link().get_current() {
                        for url in urls {
                            debug_log_to_file(format!("Cold start plugin-registered match found: {}", url.as_str()));
                            handle_axiom_url(app.handle(), url.as_str());
                        }
                    }
                }

                // 3. Auto-verify and bootstrap Node.js v20 LTS toolchain in the background on startup
                tauri::async_runtime::spawn(async move {
                    let _ = ensure_node_toolchain_ready(None, "").await;
                });

                // 4. Start local web-to-desktop handshake listener (127.0.0.1:24859)
                tauri::async_runtime::spawn(async move {
                    if let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:24859").await {
                        while let Ok((mut stream, _)) = listener.accept().await {
                            tokio::spawn(async move {
                                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                                let mut buf = [0u8; 1024];
                                if stream.read(&mut buf).await.is_ok() {
                                    let req = String::from_utf8_lossy(&buf);
                                    let response = if req.starts_with("OPTIONS") {
                                        "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nConnection: close\r\n\r\n".to_string()
                                    } else {
                                        let body = format!(
                                            r#"{{"app":"axiom-forge","version":"{}","status":"ready"}}"#,
                                            env!("CARGO_PKG_VERSION")
                                        );
                                        format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                            body.len(),
                                            body
                                        )
                                    };
                                    let _ = stream.write_all(response.as_bytes()).await;
                                }
                            });
                        }
                    }
                });
            }
            Ok(())
        })
        .manage(PendingDeepLink(Mutex::new(None)))
        .manage(ServerState(Mutex::new(HashMap::new())))
        .manage(mode::ModeState(Mutex::new(mode::load_mode_state(&get_base_dir()))))
        .invoke_handler(tauri::generate_handler![
            greet, 
            get_hardware_profile,
            get_local_tools_profile,
            get_axiom_settings,
            save_axiom_settings,
            store_token,
            get_token,
            delete_token,
            has_token,
            clear_all_tokens,
            ollama_check_health,
            ollama_check_installed,
            ollama_start_server,
            ollama_pull_model,
            project_get_all,
            project_save,
            project_delete,
            project_bulk_delete,
            project_update_status,
            project_get_files,
            project_commit,
            project_push_to_github,
            project_read_file,
            editor_write_file,
            editor_ai_edit,
            editor_ai_project_edit,
            server_start,
            server_stop,
            server_status,
            project_heal_compile_error,
            project_install_package,
            project_run_prisma_generate,
            get_error_katastar,
            task_start_generation,
            task_pause_generation,
            task_resume_generation,
            task_stop_generation,
            task_get_project_checkpoint,
            task_resume_project_generation,
            ollama_unload_model,
            get_pending_deep_link,
            clear_pending_deep_link,
            show_floating_window,
            close_floating_window,
            debug_log_to_file,
            agent_update_live_state,
            agent_poll_command,
            agent_write_command_result,
            project_import_local_folder,
            project_import_github,
            project_detect_missing_env_keys,
            project_save_env_keys,
            mode::get_app_mode,
            mode::set_app_mode,
            mode::save_agency_config,
            git_engine::git_create_client_proposal,
            git_engine::git_get_client_proposals,
            git_engine::git_update_proposal_status,
            git_engine::git_checkout_proposal_branch,
            git_engine::git_restore_main_branch,
            git_engine::git_refine_proposal_with_ai,
            relay::relay_submit_proposal
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_and_rewrite_import() {
        let db = vec![
            PackageMeta {
                name: "resend".to_string(),
                version: "^3.2.0".to_string(),
                aliases: vec!["@resend/client".to_string(), "@resend/base".to_string(), "resend-node".to_string(), "resend/client".to_string()],
                import_examples: vec!["import { Resend } from 'resend';".to_string()],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
            PackageMeta {
                name: "next-auth".to_string(),
                version: "^4.24.7".to_string(),
                aliases: vec!["@next-auth/client".to_string(), "next-auth/client".to_string()],
                import_examples: vec![
                    "import { getServerSession } from 'next-auth';".to_string(),
                    "import NextAuth from 'next-auth';".to_string(),
                    "import { useSession } from 'next-auth/react';".to_string(),
                ],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
            PackageMeta {
                name: "@hookform/resolvers".to_string(),
                version: "^3.4.2".to_string(),
                aliases: vec![],
                import_examples: vec!["import { zodResolver } from '@hookform/resolvers/zod';".to_string()],
                compatibility: serde_json::Map::new(),
                best_practices: "".to_string(),
            },
        ];

        // 1. Should not rewrite relative imports
        assert_eq!(resolve_and_rewrite_import("./components/Header", &db), None);
        assert_eq!(resolve_and_rewrite_import("../hooks/useAuth", &db), None);
        assert_eq!(resolve_and_rewrite_import("@/lib/prisma", &db), None);

        // 2. Should rewrite exact aliases
        assert_eq!(resolve_and_rewrite_import("@resend/client", &db), Some("resend".to_string()));
        assert_eq!(resolve_and_rewrite_import("resend-node", &db), Some("resend".to_string()));

        // 3. Should rewrite next-auth client aliases specifically to next-auth/react
        assert_eq!(resolve_and_rewrite_import("@next-auth/client", &db), Some("next-auth/react".to_string()));
        assert_eq!(resolve_and_rewrite_import("next-auth/client", &db), Some("next-auth/react".to_string()));

        // 4. Should rewrite invalid subpaths of package name
        assert_eq!(resolve_and_rewrite_import("resend/client", &db), Some("resend".to_string()));
        assert_eq!(resolve_and_rewrite_import("resend/something/else", &db), Some("resend".to_string()));

        // 5. Should NOT rewrite valid subpaths of package name
        assert_eq!(resolve_and_rewrite_import("@hookform/resolvers/zod", &db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/react", &db), None);
    }

    #[test]
    fn test_post_process_use_client() {
        let content = "import { useState } from 'react';\nexport default function Counter() { const [c, setC] = useState(0); return <div />; }";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/app/components/Counter.tsx", content, &metamanifest, false);
        assert!(processed.starts_with("\"use client\";"));

        let content_with = "\"use client\";\nimport { useState } from 'react';";
        let processed_with = post_process_generated_file("src/app/components/Counter.tsx", content_with, &metamanifest, false);
        assert_eq!(processed_with, content_with.to_string());

        let displaced = "import { useState } from 'react';\n\"use client\";\nimport Link from 'next/link';";
        let hoisted = hoist_use_client_directive(displaced);
        assert!(hoisted.starts_with("\"use client\";\nimport { useState }"));
        assert_eq!(hoisted.matches("\"use client\"").count(), 1);
        
        let mui_content = "import { styled } from '@mui/material/styles';\nconst MyDiv = styled('div')({});";
        let processed_mui = post_process_generated_file("src/app/layout.tsx", mui_content, &metamanifest, false);
        assert!(processed_mui.starts_with("\"use client\";"));
    }

    #[test]
    fn test_ensure_zod_imports() {
        let route_code = "import { NextResponse } from 'next/server';\n\nconst schema = z.object({ id: z.string() });\n\nexport async function POST() { return NextResponse.json({}); }";
        let with_zod = ensure_zod_imports(route_code);
        assert!(with_zod.contains("import { z } from 'zod';"));

        let processed = post_process_generated_file("src/app/api/subscriptions/route.ts", route_code, &serde_json::json!({}), false);
        assert!(processed.contains("import { z } from 'zod';"));
    }

    #[test]
    fn test_log_throttler_dedup_and_rate_limit() {
        let mut throttler = LogThrottler::new();
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_some());
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_some());
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_none());
        assert!(throttler.filter_line("POST /api/subscriptions 500").is_none());

        let diff = throttler.filter_line("✓ Compiled in 200ms");
        assert!(diff.is_some());
        assert!(diff.unwrap().contains("Prethodna poruka ponovljena ukupno 4 puta"));
    }

    #[test]
    fn test_post_process_use_session_layout() {
        let content = "import { useSession } from 'next-auth/react';\nexport default function RootLayout({ children }) { const { data } = useSession(); return <html><body>{children}</body></html>; }";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/app/layout.tsx", content, &metamanifest, false);
        assert!(processed.contains("export default function RootLayout"));
        assert!(processed.contains("<SessionProvider>{children}</SessionProvider>"));
        assert!(processed.starts_with("\"use client\";"));
    }

    #[test]
    fn test_extract_edits_from_json_fixes_and_code() {
        // Test JSON with "fixes" array containing "code" (exact response from qwen on layout.tsx)
        let json_fixes = serde_json::json!({
            "fixes": [
                {
                    "file": "src/app/layout.tsx",
                    "line": 12,
                    "description": "Unexpected token html",
                    "code": "export default function RootLayout({ children }: { children: React.ReactNode }) { return (<html><body>{children}</body></html>); }"
                }
            ]
        });
        let edits = extract_edits_from_json(&json_fixes, "src/app/layout.tsx");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["path"], "src/app/layout.tsx");
        assert!(edits[0]["replacement"].as_str().unwrap().contains("export default function RootLayout"));

        // Test JSON with arbitrary nested structure and custom key names
        let json_nested = serde_json::json!({
            "response": {
                "solution": {
                    "target_file": "src/app/layout.tsx",
                    "updated_code": "export default function Layout() { return <div>Hello</div>; }"
                }
            }
        });
        let edits_nested = extract_edits_from_json(&json_nested, "src/app/layout.tsx");
        assert_eq!(edits_nested.len(), 1);
        assert_eq!(edits_nested[0]["path"], "src/app/layout.tsx");
        assert!(edits_nested[0]["replacement"].as_str().unwrap().contains("export default function Layout"));

        // Test JSON with outer file and inner "path" containing target code snippet (Navbar.tsx case)
        let json_router_fix = serde_json::json!({
            "file": "src/components/Navbar.tsx",
            "changes": [
                {
                    "type": "replace",
                    "path": "import { useRouter } from 'next/router';",
                    "replacement": "import { useRouter } from 'next/navigation';"
                }
            ]
        });
        let edits_router = extract_edits_from_json(&json_router_fix, "src/components/Navbar.tsx");
        assert_eq!(edits_router.len(), 1);
        assert_eq!(edits_router[0]["path"], "src/components/Navbar.tsx");
        assert_eq!(edits_router[0]["target"], "import { useRouter } from 'next/router';");
        assert_eq!(edits_router[0]["replacement"], "import { useRouter } from 'next/navigation';");

        // Test post_process_generated_file safely unwraps raw JSON envelope into code
        let raw_json_file = "{\n  \"fixes\": [\n    {\n      \"file\": \"src/app/layout.tsx\",\n      \"code\": \"export default function Clean() { return <div>Clean</div>; }\"\n    }\n  ]\n}";
        let processed = post_process_generated_file("src/app/layout.tsx", raw_json_file, &serde_json::json!({}), false);
        assert!(!processed.contains("\"fixes\":"));
        assert!(processed.contains("export default function Clean"));
    }

    #[test]
    fn test_analyze_npm_install_failure() {
        // Test ETARGET (version mismatch)
        let etarget_log = "npm error code ETARGET\nnpm error notarget No matching version found for tw-elements-react@^4.0.0.\nnpm error notarget In most cases you or one of your dependencies...";
        let res1 = analyze_npm_install_failure(etarget_log);
        assert!(matches!(res1, Some(NpmFailureAction::SetWildcard(ref p)) if p == "tw-elements-react"));

        // Test E404 (not in registry)
        let e404_log = "npm error code E404\nnpm error 404 'some-bad-package' is not in the npm registry.";
        let res2 = analyze_npm_install_failure(e404_log);
        assert!(matches!(res2, Some(NpmFailureAction::Remove(ref p)) if p == "some-bad-package"));

        // Test E404 (legacy/requested resource)
        let legacy_e404_log = "npm ERR! 404 Not Found\nnpm ERR! 404  requested resource 'another-bad-package@^1.0.0'";
        let res3 = analyze_npm_install_failure(legacy_e404_log);
        assert!(matches!(res3, Some(NpmFailureAction::Remove(ref p)) if p == "another-bad-package"));

        // Test E404 (GET request style)
        let get_e404_log = "npm ERR! 404 Not Found - GET https://registry.npmjs.org/third-bad-package - Not found";
        let res4 = analyze_npm_install_failure(get_e404_log);
        assert!(matches!(res4, Some(NpmFailureAction::Remove(ref p)) if p == "third-bad-package"));
    }

    #[test]
    fn test_post_process_prisma_missing_url() {
        let content = "datasource db {\n  provider = \"postgresql\"\n}\n\nmodel User {\n  id            String     @id\n  userId        String\n  user          User       @relation(fields: [userId], references: [User.id])\n}";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("prisma/schema.prisma", content, &metamanifest, false);
        assert!(processed.contains("url      = env(\"DATABASE_URL\")"));
        assert!(processed.contains("references: [id]"));
        assert!(processed.contains("userId        String\n"));
    }

    #[test]
    fn test_post_process_prisma_default_export() {
        let content = "import { PrismaClient } from '@prisma/client';\nexport const prisma = new PrismaClient();";
        let metamanifest = serde_json::json!({});
        let processed = post_process_generated_file("src/lib/prisma.ts", content, &metamanifest, false);
        assert!(processed.contains("export default prisma;"));
    }

    #[test]
    fn test_prisma_sanitizer_fixes_relation_on_scalar_and_nextauth() {
        // Exact schema that broke the "Health App" project
        let content = "generator client {\n  provider = \"prisma-client-js\"\n}\n\ndatasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}\n\nmodel User {\n  id            String     @id @default(cuid())\n  createdAt     DateTime   @default(now())\n  email         String     @unique\n  name          String?\n  accounts      Account[]\n  sessions      Session[]\n}\n\nmodel Account {\n  id                String     @id @default(cuid())\n  type              String\n  provider          String\n  providerAccountId String\n  refresh_token     String?\n  access_token      String?\n  expires_at        Int?\n  token_type        String?\n  scope             String?\n  id_token          String?\n  session_state     String?\n  userId            String     @relation(fields: [userId], references: [id], onDelete: Cascade)\n  user              User       @relation(fields: [userId], references: [id], onDelete: Cascade)\n}\n\nmodel Session {\n  id           String   @id @default(cuid())\n  sessionToken String   @unique\n  expires      DateTime\n  userId       String   \n  user         User     @relation(fields: [userId], references: [id], onDelete: Cascade)\n}";
        let processed = post_process_generated_file("prisma/schema.prisma", content, &serde_json::json!({}), false);

        // No @relation on a scalar field anymore
        for line in processed.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == "userId" {
                assert!(!line.contains("@relation"), "scalar userId still has @relation: {}", line);
            }
        }
        // Canonical NextAuth constraint added
        assert!(processed.contains("@@unique([provider, providerAccountId])"));
        // Back-relations are not duplicated
        assert_eq!(processed.matches("Account[]").count(), 1);
        assert_eq!(processed.matches("Session[]").count(), 1);
    }

    #[test]
    fn test_nextauth_template_matches_int_user_id_and_adds_back_relations() {
        let content = "model User {\n  id    Int    @id @default(autoincrement())\n  email String @unique\n\n  @@index([email])\n}\n\nmodel Account {\n  id                String @id\n  userId            Int\n  provider          String\n  providerAccountId String\n}\n\nmodel Session {\n  id           String @id\n  sessionToken String @unique\n  userId       Int\n  expires      DateTime\n}";
        let processed = apply_nextauth_prisma_template(content);
        assert!(processed.contains("  userId            Int\n"));
        assert!(processed.contains("  userId       Int\n"));
        assert!(processed.contains("accounts      Account[]"));
        assert!(processed.contains("sessions      Session[]"));
        // Back-relations are inserted before the @@index block attribute
        let acc_idx = processed.find("accounts      Account[]").unwrap();
        let idx_idx = processed.find("@@index([email])").unwrap();
        assert!(acc_idx < idx_idx);
    }

    #[test]
    fn test_nextauth_template_skips_non_nextauth_models() {
        let content = "model User {\n  id String @id\n}\n\nmodel Session {\n  id      String @id\n  workout String\n}";
        assert_eq!(apply_nextauth_prisma_template(content), content);
    }


    #[test]
    fn test_ensure_prisma_opposite_relations() {
        let broken = r#"
model User {
  id    String @id @default(cuid())
  email String @unique
}

model Project {
  id   String @id @default(cuid())
  name String
}

model Team {
  id        String  @id @default(cuid())
  userId    String
  member    User    @relation(fields: [userId], references: [id], name: "TeamMember")
  projectId String
  project   Project @relation(fields: [projectId], references: [id])
}

model Analytics {
  id        String   @id @default(cuid())
  timestamp DateTime @default(now())
  projectId String?
  project   Project? @relation(fields: [projectId], references: [id])
}

model Dashboard {
  id       String @id @default(cuid())
  settings Json
  userId   String
  user     User   @relation(fields: [userId], references: [id])
}
"#;

        let fixed = post_process_generated_file("prisma/schema.prisma", broken, &serde_json::json!({}), false);
        // Opposite relation with relation name preserved
        assert!(fixed.contains("Team[]") && fixed.contains("@relation(\"TeamMember\")"));
        // Opposites for Dashboard and Analytics added
        assert!(fixed.contains("Dashboard[]"));
        assert!(fixed.contains("Analytics[]"));
        assert!(fixed.contains("Team[]"));
    }

    #[test]
    fn test_nextjs_middleware_rulepack() {
        let content = r#"import { NextRequest, NextResponse } from 'next/server';

export function middleware(request: NextRequest) {
  return NextResponse.next();
}

export const config = {
  matcher: ['/dashboard/:path*'],
};"#;
        let rules = get_stack_rulepacks("src/middleware.ts", content, None);
        assert!(rules.contains("NEXT.JS MIDDLEWARE & EDGE ROUTING RULES"));
        assert!(rules.contains("PATH-TO-REGEXP MATCHERS"));
        assert!(rules.contains("EDGE RUNTIME BOUNDARY"));
        assert!(rules.contains("SINGLE ROOT/SRC MIDDLEWARE FILE"));
    }

    #[test]
    fn test_clean_cli_output_and_extract_failing_lines() {
        let raw = "\u{1b}[1;91merror\u{1b}[0m: Invalid field type\n  -->  [4mprisma\\schema.prisma:2[0m\n  -->  prisma\\schema.prisma:2";
        let clean = clean_cli_output_for_llm(raw);
        assert!(!clean.contains("[1;91m"));
        assert!(!clean.contains("[0m"));
        assert!(!clean.contains('\u{1b}'));
        let schema = "model A {\n  userId String @relation(fields: [userId], references: [id])\n}";
        let lines = extract_prisma_error_lines(&clean, schema);
        assert_eq!(lines.matches("Line 2:").count(), 1);
        assert!(lines.contains("userId String @relation"));
    }

    #[test]
    fn test_get_generation_phase() {
        assert_eq!(get_generation_phase("package.json", "nextjs"), GenerationPhase::Config);
        assert_eq!(get_generation_phase("tsconfig.json", "nextjs"), GenerationPhase::Config);
        assert_eq!(get_generation_phase("schema.prisma", "nextjs"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("src/app/api/auth/[...nextauth]/route.ts", "nextjs"), GenerationPhase::ApiOrState);
        assert_eq!(get_generation_phase("src/hooks/useAuth.ts", "nextjs"), GenerationPhase::LogicOrHooks);
        assert_eq!(get_generation_phase("src/app/layout.tsx", "nextjs"), GenerationPhase::ShellOrWidgets);
        assert_eq!(get_generation_phase("src/app/dashboard/page.tsx", "nextjs"), GenerationPhase::PagesOrUi);

        assert_eq!(get_generation_phase("main.js", "electron"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("preload.js", "electron"), GenerationPhase::DatabaseOrMain);
        assert_eq!(get_generation_phase("src/components/Counter.tsx", "electron"), GenerationPhase::PagesOrUi);
    }

    #[test]
    fn test_resolve_import_path() {
        let temp_dir = std::env::temp_dir().join("axiom_test_import_resolve");
        let _ = std::fs::create_dir_all(&temp_dir);
        let src_dir = temp_dir.join("src").join("app").join("components");
        let _ = std::fs::create_dir_all(&src_dir);
        
        let card_file = src_dir.join("ProductCard.tsx");
        let _ = std::fs::write(&card_file, "export const ProductCard = () => null;");
        
        // Test absolute resolve
        let resolved = resolve_import_path(&temp_dir, "@/app/components/ProductCard", "src/app/page.tsx");
        assert_eq!(resolved, Some("src/app/components/ProductCard.tsx".to_string()));
        
        // Test relative resolve (canonical path collapses `..` segments cleanly)
        let resolved_rel = resolve_import_path(&temp_dir, "../components/ProductCard", "src/app/pages/page.tsx");
        assert_eq!(resolved_rel, Some("src/app/components/ProductCard.tsx".to_string()));
        
        // Clean up
        let _ = std::fs::remove_file(card_file);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_generate_axiom_map() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        let src_hooks = temp_dir.join("src/hooks");
        std::fs::create_dir_all(&src_hooks).unwrap();
        
        let hook_content = r#"
import axios from 'axios';
export function useAuth() {
    return "auth";
}
"#;
        std::fs::write(src_hooks.join("useAuth.ts"), hook_content).unwrap();

        let comp_dir = temp_dir.join("src/components");
        std::fs::create_dir_all(&comp_dir).unwrap();
        
        let comp_content = r#"
import { useAuth } from '@/hooks/useAuth';
export default function Dashboard() {
    const auth = useAuth();
    return "dashboard";
}
export const version = "1.0.0";
"#;
        std::fs::write(comp_dir.join("Dashboard.tsx"), comp_content).unwrap();

        let map_res = generate_axiom_map(&temp_dir);
        assert!(map_res.is_ok());

        let map_path = temp_dir.join("axiom-map.json");
        assert!(map_path.exists());

        let map_content = std::fs::read_to_string(map_path).unwrap();
        let map_val: Value = serde_json::from_str(&map_content).unwrap();

        assert!(map_val.get("files").unwrap().get("src/hooks/useAuth.ts").is_some());
        assert!(map_val.get("files").unwrap().get("src/components/Dashboard.tsx").is_some());

        let dash_entry = map_val.get("files").unwrap().get("src/components/Dashboard.tsx").unwrap();
        assert!(dash_entry.get("exports").unwrap().as_array().unwrap().contains(&serde_json::json!("default")));
        assert!(dash_entry.get("exports").unwrap().as_array().unwrap().contains(&serde_json::json!("version")));
        assert!(dash_entry.get("dependencies").unwrap().as_array().unwrap().contains(&serde_json::json!("src/hooks/useAuth.ts")));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1B[31mx\x1B[0m You're importing a component that needs \u{1b}[36museState\u{1b}[0m.";
        let cleaned = strip_ansi_codes(input);
        assert_eq!(cleaned, "x You're importing a component that needs useState.");
    }

    #[test]
    fn test_extract_file_path_from_line() {
        let input1 = " ⨯ ./src/hooks/useAuth.ts";
        assert_eq!(extract_file_path_from_line(input1), Some("src/hooks/useAuth.ts".to_string()));

        let input2 = "Error: C:\\projects\\app\\src\\components\\Button.tsx:10:15";
        assert_eq!(extract_file_path_from_line(input2), Some("C:\\projects\\app\\src\\components\\Button.tsx".to_string()));

        let input3 = "   ,-[C:\\projects\\app\\src\\hooks\\useAuth.ts:1:1]";
        assert_eq!(extract_file_path_from_line(input3), Some("C:\\projects\\app\\src\\hooks\\useAuth.ts".to_string()));

        let input4 = "  at Home (./src/app/page.tsx:15:98)";
        assert_eq!(extract_file_path_from_line(input4), Some("src/app/page.tsx".to_string()));

        let input5 = "  at Module._compile (C:\\projects\\node_modules\\next\\dist\\server\\require.js:5:10)";
        assert_eq!(extract_file_path_from_line(input5), None);

        let input6 = "  at eval (./.next/server/app/products/page.js:151:1)";
        assert_eq!(extract_file_path_from_line(input6), None);

        let input7 = "Unhandled Runtime Error: Error: The default export is not a React Component in page: \"/\"";
        assert_eq!(extract_file_path_from_line(input7), Some("src/app/page.tsx".to_string()));

        let input8 = "Error: The default export is not a React Component in page: \"/dashboard\"";
        assert_eq!(extract_file_path_from_line(input8), Some("src/app/dashboard/page.tsx".to_string()));

        let input9 = "Error: Handler is not defined in page: \"/api/auth/[...nextauth]\"";
        assert_eq!(extract_file_path_from_line(input9), Some("src/app/api/auth/[...nextauth]/route.ts".to_string()));
    }

    #[test]
    fn test_extract_package_name_from_error() {
        let input1 = "Error: Cannot find module 'decimal.js'";
        assert_eq!(extract_package_name_from_error(input1), Some("decimal.js".to_string()));

        let input2 = "Module not found: Can't resolve '@emotion/react' in 'C:\\projects\\...'";
        assert_eq!(extract_package_name_from_error(input2), Some("@emotion/react".to_string()));

        let input3 = "Error: Cannot find module './extensions'";
        assert_eq!(extract_package_name_from_error(input3), None);

        let input4 = "Error: Cannot find module '../utils/helpers'";
        assert_eq!(extract_package_name_from_error(input4), None);

        let input5 = "Error: Cannot find module 'C:\\projects\\app\\extensions'";
        assert_eq!(extract_package_name_from_error(input5), None);

        let input6 = "Error: Cannot find module 'C:/projects/app/extensions'";
        assert_eq!(extract_package_name_from_error(input6), None);
    }

    #[test]
    fn test_inject_axiom_attrs_generics() {
        let input = "const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => { return <form>Hello</form>; };";
        let output = inject_axiom_attrs(input, "src/app/register/page.tsx");
        assert!(!output.contains("<HTMLFormElement data-axiom-component"));
        assert!(output.contains("<form data-axiom-component=\"Page\" data-axiom-file=\"src/app/register/page.tsx\">"));
        
        let input2 = "const [error, setError] = useState<string | null>(null); return <div>Error</div>;";
        let output2 = inject_axiom_attrs(input2, "src/app/register/page.tsx");
        assert!(!output2.contains("<string data-axiom-component"));
        assert!(output2.contains("<div data-axiom-component=\"Page\" data-axiom-file=\"src/app/register/page.tsx\">"));
    }

    #[test]
    fn test_inject_axiom_attrs_context_provider_and_member_expr() {
        // 1. Context Provider should NOT have attributes injected into it
        let input = "return (<CartContext.Provider value={{ cart }}><div>Content</div></CartContext.Provider>);";
        let output = inject_axiom_attrs(input, "src/lib/cartContext.tsx");
        assert!(!output.contains("<CartContext data-axiom-component"));
        assert!(!output.contains("<CartContext.Provider data-axiom-component"));
        assert!(output.contains("<div data-axiom-component=\"CartContext\" data-axiom-file=\"src/lib/cartContext.tsx\">Content</div>"));

        // 2. Member expression components (like Menu.Item) should have attribute placed cleanly on full tag name
        let input2 = "return (<Menu.Item className=\"p-2\"><span>Text</span></Menu.Item>);";
        let output2 = inject_axiom_attrs(input2, "src/components/Menu.tsx");
        assert!(output2.contains("<Menu.Item data-axiom-component=\"Menu\" data-axiom-file=\"src/components/Menu.tsx\" className=\"p-2\">"));
        assert!(!output2.contains("<Menu data-axiom-component"));
    }

    #[test]
    fn test_get_project_path() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        
        // When project is not registered, it should fallback to internal projects dir
        let fallback_path = get_project_path("non_existent_project");
        assert!(fallback_path.ends_with("non_existent_project"));
        
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_get_stack_rulepacks() {
        // Test 1: React + NextIntl file and error detection
        let file_path = "src/components/header.tsx";
        let content = "import { useTranslations } from 'next-intl'; export function Header() { const t = useTranslations('Header'); }";
        let err = "Error: Failed to call `useTranslations` because the context from `NextIntlClientProvider` was not found.";

        let rules = get_stack_rulepacks(file_path, content, Some(err));
        assert!(rules.contains("REACT JSX SYNTAX, HOOKS & STYLING RULES"));
        assert!(rules.contains("NEVER USE STRING STYLE ATTRIBUTES"));
        assert!(rules.contains("NEVER CALL HOOKS AT MODULE SCOPE"));
        assert!(rules.contains("NEXT-INTL (INTERNATIONALIZATION) RULES"));
        assert!(rules.contains("useTranslations SIGNATURE ENFORCEMENT"));
        assert!(rules.contains("NEXT.JS LINK COMPONENT RULES") == false);

        // Test 2: Next.js Link component
        let link_content = "import Link from 'next/link'; export default function Nav() { return <Link href='/'>Home</Link>; }";
        let link_rules = get_stack_rulepacks("src/components/Nav.tsx", link_content, None);
        assert!(link_rules.contains("NEXT.JS LINK COMPONENT RULES"));
        assert!(link_rules.contains("PREFETCH OPTIMIZATION"));

        // Test 3: Prisma schema
        let prisma_rules = get_stack_rulepacks("prisma/schema.prisma", "datasource db { provider = \"sqlite\" }", None);
        assert!(prisma_rules.contains("PRISMA ORM & DATABASE RULES"));
        assert!(prisma_rules.contains("SERVER-ONLY BOUNDARY"));

        // Test 4: MERN server.js
        let mern_rules = get_stack_rulepacks("server.js", "const express = require('express'); const app = express();", None);
        assert!(mern_rules.contains("MERN STACK (EXPRESS + MONGOOSE + VITE/REACT) RULES"));
        assert!(mern_rules.contains("EXPRESS ASYNC ROUTE SAFETY"));

        // Test 5: React Native / Expo screen
        let rn_rules = get_stack_rulepacks("src/screens/HomeScreen.tsx", "import { View, Text } from 'react-native';", None);
        assert!(rn_rules.contains("REACT NATIVE & EXPO MOBILE RULES"));
        assert!(rn_rules.contains("ABSOLUTE WEB DOM ELEMENT BAN"));
        // React Native should NOT have web React rules injected
        assert!(!rn_rules.contains("REACT JSX SYNTAX, HOOKS & STYLING RULES"));

        // Test 6: Desktop Electron preload
        let desktop_rules = get_stack_rulepacks("preload.js", "const { contextBridge, ipcRenderer } = require('electron');", None);
        assert!(desktop_rules.contains("DESKTOP (ELECTRON & TAURI) RULES"));
        assert!(desktop_rules.contains("ELECTRON CONTEXT ISOLATION"));

        // Test 7: Unrelated plain file
        let plain_rules = get_stack_rulepacks("README.md", "# My Project", None);
        assert!(plain_rules.is_empty());

        // Test 8: Configuration files (tsconfig.json)
        let config_rules = get_stack_rulepacks("tsconfig.json", "{}", None);
        assert!(config_rules.contains("CONFIGURATION & TSCONFIG RULES"));
        assert!(config_rules.contains("TSCONFIG.JSON CONCISENESS & VALIDITY"));

        // Test 9: Middleware on error
        let mw_err_rules = get_stack_rulepacks("src/middleware.ts", "", Some("Error parsing `/dashboard/*`: Invalid middlewares found"));
        assert!(mw_err_rules.contains("NEXT.JS MIDDLEWARE & EDGE ROUTING RULES"));
        assert!(mw_err_rules.contains("PATH-TO-REGEXP MATCHERS"));
    }

    #[test]
    fn test_apply_targeted_edit() {
        let original = "
        <Toolbar>
          <IconButton edge=\"start\">
            <FaBars />
          </IconButton>
          <Typography variant=\"h6\" component=\"div\" sx={{ flexGrow: 1, ml: 1 }}>
            Axiom Dev
          </Typography>
          <IconButton>
            <IoMdNotifications />
          </IconButton>
        </Toolbar>
";

        // LLM generates target compressed onto one line
        let target = "<Typography variant=\"h6\" component=\"div\" sx={{ flexGrow: 1, ml: 1 }}>Axiom Dev</Typography>";
        let replacement = "<Typography variant=\"h6\" component=\"div\" sx={{ flexGrow: 1, ml: 1 }}>Construction Smart System</Typography>";

        let result = apply_targeted_edit(original, target, replacement);
        assert!(result.is_ok(), "Targeted edit should succeed: {:?}", result.err());
        let modified = result.unwrap();
        assert!(modified.contains("Construction Smart System"));
        assert!(!modified.contains("Axiom Dev"));
    }

    #[test]
    fn test_extract_related_project_files_for_error() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_rel_test_{}", uuid::Uuid::new_v4()));
        let comp_dir = temp_dir.join("src/components");
        let _ = std::fs::create_dir_all(&comp_dir);

        let navbar_file = comp_dir.join("Navbar.tsx");
        std::fs::write(&navbar_file, "export default function Navbar() { return <nav>Nav</nav>; }").unwrap();

        let layout_content = "import React from 'react';\nimport Navbar from '@/components/Navbar';\nexport default function RootLayout() { return <Navbar />; }";
        let err_msg = "Attempted import error: '@/components/Navbar' does not contain a default export (imported as 'Navbar').";

        let related = extract_related_project_files_for_error(&temp_dir, layout_content, "src/app/layout.tsx", err_msg);
        assert!(!related.is_empty());
        assert_eq!(related[0].0, "src/components/Navbar.tsx");
        assert!(related[0].1.contains("Navbar"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_resolve_app_router_route() {
        assert_eq!(resolve_app_router_route("src/app/page.tsx"), Some("/".to_string()));
        assert_eq!(resolve_app_router_route("src/app/page.jsx"), Some("/".to_string()));
        assert_eq!(resolve_app_router_route("src/app/dashboard/page.tsx"), Some("/dashboard".to_string()));
        assert_eq!(resolve_app_router_route("src/app/dashboard/data-grid/page.tsx"), Some("/dashboard/data-grid".to_string()));
        assert_eq!(resolve_app_router_route("src/app/(auth)/login/page.tsx"), Some("/login".to_string()));
        assert_eq!(resolve_app_router_route("src/app/(dashboard)/analytics/page.tsx"), Some("/analytics".to_string()));
        assert_eq!(resolve_app_router_route("src/app/users/[id]/page.tsx"), Some("/users/[id]".to_string()));
        assert_eq!(resolve_app_router_route("src/components/Navbar.tsx"), None);
        assert_eq!(resolve_app_router_route("src/app/api/auth/route.ts"), None);
    }

    #[test]
    fn test_apply_targeted_edit_duplicated_links() {
        let content = "
        <nav className=\"hidden md:flex\">
          <Link href=\"/data-grid\" className=\"text-gray-700\">Data Grid</Link>
        </nav>
        <div className=\"mobile-menu md:hidden\">
          <Link href=\"/data-grid\" className=\"text-gray-700\">Data Grid</Link>
        </div>
        ";
        let target = "<Link href=\"/data-grid\" className=\"text-gray-700\">Data Grid</Link>";
        let replacement = "<Link href=\"/dashboard/data-grid\" className=\"text-gray-700\">Data Grid</Link>";

        let result = apply_targeted_edit(content, target, replacement);
        assert!(result.is_ok(), "Specific duplicate target should be replaced across all occurrences");
        let modified = result.unwrap();
        assert!(!modified.contains("href=\"/data-grid\""));
        assert_eq!(modified.matches("href=\"/dashboard/data-grid\"").count(), 2);
    }

    #[test]
    fn test_sanitize_react_icons_imports() {
        let input = "import { HiOutlineChartBar, FaBars } from 'react-icons';\nimport React from 'react';";
        let output = sanitize_react_icons_imports(input);
        assert!(output.contains("import { FaBars } from 'react-icons/fa';"));
        assert!(output.contains("import { HiOutlineChartBar } from 'react-icons/hi';"));
        assert!(!output.contains("from 'react-icons';"));
    }

    #[test]
    fn test_resolve_best_matching_route() {
        let valid_routes = vec![
            "/".to_string(),
            "/dashboard".to_string(),
            "/dashboard/data-grid".to_string(),
            "/dashboard/detailed-analytics".to_string(),
            "/dashboard/kpi-widgets".to_string(),
            "/dashboard/settings".to_string(),
        ];

        assert_eq!(resolve_best_matching_route("/data-grid", &valid_routes), Some("/dashboard/data-grid".to_string()));
        assert_eq!(resolve_best_matching_route("/detailed-analytics", &valid_routes), Some("/dashboard/detailed-analytics".to_string()));
        assert_eq!(resolve_best_matching_route("/settings-panel", &valid_routes), Some("/dashboard/settings".to_string()));
        assert_eq!(resolve_best_matching_route("/dashboard", &valid_routes), None);
        assert_eq!(resolve_best_matching_route("/", &valid_routes), None);
    }

    #[test]
    fn test_sanitize_app_router_links_with_routes() {
        let valid_routes = vec![
            "/".to_string(),
            "/dashboard".to_string(),
            "/dashboard/data-grid".to_string(),
            "/dashboard/settings".to_string(),
        ];
        let input = r#"<Link href="/data-grid">Data Grid</Link><Link href="/settings-panel">Settings</Link>"#;
        let output = sanitize_app_router_links_with_routes(input, &valid_routes);
        assert!(output.contains(r#"href="/dashboard/data-grid""#));
        assert!(output.contains(r#"href="/dashboard/settings""#));
    }

    #[test]
    fn test_sanitize_unhandled_session_loading() {
        let input = "import { useSession } from 'next-auth/react';\nconst [loading, setLoading] = useState(true);\nuseEffect(() => {\n  if (session) {\n    fetchKpis();\n  }\n}, [session]);";
        let output = sanitize_unhandled_session_loading(input);
        assert!(output.contains("else {"));
        assert!(output.contains("setLoading(false);"));
    }


    #[test]
    fn test_nextjs_react_rulepack_contains_tanstack_query_v5() {
        assert!(NEXTJS_REACT_RULEPACK.contains("TANSTACK REACT QUERY V5 OBJECT SIGNATURE"));
        assert!(NEXTJS_REACT_RULEPACK.contains("useQuery({ queryKey: ['metrics'], queryFn: fetchMetrics })"));
    }


    #[test]
    fn test_sanitize_ts_interfaces() {
        let input = r#"
interface SkeletonProps {
  className?: string;
  data-axiom-component?: string;
  data-axiom-file?: string;
  aria-label?: string;
}

export type ButtonProps = {
  variant?: 'default' | 'outline';
  "data-axiom-component"?: string;
  data-testid?: string;
};
"#;
        let output = sanitize_ts_interfaces(input);
        assert!(!output.contains("data-axiom-component?:"));
        assert!(!output.contains("data-axiom-file?:"));
        assert!(output.contains(r#""aria-label"?: string;"#));
        assert!(output.contains(r#""data-testid"?: string;"#));

        let orphan_input = r#"import React from 'react';
export const Skeleton = () => {
  return (
    <div
      data-axiom-component="Skeleton"
      data-axiom-file="src/components/ui/skeleton.tsx"
    />
  );
};

data-axiom-component="Skeleton"
data-axiom-file="src/components/ui/skeleton.tsx"

export default Skeleton;
"#;
        let orphan_output = sanitize_ts_interfaces(orphan_input);
        assert_eq!(orphan_output.matches(r#"data-axiom-component="Skeleton""#).count(), 1);
        assert_eq!(orphan_output.matches(r#"data-axiom-file="src/components/ui/skeleton.tsx""#).count(), 1);
    }

    #[test]
    fn test_dual_export_synchronization() {
        // Case 1: Button with only default export gets named export { Button }
        let button_input = r#"
import React from 'react';
const Button: React.FC<any> = ({ children }) => <button>{children}</button>;
export default Button;
"#;
        let button_output = post_process_generated_file("src/components/ui/button.tsx", button_input, &serde_json::json!({}), false);
        assert!(button_output.contains("export default Button;"));
        assert!(button_output.contains("export { Button };"));

        // Case 2: Card with named export gets export default Card;
        let card_input = r#"
import React from 'react';
export const Card: React.FC<any> = ({ children }) => <div>{children}</div>;
export const CardHeader: React.FC<any> = ({ children }) => <div>{children}</div>;
"#;
        let card_output = post_process_generated_file("src/components/ui/card.tsx", card_input, &serde_json::json!({}), false);
        assert!(card_output.contains("export const Card:"));
        assert!(card_output.contains("export default Card;"));

        // Case 3: Kebab-case file alert-dialog.tsx gets PascalCase named/default export
        let alert_input = r#"
import React from 'react';
export default function AlertDialog() { return <div />; }
"#;
        let alert_output = post_process_generated_file("src/components/ui/alert-dialog.tsx", alert_input, &serde_json::json!({}), false);
        assert!(alert_output.contains("export default function AlertDialog"));
        assert!(alert_output.contains("export { AlertDialog };"));
    }

    #[test]
    fn test_extract_edits_ignores_explanation_and_supports_deletion() {
        // 1. Explanation mentioning "export " must NEVER be extracted as code replacement
        let payload = serde_json::json!({
            "explanation": "The error indicates that the name `Skeleton` is defined multiple times in the file. This is likely due to an additional export of the `Skeleton` component without a default export.",
            "edits": [
                {
                    "target": "export const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {",
                    "replacement": "const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {"
                }
            ]
        });
        let edits = extract_edits_from_json(&payload, "src/components/ui/skeleton.tsx");
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].get("target").and_then(|v| v.as_str()),
            Some("export const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {")
        );
        assert_eq!(
            edits[0].get("replacement").and_then(|v| v.as_str()),
            Some("const Skeleton: React.FC<SkeletonProps> = ({ className = '' }) => {")
        );

        // 2. Empty replacement ("") with a valid target must be preserved as a deletion edit
        let deletion_payload = serde_json::json!({
            "explanation": "The error indicates that the name 'Skeleton' is defined multiple times in the file. This is likely due to an additional export of the 'Skeleton' component without a default export.",
            "edits": [
                {
                    "target": "export default Skeleton;",
                    "replacement": ""
                }
            ]
        });
        let del_edits = extract_edits_from_json(&deletion_payload, "src/components/ui/skeleton.tsx");
        assert_eq!(del_edits.len(), 1);
        assert_eq!(del_edits[0].get("target").and_then(|v| v.as_str()), Some("export default Skeleton;"));
        assert_eq!(del_edits[0].get("replacement").and_then(|v| v.as_str()), Some(""));

        // 3. Natural language prose mentioning keywords is not classified as code
        assert!(!looks_like_code_payload(
            "The error indicates that the name `Skeleton` is defined multiple times in the file. This is likely due to an additional export of the `Skeleton` component without a default export."
        ));
    }

    #[test]
    fn test_context_isolation_ui_vs_backend_and_auth() {
        let temp_dir = std::env::temp_dir().join("axiom_test_ctx_isolation");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/lib")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/auth/[...nextauth]")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();

        std::fs::write(temp_dir.join("prisma/schema.prisma"), "model Project { id String @id }").unwrap();
        std::fs::write(temp_dir.join("src/lib/prisma.ts"), "export const prisma = {};").unwrap();
        std::fs::write(temp_dir.join("src/app/api/auth/[...nextauth]/route.ts"), "export const authOptions = {};").unwrap();
        std::fs::write(temp_dir.join("src/hooks/useProjects.ts"), "export const useProjects = () => [];").unwrap();

        let ui_ctx = get_context_files_for("src/app/dashboard/analytics/page.tsx", &temp_dir);
        assert!(!ui_ctx.contains("schema.prisma"));
        assert!(!ui_ctx.contains("src/lib/prisma.ts"));
        assert!(!ui_ctx.contains("[...nextauth]"));
        assert!(ui_ctx.contains("src/hooks/useProjects.ts"));

        let navbar_ctx = get_context_files_for("src/components/Navbar.tsx", &temp_dir);
        assert!(!navbar_ctx.contains("schema.prisma"));
        assert!(!navbar_ctx.contains("src/hooks/useProjects.ts"));

        let api_ctx = get_context_files_for("src/app/api/projects/route.ts", &temp_dir);
        assert!(api_ctx.contains("schema.prisma"));
        assert!(api_ctx.contains("src/lib/prisma.ts"));
        assert!(!api_ctx.contains("[...nextauth]"));

        let auth_ctx = get_context_files_for("src/app/login/page.tsx", &temp_dir);
        assert!(auth_ctx.contains("[...nextauth]"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_scoped_ui_rules_and_design_archetype() {
        let navbar_rules = get_scoped_ui_rules("src/components/Navbar.tsx");
        assert!(navbar_rules.contains("BRAND LOGO LOCKUP"));
        assert!(navbar_rules.contains("HAMBURGER MENU"));
        assert!(!navbar_rules.contains("SCOPED FOOTER LAYOUT & COMPACTNESS RULES"));
        assert!(!navbar_rules.contains("SCOPED ANALYTICS, KPI & DATA VISUALIZATION RULES"));

        let footer_rules = get_scoped_ui_rules("src/components/Footer.tsx");
        assert!(footer_rules.contains("SCOPED FOOTER LAYOUT & COMPACTNESS RULES"));
        assert!(!footer_rules.contains("BRAND LOGO LOCKUP"));

        let analytics_rules = get_scoped_ui_rules("src/app/dashboard/analytics/page.tsx");
        assert!(analytics_rules.contains("SCOPED ANALYTICS, KPI & DATA VISUALIZATION RULES"));
        assert!(analytics_rules.contains("SCOPED PAGE & DASHBOARD PREVIEW RESILIENCE RULES"));

        let api_rulepacks = get_stack_rulepacks("src/app/api/projects/route.ts", "import { NextResponse } from 'next/server';", None);
        assert!(!api_rulepacks.contains("UI DESIGN & LAYOUT FOUNDATION"));

        let archetype = resolve_design_archetype_prompt("NexusFlow Serbia", "B2B SaaS project management and analytics dashboard", "saas");
        assert!(archetype.contains("Executive B2B SaaS & Collaborative Workspace"));
    }

    #[test]
    fn test_sanitize_unhandled_session_loading_strips_preview_blockers() {
        let analytics_input = r#"
"use client";
import React from 'react';
import { useSession } from 'next-auth/react';

const AnalyticsPage = () => {
  const { data: session } = useSession();

  if (!session) {
    return <div>Loading...</div>;
  }

  return (
    <div className="max-w-7xl mx-auto p-6">
      <h1>Analytics Overview</h1>
    </div>
  );
};
export default AnalyticsPage;
"#;
        let processed = post_process_generated_file("src/app/dashboard/analytics/page.tsx", analytics_input, &serde_json::json!({}), true);
        assert!(!processed.contains("return <div>Loading...</div>;"));
        assert!(processed.contains(r#"<div data-axiom-component="Page" data-axiom-file="src/app/dashboard/analytics/page.tsx" className="max-w-7xl mx-auto p-6">"#));

        let dash_input = r#"
"use client";
import React from 'react';
import { useSession } from 'next-auth/react';
import Link from 'next/link';

export default function DashboardPage() {
  const { status } = useSession();
  if (status === 'unauthenticated') return <Link href="/login">Log in</Link>;
  return (
    <main className="p-8">
      <h1>Dashboard KPI</h1>
    </main>
  );
}
"#;
        let processed_dash = post_process_generated_file("src/app/dashboard/page.tsx", dash_input, &serde_json::json!({}), true);
        assert!(!processed_dash.contains("return <Link href=\"/login\">Log in</Link>;"));
        assert!(processed_dash.contains("<h1>Dashboard KPI</h1>"));
    }

    #[test]
    fn test_extract_file_path_windows_and_react_hook_imports() {
        let win_line = " ⨯ src\\components\\Navbar.tsx (14:51) @ useState";
        assert_eq!(extract_file_path_from_line(win_line), Some("src\\components\\Navbar.tsx".to_string()));

        let navbar_missing_usestate = r#"import React from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';

const Navbar: React.FC = () => {
  const pathname = usePathname();
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);
  return <nav className="p-4">Brand</nav>;
};
export default Navbar;
"#;
        let processed = post_process_generated_file("src/components/Navbar.tsx", navbar_missing_usestate, &serde_json::json!({}), true);
        assert!(processed.starts_with("\"use client\";"));
        assert!(processed.contains("import React, { useState } from 'react';"));
    }

    #[test]
    fn test_offline_package_sanitizer_and_prisma_fk_healer() {
        let mut pkg_json = serde_json::json!({
            "name": "skillmatch",
            "dependencies": {
                "next": "14.2.35",
                "fi": "*",
                "typescript": "^4.5.2"
            },
            "devDependencies": {
                "typescript": "^5.3.3",
                "@typescript-eslint/parser": "^4.29.2",
                "@typescript-eslint/eslint-plugin": "^4.29.2",
                "eslint-config-next": "14.2.35",
                "eslint": "^8.57.0"
            }
        });
        assert!(sanitize_package_json_value_for_offline(&mut pkg_json));
        let deps = pkg_json["dependencies"].as_object().unwrap();
        let dev_deps = pkg_json["devDependencies"].as_object().unwrap();
        assert!(!deps.contains_key("fi"));
        assert!(deps.contains_key("react-icons"));
        assert!(!deps.contains_key("typescript"));
        assert!(dev_deps.contains_key("typescript"));
        assert!(!dev_deps.contains_key("@typescript-eslint/parser"));
        assert!(!dev_deps.contains_key("eslint-config-next"));

        let broken_prisma = r#"datasource db {
  provider = "sqlite"
  url      = env("DATABASE_URL")
}

generator client {
  provider = "prisma-client-js"
}

model UserProfile {
  id   String @id @default(cuid())
  name String
}

model Payment {
  id          String      @id @default(cuid())
  amount      Float
  userProfile UserProfile @relation(fields: [userProfileId], references: [id])
}
"#;
        let fixed_prisma = post_process_generated_file("prisma/schema.prisma", broken_prisma, &serde_json::json!({}), false);
        assert!(fixed_prisma.contains("userProfileId"));
        assert!(fixed_prisma.contains("payments"));

        let partial_ai_snippet = r#"model Payment {
  id            String      @id @default(cuid())
  amount        Float
  userProfileId String
  userProfile   UserProfile @relation(fields: [userProfileId], references: [id])
}"#;
        let merged = merge_prisma_ai_response(&fixed_prisma, partial_ai_snippet);
        assert!(merged.contains("datasource db"));
        assert!(merged.contains("generator client"));
        assert!(merged.contains("model UserProfile"));
        assert!(merged.contains("model Payment"));
    }

    #[test]
    fn test_sanitize_prisma_relations_removes_ghosts_and_enforces_unique() {
        let input_schema = r#"datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id                String         @id @default(cuid())
  subscriptions     Subscription[] @relation("UserSubscriptions")
  userSubscriptions Subscription[] @relation("UserSubscriptionsSecondary")
}

model Project {
  id                        String      @id @default(cuid())
  projectAnalytics          Analytics?  @relation("ProjectAnalytics")
  projectAnalyticsSecondary Analytics[] @relation("ProjectAnalyticsSecondary")
}

model Subscription {
  id                String         @id @default(cuid())
  userId            String
  user              User           @relation("UserSubscriptions", fields: [userId], references: [id])
  userSubscriptions Subscription[] @relation("UserSubscriptionsSecondary")
}

model Analytics {
  id                        String      @id @default(cuid())
  projectId                 String
  project                   Project     @relation("ProjectAnalytics", fields: [projectId], references: [id])
  projectAnalyticsSecondary Analytics[] @relation("ProjectAnalyticsSecondary")
}
"#;
        let sanitized = sanitize_prisma_relations(input_schema);
        
        // 1. Ghost duplicate relation on User is pruned
        assert!(!sanitized.contains("UserSubscriptionsSecondary"));
        assert!(!sanitized.contains("userSubscriptions"));
        assert!(sanitized.contains("subscriptions     Subscription[] @relation(\"UserSubscriptions\")"));

        // 2. Ghost self-relation on Subscription is pruned
        assert!(!sanitized.contains("Subscription[] @relation(\"UserSubscriptionsSecondary\")"));

        // 3. Ghost duplicate and self relations on Project & Analytics are pruned
        assert!(!sanitized.contains("ProjectAnalyticsSecondary"));
        assert!(!sanitized.contains("projectAnalyticsSecondary"));

        // 4. 1-to-1 relation foreign key on Analytics gets @unique
        assert!(sanitized.contains("projectId                 String @unique"));
    }

    #[test]
    fn test_ensure_directory_barrel_files_demanded_barrels() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_barrel_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let src_dir = temp_dir.join("src");
        let app_dir = src_dir.join("app");
        let hooks_dir = src_dir.join("hooks");
        let types_dir = src_dir.join("types");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::create_dir_all(&types_dir).unwrap();

        // Actual hook file in src/hooks
        std::fs::write(hooks_dir.join("useProjects.ts"), "export function useProjects() { return []; }").unwrap();

        // Write a page that imports from @/hooks and @/types and @/utils (which does not exist yet)
        let page_tsx = app_dir.join("page.tsx");
        std::fs::write(&page_tsx, r#"
import React from 'react';
import { useProjects, useAuth } from '@/hooks';
import { UserProfile, DashboardStats } from '@/types';
import { formatCurrency } from '@/utils';

export default function Page() {
    return <div>Test</div>;
}
"#).unwrap();

        ensure_directory_barrel_files(&temp_dir);

        let hooks_index = hooks_dir.join("index.ts");
        let types_index = types_dir.join("index.ts");
        let utils_index = src_dir.join("utils").join("index.ts");

        assert!(hooks_index.exists(), "src/hooks/index.ts should have been created for existing hooks folder");
        assert!(types_index.exists(), "src/types/index.ts should have been created for existing types folder");
        assert!(utils_index.exists(), "src/utils/index.ts should have been created on demand for @/utils");

        let hooks_content = std::fs::read_to_string(&hooks_index).unwrap();
        assert!(hooks_content.contains("export * from './useProjects';"));
        assert!(hooks_content.contains("export const useAuth: any = () => ({} as any);"));

        let types_content = std::fs::read_to_string(&types_index).unwrap();
        assert!(types_content.contains("export type UserProfile = any;"));
        assert!(types_content.contains("export type DashboardStats = any;"));

        let utils_content = std::fs::read_to_string(&utils_index).unwrap();
        assert!(utils_content.contains("export const formatCurrency: any = () => ({} as any);"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generate_project_metadata_resolves_alias_dependencies() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_meta_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let src_dir = temp_dir.join("src");
        let app_dir = src_dir.join("app");
        let components_dir = src_dir.join("components");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&components_dir).unwrap();

        let navbar_file = components_dir.join("Navbar.tsx");
        std::fs::write(&navbar_file, "export default function Navbar() { return <nav>Navbar</nav>; }").unwrap();

        let page_file = app_dir.join("page.tsx");
        std::fs::write(&page_file, "import Navbar from '@/components/Navbar';\nexport default function Page() { return <Navbar />; }").unwrap();

        let files_list = vec![
            serde_json::json!({ "path": "src/app/page.tsx" }),
            serde_json::json!({ "path": "src/components/Navbar.tsx" }),
        ];

        generate_project_metadata(&temp_dir, "test-proj", &files_list);

        let meta_file = temp_dir.join(".axiom").join("metadata.json");
        assert!(meta_file.exists());
        let meta_str = std::fs::read_to_string(&meta_file).unwrap();
        let meta_val: serde_json::Value = serde_json::from_str(&meta_str).unwrap();

        let page_deps = meta_val.get("dependencies").and_then(|d| d.get("src/app/page.tsx")).and_then(|v| v.as_array()).unwrap();
        assert_eq!(page_deps.len(), 1);
        assert_eq!(page_deps[0].as_str(), Some("src/components/Navbar.tsx"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_harvester_extracts_and_updates_live_builder_guide() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_harvester_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let prisma_sample = r#"
datasource db {
  provider = "sqlite"
  url = "file:./dev.db"
}

generator client {
  provider = "prisma-client-js"
}

enum Role {
  USER
  ADMIN
}

enum Status {
  ACTIVE
  ARCHIVED
}

model User {
  id        String   @id @default(cuid())
  email     String   @unique
  name      String?
  role      Role     @default(USER)
  projects  Project[]
  createdAt DateTime @default(now())
}

model Project {
  id        String   @id @default(cuid())
  name      String
  status    Status   @default(ACTIVE)
  ownerId   String
  owner     User     @relation(fields: [ownerId], references: [id])
}
"#;

        // 1. Harvest schema.prisma
        harvest_file_contract_to_lbg(&temp_dir, "prisma/schema.prisma", prisma_sample);

        // 2. Harvest API route
        let route_sample = r#"
import { NextResponse, NextRequest } from 'next/server';

export async function GET(request: NextRequest) {
    return NextResponse.json([]);
}

export async function POST(request: NextRequest) {
    return NextResponse.json({ id: '1' });
}
"#;
        harvest_file_contract_to_lbg(&temp_dir, "src/app/api/projects/route.ts", route_sample);

        // 3. Harvest Hook
        let hook_sample = r#"
import { useState, useEffect } from 'react';

export function useProjects() {
    const [projects, setProjects] = useState([]);
    return { projects };
}
"#;
        harvest_file_contract_to_lbg(&temp_dir, "src/hooks/useProjects.ts", hook_sample);

        // Verify live_builder_guide.json
        let lbg_path = temp_dir.join(".axiom").join("live_builder_guide.json");
        assert!(lbg_path.exists());
        let lbg_str = std::fs::read_to_string(&lbg_path).unwrap();
        let lbg: serde_json::Value = serde_json::from_str(&lbg_str).unwrap();

        let shared = lbg.get("sharedContracts").unwrap();
        let db = shared.get("database").unwrap();
        assert!(db.get("models").and_then(|m| m.get("User")).is_some());
        assert!(db.get("models").and_then(|m| m.get("Project")).is_some());
        assert!(db.get("enums").and_then(|e| e.get("Role")).is_some());

        let api = shared.get("api").and_then(|a| a.get("endpoints")).unwrap();
        assert!(api.get("/api/projects").is_some());
        let methods = api.get("/api/projects").and_then(|m| m.as_array()).unwrap();
        assert!(methods.iter().any(|v| v.as_str() == Some("GET")));
        assert!(methods.iter().any(|v| v.as_str() == Some("POST")));

        let hooks = shared.get("hooks").unwrap();
        assert!(hooks.get("useProjects").is_some());

        // Verify prompt formatters
        let db_prompt = format_database_contract_prompt(db);
        assert!(db_prompt.contains("ACTIVE DATABASE MODELS:"));
        assert!(db_prompt.contains("model Project"));
        assert!(db_prompt.contains("model User"));
        assert!(db_prompt.contains("enum Role { USER, ADMIN }"));

        let api_prompt = format_api_endpoints_prompt(api);
        assert!(api_prompt.contains("VERIFIED API ENDPOINTS TO CALL:"));
        assert!(api_prompt.contains("/api/projects"));

        let hooks_prompt = format_hooks_prompt(hooks);
        assert!(hooks_prompt.contains("VERIFIED CUSTOM HOOKS (Import from '@/hooks'):"));
        assert!(hooks_prompt.contains("useProjects"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_api_route_cot_and_prisma_singleton_scope() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_api_cot_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(temp_dir.join(".axiom"));

        // 1. Verify src/lib/prisma.ts gets singleton scope
        let prisma_lib_prompt = get_expert_system_prompt("src/lib/prisma.ts", "nextjs", "", "", "", "", &temp_dir);
        assert!(prisma_lib_prompt.contains("PRISMA SINGLETON RULES:"));
        assert!(prisma_lib_prompt.contains("globalForPrisma.prisma ??"));
        assert!(prisma_lib_prompt.contains("export default prisma;"));

        // 2. Populate live_builder_guide with harvested database models
        let lbg = serde_json::json!({
            "projectSpecification": { "category": "edtech" },
            "sharedContracts": {
                "database": {
                    "models": {
                        "Course": {
                            "fields": ["id", "title", "price"],
                            "relations": ["lessons: Lesson[]"]
                        }
                    },
                    "enums": {
                        "CourseStatus": ["DRAFT", "PUBLISHED"]
                    }
                },
                "api": { "endpoints": {} },
                "hooks": {}
            }
        });
        std::fs::write(temp_dir.join(".axiom/live_builder_guide.json"), serde_json::to_string_pretty(&lbg).unwrap()).unwrap();

        // 3. Verify API route prompt gets CoT, domain archetype, and active models
        let api_prompt = get_expert_system_prompt("src/app/api/courses/route.ts", "nextjs", "", "", "", "", &temp_dir);
        assert!(api_prompt.contains("API DOMAIN ARCHETYPE: EdTech, HealthCare & Progression Tracking"));
        assert!(api_prompt.contains("BACKEND API GENERATION CHAIN-OF-THOUGHT"));
        assert!(api_prompt.contains("Step 1 [Imports & Core Bindings]"));
        assert!(api_prompt.contains("Step 2 [Schema Validation & Safe Defaults]"));
        assert!(api_prompt.contains("Step 3 [Typed Relational Queries]"));
        assert!(api_prompt.contains("Step 4 [Local Resilience & Status Gate]"));
        assert!(api_prompt.contains("ACTIVE DATABASE MODELS:"));
        assert!(api_prompt.contains("model Course"));
        assert!(api_prompt.contains("scalars: [id, title, price]"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_custom_hook_and_ui_cot_scopes() {
        let temp_dir = std::env::temp_dir().join(format!("axiom_hook_ui_cot_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::create_dir_all(temp_dir.join(".axiom"));

        // Setup live builder guide with endpoints and hooks
        let lbg = serde_json::json!({
            "projectSpecification": { "category": "saas" },
            "sharedContracts": {
                "database": {},
                "api": {
                    "endpoints": {
                        "/api/projects": ["GET", "POST"]
                    }
                },
                "hooks": {
                    "useProjects": "@/hooks/useProjects"
                }
            }
        });
        std::fs::write(temp_dir.join(".axiom/live_builder_guide.json"), serde_json::to_string_pretty(&lbg).unwrap()).unwrap();

        // 1. Verify custom hook scope
        let hook_prompt = get_expert_system_prompt("src/hooks/useProjects.ts", "nextjs", "", "", "", "", &temp_dir);
        assert!(hook_prompt.contains("Senior React Hooks & State Management Architect"));
        assert!(hook_prompt.contains("CUSTOM REACT HOOK GENERATION CHAIN-OF-THOUGHT"));
        assert!(hook_prompt.contains("Step 1 [Signature & Parameter Safety]"));
        assert!(hook_prompt.contains("Step 4 [Dual Export Compatibility]"));
        assert!(hook_prompt.contains("VERIFIED API ENDPOINTS TO CALL:"));
        assert!(hook_prompt.contains("/api/projects"));
        assert!(!hook_prompt.contains("ROOT LAYOUT ARCHITECTURE RULES"));

        // 2. Verify UI Page scope
        let ui_prompt = get_expert_system_prompt("src/app/dashboard/projects/page.tsx", "nextjs", "", "", "", "", &temp_dir);
        assert!(ui_prompt.contains("FRONTEND UI GENERATION CHAIN-OF-THOUGHT"));
        assert!(ui_prompt.contains("Step 1 [Client Boundary & Imports]"));
        assert!(ui_prompt.contains("Step 2 [Domain Mock State]"));
        assert!(ui_prompt.contains("Step 3 [Visual Hierarchy & Layout]"));
        assert!(ui_prompt.contains("Step 4 [Navigation & Dual Export]"));
        assert!(ui_prompt.contains("VERIFIED API ENDPOINTS TO CALL:"));
        assert!(ui_prompt.contains("/api/projects"));
        assert!(ui_prompt.contains("VERIFIED CUSTOM HOOKS (Import from '@/hooks'):"));
        assert!(ui_prompt.contains("useProjects"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_disambiguate_prisma_ambiguous_relations() {
        let broken_schema = r#"
datasource db {
  provider = "sqlite"
  url      = "file:./dev.db"
}

generator client {
  provider = "prisma-client-js"
}

model Transaction {
  id                  String   @id @default(cuid())
  amount              Float
  sourceWalletId      String
  sourceWallet        Wallet   @relation(fields: [sourceWalletId], references: [id])
  destinationWalletId String?
  destinationWallet   Wallet?  @relation(fields: [destinationWalletId], references: [id])
}

model Wallet {
  id        String   @id @default(cuid())
  balance   Float
}
"#;

        let sanitized = post_process_generated_file("prisma/schema.prisma", broken_schema, &serde_json::json!({}), false);
        // Verify both sides received explicit relation names
        assert!(sanitized.contains("@relation(\"TransactionSourceWallet\""));
        assert!(sanitized.contains("@relation(\"TransactionDestinationWallet\""));
        // Verify Wallet received named back-relations
        assert!(sanitized.contains("sourceWalletTransactions"));
        assert!(sanitized.contains("destinationWalletTransactions"));
        // Verify ambiguous collision field `transactionsTransaction` was NEVER generated
        assert!(!sanitized.contains("transactionsTransaction"));
    }

    #[test]
    fn test_clean_react_navigation_hooks_from_react_imports() {
        // Case 1: Mixed import from react
        let input1 = "import { useRouter, useState, useEffect } from 'react';\nimport { useRouter } from 'next/navigation';";
        let cleaned1 = clean_react_navigation_hooks_from_react_imports(input1);
        assert!(cleaned1.contains("import { useState, useEffect } from 'react';"));
        assert!(cleaned1.contains("import { useRouter } from 'next/navigation';"));
        assert!(!cleaned1.contains("useRouter, useState"));

        // Case 2: Solo useRouter from react
        let input2 = "import { useRouter } from 'react';\nexport default function Page() { return <div />; }";
        let cleaned2 = clean_react_navigation_hooks_from_react_imports(input2);
        assert!(!cleaned2.contains("from 'react'"));

        // Case 3: Default React with useRouter
        let input3 = "import React, { useRouter, useState } from 'react';";
        let cleaned3 = clean_react_navigation_hooks_from_react_imports(input3);
        assert_eq!(cleaned3.trim(), "import React, { useState } from 'react';");

        // Case 4: Full page post-processing
        let page_code = r#"import { useRouter } from 'next/navigation';
import { useRouter, useState, useEffect } from 'react';

export default function AnalyticsPage() {
  const router = useRouter();
  const [data, setData] = useState([]);
  return <div>{data.length}</div>;
}
"#;
        let processed = post_process_generated_file("src/app/real-time-analytics/page.tsx", page_code, &serde_json::json!({}), false);
        // Ensure useRouter is only imported once from next/navigation, and NOT from react
        assert!(processed.contains("import { useRouter } from 'next/navigation';"));
        assert!(processed.contains("import { useState, useEffect } from 'react';"));
        assert!(!processed.contains("useRouter, useState"));
    }

    #[test]
    fn test_disambiguate_prisma_self_relations_and_mutual_relations() {
        let self_rel_schema = r#"
model Category {
  id        String     @id @default(cuid())
  parentId  String?
  parent    Category?  @relation(fields: [parentId], references: [id])
  children  Category[]
}
"#;
        let sanitized = post_process_generated_file("prisma/schema.prisma", self_rel_schema, &serde_json::json!({}), false);
        // Verify self-relation hierarchy receives relation name
        assert!(sanitized.contains("@relation(\"CategoryHierarchy\""));

        // Mutual relations test
        let mutual_schema = r#"
model User {
  id          String   @id @default(cuid())
  companyId   String
  company     Company  @relation(fields: [companyId], references: [id])
}

model Company {
  id          String   @id @default(cuid())
  ownerId     String
  owner       User     @relation(fields: [ownerId], references: [id])
}
"#;
        let sanitized_mutual = post_process_generated_file("prisma/schema.prisma", mutual_schema, &serde_json::json!({}), false);
        assert!(sanitized_mutual.contains("@relation(\"UserCompany\""));
        assert!(sanitized_mutual.contains("@relation(\"CompanyOwner\""));
    }

    #[test]
    fn test_prisma_referential_actions_and_carbonchain_assigned_to_healing() {
        // 1. Referential action normalization inside @relation(...) without mutating enum/scalar names
        let raw_actions = r#"
enum ActionType {
  NO_ACTION
}

model Item {
  id       String  @id @default(cuid())
  parentId String?
  action   ActionType @default(NO_ACTION)
  parent   Item?   @relation(fields: [parentId], references: [id], onDelete: SET_NULL, onUpdate: CASCADE)
  otherId  String?
  other    Other?  @relation(fields: [otherId], references: [id], onDelete: NO_ACTION, onUpdate: SET_DEFAULT)
  // onDelete: SET_NULL comment must remain untouched
}
"#;
        let normalized = normalize_prisma_referential_actions(raw_actions);
        assert!(normalized.contains("onDelete: SetNull, onUpdate: Cascade"));
        assert!(normalized.contains("onDelete: NoAction, onUpdate: SetDefault"));
        assert!(normalized.contains("NO_ACTION\n"));
        assert!(normalized.contains("@default(NO_ACTION)"));
        assert!(normalized.contains("// onDelete: SET_NULL comment must remain untouched"));

        // 2. extract_prisma_relation_name must ignore map: "..." and extract positional or name: "..."
        assert_eq!(
            extract_prisma_relation_name("  user User @relation(fields: [userId], references: [id], map: \"fk_user\")"),
            None
        );
        assert_eq!(
            extract_prisma_relation_name("  assignedTo User? @relation(fields: [assignedToId], references: [id], onDelete: SetNull, name: \"AssignedTo\")"),
            Some("AssignedTo".to_string())
        );

        // 3. CarbonChain Check 3 scenario (SET_NULL + explicit name: "AssignedTo" + un-named User.tasks)
        let check3_schema = r#"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id           String        @id @default(cuid())
  email        String        @unique
  tasks        Task[]
}

model Task {
  id           String   @id @default(cuid())
  assignedToId String?
  assignedTo   User?    @relation(fields: [assignedToId], references: [id], onDelete: SET_NULL, name: "AssignedTo")
}
"#;
        let fixed_check3 = post_process_generated_file("prisma/schema.prisma", check3_schema, &serde_json::json!({}), false);
        assert!(fixed_check3.contains("onDelete: SetNull"), "SET_NULL should become SetNull:\n{}", fixed_check3);
        assert!(!fixed_check3.contains("SET_NULL"));
        assert!(!fixed_check3.contains("tasksTask"), "must NOT synthesize duplicate tasksTask:\n{}", fixed_check3);
        assert!(fixed_check3.contains("tasks        Task[] @relation(\"AssignedTo\")"), "User.tasks should receive @relation(\"AssignedTo\"):\n{}", fixed_check3);

        // 4. CarbonChain Check 4 full corrupted schema (both `tasks` and `tasksTask` + dual Project/Analytics + TeamProjects M2M)
        let check4_schema = r#"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "postgresql"
  url      = env("DATABASE_URL")
}

model User {
  id            String   @id @default(cuid())
  email         String   @unique
  projects      Project[]
  teams         Team[]
  tasks        Task[] @relation("AssignedTo")
  subscriptions Subscription[]
  tasksTask    Task[] @relation("AssignedTo")
}

model Team {
  id        String   @id @default(cuid())
  name      String
  users     User[]
  projects  Project[] @relation("TeamProjects")
  subscriptions Subscription[]
}

model Project {
  id          String   @id @default(cuid())
  ownerId     String
  owner       User     @relation(fields: [ownerId], references: [id])
  tasks       Task[]
  analytics   Analytics? @relation("ProjectAnalytics", fields: [analyticsId], references: [id])
  analyticsId String?  @unique
  projectAnalytics Analytics? @relation("AnalyticsProject")
  teams       Team[] @relation("TeamProjects")
}

model Task {
  id           String   @id @default(cuid())
  projectId    String
  project      Project  @relation(fields: [projectId], references: [id])
  assignedTo   User?    @relation(fields: [assignedToId], references: [id], onDelete: SET_NULL, name: "AssignedTo")
  assignedToId String?
}

model Subscription {
  id     String  @id @default(cuid())
  userId String?
  teamId String?
  user   User?   @relation(fields: [userId], references: [id])
  team   Team?   @relation(fields: [teamId], references: [id])
}

model Analytics {
  id               String   @id @default(cuid())
  projectId        String   @unique
  project          Project  @relation("AnalyticsProject", fields: [projectId], references: [id])
  analyticsProject Project? @relation("ProjectAnalytics")
}
"#;
        let healed_check4 = post_process_generated_file("prisma/schema.prisma", check4_schema, &serde_json::json!({}), false);
        assert!(!healed_check4.contains("tasksTask"), "duplicate tasksTask must be pruned:\n{}", healed_check4);
        assert!(healed_check4.contains("tasks        Task[] @relation(\"AssignedTo\")"));
        assert!(!healed_check4.contains("SET_NULL"));
        assert!(healed_check4.contains("onDelete: SetNull"));
        assert!(healed_check4.contains("projects  Project[] @relation(\"TeamProjects\")"));
        assert!(healed_check4.contains("teams       Team[] @relation(\"TeamProjects\")"));
        assert!(healed_check4.contains("projectAnalytics Analytics? @relation(\"AnalyticsProject\")"));
        assert!(healed_check4.contains("analyticsProject Project? @relation(\"ProjectAnalytics\")"));

        // 4b. Even if `tasksTask` precedes `tasks` in User, natural `tasks` wins over synthetic `tasksTask`
        let reversed_schema = check4_schema
            .replace("  tasks        Task[] @relation(\"AssignedTo\")\n", "")
            .replace("  tasksTask    Task[] @relation(\"AssignedTo\")", "  tasksTask    Task[] @relation(\"AssignedTo\")\n  tasks        Task[] @relation(\"AssignedTo\")");
        let healed_reversed = post_process_generated_file("prisma/schema.prisma", &reversed_schema, &serde_json::json!({}), false);
        assert!(!healed_reversed.contains("tasksTask"));
        assert!(healed_reversed.contains("tasks        Task[] @relation(\"AssignedTo\")"));

        // 5. Self-relation paired child field must survive sanitize_prisma_relations AND full pipeline
        let self_rel_with_set_null = r#"
model Category {
  id       String     @id @default(cuid())
  parentId String?
  parent   Category?  @relation("CategoryHierarchy", fields: [parentId], references: [id], onDelete: SET_NULL)
  children Category[] @relation("CategoryHierarchy")
}
"#;
        let healed_self = post_process_generated_file("prisma/schema.prisma", self_rel_with_set_null, &serde_json::json!({}), false);
        assert!(healed_self.contains("onDelete: SetNull"));
        assert!(healed_self.contains("children Category[] @relation(\"CategoryHierarchy\")"), "paired children self-relation was wrongly deleted:\n{}", healed_self);
    }

    #[test]
    fn test_npm_self_healing_shell_quote_and_webpack_dev_server() {
        // 1. ETARGET parser extracts package name cleanly for transitive dependency `shell-quote@^1.10.0.`
        let npm_log = "npm error code ETARGET\nnpm error notarget No matching version found for shell-quote@^1.10.0.\nnpm error notarget In most cases you or one of your dependencies...";
        let parsed = analyze_npm_install_failure(npm_log);
        assert!(matches!(parsed, Some(NpmFailureAction::SetWildcard(ref p)) if p == "shell-quote"));

        // 2. Offline sanitizer strips hallucinated `webpack-dev-server` etc.
        let mut pkg_json = serde_json::json!({
            "name": "carbonchain-navigator",
            "dependencies": {
                "@auth/prisma-adapter": "^1.0.12",
                "@prisma/client": "5.22.0",
                "next": "14.2.35",
                "webpack-dev-server": "^4.7.3",
                "webpack-hot-middleware": "^2.25.4"
            },
            "devDependencies": {
                "html-webpack-plugin": "^5.6.0",
                "prisma": "5.22.0"
            }
        });
        assert!(sanitize_package_json_value_for_offline(&mut pkg_json));
        let deps = pkg_json["dependencies"].as_object().unwrap();
        let dev_deps = pkg_json["devDependencies"].as_object().unwrap();
        assert!(!deps.contains_key("webpack-dev-server"));
        assert!(!deps.contains_key("webpack-hot-middleware"));
        assert!(!dev_deps.contains_key("html-webpack-plugin"));
        assert!(deps.contains_key("@auth/prisma-adapter"));
        assert!(deps.contains_key("next"));

        // 3. No global wildcard override without parent; parent-scoped override only when parent is known
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_pkg_heal_test_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let pkg_path = temp_dir.join("package.json");
        std::fs::write(
            &pkg_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "test-app",
                "dependencies": {
                    "next": "14.2.35",
                    "some-direct-pkg": "^1.2.3"
                },
                "devDependencies": {
                    "webpack-dev-server": "^4.15.2"
                }
            })).unwrap(),
        ).unwrap();

        // Unknown parent -> refuses to pollute package.json with global overrides
        assert!(!apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(npm_log)));
        let after_unknown: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert!(after_unknown.get("overrides").is_none());

        // Known parent in log -> writes parent-scoped override once, second call is a no-op
        let parented_npm_log = "npm error code ETARGET\nnpm error While resolving: test-app@0.1.0\nnpm error notarget No matching version found for shell-quote@^1.10.0.\nnpm error requested by webpack-dev-server@4.15.2";
        assert!(apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(parented_npm_log)));
        assert!(!apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(parented_npm_log)));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert!(saved["overrides"].get("shell-quote").is_none(), "Must NEVER emit global overrides['shell-quote']!");
        assert_eq!(saved["overrides"]["webpack-dev-server"]["shell-quote"], serde_json::json!("*"));

        // Direct non-core dependency: first call mutates "^1.2.3" -> "*" and returns true; second call returns false (no-op)
        assert!(set_package_version_to_wildcard(&pkg_path, "some-direct-pkg"));
        assert!(!set_package_version_to_wildcard(&pkg_path, "some-direct-pkg"));
        let saved2: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert_eq!(saved2["dependencies"]["some-direct-pkg"], serde_json::json!("*"));

        // Core framework lock: `next` can NEVER be downgraded to "*" or removed
        assert!(!set_package_version_to_wildcard(&pkg_path, "next"));
        assert!(!remove_package_from_json(&pkg_path, "next"));
        let saved3: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert_eq!(saved3["dependencies"]["next"], serde_json::json!("14.2.35"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_axiom_contracts_harvester_refresh_and_prompt() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_contracts_test_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join(".axiom")).unwrap();
        // Simulate corrupted/empty stub like v0.1.45 CarbonChain Navigator
        std::fs::write(
            temp_dir.join(".axiom").join("axiom-contracts.json"),
            r#"{"lastUpdated":"2026-01-01T00:00:00Z","totalFilesTracked":0}"#,
        ).unwrap();

        let schema_code = "enum Role {\n  ADMIN\n  MEMBER\n}\nmodel Supplier {\n  id String @id @default(cuid())\n}\n";
        let api_code = "import { NextResponse } from 'next/server';\nexport async function GET() { return NextResponse.json([]); }\n";
        let hook_code = "import { useState, useEffect } from 'react';\nexport function useSuppliers() {\n  const [suppliers, setSuppliers] = useState([]);\n  useEffect(() => { fetch('/api/suppliers'); }, []);\n  return { suppliers };\n}\n";
        let nav_code = "import Link from 'next/link';\nimport { useRouter } from 'next/navigation';\nexport default function Navbar() {\n  const router = useRouter();\n  const [isMenuOpen, setIsMenuOpen] = useState(false);\n  return <nav onClick={() => router.push('/dashboard')}><Link href=\"/suppliers\">Suppliers</Link></nav>;\n}\n";

        std::fs::create_dir_all(temp_dir.join("prisma")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app/api/suppliers")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::write(temp_dir.join("prisma/schema.prisma"), schema_code).unwrap();
        std::fs::write(temp_dir.join("src/app/api/suppliers/route.ts"), api_code).unwrap();
        std::fs::write(temp_dir.join("src/hooks/useSuppliers.tsx"), hook_code).unwrap();
        std::fs::write(temp_dir.join("src/components/Navbar.tsx"), nav_code).unwrap();

        let files_list = vec![
            serde_json::json!({ "path": "prisma/schema.prisma" }),
            serde_json::json!({ "path": "src/app/api/suppliers/route.ts" }),
            serde_json::json!({ "path": "src/hooks/useSuppliers.tsx" }),
            serde_json::json!({ "path": "src/components/Navbar.tsx" }),
        ];

        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "prisma/schema.prisma", schema_code, &files_list);
        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "src/app/api/suppliers/route.ts", api_code, &files_list);
        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "src/hooks/useSuppliers.tsx", hook_code, &files_list);
        harvest_file_to_axiom_contracts(&temp_dir, "proj-1", "src/components/Navbar.tsx", nav_code, &files_list);

        let root_contracts: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(temp_dir.join("axiom-contracts.json")).unwrap()
        ).unwrap();
        assert_eq!(root_contracts["version"], serde_json::json!("1.0"));
        assert_eq!(root_contracts["projectId"], serde_json::json!("proj-1"));
        assert_eq!(root_contracts["totalFilesTracked"], serde_json::json!(4));

        let files_arr = root_contracts["files"].as_array().unwrap();
        assert_eq!(files_arr.len(), 4);

        let hook_entry = files_arr.iter().find(|f| f["path"] == "src/hooks/useSuppliers.tsx").unwrap();
        assert_eq!(hook_entry["role"], serde_json::json!("client-hook"));
        assert!(hook_entry["capabilities"]["states"].get("suppliers").is_some());
        assert!(hook_entry["consumedContracts"]["apiCalls"].as_array().unwrap().iter().any(|v| v == "/api/suppliers"));

        let nav_entry = files_arr.iter().find(|f| f["path"] == "src/components/Navbar.tsx").unwrap();
        let nav_routes = nav_entry["connectedRoutes"].as_array().unwrap();
        assert!(nav_routes.iter().any(|v| v == "/suppliers"));
        assert!(nav_routes.iter().any(|v| v == "/dashboard"));
        assert!(nav_entry["capabilities"]["exports"].as_array().unwrap().iter().any(|v| v == "Navbar"));

        // Prompt injection skips configuration and current file
        let prompt_ctx = format_axiom_contracts_prompt(&temp_dir, "src/app/dashboard/page.tsx");
        assert!(prompt_ctx.contains("CROSS-FILE CONTRACTS REGISTRY"));
        assert!(prompt_ctx.contains("src/hooks/useSuppliers.tsx"));
        assert!(prompt_ctx.contains("/api/suppliers"));

        // Refresh picks up files added later by Import Fixer
        std::fs::write(
            temp_dir.join("src/components/MissingWidget.tsx"),
            "export const MissingWidget = () => <div />;\nexport default MissingWidget;\n",
        ).unwrap();
        refresh_axiom_contracts_for_project(&temp_dir, "proj-1", &files_list);
        let refreshed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(temp_dir.join(".axiom").join("axiom-contracts.json")).unwrap()
        ).unwrap();
        assert_eq!(refreshed["totalFilesTracked"], serde_json::json!(5));
        assert!(refreshed["files"].as_array().unwrap().iter().any(|f| f["path"] == "src/components/MissingWidget.tsx"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_hook_use_callback_client_directive_and_ui_metacognitive_prompt() {
        let raw_hook = r#"import { useRouter, useState, useEffect } from 'react';

export function useSuppliers() {
  const router = useRouter();
  const [suppliers, setSuppliers] = useState([]);

  const fetchSuppliers = async () => {
    const res = await fetch('/api/suppliers');
    if (res.ok) setSuppliers(await res.json());
  };

  useEffect(() => {
    fetchSuppliers();
  }, [fetchSuppliers, router]);

  return { suppliers, fetchSuppliers };
}
"#;
        let processed = post_process_generated_file("src/hooks/useSuppliers.tsx", raw_hook, &serde_json::json!({}), false);
        assert!(processed.starts_with("\"use client\";"), "Hook must get 'use client' directive:\n{}", processed);
        assert!(processed.contains("useCallback"), "Action must be memoized with useCallback:\n{}", processed);
        assert!(processed.contains("import { useRouter } from 'next/navigation';"), "useRouter must move to next/navigation:\n{}", processed);
        // Must actually import useCallback alongside useState, useEffect in 'react'
        let react_import_line = processed.lines().find(|l| l.contains("from 'react'")).unwrap();
        assert!(react_import_line.contains("useCallback"), "useCallback MUST be imported from 'react':\n{}", processed);
        assert!(!react_import_line.contains("useRouter"), "useRouter MUST NOT remain in 'react' import:\n{}", processed);

        // Ensure UI generation prompt injects METACOGNITIVE_UI_STATE_RULEPACK for UI files, NOT hooks/API
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_ui_meta_prompt_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let ui_prompt = get_expert_system_prompt("src/components/Navbar.tsx", "nextjs", "", "", "", "", &temp_dir);
        assert!(ui_prompt.contains("METACOGNITIVE UI COMPONENT STATE & INTERACTION PROTOCOL"));
        let hook_prompt = get_expert_system_prompt("src/hooks/useSuppliers.tsx", "nextjs", "", "", "", "", &temp_dir);
        assert!(!hook_prompt.contains("METACOGNITIVE UI COMPONENT STATE & INTERACTION PROTOCOL"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_npm_offline_network_detection_and_scoped_override_safety() {
        assert!(is_npm_network_error("npm error code ENOTFOUND\nnpm error syscall getaddrinfo"));
        assert!(is_npm_network_error("npm error errno EAI_NODATA"));
        assert!(is_npm_network_error("npm error network request failed: fetch failed"));
        assert!(is_npm_network_error("npm error code ENOTCACHED\nnpm error request to https://registry.npmjs.org/next failed, reason: cache mode is 'only-if-cached'"));
        assert!(!is_npm_network_error("npm warn deprecated offline-plugin@5.0.0"));
        assert!(!is_npm_network_error("npm error code ETARGET\nnpm error notarget No matching version found for shell-quote@^1.10.0."));
        assert!(!is_npm_network_error("npm error code E404\nnpm error 404 Not Found - GET https://registry.npmjs.org/nonexistent"));

        // Non-Next.js Webpack/Express project keeps webpack-dev-server
        let mut standalone_webpack = serde_json::json!({
            "name": "legacy-spa",
            "scripts": { "dev": "webpack serve --mode development" },
            "dependencies": { "express": "^4.18.0" },
            "devDependencies": { "webpack": "^5.0.0", "webpack-dev-server": "^4.7.3" }
        });
        assert!(!sanitize_package_json_value_for_offline(&mut standalone_webpack));
        assert!(standalone_webpack["devDependencies"].get("webpack-dev-server").is_some());

        // Scoped transitive dependency override must NEVER delete @auth/* core/direct deps or write global overrides
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_scoped_override_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let pkg_path = temp_dir.join("package.json");
        std::fs::write(
            &pkg_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "auth-app",
                "dependencies": {
                    "next": "14.2.35",
                    "next-auth": "^4.24.7",
                    "@auth/prisma-adapter": "^1.0.12"
                }
            })).unwrap(),
        ).unwrap();

        // Without parent evidence, returns false and adds no global override
        assert!(!set_package_version_to_wildcard(&pkg_path, "@auth/core"));
        // With parent evidence in npm ERESOLVE/stderr log, writes strictly parent-scoped override
        let auth_log = "npm error Could not resolve dependency:\nnpm error peer @auth/core@^0.18.0 from @auth/prisma-adapter@1.0.12";
        assert!(apply_npm_etarget_heal(&pkg_path, "@auth/core", Some(auth_log)));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert!(saved["dependencies"].get("@auth/prisma-adapter").is_some(), "@auth/prisma-adapter must NOT be deleted!");
        assert!(saved["overrides"].get("@auth/core").is_none(), "Global @auth/core override must NEVER be written!");
        assert_eq!(saved["overrides"]["@auth/prisma-adapter"]["@auth/core"], serde_json::json!("*"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_prisma_quoted_parens_utf8_and_inline_comment_unique() {
        // Parentheses inside map: "..." must not truncate @relation(...) parsing
        let rel_line = r#"  assignedTo User? @relation(map: "fk(assigned_1)", fields: [assignedToId], references: [id], onDelete: SET_NULL, name: "AssignedTo")"#;
        assert_eq!(extract_prisma_relation_name(rel_line), Some("AssignedTo".to_string()));
        let norm_line = normalize_prisma_referential_actions(rel_line);
        assert!(norm_line.contains("onDelete: SetNull"), "Must normalize past quoted parens:\n{}", norm_line);

        // Multi-byte UTF-8 inline comment + scalar @relation must not panic or corrupt bytes
        let utf8_scalar = "model Audit {\n  članId String @relation(fields: [x], references: [id]) // čćžšđ komentar\n}\n";
        let fixed_utf8 = post_process_generated_file("prisma/schema.prisma", utf8_scalar, &serde_json::json!({}), false);
        assert!(!fixed_utf8.contains("@relation"));
        assert!(fixed_utf8.contains("// čćžšđ komentar"));

        // 1-to-1 @unique injection and back-relation @relation("...") attachment must preserve trailing inline comments
        let one_to_one_comment = r#"
model User {
  id    String @id @default(cuid())
  tasks Task[] // dodeljeni zadaci
}

model Task {
  id           String  @id @default(cuid())
  assignedToId String?
  assignedTo   User?   @relation(name: "AssignedTo", fields: [assignedToId], references: [id])
}

model Project {
  id        String     @id @default(cuid())
  analytics Analytics? @relation("ProjectAnalytics")
}

model Analytics {
  id        String  @id @default(cuid())
  projectId String  // FK ka projektu
  project   Project @relation("ProjectAnalytics", fields: [projectId], references: [id])
}
"#;
        let fixed_unique = sanitize_prisma_relations(one_to_one_comment);
        assert!(
            fixed_unique.contains("projectId String @unique // FK ka projektu"),
            "@unique must be inserted before inline comment:\n{}",
            fixed_unique
        );
        assert!(
            fixed_unique.contains("tasks Task[] @relation(\"AssignedTo\") // dodeljeni zadaci"),
            "@relation must be inserted before inline comment:\n{}",
            fixed_unique
        );
    }

    #[test]
    fn test_katastar_normalization_and_scrubbing() {
        // 1. Prisma P1012 signature normalizes identifiers and line numbers
        let prisma_err = "Error: Prisma schema validation - (get-dmmf wasm)\nError code: P1012\nerror: Error validating: You defined the enum `Role`. But the current connector does not support enums.\n  -->  prisma\\schema.prisma:10";
        let prisma_sig = crate::katastar::normalize_error_signature("prisma", prisma_err, Some("prisma/schema.prisma"));
        assert_eq!(prisma_sig.category, "prisma");
        assert_eq!(prisma_sig.error_code.as_deref(), Some("P1012"));
        assert!(prisma_sig.error_signature.starts_with("prisma:P1012:"));
        assert!(prisma_sig.normalized_sample.contains("<IDENT>"));

        // 2. npm ETARGET extracts error code + offending package
        let npm_err = "npm error code ETARGET\nnpm error notarget No matching version found for shell-quote@^1.10.0.";
        let npm_sig = crate::katastar::normalize_error_signature("npm", npm_err, Some("package.json"));
        assert_eq!(npm_sig.category, "npm");
        assert_eq!(npm_sig.error_code.as_deref(), Some("ETARGET"));
        assert_eq!(npm_sig.error_signature, "npm:ETARGET:shell-quote");

        // 3. Compile / runtime error normalization
        let ts_local = "src/app/page.tsx(4,20): error TS2307: Cannot find module '@/components/Navbar' or its corresponding type declarations.";
        let ts_sig = crate::katastar::normalize_error_signature("", ts_local, Some("src/app/page.tsx"));
        assert_eq!(ts_sig.error_signature, "compile:TS2307:local-import");

        let client_err = "You're importing a component that needs useState. It only works in a Client Component, mark the file with the 'use client' directive.";
        let client_sig = crate::katastar::normalize_error_signature("", client_err, Some("src/components/Navbar.tsx"));
        assert_eq!(client_sig.error_signature, "compile:NEXT_USE_CLIENT:missing-client-directive");

        let ref_err = "Unhandled Runtime Error\nReferenceError: isMobileMenuOpen is not defined";
        let ref_sig = crate::katastar::normalize_error_signature("", ref_err, Some("src/components/Navbar.tsx"));
        assert_eq!(ref_sig.category, "runtime");
        assert_eq!(ref_sig.error_signature, "runtime:ReferenceError:identifier-not-defined");

        // 4. Sensitive path, ANSI, and credential scrubbing
        let dirty = "\x1b[31mError\x1b[0m in C:\\Users\\Alice\\AppData\\Local\\axiom-forge\\projects\\123e4567-e89b-12d3-a456-426614174000\\src\\app.ts with postgresql://admin:supersecret@db.internal:5432/prod and sk_live_1234567890abcdef Bearer eyJhbGciOiJIUzI1NiJ9.payload";
        let scrubbed = crate::katastar::scrub_sensitive_text(dirty);
        assert!(!scrubbed.contains("Alice"));
        assert!(!scrubbed.contains("supersecret"));
        assert!(!scrubbed.contains("sk_live_"));
        assert!(!scrubbed.contains("eyJhbGciOi"));
        assert!(scrubbed.contains("<USER_DIR>"));
        assert!(scrubbed.contains("postgresql://<REDACTED>"));
        assert!(scrubbed.contains("<SECRET>"));
    }

    #[test]
    fn test_katastar_safe_patch_extraction_and_guards() {
        // Atomic import fix is accepted
        let before_imp = "import { useState } from 'react';\nimport { Home } from 'react-icons';\nexport default function A() { return <div />; }\n";
        let after_imp = "import { useState } from 'react';\nimport { FiHome as Home } from 'react-icons/fi';\nexport default function A() { return <div />; }\n";
        let patch_imp = crate::katastar::extract_safe_atomic_patch("src/components/Header.tsx", before_imp, after_imp);
        assert!(patch_imp.is_some());

        // Atomic next.config fix is accepted
        let before_cfg = "const nextConfig = {\n  webpack: (config) => {\n    config.resolve.fallback = { path: require.resolve('path-browserify') };\n    return config;\n  }\n};\n";
        let after_cfg = "const nextConfig = {\n  webpack: (config) => {\n    config.resolve.fallback = { path: false };\n    return config;\n  }\n};\n";
        assert!(crate::katastar::extract_safe_atomic_patch("next.config.js", before_cfg, after_cfg).is_some());

        // Prisma universal referential fix accepted; provider & model edits rejected
        let before_prisma = "model Item {\n  parent Item? @relation(fields: [p], references: [id], onDelete: SET_NULL)\n}\n";
        let after_prisma = "model Item {\n  parent Item? @relation(fields: [p], references: [id], onDelete: SetNull)\n}\n";
        assert!(crate::katastar::extract_safe_atomic_patch("prisma/schema.prisma", before_prisma, after_prisma).is_some());

        let before_prov = "datasource db {\n  provider = \"sqlite\"\n  url = env(\"DATABASE_URL\")\n}\n";
        let after_prov = "datasource db {\n  provider = \"postgresql\"\n  url = env(\"DATABASE_URL\")\n}\n";
        assert!(crate::katastar::extract_safe_atomic_patch("prisma/schema.prisma", before_prov, after_prov).is_none());

        // JSX component markup edits are NEVER auto-patched globally across projects
        let before_jsx = "export default function Nav() {\n  return <div>{isOpen ? 'Open' : 'Closed'}</div>;\n}\n";
        let after_jsx = "export default function Nav() {\n  return <div><span>Menu</span></div>;\n}\n";
        assert!(crate::katastar::extract_safe_atomic_patch("src/components/Navbar.tsx", before_jsx, after_jsx).is_none());
    }

    #[test]
    fn test_katastar_two_phase_verification_and_disable_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_katastar_lifecycle_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let store_path = temp_dir.join("axiom-error-katastar.json");

        let before_code = "import { Home } from 'react-icons';\nexport default function Hero() { return <main />; }\n";
        let after_code = "import { FiHome as Home } from 'react-icons/fi';\nexport default function Hero() { return <main />; }\n";
        let patch = crate::katastar::extract_safe_atomic_patch("src/components/Hero.tsx", before_code, after_code).unwrap();
        let err_msg = "src/components/Hero.tsx(1,22): error TS2307: Cannot find module 'react-icons' or its corresponding type declarations.";

        // 1. Register as pending -> must NOT surface in verified lookup or blind sanitizer patches yet
        let eid = crate::katastar::register_repair_attempt_at(
            &store_path,
            "compile",
            err_msg,
            "src/components/Hero.tsx",
            "ai",
            "search_replace",
            Some(patch),
            "Pending fix",
            "qwen2.5-coder:7b",
        );
        let pre_lookup = crate::katastar::lookup_katastar_at(&store_path, "compile", err_msg, "src/components/Other.tsx", before_code, &[]);
        assert!(pre_lookup.applicable_patches.is_empty());
        assert!(pre_lookup.hints.is_empty());
        assert!(crate::katastar::get_verified_sanitizer_patches_at(&store_path, "src/components/Other.tsx").is_empty());

        // 2. Finalize passing build test -> promotes to "verified" and becomes usable
        crate::katastar::finalize_katastar_outcome_at(
            &store_path,
            &eid,
            true,
            "ai",
            "next_build",
            None,
            Some("Import react-icons icons from specific subpath like 'react-icons/fi'"),
            "qwen2.5-coder:7b",
        );
        let post_lookup = crate::katastar::lookup_katastar_at(&store_path, "compile", err_msg, "src/components/Other.tsx", before_code, &[]);
        assert_eq!(post_lookup.applicable_patches.len(), 1);
        assert_eq!(post_lookup.applicable_patches[0].entry_id, eid);
        assert_eq!(post_lookup.hints.len(), 1);
        assert_eq!(crate::katastar::get_verified_sanitizer_patches_at(&store_path, "src/components/Other.tsx").len(), 1);

        // 3. Two consecutive failures auto-disable the verified rule
        crate::katastar::finalize_katastar_outcome_at(&store_path, &eid, false, "katastar", "tsc_noemit", None, None, "qwen2.5-coder:7b");
        crate::katastar::finalize_katastar_outcome_at(&store_path, &eid, false, "katastar", "tsc_noemit", None, None, "qwen2.5-coder:7b");
        let disabled_lookup = crate::katastar::lookup_katastar_at(&store_path, "compile", err_msg, "src/components/Other.tsx", before_code, &[]);
        assert!(disabled_lookup.applicable_patches.is_empty());
        assert!(crate::katastar::get_verified_sanitizer_patches_at(&store_path, "src/components/Other.tsx").is_empty());

        let store = crate::katastar::load_katastar_store_at(&store_path);
        assert_eq!(store.entries[0].status, "disabled");
        assert_eq!(store.entries[0].history.len(), 3);

        // 4. Interactive pending repair tracking & reoccurrence detection
        let runtime_err = "ReferenceError: isMobileMenuOpen is not defined";
        let sig_info = crate::katastar::normalize_error_signature("runtime", runtime_err, Some("src/components/Navbar.tsx"));
        let eid2 = crate::katastar::register_repair_attempt_at(
            &store_path,
            &sig_info.category,
            runtime_err,
            "src/components/Navbar.tsx",
            "ai",
            "ai_edit",
            None,
            "Declare useState for mobile menu",
            "qwen2.5-coder:7b",
        );
        crate::katastar::track_pending_project_repair_at(
            &store_path,
            "proj-demo",
            "src/components/Navbar.tsx",
            &sig_info.error_signature,
            &eid2,
            "ai",
        );
        let reoccurred = crate::katastar::resolve_reoccurred_pending_repair_at(
            &store_path,
            "proj-demo",
            "src/components/Navbar.tsx",
            &sig_info.error_signature,
            "qwen2.5-coder:7b",
        );
        assert_eq!(reoccurred, vec![eid2.clone()]);
        let store_after = crate::katastar::load_katastar_store_at(&store_path);
        assert!(store_after.pending_repairs.is_empty());
        let failed_entry = store_after.entries.iter().find(|e| e.id == eid2).unwrap();
        assert_eq!(failed_entry.status, "failed");
        assert_eq!(failed_entry.fail_count, 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_extract_offending_parent_from_debug_log_and_core_lock() {
        // Core framework package lock verification
        for core_pkg in &[
            "next",
            "react",
            "react-dom",
            "prisma",
            "@prisma/client",
            "next-auth",
            "@auth/prisma-adapter",
            "typescript",
            "tailwindcss",
        ] {
            assert!(is_core_framework_package(core_pkg), "{} must be core-locked", core_pkg);
        }
        assert!(!is_core_framework_package("lucide-react"));
        assert!(!is_core_framework_package("shell-quote"));
        assert!(!is_core_framework_package("recharts"));

        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_parent_log_test_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let pkg_path = temp_dir.join("package.json");
        std::fs::write(
            &pkg_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "decentral-grid",
                "dependencies": {
                    "next": "14.2.35",
                    "react": "^18.2.0",
                    "@prisma/client": "5.22.0",
                    "next-auth": "^4.24.7"
                },
                "devDependencies": {
                    "prisma": "5.22.0",
                    "some-legacy-tool": "^2.1.0"
                }
            })).unwrap(),
        ).unwrap();

        // Core framework packages cannot be modified or removed via self-healing
        for protected in &["next", "react", "@prisma/client", "prisma", "next-auth"] {
            assert!(!apply_npm_etarget_heal(&pkg_path, protected, None));
            assert!(!remove_package_from_json(&pkg_path, protected));
        }

        // Mock npm _logs/debug-0.log referenced by stderr
        let debug_log_path = temp_dir.join("2026-11-10T07_12_26_520Z-debug-0.log");
        std::fs::write(
            &debug_log_path,
            "0 verbose cli node npm\n12 silly placeDep ROOT shell-quote@^1.10.0 KEEP for: some-legacy-tool@2.1.0 want: ^1.10.0\n13 error notarget No matching version found for shell-quote@^1.10.0.\n",
        ).unwrap();

        let stderr_only = format!(
            "npm error code ETARGET\nnpm error While resolving: decentral-grid@0.1.0\nnpm error notarget No matching version found for shell-quote@^1.10.0.\nnpm error A complete log of this run can be found in: {}",
            debug_log_path.display()
        );
        let direct = vec!["next".to_string(), "some-legacy-tool".to_string()];
        let extracted = extract_offending_parent_from_npm_log(
            &stderr_only,
            "shell-quote",
            &direct,
            Some("decentral-grid"),
        );
        assert_eq!(extracted.as_deref(), Some("some-legacy-tool"));

        assert!(apply_npm_etarget_heal(&pkg_path, "shell-quote", Some(&stderr_only)));
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pkg_path).unwrap()).unwrap();
        assert_eq!(saved["overrides"]["some-legacy-tool"]["shell-quote"], serde_json::json!("*"));
        assert!(saved["overrides"].get("decentral-grid").is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_local_import_guard_and_package_repair_v47() {
        // 1. Bare local specifiers must never be mistaken for npm packages
        assert!(crate::packages::is_local_import_specifier("src/models/MaintenanceAlert"));
        assert!(crate::packages::is_local_import_specifier("app/dashboard/page"));
        assert!(crate::packages::is_local_import_specifier("components/Navbar"));
        assert!(crate::packages::is_local_import_specifier("@/hooks/useMaintenanceAlerts"));
        assert!(crate::packages::is_local_import_specifier("~/lib/prisma"));
        assert!(!crate::packages::is_local_import_specifier("lucide-react"));
        assert!(!crate::packages::is_local_import_specifier("next-auth/providers/credentials"));

        // 2. Official next-auth v4 subpaths must not be mangled even against incomplete cached DBs
        let mock_db = vec![PackageMeta {
            name: "next-auth".to_string(),
            version: "^4.24.7".to_string(),
            aliases: vec![],
            import_examples: vec!["import NextAuth from 'next-auth';".to_string()],
            compatibility: serde_json::Map::new(),
            best_practices: String::new(),
        }];
        assert_eq!(resolve_and_rewrite_import("next-auth/providers/credentials", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/react", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/next", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("next-auth/middleware", &mock_db), None);
        assert_eq!(resolve_and_rewrite_import("src/models/MaintenanceAlert", &mock_db), None);

        // 3. Server log package extractor must ignore local paths & builtins, and normalize scoped/subpath packages
        assert_eq!(
            crate::server::extract_package_name_from_error("Module not found: Can't resolve 'src/models/MaintenanceAlert'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Cannot find module '@/hooks/useMaintenanceAlerts'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve '@components/Button'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Cannot find module 'node:fs'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Cannot find module 'fs'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve 'next/navigation'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve 'react'"),
            None
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve 'next-auth/providers/credentials'"),
            Some("next-auth".to_string())
        );
        assert_eq!(
            crate::server::extract_package_name_from_error("Can't resolve '@mui/material/Button'"),
            Some("@mui/material".to_string())
        );

        // 4. repair_package_json_dependencies rewrites src/... and ~/... imports to @/... and scrubs bogus local deps
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_pkg_repair_v47_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::write(temp_dir.join("src/app/page.tsx"), "export default function Home() { return null; }").unwrap();
        std::fs::write(
            temp_dir.join("src/hooks/useMaintenanceAlerts.ts"),
            "import { MaintenanceAlert } from 'src/models/MaintenanceAlert';\nimport { cn } from '~/lib/utils';\nimport { z } from 'zod';\nimport { Wrench } from 'lucide-react';\nexport { MaintenanceAlert, cn, z, Wrench };\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("package.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "name": "shoecraft-test",
                "dependencies": {
                    "next": "14.2.35",
                    "react": "^18.2.0",
                    "src": "*",
                    "models": "*"
                },
                "devDependencies": {}
            }))
            .unwrap(),
        )
        .unwrap();

        crate::packages::repair_package_json_dependencies(&temp_dir);

        let updated_hook = std::fs::read_to_string(temp_dir.join("src/hooks/useMaintenanceAlerts.ts")).unwrap();
        assert!(updated_hook.contains("from '@/models/MaintenanceAlert'"));
        assert!(updated_hook.contains("from '@/lib/utils'"));
        assert!(!updated_hook.contains("'src/models/MaintenanceAlert'"));

        let updated_pkg: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("package.json")).unwrap()).unwrap();
        let deps = updated_pkg["dependencies"].as_object().unwrap();
        assert!(!deps.contains_key("src"));
        assert!(!deps.contains_key("models"));
        assert!(!deps.contains_key("app"));
        assert!(deps.contains_key("zod"));
        assert!(deps.contains_key("lucide-react"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_katastar_prisma_preamble_local_import_and_npm_errors_v47() {
        // 1. Prisma CLI preambles must be stripped so real P1012 error produces clean signature
        let prisma_raw = "Environment variables loaded from .env\nPrisma schema loaded from prisma\\schema.prisma\nDatasource \"db\": PostgreSQL\nError code: P1012\nerror: Error parsing attribute \"@relation\": Invalid referential action: `SET_NULL`";
        let prisma_sig = crate::katastar::normalize_error_signature("prisma", prisma_raw, Some("prisma/schema.prisma"));
        assert_eq!(prisma_sig.category, "prisma");
        assert_eq!(prisma_sig.error_code.as_deref(), Some("P1012"));
        assert!(prisma_sig.error_signature.starts_with("prisma:P1012:"));
        assert!(!prisma_sig.normalized_sample.contains("Environment variables loaded"));
        assert!(!prisma_sig.normalized_sample.contains("Prisma schema loaded"));

        // 2. Local import specifiers produce canonical :local-import signature
        let ts_local = crate::katastar::normalize_error_signature(
            "",
            "TS2307: Cannot find module 'src/models/MaintenanceAlert' or its corresponding type declarations.",
            Some("src/hooks/useMaintenanceAlerts.ts"),
        );
        assert_eq!(ts_local.category, "compile");
        assert_eq!(ts_local.error_code.as_deref(), Some("TS2307"));
        assert_eq!(ts_local.error_signature, "compile:TS2307:local-import");

        let wp_local = crate::katastar::normalize_error_signature(
            "",
            "Module not found: Can't resolve 'app/components/Missing'",
            Some("src/app/page.tsx"),
        );
        assert_eq!(wp_local.error_signature, "compile:MODULE_NOT_FOUND:local-import");

        // 3. npm ENOENT / ENOTFOUND / missing package normalization
        let enoent_sig = crate::katastar::normalize_error_signature(
            "npm",
            "npm error code ENOENT\nmissing package 'foo'",
            None,
        );
        assert_eq!(enoent_sig.error_signature, "npm:ENOENT:foo");

        let net_sig = crate::katastar::normalize_error_signature(
            "npm",
            "npm error code ENOTFOUND\nnpm error request to https://registry.npmjs.org/zod failed",
            None,
        );
        assert_eq!(net_sig.error_signature, "npm:ENOTFOUND:zod");
    }

    #[test]
    fn test_sanitizer_tsconfig_client_prisma_css_exports_and_ui_v47() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_san_v47_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/app/dashboard")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();

        // 1. ensure_valid_tsconfig adds baseUrl and @/* -> ["./src/*"]
        std::fs::write(
            temp_dir.join("tsconfig.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "compilerOptions": { "strict": true }
            }))
            .unwrap(),
        )
        .unwrap();
        crate::sanitizer::ensure_valid_tsconfig(&temp_dir);
        let ts_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("tsconfig.json")).unwrap()).unwrap();
        assert_eq!(ts_val["compilerOptions"]["baseUrl"], ".");
        assert_eq!(ts_val["compilerOptions"]["paths"]["@/*"], serde_json::json!(["./src/*"]));

        // 2. has_named_export_symbol respects word boundaries; dual-export adds missing `export { Card }`
        assert!(!crate::sanitizer::has_named_export_symbol("export const CardHeader = () => null;", "Card"));
        assert!(crate::sanitizer::has_named_export_symbol("export const CardHeader = () => null;", "CardHeader"));
        let card_src = "export const CardHeader = () => null;\nconst Card = () => null;\nexport default Card;";
        let card_post = post_process_generated_file("src/components/Card.tsx", card_src, &serde_json::json!({}), false);
        assert!(card_post.contains("export { Card };"));

        // 3. deduplicate_repeated_css_blocks collapses repeated CSS rule loops
        let mut looped_css = String::from("body { margin: 0; }\n");
        for _ in 0..8 {
            looped_css.push_str(".table-responsive { display: block; width: 100%; overflow-x: auto; }\n");
        }
        let deduped_css = crate::sanitizer::deduplicate_repeated_css_blocks(&looped_css);
        assert_eq!(deduped_css.matches(".table-responsive").count(), 1);
        assert_eq!(deduped_css.matches("body { margin: 0; }").count(), 1);

        // 4. ensure_next_server_imports fixes default NextResponse import and injects missing imports
        let bad_mw = "import NextResponse from 'next/server';\nexport function middleware(req: NextRequest) { return NextResponse.next(); }\n";
        let fixed_mw = crate::sanitizer::ensure_next_server_imports(bad_mw, "middleware.ts");
        assert!(fixed_mw.contains("NextResponse"));
        assert!(fixed_mw.contains("NextRequest"));
        assert!(!fixed_mw.contains("import NextResponse from"));
        assert!(fixed_mw.contains("{ NextResponse") && fixed_mw.contains("NextRequest }"));

        // 5. Client hook preserves `import type` from @prisma/client while stripping value @prisma/client & ../lib/prisma
        let hook_src = r#"import { useState } from 'react';
import { prisma } from '../lib/prisma';
import { PrismaClient } from '@prisma/client';
import type { MaintenanceAlert } from '@prisma/client';

export function useMaintenanceAlerts() {
  const [alerts, setAlerts] = useState<MaintenanceAlert[]>([]);
  return { alerts, setAlerts };
}
"#;
        let processed_hook = post_process_generated_file("src/hooks/useMaintenanceAlerts.ts", hook_src, &serde_json::json!({}), false);
        assert!(processed_hook.trim_start().starts_with("\"use client\";"));
        assert!(!processed_hook.contains("../lib/prisma"));
        assert!(!processed_hook.contains("@/lib/prisma"));
        assert!(!processed_hook.contains("PrismaClient"));
        assert!(processed_hook.contains("import type { MaintenanceAlert } from '@prisma/client';"));

        // 6. sanitize_root_layout_direct_use_session neutralizes direct calls & prunes unused useSession import
        let root_with_session = "import { useSession, signOut } from 'next-auth/react';\nexport default function RootLayout({ children }: any) {\n  const { data: sess, status } = useSession();\n  return <html><body data-status={status}>{children}</body></html>;\n}\n";
        let neutralized = crate::sanitizer::sanitize_root_layout_direct_use_session(root_with_session);
        assert!(!neutralized.contains("useSession("));
        assert!(neutralized.contains("const sess: any = null;"));
        assert!(neutralized.contains("const status = 'unauthenticated';"));
        assert!(!neutralized.contains("useSession,"));
        assert!(neutralized.contains("{ signOut }"));

        // 7. ensure_ui_primitives_reconciled relaxes strict unions & appends missing Card / MetricCard / Option
        std::fs::write(
            temp_dir.join("src/components/ui.tsx"),
            "import React from 'react';\nexport interface ButtonProps {\n  variant?: 'primary' | 'secondary';\n  size?: 'sm' | 'md';\n}\nexport const CardHeader = ({ children }: any) => <div>{children}</div>;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/app/dashboard/page.tsx"),
            "import { Card, CardHeader, MetricCard, Option } from '@/components/ui';\nexport default function Dashboard() { return <Card><CardHeader>Hi</CardHeader><MetricCard value={1} /><select><Option value='1'>One</Option></select></Card>; }\n",
        )
        .unwrap();

        crate::sanitizer::ensure_ui_primitives_reconciled(&temp_dir);
        let ui_after = std::fs::read_to_string(temp_dir.join("src/components/ui.tsx")).unwrap();
        assert!(ui_after.contains("| (string & {})"));
        assert!(ui_after.contains("export const Card ="));
        assert!(ui_after.contains("export const MetricCard ="));
        assert!(ui_after.contains("export const Option ="));
        assert_eq!(ui_after.matches("export const CardHeader").count(), 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_healer_guards_import_rewrite_and_prose_rejection_v47() {
        // 1. Path validation rejects extensionless module specifiers, absolute paths, and spaces
        assert!(!is_valid_file_path("src/models/MaintenanceAlert"));
        assert!(!is_valid_file_path("/abs/path/file.ts"));
        assert!(!is_valid_file_path("src/components/My Card.tsx"));
        assert!(is_valid_file_path("src/models/MaintenanceAlert.ts"));
        assert!(is_valid_file_path(".env.local"));

        // 2. Prose & JSON payload rejection
        assert!(!looks_like_code_payload("The error indicates that the module 'src/models/MaintenanceAlert' cannot be found."));
        assert!(!looks_like_code_payload("1. Ensure the file exists at src/models/MaintenanceAlert.ts\n2. Verify the import path in tsconfig.json"));
        assert!(!looks_like_code_payload("{\"edits\":[]}"));
        assert!(looks_like_code_payload("import type { MaintenanceAlert } from '@/models/MaintenanceAlert';\nexport const x = 1;"));
        assert!(looks_like_code_payload(".btn { color: red; }"));
        assert!(looks_like_code_payload("model Alert {\n  id String @id\n}"));

        // 3. `type: "import"` swap edits must target the consumer file and rewrite `'src/...'` -> `'@/...'`, never creating an extensionless file
        let import_swap_json = serde_json::json!({
            "response": "1. Ensure src/models/MaintenanceAlert.ts exists\n2. Update import",
            "steps": "Check tsconfig.json",
            "edits": [
                {
                    "type": "import",
                    "path": "src/models/MaintenanceAlert",
                    "newPath": "src/models/MaintenanceAlert.ts"
                }
            ]
        });
        let extracted = extract_edits_from_json(&import_swap_json, "src/hooks/useMaintenanceAlerts.ts");
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0]["path"], "src/hooks/useMaintenanceAlerts.ts");
        assert_eq!(extracted[0]["target"], "'src/models/MaintenanceAlert'");
        assert_eq!(extracted[0]["replacement"], "'@/models/MaintenanceAlert'");
    }

    #[test]
    fn test_orchestrator_validator_contracts_and_self_healing_v47() {
        // 1. Deterministic validate_file_with_abstract_rules checks
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let clean_model = "export interface MaintenanceAlert {\n  id: string;\n  severity: 'LOW' | 'HIGH';\n}\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/models/MaintenanceAlert.ts", clean_model, "qwen").await.is_ok());

            let bad_model = "import { PrismaClient } from '@prisma/client';\nexport const db = new PrismaClient();\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/models/MaintenanceAlert.ts", bad_model, "qwen").await.is_err());

            let bad_mw = "import { NextFunction } from 'express';\nexport function middleware() {}\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("middleware.ts", bad_mw, "qwen").await.is_err());

            let bad_client = "\"use client\";\nimport prisma from '@/lib/prisma';\nexport function Panel() { return <div />; }\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/components/Panel.tsx", bad_client, "qwen").await.is_err());

            let prose_leak = "export const a = 1;\nThe error indicates that something is wrong.\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/utils/a.ts", prose_leak, "qwen").await.is_err());

            let mut repeating_css = String::new();
            for _ in 0..12 {
                repeating_css.push_str(".loop-card { padding: 12px; border: 1px solid #ccc; }\n");
            }
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/app/globals.css", &repeating_css, "qwen").await.is_err());

            let unclosed = "export function broken() {\n  if (true) {\n    const obj = {\n      a: 1\n";
            assert!(crate::orchestrator::validate_file_with_abstract_rules("src/utils/broken.ts", unclosed, "qwen").await.is_err());
        });

        // 2. Contract harvester: multiline useState, hookCalls, src/ internal deps, and CSS inclusion on refresh
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_orch_sh_v47_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("src/models")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/hooks")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/utils")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/components")).unwrap();
        std::fs::create_dir_all(temp_dir.join("src/app")).unwrap();

        let hook_code = r#""use client";
import { useState, useCallback } from 'react';
import type { MaintenanceAlert } from 'src/models/MaintenanceAlert';
import { useAuth } from '@/hooks/useAuth';

export function useMaintenanceAlerts() {
  const [alerts, setAlerts] = useState<MaintenanceAlert[]>(
    []
  );
  const { user } = useAuth();
  const reload = useCallback(() => {
    fetch('/api/maintenance');
  }, []);
  return { alerts, setAlerts, user, reload };
}
"#;
        std::fs::write(temp_dir.join("src/models/MaintenanceAlert.ts"), "export interface MaintenanceAlert { id: string; }\n").unwrap();
        std::fs::write(temp_dir.join("src/hooks/useMaintenanceAlerts.ts"), hook_code).unwrap();
        std::fs::write(temp_dir.join("src/app/globals.css"), "@tailwind base;\nbody { color: #111; }\n").unwrap();

        crate::orchestrator::harvest_file_to_axiom_contracts(
            &temp_dir,
            "proj-v47",
            "src/hooks/useMaintenanceAlerts.ts",
            hook_code,
            &[serde_json::json!({ "path": "src/models/MaintenanceAlert.ts" })],
        );
        crate::orchestrator::refresh_axiom_contracts_for_project(&temp_dir, "proj-v47", &[]);

        let contracts_val: Value = serde_json::from_str(
            &std::fs::read_to_string(temp_dir.join(".axiom/axiom-contracts.json")).unwrap(),
        )
        .unwrap();
        let files_arr = contracts_val["files"].as_array().unwrap();
        let hook_entry = files_arr.iter().find(|f| f["path"] == "src/hooks/useMaintenanceAlerts.ts").unwrap();
        assert_eq!(hook_entry["capabilities"]["states"]["alerts"]["type"], "MaintenanceAlert[]");
        assert!(hook_entry["consumedContracts"]["hookCalls"].as_array().unwrap().iter().any(|h| h == "useAuth"));
        assert!(hook_entry["dependencies"]["internal"].as_array().unwrap().iter().any(|d| d == "src/models/MaintenanceAlert.ts"));
        assert!(files_arr.iter().any(|f| f["path"] == "src/app/globals.css"));

        // 3. scan_and_fix_imports must NOT self-import `src/utils/iot.ts` -> `@/utils/iot`, while fixing unique non-self matches
        std::fs::write(
            temp_dir.join("src/utils/iot.ts"),
            "import { missing } from './nonexistent/iot';\nexport const ok = true;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/components/ProductCard.tsx"),
            "export const ProductCard = () => null;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("src/app/page.tsx"),
            "import { ProductCard } from './wrong/ProductCard';\nexport default function Home() { return <ProductCard />; }\n",
        )
        .unwrap();

        let mut mod_index = std::collections::HashMap::new();
        let src_root = temp_dir.join("src");
        crate::self_healing::index_modules(&src_root, &src_root, &mut mod_index);
        crate::self_healing::scan_and_fix_imports(&src_root, &src_root, &mod_index);

        let iot_after = std::fs::read_to_string(temp_dir.join("src/utils/iot.ts")).unwrap();
        assert!(!iot_after.contains("@/utils/iot"));
        let page_after = std::fs::read_to_string(temp_dir.join("src/app/page.tsx")).unwrap();
        assert!(page_after.contains("@/components/ProductCard"));

        // 4. parse_tsc_diagnostics & file_dependency_layer_priority
        let tsc_out = "src\\hooks\\useMaintenanceAlerts.ts(4,5): error TS2307: Cannot find module 'src/models/MaintenanceAlert'.\r\nsrc/app/page.tsx(10,12): error TS2322: Type 'string' is not assignable.";
        let diags = crate::self_healing::parse_tsc_diagnostics(tsc_out);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].file, "src/hooks/useMaintenanceAlerts.ts");
        assert_eq!(diags[0].line, 4);
        assert_eq!(diags[0].col, 5);
        assert_eq!(diags[0].code, "TS2307");

        assert!(crate::self_healing::file_dependency_layer_priority("prisma/schema.prisma")
            < crate::self_healing::file_dependency_layer_priority("src/models/Alert.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/models/Alert.ts")
            < crate::self_healing::file_dependency_layer_priority("src/lib/prisma.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/lib/prisma.ts")
            < crate::self_healing::file_dependency_layer_priority("src/components/ui.tsx"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/components/ui.tsx")
            < crate::self_healing::file_dependency_layer_priority("src/hooks/useAlerts.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/hooks/useAlerts.ts")
            < crate::self_healing::file_dependency_layer_priority("src/app/api/alerts/route.ts"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/app/api/alerts/route.ts")
            < crate::self_healing::file_dependency_layer_priority("src/components/Navbar.tsx"));
        assert!(crate::self_healing::file_dependency_layer_priority("src/components/Navbar.tsx")
            < crate::self_healing::file_dependency_layer_priority("src/app/page.tsx"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_destructive_code_drop_guard_blocks_wiped_ui() {
        let mut full_tsx = String::from("\"use client\";\nimport React from 'react';\nexport default function DashboardPage() {\n  const title = 'Overview';\n  return (\n    <div className=\"p-6 space-y-4\">\n      <h1>{title}</h1>\n");
        for idx in 0..18 {
            full_tsx.push_str(&format!("      <div className=\"card-{}\"><span className=\"label\">Row {}</span></div>\n", idx, idx));
        }
        full_tsx.push_str("    </div>\n  );\n}\n");

        // 1. Replacing the full component with a minimal stub must be blocked
        let wiped_tsx = "\"use client\";\nexport default function DashboardPage() {\n  return <div className=\"p-4\">Fixed</div>;\n}\n";
        assert!(crate::healer::detect_destructive_code_drop("src/app/dashboard/page.tsx", &full_tsx, wiped_tsx).is_some());

        // 2. Keeping >20 lines of boilerplate while stripping >66% of JSX tags must also be blocked
        let mut stripped_jsx = String::from("\"use client\";\nimport React from 'react';\n");
        for idx in 0..20 {
            stripped_jsx.push_str(&format!("type UnusedHelper{} = string;\n", idx));
        }
        stripped_jsx.push_str("export default function DashboardPage() {\n  return <div>Stub</div>;\n}\n");
        assert!(crate::healer::detect_destructive_code_drop("src/app/dashboard/page.tsx", &full_tsx, &stripped_jsx).is_some());

        // 3. Surgical single-line fix must be allowed
        let surgical_tsx = full_tsx.replace("const title = 'Overview';", "const title: string = 'Overview';");
        assert!(crate::healer::detect_destructive_code_drop("src/app/dashboard/page.tsx", &full_tsx, &surgical_tsx).is_none());

        // 4. Creating a brand-new file (empty before) or editing non-JS files is allowed
        assert!(crate::healer::detect_destructive_code_drop("src/models/NewModel.ts", "", "export interface NewModel { id: string; }\n").is_none());
        assert!(crate::healer::detect_destructive_code_drop("src/app/globals.css", &full_tsx, "body { margin: 0; }").is_none());
    }

    #[test]
    fn test_stack_rulepacks_narrowing_no_false_mern_or_config() {
        let model_rules = get_stack_rulepacks(
            "src/models/MaintenanceAlert.ts",
            "export interface MaintenanceAlert {\n  id: string;\n  severity: string;\n}\n",
            None,
        );
        assert!(!model_rules.contains("MERN STACK"));
        assert!(!model_rules.contains("CONFIGURATION & TSCONFIG RULES"));

        let root_model_rules = get_stack_rulepacks(
            "models/Invoice.ts",
            "export interface Invoice { total: number; }",
            None,
        );
        assert!(!root_model_rules.contains("MERN STACK"));

        let config_modal_rules = get_stack_rulepacks(
            "src/components/ConfigModal.tsx",
            "import React from 'react'; export function ConfigModal() { return <div />; }",
            None,
        );
        assert!(!config_modal_rules.contains("CONFIGURATION & TSCONFIG RULES"));

        let next_cfg_rules = get_stack_rulepacks("next.config.js", "module.exports = {};", None);
        assert!(next_cfg_rules.contains("CONFIGURATION & TSCONFIG RULES"));
    }

    #[test]
    fn test_healer_import_rewrite_respects_explicit_path_with_from_to() {
        let cross_file_import_fix = serde_json::json!({
            "edits": [
                {
                    "type": "import",
                    "path": "src/components/Chart.tsx",
                    "from": "react-icons",
                    "to": "react-icons/hi"
                }
            ]
        });
        let edits = extract_edits_from_json(&cross_file_import_fix, "src/app/page.tsx");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["path"], "src/components/Chart.tsx");
        assert_eq!(edits[0]["target"], "'react-icons'");
        assert_eq!(edits[0]["replacement"], "'react-icons/hi'");
    }

    #[test]
    fn test_relevant_packages_db_summary_scoping() {
        let make_pkg = |name: &str, ex: &str| crate::packages::PackageMeta {
            name: name.to_string(),
            version: "^1.0.0".to_string(),
            aliases: vec![],
            import_examples: vec![ex.to_string()],
            compatibility: serde_json::Map::new(),
            best_practices: format!("Use {} safely", name),
        };
        let db = vec![
            make_pkg("next-auth", "import { getServerSession } from 'next-auth';"),
            make_pkg("@prisma/client", "import { PrismaClient } from '@prisma/client';"),
            make_pkg("zod", "import { z } from 'zod';"),
            make_pkg("lucide-react", "import { Home } from 'lucide-react';"),
            make_pkg("react-icons", "import { HiHome } from 'react-icons/hi';"),
            make_pkg("stripe", "import Stripe from 'stripe';"),
            make_pkg("ioredis", "import Redis from 'ioredis';"),
            make_pkg("bullmq", "import { Queue } from 'bullmq';"),
            make_pkg("phaser", "import Phaser from 'phaser';"),
        ];
        let pkg_deps = vec!["next".to_string(), "react".to_string(), "stripe".to_string()];
        let ctx = "Error in src/app/api/auth/route.ts: Cannot find module 'zod' when using getServerSession";
        let summary = crate::packages::format_relevant_packages_db_summary(&db, &pkg_deps, ctx);

        assert!(summary.contains("zod"));
        assert!(summary.contains("next-auth"));
        assert!(summary.contains("stripe"));
        assert!(!summary.contains("phaser"));
        assert!(!summary.contains("ioredis"));
        assert!(!summary.contains("bullmq"));
    }

    #[test]
    fn test_parse_batch_file_fixes_response_variants() {
        let obj_envelope = r#"{"files": [{"path": "./src/a.ts", "content": "export const a = 1;"}]}"#;
        let res1 = crate::self_healing::parse_batch_file_fixes_response(obj_envelope, &["src/a.ts".to_string()]);
        assert_eq!(res1, vec![("src/a.ts".to_string(), "export const a = 1;".to_string())]);

        let arr_envelope = r#"[{"path": "src/b.tsx", "content": "export const B = () => <div />;"}]"#;
        let res2 = crate::self_healing::parse_batch_file_fixes_response(arr_envelope, &[]);
        assert_eq!(res2.len(), 1);
        assert_eq!(res2[0].0, "src/b.tsx");

        let edits_envelope = r#"{"edits": [{"file": "src/c.ts", "code": "export type C = string;"}]}"#;
        let res3 = crate::self_healing::parse_batch_file_fixes_response(edits_envelope, &[]);
        assert_eq!(res3, vec![("src/c.ts".to_string(), "export type C = string;".to_string())]);

        let map_envelope = r#"{"src/d.ts": "export const d = 4;"}"#;
        let res4 = crate::self_healing::parse_batch_file_fixes_response(map_envelope, &[]);
        assert_eq!(res4, vec![("src/d.ts".to_string(), "export const d = 4;".to_string())]);

        let raw_single = "export const e = 5;\n";
        let res5 = crate::self_healing::parse_batch_file_fixes_response(raw_single, &["src/e.ts".to_string()]);
        assert_eq!(res5.len(), 1);
        assert_eq!(res5[0].0, "src/e.ts");

        // Multi-file batch must NOT assign single raw blob blindly to multiple expected files
        let res6 = crate::self_healing::parse_batch_file_fixes_response(
            raw_single,
            &["src/e.ts".to_string(), "src/f.ts".to_string()],
        );
        assert!(res6.is_empty());
    }

    #[test]
    fn test_root_app_layout_hygiene_and_tsconfig() {
        let temp_dir = std::env::temp_dir().join(format!(
            "axiom_root_app_hygiene_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(temp_dir.join("app")).unwrap();
        std::fs::create_dir_all(temp_dir.join("components")).unwrap();
        std::fs::create_dir_all(temp_dir.join("lib")).unwrap();

        std::fs::write(
            temp_dir.join("tsconfig.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "compilerOptions": {
                    "baseUrl": ".",
                    "paths": { "@/*": ["./src/*"] }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("app/page.tsx"),
            "import { Card } from '~/components/ui';\nimport { cn } from '~/lib/utils';\nexport default function Page() { return <Card className={cn('p-4')}>Hi</Card>; }\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("app/globals.css"),
            "body { margin: 0; }\n.dup { color: red; }\n.dup { color: red; }\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("components/ui.tsx"),
            "import React from 'react';\nexport const Badge = ({ children }: any) => <span>{children}</span>;\n",
        )
        .unwrap();
        std::fs::write(
            temp_dir.join("lib/utils.ts"),
            "export const cn = (...a: string[]) => a.join(' ');\n",
        )
        .unwrap();

        crate::self_healing::run_hygiene_pass_on_project(&temp_dir);

        let ts_val: Value = serde_json::from_str(&std::fs::read_to_string(temp_dir.join("tsconfig.json")).unwrap()).unwrap();
        assert_eq!(ts_val["compilerOptions"]["paths"]["@/*"], serde_json::json!(["./*"]));

        let page_after = std::fs::read_to_string(temp_dir.join("app/page.tsx")).unwrap();
        assert!(page_after.contains("from '@/components/ui'"));
        assert!(page_after.contains("from '@/lib/utils'"));

        let css_after = std::fs::read_to_string(temp_dir.join("app/globals.css")).unwrap();
        assert_eq!(css_after.matches(".dup { color: red; }").count(), 1);

        let ui_after = std::fs::read_to_string(temp_dir.join("components/ui.tsx")).unwrap();
        assert!(ui_after.contains("export const Card ="));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

}
