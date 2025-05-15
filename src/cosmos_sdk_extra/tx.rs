use cosmrs::Coin;
use serde_json::{Value, json};

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
          "tip": null,
        },
        "signatures": [],
    })
}
