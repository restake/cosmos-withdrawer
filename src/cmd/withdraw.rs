use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use cosmos_sdk_proto::cosmos::{
    bank::v1beta1::MsgSend,
    base::v1beta1::Coin,
    distribution::v1beta1::{
        MsgWithdrawDelegatorReward, MsgWithdrawValidatorCommission,
        QueryDelegationTotalRewardsRequest,
    },
};
use cosmrs::{AccountId, rpc::HttpClient};
use eyre::{Context, ContextCompat, bail};
use num_bigint::BigUint;
use tracing::{debug, info, trace};

use crate::{
    AccountArgs, TransactionArgs,
    chain::{get_account_info, get_chain_info, get_validator_commission},
    cosmos_sdk_extra::{
        abci_query::{QueryDelegationTotalRewards, execute_abci_query},
        gas::GasInfo,
        simulate::simulate_tx_messages,
        str_coin::StrCoin,
        tx::generate_unsigned_tx_json,
    },
    ser::{CosmosJsonSerializable, MsgExecCustom},
};

pub async fn withdraw(
    rpc_url: &str,
    valoper_hrp: Option<&String>,
    account: AccountArgs,
    transaction_args: TransactionArgs,
    thresholds: Vec<StrCoin>,
    generate_only: bool,
) -> eyre::Result<()> {
    let client = HttpClient::new(rpc_url)?;
    let chain_info = get_chain_info(&client, valoper_hrp).await?;
    let gas_info = GasInfo::determine_gas(&chain_info, &transaction_args)?;

    info!(?chain_info, ?gas_info.denom, ?gas_info.price, "chain info");

    account.verify_accounts(&chain_info)?;

    // Ensure delegator & controller accounts are initialized
    // Withdrawal address does not need to be initialized, as it'll only receive rewards
    let delegator_account = get_account_info(&client, &account.delegator_address)
        .await?
        .wrap_err("delegator account is not initialized")?;

    trace!(?delegator_account, "delegator account info");

    let controller_account = get_account_info(&client, &account.controller_address)
        .await?
        .wrap_err("controller account is not initialized")?;

    trace!(?controller_account, "controller account info");

    let delegation_total_rewards = execute_abci_query::<QueryDelegationTotalRewards>(
        &client,
        QueryDelegationTotalRewardsRequest {
            delegator_address: account.delegator_address.to_string(),
        },
    )
    .await?;

    trace!(?delegation_total_rewards, "available rewards");

    let thresholds_by_denom: HashMap<String, u128> = thresholds
        .iter()
        .map(|coin| (coin.denom.to_string(), coin.amount))
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

        // TODO: if any of the thresholds match, insert them into collected_coins
        for coin in reward.reward.iter() {
            let amount: u128 = coin
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
                    amount, threshold, "not interested in reward due to threshold"
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

    // TODO: query grants
    // If there's a grant for MsgSend, then use that.

    let mut msgs: Vec<CosmosJsonSerializable> = Vec::new();
    for validator_address in withdraw_validators {
        msgs.push(
            MsgWithdrawDelegatorReward {
                delegator_address: account.delegator_address.to_string(),
                validator_address,
            }
            .into(),
        );
    }

    if let Some(validator_address) = withdraw_self_valoper {
        msgs.push(MsgWithdrawValidatorCommission { validator_address }.into());
    }

    let use_msg_send = false;
    if use_msg_send {
        let withdraw_address = account
            .reward_address
            .as_ref()
            .unwrap_or(&account.controller_address);

        let amount = collected_coins
            .into_iter()
            .map(|(denom, amount)| Coin {
                amount: amount.to_string(),
                denom,
            })
            .collect::<Vec<_>>();

        msgs.push(
            MsgSend {
                from_address: account.delegator_address.to_string(),
                to_address: withdraw_address.to_string(),
                amount,
            }
            .into(),
        );
    } else {
        msgs = vec![
            MsgExecCustom {
                grantee: account.controller_address.to_string(),
                msgs,
            }
            .into(),
        ];
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
        )
        .await?
    };

    if generate_only {
        println!(
            "{}",
            generate_unsigned_tx_json(msgs, &transaction_args.memo, fee.gas_limit, fee.amount,)
        );

        return Ok(());
    }

    let _ = delegator_account;
    let _ = controller_account;

    bail!("transaction signing & broadcasting is not implemented yet")
}
