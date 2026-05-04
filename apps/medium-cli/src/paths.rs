use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub home_dir: PathBuf,
    pub app_config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub state_path: PathBuf,
    pub ssh_dir: PathBuf,
    pub ssh_config_path: PathBuf,
    pub ssh_config_dir: PathBuf,
    pub overlay_ssh_config_path: PathBuf,
}

impl AppPaths {
    pub fn from_home(home_dir: impl AsRef<Path>) -> Self {
        #[cfg(target_os = "macos")]
        {
            return Self::for_macos_home(home_dir);
        }

        #[cfg(not(target_os = "macos"))]
        {
            return Self::for_linux_home(home_dir);
        }
    }

    pub fn for_linux_home(home_dir: impl AsRef<Path>) -> Self {
        let home_dir = home_dir.as_ref().to_path_buf();
        let app_config_dir = home_dir.join(".medium");
        let state_dir = home_dir.join(".local").join("share").join("medium");
        let ssh_dir = home_dir.join(".ssh");
        let ssh_config_dir = ssh_dir.join("config.d");

        Self {
            state_dir: state_dir.clone(),
            state_path: state_dir.join("state.json"),
            ssh_config_path: ssh_dir.join("config"),
            overlay_ssh_config_path: ssh_config_dir.join("medium.conf"),
            home_dir,
            app_config_dir,
            ssh_dir,
            ssh_config_dir,
        }
    }

    pub fn for_macos_home(home_dir: impl AsRef<Path>) -> Self {
        let home_dir = home_dir.as_ref().to_path_buf();
        let app_config_dir = home_dir.join(".medium");
        let app_root = home_dir
            .join("Library")
            .join("Application Support")
            .join("Medium");
        let state_dir = app_root.join("state");
        let ssh_dir = home_dir.join(".ssh");
        let ssh_config_dir = ssh_dir.join("config.d");

        Self {
            state_dir: state_dir.clone(),
            state_path: state_dir.join("state.json"),
            ssh_config_path: ssh_dir.join("config"),
            overlay_ssh_config_path: ssh_config_dir.join("medium.conf"),
            home_dir,
            app_config_dir,
            ssh_dir,
            ssh_config_dir,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        if let Some(home) = std::env::var_os("MEDIUM_HOME") {
            return Ok(Self::from_home(home));
        }

        if let Some(home) = std::env::var_os("OVERLAY_HOME") {
            return Ok(Self::from_home(home));
        }

        let home = std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
        Ok(Self::from_home(home))
    }
}
