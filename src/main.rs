use telegram_alarm_bot::{SharedState, sensors::PrevSensorsData};

pub mod telegram;
pub mod mqtt;
pub mod sensors;
use std::sync::Arc;
use tokio::sync::Mutex;
use telegram_alarm_bot::ProtectedSharedState;


async fn terminate(source: &str, shared_state: Arc<Mutex<SharedState>>) -> ! {
    log::info!("received {}, terminating", source);

    let locked_shared_data = shared_state.lock().await;

    if let Err(save_error) = locked_shared_data.prev_sensors_data.save_to_file() {
        log::info!("failed to save sensors data to file: {}", save_error);
    }

    std::process::exit(0);
}

#[tokio::main]
async fn main() {
    pretty_env_logger::formatted_builder().parse_filters("info").init();

    let mut sigterm_stream = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("failed to setup termination handler");

    let shared_state = Arc::new(Mutex::new(SharedState::new()));

    match PrevSensorsData::load_from_file() {
        Ok(prev_sensors_data_from_file) => {
            let mut shared_state_locked = shared_state.lock().await;
            log::info!("loaded prev sensors data");
            shared_state_locked.prev_sensors_data = prev_sensors_data_from_file;
        },
        Err(load_error) => {
            log::error!("prev sensors data load error: {}", load_error);
        }
    };

    let shared_bot = telegram::start_repl(shared_state.clone()).await;

    let mut mqtt_event_loop = mqtt::init().await;

    loop {
        tokio::select! {
            () = mqtt::handle_events(&mut mqtt_event_loop, &shared_bot, &shared_state) => {},
            Ok(_) = tokio::signal::ctrl_c() => terminate("Ctrl-C", shared_state).await,
            Some(_) = sigterm_stream.recv() => terminate("SIGTERM", shared_state).await
        }
    }
}
