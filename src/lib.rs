
pub mod telegram;
pub mod time;
pub mod sensors;
pub mod config;
pub mod mqtt;
pub mod log_level;

use std::sync::Arc;
use sensors::PrevSensorsData;
use tokio::sync::Mutex;

pub struct SharedState {
    pub prev_sensors_data: PrevSensorsData,
    pub notifications_enabled: bool
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            prev_sensors_data: PrevSensorsData::new(),
            notifications_enabled: false
        }
    }
}

pub type ProtectedSharedState = Arc<Mutex<SharedState>>;
