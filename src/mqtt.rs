
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

    let payload_string = String::from_utf8_lossy(&publish.payload).to_string();
    let sensor_data: sensors::Data = serde_json::from_str(&payload_string).map_err(|_| "Failed to parse publish notification payload")?;

    let mut locked_shared_state = shared_state.lock().await;

    if locked_shared_state.notifications_enabled {
        let prev_sensor_data = locked_shared_state.prev_sensors_data.get(sensor_name);

        // let sensor_name = get_sensor_name_from_mqtt_topic(&publish.topic).ok_or("Failed to parse topic to get sensor name");
        let sensor = sensors::Sensor::identify(&sensor_name)?;

        if let Some(sensor_message) = sensor.message(&sensor_data, prev_sensor_data.map(|psd| &psd.data))? {
            telegram::shared_bot_send_message(&shared_bot.lock().await, sensor_message.as_str()).await;
            // if let Err(_) = notification_tx.send(sensor_message).await {
            //     log::error!("Failed to send notification into channel");
            // }
        }
    }

    locked_shared_state.prev_sensors_data.insert(sensor_name, sensor_data);

    Ok(())
}