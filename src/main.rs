
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::signal::unix::{signal,SignalKind};
use teloxide::types::ChatId;
use clap::Parser;
use telegram_alarm_bot::{config,mqtt,sensors,telegram};
use config::Config;
use telegram::SharedBot;
use sensors::PrevSensorsData;
use telegram_alarm_bot::{SharedState,ProtectedSharedState};
use telegram_alarm_bot::log_level::LogLevel;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// don't run bot, only check config file
    #[clap(short, long, action)]
    check_only: bool,

    #[clap(value_parser, default_value_t = String::from("config.json"))]
    config_file: String,

    #[clap(short, long, arg_enum, value_parser)]
    log_level: Option<LogLevel>
}


async fn terminate(source: &str, shared_state: ProtectedSharedState) -> ! {
    log::info!("received {}, terminating", source);

    let locked_shared_data = shared_state.lock().await;

    if let Err(save_error) = locked_shared_data.prev_sensors_data.save_to_file() {
        log::info!("failed to save sensors data to file: {}", save_error);
    }

    std::process::exit(0);
}

async fn notify_start(shared_bot: &SharedBot, notification_chat_ids: &Vec<ChatId>) {
    let locked_bot = shared_bot.lock().await;
    for chat_id in notification_chat_ids {
        telegram::shared_bot_send_message(&locked_bot, chat_id, "Started").await;
    }
}

async fn load_prev_sensors_data(shared_state: &ProtectedSharedState) {
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
}

async fn bot(config: &Config) {
    pretty_env_logger::formatted_builder().parse_filters(config.log_level.to_string().as_str()).init();

    let mut sigterm_stream = signal(SignalKind::terminate()).expect("failed to setup termination handler");

    let shared_state = Arc::new(Mutex::new(SharedState::new()));

    load_prev_sensors_data(&shared_state).await;

    let shared_bot = telegram::start_repl(&config.telegram, shared_state.clone()).await;

    let mut mqtt_event_loop = mqtt::init(&config).await;

    notify_start(&shared_bot, &config.telegram.notification_chat_ids).await;

    loop {
        tokio::select! {
            () = mqtt::handle_events(&mut mqtt_event_loop, &config, &shared_bot, &shared_state) => {},
            Ok(_) = tokio::signal::ctrl_c() => terminate("Ctrl-C", shared_state).await,
            Some(_) = sigterm_stream.recv() => terminate("SIGTERM", shared_state).await
        }
    }
}

fn check_config(config: &Config) {
    println!("Checking config...");
    match config.check() {
        true => println!("OK"),
        false => std::process::exit(1)
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut config = config::Config::load_from_file(&cli.config_file).expect("config load error");

    check_config(&config);

    if let Some(log_level) = cli.log_level {
        config.log_level = log_level;
    }

    if !cli.check_only {
        bot(&config).await;
    }
}
