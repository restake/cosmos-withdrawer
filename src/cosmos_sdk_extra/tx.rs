use std::time::Duration;

use cosmrs::{Coin, Tx, rpc::HttpClient, tendermint::Hash};
use eyre::bail;
use serde_json::{Value, json};
use tokio::time::sleep;
use tracing::trace;

use crate::ser::ToCosmosJson;

pub fn generate_unsigned_tx_json(
    msgs: impl IntoIterator<Item = impl ToCosmosJson>,
    memo: &str,
    gas_limit: u64,
    fee_tokens: Vec<Coin>,
) -> Value {
    json!({
        "body": {
            "messages": msgs.into_iter().map(|v| v.to_value()).collect::<Vec<_>>(),
            "memo": memo,
            "timeout_height": "0",
            "extension_options": [],
            "non_critical_extension_options": [],
        },
        "auth_info": {
          "signer_infos": [],
          "fee": {
            "amount": fee_tokens.into_iter().map(|coin| json!({
                "amount": coin.amount.to_string(),
                "denom": coin.denom,
            })).collect::<Vec<_>>(),
            "gas_limit": gas_limit.to_string(),
            "payer": "",
            "granter": "",
          },
        },
        "signatures": [],
    })
}

pub fn print_tx_result(
    result: &cosmrs::rpc::endpoint::broadcast::tx_sync::Response,
) -> eyre::Result<()> {
    eprintln!("codespace: {:?}", result.codespace);
    eprintln!("code: {:?}", result.code);
    eprintln!("data: {:?}", result.data);
    eprintln!("log: {:?}", result.log);
    eprintln!("hash: {:?}", result.hash);

    if result.code.is_err() {
        bail!("transaction failed");
    }

    Ok(())
}

pub async fn poll_tx(client: &HttpClient, tx_hash: Hash) -> eyre::Result<Tx> {
    for attempt in 0..5 {
        trace!(?tx_hash, attempt, "polling for transaction");
        match Tx::find_by_hash(client, tx_hash).await {
            Ok(tx) => return Ok(tx),
            Err(err) => {
                trace!(?err, ?tx_hash, "poll failed, sleeping");
            }
        }

        sleep(Duration::from_millis(2500)).await;
    }

    bail!("polling for tx timed out: {tx_hash:?}")
}
