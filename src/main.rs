use clap::{Parser, Subcommand};
use eyre::eyre;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

mod chain;
mod chain_registry;
mod cmd;
mod cosmos_sdk_extra;
mod ser;
mod wallet;

use crate::{
    cmd::{AccountArgs, DebugSubcommand, SetupValoperMethod, TransactionArgs},
    cosmos_sdk_extra::str_coin::StrCoin,
    ser::TimestampStr,
};

#[derive(Debug, Parser)]
struct Cli {
    /// Cosmos RPC URL (Tendermint RPC)
    #[arg(
        long,
        env = "COSMOS_WITHDRAWER_RPC_URL",
        global = true,
        default_value = "http://127.0.0.1:26657"
    )]
    rpc_url: String,

    /// Network account address prefix. Some chains do not support querying bech32 prefix, therefore you need to supply this
    #[arg(long, env = "COSMOS_WITHDRAWER_ACCOUNT_HRP", global = true)]
    account_hrp: Option<String>,

    /// Network valoper address prefix. E.g. on Iris Hub `iaa1` is for accounts, but on valopers they have `iva1`. Defaults to `{account-bech32}valoper`.
    #[arg(long, env = "COSMOS_WITHDRAWER_VALOPER_HRP", global = true)]
    valoper_hrp: Option<String>,

    #[command(subcommand)]
    command: Option<Subcommands>,
}

#[derive(Debug, Subcommand)]
enum Subcommands {
    /// Setup validator operator or delegator account
    SetupValoper {
        #[clap(flatten)]
        account: AccountArgs,

        #[clap(flatten)]
        transaction_args: TransactionArgs,

        #[command(subcommand)]
        method: SetupValoperMethod,

        /// Authz grant expiration. Either RFC3339 timestamp, or duration string (relative from now). By default grants never expire, however some older Cosmos SDK based chains require expiration to be set.
        #[arg(long)]
        expiration: Option<TimestampStr>,
    },
    /// Withdraw validator rewards & commissions
    Withdraw {
        #[clap(flatten)]
        account: AccountArgs,

        #[clap(flatten)]
        transaction_args: TransactionArgs,

        /// Token thresholds for withdrawal. Format: 1234denom
        #[clap(
            long = "threshold",
            env = "COSMOS_WITHDRAWER_WITHDRAW_THRESHOLDS",
            value_delimiter = ','
        )]
        thresholds: Vec<StrCoin>,
    },
    /// Debug subcommands
    Debug {
        /// Debug subcommand
        #[command(subcommand)]
        debug: DebugSubcommand,
    },
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_writer(|| Box::new(std::io::stderr()))
                .with_target(true)
                .with_span_events(FmtSpan::CLOSE),
        )
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    entrypoint().await
}

async fn entrypoint() -> eyre::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Subcommands::SetupValoper {
            account,
            transaction_args,
            method,
            expiration,
        }) => {
            crate::cmd::setup_valoper(
                &cli.rpc_url,
                cli.account_hrp.as_ref(),
                cli.valoper_hrp.as_ref(),
                account,
                transaction_args,
                method,
                expiration.as_ref(),
            )
            .await?
        }
        Some(Subcommands::Withdraw {
            account,
            transaction_args,
            thresholds,
        }) => {
            crate::cmd::withdraw(
                &cli.rpc_url,
                cli.account_hrp.as_ref(),
                cli.valoper_hrp.as_ref(),
                account,
                transaction_args,
                thresholds,
            )
            .await?
        }
        Some(Subcommands::Debug { debug }) => {
            crate::cmd::debug(
                &cli.rpc_url,
                cli.account_hrp.as_ref(),
                cli.valoper_hrp.as_ref(),
                debug,
            )
            .await?
        }
        None => {
            return Err(eyre!("no subcommand specified"));
        }
    }

    Ok(())
}
