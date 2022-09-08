
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::time;

pub type Data = HashMap<String, serde_json::Value>;

pub struct PrevData {
    pub timestamp: time::Instant,
    pub data: Data
}

impl PrevData {
    fn new(sensor_data: Data) -> Self {
        Self {
            timestamp: time::Instant::now(),
            data: sensor_data
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

    pub fn insert(&mut self, sensor_name: &str, sensor_data: Data) {
        self.0.insert(sensor_name.to_string(), PrevData::new(sensor_data));
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

    pub fn message(&self, sensor_data: &Data, prev_sensor_data: Option<&Data>) -> Result<Option<String>, &'static str> {
        match self {
            Sensor::Contact => Self::contact_sensor_message(sensor_data, &prev_sensor_data),
            Sensor::Motion { .. } => self.motion_sensor_message(sensor_data, &prev_sensor_data)
        }
    }

    fn contact_sensor_message(sensor_data: &Data, prev_sensor_data: &Option<&Data>) -> Result<Option<String>, &'static str> {

        let contact_value = match sensor_data.get("contact") {
            Some(serde_json::Value::Bool(bool_value)) => bool_value,
            _ => return Err("Unexpected value type for contact")
        };

        if let Some(prev_sensor_data) = prev_sensor_data {
            if let Some(serde_json::Value::Bool(prev_contact_value)) = prev_sensor_data.get("contact") {
                if contact_value == prev_contact_value {
                    return Ok(None);
                }
            };
        }

        let message = match contact_value {
            false => "La porte d'entrée vient d'être ouverte",
            true => "La porte d'entrée vient d'être fermée"
        };

        Ok(Some(message.to_owned()))
    }

    fn motion_sensor_message(&self, sensor_data: &Data, prev_sensor_data: &Option<&Data>) -> Result<Option<String>, &'static str> {
        if let Sensor::Motion { location, id } = self {
            let occupancy_value = match sensor_data.get("occupancy") {
                Some(serde_json::Value::Bool(bool_value)) => bool_value,
                None => return Err("Cannot find occupancy value in motion sensor notification payload"),
                _ => return Err("Unexpected value type for motion sensor notification's occupancy value")
            };

            if *occupancy_value {

                if let Some(prev_sensor_data) = prev_sensor_data {
                    if let Some(serde_json::Value::Bool(prev_occupancy_value)) = prev_sensor_data.get("occupancy") {
                        if occupancy_value == prev_occupancy_value {
                            return Ok(None);
                        }
                    };
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