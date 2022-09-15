# Telegram alarm bot

This is a small bot listening to MQTT events and publishing notifications on the Telegram instant messenging service

## How to start

1. Start by making a copy of the config.json.template file to config.json then edit the file
2. Change the MQTT host/port and set the Telegam token to a valid one
3. Start the bot with `-i` to start in chat ID discovery mode
4. Send a message from Telegram to the bot from the various groups/chats you want the bot to talk into. Two different kind of chats can be configured: admin and notifications. Only the chat from which the IDs are set into the config key "notification_chat_ids" receive notifications. The chats from which the IDs are set into the config key "admin_chat_ids" can be used to send commands to the bot but will not receive notifications.
5. Define MQTT topics / sensor names / event messages in the config.json file like in the template
6. You can now start the bot without arguments. By default the notification are disabled on startup.

## Bot commands

### /enable

Enables the notifications. A confirmation message is sent to the chat in which the command was sent. 

### /disable

Disables the notifications. A confirmation message is sent to the chat in which the command was sent.

### /status

Lists the sensors which the bot has received notifications for with the time they have since been seen. Also displays whether the notifications are enabled or not

### /battery

Displays the known remaining battery percentage, battery voltage and time of last update for each sensor
