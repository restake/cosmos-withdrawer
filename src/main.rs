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

use crate::{
    cmd::{AccountArgs, TransactionArgs},
    cosmos_sdk_extra::str_coin::StrCoin,
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
        gas: TransactionArgs,

        #[command(subcommand)]
        method: SetupValoperMethod,

        #[arg(long)]
        generate_only: bool,
    },
    /// Withdraw validator rewards & commissions
    Withdraw {
        #[clap(flatten)]
        account: AccountArgs,

        #[clap(flatten)]
        gas: TransactionArgs,

        /// Token thresholds for withdrawal. Format: 1234denom
        #[clap(
            long = "threshold",
            env = "COSMOS_WITHDRAWER_WITHDRAW_THRESHOLDS",
            value_delimiter = ','
        )]
        thresholds: Vec<StrCoin>,

        #[arg(long)]
        generate_only: bool,
    },
}

#[derive(Debug, Default, Subcommand)]
enum SetupValoperMethod {
    #[default]
    /// Determine valoper setup method based on available chain functionality
    Auto,

    /// Use authz and set withdraw address
    AuthzWithdraw,

    /// Use authz and grant sending tokens
    AuthzSend,
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
            gas,
            method,
            generate_only,
        }) => {
            crate::cmd::setup_valoper(
                &cli.rpc_url,
                cli.account_hrp.as_ref(),
                cli.valoper_hrp.as_ref(),
                account,
                gas,
                method,
                generate_only,
            )
            .await?
        }
        Some(Subcommands::Withdraw {
            account,
            gas,
            thresholds,
            generate_only,
        }) => {
            crate::cmd::withdraw(
                &cli.rpc_url,
                cli.account_hrp.as_ref(),
                cli.valoper_hrp.as_ref(),
                account,
                gas,
                thresholds,
                generate_only,
            )
            .await?
        }
        None => {
            return Err(eyre!("no subcommand specified"));
        }
    }

    Ok(())
}
