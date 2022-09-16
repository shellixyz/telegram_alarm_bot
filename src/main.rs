
use std::path::Path;
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
    /// Do not run bot, only check config file
    #[clap(short, long, action)]
    check_only: bool,

    /// Start telegram chat ID discovery mode
    #[clap(short = 'i', long, action)]
    chat_id_discovery: bool,

    /// Start in test mode. The notifications are sent in the admin chats instead of notification chats
    #[clap(short, long, action)]
    test_mode: bool,

    #[clap(value_parser, default_value_t = String::from("config.json"))]
    config_file: String,

    /// The default level if not specified in the config file is "info"
    #[clap(short, long, arg_enum, value_parser)]
    log_level: Option<LogLevel>
}


async fn terminate(source: &str, shared_state: ProtectedSharedState, config: &Config) -> ! {
    log::info!("received {}, terminating", source);

    let locked_shared_data = shared_state.lock().await;

    if let Err(save_error) = locked_shared_data.prev_sensors_data.save_to_file(&config.sensors_data_file) {
        log::info!("failed to save sensors data to file: {}", save_error);
    }

    std::process::exit(0);
}

async fn notify_start(shared_bot: &SharedBot, notification_chat_ids: &Vec<ChatId>) {
    log::info!("bot started");
    let locked_bot = shared_bot.lock().await;
    for chat_id in notification_chat_ids {
        telegram::shared_bot_send_message(&locked_bot, chat_id, "Started").await;
    }
}

async fn load_prev_sensors_data<S: AsRef<Path> + std::fmt::Debug>(prev_sensors_data_file_path: S, shared_state: &ProtectedSharedState) {
    match PrevSensorsData::load_from_file(&prev_sensors_data_file_path) {
        Ok(prev_sensors_data_from_file) => {
            let mut shared_state_locked = shared_state.lock().await;
            log::info!("loaded prev sensors data from file {:?}", prev_sensors_data_file_path);
            shared_state_locked.prev_sensors_data = prev_sensors_data_from_file;
        },
        Err(sensors::DataFileLoadError::IOError(load_io_error)) if load_io_error.kind() == std::io::ErrorKind::NotFound =>
            log::info!("prev sensors data file {:?} does not exist", prev_sensors_data_file_path),
        Err(load_error) => {
            log::error!("prev sensors data load error: {}", load_error);
        }
    };
}

async fn bot(config: &Config) {
    pretty_env_logger::formatted_builder().parse_filters(config.log_level.to_string().as_str()).init();

    let mut sigterm_stream = signal(SignalKind::terminate()).expect("failed to setup termination handler");

    let shared_state = Arc::new(Mutex::new(SharedState::new()));

    load_prev_sensors_data(&config.sensors_data_file, &shared_state).await;

    let shared_bot = telegram::start_repl(&config.telegram, shared_state.clone()).await;

    let mut mqtt_event_loop = mqtt::init(&config).await;

    notify_start(&shared_bot, &config.telegram.notification_chat_ids).await;

    loop {
        tokio::select! {
            () = mqtt::handle_events(&mut mqtt_event_loop, &config, &shared_bot, &shared_state) => {},
            Ok(_) = tokio::signal::ctrl_c() => terminate("Ctrl-C", shared_state, &config).await,
            Some(_) = sigterm_stream.recv() => terminate("SIGTERM", shared_state, &config).await
        }
    }
}

async fn chat_id_discovery(config: &config::Telegram) {
    pretty_env_logger::formatted_builder().parse_filters("info").init();
    log::info!("Started bot in Chat ID discovery mode");
    telegram::start_chat_id_discovery(config).await;
    tokio::signal::ctrl_c().await.expect("failed to setup Ctrl-C handler");
}

fn check_config(config: &Config, check_only: &bool) {

    if *check_only { println!("Checking config...") }

    match config.check() {
        true => if *check_only { println!("OK") },
        false => std::process::exit(1)
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut config = config::Config::load_from_file(&cli.config_file).expect("config load error");

    check_config(&config, &cli.check_only);

    if cli.chat_id_discovery {
        chat_id_discovery(&config.telegram).await;
    } else {
        if let Some(log_level) = cli.log_level {
            config.log_level = log_level;
        }

        if cli.test_mode {
            if let Some(admin_chat_ids) = &config.telegram.admin_chat_ids {
                config.telegram.notification_chat_ids = admin_chat_ids.clone();
            } else {
                eprintln!("Error: admin chat IDs have not been defined");
                std::process::exit(1);
            }
        }

        if !cli.check_only {
            bot(&config).await;
        }
    }
}
