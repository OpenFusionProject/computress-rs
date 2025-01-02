use ffmonitor::{ChatEvent, EmailEvent, Event, MonitorNotification, MonitorUpdate};

use crate::{send_message, Globals, Result, GLOBALS};

async fn handle_chat_event(globals: &Globals, chat: ChatEvent) -> Result<()> {
    let message = match chat.to {
        Some(to) => format!(
            "[{:?}] {} (to {}): {}",
            chat.kind, chat.from, to, chat.message
        ),
        None => format!("[{:?}] {}: {}", chat.kind, chat.from, chat.message),
    };
    send_message(globals.log_channel, &message).await?;
    Ok(())
}

async fn handle_email_event(globals: &Globals, email: EmailEvent) -> Result<()> {
    let subject = email.subject.unwrap_or("no subject".to_string());
    let body = email.body.join("\n");
    let message = format!(
        "[Email] {} (to {}): <{}>\n```{}```",
        email.from, email.to, subject, body
    );
    send_message(globals.log_channel, &message).await?;
    Ok(())
}

async fn handle_update(globals: &Globals, update: MonitorUpdate) -> Result<()> {
    let events = update.get_events();
    for event in events {
        match event {
            Event::Chat(chat_event) => handle_chat_event(globals, chat_event).await?,
            Event::Email(email_event) => handle_email_event(globals, email_event).await?,
            _ => {}
        }
    }
    Ok(())
}

pub(crate) async fn handle_notification(event: MonitorNotification) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    match event {
        MonitorNotification::Connected => println!("Connected to monitor"),
        MonitorNotification::Disconnected => println!("Disconnected from monitor"),
        MonitorNotification::Updated(update) => handle_update(globals, update).await?,
    }
    Ok(())
}
