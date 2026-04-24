use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub server_url: String,
    pub device_name: String,
    pub bootstrap_code: String,
}

impl AppState {
    pub fn load(paths: &AppPaths) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(&paths.state_path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, paths: &AppPaths) -> anyhow::Result<()> {
        std::fs::create_dir_all(&paths.state_dir)?;
        std::fs::write(&paths.state_path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
