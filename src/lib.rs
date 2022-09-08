
pub mod telegram;
pub mod sensors;
// pub mod mqtt;

use std::sync::Arc;
use sensors::PrevSensorsData;
use tokio::sync::Mutex;

pub struct SharedState {
    pub prev_sensors_data: PrevSensorsData,
    pub notifications_enabled: bool
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            prev_sensors_data: PrevSensorsData::new(),
            notifications_enabled: false
        }
    }
}

pub type ProtectedSharedState = Arc<Mutex<SharedState>>;