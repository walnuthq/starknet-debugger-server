use reqwest::Client;
use starknet_api::core::ChainId;
use tracing::{debug, info, warn};
use urlencoding::encode;
use simulate::{SimulationArgs, SimulationRawArgs};

pub async fn send_telegram_notification_tx_id(tx_id: &str, chain_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let message = format!("New transaction [{chain_id}]: https://app.walnut.dev/transactions?chainId={chain_id}&txHash={tx_id}&skip_tracking=true");
    println!("mam to ziomek: {}", message);
    // send_telegram_notification(message.as_str())
    Ok(())
}

pub async fn send_telegram_notification_custom_rpc(tx_id: &str, rpc_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let message = format!("New transaction [custom RPC]: https://app.walnut.dev/transactions?rpcUrl={}&txHash={}&skip_tracking=true", encode(rpc_url), tx_id);
    println!("mam to ziomek custom rpc: {}", message);
    Ok(())
    // send_telegram_notification(message.as_str())
}

pub async fn send_telegram_notification_calldata(simulation_args: &SimulationArgs) -> Result<(), Box<dyn std::error::Error>> {
    let message = format!("New transaction [calldata]: https://app.walnut.dev/transactions?senderAddress={}&calldata={:?}&transactionVersion={}&blockNumber={}&rpcUrl={}&skip_tracking=true",
        simulation_args.sender_address,
        simulation_args.calldata,
        simulation_args.transaction_version.to_string(),
        simulation_args.block_number,
        encode(simulation_args.rpc_url.as_str()),
    );
    println!("mam to ziomek calldata: {}", message);
    Ok(())
    // send_telegram_notification(message.as_str())
}

async fn send_telegram_notification(message: &str) -> Result<(), Box<dyn std::error::Error>> {
    let telegram_bot_api_key: String = std::env::var("TELEGRAM_BOT_API_KEY").unwrap_or(String::new());
    let telegram_bot_api_url: String = format!("https://api.telegram.org/bot{telegram_bot_api_key}/sendMessage");
    let telegram_walnut_notifications_chat_id: String = std::env::var("TELEGRAM_WALNUT_NOTIFICATIONS_CHAT_ID").unwrap_or(String::new());

    // skip if telegram_bot_api_key is not set
    if telegram_bot_api_key.is_empty() {
        debug!("telegram_bot_api_key is not set, skipping notification.");
        return Ok(());
    }

    if telegram_walnut_notifications_chat_id.is_empty() {
        debug!("telegram_walnut_notifications_chat_id is not set, skipping notification.");
        return Ok(());
    }

    let client = Client::new();

    let payload = serde_json::json!({
        "chat_id": telegram_walnut_notifications_chat_id,
        "text": message,
    });

    let res = client.post(telegram_bot_api_url)
        .json(&payload)
        .send()
        .await;

    match res {
        Ok(res) => {
            if !res.status().is_success() {
                info!("Failed to send telegram notification. Message to send: {}. Error: {}", message, res.text().await.unwrap_or_else(|_| "Unknown error".to_string()));
            }
        }
        Err(e) => {
            warn!("Failed to send telegram notification. Message to send: {}. Error: {}", message, e);
        }
    }
    Ok(())
}