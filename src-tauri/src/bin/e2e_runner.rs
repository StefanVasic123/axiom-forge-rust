use std::env;

#[tokio::main]
async fn main() {
    env::set_var("AXIOM_E2E_HEADLESS", "1");

    let args: Vec<String> = env::args().collect();
    let mut manifest_id = String::new();
    let mut project_id = String::new();
    let mut host: Option<String> = None;
    let mut model_id = "qwen2.5-coder:7b".to_string();
    let mut token = "e2e-local-token".to_string();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest-id" if i + 1 < args.len() => {
                manifest_id = args[i + 1].clone();
                i += 2;
            }
            "--project-id" if i + 1 < args.len() => {
                project_id = args[i + 1].clone();
                i += 2;
            }
            "--host" if i + 1 < args.len() => {
                host = Some(args[i + 1].clone());
                i += 2;
            }
            "--model" if i + 1 < args.len() => {
                model_id = args[i + 1].clone();
                i += 2;
            }
            "--token" if i + 1 < args.len() => {
                token = args[i + 1].clone();
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    if manifest_id.is_empty() || project_id.is_empty() {
        eprintln!("Usage: e2e_runner --manifest-id <id> --project-id <id> [--host <url>] [--model <model>]");
        std::process::exit(2);
    }

    let task_id = format!("e2e-task-{}", project_id);
    println!(
        "__AXIOM_PROGRESS__:{}",
        serde_json::json!({
            "taskId": task_id,
            "projectId": project_id,
            "phase": "initializing",
            "status": "generating",
            "message": format!("Pokretanje headless E2E generisanja za manifest '{}' (model: {})...", manifest_id, model_id),
            "progress": 0.5
        })
    );

    match axiom_forge_rust_lib::orchestrator::run_orchestrator_opt(
        None,
        task_id,
        manifest_id,
        project_id,
        token,
        model_id,
        host,
    )
    .await
    {
        Ok(()) => {
            println!("__AXIOM_E2E_DONE__:OK");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("__AXIOM_E2E_ERROR__:{}", e);
            std::process::exit(1);
        }
    }
}
