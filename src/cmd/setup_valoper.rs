use cosmrs::{
    Any,
    proto::{
        cosmos::{
            authz::v1beta1::{GenericAuthorization, Grant, MsgGrant},
            bank::v1beta1::MsgSend,
            distribution::v1beta1::{
                MsgSetWithdrawAddress, MsgWithdrawDelegatorReward, MsgWithdrawValidatorCommission,
            },
            tx::v1beta1::Tx,
        },
        prost::Name,
    },
    rpc::{Client, HttpClient},
    tx::MessageExt,
};
use eyre::{Context, eyre};
use tracing::{info, warn};

use crate::{
    AccountArgs, SetupValoperMethod, TransactionArgs,
    chain::get_chain_info,
    cmd::ResolvedAccounts,
    cosmos_sdk_extra::{
        gas::GasInfo,
        simulate::simulate_tx,
        tx::{generate_unsigned_tx_json, poll_tx, print_tx_result},
    },
    ser::{CosmosJsonSerializable, TimestampStr},
    wallet::{SigningAccountType, construct_transaction_body, setup_signer, sign_transaction},
};

pub async fn setup_valoper(
    rpc_url: &str,
    account_hrp: Option<&String>,
    valoper_hrp: Option<&String>,
    account: AccountArgs,
    transaction_args: TransactionArgs,
    method: SetupValoperMethod,
    expiration: Option<&TimestampStr>,
) -> eyre::Result<()> {
    let client = HttpClient::new(rpc_url)?;
    let chain_info = get_chain_info(&client, account_hrp, valoper_hrp).await?;
    let gas_info = GasInfo::determine_gas(&chain_info, &transaction_args)?;

    info!(?chain_info, ?gas_info.denom, ?gas_info.price, "chain info");

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
    let ResolvedAccounts {
        delegator_account,
        delegator_key_type,
        ..
    } = account.get_account_details(&client, &chain_info).await?;

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

    // This transaction will be signed by the delegator account
    let signer = setup_signer(
        &account,
        &chain_info.bech32,
        SigningAccountType::Delegator {
            key_type: delegator_key_type,
            account_number: transaction_args
                .account_number
                .unwrap_or(delegator_account.account_number),
            sequence: transaction_args
                .sequence
                .unwrap_or(delegator_account.sequence),
        },
        transaction_args.generate_only,
    )?;

    // Determine necessary fee for transaction execution
    let fee = if let Some(fee) = gas_info.get_fee() {
        fee
    } else {
        simulate_tx(
            &client,
            &chain_info,
            &gas_info,
            &signer,
            construct_transaction_body(&transaction_args.memo, &msgs)?,
        )
        .await?
    };

    if transaction_args.generate_only {
        println!(
            "{}",
            generate_unsigned_tx_json(msgs, &transaction_args.memo, fee.gas_limit, fee.amount)
        );

        return Ok(());
    }

    let signed_tx = sign_transaction(
        &chain_info,
        &signer,
        fee,
        construct_transaction_body(&transaction_args.memo, &msgs)?,
    )
    .wrap_err("failed to sign withdraw transaction")?;

    if transaction_args.dry_run {
        info!("dry run was requested, nothing was done");
        return Ok(());
    }

    let tx_result = client
        .broadcast_tx_sync(Tx::from(signed_tx).to_bytes()?)
        .await?;

    print_tx_result(&tx_result)?;
    poll_tx(&client, tx_result.hash).await?;
    info!(tx_hash = ?tx_result.hash, "transaction committed to chain, valoper set up");

    Ok(())
}
