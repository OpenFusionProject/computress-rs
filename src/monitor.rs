use ffmonitor::MonitorNotification;

use crate::{send_message, Result, GLOBALS};

pub(crate) async fn handle_monitor_event(event: MonitorNotification) -> Result<()> {
    let globals = GLOBALS.get().unwrap();
    let message = format!("{:?}", event);
    send_message(globals.mod_channel, &message).await?;
    Ok(())
}
