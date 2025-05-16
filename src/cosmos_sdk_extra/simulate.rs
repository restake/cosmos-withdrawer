use cosmrs::{
    Coin, Denom,
    proto::{
        cosmos::tx::v1beta1::{SimulateRequest, SimulateResponse, Tx},
        prost::Message,
    },
    rpc::HttpClient,
    tx::{Body, Fee},
};
use eyre::ContextCompat;
use tracing::trace;

use crate::{
    chain::ChainInfo,
    cosmos_sdk_extra::{
        abci_query::{Simulate, execute_abci_query},
        gas::GasInfo,
    },
    wallet::{TxSigner, sign_transaction},
};

/// Creates dummy fee structure used for simulation
pub fn simulation_fee(denom: Denom) -> Fee {
    let amount = Coin { denom, amount: 1 };
    Fee::from_amount_and_gas(amount, 1_u64)
}

pub async fn simulate_tx(
    client: &HttpClient,
    chain_info: &ChainInfo,
    gas_info: &GasInfo,
    signer: &TxSigner,
    body: Body,
) -> eyre::Result<Fee> {
    let tx = sign_transaction(
        chain_info,
        signer,
        simulation_fee(gas_info.denom.clone()),
        body,
    )?;

    #[allow(deprecated)]
    let SimulateResponse {
        gas_info: sim_gas_info,
        ..
    } = execute_abci_query::<Simulate>(
        client,
        SimulateRequest {
            // TODO: some older cosmos SDKs don't support tx_bytes
            tx: None,
            tx_bytes: Tx::from(tx).encode_to_vec(),
        },
    )
    .await?;

    let sim_gas_info = sim_gas_info.wrap_err("Simulation did not contain spent gas info")?;
    let gas_limit = ((sim_gas_info.gas_used as f64) * gas_info.adjustment).round();
    let amount = Coin {
        amount: (gas_limit * gas_info.price).round() as u128,
        denom: gas_info.denom.clone(),
    };

    let fee = Fee::from_amount_and_gas(amount, gas_limit as u64);
    trace!(?fee, "transaction simulation result");
    Ok(fee)
}
