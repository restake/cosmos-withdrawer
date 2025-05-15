use std::num::ParseIntError;
use std::str::FromStr;

use clap::Args;
use cosmrs::AccountId;
use eyre::eyre;

mod setup_valoper;
mod withdraw;

use crate::{chain::ChainInfo, cosmos_sdk_extra::str_coin::FloatStrCoin};

pub use self::setup_valoper::setup_valoper;
pub use self::withdraw::withdraw;

#[derive(Debug, Args)]
pub struct AccountArgs {
    /// Delegator address, as in account which delegated to a validator, or a valoper
    #[arg(long, env = "COSMOS_WITHDRAWER_DELEGATOR_ADDRESS")]
    delegator_address: AccountId,

    /// Controller address, as in account which will execute transactions for withdrawal and sending
    #[arg(long, env = "COSMOS_WITHDRAWER_CONTROLLER_ADDRESS")]
    controller_address: AccountId,

    /// Reward address, as in account which will get the rewards. Optional - uses controller address if not set.
    #[arg(long, env = "COSMOS_WITHDRAWER_REWARD_ADDRESS")]
    reward_address: Option<AccountId>,
}

impl AccountArgs {
    fn verify_accounts(&self, chain_info: &ChainInfo) -> eyre::Result<()> {
        if self.delegator_address.prefix() != chain_info.bech32_account_prefix.as_str() {
            return Err(eyre!(
                "provided delegator address prefix does not match with chain: {} != {}",
                self.delegator_address.prefix(),
                chain_info.bech32_account_prefix.as_str()
            ));
        }

        if self.controller_address.prefix() != chain_info.bech32_account_prefix.as_str() {
            return Err(eyre!(
                "provided controller address prefix does not match with chain: {} != {}",
                self.controller_address.prefix(),
                chain_info.bech32_account_prefix.as_str()
            ));
        }

        if self.delegator_address == self.controller_address {
            return Err(eyre!(
                "delegator and controller addresses should not be equal"
            ));
        }

        if let Some(reward_address) = &self.reward_address {
            if reward_address.prefix() != chain_info.bech32_account_prefix.as_str() {
                return Err(eyre!(
                    "provided reward address prefix does not match with chain: {} != {}",
                    reward_address.prefix(),
                    chain_info.bech32_account_prefix.as_str()
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
}

#[derive(Debug, Args)]
pub struct TransactionArgs {
    /// Public note to add a description to the transaction
    #[arg(long, default_value = "cosmos-withdrawer", alias = "note")]
    pub memo: String,

    /// Gas limit. Set to "auto" to calculate sufficient gas by simulating the transaction.
    #[arg(long, default_value = "auto")]
    pub gas: GasOption,

    /// Adjustment factor to be multiplied against the estimate returned by the transaction simulation. If the gas limit is set manually, then this flag is ignored
    #[arg(long, default_value = "1.25")]
    pub gas_adjustment: f64,

    // TODO: only one is supported for now
    /// Gas prices in decimal format to determine the transaction fee (e.g. 0.1uatom)
    #[arg(long, value_delimiter = ',')]
    pub gas_prices: Vec<FloatStrCoin>,

    /// The sequence number of the signing account. Used as an escape hatch for unconventional Cosmos SDK transaction simulation
    #[arg(long)]
    pub sequence: Option<u64>,

    /// The account number of the signing account. Used as an escape hatch for unconventional Cosmos SDK transaction simulation
    #[arg(long)]
    pub account_number: Option<u64>,
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
