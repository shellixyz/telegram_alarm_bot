
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::time;
use compound_duration::format_dhms;

pub type Data = HashMap<String, serde_json::Value>;

trait TimeSinceLastUpdate {
    fn time_since_last_update(&self) -> time::Duration;
}

pub struct CommonBatteryState {
    pub(self) update_timestamp: time::Instant,
    value: u8
}

impl CommonBatteryState {
    fn new(value: u8) -> Self {
        Self {
            update_timestamp: time::Instant::now(),
            value
        }
    }
}

impl TimeSinceLastUpdate for CommonBatteryState {
    fn time_since_last_update(&self) -> time::Duration {
        self.update_timestamp.elapsed()
    }
}

pub struct CommonVoltageState {
    update_timestamp: time::Instant,
    value: f32
}

impl CommonVoltageState {
    fn new(value: f32) -> Self {
        Self {
            update_timestamp: time::Instant::now(),
            value
        }
    }
}

impl TimeSinceLastUpdate for CommonVoltageState {
    fn time_since_last_update(&self) -> time::Duration {
        self.update_timestamp.elapsed()
    }
}

pub struct CommonState {
    battery: Option<CommonBatteryState>,
    voltage: Option<CommonVoltageState>
}

impl CommonState {
    fn new(battery: Option<u8>, voltage: Option<f32>) -> Self {
        Self {
            battery: battery.map(|battery_value| CommonBatteryState::new(battery_value)),
            voltage: voltage.map(|voltage_value| CommonVoltageState::new(voltage_value)),
        }
    }

    pub fn update_battery(&mut self, battery: u8) {
        self.battery = Some(CommonBatteryState {
            update_timestamp: time::Instant::now(),
            value: battery
        });
    }

    pub fn update_voltage(&mut self, voltage: f32) {
        self.voltage = Some(CommonVoltageState {
            update_timestamp: time::Instant::now(),
            value: voltage
        });
    }

    pub fn time_max_since_last_update(&self) -> Option<time::Duration> {
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
            Some(duration) => format!("last update {} ago", format_dhms(duration.as_secs())),
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

pub enum SpecificStateValue {
    Contact { contact: bool },
    Motion { occupancy: bool }
}

pub struct SpecificState {
    timestamp: time::Instant,
    pub value: SpecificStateValue
}

impl SpecificState {
    fn new(value: SpecificStateValue) -> Self {
        Self {
            timestamp: time::Instant::now(),
            value
        }
    }

    pub fn time_since_last_update(&self) -> time::Duration {
        self.timestamp.elapsed()
    }
}

pub struct PrevData {
    pub common: CommonState,
    pub specific: Option<SpecificState>
}

impl PrevData {
    fn new(battery: Option<u8>, voltage: Option<f32>, sensor_value: Option<SpecificStateValue>) -> Self {
        Self {
            common: CommonState::new(battery, voltage),
            specific: sensor_value.map(|specific_state_value| SpecificState::new(specific_state_value))
        }
    }
}

type Name = String;

pub struct PrevSensorsData(HashMap<Name, PrevData>);

impl PrevSensorsData {

    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn get(&self, sensor_name: &str) -> Option<&PrevData> {
        self.0.get(sensor_name)
    }

    pub fn insert(&mut self, sensor_name: &str, battery: Option<u8>, voltage: Option<f32>, sensor_value: Option<SpecificStateValue>) {
        self.0.insert(sensor_name.to_string(), PrevData::new(battery, voltage, sensor_value));
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<String, PrevData> {
        self.0.iter()
    }
}

pub enum Sensor {
    Contact,
    Motion {
        location: Option<String>,
        id: usize
    }
}

impl Sensor {

    pub fn identify(sensor_name: &str) -> Result<Sensor, &'static str> {
        match sensor_name {
            "Door opening sensor" => Ok(Sensor::Contact),
            _ => {
                lazy_static! { static ref MOTION_SENSOR_RE: Regex = Regex::new(r"^(?:(?P<location>\w+) )?[Mm]otion sensor \((?P<id>\d+)\)$").unwrap(); }
                if let Some(captures) = MOTION_SENSOR_RE.captures(sensor_name) {
                    let id = captures.name("id").unwrap().as_str().parse::<usize>().unwrap();
                    let location = captures.name("location").map(|rematch| rematch.as_str().to_owned());
                    Ok(Sensor::Motion { location, id })
                } else {
                    Err("failed to parse sensor name")
                }
            }
        }
    }

    pub fn message(&self, sensor_data: &Data, prev_state: &Option<SpecificStateValue>) -> Result<Option<String>, &'static str> {
        match self {
            Sensor::Contact => Self::contact_sensor_message(sensor_data, prev_state),
            Sensor::Motion { .. } => self.motion_sensor_message(sensor_data, prev_state)
        }
    }

    fn contact_sensor_message(sensor_data: &Data, prev_state: &Option<SpecificStateValue>) -> Result<Option<String>, &'static str> {

        let contact_value = match sensor_data.get("contact") {
            Some(serde_json::Value::Bool(bool_value)) => bool_value,
            _ => return Err("Unexpected value type for contact")
        };

        match prev_state {
            Some(SpecificStateValue::Contact { contact: prev_contact_value }) => {
                if contact_value == prev_contact_value {
                    return Ok(None);
                }
            },
            None => {},
            _ => {
                let msg = "prev specific state has an unmatched type";
                log::info!("{}", msg);
                return Err(msg)
            }
        }

        let message = match contact_value {
            false => "La porte d'entrée vient d'être ouverte",
            true => "La porte d'entrée vient d'être fermée"
        };

        Ok(Some(message.to_owned()))
    }

    fn motion_sensor_message(&self, sensor_data: &Data, prev_state: &Option<SpecificStateValue>) -> Result<Option<String>, &'static str> {
        if let Sensor::Motion { location, id } = self {
            let occupancy_value = match sensor_data.get("occupancy") {
                Some(serde_json::Value::Bool(bool_value)) => bool_value,
                None => return Err("Cannot find occupancy value in motion sensor notification payload"),
                _ => return Err("Unexpected value type for motion sensor notification's occupancy value")
            };

            if *occupancy_value {

                match prev_state {
                    Some(SpecificStateValue::Motion { occupancy: prev_occupancy_value }) => {
                        if occupancy_value == prev_occupancy_value {
                            return Ok(None);
                        }
                    },
                    None => {},
                    _ => {
                        let msg = "prev specific state has an unmatched type";
                        log::info!("{}", msg);
                        return Err(msg)
                    }
                }

                match location {
                    Some(location) =>
                        Ok(Some(format!("Mouvement détecté dans {} (capteur #{})", location.to_lowercase(), id))),
                    None =>
                        Ok(Some(format!("Mouvement détecté par capteur #{}", id))),
                }

            } else {
                Ok(None)
            }
        } else {
            Err("motion_sensor_message called on something else than a motion sensor")
        }
    }

}