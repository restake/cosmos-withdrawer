use cosmrs::{
    Coin,
    crypto::secp256k1::SigningKey,
    proto::{
        cosmos::tx::v1beta1::{SimulateRequest, SimulateResponse, Tx},
        prost::Message,
    },
    rpc::HttpClient,
    tx::{Body, BodyBuilder, Fee, SignDoc, SignerInfo},
};
use eyre::ContextCompat;

use crate::{
    chain::ChainInfo,
    cosmos_sdk_extra::{
        abci_query::{Simulate, execute_abci_query},
        gas::GasInfo,
    },
    ser::CosmosJsonSerializable,
};

pub struct TxSimulationAccount {
    pub signing_key: SigningKey,
    pub account_number: u64,
    pub sequence_number: u64,
}

impl TxSimulationAccount {
    pub fn random() -> Self {
        Self {
            signing_key: SigningKey::random(),
            account_number: 0,
            sequence_number: 0,
        }
    }
}

pub async fn simulate_tx(
    client: &HttpClient,
    chain_info: &ChainInfo,
    gas_info: &GasInfo,
    signer: Option<TxSimulationAccount>,
    body: Body,
) -> eyre::Result<Fee> {
    let (auth_info, signatures) = {
        let TxSimulationAccount {
            signing_key,
            account_number,
            sequence_number,
        } = signer.unwrap_or_else(TxSimulationAccount::random);

        let amount = Coin {
            denom: gas_info.denom.clone(),
            amount: 1,
        };

        let signer_info =
            SignerInfo::single_direct(Some(signing_key.public_key()), sequence_number);
        let auth_info = signer_info.auth_info(Fee::from_amount_and_gas(amount, 1_u64));
        let sign_doc = SignDoc::new(&body, &auth_info, &chain_info.id, account_number)?;

        let sign_doc_bytes = sign_doc.into_bytes()?;
        let signature = signing_key.sign(&sign_doc_bytes)?;

        (auth_info, vec![signature.to_vec()])
    };

    let tx = cosmrs::Tx {
        body,
        auth_info,
        signatures,
    };

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

    Ok(Fee::from_amount_and_gas(amount, gas_limit as u64))
}

pub async fn simulate_tx_messages<'a, I: IntoIterator<Item = &'a CosmosJsonSerializable>>(
    client: &HttpClient,
    chain_info: &ChainInfo,
    gas_info: &GasInfo,
    msgs: I,
    memo: &str,
) -> eyre::Result<Fee> {
    let tx_body = BodyBuilder::new()
        .memo(memo)
        .msgs(
            msgs.into_iter()
                .map(|msg| msg.to_any())
                .collect::<Result<Vec<_>, _>>()?,
        )
        .finish();

    simulate_tx(client, chain_info, gas_info, None, tx_body).await
}
