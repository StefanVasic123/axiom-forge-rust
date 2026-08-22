use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AppModeType {
    Developer,
    Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgencyConfig {
    pub agency_name: String,
    pub setup_key: String,
    pub relay_url: Option<String>,
    pub brand_color: Option<String>,
    pub logo_url: Option<String>,
    pub client_project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppModeState {
    pub mode: AppModeType,
    pub agency_config: Option<AgencyConfig>,
}

impl Default for AppModeState {
    fn default() -> Self {
        Self {
            mode: AppModeType::Developer,
            agency_config: None,
        }
    }
}

pub struct ModeState(pub Mutex<AppModeState>);

fn get_mode_file_path(base_dir: &PathBuf) -> PathBuf {
    base_dir.join("app-mode.json")
}

pub fn load_mode_state(base_dir: &PathBuf) -> AppModeState {
    let file_path = get_mode_file_path(base_dir);
    if file_path.exists() {
        if let Ok(content) = fs::read_to_string(&file_path) {
            if let Ok(state) = serde_json::from_str::<AppModeState>(&content) {
                return state;
            }
        }
    }
    AppModeState::default()
}

pub fn save_mode_state_to_file(base_dir: &PathBuf, state: &AppModeState) -> Result<(), String> {
    let file_path = get_mode_file_path(base_dir);
    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(file_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_app_mode(state: tauri::State<'_, ModeState>) -> AppModeState {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_app_mode(
    mode: AppModeType,
    state: tauri::State<'_, ModeState>,
) -> Result<AppModeState, String> {
    let mut lock = state.0.lock().unwrap();
    lock.mode = mode;
    
    // Save to disk
    let local_data = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let base_dir = local_data.join("axiom-forge");
    let _ = fs::create_dir_all(&base_dir);
    save_mode_state_to_file(&base_dir, &lock)?;

    Ok(lock.clone())
}

#[tauri::command]
pub fn save_agency_config(
    config: AgencyConfig,
    state: tauri::State<'_, ModeState>,
) -> Result<AppModeState, String> {
    let mut lock = state.0.lock().unwrap();
    lock.agency_config = Some(config);

    // Save to disk
    let local_data = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let base_dir = local_data.join("axiom-forge");
    let _ = fs::create_dir_all(&base_dir);
    save_mode_state_to_file(&base_dir, &lock)?;

    Ok(lock.clone())
}
