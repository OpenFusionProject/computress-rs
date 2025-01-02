use std::{process::exit, sync::Arc};

use dotenv::dotenv;
use poise::serenity_prelude::{ClientBuilder, GatewayIntents, Http, User};
use tokio::sync::OnceCell;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
struct Globals {
    bot_user: User,
    http: Arc<Http>,
}

static GLOBALS: OnceCell<Globals> = OnceCell::const_new();

async fn on_init() -> Result<()> {
    let globals = GLOBALS.get().unwrap();

    let bot_user = &globals.bot_user;
    println!(
        "Logged in as {}#{} ({})",
        bot_user.display_name(),
        bot_user.discriminator.map_or(0, |d| d.get()),
        bot_user.id
    );
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
