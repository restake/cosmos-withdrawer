use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use cosmrs::{
    AccountId,
    proto::cosmos::{
        distribution::v1beta1::{
            MsgWithdrawDelegatorReward, MsgWithdrawValidatorCommission,
            QueryDelegationTotalRewardsRequest,
        },
        tx::v1beta1::Tx,
    },
    rpc::{Client, HttpClient},
    tx::MessageExt,
};
use eyre::Context;
use num_bigint::BigUint;
use tracing::{debug, info, trace, warn};

use crate::{
    AccountArgs, TransactionArgs,
    chain::{get_chain_info, get_validator_commission},
    cmd::ResolvedAccounts,
    cosmos_sdk_extra::{
        abci_query::{QueryDelegationTotalRewards, execute_abci_query},
        gas::GasInfo,
        simulate::simulate_tx,
        str_coin::StrCoin,
        tx::generate_unsigned_tx_json,
    },
    ser::{CosmosJsonSerializable, MsgExecCustom},
    wallet::{SigningAccountType, construct_transaction_body, setup_signer, sign_transaction},
};

pub async fn withdraw(
    rpc_url: &str,
    account_hrp: Option<&String>,
    valoper_hrp: Option<&String>,
    account: AccountArgs,
    transaction_args: TransactionArgs,
    thresholds: Vec<StrCoin>,
) -> eyre::Result<()> {
    let client = HttpClient::new(rpc_url)?;
    let chain_info = get_chain_info(&client, account_hrp, valoper_hrp).await?;
    let gas_info = GasInfo::determine_gas(&chain_info, &transaction_args)?;

    info!(?chain_info, ?gas_info.denom, ?gas_info.price, "chain info");

    // Ensure delegator & controller accounts are initialized
    // Withdrawal address does not need to be initialized, as it'll only receive rewards
    let ResolvedAccounts {
        controller_account,
        controller_key_type,
        ..
    } = account.get_account_details(&client, &chain_info).await?;

    let delegation_total_rewards = execute_abci_query::<QueryDelegationTotalRewards>(
        &client,
        QueryDelegationTotalRewardsRequest {
            delegator_address: account.delegator_address.to_string(),
        },
    )
    .await?;

    trace!(?delegation_total_rewards, "available rewards");

    let thresholds_by_denom: HashMap<String, BigUint> = thresholds
        .iter()
        .map(|coin| (coin.denom.to_string(), BigUint::from(coin.amount)))
        .collect();

    let mut withdraw_self_valoper: Option<String> = None;
    let mut withdraw_validators: HashSet<String> = HashSet::new();
    let mut collected_coins: HashMap<String, BigUint> = HashMap::new();

    for reward in delegation_total_rewards.rewards.iter() {
        let validator_address = AccountId::from_str(&reward.validator_address)
            .wrap_err("failed to parse validator address")?;

        // Check if we can withdraw commissions
        if validator_address.to_bytes() == account.delegator_address.to_bytes() {
            debug!(?validator_address, delegator_address = ?account.delegator_address, "delegator is also a validator, checking for commissions");
            if let Some(commission) = get_validator_commission(&client, &validator_address).await? {
                trace!(?commission, "validator commissions");
                for coin in commission {
                    let amount: BigUint = coin
                        .amount
                        .parse()
                        .wrap_err("failed to parse reward coin amount")?;

                    *collected_coins.entry(coin.denom.to_string()).or_default() += amount;
                }

                withdraw_self_valoper = Some(reward.validator_address.clone());
            }
        }

        for coin in reward.reward.iter() {
            let amount: BigUint = coin
                .amount
                .parse()
                .wrap_err("failed to parse reward coin amount")?;

            let Some(threshold) = thresholds_by_denom.get(&coin.denom) else {
                debug!(?coin, "not interested in reward due to configuration");
                continue;
            };

            if amount < *threshold {
                debug!(
                    ?coin,
                    ?amount,
                    ?threshold,
                    "not interested in reward due to threshold"
                );
                continue;
            }

            withdraw_validators.insert(reward.validator_address.clone());
            *collected_coins.entry(coin.denom.to_string()).or_default() += amount;
        }
    }

    if withdraw_validators.is_empty() && withdraw_self_valoper.is_none() {
        info!("nothing to withdraw yet");
        return Ok(());
    }

    info!(
        ?withdraw_validators,
        withdraw_commissions = withdraw_self_valoper.is_some(),
        "withdrawing"
    );

    let mut authz_msgs: Vec<CosmosJsonSerializable> = Vec::new();
    for validator_address in withdraw_validators {
        authz_msgs.push(
            MsgWithdrawDelegatorReward {
                delegator_address: account.delegator_address.to_string(),
                validator_address,
            }
            .into(),
        );
    }

    if let Some(validator_address) = withdraw_self_valoper {
        authz_msgs.push(MsgWithdrawValidatorCommission { validator_address }.into());
    }

    // if !chain_info.chain_supports_setting_withdrawal_address {
    //     let withdraw_address = account
    //         .reward_address
    //         .as_ref()
    //         .unwrap_or(&account.controller_address);

    //     let amount = collected_coins
    //         .into_iter()
    //         .map(|(denom, amount)| Coin {
    //             amount: amount.to_string(),
    //             denom,
    //         })
    //         .collect::<Vec<_>>();

    //     authz_msgs.push(
    //         MsgSend {
    //             from_address: account.delegator_address.to_string(),
    //             to_address: withdraw_address.to_string(),
    //             amount,
    //         }
    //         .into(),
    //     );
    // }
    if !chain_info.chain_supports_setting_withdrawal_address && transaction_args.generate_only {
        // Due to the way how cosmos transactions work, you cannot stack multiple messages on top of each other - MsgSend won't know about updated balance before
        // the transaction has been committed on the chain. If transaction is executed within the tool, then we can easily wait until withdraw succeeds, and then
        // construct a new transaction.
        warn!(
            "as this chain requires using MsgSend for withdrawing rewards, and --generate-only was requested, you need to construct authz transaction yourself"
        );
    }

    let msgs = vec![
        MsgExecCustom {
            grantee: account.controller_address.to_string(),
            msgs: authz_msgs,
        }
        .into(),
    ];

    // This transaction will be signed by the controller account
    let signer = setup_signer(
        &account,
        &chain_info.bech32,
        SigningAccountType::Controller {
            key_type: controller_key_type,
            account_number: transaction_args
                .account_number
                .unwrap_or(controller_account.account_number),
            sequence: transaction_args
                .sequence
                .unwrap_or(controller_account.sequence),
        },
        transaction_args.generate_only,
    )?;

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

    dbg!(tx_result);

    Ok(())
}
