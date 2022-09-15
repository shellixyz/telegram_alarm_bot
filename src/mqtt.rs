
use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Packet, EventLoop};
use thiserror::Error;

use crate::config;

use crate::sensors;
use crate::config::Config;
use crate::{ProtectedSharedState, telegram::{SharedBot, self}};

pub async fn init(config: &Config) -> EventLoop {
    let mut mqtt_options = MqttOptions::new("telegram-alarm-bot", "localhost", 1883);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    let (client, event_loop) = AsyncClient::new(mqtt_options, 10);

    for subscribe_pattern in config.mqtt_subscribe_patterns() {
        client.subscribe(subscribe_pattern, QoS::AtMostOnce).await.unwrap();
    }

    event_loop
}

pub async fn handle_events(event_loop: &mut EventLoop, config: &Config, shared_bot: &SharedBot, shared_state: &ProtectedSharedState) {

    match event_loop.poll().await {
        Ok(Event::Incoming(Packet::Publish(publish))) => {
            if let Err(error_str) = process_publish_notification(publish, config, &shared_bot, &shared_state).await {
                log::error!("Error processing publish notification: {}", error_str);
            }
        },
        Err(mqtt_connection_error) => log::error!("mqtt connection error: {}", mqtt_connection_error),
        _ => {}
    }

}

async fn update_prev_sensor_data(shared_state: &ProtectedSharedState, topic: &String, sensor_name: &String, sensor_payload_field_names_and_state_messages: &config::SensorPayloadFieldNameAndStateMessages, sensor_data: &sensors::Data) {
    let mut locked_shared_state = shared_state.lock().await;

    let prev_sensor_data_entry = locked_shared_state.prev_sensors_data.entry(topic.clone());

    let prev_sensor_data = match prev_sensor_data_entry {
        std::collections::hash_map::Entry::Occupied(entry) => {
            let data = entry.into_mut();
            data.last_seen_now();
            data
        },
        std::collections::hash_map::Entry::Vacant(entry) =>
            entry.insert(sensors::PrevData::new(sensor_name.clone()))
    };

    for field_name in sensor_payload_field_names_and_state_messages.payload_field_names() {
        if let Some(field_value) = sensor_data.get(field_name) {
            prev_sensor_data.trigger_states.insert(field_name.clone(), field_value.clone());
        }
    }

    match sensor_data.get("battery") {
            Some(serde_json::Value::Number(battery)) =>
                match battery.as_u64() {
                    Some(battery) => match u8::try_from(battery) {
                        Ok(battery) => prev_sensor_data.update_battery(battery),
                        Err(_) => log::error!("number too large to be represented as u8")
                    },
                    None => log::error!("impossible to get voltage value as u64")
                },
                None => {},
                _ => log::error!("got invalid sensor voltage value type")
    };


    match sensor_data.get("voltage") {
        Some(serde_json::Value::Number(voltage)) =>
            match voltage.as_f64() {
                Some(voltage) => prev_sensor_data.update_voltage(voltage as f32 / 1000.0),
                None => log::error!("impossible to get voltage value as f64")
            },
            None => {},
        _ => log::error!("got invalid sensor voltage value type")
    };
}


#[derive(Debug, Error)]
pub enum PublishNotificationProcessingError {
    #[error("regex error")]
    RegexError(regex::Error),
    #[error("deserialization error")]
    DeserializationError(serde_json::Error)
}

async fn process_publish_notification(publish: rumqttc::Publish, config: &Config, shared_bot: &SharedBot, shared_state: &ProtectedSharedState) -> Result<(), PublishNotificationProcessingError> {

    log::debug!("got mqtt pushblish notification - topic: {}, payload: {:?}", publish.topic, publish.payload);

    let payload_string = String::from_utf8_lossy(&publish.payload).to_string();
    let sensor_data: sensors::Data = serde_json::from_str(&payload_string).map_err(|deser_err| PublishNotificationProcessingError::DeserializationError(deser_err))?;

    if let Some((sensor_name, sensor_name_captures, sensor_payload_field_names_and_state_messages)) =
            config.mqtt_topics.match_topic(&publish.topic).map_err(|re_error| PublishNotificationProcessingError::RegexError(re_error))? {
        for (sensor_field_name, state_messages) in sensor_payload_field_names_and_state_messages.iter() {
            if let Some(sensor_value) = sensor_data.get(sensor_field_name) {
                if let Some(message_template) = state_messages.get(sensor_value.to_string().as_str()) {

                    let locked_shared_state = shared_state.lock().await;
                    let prev_sensor_data = locked_shared_state.prev_sensors_data.get(&publish.topic);

                    let prev_value = prev_sensor_data.and_then(|psd| psd.trigger_states.get(sensor_field_name));

                    if locked_shared_state.notifications_enabled && (prev_value.is_none() || sensor_value != prev_value.unwrap()) {

                        let mut message = message_template.clone();
                        for (cname, cstr) in &sensor_name_captures {
                            if let Some(cstr) = cstr {
                                message.replace_range(0.., message.replace(format!("{{{cname}}}").as_str(), &cstr).as_str());
                            }
                        }

                        for chat_id in &config.telegram.notification_chat_ids {
                            telegram::shared_bot_send_message(&shared_bot.lock().await, chat_id, message.as_str()).await;
                        }

                    }
                }
            }
        }

        update_prev_sensor_data(shared_state, &publish.topic, &sensor_name, sensor_payload_field_names_and_state_messages, &sensor_data).await;

    }

    Ok(())
}
