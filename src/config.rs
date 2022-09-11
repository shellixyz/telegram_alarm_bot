

use std::collections::HashMap;
use serde::Deserialize;
use teloxide::types::ChatId;

#[derive(Deserialize, Debug)]
pub struct MqttBroker {
    pub hostname: String,
    pub port: u16
}

pub type SensorState = String;
pub type SensorStateMessage = String;

#[derive(Deserialize, Debug)]
pub struct SensorStateMessages(pub HashMap<SensorState, SensorStateMessage>);

pub type SensorNameRegex = String;
pub type PayloadFieldName = String;

pub type PayloadFieldNameAndStateMessages = HashMap<PayloadFieldName, SensorStateMessages>;

#[derive(Deserialize, Debug)]
pub struct Sensor(pub HashMap<SensorNameRegex, PayloadFieldNameAndStateMessages>);

pub type MqttTopic = String;
pub type MqttTopics = HashMap<MqttTopic, Sensor>;

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

#[derive(Deserialize, Debug)]
pub struct Config {
    pub mqtt_broker: Option<MqttBroker>,

    pub telegram: Telegram,

    #[serde(rename = "sensors")]
    pub mqtt_topics: MqttTopics
}

impl Config {

    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|open_error| format!("open error: {}: {}", path, open_error))?;
        let reader = std::io::BufReader::new(file);
        serde_json::from_reader(reader).map_err(|deser_err| format!("error deserializing sensors data: {}", deser_err))
    }

    pub fn mqtt_topics(&self) -> Vec<&String> {
        self.mqtt_topics.keys().collect()
    }

    pub fn mqtt_subscribe_patterns(&self) -> Vec<String> {
        self.mqtt_topics.keys().map(|mqtt_topic| format!("{mqtt_topic}/+")).collect()
    }

}