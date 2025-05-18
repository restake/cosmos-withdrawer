use bech32::Hrp;
use clap::Subcommand;
use cosmrs::rpc::HttpClient;
use serde_json::json;

use crate::{
    chain::get_chain_bech32_prefixes,
    wallet::{TxSigner, WalletKeyType, derive_key},
};

#[derive(Clone, Debug, Subcommand)]
pub enum DebugSubcommand {
    /// Derive address
    DeriveAddress {
        /// 24-word mnemonic
        #[clap(long, env = "MNEMONIC", hide_env_values = true)]
        mnemonic: String,

        /// Wallet key type. Supported values are secp256k1, and eth_secp256k1.
        #[clap(long, default_value = "secp256k1")]
        key_type: WalletKeyType,

        /// Coin type. Defaults to 118, which is widely used by many Cosmos SDK based networks
        #[clap(long, default_value = "118")]
        coin_type: u64,
    },
}

pub async fn debug(
    rpc_url: &str,
    account_hrp: Option<&String>,
    valoper_hrp: Option<&String>,
    debug: DebugSubcommand,
) -> eyre::Result<()> {
    match debug {
        DebugSubcommand::DeriveAddress {
            mnemonic,
            key_type,
            coin_type,
        } => {
            derive_address(
                rpc_url,
                account_hrp,
                valoper_hrp,
                &mnemonic,
                key_type,
                coin_type,
            )
            .await?
        }
    }
    Ok(())
}

async fn derive_address(
    rpc_url: &str,
    account_hrp: Option<&String>,
    valoper_hrp: Option<&String>,
    mnemonic: &str,
    key_type: WalletKeyType,
    coin_type: u64,
) -> eyre::Result<()> {
    let signing_key = derive_key(mnemonic, "", coin_type)?;
    let signer = TxSigner::new(signing_key, key_type);

    // Ensure that we have HRPs for deriving account ids
    let (account_hrp, valoper_hrp) = match (account_hrp, valoper_hrp) {
        (Some(account_hrp), Some(valoper_hrp)) => {
            (Hrp::parse(account_hrp)?, Hrp::parse(valoper_hrp)?)
        }
        (account_hrp, valoper_hrp) => {
            let client = HttpClient::new(rpc_url)?;
            let chain_info = get_chain_bech32_prefixes(&client, account_hrp, valoper_hrp).await?;
            (chain_info.account_prefix, chain_info.valoper_prefix)
        }
    };

    let account_id = signer.account_id(&account_hrp)?;
    let valoper_id = signer.account_id(&valoper_hrp)?;

    println!(
        "{}",
        json!({
            "account_id": account_id,
            "valoper_id": valoper_id,
        })
    );

    Ok(())
}
