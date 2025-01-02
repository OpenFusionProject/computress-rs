# computress-rs

Discord bot that helps you moderate OpenFusion servers. Operates by listening to the monitor port on your OpenFusion server.

computress-rs is a rewrite of [Computress](https://github.com/OpenFusionProject/Computress) in Rust using [poise](https://github.com/serenity-rs/poise) and [ffmonitor](https://github.com/OpenFusionProject/ffmonitor)

## Usage

First, ensure the monitor is enabled and configured correctly in your OpenFusion server's `config.ini`:
```
...
[monitor]
enabled=true
...
```

Next, fill out `config.json` in this project:
```
{
    "guild_id": <your Discord server's ID>,
    "mod_channel_id": <ID of your moderation/alerts channel>,
    "log_channel_id": <ID of your chat/email log channel>,
    "monitor_address": <IP address and port of your OpenFusion monitor>
}
```

Finally, set the `DISCORD_TOKEN` environment variable to your Discord bot's token (.env file supported!) and run the bot with:
```
cargo run --release
```

## Features
- Show server population in activity message
- Check server status and population with `/check`
- Dump in-game chat and email to a specific text channel
