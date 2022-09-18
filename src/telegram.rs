
use teloxide::{prelude::*, dispatching};
use std::sync::Arc;
use tokio::sync::Mutex;
use Sync;

use crate::ProtectedSharedState;
use crate::config;

pub type SharedBot = Arc<Mutex<AutoSend<Bot>>>;

pub async fn start_repl(config: &config::Telegram, shared_state: ProtectedSharedState) -> SharedBot {

    let bot = Bot::new(&config.token).auto_send();
    let shared_bot = Arc::new(Mutex::new(bot.clone()));
    let repl_shared_bot = shared_bot.clone();

    tokio::spawn(
        repl_with_deps(bot, repl_shared_bot, shared_state, config.valid_chat_ids(), |message: Message, _bot: AutoSend<Bot>, shared_bot: SharedBot, shared_state: ProtectedSharedState, valid_chat_ids: Vec<ChatId>| async move {
            if valid_chat_ids.contains(&message.chat.id) {
                if let Some(command) = message.text() {
                    log::debug!("Got message with text: {:?}", command);
                    let locked_bot = shared_bot.lock().await;
                    handle_commands(&locked_bot, &message.chat.id, command, &shared_state).await;
                }
            }
            respond(())
        })
    );

    shared_bot
}

async fn repl<'a, R, H, E, Args>(bot: R, handler: H)
where
    H: dptree::di::Injectable<DependencyMap, Result<(), E>, Args> + Send + Sync + 'static,
    Result<(), E>: OnError<E>,
    E: std::fmt::Debug + Send + Sync + 'static,
    R: Requester + Send + Sync + Clone + 'static,
    <R as Requester>::GetUpdates: Send,
{
    let listener = dispatching::update_listeners::polling_default(bot.clone()).await;

    // Other update types are of no interest to use since this REPL is only for
    // messages. See <https://github.com/teloxide/teloxide/issues/557>.
    let ignore_update = |_upd| Box::pin(async {});

    Dispatcher::builder(bot, Update::filter_message().chain(dptree::endpoint(handler)))
        .default_handler(ignore_update)
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;
}

async fn repl_with_deps<'a, R, H, E, D1, D2, D3, Args>(bot: R, dep1: D1, dep2: D2, dep3: D3, handler: H)
where
    H: dptree::di::Injectable<DependencyMap, Result<(), E>, Args> + Send + Sync + 'static,
    Result<(), E>: OnError<E>,
    E: std::fmt::Debug + Send + Sync + 'static,
    R: Requester + Send + Sync + Clone + 'static,
    <R as Requester>::GetUpdates: Send,
    D1: Send + Sync + 'static,
    D2: Send + Sync + 'static,
    D3: Send + Sync + 'static
{
    let listener = dispatching::update_listeners::polling_default(bot.clone()).await;

    // Other update types are of no interest to use since this REPL is only for
    // messages. See <https://github.com/teloxide/teloxide/issues/557>.
    let ignore_update = |_upd| Box::pin(async {});

    Dispatcher::builder(bot, Update::filter_message().chain(dptree::endpoint(handler)))
        .dependencies(dptree::deps![dep1, dep2, dep3])
        .default_handler(ignore_update)
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;
}

pub async fn send_message(bot: &AutoSend<Bot>, chat_id: &ChatId, message: &str) {
    let send_message = bot
        .send_message(*chat_id, message)
        .parse_mode(teloxide::types::ParseMode::Html);
    if let Err(send_error) = send_message.await {
        log::error!("Failed to send notification message: {}", send_error);
    }
}

pub async fn shared_bot_send_message(shared_bot: &tokio::sync::MutexGuard<'_, AutoSend<Bot>>, chat_id: &ChatId, message: &str) {
    let send_message = shared_bot
        .send_message(*chat_id, message)
        .parse_mode(teloxide::types::ParseMode::Html);
    if let Err(send_error) = send_message.await {
        log::error!("Failed to send notification message: {}", send_error);
    }
}

async fn handle_commands(bot: &AutoSend<Bot>, chat_id: &ChatId, command: &str, shared_data: &ProtectedSharedState) {
    let mut locked_shared_data = shared_data.lock().await;
    match command {

        "/battery" => {
            let battery_info = locked_shared_data.prev_sensors_data.iter().map(|(_mqtt_topic, prev_sensor_data)| {
                format!("• <b>{}</b>: {} / {} ({})", prev_sensor_data.name, prev_sensor_data.common.battery_value_str(), prev_sensor_data.common.voltage_value_str(), prev_sensor_data.common.time_max_since_last_update_str())
            }).collect::<Vec<String>>().join("\n");
            let message = if battery_info.is_empty() { "No data" } else { battery_info.as_str() };
            send_message(bot, chat_id, message).await
        },

        "/enable" => {
            locked_shared_data.notifications_enabled = true;
            send_message(bot, chat_id, "Notifications enabled").await;
        },

        "/disable" => {
            locked_shared_data.notifications_enabled = false;
            send_message(bot, chat_id, "Notifications disabled").await;
        },

        "/status" => {
            let sensors_info = locked_shared_data.prev_sensors_data.iter().map(|(_mqtt_topic, prev_sensor_data)| {
                format!("• <b>{}</b>: last seen {} ago", prev_sensor_data.name, prev_sensor_data.time_since_last_seen())
            }).collect::<Vec<String>>();

            let sensors_info_str = if sensors_info.is_empty() { "no sensors seen".to_owned() } else { sensors_info.join("\n") };

            let notifications_status_str = match locked_shared_data.notifications_enabled {
                true => "enabled",
                false => "disabled",
            };
            send_message(bot, chat_id, format!("Sensors:\n{}\n\nNotifications are {}", sensors_info_str, notifications_status_str).as_str()).await;
        },

        "/help" => {
            send_message(bot, chat_id, "/enable - enable notifications\n\
                                        /disable - disable notifications\n\
                                        /status - display bot and sensors status\n\
                                        /battery - display latest sensors battery info").await;
        }

        _ => send_message(bot, chat_id, "Invalid command, use /help to display available commands").await
    }

}

pub async fn start_chat_id_discovery(config: &config::Telegram) {
    let bot = Bot::new(&config.token).auto_send();
    tokio::spawn(repl(bot, |message: Message, _bot: AutoSend<Bot>| async move {
        if let Some(message_text) = message.text() {

            let chat_name = if message.chat.is_private() {
                let (first_name, last_name) = (message.chat.first_name(), message.chat.last_name());
                first_name.into_iter().chain(last_name.into_iter()).collect::<Vec<&str>>().join(" ")
            } else {
                message.chat.title().unwrap_or_default().to_owned()
            };

            log::info!("got text message from chat ID {} ({}): {}", message.chat.id, chat_name, message_text);
        }
        respond(())
    }));
}
