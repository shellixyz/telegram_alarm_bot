use lazy_static::lazy_static;
use teloxide::prelude::*;
use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Packet};
use std::collections::HashMap;
use std::time::Duration;
use regex::Regex;

type SensorData = HashMap<String, serde_json::Value>;

const MAISON_ESSERT_CHAT_ID: ChatId = ChatId(-688154163);

enum Sensor {
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

    pub fn message(&self, sensor_data: &SensorData) -> Result<Option<String>, &'static str> {
        match self {
            Sensor::Contact => Self::contact_sensor_message(sensor_data),
            Sensor::Motion { .. } => self.motion_sensor_message(sensor_data)
        }
    }

    fn contact_sensor_message(sensor_data: &SensorData) -> Result<Option<String>, &'static str> {

        let contact_value = match sensor_data.get("contact") {
            Some(serde_json::Value::Bool(bool_value)) => bool_value,
            _ => return Err("Unexpected value type for contact")
        };

        let message = match contact_value {
            false => "La porte d'entrée vient d'être ouverte",
            true => "La porte d'entrée vient d'être fermée"
        };

        Ok(Some(message.to_owned()))
    }

    fn motion_sensor_message(&self, sensor_data: &SensorData) -> Result<Option<String>, &'static str> {
        if let Sensor::Motion { location, id } = self {
            let occupancy_value = match sensor_data.get("occupancy") {
                Some(serde_json::Value::Bool(bool_value)) => bool_value,
                None => return Err("Cannot find occupancy value in motion sensor notification payload"),
                _ => return Err("Unexpected value type for motion sensor notification's occupancy value")
            };

            if *occupancy_value {
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

async fn process_zigbee2mqtt_publish_notification(publish: rumqttc::Publish, bot: &AutoSend<Bot>) -> Result<(), &'static str> {

    // println!("topic: {}, payload: {:?}", publish.topic, publish.payload);

    let sensor_name = publish.topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?;

    let payload_string = String::from_utf8_lossy(&publish.payload).to_string();
    let sensor_data: SensorData = serde_json::from_str(&payload_string).map_err(|_| "Failed to parse publish notification payload")?;

    // let sensor_name = get_sensor_name_from_mqtt_topic(&publish.topic).ok_or("Failed to parse topic to get sensor name");
    let sensor = Sensor::identify(&sensor_name)?;

    if let Some(sensor_message) = sensor.message(&sensor_data)? {
        bot.send_message(MAISON_ESSERT_CHAT_ID, &sensor_message).await.unwrap();
    }

    Ok(())
}

// fn get_sensor_name_from_mqtt_topic(topic: &str) -> Result<String, &'static str> {
//     Ok(topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?.to_string())
// }

#[tokio::main]
async fn main() {
    // pretty_env_logger::init();
    // log::info!("Starting throw dice bot...");

    let bot = Bot::from_env().auto_send();

    let mut mqtt_options = MqttOptions::new("telegram-alarm-bot", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut event_loop) = AsyncClient::new(mqtt_options, 10);
    client.subscribe("zigbee2mqtt/+", QoS::AtMostOnce).await.unwrap();

    while let Ok(notification) = event_loop.poll().await {
        if let Event::Incoming(Packet::Publish(publish)) = notification {
            if let Err(error_str) = process_zigbee2mqtt_publish_notification(publish, &bot).await {
                println!("Error processing zigbee2mqtt publish notification: {}", error_str);
            }
        }
    }

}
