use cosmrs::{Coin, Denom, tx::Fee};
use eyre::bail;

use crate::{
    chain::ChainInfo,
    chain_registry::CHAIN_GAS_DENOMS_PRICES,
    cmd::{GasOption, TransactionArgs},
};

#[derive(Debug)]
pub struct GasInfo {
    pub price: f64,
    pub adjustment: f64,
    pub denom: Denom,
    pub limit: Option<u64>,
}

impl GasInfo {
    /// Determines gas info based on supplied chain & command line arguments
    pub fn determine_gas(
        chain_info: &ChainInfo,
        transaction_args: &TransactionArgs,
    ) -> eyre::Result<Self> {
        let adjustment = transaction_args.gas_adjustment;
        let supplied_price = transaction_args.gas_prices.first();
        let known_price = CHAIN_GAS_DENOMS_PRICES.get(&chain_info.id);

        let (denom, price) = match (supplied_price, known_price) {
            (Some(supplied), _) => (supplied.denom.clone(), supplied.amount),
            (None, Some(known)) => known.clone(),
            (None, None) => {
                bail!(
                    "Unknown chain id '{}'. Provide gas parameters for this chain before proceeding",
                    chain_info.id.as_str()
                );
            }
        };

        let limit = match transaction_args.gas {
            GasOption::Auto => None,
            GasOption::Amount(value) => Some(value),
        };

        Ok(Self {
            price,
            adjustment,
            denom,
            limit,
        })
    }

    pub fn get_fee(&self) -> Option<Fee> {
        if let Some(limit) = self.limit {
            let coin = Coin {
                amount: ((limit as f64 * self.adjustment).ceil() * self.price).ceil() as u128,
                denom: self.denom.clone(),
            };
            Some(Fee::from_amount_and_gas(coin, limit))
        } else {
            None
        }
    }
}
