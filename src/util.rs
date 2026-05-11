use poise::serenity_prelude::{ButtonStyle, ChannelId, CreateButton};
use urlencoding::encode;

use crate::{send_message_with_buttons, NameRequest, Result};

pub(crate) async fn send_name_request_message(
    channel: ChannelId,
    name_request: &NameRequest,
    search_template: &str,
) -> Result<()> {
    let messsage = format!(
        "Name request from Player {}: **{}**",
        name_request.player_uid, name_request.requested_name
    );

    let mut buttons = vec![
        CreateButton::new("namereq_approve")
            .label("Approve")
            .style(ButtonStyle::Success),
        CreateButton::new("namereq_deny")
            .label("Deny")
            .style(ButtonStyle::Danger),
    ];

    if !search_template.is_empty() {
        let encoded_name = encode(&name_request.requested_name).into_owned();
        let search_url = search_template.replace("{}", &encoded_name);
        buttons.push(CreateButton::new_link(&search_url).label("Search"))
    }

    send_message_with_buttons(channel, &messsage, buttons).await?;
    Ok(())
}
