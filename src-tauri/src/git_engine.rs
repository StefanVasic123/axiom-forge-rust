use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use crate::get_project_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalFileDiff {
    pub file_path: String,
    pub diff: String,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProposal {
    pub id: String,
    pub project_id: String,
    pub branch_name: String,
    pub prompt: String,
    pub created_at: String,
    pub status: String, // "pending_review" | "approved" | "rejected"
    pub files: Vec<ProposalFileDiff>,
}

fn get_proposals_dir(proj_dir: &PathBuf) -> PathBuf {
    proj_dir.join(".axiom-proposals")
}

async fn run_git_cmd(proj_dir: &PathBuf, args: &[&str]) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args).current_dir(proj_dir);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd.output().await.map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let err_msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if err_msg.is_empty() { String::from_utf8_lossy(&output.stdout).trim().to_string() } else { err_msg })
    }
}

async fn ensure_git_repo(proj_dir: &PathBuf) -> Result<(), String> {
    let git_dir = proj_dir.join(".git");
    if !git_dir.exists() {
        let _ = run_git_cmd(proj_dir, &["init"]).await?;
        let _ = run_git_cmd(proj_dir, &["add", "."]).await;
        let _ = run_git_cmd(proj_dir, &["commit", "-m", "Initial commit"]).await;
    }
    Ok(())
}

async fn get_default_branch(proj_dir: &PathBuf) -> String {
    if run_git_cmd(proj_dir, &["rev-parse", "--verify", "main"]).await.is_ok() {
        "main".to_string()
    } else if run_git_cmd(proj_dir, &["rev-parse", "--verify", "master"]).await.is_ok() {
        "master".to_string()
    } else {
        "main".to_string()
    }
}

#[tauri::command]
pub async fn git_create_client_proposal(
    project_id: String,
    prompt: String,
) -> Result<ClientProposal, String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }

    ensure_git_repo(&proj_dir).await?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let proposal_id = format!("prop-{}", timestamp);
    let branch_name = format!("client-proposal/{}", timestamp);

    // 1. Get current branch (default to main)
    let original_branch = run_git_cmd(&proj_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await
        .unwrap_or_else(|_| "main".to_string());

    // 2. Create and checkout proposal branch
    let _ = run_git_cmd(&proj_dir, &["checkout", "-b", &branch_name]).await?;

    // 3. Stage changes
    let _ = run_git_cmd(&proj_dir, &["add", "."]).await;

    // 4. Extract diff before commit per file
    let name_only = run_git_cmd(&proj_dir, &["diff", "--name-only", "--staged"]).await.unwrap_or_default();
    let mut files = Vec::new();
    for f in name_only.lines() {
        let f_trim = f.trim();
        if !f_trim.is_empty() {
            let f_diff = run_git_cmd(&proj_dir, &["diff", "--staged", "--", f_trim]).await.unwrap_or_default();
            files.push(ProposalFileDiff {
                file_path: f_trim.to_string(),
                diff: f_diff,
                is_new: false,
            });
        }
    }

    let diff_raw = run_git_cmd(&proj_dir, &["diff", "--staged"]).await.unwrap_or_default();
    if files.is_empty() && !diff_raw.is_empty() {
        files.push(ProposalFileDiff {
            file_path: "Modified Files".to_string(),
            diff: diff_raw,
            is_new: false,
        });
    }

    // 5. Commit changes on proposal branch
    let commit_msg = format!("Client Proposal: {}", prompt);
    let _ = run_git_cmd(&proj_dir, &["commit", "-m", &commit_msg]).await?;

    let proposal = ClientProposal {
        id: proposal_id.clone(),
        project_id: project_id.clone(),
        branch_name: branch_name.clone(),
        prompt: prompt.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        status: "pending_review".to_string(),
        files,
    };

    // 6. Save proposal JSON locally
    let prop_dir = get_proposals_dir(&proj_dir);
    let _ = fs::create_dir_all(&prop_dir);
    let prop_file = prop_dir.join(format!("{}.json", proposal_id));
    let content = serde_json::to_string_pretty(&proposal).map_err(|e| e.to_string())?;
    fs::write(prop_file, content).map_err(|e| e.to_string())?;

    // 7. Return to original branch
    let _ = run_git_cmd(&proj_dir, &["checkout", &original_branch]).await;

    Ok(proposal)
}

#[tauri::command]
pub async fn git_get_client_proposals(
    project_id: String,
) -> Result<Vec<ClientProposal>, String> {
    let proj_dir = get_project_path(&project_id);
    let prop_dir = get_proposals_dir(&proj_dir);

    if !prop_dir.exists() {
        return Ok(vec![]);
    }

    let mut proposals = Vec::new();
    if let Ok(entries) = fs::read_dir(&prop_dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(prop) = serde_json::from_str::<ClientProposal>(&content) {
                            proposals.push(prop);
                        }
                    }
                }
            }
        }
    }

    proposals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(proposals)
}

#[tauri::command]
pub async fn git_checkout_proposal_branch(
    project_id: String,
    branch_name: String,
) -> Result<(), String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }
    run_git_cmd(&proj_dir, &["checkout", &branch_name]).await.map(|_| ())
}

#[tauri::command]
pub async fn git_restore_main_branch(
    project_id: String,
) -> Result<(), String> {
    let proj_dir = get_project_path(&project_id);
    if !proj_dir.exists() {
        return Err("Project directory not found".to_string());
    }
    let default_branch = get_default_branch(&proj_dir).await;
    run_git_cmd(&proj_dir, &["checkout", &default_branch]).await.map(|_| ())
}

#[tauri::command]
pub async fn git_refine_proposal_with_ai(
    window: tauri::Window,
    project_id: String,
    proposal_id: String,
    developer_prompt: String,
    model_id: Option<String>,
) -> Result<ClientProposal, String> {
    let proj_dir = get_project_path(&project_id);
    let prop_file = get_proposals_dir(&proj_dir).join(format!("{}.json", proposal_id));

    if !prop_file.exists() {
        return Err("Proposal not found".to_string());
    }

    let content = fs::read_to_string(&prop_file).map_err(|e| e.to_string())?;
    let mut proposal: ClientProposal = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // 1. Checkout the proposal branch
    run_git_cmd(&proj_dir, &["checkout", &proposal.branch_name]).await?;

    // 2. Call editor_ai_project_edit to apply changes
    let edit_res = crate::editor_ai::editor_ai_project_edit(
        window,
        project_id.clone(),
        developer_prompt.clone(),
        model_id,
    ).await?;

    if !edit_res.success {
        // Return to default branch before erroring
        let default_branch = get_default_branch(&proj_dir).await;
        let _ = run_git_cmd(&proj_dir, &["checkout", &default_branch]).await;
        return Err(edit_res.error.unwrap_or_else(|| "AI edit failed".to_string()));
    }

    // 3. Stage changes
    let _ = run_git_cmd(&proj_dir, &["add", "."]).await;

    // 4. Commit changes
    let commit_msg = format!("Developer Refinement: {}", developer_prompt);
    let _ = run_git_cmd(&proj_dir, &["commit", "-m", &commit_msg]).await;

    // 5. Recompute diff against main branch
    let default_branch = get_default_branch(&proj_dir).await;
    let name_only = run_git_cmd(&proj_dir, &["diff", "--name-only", &default_branch, "HEAD"]).await.unwrap_or_default();
    let mut files = Vec::new();
    for f in name_only.lines() {
        let f_trim = f.trim();
        if !f_trim.is_empty() {
            let f_diff = run_git_cmd(&proj_dir, &["diff", &default_branch, "HEAD", "--", f_trim]).await.unwrap_or_default();
            files.push(ProposalFileDiff {
                file_path: f_trim.to_string(),
                diff: f_diff,
                is_new: false,
            });
        }
    }

    if files.is_empty() {
        let diff_raw = run_git_cmd(&proj_dir, &["diff", &default_branch, "HEAD"]).await.unwrap_or_default();
        if !diff_raw.is_empty() {
            files.push(ProposalFileDiff {
                file_path: "Modified Files".to_string(),
                diff: diff_raw,
                is_new: false,
            });
        }
    }

    proposal.files = files;
    proposal.prompt = format!("{} [AI Dorada: {}]", proposal.prompt, developer_prompt);

    // 6. Save updated proposal JSON
    let updated_content = serde_json::to_string_pretty(&proposal).map_err(|e| e.to_string())?;
    fs::write(prop_file, updated_content).map_err(|e| e.to_string())?;

    // 7. Return to default branch so background repository state stays on main
    let _ = run_git_cmd(&proj_dir, &["checkout", &default_branch]).await;

    Ok(proposal)
}

#[tauri::command]
pub async fn git_update_proposal_status(
    project_id: String,
    proposal_id: String,
    status: String,
) -> Result<ClientProposal, String> {
    let proj_dir = get_project_path(&project_id);
    let prop_file = get_proposals_dir(&proj_dir).join(format!("{}.json", proposal_id));

    if !prop_file.exists() {
        return Err("Proposal not found".to_string());
    }

    let content = fs::read_to_string(&prop_file).map_err(|e| e.to_string())?;
    let mut proposal: ClientProposal = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    proposal.status = status.clone();

    // If approved, merge the proposal branch into default branch
    if status == "approved" {
        let default_branch = get_default_branch(&proj_dir).await;
        // Make sure we are on default branch before merging
        let _ = run_git_cmd(&proj_dir, &["checkout", &default_branch]).await;
        let _ = run_git_cmd(&proj_dir, &["merge", &proposal.branch_name]).await?;
    }

    let updated_content = serde_json::to_string_pretty(&proposal).map_err(|e| e.to_string())?;
    fs::write(prop_file, updated_content).map_err(|e| e.to_string())?;

    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_proposal_serialization() {
        let prop = ClientProposal {
            id: "prop-100".to_string(),
            project_id: "my-test-proj".to_string(),
            branch_name: "client-proposal/20260823-100000".to_string(),
            prompt: "Change header color".to_string(),
            created_at: "2026-08-23T14:50:00Z".to_string(),
            status: "pending_review".to_string(),
            files: vec![ProposalFileDiff {
                file_path: "src/App.jsx".to_string(),
                diff: "+ const color = 'blue';".to_string(),
                is_new: false,
            }],
        };

        let json = serde_json::to_string(&prop).unwrap();
        let parsed: ClientProposal = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "prop-100");
        assert_eq!(parsed.status, "pending_review");
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.files[0].file_path, "src/App.jsx");
    }
}
