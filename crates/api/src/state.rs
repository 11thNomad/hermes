use common::{AppConfig, SharedServices};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    #[allow(dead_code)]
    pub services: SharedServices,
}
