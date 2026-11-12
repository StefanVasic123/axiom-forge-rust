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
            katastar::get_model_progress_telemetry,
            katastar::export_mini_llm_training_dataset,
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
mod tests;
