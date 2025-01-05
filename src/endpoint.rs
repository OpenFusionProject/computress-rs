use std::{env, sync::OnceLock, time::Duration};

use reqwest::Client;
use serde::Serialize;

use crate::{Globals, NameRequest, Result};

#[derive(Debug, Serialize)]
struct NameRequestDecision {
    player_uid: u64,
    requested_name: String,
    decision: String,
}

fn get_http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(format!("computress-rs/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap()
    })
}

fn get_token() -> Result<String> {
    env::var("OFAPI_TOKEN").map_err(|_| "OFAPI_TOKEN environment variable missing".into())
}

pub(crate) async fn send_name_request_decision(
    globals: &Globals,
    namereq: &NameRequest,
    decision: &str,
) -> Result<()> {
    let endpoint = format!("https://{}/namereq", globals.ofapi_endpoint);
    let req = NameRequestDecision {
        player_uid: namereq.player_uid,
        requested_name: namereq.requested_name.clone(),
        decision: decision.to_string(),
    };

    let token = get_token()?;
    let resp = get_http_client()
        .post(&endpoint)
        .bearer_auth(token)
        .json(&req)
        .send()
        .await?;

    let status_code = resp.status();
    if !status_code.is_success() {
        return Err(format!("OFAPI error: {} {}", endpoint, status_code).into());
    }
    Ok(())
}
