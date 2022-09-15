
use std::{collections::HashMap, iter::FromIterator};
use regex::Regex;
use serde::Deserialize;
use teloxide::types::ChatId;
use derive_more::Deref;
use thiserror::Error;
use crate::log_level::LogLevel;

#[derive(Deserialize, Debug)]
pub struct MqttBroker {
    pub hostname: String,
    pub port: u16
}

pub type SensorState = String;
pub type SensorStateMessage = String;

pub type SensorStateMessagesInner = HashMap<SensorState, SensorStateMessage>;

#[derive(Deserialize, Debug, Deref)]
pub struct SensorStateMessages(SensorStateMessagesInner);

pub type SensorNameRegex = String;
pub type PayloadFieldName = String;

pub type SensorPayloadFieldNameAndStateMessagesInner = HashMap<PayloadFieldName, SensorStateMessages>;

#[derive(Deserialize, Debug, Deref)]
pub struct SensorPayloadFieldNameAndStateMessages(SensorPayloadFieldNameAndStateMessagesInner);

impl SensorPayloadFieldNameAndStateMessages {

    pub fn payload_field_names(&self) -> Vec<&String> {
        self.keys().collect()
    }

}

pub type SensorName = String;
pub type SensorNameCaptures = HashMap<String, Option<String>>;
pub type SensorsInner = HashMap<SensorNameRegex, SensorPayloadFieldNameAndStateMessages>;

#[derive(Deserialize, Debug, Deref)]
pub struct Sensors(SensorsInner);

impl Sensors {

    // returns: captures, sensor_payload_field_names_and_state_messages
    pub fn match_sensor_name(&self, sensor_name: &str) -> Result<Option<(SensorName, SensorNameCaptures, &SensorPayloadFieldNameAndStateMessages)>, regex::Error> {
        for (sensor_name_re_str, payload_field_name_and_state_messages) in self.0.iter() {
            let re = Regex::new(sensor_name_re_str)?;
            if let Some(captures) = re.captures(sensor_name) {
                let sensor_name = captures.get(0).unwrap().as_str().to_string();
                let name_captures: SensorNameCaptures = HashMap::from_iter(re.capture_names().skip(1).map(|cname| {
                    let cname = cname.unwrap();
                    let cstr = captures.name(cname).map(|ncap| ncap.as_str().to_string());
                    (cname.to_string(), cstr)
                }));
                return Ok(Some((sensor_name, name_captures, payload_field_name_and_state_messages)));
            }
        }
        Ok(None)
    }

}

pub type MqttTopicBase = String;
pub type MqttTopicsInner = HashMap<MqttTopicBase, Sensors>;

#[derive(Deserialize, Debug, Deref)]
pub struct MqttTopics(MqttTopicsInner);

impl MqttTopics {

    // returns: captures, sensor_payload_field_names_and_state_messages
    pub fn match_topic(&self, topic: &String) -> Result<Option<(SensorName, SensorNameCaptures, &SensorPayloadFieldNameAndStateMessages)>, regex::Error> {
        let tmatch = self.0.iter().find(|(topic_base, _)| {
            let base_slash = (*topic_base).clone() + "/";
            topic.starts_with(&base_slash)
        });

        match tmatch {
            Some((topic_base, sensors)) => {
                let sensor_name = &topic[topic_base.len()+1..];
                sensors.match_sensor_name(sensor_name)
            },
            None => Ok(None),
        }
    }

}

mod chat_ids {
    use teloxide::types::ChatId;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<ChatId>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let orig: Vec<i64> = serde::de::Deserialize::deserialize(deserializer)?;
        Ok(orig.iter().map(|id| ChatId(*id)).collect())
    }

    pub fn deserialize_option<'de, D>(deserializer: D) -> Result<Option<Vec<ChatId>>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let orig: Vec<i64> = serde::de::Deserialize::deserialize(deserializer)?;
        Ok(Some(orig.iter().map(|id| ChatId(*id)).collect()))
    }

}

#[derive(Deserialize, Debug)]
pub struct Telegram {
    pub token: String,

    #[serde(deserialize_with = "chat_ids::deserialize")]
    pub notification_chat_ids: Vec<ChatId>,

    #[serde(default, deserialize_with = "chat_ids::deserialize_option")]
    pub admin_chat_ids: Option<Vec<ChatId>>
}

impl Telegram {

    pub fn valid_chat_ids(&self) -> Vec<ChatId> {
        self.notification_chat_ids.clone().into_iter().chain(self.admin_chat_ids.clone().unwrap_or_default().into_iter()).collect()
    }

}

#[derive(Debug, Error)]
pub enum ConfigFileLoadError {
    #[error("IO error")]
    IOError(std::io::Error),
    #[error("deserialization error")]
    DeserializationError(serde_json::Error)
}

fn sensors_data_file_default() -> String {
    "sensors_data.json".to_owned()
}

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub log_level: LogLevel,

    #[serde(default = "sensors_data_file_default")]
    pub sensors_data_file: String,

    pub mqtt_broker: Option<MqttBroker>,

    pub telegram: Telegram,

    #[serde(rename = "sensors")]
    pub mqtt_topics: MqttTopics
}

impl Config {

    pub fn load_from_file(path: &str) -> Result<Self, ConfigFileLoadError> {
        let file = std::fs::File::open(path).map_err(|open_error| ConfigFileLoadError::IOError(open_error))?;
        let reader = std::io::BufReader::new(file);
        serde_json::from_reader(reader).map_err(|deser_err| ConfigFileLoadError::DeserializationError(deser_err))
    }

    pub fn mqtt_topics(&self) -> Vec<&String> {
        self.mqtt_topics.0.keys().collect()
    }

    pub fn mqtt_subscribe_patterns(&self) -> Vec<String> {
        self.mqtt_topics.0.keys().map(|mqtt_topic| format!("{mqtt_topic}/+")).collect()
    }

    pub fn check(&self) -> bool {
        let mut config_good = true;

        // check sensor name regexes
        for (_, sensors) in self.mqtt_topics.iter() {
            for (sensor_name_re, _) in sensors.iter() {
                if let Err(re_error) = Regex::new(&sensor_name_re) {
                    eprintln!("\n{re_error}");
                    config_good = false;
                }
            }
        }

        config_good
    }

}
