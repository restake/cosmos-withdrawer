use std::{fmt, str::FromStr};

use bech32::Hrp;
use cosmos_sdk_proto::cosmos::vesting::v1beta1::{
    ContinuousVestingAccount, PeriodicVestingAccount,
};
use cosmrs::{
    AccountId,
    proto::cosmos::{
        auth::v1beta1::{BaseAccount, Bech32PrefixRequest, QueryAccountRequest},
        base::v1beta1::DecCoin,
        distribution::v1beta1::{QueryParamsRequest, QueryValidatorCommissionRequest},
    },
};
use cosmrs::{rpc::HttpClient, tendermint::chain::Id};
use eyre::{Context, ContextCompat, eyre};
use tracing::trace;

use crate::cosmos_sdk_extra::{
    abci_query::{
        Bech32Prefix, QueryAccount, QueryDistributionParams, QueryValidatorCommission,
        execute_abci_query,
    },
    ethermint::EthAccount,
    injective::EthAccount as InjectiveEthAccount,
    rpc::get_status,
};

pub struct ChainInfo {
    pub id: Id,
    pub chain_supports_setting_withdrawal_address: bool,
    pub bech32_account_prefix: Hrp,
    pub bech32_valoper_prefix: Hrp,
}

impl fmt::Debug for ChainInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainInfo")
            .field("id", &self.id)
            .field(
                "chain_supports_setting_withdrawal_address",
                &self.chain_supports_setting_withdrawal_address,
            )
            .field(
                "bech32_account_prefix",
                &self.bech32_account_prefix.as_str(),
            )
            .field(
                "bech32_valoper_prefix",
                &self.bech32_valoper_prefix.as_str(),
            )
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

    let prefix = if let Some(prefix) = supplied_account_hrp {
        prefix.clone()
    } else {
        trace!("querying chain bech32 prefix");
        execute_abci_query::<Bech32Prefix>(client, Bech32PrefixRequest {})
            .await
            .map(|res| res.bech32_prefix)
            .wrap_err("failed to query chain bech32 prefix")?
    };

    let distribution_params =
        execute_abci_query::<QueryDistributionParams>(client, QueryParamsRequest::default())
            .await
            .wrap_err("failed to query chain distribution module parameters")?;

    let chain_supports_setting_withdrawal_address = distribution_params
        .params
        .map(|params| params.withdraw_addr_enabled)
        .unwrap_or_default();

    let bech32_account_prefix = Hrp::parse(&prefix).wrap_err("failed to parse account prefix")?;
    let bech32_valoper_prefix = Hrp::parse(
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

    Ok(ChainInfo {
        id: status.node_info.network,
        chain_supports_setting_withdrawal_address,
        bech32_account_prefix,
        bech32_valoper_prefix,
    })
}

pub async fn get_account_info(
    client: &HttpClient,
    account_id: &AccountId,
) -> eyre::Result<Option<BaseAccount>> {
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

    match account.type_url.as_str() {
        /* BaseAccount::type_url() */
        "/cosmos.auth.v1beta1.BaseAccount" => {
            let account: BaseAccount = account.to_msg()?;
            Ok(Some(account))
        }
        /* ContinuousVestingAccount::type_url() */
        "/cosmos.vesting.v1beta1.ContinuousVestingAccount" => {
            let account: ContinuousVestingAccount = account.to_msg()?;
            let base_account = account
                .base_vesting_account
                .wrap_err("Continuous does not have BaseVestingAccount data")?
                .base_account
                .wrap_err("BaseVestingAccount does not have BaseAccount data")?;

            Ok(Some(base_account))
        }
        /* PeriodicVestingAccount::type_url() */
        "/cosmos.vesting.v1beta1.PeriodicVestingAccount" => {
            let account: PeriodicVestingAccount = account.to_msg()?;
            let base_account = account
                .base_vesting_account
                .wrap_err("PeriodicVestingAccount does not have BaseVestingAccount data")?
                .base_account
                .wrap_err("BaseVestingAccount does not have BaseAccount data")?;

            Ok(Some(base_account))
        }
        /* EthAccount::type_url() */
        "/ethermint.types.v1.EthAccount" => {
            let account: EthAccount = account.to_msg()?;
            Ok(Some(account.base_account))
        }
        /* InjectiveEthAccount::type_url() */
        "/injective.types.v1beta1.EthAccount" => {
            let account: InjectiveEthAccount = account.to_msg()?;
            Ok(Some(account.base_account))
        }
        type_url => Err(eyre!("unsupported account type '{type_url}'")),
    }
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

#[derive(Clone, Debug, Default)]
pub enum WalletKeyType {
    /// Standard Cosmos SDK secp256k1 key
    #[default]
    Secp256k1,
    /// eth_secp256k1, used by Ethermint/Evmos/etc.
    EthermintSecp256k1,
}

impl FromStr for WalletKeyType {
    type Err = eyre::ErrReport;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "secp256k1" => Ok(Self::Secp256k1),
            "eth_secp256k1" => Ok(Self::EthermintSecp256k1),
            s => Err(eyre!("Unsupported wallet key type '{s}'")),
        }
    }
}
