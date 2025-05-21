use std::fmt;

use bech32::Hrp;
use cosmrs::{
    AccountId,
    proto::cosmos::{
        auth::v1beta1::{BaseAccount, Bech32PrefixRequest, QueryAccountRequest},
        base::v1beta1::DecCoin,
        distribution::v1beta1::{QueryParamsRequest, QueryValidatorCommissionRequest},
        vesting::v1beta1::{ContinuousVestingAccount, PeriodicVestingAccount},
    },
};
use cosmrs::{rpc::HttpClient, tendermint::chain::Id};
use eyre::{Context, ContextCompat, bail};
use tracing::trace;

use crate::{
    cosmos_sdk_extra::{
        abci_query::{
            Bech32Prefix, QueryAccount, QueryDistributionParams, QueryValidatorCommission,
            execute_abci_query,
        },
        ethermint::EthAccount,
        injective::EthAccount as InjectiveEthAccount,
        rpc::get_status,
    },
    wallet::WalletKeyType,
};

pub struct Bech32Prefixes {
    pub account_prefix: Hrp,
    pub valoper_prefix: Hrp,
}

impl fmt::Debug for Bech32Prefixes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bech32Prefixes")
            .field("account_prefix", &self.account_prefix.as_str())
            .field("valoper_prefix", &self.valoper_prefix.as_str())
            .finish()
    }
}

pub async fn get_chain_bech32_prefixes(
    client: &HttpClient,
    supplied_account_hrp: Option<&String>,
    supplied_valoper_hrp: Option<&String>,
) -> eyre::Result<Bech32Prefixes> {
    let prefix = if let Some(prefix) = supplied_account_hrp {
        prefix.clone()
    } else {
        trace!("querying chain bech32 prefix");
        execute_abci_query::<Bech32Prefix>(client, Bech32PrefixRequest {})
            .await
            .map(|res| res.bech32_prefix)
            .wrap_err("failed to query chain bech32 prefix")?
    };

    let account_prefix = Hrp::parse(&prefix).wrap_err("failed to parse account prefix")?;
    let valoper_prefix = Hrp::parse(
        supplied_valoper_hrp
            .cloned()
            .unwrap_or_else(|| {
                // Usually chains have `valoper` suffix to normal account bech32 prefix.
                // This assumption works quite well in the wild, but there are some chains which
                // don't use this scheme
                format!("{prefix}valoper")
            })
            .as_str(),
    )
    .wrap_err("failed to parse valoper prefix")?;

    Ok(Bech32Prefixes {
        account_prefix,
        valoper_prefix,
    })
}

pub struct ChainInfo {
    pub id: Id,
    pub chain_supports_setting_withdrawal_address: bool,
    pub bech32: Bech32Prefixes,
}

impl fmt::Debug for ChainInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainInfo")
            .field("id", &self.id)
            .field(
                "chain_supports_setting_withdrawal_address",
                &self.chain_supports_setting_withdrawal_address,
            )
            .field("bech32", &self.bech32)
            .finish()
    }
}

pub async fn get_chain_info(
    client: &HttpClient,
    supplied_account_hrp: Option<&String>,
    supplied_valoper_hrp: Option<&String>,
) -> eyre::Result<ChainInfo> {
    let status = get_status(client)
        .await
        .wrap_err("failed to get chain status")?;

    let distribution_params =
        execute_abci_query::<QueryDistributionParams>(client, QueryParamsRequest::default())
            .await
            .wrap_err("failed to query chain distribution module parameters")?;

    let chain_supports_setting_withdrawal_address = distribution_params
        .params
        .map(|params| params.withdraw_addr_enabled)
        .unwrap_or_default();

    let bech32 =
        get_chain_bech32_prefixes(client, supplied_account_hrp, supplied_valoper_hrp).await?;

    Ok(ChainInfo {
        id: status.node_info.network,
        chain_supports_setting_withdrawal_address,
        bech32,
    })
}

pub async fn get_account_info(
    client: &HttpClient,
    account_id: &AccountId,
) -> eyre::Result<Option<(BaseAccount, Option<WalletKeyType>)>> {
    let account = execute_abci_query::<QueryAccount>(
        client,
        QueryAccountRequest {
            address: account_id.to_string(),
        },
    )
    .await
    .wrap_err("failed to query account")?;

    let Some(account) = account.account else {
        return Ok(None);
    };

    let base_account: BaseAccount = match account.type_url.as_str() {
        /* BaseAccount::type_url() */
        "/cosmos.auth.v1beta1.BaseAccount" => account.to_msg()?,

        /* ContinuousVestingAccount::type_url() */
        "/cosmos.vesting.v1beta1.ContinuousVestingAccount" => account
            .to_msg::<ContinuousVestingAccount>()?
            .base_vesting_account
            .wrap_err("Continuous does not have BaseVestingAccount data")?
            .base_account
            .wrap_err("BaseVestingAccount does not have BaseAccount data")?,

        /* PeriodicVestingAccount::type_url() */
        "/cosmos.vesting.v1beta1.PeriodicVestingAccount" => {
            let account: PeriodicVestingAccount = account.to_msg()?;

            account
                .base_vesting_account
                .wrap_err("PeriodicVestingAccount does not have BaseVestingAccount data")?
                .base_account
                .wrap_err("BaseVestingAccount does not have BaseAccount data")?
        }

        /* EthAccount::type_url() */
        "/ethermint.types.v1.EthAccount" => account.to_msg::<EthAccount>()?.base_account,

        /* InjectiveEthAccount::type_url() */
        "/injective.types.v1beta1.EthAccount" => {
            account.to_msg::<InjectiveEthAccount>()?.base_account
        }

        type_url => bail!("unsupported account type '{type_url}'"),
    };

    let wallet_key_type = if let Some(pub_key) = base_account.pub_key.as_ref() {
        Some(WalletKeyType::try_from(pub_key)?)
    } else {
        None
    };

    Ok(Some((base_account, wallet_key_type)))
}

pub async fn get_validator_commission(
    client: &HttpClient,
    validator_account_id: &AccountId,
) -> eyre::Result<Option<Vec<DecCoin>>> {
    let commission = execute_abci_query::<QueryValidatorCommission>(
        client,
        QueryValidatorCommissionRequest {
            validator_address: validator_account_id.to_string(),
        },
    )
    .await
    .wrap_err("failed to query validator")?;

    Ok(commission
        .commission
        .map(|commission| commission.commission))
}
