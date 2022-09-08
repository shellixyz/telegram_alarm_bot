use telegram_alarm_bot::SharedState;

mod telegram;
mod mqtt;
mod sensors;
use std::sync::Arc;
use tokio::sync::Mutex;
use telegram_alarm_bot::ProtectedSharedState;

#[tokio::main]
async fn main() {
    pretty_env_logger::formatted_builder().parse_filters("info").init();

    let shared_state = Arc::new(Mutex::new(SharedState::new()));

    let shared_bot = telegram::start_repl(shared_state.clone()).await;

    mqtt::listen(shared_bot, shared_state).await;
}
