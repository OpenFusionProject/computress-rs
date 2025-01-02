use std::{process::exit, sync::Arc};

use dotenv::dotenv;
use poise::serenity_prelude::{ClientBuilder, GatewayIntents, Http};
use tokio::sync::OnceCell;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;
type Context<'a> = poise::Context<'a, (), Error>;

#[derive(Debug)]
struct Globals {
    http: Arc<Http>,
}

static GLOBALS: OnceCell<Globals> = OnceCell::const_new();

#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    let dotenv_loaded = dotenv().is_ok();

    // Initialize logging (do this after dotenv so RUST_LOG can be set in there if desired)
    env_logger::init();

    if dotenv_loaded {
        println!("Loaded .env");
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
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                let bot_user = ctx.http.get_current_user().await?;
                println!(
                    "Logged in as {}#{} ({})",
                    bot_user.display_name(),
                    bot_user.discriminator.map_or(0, |d| d.get()),
                    bot_user.id
                );

                GLOBALS
                    .set(Globals {
                        http: ctx.http.clone(),
                    })
                    .unwrap();
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
