mod endpoint;
mod monitor;
mod util;

use std::{
    collections::HashSet,
    env::args,
    process::exit,
    sync::{Arc, LazyLock},
    time::Duration,
};

use dotenv::dotenv;
use ffmonitor::{Monitor, NameRequestEvent};
use poise::{
    futures_util::StreamExt as _,
    serenity_prelude::{
        ActivityData, ChannelId, ClientBuilder, ComponentInteraction,
        ComponentInteractionCollector, Context, CreateActionRow, CreateAllowedMentions,
        CreateButton, CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage,
        FullEvent, GatewayIntents, GuildId, Http, Mention, RoleId, User,
    },
    CreateReply,
};
use regex::Regex;
use serde::Deserialize;
use tokio::sync::{Mutex, Notify, OnceCell, RwLock};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, Error>;

const NAME_REQUEST_PATTERN: &str = r"^Name request from Player (\d+): \*\*(.+)\*\*$";
static NAME_REQUEST_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(NAME_REQUEST_PATTERN).unwrap());

#[derive(Debug, Deserialize)]
struct Config {
    guild_id: u64,
    mod_role_id: Option<u64>,
    mod_role_ids: Option<Vec<u64>>,
    mod_channel_id: u64,
    log_channel_id: u64,
    name_approvals_channel_id: u64,
    monitor_address: String,
    ofapi_endpoint: String,
    search_query_string: Option<String>,
}
impl Config {
    fn validate(&self) -> Option<&str> {
        if self.guild_id == 0 {
            return Some("guild_id must be set");
        }
        if self.mod_role_id.is_none_or(|id| id == 0)
            && self.mod_role_ids.as_ref().is_none_or(|ids| ids.is_empty())
        {
            return Some("Either mod_role_id or mod_role_ids must be set");
        }
        if self.mod_channel_id == 0 {
            return Some("mod_channel_id must be set");
        }
        None
    }

    fn get_mod_role_ids(&self) -> HashSet<RoleId> {
        let mut set = HashSet::new();
        if let Some(id) = self.mod_role_id {
            set.insert(RoleId::new(id));
        }
        if let Some(ids) = &self.mod_role_ids {
            for id in ids {
                set.insert(RoleId::new(*id));
            }
        }
        set
    }
}

#[derive(Debug)]
struct State {
    last_player_count: Option<usize>,
}

#[derive(Debug)]
struct Globals {
    bot_user: User,
    context: RwLock<Context>,
    mod_roles: HashSet<RoleId>,
    mod_channel: ChannelId,
    log_channel: Option<ChannelId>,
    name_approvals_channel: Option<ChannelId>,
    monitor_address: String,
    ofapi_endpoint: String,
    search_query_string: Option<String>,
    //
    state: Mutex<State>,
    reconnect_notification: Notify,
}

#[derive(Debug, Deserialize)]
struct NameRequest {
    player_uid: u64,
    requested_name: String,
}
impl NameRequest {
    // we can't rely on state to hold the name request, so reconstruct it from the notification we sent
    fn parse_from_notification_message(msg: &str) -> Result<NameRequest> {
        let captures = NAME_REQUEST_REGEX.captures(msg).ok_or("Malformed")?;
        let player_uid = captures[1].parse::<u64>()?;
        let requested_name = captures[2].to_string();
        let req = NameRequest {
            player_uid,
            requested_name,
        };
        Ok(req)
    }
}
impl From<NameRequestEvent> for NameRequest {
    fn from(value: NameRequestEvent) -> Self {
        Self {
            player_uid: value.player_uid,
            requested_name: value.requested_name,
        }
    }
}

static GLOBALS: OnceCell<Globals> = OnceCell::const_new();

async fn get_http() -> Arc<Http> {
    let globals = GLOBALS.get().unwrap();
    let context = globals.context.read().await;
    context.http.clone()
}

async fn set_listening_to(text: &str) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    globals
        .context
        .read()
        .await
        .set_activity(Some(ActivityData::listening(text)));
    Ok(())
}

async fn send_message(channel_id: ChannelId, message: &str) -> Result<()> {
    let http = get_http().await;
    channel_id.say(http, message).await?;
    Ok(())
}

async fn send_message_with_buttons(
    channel_id: ChannelId,
    message: &str,
    buttons: Vec<CreateButton>,
) -> Result<()> {
    let components = vec![CreateActionRow::Buttons(buttons)];
    let http = get_http().await;
    channel_id
        .send_message(
            http,
            CreateMessage::default()
                .content(message)
                .components(components),
        )
        .await?;
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

async fn handle_namereq_approve(interaction: &ComponentInteraction) -> Result<()> {
    let msg = interaction.message.content.clone();
    let user = &interaction.member.as_ref().unwrap().user;
    let by = user.tag();

    let namereq = NameRequest::parse_from_notification_message(&msg)?;
    let updated = endpoint::send_name_request_decision(&namereq, "approved", &by).await?;

    // Try to delete the initial message
    let _ = interaction.message.delete(get_http().await).await;

    if !updated {
        interaction
            .create_response(
                get_http().await,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::default()
                        .ephemeral(true)
                        .content("Request has already been processed"),
                ),
            )
            .await?;
        return Ok(());
    }

    let globals = GLOBALS.get().unwrap();
    let Some(channel) = globals.log_channel else {
        return Ok(());
    };

    let mention = Mention::from(user.id);
    let content = format!(
        "Name request from Player {} **approved** :white_check_mark: by {}: {}",
        namereq.player_uid, mention, namereq.requested_name
    );
    let allowed_mentions = CreateAllowedMentions::default().empty_users(); // avoids ping
    let msg = CreateMessage::default()
        .content(content)
        .allowed_mentions(allowed_mentions);
    channel.send_message(get_http().await, msg).await?;
    Ok(())
}

async fn handle_namereq_deny(interaction: &ComponentInteraction) -> Result<()> {
    let msg = interaction.message.content.clone();
    let user = &interaction.member.as_ref().unwrap().user;
    let by = user.tag();

    let namereq = NameRequest::parse_from_notification_message(&msg)?;
    let updated = endpoint::send_name_request_decision(&namereq, "denied", &by).await?;

    // Try to delete the initial message
    let _ = interaction.message.delete(get_http().await).await;

    if !updated {
        interaction
            .create_response(
                get_http().await,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::default()
                        .ephemeral(true)
                        .content("Request has already been processed"),
                ),
            )
            .await?;
        return Ok(());
    }

    let globals = GLOBALS.get().unwrap();
    let Some(channel) = globals.log_channel else {
        return Ok(());
    };

    let mention = Mention::from(user.id);
    let content = format!(
        "Name request from Player {} **denied** :no_entry: by {}: {}",
        namereq.player_uid, mention, namereq.requested_name
    );
    let allowed_mentions = CreateAllowedMentions::default().empty_users(); // avoids ping
    let msg = CreateMessage::default()
        .content(content)
        .allowed_mentions(allowed_mentions);
    channel.send_message(get_http().await, msg).await?;
    Ok(())
}

const ALLOWED_INTERACTIONS: [&str; 2] = ["namereq_approve", "namereq_deny"];
const PRIVILEGED_INTERACTIONS: [&str; 2] = ["namereq_approve", "namereq_deny"];

async fn handle_interaction(interaction: ComponentInteraction) -> Result<()> {
    let globals = GLOBALS.get().unwrap();

    // Check perms
    let id = interaction.data.custom_id.as_str();
    let member = interaction.member.as_ref().unwrap();
    if PRIVILEGED_INTERACTIONS.contains(&id)
        && !member.roles.iter().any(|r| globals.mod_roles.contains(r))
    {
        interaction
            .create_response(
                get_http().await,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::default()
                        .ephemeral(true)
                        .content("You don't have permission to do that."),
                ),
            )
            .await?;
        return Ok(());
    }

    match id {
        "namereq_approve" => handle_namereq_approve(&interaction).await?,
        "namereq_deny" => handle_namereq_deny(&interaction).await?,
        _ => return Err(format!("Unknown interaction: {}", id).into()),
    }

    Ok(())
}

async fn collect_interactions() {
    wait_for_globals().await;
    let globals = GLOBALS.get().unwrap();
    loop {
        let context = {
            let context = globals.context.read().await;
            context.clone()
        };

        let mut collector = std::pin::pin!(ComponentInteractionCollector::new(context)
            .filter(move |i| ALLOWED_INTERACTIONS.contains(&i.data.custom_id.as_str()))
            .stream());

        println!("Listening for interactions");
        loop {
            tokio::select! {
                interaction = collector.next() => {
                    let Some(interaction) = interaction else {
                        break; // stream ended
                    };

                    if let Err(e) = handle_interaction(interaction).await {
                        println!("Error while handling interaction: {:?}", e);
                    }
                }
                _ = globals.reconnect_notification.notified() => {
                    break; // break out to recreate collector with new context
                }
            }
        }
    }
}

async fn wait_for_globals() {
    while GLOBALS.get().is_none() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn on_reconnect(new_context: Context) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    {
        let mut context = globals.context.write().await;
        *context = new_context;
    }

    println!("Reconnected to Discord");
    send_message(globals.mod_channel, "Bot reconnected").await?;
    globals.reconnect_notification.notify_waiters();

    let num_players = globals.state.lock().await.last_player_count;
    update_status(num_players).await?;
    Ok(())
}

async fn on_init() -> Result<()> {
    let globals = GLOBALS.get().unwrap();

    let bot_user = &globals.bot_user;
    println!("Logged in as {} ({})", bot_user.tag(), bot_user.id);

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

/// Get all outstanding name requests
#[poise::command(slash_command)]
async fn namereqs(ctx: poise::Context<'_, (), Error>) -> Result<()> {
    let globals = GLOBALS.get().unwrap();

    // check moderator role
    let member = ctx.author_member().await.unwrap();
    if !member.roles.iter().any(|r| globals.mod_roles.contains(r)) {
        let reply = CreateReply::default()
            .content("You don't have permission to do that.")
            .reply(true)
            .ephemeral(true);
        if let Err(e) = ctx.send(reply).await {
            println!("Failed to reply to /namereqs: {}", e);
        }
        return Ok(());
    }

    let reqs = endpoint::get_outstanding_namereqs(globals).await?;

    let msg = format!("Found {} outstanding requests", reqs.len());
    let reply = CreateReply::default()
        .content(msg)
        .reply(true)
        .ephemeral(true);
    if let Err(e) = ctx.send(reply).await {
        println!("Failed to reply to /namereqs: {}", e);
    }

    let channel = ctx.channel_id();
    for req in reqs {
        if let Err(e) = util::send_name_request_message(channel, &req, &globals.search_query_string).await {
            println!("Failed to send name request message: {}", e);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    println!("computress-rs v{}", env!("CARGO_PKG_VERSION"));

    // Load environment variables from .env file
    let dotenv_loaded = dotenv().is_ok();

    // Initialize logging (do this after dotenv so RUST_LOG can be set in there if desired)
    env_logger::init();

    if dotenv_loaded {
        println!("Loaded .env");
    }

    // Load, parse, and validate config
    let config_file_path = args().nth(1).unwrap_or("config.json".to_string());
    let Ok(config_file_contents) = std::fs::read_to_string(&config_file_path) else {
        println!("Config file missing: {}", config_file_path);
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
    println!("Loaded config: {}", config_file_path);

    let Ok(token) = std::env::var("DISCORD_TOKEN") else {
        println!("DISCORD_TOKEN environment variable missing");
        exit(1);
    };

    let intents = GatewayIntents::non_privileged();
    let commands = vec![check(), namereqs()];
    let framework: poise::Framework<(), Error> = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands,
            event_handler: |ctx, event, _framework, _data| {
                Box::pin(async move {
                    if let FullEvent::Resume { .. } = event {
                        let _ = on_reconnect(ctx.clone()).await;
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                // Deregister any previously set global commands
                let empty_global_commands: Vec<poise::Command<(), ()>> = vec![];
                let _ =
                    poise::builtins::register_globally(ctx, empty_global_commands.as_slice()).await;

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
                        context: RwLock::new(ctx.clone()),
                        mod_roles: config.get_mod_role_ids(),
                        mod_channel: ChannelId::new(config.mod_channel_id),
                        log_channel: if config.log_channel_id != 0 {
                            Some(ChannelId::new(config.log_channel_id))
                        } else {
                            None
                        },
                        name_approvals_channel: if config.name_approvals_channel_id != 0 {
                            Some(ChannelId::new(config.name_approvals_channel_id))
                        } else {
                            None
                        },
                        monitor_address: config.monitor_address,
                        ofapi_endpoint: config.ofapi_endpoint,
                        search_query_string: config.search_query_string,
                        //
                        state: Mutex::new(state),
                        reconnect_notification: Notify::new(),
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

    tokio::spawn(collect_interactions());

    let res = client.start().await;
    if let Err(e) = res {
        println!("Client error: {:?}", e);
    }
}
