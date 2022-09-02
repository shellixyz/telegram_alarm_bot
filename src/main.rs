use lazy_static::lazy_static;
use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Packet};
use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use regex::Regex;
use std::sync::{Mutex, Arc};
use teloxide::{prelude::*, dispatching};
use std::future::Future;


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

    pub fn message(&self, sensor_data: &SensorData, prev_sensor_data: &Option<&SensorData>) -> Result<Option<String>, &'static str> {
        match self {
            Sensor::Contact => Self::contact_sensor_message(sensor_data, prev_sensor_data),
            Sensor::Motion { .. } => self.motion_sensor_message(sensor_data, prev_sensor_data)
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

async fn process_zigbee2mqtt_publish_notification(publish: rumqttc::Publish, prev_sensors_data: &mut PrevSensorData, notification_messages_storage: Arc<Mutex<VecDeque<String>>>) -> Result<(), &'static str> {

    // println!("topic: {}, payload: {:?}", publish.topic, publish.payload);

    let sensor_name = publish.topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?;

    let payload_string = String::from_utf8_lossy(&publish.payload).to_string();
    let sensor_data: SensorData = serde_json::from_str(&payload_string).map_err(|_| "Failed to parse publish notification payload")?;

    let prev_sensor_data = prev_sensors_data.get(sensor_name);

    // let sensor_name = get_sensor_name_from_mqtt_topic(&publish.topic).ok_or("Failed to parse topic to get sensor name");
    let sensor = Sensor::identify(&sensor_name)?;

    if let Some(sensor_message) = sensor.message(&sensor_data, &prev_sensor_data)? {
        // bot.send_message(MAISON_ESSERT_CHAT_ID, &sensor_message).await.map_err(|_| "Failed to send message")?;
        notification_messages_storage.lock().unwrap().push_front(sensor_message.to_string());
    }

    prev_sensors_data.insert(sensor_name.to_string(), sensor_data);

    Ok(())
}

// fn get_sensor_name_from_mqtt_topic(topic: &str) -> Result<String, &'static str> {
//     Ok(topic.split('/').nth(1).ok_or("Failed to parse topic to get sensor name")?.to_string())
// }

type SensorName = String;
type PrevSensorData = HashMap<SensorName, SensorData>;

fn handle_bot_incoming_messages(bot: AutoSend<Bot>, incoming_messages_storage: Arc<Mutex<VecDeque<String>>>) -> impl Future<Output = ()> {
    repl_with_dep(bot, incoming_messages_storage, |message: Message, _bot: AutoSend<Bot>, incoming_messages_storage: Arc<Mutex<VecDeque<String>>>| async move {
        if let Some(message_text) = message.text() {
            println!("Got message with text: {:?}", message_text);
            incoming_messages_storage.lock().unwrap().push_front(message_text.to_string());
        }
        respond(())
    })
}

async fn handle_bot_outgoing_messages(bot: AutoSend<Bot>, incoming_messages_storage: Arc<Mutex<VecDeque<String>>>, notification_messages_storage: Arc<Mutex<VecDeque<String>>>) {
    if let Some(notification_message) = notification_messages_storage.lock().unwrap().pop_back() {
        if let Err(send_error) = bot.send_message(MAISON_ESSERT_CHAT_ID, &notification_message).await {
            notification_messages_storage.lock().unwrap().push_back(notification_message);
            log::error!("Failed to send notification message: {}", send_error);
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::formatted_builder().parse_filters("info").init();

    let incoming_messages_storage: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let notification_messages_storage: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let mut prev_sensors_data = PrevSensorData::new();

    let bot = Bot::from_env().auto_send();

    let mut mqtt_options = MqttOptions::new("telegram-alarm-bot", "localhost", 1883);
    mqtt_options.set_keep_alive(Duration::from_secs(5));

    let (client, mut event_loop) = AsyncClient::new(mqtt_options, 10);
    client.subscribe("zigbee2mqtt/+", QoS::AtMostOnce).await.unwrap();

    log::info!("Started Telegram alarm bot");

    // while let Ok(notification) = event_loop.poll().await {
    //     if let Event::Incoming(Packet::Publish(publish)) = notification {
    //         if let Err(error_str) = process_zigbee2mqtt_publish_notification(publish, &mut prev_sensors_data, &bot).await {
    //             println!("Error processing zigbee2mqtt publish notification: {}", error_str);
    //         }
    //     }
    // }

    tokio::spawn(handle_bot_incoming_messages(bot.clone(), incoming_messages_storage.clone()));
    tokio::spawn(handle_bot_outgoing_messages(bot.clone(), incoming_messages_storage.clone(), notification_messages_storage.clone()));

    tokio::select! {

        Ok(Event::Incoming(Packet::Publish(publish))) = event_loop.poll() => {
            if let Err(error_str) = process_zigbee2mqtt_publish_notification(publish, &mut prev_sensors_data, notification_messages_storage.clone()).await {
                println!("Error processing zigbee2mqtt publish notification: {}", error_str);
            }
        },

        // () = handle_bot_incoming_messages(bot.clone(), message_storage.clone()) => {}

    }

}
