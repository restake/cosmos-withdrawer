use std::{collections::HashMap, sync::LazyLock};

use cosmrs::{Denom, tendermint::chain::Id};

/// CHAIN_GAS_DENOMS_PRICES holds a default mapping of chain id -> gas denom, gas price.
/// There's no way to query this info reliably from the chain RPC for now.
pub static CHAIN_GAS_DENOMS_PRICES: LazyLock<HashMap<Id, (Denom, f64)>> = LazyLock::new(|| {
    const DATA: &str = include_str!("gas_data.json");
    serde_json::from_str(DATA).expect("Failed to parse supplied chain data")
});
