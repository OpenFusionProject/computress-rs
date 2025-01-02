mod monitor;

use std::process::exit;

use dotenv::dotenv;
use ffmonitor::Monitor;
use poise::serenity_prelude::{
    ActivityData, ChannelId, ClientBuilder, Context, GatewayIntents, GuildId, User,
};
use serde::Deserialize;
use tokio::sync::{Mutex, OnceCell};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
struct Config {
    guild_id: u64,
    mod_channel_id: u64,
    log_channel_id: u64,
    monitor_address: String,
}
impl Config {
    fn validate(&self) -> Option<&str> {
        if self.guild_id == 0 {
            return Some("guild_id must be set");
        }
        if self.mod_channel_id == 0 {
            return Some("mod_channel_id must be set");
        }
        if self.log_channel_id == 0 {
            return Some("log_channel_id must be set");
        }
        None
    }
}

#[derive(Debug)]
struct State {
    last_player_count: Option<usize>,
}

#[derive(Debug)]
struct Globals {
    bot_user: User,
    context: Context,
    mod_channel: ChannelId,
    log_channel: ChannelId,
    monitor_address: String,
    //
    state: Mutex<State>,
}

static GLOBALS: OnceCell<Globals> = OnceCell::const_new();

async fn set_listening_to(text: &str) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    globals
        .context
        .set_activity(Some(ActivityData::listening(text)));
    Ok(())
}

async fn send_message(channel_id: ChannelId, message: &str) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    let http = &globals.context.http;
    channel_id.say(http, message).await?;
    Ok(())
}

async fn update_status(num_players: Option<usize>) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    let mut state = globals.state.lock().await;
    state.last_player_count = num_players;

    let text = if let Some(num_players) = num_players {
        if num_players == 1 {
            "1 player".to_string()
        } else {
            format!("{} players", num_players)
        }
    } else {
        "nothing".to_string()
    };
    set_listening_to(&text).await?;
    Ok(())
}

async fn on_init() -> Result<()> {
    let globals = GLOBALS.get().unwrap();

    let bot_user = &globals.bot_user;
    println!(
        "Logged in as {}#{} ({})",
        bot_user.display_name(),
        bot_user.discriminator.map_or(0, |d| d.get()),
        bot_user.id
    );

    send_message(globals.mod_channel, "Bot started").await?;
    update_status(None).await?;

    // start ffmonitor
    let rt = tokio::runtime::Handle::current();
    let callback = move |notification| {
        rt.spawn(async move {
            if let Err(e) = monitor::handle_notification(notification).await {
                println!("Error while handling monitor event: {:?}", e);
            }
        });
    };
    if let Err(e) = Monitor::new_with_callback(&globals.monitor_address, Box::new(callback)) {
        return Err(format!("Error preparing ffmonitor: {:?}", e).into());
    }

    Ok(())
}

/// Check the status of the server
#[poise::command(slash_command)]
async fn check(ctx: poise::Context<'_, (), Error>) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    let state = globals.state.lock().await;
    let msg = match state.last_player_count {
        Some(num_players) => {
            let mut s = format!(
                "The server is currently **online** :white_check_mark: with **{}** player",
                num_players
            );
            if num_players != 1 {
                s.push('s');
            }
            s
        }
        None => "The server is currently **offline** :no_entry:".to_string(),
    };
    ctx.say(msg).await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    let dotenv_loaded = dotenv().is_ok();

    // Initialize logging (do this after dotenv so RUST_LOG can be set in there if desired)
    env_logger::init();

    if dotenv_loaded {
        println!("Loaded .env");
    }

    // Load, parse, and validate config
    let Ok(config_file_contents) = std::fs::read_to_string("config.json") else {
        println!("config.json missing");
        exit(1);
    };
    let config: Config = match serde_json::from_str(&config_file_contents) {
        Ok(config) => config,
        Err(e) => {
            println!("Error while parsing config.json: {:?}", e);
            exit(1);
        }
    };
    if let Some(e) = config.validate() {
        println!("Invalid config: {}", e);
        exit(1);
    }

    let Ok(token) = std::env::var("DISCORD_TOKEN") else {
        println!("DISCORD_TOKEN environment variable missing");
        exit(1);
    };

    let intents = GatewayIntents::non_privileged();
    let framework: poise::Framework<(), Error> = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![check()],
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                let guild_id = GuildId::new(config.guild_id);
                if let Err(e) =
                    poise::builtins::register_in_guild(ctx, &framework.options().commands, guild_id)
                        .await
                {
                    println!("Error while registering commands: {:?}", e);
                };

                let bot_user: User = match ctx.http.get_current_user().await {
                    Ok(user) => user.into(),
                    Err(e) => {
                        println!("Error while getting current user: {:?}", e);
                        exit(1);
                    }
                };

                let state = State {
                    last_player_count: None,
                };

                GLOBALS
                    .set(Globals {
                        bot_user,
                        context: ctx.clone(),
                        mod_channel: ChannelId::new(config.mod_channel_id),
                        log_channel: ChannelId::new(config.log_channel_id),
                        monitor_address: config.monitor_address,
                        //
                        state: Mutex::new(state),
                    })
                    .unwrap();

                if let Err(e) = on_init().await {
                    println!("Error while initializing: {:?}", e);
                    exit(1);
                }
                Ok(())
            })
        })
        .build();

    let mut client = match ClientBuilder::new(token, intents)
        .framework(framework)
        .await
    {
        Ok(client) => client,
        Err(e) => {
            println!("Couldn't build client: {:?}", e);
            exit(1);
        }
    };

    let res = client.start().await;
    if let Err(e) = res {
        println!("Client error: {:?}", e);
    }
}
