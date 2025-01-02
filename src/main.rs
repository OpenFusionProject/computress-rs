mod monitor;

use std::{process::exit, sync::Arc};

use dotenv::dotenv;
use ffmonitor::Monitor;
use poise::serenity_prelude::{ChannelId, ClientBuilder, GatewayIntents, Http, User};
use serde::Deserialize;
use tokio::sync::OnceCell;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize)]
struct Config {
    mod_channel_id: u64,
    log_channel_id: u64,
    monitor_address: String,
}
impl Config {
    fn validate(&self) -> Option<&str> {
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
struct Globals {
    bot_user: User,
    http: Arc<Http>,
    mod_channel: ChannelId,
    log_channel: ChannelId,
    monitor_address: String,
}

static GLOBALS: OnceCell<Globals> = OnceCell::const_new();

async fn send_message(channel_id: ChannelId, message: &str) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    let http = &globals.http;
    channel_id.say(http, message).await?;
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
            commands: vec![],
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                if let Err(e) =
                    poise::builtins::register_globally(ctx, &framework.options().commands).await
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

                GLOBALS
                    .set(Globals {
                        bot_user,
                        http: ctx.http.clone(),
                        mod_channel: ChannelId::new(config.mod_channel_id),
                        log_channel: ChannelId::new(config.log_channel_id),
                        monitor_address: config.monitor_address,
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
