// Design Decision: The notifier module is deliberately thin — it only formats
// and ships a message. Deciding *when* to notify is main's responsibility,
// keeping this easy to swap for Slack/PagerDuty without touching business logic.

use anyhow::{Context, Result};
use serde_json::json;
use tracing::warn;

pub async fn send_discord(webhook_url: &str, message: &str) -> Result<()> {
    let client = reqwest::Client::new();

    // Discord's Execute Webhook endpoint expects { "content": "..." }
    let payload = json!({ "content": message });

    let resp = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .context("HTTP request to Discord webhook failed")?;

    // Successful delivery is silent here — main.rs logs the meaningful
    // version ("Discord DOWN alert sent service=X") which actually identifies
    // *which* service the notification was about.
    if !resp.status().is_success() {
        // Non-fatal: a missed notification should not crash monitoring
        warn!(status = %resp.status(), "Discord webhook returned non-2xx");
    }

    Ok(())
}
