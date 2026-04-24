use crate::registry::RegistryStore;

#[derive(Debug, Clone)]
pub struct ControlState {
    pub registry: RegistryStore,
    pub shared_secret: String,
}

impl ControlState {
    pub async fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("OVERLAY_CONTROL_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://control-plane.db".into());
        Ok(Self {
            registry: RegistryStore::connect(&database_url).await?,
            shared_secret: std::env::var("OVERLAY_SHARED_SECRET")
                .unwrap_or_else(|_| "local-dev-secret".into()),
        })
    }
}
