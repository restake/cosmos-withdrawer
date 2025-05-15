use cosmos_sdk_proto::{
    cosmos::{
        authz::v1beta1::{GenericAuthorization, Grant, MsgGrant},
        bank::v1beta1::MsgSend,
        distribution::v1beta1::{
            MsgSetWithdrawAddress, MsgWithdrawDelegatorReward, MsgWithdrawValidatorCommission,
        },
    },
    prost::Name,
};
use cosmrs::{Any, rpc::HttpClient};
use eyre::{ContextCompat, bail, eyre};
use tracing::{info, warn};

use crate::{
    AccountArgs, SetupValoperMethod, TransactionArgs,
    chain::{get_account_info, get_chain_info},
    cosmos_sdk_extra::{
        gas::GasInfo, simulate::simulate_tx_messages, tx::generate_unsigned_tx_json,
    },
    ser::{CosmosJsonSerializable, TimestampStr},
};

pub async fn setup_valoper(
    rpc_url: &str,
    account_hrp: Option<&String>,
    valoper_hrp: Option<&String>,
    account: AccountArgs,
    transaction_args: TransactionArgs,
    method: SetupValoperMethod,
    expiration: Option<&TimestampStr>,
    generate_only: bool,
) -> eyre::Result<()> {
    let client = HttpClient::new(rpc_url)?;
    let chain_info = get_chain_info(&client, account_hrp, valoper_hrp).await?;
    let gas_info = GasInfo::determine_gas(&chain_info, &transaction_args)?;

    info!(?chain_info, ?gas_info.denom, ?gas_info.price, "chain info");

    account.verify_accounts(&chain_info)?;

    // Determine setup method
    let setup_method = match (method, chain_info.chain_supports_setting_withdrawal_address) {
        (SetupValoperMethod::Auto, true) => SetupValoperMethod::AuthzWithdraw,
        (SetupValoperMethod::Auto, false) => SetupValoperMethod::AuthzSend,

        // Invariants
        (SetupValoperMethod::AuthzWithdraw, false) => {
            return Err(eyre!(
                "this chain does not support setting withdrawal address for distribution"
            ));
        }
        (m @ SetupValoperMethod::AuthzSend, true) => {
            warn!(
                chain_id = ?chain_info.id,
                "this chain supports setting withdrawal address, granting MsgSend has security implications"
            );
            m
        }

        // Pass-through
        (method, _) => method,
    };

    // Ensure delegator & controller accounts are initialized
    // Withdrawal address does not need to be initialized, as it'll only receive rewards
    let delegator_account = get_account_info(&client, &account.delegator_address)
        .await?
        .wrap_err("delegator account is not initialized")?;

    let controller_account = get_account_info(&client, &account.controller_address)
        .await?
        .wrap_err("controller account is not initialized")?;

    let mut msgs: Vec<CosmosJsonSerializable> = Vec::new();
    info!(?setup_method, "setting up valoper account grants");
    match setup_method {
        SetupValoperMethod::AuthzWithdraw => {
            let withdraw_address = account
                .reward_address
                .as_ref()
                .unwrap_or(&account.controller_address);

            let msg_withdraw = MsgSetWithdrawAddress {
                delegator_address: account.delegator_address.to_string(),
                withdraw_address: withdraw_address.to_string(),
            };

            let msg_authz_withdraw_reward = MsgGrant {
                granter: account.delegator_address.to_string(),
                grantee: account.controller_address.to_string(),
                grant: Some(Grant {
                    authorization: Some(Any::from_msg(&GenericAuthorization {
                        msg: MsgWithdrawDelegatorReward::type_url(),
                    })?),
                    expiration: expiration.map(|e| *e.as_ref()),
                }),
            };
            let msg_authz_withdraw_commission = MsgGrant {
                granter: account.delegator_address.to_string(),
                grantee: account.controller_address.to_string(),
                grant: Some(Grant {
                    authorization: Some(Any::from_msg(&GenericAuthorization {
                        msg: MsgWithdrawValidatorCommission::type_url(),
                    })?),
                    expiration: expiration.map(|e| *e.as_ref()),
                }),
            };

            msgs.push(msg_withdraw.into());
            msgs.push(msg_authz_withdraw_reward.into());
            msgs.push(msg_authz_withdraw_commission.into());
        }
        SetupValoperMethod::AuthzSend => {
            warn!("authz-send method does not work with --generate-only");
            let msg_authz_withdraw_reward = MsgGrant {
                granter: account.delegator_address.to_string(),
                grantee: account.controller_address.to_string(),
                grant: Some(Grant {
                    authorization: Some(Any::from_msg(&GenericAuthorization {
                        msg: MsgWithdrawDelegatorReward::type_url(),
                    })?),
                    expiration: None,
                }),
            };
            let msg_authz_withdraw_commission = MsgGrant {
                granter: account.delegator_address.to_string(),
                grantee: account.controller_address.to_string(),
                grant: Some(Grant {
                    authorization: Some(Any::from_msg(&GenericAuthorization {
                        msg: MsgWithdrawValidatorCommission::type_url(),
                    })?),
                    expiration: None,
                }),
            };
            let msg_authz_send = MsgGrant {
                granter: account.delegator_address.to_string(),
                grantee: account.controller_address.to_string(),
                grant: Some(Grant {
                    authorization: Some(Any::from_msg(&GenericAuthorization {
                        msg: MsgSend::type_url(),
                    })?),
                    expiration: None,
                }),
            };

            msgs.push(msg_authz_withdraw_reward.into());
            msgs.push(msg_authz_withdraw_commission.into());
            msgs.push(msg_authz_send.into());
        }
        _ => unreachable!(),
    }

    let fee = if let Some(fee) = gas_info.get_fee() {
        fee
    } else {
        simulate_tx_messages(
            &client,
            &chain_info,
            &gas_info,
            &msgs,
            &transaction_args.memo,
            account.delegator_address_type,
            transaction_args
                .account_number
                .or(Some(delegator_account.account_number)),
            transaction_args
                .sequence
                .or(Some(delegator_account.sequence)),
        )
        .await?
    };

    if generate_only {
        println!(
            "{}",
            generate_unsigned_tx_json(msgs, &transaction_args.memo, fee.gas_limit, fee.amount)
        );

        return Ok(());
    }

    let _ = delegator_account;
    let _ = controller_account;

    bail!("transaction signing & broadcasting is not implemented yet")
}
