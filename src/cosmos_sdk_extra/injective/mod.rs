use cosmrs::proto::cosmos::auth::v1beta1::BaseAccount;
use prost::{Message, Name};

/// Manually rolled structure for /injective.types.v1beta1.EthAccount
#[derive(Message)]
pub struct EthAccount {
    #[prost(message, required, tag = "1")]
    pub base_account: BaseAccount,
    #[prost(bytes = "vec", tag = "2")]
    pub code_hash: Vec<u8>,
}

impl Name for EthAccount {
    const NAME: &'static str = "EthAccount";
    const PACKAGE: &'static str = "injective.types.v1beta1";

    fn full_name() -> String {
        format!("{}.{}", Self::PACKAGE, Self::NAME)
    }
}

/// Manually rolled structure for /injective.crypto.v1.ethsecp256k1.PubKey
#[derive(Message)]
pub struct EthPubKey {
    #[prost(bytes = "vec", tag = "1")]
    pub key: Vec<u8>,
}

impl Name for EthPubKey {
    const NAME: &'static str = "PubKey";
    const PACKAGE: &'static str = "injective.crypto.v1beta1.ethsecp256k1";

    fn full_name() -> String {
        ::prost::alloc::format!("{}.{}", Self::PACKAGE, Self::NAME)
    }
}

impl From<super::ethermint::EthPubKey> for EthPubKey {
    fn from(value: super::ethermint::EthPubKey) -> Self {
        Self { key: value.key }
    }
}
