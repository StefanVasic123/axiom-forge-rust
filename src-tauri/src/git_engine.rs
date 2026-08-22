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

    // 4. Extract diff before commit
    let diff_raw = run_git_cmd(&proj_dir, &["diff", "--staged"]).await.unwrap_or_default();

    // 5. Commit changes on proposal branch
    let commit_msg = format!("Client Proposal: {}", prompt);
    let _ = run_git_cmd(&proj_dir, &["commit", "-m", &commit_msg]).await?;

    // 6. Parse diff into file list
    let mut files = Vec::new();
    if !diff_raw.is_empty() {
        files.push(ProposalFileDiff {
            file_path: "Modified Files".to_string(),
            diff: diff_raw,
            is_new: false,
        });
    }

    let proposal = ClientProposal {
        id: proposal_id.clone(),
        project_id: project_id.clone(),
        branch_name: branch_name.clone(),
        prompt: prompt.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        status: "pending_review".to_string(),
        files,
    };

    // 7. Save proposal JSON locally
    let prop_dir = get_proposals_dir(&proj_dir);
    let _ = fs::create_dir_all(&prop_dir);
    let prop_file = prop_dir.join(format!("{}.json", proposal_id));
    let content = serde_json::to_string_pretty(&proposal).map_err(|e| e.to_string())?;
    fs::write(prop_file, content).map_err(|e| e.to_string())?;

    // 8. Return to original branch
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

    // If approved, merge the proposal branch into current branch
    if status == "approved" {
        let current_branch = run_git_cmd(&proj_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .unwrap_or_else(|_| "main".to_string());

        let _ = run_git_cmd(&proj_dir, &["merge", &proposal.branch_name]).await?;
        let _ = run_git_cmd(&proj_dir, &["checkout", &current_branch]).await;
    }

    let updated_content = serde_json::to_string_pretty(&proposal).map_err(|e| e.to_string())?;
    fs::write(prop_file, updated_content).map_err(|e| e.to_string())?;

    Ok(proposal)
}
