use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProposalPayload {
    pub setup_key: String,
    pub project_id: String,
    pub prompt: String,
    pub branch_name: String,
    pub files: Vec<Value>,
}

#[tauri::command]
pub async fn relay_submit_proposal(
    relay_url: String,
    payload: RelayProposalPayload,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/v1/proposals/submit", relay_url.trim_end_matches('/'));

    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error sending to relay: {}", e))?;

    if res.status().is_success() {
        let json_val = res.json::<Value>().await.map_err(|e| e.to_string())?;
        Ok(json_val)
    } else {
        Err(format!("Relay HTTP error status: {}", res.status()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_payload_serialization() {
        let payload = RelayProposalPayload {
            setup_key: "MOJ-RESTORAN-2026".to_string(),
            project_id: "camper-connect".to_string(),
            prompt: "Update logo".to_string(),
            branch_name: "client-proposal/20260823".to_string(),
            files: vec![serde_json::json!({ "filePath": "src/App.jsx" })],
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("setupKey"));
        assert!(json.contains("MOJ-RESTORAN-2026"));
        assert!(json.contains("camper-connect"));
    }
}
