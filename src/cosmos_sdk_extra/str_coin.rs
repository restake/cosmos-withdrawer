use std::{ops::Deref, str::FromStr};

use cosmrs::{Coin, Denom};
use eyre::Context;

#[derive(Clone, Debug)]
pub struct StrCoin(pub Coin);

impl FromStr for StrCoin {
    type Err = eyre::ErrReport;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let amount = s.chars().take_while(|c| c.is_numeric()).collect::<String>();
        let denom: Denom = s
            .chars()
            .skip(amount.len())
            .collect::<String>()
            .parse()
            .wrap_err("invalid denom")?;

        let amount: u128 = amount.parse().wrap_err("invalid amount")?;
        Ok(Self(Coin { amount, denom }))
    }
}

impl Deref for StrCoin {
    type Target = Coin;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<StrCoin> for Coin {
    fn from(value: StrCoin) -> Self {
        value.0
    }
}

#[derive(Clone, Debug)]
pub struct FloatStrCoin {
    pub amount: f64,
    pub denom: Denom,
}

impl FromStr for FloatStrCoin {
    type Err = eyre::ErrReport;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let amount = s
            .chars()
            .take_while(|c| c.is_numeric() || *c == '.')
            .collect::<String>();

        let denom: Denom = s
            .chars()
            .skip(amount.len())
            .collect::<String>()
            .parse()
            .wrap_err("invalid denom")?;
        let amount: f64 = amount.parse().wrap_err("invalid amount")?;

        Ok(Self { amount, denom })
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::{FloatStrCoin, StrCoin};

    macro_rules! assert_denom_amount {
        ($coin:expr, $amount:expr, $denom:expr $(,)?) => {{
            let result = $coin;
            // assert!(result.is_ok(), concat!(stringify!($coin), " failed"));

            let result = result.unwrap();
            assert_eq!(result.denom.to_string(), $denom);
            assert_eq!(result.amount, $amount);
        }};
    }

    #[test]
    fn test_strcoin() {
        assert!(StrCoin::from_str("0uosmo").is_ok());
        assert!(StrCoin::from_str("1uosmo").is_ok());
        assert!(StrCoin::from_str("5000uosmo").is_ok());
        assert!(StrCoin::from_str("10000000000000uosmo").is_ok());
        assert_denom_amount!(
            StrCoin::from_str(
                "500IBC/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2"
            ),
            500,
            "IBC/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2",
        );

        assert!(StrCoin::from_str("uatom").is_err());
    }

    #[test]
    fn test_floatstrcoin() {
        // Same test cases as for StrCoin
        assert!(FloatStrCoin::from_str("0uosmo").is_ok());
        assert!(FloatStrCoin::from_str("1uosmo").is_ok());
        assert!(FloatStrCoin::from_str("5000uosmo").is_ok());
        assert!(FloatStrCoin::from_str("10000000000000uosmo").is_ok());
        assert!(
            FloatStrCoin::from_str(
                "500IBC/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2"
            )
            .is_ok()
        );

        assert!(FloatStrCoin::from_str("uatom").is_err());

        // Intended use-case
        assert_denom_amount!(FloatStrCoin::from_str("0.0025uosmo"), 0.0025, "uosmo");
        assert_denom_amount!(
            FloatStrCoin::from_str("2500000000000000azeta"),
            2500000000000000_f64,
            "azeta"
        );
        assert_denom_amount!(
            FloatStrCoin::from_str(
                "0.0123IBC/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2"
            ),
            0.0123,
            "IBC/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2"
        );
    }
}
