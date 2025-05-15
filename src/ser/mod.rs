use std::ops::Deref;

use cosmos_sdk_proto::{
    cosmos::{
        authz::v1beta1::{GenericAuthorization, MsgExec, MsgGrant},
        bank::v1beta1::MsgSend,
        distribution::v1beta1::{
            MsgSetWithdrawAddress, MsgWithdrawDelegatorReward, MsgWithdrawValidatorCommission,
        },
    },
    prost::{EncodeError, Name},
};
use cosmrs::Any;
use serde_json::{Value, json};

pub trait ToCosmosJson {
    fn to_value(&self) -> Value;
}

impl ToCosmosJson for Box<dyn ToCosmosJson> {
    fn to_value(&self) -> Value {
        self.deref().to_value()
    }
}

// XXX: nasty escape hatch
impl ToCosmosJson for Value {
    fn to_value(&self) -> Value {
        self.clone()
    }
}

impl ToCosmosJson for MsgGrant {
    fn to_value(&self) -> Value {
        json!({
            "@type": MsgGrant::type_url(),
            "granter": self.granter,
            "grantee": self.grantee,
            "grant": self.grant.as_ref().map(|grant| json!({
                "authorization": if let Some(authorization) = &grant.authorization {
                    // NOTE: this tool utilizes GenericAuthorization only, should be alright for now
                    let authz: GenericAuthorization = cosmrs::Any::to_msg(authorization).expect("failed to decode authorization");
                    Some(authz.to_value())
                } else {
                    None
                },
                // RFC3339
                "expiration": grant.expiration,
            })),
        })
    }
}

impl ToCosmosJson for MsgSetWithdrawAddress {
    fn to_value(&self) -> Value {
        json!({
            "@type": MsgSetWithdrawAddress::type_url(),
            "delegator_address": self.delegator_address,
            "withdraw_address": self.withdraw_address,
        })
    }
}

impl ToCosmosJson for MsgWithdrawDelegatorReward {
    fn to_value(&self) -> Value {
        json!({
            "@type": MsgWithdrawDelegatorReward::type_url(),
            "delegator_address": self.delegator_address,
            "validator_address": self.validator_address,
        })
    }
}

impl ToCosmosJson for MsgWithdrawValidatorCommission {
    fn to_value(&self) -> Value {
        json!({
            "@type": MsgWithdrawValidatorCommission::type_url(),
            "validator_address": self.validator_address,
        })
    }
}

impl ToCosmosJson for MsgSend {
    fn to_value(&self) -> Value {
        json!({
            "@type": MsgSend::type_url(),
            "from_address": self.from_address,
            "to_address": self.to_address,
            "amount": self.amount.iter().map(|coin| {
                json!({
                    "denom": coin.denom,
                    "amount": coin.amount,
                })
            }).collect::<Vec<_>>()
        })
    }
}

impl ToCosmosJson for GenericAuthorization {
    fn to_value(&self) -> Value {
        json!({
            "@type": GenericAuthorization::type_url(),
            "msg": self.msg,
        })
    }
}

#[derive(Clone)]
pub enum CosmosJsonSerializable {
    MsgGrant(MsgGrant),
    MsgSetWithdrawAddress(MsgSetWithdrawAddress),
    MsgWithdrawDelegatorReward(MsgWithdrawDelegatorReward),
    MsgWithdrawValidatorCommission(MsgWithdrawValidatorCommission),
    MsgSend(MsgSend),
    MsgExec(MsgExecCustom),
    GenericAuthorization(GenericAuthorization),
}

impl ToCosmosJson for CosmosJsonSerializable {
    fn to_value(&self) -> Value {
        match self {
            Self::MsgGrant(msg) => msg.to_value(),
            Self::MsgSetWithdrawAddress(msg) => msg.to_value(),
            Self::MsgWithdrawDelegatorReward(msg) => msg.to_value(),
            Self::MsgWithdrawValidatorCommission(msg) => msg.to_value(),
            Self::MsgSend(msg) => msg.to_value(),
            Self::MsgExec(msg) => json!({
                "@type": MsgExec::type_url(),
                "grantee": msg.grantee,
                "msgs": msg.msgs.iter().map(|v| v.to_value()).collect::<Vec<_>>(),
            }),
            Self::GenericAuthorization(msg) => msg.to_value(),
        }
    }
}

impl CosmosJsonSerializable {
    pub fn to_any(&self) -> Result<Any, EncodeError> {
        match self {
            Self::MsgGrant(msg) => Any::from_msg(msg),
            Self::MsgSetWithdrawAddress(msg) => Any::from_msg(msg),
            Self::MsgWithdrawDelegatorReward(msg) => Any::from_msg(msg),
            Self::MsgWithdrawValidatorCommission(msg) => Any::from_msg(msg),
            Self::MsgSend(msg) => Any::from_msg(msg),
            Self::MsgExec(msg) => Any::from_msg(&msg.to_native_msg_exec()?),
            Self::GenericAuthorization(msg) => Any::from_msg(msg),
        }
    }
}

impl From<MsgGrant> for CosmosJsonSerializable {
    fn from(value: MsgGrant) -> Self {
        Self::MsgGrant(value)
    }
}

impl From<MsgSetWithdrawAddress> for CosmosJsonSerializable {
    fn from(value: MsgSetWithdrawAddress) -> Self {
        Self::MsgSetWithdrawAddress(value)
    }
}

impl From<MsgWithdrawDelegatorReward> for CosmosJsonSerializable {
    fn from(value: MsgWithdrawDelegatorReward) -> Self {
        Self::MsgWithdrawDelegatorReward(value)
    }
}

impl From<MsgWithdrawValidatorCommission> for CosmosJsonSerializable {
    fn from(value: MsgWithdrawValidatorCommission) -> Self {
        Self::MsgWithdrawValidatorCommission(value)
    }
}

impl From<MsgSend> for CosmosJsonSerializable {
    fn from(value: MsgSend) -> Self {
        Self::MsgSend(value)
    }
}

impl From<MsgExecCustom> for CosmosJsonSerializable {
    fn from(value: MsgExecCustom) -> Self {
        Self::MsgExec(value)
    }
}

impl From<GenericAuthorization> for CosmosJsonSerializable {
    fn from(value: GenericAuthorization) -> Self {
        Self::GenericAuthorization(value)
    }
}

/// MsgExecCustom represents MsgExec message, but constrainted to message types supported by CosmosJsonSerializable enum
#[derive(Clone)]
pub struct MsgExecCustom {
    pub grantee: String,
    pub msgs: Vec<CosmosJsonSerializable>,
}

impl MsgExecCustom {
    /// Converts MsgExecCustom back to MsgExec used for constructing transaction bytes
    pub fn to_native_msg_exec(&self) -> Result<MsgExec, EncodeError> {
        let msgs: Result<Vec<Any>, EncodeError> =
            self.msgs.iter().map(|msg| msg.to_any()).collect();

        Ok(MsgExec {
            grantee: self.grantee.clone(),
            msgs: msgs?,
        })
    }
}

impl TryFrom<MsgExecCustom> for MsgExec {
    type Error = EncodeError;

    fn try_from(value: MsgExecCustom) -> Result<Self, Self::Error> {
        value.to_native_msg_exec()
    }
}
