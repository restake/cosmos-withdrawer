use std::num::ParseIntError;
use std::str::FromStr;

use clap::{Args, Subcommand};
use cosmrs::AccountId;
use cosmrs::proto::cosmos::auth::v1beta1::BaseAccount;
use cosmrs::rpc::HttpClient;
use eyre::{ContextCompat, eyre};
use tracing::trace;

mod debug;
mod setup_valoper;
mod withdraw;

use crate::chain::get_account_info;
use crate::wallet::WalletKeyType;
use crate::{chain::ChainInfo, cosmos_sdk_extra::str_coin::FloatStrCoin};

pub use self::debug::{DebugSubcommand, debug};
pub use self::setup_valoper::setup_valoper;
pub use self::withdraw::withdraw;

#[derive(Debug, Default, Subcommand)]
pub enum SetupValoperMethod {
    #[default]
    /// Determine valoper setup method based on available chain functionality
    Auto,

    /// Use authz and set withdraw address
    AuthzWithdraw,

    /// Use authz and grant sending tokens
    AuthzSend,
}

#[derive(Debug, Args)]
pub struct AccountArgs {
    /// Delegator address, as in account which delegated to a validator, or a valoper
    #[arg(long, env = "COSMOS_WITHDRAWER_DELEGATOR_ADDRESS")]
    pub delegator_address: AccountId,

    /// Delegator mnemonic phrase
    #[arg(
        long,
        env = "COSMOS_WITHDRAWER_DELEGATOR_MNEMONIC",
        hide_env_values = true
    )]
    pub delegator_mnemonic: Option<String>,

    /// Delegator address key type. Supported values are secp256k1, and eth_secp256k1.  Determined from the account info on chain by default.
    #[arg(long, env = "COSMOS_WITHDRAWER_DELEGATOR_ADDRESS_TYPE")]
    pub delegator_address_type: Option<WalletKeyType>,

    /// Delegator mnemonic coin type. Defaults to 118, which is widely used by many Cosmos SDK based networks
    #[arg(
        long,
        env = "COSMOS_WITHDRAWER_DELEGATOR_MNEMONIC_COIN_TYPE",
        default_value = "118"
    )]
    pub delegator_mnemonic_coin_type: u64,

    /// Controller address, as in account which will execute transactions for withdrawal and sending
    #[arg(long, env = "COSMOS_WITHDRAWER_CONTROLLER_ADDRESS")]
    pub controller_address: AccountId,

    /// Controller mnemonic phrase
    #[arg(
        long,
        env = "COSMOS_WITHDRAWER_CONTROLLER_MNEMONIC",
        hide_env_values = true
    )]
    pub controller_mnemonic: Option<String>,

    /// Controller mnemonic coin type. Defaults to 118, which is widely used by many Cosmos SDK based networks
    #[arg(
        long,
        env = "COSMOS_WITHDRAWER_CONTROLLER_MNEMONIC_COIN_TYPE",
        default_value = "118"
    )]
    pub controller_mnemonic_coin_type: u64,

    /// Controller address key type. Supported values are secp256k1, and eth_secp256k1. Determined from the account info on chain by default.
    #[arg(long, env = "COSMOS_WITHDRAWER_CONTROLLER_ADDRESS_TYPE")]
    pub controller_address_type: Option<WalletKeyType>,

    /// Reward address, as in account which will get the rewards. Optional - uses controller address if not set.
    #[arg(long, env = "COSMOS_WITHDRAWER_REWARD_ADDRESS")]
    pub reward_address: Option<AccountId>,
}

impl AccountArgs {
    fn verify_accounts(&self, chain_info: &ChainInfo) -> eyre::Result<()> {
        if self.delegator_address.prefix() != chain_info.bech32.account_prefix.as_str() {
            return Err(eyre!(
                "provided delegator address prefix does not match with chain: {} != {}",
                self.delegator_address.prefix(),
                chain_info.bech32.account_prefix.as_str()
            ));
        }

        if self.controller_address.prefix() != chain_info.bech32.account_prefix.as_str() {
            return Err(eyre!(
                "provided controller address prefix does not match with chain: {} != {}",
                self.controller_address.prefix(),
                chain_info.bech32.account_prefix.as_str()
            ));
        }

        if self.delegator_address == self.controller_address {
            return Err(eyre!(
                "delegator and controller addresses should not be equal"
            ));
        }

        if let Some(reward_address) = &self.reward_address {
            if reward_address.prefix() != chain_info.bech32.account_prefix.as_str() {
                return Err(eyre!(
                    "provided reward address prefix does not match with chain: {} != {}",
                    reward_address.prefix(),
                    chain_info.bech32.account_prefix.as_str()
                ));
            }

            if reward_address == &self.delegator_address {
                return Err(eyre!("delegator and reward addresses should not be equal"));
            }

            // NOTE: allowing this as by default controller address is reward address
            // if reward_address == &self.controller_address {
            //     return Err(eyre!("controller and reward addresses should not be equal"));
            // }
        }

        Ok(())
    }

    pub async fn get_account_details(
        &self,
        client: &HttpClient,
        chain_info: &ChainInfo,
    ) -> eyre::Result<ResolvedAccounts> {
        self.verify_accounts(chain_info)?;

        let (delegator_account, delegator_key_type) =
            get_account_info(client, &self.delegator_address)
                .await?
                .wrap_err("delegator account is not initialized")?;

        let delegator_key_type = delegator_key_type
            .wrap_err("delegation account does not have public key information")?
            .override_type(self.delegator_address_type);

        trace!(
            ?delegator_account,
            ?delegator_key_type,
            "delegator account info"
        );

        let (controller_account, controller_key_type) =
            get_account_info(client, &self.controller_address)
                .await?
                .wrap_err("controller account is not initialized")?;

        // Allow controller account public key to be missing, assume it's the same type
        // as delegator account public key by default
        let controller_key_type = controller_key_type
            .unwrap_or(delegator_key_type)
            .override_type(self.controller_address_type);

        trace!(
            ?controller_account,
            ?controller_key_type,
            "controller account info"
        );

        Ok(ResolvedAccounts {
            delegator_account,
            delegator_key_type,
            controller_account,
            controller_key_type,
        })
    }
}

#[derive(Debug)]
pub struct ResolvedAccounts {
    pub delegator_account: BaseAccount,
    pub delegator_key_type: WalletKeyType,
    pub controller_account: BaseAccount,
    pub controller_key_type: WalletKeyType,
}

#[derive(Debug, Args)]
pub struct TransactionArgs {
    /// Public note to add a description to the transaction
    #[arg(
        long,
        env = "COSMOS_WITHDRAWER_TX_MEMO",
        default_value = "cosmos-withdrawer",
        alias = "note"
    )]
    pub memo: String,

    /// Gas limit. Set to "auto" to calculate sufficient gas by simulating the transaction.
    #[arg(long, env = "COSMOS_WITHDRAWER_TX_GAS", default_value = "auto")]
    pub gas: GasOption,

    /// Adjustment factor to be multiplied against the estimate returned by the transaction simulation. If the gas limit is set manually, then this flag is ignored
    #[arg(
        long,
        env = "COSMOS_WITHDRAWER_TX_GAS_ADJUSTMENT",
        default_value = "1.25"
    )]
    pub gas_adjustment: f64,

    // TODO: only one is supported for now
    /// Gas prices in decimal format to determine the transaction fee (e.g. 0.1uatom). Note that you can supply only one gas price at this time
    #[arg(long, env = "COSMOS_WITHDRAWER_TX_GAS_PRICES", value_delimiter = ',')]
    pub gas_prices: Vec<FloatStrCoin>,

    /// The sequence number of the signing account. Used as an escape hatch for unconventional Cosmos SDK transaction simulation
    #[arg(long)]
    pub sequence: Option<u64>,

    /// The account number of the signing account. Used as an escape hatch for unconventional Cosmos SDK transaction simulation
    #[arg(long)]
    pub account_number: Option<u64>,

    /// Whether to only generate the transaction JSON to stdout for signing & broadcasting externally, e.g. `osmosisd tx sign ./tx_unsigned.json --from=mykey | osmosisd tx broadcast -`.
    #[arg(long)]
    pub generate_only: bool,

    /// Do everything but broadcast the transaction.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub enum GasOption {
    Auto,
    Amount(u64),
}

impl FromStr for GasOption {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "auto" {
            Ok(Self::Auto)
        } else {
            Ok(Self::Amount(s.parse()?))
        }
    }
}
