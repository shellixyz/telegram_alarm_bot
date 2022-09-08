
//const MAISON_ESSERT_CHAT_ID: ChatId = ChatId(-688154163);
const MAISON_ESSERT_CHAT_ID: ChatId = ChatId(554088050);

use teloxide::{prelude::*, dispatching};
use std::sync::Arc;
use tokio::sync::Mutex;
use Sync;
use compound_duration::format_dhms;

use crate::ProtectedSharedState;

pub type SharedBot = Arc<Mutex<AutoSend<Bot>>>;

pub async fn start_repl(shared_state: ProtectedSharedState) -> SharedBot {

    let bot = Bot::from_env().auto_send();
    let shared_bot = Arc::new(Mutex::new(bot.clone()));
    let repl_shared_bot = shared_bot.clone();

    // tokio::spawn(async move {
    tokio::spawn(
        repl_with_deps(bot, repl_shared_bot, shared_state, |message: Message, _bot: AutoSend<Bot>, shared_bot: SharedBot, shared_state: ProtectedSharedState| async move {
            // XXX check message is coming from somewhere we are expecting it to come from (Maison Essert chat for example)
            if let Some(command) = message.text() {
                println!("Got message with text: {:?}", command);
                let locked_bot = shared_bot.lock().await;
                handle_commands(&locked_bot, command, &shared_state).await;
                // if let Err(_) = in_message_tx.send(message_text.to_string()).await {
                //     log::error!("Failed to send in message into channel");
                // }
            }
            respond(())
        }));
    // });

    shared_bot
}

async fn repl_with_deps<'a, R, H, E, D1, D2, Args>(bot: R, dep1: D1, dep2: D2, handler: H)
where
    H: dptree::di::Injectable<DependencyMap, Result<(), E>, Args> + Send + Sync + 'static,
    Result<(), E>: OnError<E>,
    E: std::fmt::Debug + Send + Sync + 'static,
    R: Requester + Send + Sync + Clone + 'static,
    <R as Requester>::GetUpdates: Send,
    D1: Send + Sync + 'static,
    D2: Send + Sync + 'static
{
    let listener = dispatching::update_listeners::polling_default(bot.clone()).await;

    // Other update types are of no interest to use since this REPL is only for
    // messages. See <https://github.com/teloxide/teloxide/issues/557>.
    let ignore_update = |_upd| Box::pin(async {});

    Dispatcher::builder(bot, Update::filter_message().chain(dptree::endpoint(handler)))
        .dependencies(dptree::deps![dep1, dep2])
        .default_handler(ignore_update)
        .build()
        .dispatch_with_listener(
            listener,
            LoggingErrorHandler::with_custom_text("An error from the update listener"),
        )
        .await;
}

pub async fn send_message(bot: &AutoSend<Bot>, message: &str) {
    if let Err(send_error) = bot.send_message(MAISON_ESSERT_CHAT_ID, message).await {
        log::error!("Failed to send notification message: {}", send_error);
    }
}

pub async fn shared_bot_send_message(shared_bot: &tokio::sync::MutexGuard<'_, AutoSend<Bot>>, message: &str) {
    if let Err(send_error) = shared_bot.send_message(MAISON_ESSERT_CHAT_ID, message).await {
        log::error!("Failed to send notification message: {}", send_error);
    }
}

async fn handle_commands(bot: &AutoSend<Bot>, command: &str, shared_data: &ProtectedSharedState) {
    let mut locked_shared_data = shared_data.lock().await;
    match command {

        "/battery" => {
            let battery_info = locked_shared_data.prev_sensors_data.iter().map(|(sensor_name, prev_sensor_data)| {
                let data = &prev_sensor_data.data;
                let battery_str = match data.get("battery") {
                    Some(serde_json::Value::Number(battery_value)) => format!("{}%", battery_value),
                    _ => "unknown".to_owned(),
                };
                let elapsed_since_last_seen = prev_sensor_data.timestamp.elapsed();
                format!("{}: {} (last seen {} ago)", sensor_name, battery_str, format_dhms(elapsed_since_last_seen.as_secs()))
            }).collect::<Vec<String>>().join("\n");
            let message = if battery_info.is_empty() { "No data" } else { battery_info.as_str() };
            send_message(bot, message).await
        },

        "/enable" => {
            locked_shared_data.notifications_enabled = true;
            send_message(bot, "Notifications enabled").await;
        },

        "/disable" => {
            locked_shared_data.notifications_enabled = false;
            send_message(bot, "Notifications disabled").await;
        },

        "/status" => {
            let notifications_status_str = match locked_shared_data.notifications_enabled {
                true => "enabled",
                false => "disabled",
            };
            send_message(bot, format!("Notifications are {}", notifications_status_str).as_str()).await;
        },

        _ => send_message(bot, "Invalid command").await
    }

}
