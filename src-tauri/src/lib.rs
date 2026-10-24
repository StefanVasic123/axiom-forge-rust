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
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path) {
            let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", time, message);
        }
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
            task_start_generation,
            ollama_unload_model,
            get_pending_deep_link,
            clear_pending_deep_link,
            debug_log_to_file,
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
        
        let mui_content = "import { styled } from '@mui/material/styles';\nconst MyDiv = styled('div')({});";
        let processed_mui = post_process_generated_file("src/app/layout.tsx", mui_content, &metamanifest, false);
        assert!(processed_mui.starts_with("\"use client\";"));
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
        
        // Test relative resolve
        let resolved_rel = resolve_import_path(&temp_dir, "../components/ProductCard", "src/app/pages/page.tsx");
        assert_eq!(resolved_rel, Some("src/app/pages/../components/ProductCard.tsx".to_string()));
        
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

}
