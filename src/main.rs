use teloxide::prelude::*;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log::info!("Starting throw dice bot...");

    let bot = Bot::from_env().auto_send();

    let chat_id = ChatId(-688154163);

    bot.send_message(chat_id, "coucou").await.unwrap();

    // teloxide::repl(bot, |message: Message, bot: AutoSend<Bot>| async move {
    //     println!("chat id: {}", message.chat.id.0);
    //     if message.chat.title().unwrap_or_default() == "Maison Essert" {
    //         bot.send_message(message.chat.id, "coucou").await?;

    //     } else {
    //         println!("Group \"{}\" tried to add bot", message.chat.title().unwrap());
    //         bot.send_message(message.chat.id, "This is a private bot, you cannot add it to your group").await?;
    //         bot.leave_chat(message.chat.id).await?;
    //     }

    //     // bot.send_dice(message.chat.id).await?;
    //     respond(())
    // })
    // .await;
}
