use poise::serenity_prelude::{ButtonStyle, ChannelId, CreateButton};

use crate::{send_message_with_buttons, NameRequest, Result};

pub(crate) async fn send_name_request_message(
    channel: ChannelId,
    name_request: &NameRequest,
) -> Result<()> {
    let messsage = format!(
        "Name request from Player {}: **{}**",
        name_request.player_uid, name_request.requested_name
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
