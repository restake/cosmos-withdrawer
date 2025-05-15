use cosmos_sdk_proto::cosmos::auth::v1beta1::BaseAccount;
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
