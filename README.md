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
    "mod_role_ids": [<IDs of your moderator roles for privileges, comma-separated>],
    "mod_channel_id": <ID of your moderation/alerts channel>,
    "log_channel_id": <ID of your chat/email log channel>,
    "name_approvals_channel_id": <ID of your name approval requests channel>,
    "monitor_address": <IP address and port of your OpenFusion monitor>,
    "ofapi_endpoint": <address of your ofapi endpoint> 
}
```

Finally, set the `DISCORD_TOKEN` environment variable to your Discord bot's token (.env file supported!) and run the bot with:
```
cargo run --release [path to config.json]
```

## Features
- Show server population in activity message
- Check server status and population with `/check`
- Dump in-game chat and email to a specific text channel
- Send name requests into a specific text channel (only moderators can interact)
- Check for outstanding name requests with `/namereqs`
