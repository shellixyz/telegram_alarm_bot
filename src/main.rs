use lazy_static::lazy_static;
use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Packet};
use std::{collections::HashMap, sync::Arc};
use std::time::Duration;
use regex::Regex;
// use std::sync::{Mutex, Arc};
use tokio::sync::{mpsc, Mutex};
use teloxide::{prelude::*, dispatching};
use std::future::Future;
use std::time;


type SensorData = HashMap<String, serde_json::Value>;

//const MAISON_ESSERT_CHAT_ID: ChatId = ChatId(-688154163);
const MAISON_ESSERT_CHAT_ID: ChatId = ChatId(554088050);

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

    pub fn message(&self, sensor_data: &SensorData, prev_sensor_data: Option<&SensorData>) -> Result<Option<String>, &'static str> {
        match self {
            Sensor::Contact => Self::contact_sensor_message(sensor_data, &prev_sensor_data),
            Sensor::Motion { .. } => self.motion_sensor_message(sensor_data, &prev_sensor_data)
        }
    }

    fn contact_sensor_message(sensor_data: &SensorData, prev_sensor_data: &Option<&SensorData>) -> Result<Option<String>, &'static str> {

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

    fn motion_sensor_message(&self, sensor_data: &SensorData, prev_sensor_data: &Option<&SensorData>) -> Result<Option<String>, &'static str> {
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

pub async fn repl_with_dep<'a, R, H, E, X, Args>(bot: R, dep: X, handler: H)
where
    H: dptree::di::Injectable<DependencyMap, Result<(), E>, Args> + Send + Sync + 'static,
    Result<(), E>: OnError<E>,
    E: std::fmt::Debug + Send + Sync + 'static,
    R: Requester + Send + Sync + Clone + 'static,
    <R as Requester>::GetUpdates: Send,
    X: Send + Sync + 'static
{
    let listener = dispatching::update_listeners::polling_default(bot.clone()).await;

    // Other update types are of no interest to use since this REPL is only for
    // messages. See <https://github.com/teloxide/teloxide/issues/557>.
    let ignore_update = |_upd| Box::pin(async {});

    Dispatcher::builder(bot, Update::filter_message().chain(dptree::endpoint(handler)))
        .dependencies(dptree::deps![dep])
        .default_handler(ignore_update)
        .enable_ctrlc_handler()
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;
}

async fn process_zigbee2mqtt_publish_notification(publish: rumqttc::Publish, prev_sensors_data: &Arc<Mutex<PrevSensorHM>>, notification_tx: mpsc::Sender<String>) -> Result<(), &'static str> {

    println!("topic: {}, payload: {:?}", publish.topic, publish.payload);

    let sensor_name = publish.topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?;

    let payload_string = String::from_utf8_lossy(&publish.payload).to_string();
    let sensor_data: SensorData = serde_json::from_str(&payload_string).map_err(|_| "Failed to parse publish notification payload")?;

    let mut locked_prev_sensors_data = prev_sensors_data.lock().await;
    let prev_sensor_data = locked_prev_sensors_data.get(sensor_name);

    // let sensor_name = get_sensor_name_from_mqtt_topic(&publish.topic).ok_or("Failed to parse topic to get sensor name");
    let sensor = Sensor::identify(&sensor_name)?;

    if let Some(sensor_message) = sensor.message(&sensor_data, prev_sensor_data.map(|psd| &psd.data))? {
        if let Err(_) = notification_tx.send(sensor_message).await {
            log::error!("Failed to send notification into channel");
        }
    }

    locked_prev_sensors_data.insert(sensor_name.to_string(), PrevSensorData::new(sensor_data));

    Ok(())
}

// fn get_sensor_name_from_mqtt_topic(topic: &str) -> Result<String, &'static str> {
//     Ok(topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?.to_string())
// }

type SensorName = String;
type PrevSensorHM = HashMap<SensorName, PrevSensorData>;

struct PrevSensorData {
    timestamp: time::Instant,
    data: SensorData
}

impl PrevSensorData {
    fn new(sensor_data: SensorData) -> Self {
        Self {
            timestamp: time::Instant::now(),
            data: sensor_data
        }
    }
}


fn handle_bot_incoming_messages(bot: AutoSend<Bot>, in_message_tx: mpsc::Sender<String>) -> impl Future<Output = ()> {
    repl_with_dep(bot, in_message_tx, |message: Message, _bot: AutoSend<Bot>, in_message_tx: mpsc::Sender<String>| async move {
        // XXX check message is coming from somewhere we are expecting it to come from (Maison Essert chat for example)
        if let Some(message_text) = message.text() {
            println!("Got message with text: {:?}", message_text);
            if let Err(_) = in_message_tx.send(message_text.to_string()).await {
                log::error!("Failed to send in message into channel");
            }
        }
        respond(())
    })
}

async fn bot_send_message(bot: &AutoSend<Bot>, message: &str) {
    if let Err(send_error) = bot.send_message(MAISON_ESSERT_CHAT_ID, message).await {
        log::error!("Failed to send notification message: {}", send_error);
    }
}

async fn reply_to_command(bot: &AutoSend<Bot>, command: &str, prev_sensors_data: &Arc<Mutex<PrevSensorHM>>) {
    let locked_prev_sensors_data = prev_sensors_data.lock().await;
    match command {
        "/battery" => {
            // for (sensor_name, prev_sensor_data) in locked_prev_sensors_data.iter() {
            // }
            let battery_info = locked_prev_sensors_data.iter().map(|(sensor_name, prev_sensor_data)| {
                let data = &prev_sensor_data.data;
                let battery_str = match data.get("battery") {
                    Some(serde_json::Value::Number(battery_value)) => format!("{}%", battery_value),
                    _ => "unknown".to_owned(),
                };
                format!("{}: {} (last seen: {:?})", sensor_name, battery_str, &prev_sensor_data.timestamp)
            }).collect::<Vec<String>>().join("\n");
            bot_send_message(&bot, &battery_info).await
        },
        _ => {}
    }

}

async fn handle_bot_outgoing_messages(bot: AutoSend<Bot>, mut in_message_rx: mpsc::Receiver<String>, mut notification_rx: mpsc::Receiver<String>, prev_sensors_data: Arc<Mutex<PrevSensorHM>>) {
    loop {
        tokio::select! {
            Some(in_message) = in_message_rx.recv() => {
                reply_to_command(&bot, &in_message, &prev_sensors_data).await;
            },
            Some(notification) = notification_rx.recv() => bot_send_message(&bot, &notification).await
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::formatted_builder().parse_filters("info").init();

    let (in_message_tx, in_message_rx) = mpsc::channel(100);
    let (notification_tx, notification_rx) = mpsc::channel(100);
    let prev_sensors_data = Arc::new(Mutex::new(PrevSensorHM::new()));
    // let mut alarm_enabled = false;

    let bot = Bot::from_env().auto_send();

    let mut mqtt_options = MqttOptions::new("telegram-alarm-bot", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut event_loop) = AsyncClient::new(mqtt_options, 10);
    client.subscribe("zigbee2mqtt/+", QoS::AtMostOnce).await.unwrap();

    log::info!("Started Telegram alarm bot");

    tokio::spawn(handle_bot_incoming_messages(bot.clone(), in_message_tx));
    tokio::spawn(handle_bot_outgoing_messages(bot.clone(), in_message_rx, notification_rx, prev_sensors_data.clone()));

    loop {
        match event_loop.poll().await {
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                if let Err(error_str) = process_zigbee2mqtt_publish_notification(publish, &prev_sensors_data, notification_tx.clone()).await {
                    println!("Error processing zigbee2mqtt publish notification: {}", error_str);
                }
            },
            Err(mqtt_connection_error) => log::error!("mqtt connection error: {}", mqtt_connection_error),
            _ => {}
        }
    }
}
