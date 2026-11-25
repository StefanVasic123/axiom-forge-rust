use std::env;
use std::fs;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    env::set_var("AXIOM_E2E_HEADLESS", "1");

    let args: Vec<String> = env::args().collect();
    let mut mode = "single".to_string();
    let mut project_id = String::new();
    let mut rel_file = String::new();
    let mut prompt = String::new();
    let mut model_id = "qwen2.5-coder:7b".to_string();
    let mut visual_context_file: Option<String> = None;
    let mut apply = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" if i + 1 < args.len() => {
                mode = args[i + 1].clone();
                i += 2;
            }
            "--project-id" if i + 1 < args.len() => {
                project_id = args[i + 1].clone();
                i += 2;
            }
            "--file" if i + 1 < args.len() => {
                rel_file = args[i + 1].clone();
                i += 2;
            }
            "--prompt" if i + 1 < args.len() => {
                prompt = args[i + 1].clone();
                i += 2;
            }
            "--model" if i + 1 < args.len() => {
                model_id = args[i + 1].clone();
                i += 2;
            }
            "--visual-context-file" if i + 1 < args.len() => {
                visual_context_file = Some(args[i + 1].clone());
                i += 2;
            }
            "--apply" => {
                apply = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if project_id.is_empty() || prompt.is_empty() {
        eprintln!(
            "Usage: e2e_edit_runner --mode <single|project> --project-id <id> --prompt <text> [--file <rel>] [--visual-context-file <json>] [--model <model>] [--apply]"
        );
        std::process::exit(2);
    }

    let started = Instant::now();
    let proj_dir = axiom_forge_rust_lib::project::get_project_path(&project_id);
    if !proj_dir.is_dir() {
        eprintln!("Project directory not found: {}", proj_dir.display());
        std::process::exit(1);
    }

    match mode.as_str() {
        "single" => {
            if rel_file.is_empty() {
                eprintln!("--file is required for --mode single");
                std::process::exit(2);
            }
            let full_path = proj_dir.join(&rel_file);
            let before_content = match fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Cannot read target file '{}': {}", full_path.display(), e);
                    std::process::exit(1);
                }
            };
            let visual_ctx: Option<serde_json::Value> = visual_context_file
                .as_deref()
                .and_then(|p| fs::read_to_string(p).ok())
                .and_then(|raw| serde_json::from_str(&raw).ok());

            println!(
                "__AXIOM_EDIT_PROGRESS__:{}",
                serde_json::json!({
                    "status": format!("Pozivam editor_ai_edit za '{}'...", rel_file),
                    "percent": 20.0
                })
            );

            match axiom_forge_rust_lib::editor_ai::editor_ai_edit(
                rel_file.clone(),
                before_content.clone(),
                prompt.clone(),
                project_id.clone(),
                Some(model_id.clone()),
                visual_ctx,
            )
            .await
            {
                Ok(res) => {
                    let mut applied_ok = false;
                    let mut write_err: Option<String> = None;
                    let mut after_bytes = before_content.len();
                    let mut has_use_client = false;
                    let mut changed = false;

                    if res.success {
                        if let Some(ref new_src) = res.new_content {
                            after_bytes = new_src.len();
                            has_use_client = new_src.contains("\"use client\"") || new_src.contains("'use client'");
                            changed = new_src.trim() != before_content.trim();
                            if apply {
                                println!(
                                    "__AXIOM_EDIT_PROGRESS__:{}",
                                    serde_json::json!({
                                        "status": format!("Snimam izmene preko editor_write_file ({})...", rel_file),
                                        "percent": 80.0
                                    })
                                );
                                let wr = axiom_forge_rust_lib::project::editor_write_file(
                                    project_id.clone(),
                                    rel_file.clone(),
                                    new_src.clone(),
                                );
                                applied_ok = wr.success;
                                write_err = wr.error;
                                // Allow non-blocking background map refresh to settle
                                tokio::time::sleep(Duration::from_millis(900)).await;
                            }
                        }
                    }

                    let payload = serde_json::json!({
                        "success": res.success && (!apply || applied_ok),
                        "mode": "single",
                        "projectId": project_id,
                        "filePath": rel_file,
                        "durationSec": started.elapsed().as_secs(),
                        "beforeBytes": before_content.len(),
                        "afterBytes": after_bytes,
                        "changed": changed,
                        "hasUseClient": has_use_client,
                        "applied": applied_ok,
                        "error": res.error.or(write_err)
                    });
                    println!("__AXIOM_EDIT_RESULT__:{}", payload);
                    std::process::exit(if payload["success"].as_bool() == Some(true) { 0 } else { 1 });
                }
                Err(e) => {
                    let payload = serde_json::json!({
                        "success": false,
                        "mode": "single",
                        "projectId": project_id,
                        "filePath": rel_file,
                        "durationSec": started.elapsed().as_secs(),
                        "error": e
                    });
                    println!("__AXIOM_EDIT_RESULT__:{}", payload);
                    std::process::exit(1);
                }
            }
        }
        "project" => {
            match axiom_forge_rust_lib::editor_ai::editor_ai_project_edit_opt(
                None,
                project_id.clone(),
                prompt.clone(),
                Some(model_id.clone()),
            )
            .await
            {
                Ok(res) => {
                    let mut modified_paths: Vec<String> = Vec::new();
                    let mut touched_schema = false;
                    let mut touched_api = false;
                    let mut has_use_client_any = false;
                    let mut applied_count = 0usize;
                    let mut write_errors: Vec<String> = Vec::new();

                    if res.success {
                        if let Some(ref map) = res.modified_files {
                            let mut keys: Vec<String> = map.keys().cloned().collect();
                            keys.sort();
                            for rel in keys {
                                let low = rel.replace('\\', "/").to_lowercase();
                                if low.ends_with(".prisma") {
                                    touched_schema = true;
                                }
                                if low.contains("/api/") || low.starts_with("api/") || low.ends_with("/route.ts") {
                                    touched_api = true;
                                }
                                if let Some(new_src) = map.get(&rel) {
                                    if new_src.contains("\"use client\"") || new_src.contains("'use client'") {
                                        has_use_client_any = true;
                                    }
                                    modified_paths.push(rel.clone());
                                    if apply {
                                        let wr = axiom_forge_rust_lib::project::editor_write_file(
                                            project_id.clone(),
                                            rel.clone(),
                                            new_src.clone(),
                                        );
                                        if wr.success {
                                            applied_count += 1;
                                        } else if let Some(err) = wr.error {
                                            write_errors.push(format!("{}: {}", rel, err));
                                        }
                                    }
                                }
                            }
                            if apply && applied_count > 0 {
                                tokio::time::sleep(Duration::from_millis(1200)).await;
                            }
                        }
                    }

                    let payload = serde_json::json!({
                        "success": res.success && write_errors.is_empty(),
                        "mode": "project",
                        "projectId": project_id,
                        "durationSec": started.elapsed().as_secs(),
                        "modifiedFiles": modified_paths,
                        "fileCount": modified_paths.len(),
                        "touchedSchema": touched_schema,
                        "touchedApi": touched_api,
                        "hasUseClientOnAny": has_use_client_any,
                        "applied": applied_count > 0 && write_errors.is_empty(),
                        "appliedCount": applied_count,
                        "error": res.error.or_else(|| if write_errors.is_empty() { None } else { Some(write_errors.join("; ")) })
                    });
                    println!("__AXIOM_EDIT_RESULT__:{}", payload);
                    std::process::exit(if payload["success"].as_bool() == Some(true) { 0 } else { 1 });
                }
                Err(e) => {
                    let payload = serde_json::json!({
                        "success": false,
                        "mode": "project",
                        "projectId": project_id,
                        "durationSec": started.elapsed().as_secs(),
                        "error": e
                    });
                    println!("__AXIOM_EDIT_RESULT__:{}", payload);
                    std::process::exit(1);
                }
            }
        }
        other => {
            eprintln!("Unknown --mode '{}', expected 'single' or 'project'", other);
            std::process::exit(2);
        }
    }
}
