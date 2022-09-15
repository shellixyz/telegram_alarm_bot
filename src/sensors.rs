
use std::collections::HashMap;
use compound_duration::format_dhms;
use serde::{Serialize,Deserialize};
use derive_more::{Deref,DerefMut};

use crate::time::{LastSeenDuration,Timestamp};

const SENSORS_DATA_PATH: &str = "sensors_data.json";

pub type Data = HashMap<String, serde_json::Value>;

trait TimeSinceLastUpdate {
    fn time_since_last_update(&self) -> LastSeenDuration;
}

#[derive(Serialize,Deserialize)]
pub struct CommonBatteryState {
    update_timestamp: Timestamp,
    value: u8
}

impl TimeSinceLastUpdate for CommonBatteryState {
    fn time_since_last_update(&self) -> LastSeenDuration {
        LastSeenDuration::new(&self.update_timestamp)
    }
}

#[derive(Serialize,Deserialize)]
pub struct CommonVoltageState {
    update_timestamp: Timestamp,
    value: f32
}

impl TimeSinceLastUpdate for CommonVoltageState {
    fn time_since_last_update(&self) -> LastSeenDuration {
        LastSeenDuration::new(&self.update_timestamp)
    }
}

#[derive(Serialize,Deserialize,Default)]
pub struct CommonState {
    battery: Option<CommonBatteryState>,
    voltage: Option<CommonVoltageState>
}

impl CommonState {

    pub fn update_battery(&mut self, battery: u8) {
        self.battery = Some(CommonBatteryState {
            update_timestamp: Timestamp::now(),
            value: battery
        });
    }

    pub fn update_voltage(&mut self, voltage: f32) {
        self.voltage = Some(CommonVoltageState {
            update_timestamp: Timestamp::now(),
            value: voltage
        });
    }

    pub fn time_min_since_last_update(&self) -> Option<LastSeenDuration> {
        match (&self.battery, &self.voltage) {
            (None, None) => None,
            (None, Some(voltage)) => Some(voltage.time_since_last_update()),
            (Some(battery), None) => Some(battery.time_since_last_update()),
            (Some(battery), Some(voltage)) =>
                Some(std::cmp::min(battery.time_since_last_update(), voltage.time_since_last_update())),
        }
    }

    pub fn time_max_since_last_update(&self) -> Option<LastSeenDuration> {
        match (&self.battery, &self.voltage) {
            (None, None) => None,
            (None, Some(voltage)) => Some(voltage.time_since_last_update()),
            (Some(battery), None) => Some(battery.time_since_last_update()),
            (Some(battery), Some(voltage)) =>
                Some(std::cmp::max(battery.time_since_last_update(), voltage.time_since_last_update())),
        }
    }

    pub fn time_max_since_last_update_str(&self) -> String {
        match self.time_max_since_last_update() {
            Some(duration) => format!("last update {} ago", format_dhms(std::cmp::max(0, duration.num_seconds()))),
            None => "no data".to_owned(),
        }
    }

    pub fn battery_value_str(&self) -> String {
        match &self.battery {
            Some(battery_state) => format!("{}%", battery_state.value),
            None => "unknown percent".to_owned(),
        }
    }

    pub fn voltage_value_str(&self) -> String {
        match &self.voltage {
            Some(voltage_state) => format!("{:.3}v", voltage_state.value),
            None => "unknown voltage".to_owned(),
        }
    }

}

pub type SensorValue = serde_json::Value;

pub type TriggerStates = HashMap<String, SensorValue>;

#[derive(Serialize,Deserialize)]
pub struct PrevData {
    #[serde(flatten)]
    pub common: CommonState,

    pub update_timestamp: Timestamp,

    #[serde(skip)]
    pub trigger_states: TriggerStates
}

impl Default for PrevData {
    fn default() -> Self {
        Self {
            common: Default::default(),
            update_timestamp: Timestamp::now(),
            trigger_states: Default::default() }
    }
}

impl PrevData {

    pub fn time_since_last_seen(&self) -> LastSeenDuration {
        LastSeenDuration::new(&self.update_timestamp)
    }

    pub fn last_seen_now(&mut self) {
        self.update_timestamp = Timestamp::now();
    }

    pub fn update_battery(&mut self, battery: u8) {
        self.common.update_battery(battery);
        self.last_seen_now();
    }

    pub fn update_voltage(&mut self, voltage: f32) {
        self.common.update_voltage(voltage);
        self.last_seen_now();
    }

}

type Name = String;

type PrevSensorsDataInner = HashMap<Name, PrevData>;

#[derive(Serialize,Deserialize,Default,Deref,DerefMut)]
pub struct PrevSensorsData(PrevSensorsDataInner);

impl PrevSensorsData {

    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn save_to_file(&self) -> Result<(), String> {
        match serde_json::to_string_pretty(self) {
            Ok(prev_sensors_data_json) => if let Err(error) = std::fs::write(SENSORS_DATA_PATH, prev_sensors_data_json) {
                Err(format!("sensors file write error: {}", error))
            } else {
                Ok(())
            },
            Err(error) => {
                Err(format!("error serializing sensors data: {}", error))
            }
        }
    }

    pub fn load_from_file() -> Result<Self, String> {
        let file = std::fs::File::open(SENSORS_DATA_PATH).map_err(|open_error| format!("open error: {}: {}", SENSORS_DATA_PATH, open_error))?;
        let reader = std::io::BufReader::new(file);
        serde_json::from_reader(reader).map_err(|deser_err| format!("error deserializing sensors data: {}", deser_err))
    }

}
