use ffmonitor::{
    BroadcastEvent, ChatEvent, EmailEvent, Event, MonitorNotification, MonitorUpdate,
    NameRequestEvent,
};
use poise::serenity_prelude::{ButtonStyle, CreateButton};

use crate::{send_message, send_message_with_buttons, update_status, Globals, Result, GLOBALS};

async fn handle_chat_event(globals: &Globals, chat: ChatEvent) -> Result<()> {
    let Some(channel) = globals.log_channel else {
        return Ok(());
    };

    let message = match chat.to {
        Some(to) => format!(
            "[{:?}] {} (to {}): {}",
            chat.kind, chat.from, to, chat.message
        ),
        None => format!("[{:?}] {}: {}", chat.kind, chat.from, chat.message),
    };
    send_message(channel, &message).await?;
    Ok(())
}

async fn handle_bcast_event(globals: &Globals, bcast: BroadcastEvent) -> Result<()> {
    let Some(channel) = globals.log_channel else {
        return Ok(());
    };

    let message = format!(
        "**[Broadcast] ({:?}) {}: {}**",
        bcast.scope, bcast.from, bcast.message,
    );
    send_message(channel, &message).await?;
    Ok(())
}

async fn handle_email_event(globals: &Globals, email: EmailEvent) -> Result<()> {
    let Some(channel) = globals.log_channel else {
        return Ok(());
    };

    let subject = email.subject.unwrap_or("no subject".to_string());
    let body = email.body.join("\n");
    let message = format!(
        "[Email] {} (to {}): <{}>\n```{}```",
        email.from, email.to, subject, body
    );
    send_message(channel, &message).await?;
    Ok(())
}

async fn handle_name_request_event(
    globals: &Globals,
    name_request_event: NameRequestEvent,
) -> Result<()> {
    let Some(channel) = globals.name_approvals_channel else {
        return Ok(());
    };

    let messsage = format!(
        "Name request from Player {}: **{}**",
        name_request_event.player_uid, name_request_event.requested_name
    );
    let buttons = vec![
        CreateButton::new("namereq_approve")
            .label("Approve")
            .style(ButtonStyle::Success),
        CreateButton::new("namereq_deny")
            .label("Deny")
            .style(ButtonStyle::Danger),
    ];

    send_message_with_buttons(channel, &messsage, buttons).await?;
    Ok(())
}

async fn handle_update(globals: &Globals, update: MonitorUpdate) -> Result<()> {
    let num_players = update.get_player_count();
    update_status(Some(num_players)).await?;

    let events = update.get_events();
    for event in events {
        match event {
            Event::Chat(chat_event) => handle_chat_event(globals, chat_event).await?,
            Event::Email(email_event) => handle_email_event(globals, email_event).await?,
            Event::Broadcast(bcast_event) => handle_bcast_event(globals, bcast_event).await?,
            Event::NameRequest(name_request_event) => {
                handle_name_request_event(globals, name_request_event).await?
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) async fn handle_notification(event: MonitorNotification) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    match event {
        MonitorNotification::Connected => println!("Connected to monitor"),
        MonitorNotification::Disconnected => {
            println!("Disconnected from monitor");
            update_status(None).await?;
        }
        MonitorNotification::Updated(update) => handle_update(globals, update).await?,
    }
    Ok(())
}
