use lazy_static::lazy_static;
use teloxide::prelude::*;
use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Packet};
use std::collections::HashMap;
use std::time::Duration;
use regex::{Regex, Captures};

fn parse_motion_sensor_name(sensor_name: &str) -> Option<Captures> {
    lazy_static! {
        static ref MOTION_SENSOR_RE: Regex = Regex::new(r"^(?:(\w+) )?[Mm]otion sensor \((\d+)\)$").unwrap();
    }
    MOTION_SENSOR_RE.captures(sensor_name)
}

fn process_door_sensor_notification(_sensor_name: &str, sensor_data: &HashMap<String, serde_json::Value>) -> Result<Option<String>, &'static str> {

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

fn process_motion_sensor_notification(sensor_name: &str, sensor_data: &HashMap<String, serde_json::Value>) -> Result<Option<String>, &'static str> {
    let sensor_name_captures = parse_motion_sensor_name(sensor_name).ok_or("Failed to parse motion sensor name")?;

    let occupancy_value = match sensor_data.get("occupancy") {
        Some(serde_json::Value::Bool(bool_value)) => bool_value,
        None => return Err("Cannot find occupancy value in motion sensor notification payload"),
        _ => return Err("Unexpected value type for motion sensor notification's occupancy value")
    };

    if *occupancy_value {
        let sensor_location = sensor_name_captures.get(1);
        let sensor_id = sensor_name_captures.get(2);

        match (sensor_location, sensor_id) {
            (Some(sensor_location), Some(sensor_id)) =>
                return Ok(Some(format!("Mouvement détecté dans {} (capteur #{})", sensor_location.as_str().to_lowercase(), sensor_id.as_str()))),
            (None, Some(sensor_id)) =>
                return Ok(Some(format!("Mouvement détecté par capteur #{}", sensor_id.as_str()))),
            _ => unreachable!()
        }
    }

    Ok(None)
}

fn process_sensor_notification(sensor_name: &str, sensor_data: &HashMap<String, serde_json::Value>) -> Result<Option<String>, &'static str> {

    match sensor_name {
        "Door opening sensor" => process_door_sensor_notification(sensor_name, sensor_data),
        _                     => process_motion_sensor_notification(sensor_name, sensor_data)
    }
}

fn process_zigbee2mqtt_publish_notification(publish: rumqttc::Publish) -> Result<Option<String>, &'static str> {

    // println!("topic: {}, payload: {:?}", publish.topic, publish.payload);

    let sensor_name = publish.topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?;

    let payload_string = String::from_utf8_lossy(&publish.payload).to_string();
    let payload: HashMap<String, serde_json::Value> = serde_json::from_str(&payload_string).map_err(|_| "Failed to parse publish notification payload")?;

    process_sensor_notification(sensor_name, &payload)
}

#[tokio::main]
async fn main() {
    // pretty_env_logger::init();
    // log::info!("Starting throw dice bot...");

    let bot = Bot::from_env().auto_send();

    let chat_id = ChatId(-688154163);

    let mut mqtt_options = MqttOptions::new("telegram-alarm-bot", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut event_loop) = AsyncClient::new(mqtt_options, 10);
    client.subscribe("zigbee2mqtt/+", QoS::AtMostOnce).await.unwrap();

    while let Ok(notification) = event_loop.poll().await {
        if let Event::Incoming(Packet::Publish(publish)) = notification {
            match process_zigbee2mqtt_publish_notification(publish) {
                Ok(message) => {
                    if message.is_some() {
                        bot.send_message(chat_id, &message.unwrap()).await.unwrap();
                    }
                },
                Err(error_str) => println!("Error processing zigbee2mqtt publish notification: {}", error_str)
            }
        }
    }

}
