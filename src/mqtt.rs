
use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Packet};
use std::time::Duration;

use crate::sensors;
use crate::{ProtectedSharedState, telegram::{SharedBot, self}};

pub async fn listen(shared_bot: SharedBot, shared_state: ProtectedSharedState) {

    let mut mqtt_options = MqttOptions::new("telegram-alarm-bot", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut event_loop) = AsyncClient::new(mqtt_options, 10);
    client.subscribe("zigbee2mqtt/+", QoS::AtMostOnce).await.unwrap();

    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                if let Err(error_str) = process_zigbee2mqtt_publish_notification(publish, &shared_bot, &shared_state).await {
                    println!("Error processing zigbee2mqtt publish notification: {}", error_str);
                }
            },
            Err(mqtt_connection_error) => log::error!("mqtt connection error: {}", mqtt_connection_error),
            _ => {}
        }
    }

}

async fn process_zigbee2mqtt_publish_notification(publish: rumqttc::Publish, shared_bot: &SharedBot, shared_state: &ProtectedSharedState) -> Result<(), &'static str> {

    println!("topic: {}, payload: {:?}", publish.topic, publish.payload);

    let sensor_name = publish.topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?;

    // let sensor_name = get_sensor_name_from_mqtt_topic(&publish.topic).ok_or("Failed to parse topic to get sensor name");
    let sensor = sensors::Sensor::identify(&sensor_name)?;
    let payload_string = String::from_utf8_lossy(&publish.payload).to_string();
    let sensor_data: sensors::Data = serde_json::from_str(&payload_string).map_err(|_| "Failed to parse publish notification payload")?;


    let mut locked_shared_state = shared_state.lock().await;

    if locked_shared_state.notifications_enabled {
        let prev_sensor_data = locked_shared_state.prev_sensors_data.get(sensor_name);

        let prev_sensor_specific_state = match prev_sensor_data {
            Some(prev_sensor_data) => match &prev_sensor_data.specific {
                // Some(prev_spec_data) => Some(&prev_spec_data.value), // XXX should be good but doesn't work for some reason ?!
                Some(prev_spec_data) => match prev_spec_data.value {
                    telegram_alarm_bot::sensors::SpecificStateValue::Contact { contact } => Some(sensors::SpecificStateValue::Contact { contact }),
                    telegram_alarm_bot::sensors::SpecificStateValue::Motion { occupancy } => Some(sensors::SpecificStateValue::Motion { occupancy })
                },
                None => None,
            }
            None => None,
        };

        if let Some(sensor_message) = sensor.message(&sensor_data, &prev_sensor_specific_state)? {
            telegram::shared_bot_send_message(&shared_bot.lock().await, sensor_message.as_str()).await;
        }
    }

    let battery = match sensor_data.get("battery") {
             Some(serde_json::Value::Number(value)) =>
                match value.as_u64() {
                    Some(value) => match u8::try_from(value) {
                        Ok(value) => Some(value),
                        Err(_) => {
                            log::error!("number too large to be represented as u8");
                            None
                        }
                    },
                    None => {
                        log::error!("impossible to get voltage value as u64");
                        None
                    }
                },
                None => None,
             _ => {
                log::error!("got invalid sensor voltage value type");
                None
             }
    };


    let fp_voltage_v = match sensor_data.get("voltage") {
             Some(serde_json::Value::Number(value)) =>
                match value.as_f64() {
                    Some(value) => Some(value as f32 / 1000.0),
                    None => {
                        log::error!("impossible to get voltage value as f64");
                        None
                    }
                },
                None => None,
             _ => {
                log::error!("got invalid sensor voltage value type");
                None
             }
    };

    let sensor_specific_state = match sensor {

        sensors::Sensor::Contact => {
            match sensor_data.get("contact") {
                Some(serde_json::Value::Bool(contact_value)) => Some(telegram_alarm_bot::sensors::SpecificStateValue::Contact { contact: *contact_value }),
                None => None,
                _ => {
                    log::error!("got invalid contact sensor value type");
                    None
                }
            }
        },

        sensors::Sensor::Motion { .. } => {
            match sensor_data.get("occupancy") {
                Some(serde_json::Value::Bool(occupancy_value)) => Some(telegram_alarm_bot::sensors::SpecificStateValue::Motion { occupancy: *occupancy_value }),
                None => None,
                _ => {
                    log::error!("got invalid motion sensor value type");
                    None
                }
            }
        }

    };

    locked_shared_state.prev_sensors_data.insert(sensor_name, battery, fp_voltage_v, sensor_specific_state);

    Ok(())
}