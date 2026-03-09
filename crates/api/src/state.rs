use common::{AppConfig, SharedServices};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub services: SharedServices,
}
